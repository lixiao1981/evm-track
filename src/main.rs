use anyhow::Result;
use clap::Parser;
use cli::{Commands, HistoricalWhichCmd, TrackWhichCmd};
use std::sync::Arc;
// use tracing::warn;
use tracing_subscriber::EnvFilter;

mod abi;
mod actions;
mod cli;
mod config;
mod data_cmd;
mod provider;
mod runtime;
mod throttle;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let filter_layer = if cli.verbose {
        EnvFilter::new("info")
    } else {
        EnvFilter::new("warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter_layer)
        .init();

    match cli.command {
        Commands::Track(track) => match track.which {
            TrackWhichCmd::Realtime(rt) => {
                let cfg = config::load_config(&track.common.config)?;
                // 初始化节流（0 表示关闭）
                crate::throttle::init(cfg.max_requests_per_second);
                if let Some(p) = &cli.event_sigs {
                    abi::set_event_sigs_path(p.display().to_string());
                }
                if let Some(p) = &cli.func_sigs {
                    abi::set_func_sigs_path(p.display().to_string());
                }
                if let Some(p) = &cfg.event_sigs_path {
                    abi::set_event_sigs_path(p.clone());
                }
                if let Some(p) = &cfg.func_sigs_path {
                    abi::set_func_sigs_path(p.clone());
                }
                let provider = provider::connect_ws(&cfg.rpcurl).await?;
                let addrs = config::collect_enabled_addresses(&cfg)?;
                // Build ActionSet: terminal logging + optional JSON + sample actions
                let mut set = actions::ActionSet::new();
                // Logging options from config if present
                let log_cfg = cfg.actions.get("Logging");
                let (log_events, log_txs, log_blocks, enable_term, enable_disc, disc_url) =
                    if let Some(ac) = log_cfg {
                        let o = &ac.options;
                        (
                            o.get("log-events")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("log-transactions")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("log-blocks")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("enable-terminal-logs")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("enable-discord-logs")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                                || cli.webhook_url.is_some(),
                            o.get("discord-webhook-url")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| cli.webhook_url.clone()),
                        )
                    } else {
                        (
                            true,
                            true,
                            true,
                            true,
                            cli.webhook_url.is_some(),
                            cli.webhook_url.clone(),
                        )
                    };
                let log_opts = actions::logging::LoggingOptions {
                    enable_terminal_logs: enable_term,
                    enable_discord_logs: enable_disc,
                    discord_webhook_url: disc_url.clone(),
                    log_events,
                    log_transactions: log_txs,
                    log_blocks,
                };
                set.add(actions::logging::LoggingAction::new(log_opts));
                if cli.json {
                    set.add(actions::jsonlog::JsonLogAction);
                }
                let prov_arc = Arc::new(provider.clone());
                set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                set.add(actions::ownership::OwnershipAction);
                set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                set.add(actions::deployment::DeploymentScanAction::new(
                    prov_arc.clone(),
                    actions::deployment::DeploymentOptions::default(),
                ));
                // LargeTransfer options from config
                let lt_cfg = cfg.actions.get("LargeTransfer");
                if let Some(ac) = lt_cfg {
                    let min_h = ac
                        .options
                        .get("min-amount")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                    let dec_default = ac
                        .options
                        .get("decimals-default")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u8)
                        .unwrap_or(18);
                    set.add(actions::large_transfer::LargeTransferAction::new(
                        actions::large_transfer::LargeTransferOptions {
                            min_amount_human: min_h,
                            decimals_default: dec_default,
                        },
                    ));
                }
                // Tornado options from config
                let torn_opts = cfg
                    .actions
                    .get("TornadoCash")
                    .and_then(|ac| ac.options.get("output-filepath"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                set.add(actions::tornado::TornadoAction::new(
                    actions::tornado::TornadoOptions {
                        output_filepath: torn_opts.clone(),
                    },
                ));
                let set = Arc::new(set);
                if rt.blocks {
                    if rt.pending_blocks {
                        // Pending transactions path
                        return runtime::realtime::run_pending_transactions(
                            provider,
                            addrs,
                            Some(set),
                        )
                        .await;
                    }
                    // Create a new ActionSet for blocks path too
                    let mut set2 = actions::ActionSet::new();
                    let log_opts2 = actions::logging::LoggingOptions {
                        enable_terminal_logs: enable_term,
                        enable_discord_logs: enable_disc,
                        discord_webhook_url: disc_url.clone(),
                        log_events,
                        log_transactions: log_txs,
                        log_blocks,
                    };
                    set2.add(actions::logging::LoggingAction::new(log_opts2));
                    if cli.json {
                        set2.add(actions::jsonlog::JsonLogAction);
                    }
                    set2.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                    set2.add(actions::ownership::OwnershipAction);
                    set2.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                    set2.add(actions::deployment::DeploymentScanAction::new(
                        prov_arc.clone(),
                        actions::deployment::DeploymentOptions::default(),
                    ));
                    if let Some(ac) = lt_cfg {
                        let min_h = ac
                            .options
                            .get("min-amount")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                        let dec_default = ac
                            .options
                            .get("decimals-default")
                            .and_then(|v| v.as_u64())
                            .map(|v| v as u8)
                            .unwrap_or(18);
                        set2.add(actions::large_transfer::LargeTransferAction::new(
                            actions::large_transfer::LargeTransferOptions {
                                min_amount_human: min_h,
                                decimals_default: dec_default,
                            },
                        ));
                    }
                    set2.add(actions::tornado::TornadoAction::new(
                        actions::tornado::TornadoOptions {
                            output_filepath: torn_opts,
                        },
                    ));
                    runtime::realtime::run_blocks(provider, addrs, Some(Arc::new(set2))).await
                } else {
                    // default to events if not explicitly --blocks
                    runtime::realtime::run_events(provider, addrs, Some(set)).await
                }
            }
            TrackWhichCmd::Historical(hist) => {
                let cfg = config::load_config(&track.common.config)?;
                // 初始化节流（0 表示关闭）
                crate::throttle::init(cfg.max_requests_per_second);
                if let Some(p) = &cli.event_sigs {
                    abi::set_event_sigs_path(p.display().to_string());
                }
                if let Some(p) = &cli.func_sigs {
                    abi::set_func_sigs_path(p.display().to_string());
                }
                if let Some(p) = &cfg.event_sigs_path {
                    abi::set_event_sigs_path(p.clone());
                }
                if let Some(p) = &cfg.func_sigs_path {
                    abi::set_func_sigs_path(p.clone());
                }
                let provider = provider::connect_ws(&cfg.rpcurl).await?;
                let addrs = config::collect_enabled_addresses(&cfg)?;
                let mut set = actions::ActionSet::new();
                let log_cfg = cfg.actions.get("Logging");
                let (log_events, log_txs, log_blocks, enable_term, enable_disc, disc_url) =
                    if let Some(ac) = log_cfg {
                        let o = &ac.options;
                        (
                            o.get("log-events")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("log-transactions")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("log-blocks")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("enable-terminal-logs")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true),
                            o.get("enable-discord-logs")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false)
                                || cli.webhook_url.is_some(),
                            o.get("discord-webhook-url")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| cli.webhook_url.clone()),
                        )
                    } else {
                        (
                            true,
                            true,
                            true,
                            true,
                            cli.webhook_url.is_some(),
                            cli.webhook_url.clone(),
                        )
                    };
                let log_opts = actions::logging::LoggingOptions {
                    enable_terminal_logs: enable_term,
                    enable_discord_logs: enable_disc,
                    discord_webhook_url: disc_url.clone(),
                    log_events,
                    log_transactions: log_txs,
                    log_blocks,
                };
                set.add(actions::logging::LoggingAction::new(log_opts));
                if cli.json {
                    set.add(actions::jsonlog::JsonLogAction);
                }
                // new provider clone for historical
                let prov_arc = Arc::new(provider.clone());
                set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                set.add(actions::ownership::OwnershipAction);
                set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                set.add(actions::deployment::DeploymentScanAction::new(
                    prov_arc.clone(),
                    actions::deployment::DeploymentOptions::default(),
                ));
                let lt_cfg = cfg.actions.get("LargeTransfer");
                if let Some(ac) = lt_cfg {
                    let min_h = ac
                        .options
                        .get("min-amount")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                    let dec_default = ac
                        .options
                        .get("decimals-default")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u8)
                        .unwrap_or(18);
                    set.add(actions::large_transfer::LargeTransferAction::new(
                        actions::large_transfer::LargeTransferOptions {
                            min_amount_human: min_h,
                            decimals_default: dec_default,
                        },
                    ));
                }
                let torn_opts = cfg
                    .actions
                    .get("TornadoCash")
                    .and_then(|ac| ac.options.get("output-filepath"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                set.add(actions::tornado::TornadoAction::new(
                    actions::tornado::TornadoOptions {
                        output_filepath: torn_opts.clone(),
                    },
                ));
                let set = Arc::new(set);
                match hist.which {
                    HistoricalWhichCmd::Events(range) => {
                        runtime::historical::run_events(provider, addrs, &range, Some(set)).await
                    }
                    HistoricalWhichCmd::Blocks(range) => {
                        let mut set2 = actions::ActionSet::new();
                        let log_opts2 = actions::logging::LoggingOptions {
                            enable_terminal_logs: enable_term,
                            enable_discord_logs: enable_disc,
                            discord_webhook_url: disc_url.clone(),
                            log_events,
                            log_transactions: log_txs,
                            log_blocks,
                        };
                        set2.add(actions::logging::LoggingAction::new(log_opts2));
                        if cli.json {
                            set2.add(actions::jsonlog::JsonLogAction);
                        }
                        set2.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                        set2.add(actions::ownership::OwnershipAction);
                        set2.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                        set2.add(actions::deployment::DeploymentScanAction::new(
                            prov_arc.clone(),
                            actions::deployment::DeploymentOptions::default(),
                        ));
                        if let Some(ac) = lt_cfg {
                            let min_h = ac
                                .options
                                .get("min-amount")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                            let dec_default = ac
                                .options
                                .get("decimals-default")
                                .and_then(|v| v.as_u64())
                                .map(|v| v as u8)
                                .unwrap_or(18);
                            set2.add(actions::large_transfer::LargeTransferAction::new(
                                actions::large_transfer::LargeTransferOptions {
                                    min_amount_human: min_h,
                                    decimals_default: dec_default,
                                },
                            ));
                        }
                        set2.add(actions::tornado::TornadoAction::new(
                            actions::tornado::TornadoOptions {
                                output_filepath: torn_opts,
                            },
                        ));
                        runtime::historical::run_blocks(
                            provider,
                            addrs,
                            &range,
                            Some(Arc::new(set2)),
                        )
                        .await
                    }
                }
            }
        },
        Commands::Data(cmd) => match cmd.which {
            cli::DataWhichCmd::Event(args) => {
                crate::data_cmd::add_events_from_abi(&args.abi, &args.output)?;
                Ok(())
            }
            cli::DataWhichCmd::FetchAbi(args) => {
                let s = crate::data_cmd::fetch_abi_from_scanner(
                    &args.address,
                    &args.scanner_url,
                    args.api_key.as_deref(),
                )
                .await?;
                std::fs::write(&args.output, s)?;
                println!("wrote ABI to {}", args.output.display());
                Ok(())
            }
        },
    }
}

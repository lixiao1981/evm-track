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
                // 子命令处的 --config 优先，其次使用 track 级别 --config
                let cfg_path = rt
                    .config
                    .as_ref()
                    .or(track.common.config.as_ref())
                    .ok_or_else(|| anyhow::anyhow!("--config is required (provide at track or subcommand)"))?;
                let cfg = config::load_config(cfg_path)?;
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
                // Deployment: allow optional file output via actions.Deployment.options.output-filepath
                let dep_out_1 = cfg
                    .actions
                    .get("Deployment")
                    .and_then(|ac| ac.options.get("output-filepath"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let dep_opts_1 = actions::deployment::DeploymentOptions { output_filepath: dep_out_1 };
                set.add(actions::deployment::DeploymentScanAction::new(prov_arc.clone(), dep_opts_1));
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
                            rt.pending_hashes_only,
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
                    // Deployment: optional file output
                    let dep_out_2 = cfg
                        .actions
                        .get("Deployment")
                        .and_then(|ac| ac.options.get("output-filepath"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let dep_opts_2 = actions::deployment::DeploymentOptions { output_filepath: dep_out_2 };
                    set2.add(actions::deployment::DeploymentScanAction::new(prov_arc.clone(), dep_opts_2));
                    // Initscan action (optional)
                    if let Some(ac) = cfg.actions.get("Initscan") {
                        if ac.enabled {
                            let o = &ac.options;
                            let from = o
                                .get("from-address")
                                .and_then(|v| v.as_str())
                                .and_then(|s| s.parse().ok());
                            let mut check_addrs: Vec<alloy_primitives::Address> = vec![];
                            if let Some(arr) = o.get("check-addresses").and_then(|v| v.as_array()) {
                                for a in arr {
                                    if let Some(s) = a.as_str() {
                                        if let Ok(addr) = s.parse() {
                                            check_addrs.push(addr);
                                        }
                                    }
                                }
                            }
                            let mut func_sigs: Vec<(String, Vec<u8>)> = vec![];
                            if let Some(map) = o
                                .get("function-signature-calldata")
                                .and_then(|v| v.as_object())
                            {
                                for (k, v) in map {
                                    if let Some(s) = v.as_str() {
                                        let h = s.trim_start_matches("0x");
                                        if let Ok(b) = hex::decode(h) {
                                            func_sigs.push((k.clone(), b));
                                        }
                                    }
                                }
                            }
                            let init_after =
                                o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
                            let usd_threshold =
                                o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let webhook_url = o
                                .get("webhook-url")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .or_else(|| cli.webhook_url.clone());
                            let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
                            let known_path = o.get("initializable-contracts-filepath").and_then(|v| v.as_str()).map(|s| s.to_string());
                            let max_inflight_inits = o.get("init-concurrency").and_then(|v| v.as_u64()).map(|v| v as usize);
                            let is_opts = actions::initscan::InitscanOptions { from, check_addresses: check_addrs, init_after_delay_secs: init_after, usd_threshold, func_sigs, webhook_url, initializable_contracts_filepath: known_path, init_known_contracts_frequency_secs: init_known_freq, max_inflight_inits };
                            set2.add(actions::initscan::InitscanAction::new(
                                prov_arc.clone(),
                                is_opts,
                            ));
                        }
                    }
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
                // 先取基础配置（historical/track 提供），在 events/blocks 层允许再次覆盖
                let base_cfg_path = hist
                    .config
                    .as_ref()
                    .or(track.common.config.as_ref())
                    .ok_or_else(|| anyhow::anyhow!("--config is required (provide at track/historical or inside events/blocks)"))?;
                if let Some(p) = &cli.event_sigs {
                    abi::set_event_sigs_path(p.display().to_string());
                }
                if let Some(p) = &cli.func_sigs {
                    abi::set_func_sigs_path(p.display().to_string());
                }
                let mut cfg = config::load_config(base_cfg_path)?;
                if let Some(p) = &cfg.event_sigs_path {
                    abi::set_event_sigs_path(p.clone());
                }
                if let Some(p) = &cfg.func_sigs_path {
                    abi::set_func_sigs_path(p.clone());
                }
                match hist.which {
                    HistoricalWhichCmd::Events(range) => {
                        if let Some(ref p) = range.config {
                            cfg = config::load_config(p)?;
                            if let Some(ep) = &cfg.event_sigs_path { abi::set_event_sigs_path(ep.clone()); }
                            if let Some(fp) = &cfg.func_sigs_path { abi::set_func_sigs_path(fp.clone()); }
                        }
                        crate::throttle::init(cfg.max_requests_per_second);
                        let provider = provider::connect_ws(&cfg.rpcurl).await?;
                        let addrs = config::collect_enabled_addresses(&cfg)?;
                        let mut set = actions::ActionSet::new();
                        let log_cfg = cfg.actions.get("Logging");
                        let (log_events, log_txs, log_blocks, enable_term, enable_disc, disc_url) = if let Some(ac) = log_cfg {
                            let o = &ac.options; (
                                o.get("log-events").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("log-transactions").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("log-blocks").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("enable-terminal-logs").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("enable-discord-logs").and_then(|v| v.as_bool()).unwrap_or(false) || cli.webhook_url.is_some(),
                                o.get("discord-webhook-url").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| cli.webhook_url.clone()),
                            ) } else { (true, true, true, true, cli.webhook_url.is_some(), cli.webhook_url.clone()) };
                        let log_opts = actions::logging::LoggingOptions { enable_terminal_logs: enable_term, enable_discord_logs: enable_disc, discord_webhook_url: disc_url.clone(), log_events, log_transactions: log_txs, log_blocks };
                        set.add(actions::logging::LoggingAction::new(log_opts));
                        if cli.json { set.add(actions::jsonlog::JsonLogAction); }
                        let prov_arc = Arc::new(provider.clone());
                        set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                        set.add(actions::ownership::OwnershipAction);
                        set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                        // Deployment: optional file output
                        let dep_out_3 = cfg
                            .actions
                            .get("Deployment")
                            .and_then(|ac| ac.options.get("output-filepath"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let dep_opts_3 = actions::deployment::DeploymentOptions { output_filepath: dep_out_3 };
                        set.add(actions::deployment::DeploymentScanAction::new(prov_arc.clone(), dep_opts_3));
                        if let Some(ac) = cfg.actions.get("LargeTransfer") {
                            let min_h = ac.options.get("min-amount").and_then(|v| v.as_str()).map(|s| s.to_string())
                                .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                            let dec_default = ac.options.get("decimals-default").and_then(|v| v.as_u64()).map(|v| v as u8).unwrap_or(18);
                            set.add(actions::large_transfer::LargeTransferAction::new(actions::large_transfer::LargeTransferOptions { min_amount_human: min_h, decimals_default: dec_default }));
                        }
                        if let Some(path) = cfg.actions.get("TornadoCash").and_then(|ac| ac.options.get("output-filepath")).and_then(|v| v.as_str()).map(|s| s.to_string()) {
                            set.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: Some(path) }));
                        } else {
                            set.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: None }));
                        }
                        runtime::historical::run_events(provider, addrs, &range, Some(Arc::new(set))).await
                    }
                    HistoricalWhichCmd::Blocks(range) => {
                        if let Some(ref p) = range.config {
                            cfg = config::load_config(p)?;
                            if let Some(ep) = &cfg.event_sigs_path { abi::set_event_sigs_path(ep.clone()); }
                            if let Some(fp) = &cfg.func_sigs_path { abi::set_func_sigs_path(fp.clone()); }
                        }
                        crate::throttle::init(cfg.max_requests_per_second);
                        let provider = provider::connect_ws(&cfg.rpcurl).await?;
                        let addrs = config::collect_enabled_addresses(&cfg)?;
                        let mut set2 = actions::ActionSet::new();
                        let log_cfg = cfg.actions.get("Logging");
                        let (log_events, log_txs, log_blocks, enable_term, enable_disc, disc_url) = if let Some(ac) = log_cfg {
                            let o = &ac.options; (
                                o.get("log-events").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("log-transactions").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("log-blocks").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("enable-terminal-logs").and_then(|v| v.as_bool()).unwrap_or(true),
                                o.get("enable-discord-logs").and_then(|v| v.as_bool()).unwrap_or(false) || cli.webhook_url.is_some(),
                                o.get("discord-webhook-url").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| cli.webhook_url.clone()),
                            ) } else { (true, true, true, true, cli.webhook_url.is_some(), cli.webhook_url.clone()) };
                        let log_opts2 = actions::logging::LoggingOptions { enable_terminal_logs: enable_term, enable_discord_logs: enable_disc, discord_webhook_url: disc_url.clone(), log_events, log_transactions: log_txs, log_blocks };
                        set2.add(actions::logging::LoggingAction::new(log_opts2));
                        if cli.json { set2.add(actions::jsonlog::JsonLogAction); }
                        let prov_arc = Arc::new(provider.clone());
                        set2.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                        set2.add(actions::ownership::OwnershipAction);
                        set2.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                        // Deployment: optional file output
                        let dep_out_4 = cfg
                            .actions
                            .get("Deployment")
                            .and_then(|ac| ac.options.get("output-filepath"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let dep_opts_4 = actions::deployment::DeploymentOptions { output_filepath: dep_out_4 };
                        set2.add(actions::deployment::DeploymentScanAction::new(prov_arc.clone(), dep_opts_4));
                        // Initscan action (optional)
                        if let Some(ac) = cfg.actions.get("Initscan") {
                            if ac.enabled {
                                let o = &ac.options;
                                let from = o.get("from-address").and_then(|v| v.as_str()).and_then(|s| s.parse().ok());
                                let mut check_addrs: Vec<alloy_primitives::Address> = vec![];
                                if let Some(arr) = o.get("check-addresses").and_then(|v| v.as_array()) {
                                    for a in arr {
                                        if let Some(s) = a.as_str() { if let Ok(addr) = s.parse() { check_addrs.push(addr); } }
                                    }
                                }
                                let mut func_sigs: Vec<(String, Vec<u8>)> = vec![];
                                if let Some(map) = o.get("function-signature-calldata").and_then(|v| v.as_object()) {
                                    for (k, v) in map {
                                        if let Some(s) = v.as_str() {
                                            let h = s.trim_start_matches("0x");
                                            if let Ok(b) = hex::decode(h) { func_sigs.push((k.clone(), b)); }
                                        }
                                    }
                                }
                                let init_after = o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
                                let usd_threshold = o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                let webhook_url = o.get("webhook-url").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| cli.webhook_url.clone());
                                let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
                                let known_path = o.get("initializable-contracts-filepath").and_then(|v| v.as_str()).map(|s| s.to_string());
                                let max_inflight_inits = o.get("init-concurrency").and_then(|v| v.as_u64()).map(|v| v as usize);
                                let is_opts = actions::initscan::InitscanOptions { from, check_addresses: check_addrs, init_after_delay_secs: init_after, usd_threshold, func_sigs, webhook_url, initializable_contracts_filepath: known_path, init_known_contracts_frequency_secs: init_known_freq, max_inflight_inits };
                                set2.add(actions::initscan::InitscanAction::new(prov_arc.clone(), is_opts));
                            }
                        }
                        if let Some(ac) = cfg.actions.get("LargeTransfer") {
                            let min_h = ac.options.get("min-amount").and_then(|v| v.as_str()).map(|s| s.to_string())
                                .or_else(|| ac.options.get("min_amount").and_then(|v| v.as_str()).map(|s| s.to_string()));
                            let dec_default = ac.options.get("decimals-default").and_then(|v| v.as_u64()).map(|v| v as u8).unwrap_or(18);
                            set2.add(actions::large_transfer::LargeTransferAction::new(actions::large_transfer::LargeTransferOptions { min_amount_human: min_h, decimals_default: dec_default }));
                        }
                        if let Some(path) = cfg.actions.get("TornadoCash").and_then(|ac| ac.options.get("output-filepath")).and_then(|v| v.as_str()).map(|s| s.to_string()) {
                            set2.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: Some(path) }));
                        } else {
                            set2.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: None }));
                        }
                        runtime::historical::run_blocks(provider, addrs, &range, Some(Arc::new(set2))).await
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
        Commands::InitScan(cmd) => {
            // 配置加载（仅此子命令层）
            let cfg_path = cmd
                .config
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("--config is required for initscan"))?;
            let mut cfg = config::load_config(cfg_path)?;
            // Optional override sig paths from CLI
            if let Some(p) = &cli.event_sigs { abi::set_event_sigs_path(p.display().to_string()); }
            if let Some(p) = &cli.func_sigs { abi::set_func_sigs_path(p.display().to_string()); }
            if let Some(p) = &cfg.event_sigs_path { abi::set_event_sigs_path(p.clone()); }
            if let Some(p) = &cfg.func_sigs_path { abi::set_func_sigs_path(p.clone()); }
            crate::throttle::init(cfg.max_requests_per_second);
            let provider = provider::connect_ws(&cfg.rpcurl).await?;

            // 构建 Initscan 选项（来自配置）
            let ac = cfg
                .actions
                .get("Initscan")
                .ok_or_else(|| anyhow::anyhow!("Config must include actions.Initscan"))?;
            anyhow::ensure!(ac.enabled, "actions.Initscan must be enabled");
            let o = &ac.options;
            let from = o
                .get("from-address")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok());
            let mut check_addrs: Vec<alloy_primitives::Address> = vec![];
            if let Some(arr) = o.get("check-addresses").and_then(|v| v.as_array()) {
                for a in arr {
                    if let Some(s) = a.as_str() { if let Ok(addr) = s.parse() { check_addrs.push(addr); } }
                }
            }
            let mut func_sigs: Vec<(String, Vec<u8>)> = vec![];
            if let Some(map) = o.get("function-signature-calldata").and_then(|v| v.as_object()) {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        let h = s.trim_start_matches("0x");
                        if let Ok(b) = hex::decode(h) { func_sigs.push((k.clone(), b)); }
                    }
                }
            }
            let init_after = o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
            let usd_threshold = o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let webhook_url = o.get("webhook-url").and_then(|v| v.as_str()).map(|s| s.to_string()).or_else(|| cli.webhook_url.clone());
            let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
            let known_path = o.get("initializable-contracts-filepath").and_then(|v| v.as_str()).map(|s| s.to_string());

            let max_inflight_inits = o.get("init-concurrency").and_then(|v| v.as_u64()).map(|v| v as usize);
            let is_opts = actions::initscan::InitscanOptions {
                from,
                check_addresses: check_addrs,
                init_after_delay_secs: init_after,
                usd_threshold,
                func_sigs,
                webhook_url,
                initializable_contracts_filepath: known_path,
                init_known_contracts_frequency_secs: init_known_freq,
                max_inflight_inits: max_inflight_inits,
            };

            let opts = actions::history_init_scan::HistoryInitScanOptions {
                from_block: cmd.from_block,
                to_block: cmd.to_block,
                initscan: is_opts,
                progress_every: cmd.progress_every,
                progress_percent: cmd.progress_percent,
            };
            let provider = std::sync::Arc::new(provider);
            actions::history_init_scan::run(provider, opts).await
        }
    }
}

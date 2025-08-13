use std::sync::Arc;
use anyhow::Result;
use cli::{Commands, HistoricalCmd, RealtimeCmd, TrackCmd};
use tracing::warn;
use tracing_subscriber::EnvFilter;

mod abi;
mod actions;
mod cli;
mod config;
mod provider;
mod runtime;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let filter_layer = if cli.verbose { EnvFilter::new("info") } else { EnvFilter::new("warn") };
    tracing_subscriber::fmt().with_env_filter(filter_layer).init();

    match cli.command {
        Commands::Track(track) => match track {
            TrackCmd::Realtime { common, which } => {
                let cfg = config::load_config(&common.config)?;
                let provider = provider::connect_ws(&cfg.rpcurl).await?;
                let addrs = config::collect_enabled_addresses(&cfg)?;
                // Build ActionSet: terminal logging + optional JSON + sample actions
                let mut set = actions::ActionSet::new();
                let log_opts = actions::logging::LoggingOptions { enable_terminal_logs: true, enable_discord_logs: false, discord_webhook_url: None };
                set.add(actions::logging::LoggingAction::new(log_opts));
                if cli.json { set.add(actions::jsonlog::JsonLogAction); }
                let prov_arc = Arc::new(provider.clone());
                set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                set.add(actions::ownership::OwnershipAction);
                set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                set.add(actions::tornado::TornadoAction);
                let set = Arc::new(set);
                match which {
                    RealtimeCmd::Events => runtime::realtime::run_events(provider, addrs, Some(set)).await,
                    RealtimeCmd::Blocks { pending_blocks } => {
                        if pending_blocks { warn!("pending-blocks not yet implemented; listening to new heads"); }
                        // Create a new ActionSet for blocks path too
                        let mut set2 = actions::ActionSet::new();
                        let log_opts2 = actions::logging::LoggingOptions { enable_terminal_logs: true, enable_discord_logs: false, discord_webhook_url: None };
                        set2.add(actions::logging::LoggingAction::new(log_opts2));
                        if cli.json { set2.add(actions::jsonlog::JsonLogAction); }
                        set2.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                        set2.add(actions::ownership::OwnershipAction);
                        set2.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                        set2.add(actions::tornado::TornadoAction);
                        runtime::realtime::run_blocks(provider, addrs, Some(Arc::new(set2))).await
                    }
                }
            }
            TrackCmd::Historical { common, which } => {
                let cfg = config::load_config(&common.config)?;
                let provider = provider::connect_ws(&cfg.rpcurl).await?;
                let addrs = config::collect_enabled_addresses(&cfg)?;
                let mut set = actions::ActionSet::new();
                let log_opts = actions::logging::LoggingOptions { enable_terminal_logs: true, enable_discord_logs: false, discord_webhook_url: None };
                set.add(actions::logging::LoggingAction::new(log_opts));
                if cli.json { set.add(actions::jsonlog::JsonLogAction); }
                // new provider clone for historical
                let prov_arc = Arc::new(provider.clone());
                set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                set.add(actions::ownership::OwnershipAction);
                set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                set.add(actions::tornado::TornadoAction);
                let set = Arc::new(set);
                match which {
                    HistoricalCmd::Events(range) => runtime::historical::run_events(provider, addrs, &range, Some(set)).await,
                    HistoricalCmd::Blocks(range) => {
                        let mut set2 = actions::ActionSet::new();
                        let log_opts2 = actions::logging::LoggingOptions { enable_terminal_logs: true, enable_discord_logs: false, discord_webhook_url: None };
                        set2.add(actions::logging::LoggingAction::new(log_opts2));
                        if cli.json { set2.add(actions::jsonlog::JsonLogAction); }
                        set2.add(actions::transfer::TransferAction::new(prov_arc.clone()));
                        set2.add(actions::ownership::OwnershipAction);
                        set2.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
                        set2.add(actions::tornado::TornadoAction);
                        runtime::historical::run_blocks(provider, addrs, &range, Some(Arc::new(set2))).await
                    }
                }
            }
        },
    }
}

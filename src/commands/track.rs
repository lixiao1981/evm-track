use std::sync::Arc;

use anyhow::Result;
// unused imports trimmed

use crate::{abi, app, cli::{HistoricalWhichCmd, RealtimeCmd, TrackWhichCmd}, config, provider, runtime};

pub async fn run(cli: &crate::cli::Cli, which: &TrackWhichCmd, common: &crate::cli::CommonFlags) -> Result<()> {
    match which {
        TrackWhichCmd::Realtime(rt) => run_realtime(cli, rt, common).await,
        TrackWhichCmd::Historical(hist) => run_historical(cli, hist, common).await,
    }
}

async fn run_realtime(cli: &crate::cli::Cli, rt: &RealtimeCmd, common: &crate::cli::CommonFlags) -> Result<()> {
    let cfg_path = rt
        .config
        .as_ref()
        .or(common.config.as_ref())
        .ok_or_else(|| anyhow::anyhow!("--config is required (provide at track or subcommand)"))?;
    let cfg = config::load_config(cfg_path)?;
    crate::throttle::init(cfg.max_requests_per_second);
    if let Some(p) = &cli.event_sigs { abi::set_event_sigs_path(p.display().to_string()); }
    if let Some(p) = &cli.func_sigs { abi::set_func_sigs_path(p.display().to_string()); }
    if let Some(p) = &cfg.event_sigs_path { abi::set_event_sigs_path(p.clone()); }
    if let Some(p) = &cfg.func_sigs_path { abi::set_func_sigs_path(p.clone()); }
    let provider = provider::connect_auto(&cfg.rpcurl).await?;
    let addrs = config::collect_enabled_addresses(&cfg)?;
    let set = Arc::new(app::build_actionset(&provider, &cfg, &cli));
    if rt.blocks {
        if rt.pending_blocks {
            return runtime::realtime::run_pending_transactions(provider, addrs, Some(set), rt.pending_hashes_only).await;
        }
        // blocks path: rebuild set for blocks (same build function for now)
        let set2 = app::build_actionset(&provider, &cfg, &cli);
        runtime::realtime::run_blocks(provider, addrs, Some(Arc::new(set2))).await
    } else {
        runtime::realtime::run_events(provider, addrs, Some(set)).await
    }
}

async fn run_historical(
    cli: &crate::cli::Cli,
    hist: &crate::cli::HistoricalCmd,
    common: &crate::cli::CommonFlags,
) -> Result<()> {
    // 先取基础配置，在 events/blocks 层允许覆盖
    let base_cfg_path = hist
        .config
        .as_ref()
        .or(common.config.as_ref())
        .ok_or_else(|| anyhow::anyhow!("--config is required (provide at track/historical or inside events/blocks)"))?;
    if let Some(p) = &cli.event_sigs { abi::set_event_sigs_path(p.display().to_string()); }
    if let Some(p) = &cli.func_sigs { abi::set_func_sigs_path(p.display().to_string()); }
    let cfg = config::load_config(base_cfg_path)?;
    if let Some(p) = &cfg.event_sigs_path { abi::set_event_sigs_path(p.clone()); }
    if let Some(p) = &cfg.func_sigs_path { abi::set_func_sigs_path(p.clone()); }
    match hist.which {
        HistoricalWhichCmd::Events(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                cfg2 = config::load_config(p)?;
                if let Some(ep) = &cfg2.event_sigs_path { abi::set_event_sigs_path(ep.clone()); }
                if let Some(fp) = &cfg2.func_sigs_path { abi::set_func_sigs_path(fp.clone()); }
            }
            crate::throttle::init(cfg2.max_requests_per_second);
            let provider = provider::connect_auto(&cfg2.rpcurl).await?;
            let addrs = config::collect_enabled_addresses(&cfg2)?;
            let set = app::build_actionset(&provider, &cfg2, &cli);
            runtime::historical::run_events(provider, addrs, range, Some(Arc::new(set))).await
        }
        HistoricalWhichCmd::Blocks(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                cfg2 = config::load_config(p)?;
                if let Some(ep) = &cfg2.event_sigs_path { abi::set_event_sigs_path(ep.clone()); }
                if let Some(fp) = &cfg2.func_sigs_path { abi::set_func_sigs_path(fp.clone()); }
            }
            crate::throttle::init(cfg2.max_requests_per_second);
            let provider = provider::connect_auto(&cfg2.rpcurl).await?;
            let addrs = config::collect_enabled_addresses(&cfg2)?;
            let set2 = app::build_actionset(&provider, &cfg2, &cli);
            runtime::historical::run_blocks(provider, addrs, range, Some(Arc::new(set2))).await
        }
    }
}

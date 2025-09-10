use std::sync::Arc;

use crate::{
    abi,
    app,
    cli::{CommonFlags, HistoricalWhichCmd, RealtimeCmd, TrackWhichCmd},
    config,
    error::{AppError, Result},
    provider,
    runtime,
};

pub async fn run(cli: &crate::cli::Cli, which: &TrackWhichCmd, common: &CommonFlags) -> Result<()> {
    match which {
        TrackWhichCmd::Realtime(rt) => run_realtime(cli, rt, common).await,
        TrackWhichCmd::Historical(hist) => run_historical(cli, hist, common).await,
    }
}

async fn run_realtime(cli: &crate::cli::Cli, rt: &RealtimeCmd, common: &CommonFlags) -> Result<()> {
    let cfg_path = rt.config.as_ref().or(common.config.as_ref()).ok_or_else(|| {
        AppError::Config("--config is required (provide at track or subcommand)".to_string())
    })?;
    let cfg = config::load_and_validate_config(cfg_path)?;
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
    let provider = provider::connect_auto(&cfg.rpcurl).await?;
    let addrs = config::collect_enabled_addresses(&cfg)?;
    let set = Arc::new(app::build_actionset(&provider, &cfg, &cli));
    if rt.deployments {
        runtime::realtime::run_contract_deployments(provider, Some(set))
            .await
            .map_err(|e| AppError::General(e.to_string()))
    } else if rt.blocks {
        if rt.pending_blocks {
            return runtime::realtime::run_pending_transactions(
                provider,
                addrs,
                Some(set),
                rt.pending_hashes_only,
            )
            .await
            .map_err(|e| AppError::General(e.to_string()));
        }
        // blocks path: rebuild set for blocks (same build function for now)
        let set2 = app::build_actionset(&provider, &cfg, &cli);
        runtime::realtime::run_blocks(provider, addrs, Some(Arc::new(set2)))
            .await
            .map_err(|e| AppError::General(e.to_string()))
    } else {
        runtime::realtime::run_events(provider, addrs, Some(set))
            .await
            .map_err(|e| AppError::General(e.to_string()))
    }
}

async fn run_historical(
    cli: &crate::cli::Cli,
    hist: &crate::cli::HistoricalCmd,
    common: &CommonFlags,
) -> Result<()> {
    // 先取基础配置，在 events/blocks 层允许覆盖
    let base_cfg_path = hist.config.as_ref().or(common.config.as_ref()).ok_or_else(|| {
        AppError::Config(
            "--config is required (provide at track/historical or inside events/blocks)".to_string(),
        )
    })?;
    if let Some(p) = &cli.event_sigs {
        abi::set_event_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cli.func_sigs {
        abi::set_func_sigs_path(p.display().to_string());
    }
    let cfg = config::load_and_validate_config(base_cfg_path)?;
    if let Some(p) = &cfg.event_sigs_path {
        abi::set_event_sigs_path(p.clone());
    }
    if let Some(p) = &cfg.func_sigs_path {
        abi::set_func_sigs_path(p.clone());
    }
    match hist.which {
        HistoricalWhichCmd::Events(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                cfg2 = config::load_and_validate_config(p)?;
                if let Some(ep) = &cfg2.event_sigs_path {
                    abi::set_event_sigs_path(ep.clone());
                }
                if let Some(fp) = &cfg2.func_sigs_path {
                    abi::set_func_sigs_path(fp.clone());
                }
            }
            crate::throttle::init(cfg2.max_requests_per_second);
            let provider = provider::connect_auto(&cfg2.rpcurl).await?;
            let addrs =
                config::collect_enabled_addresses(&cfg2)?;
            let set = app::build_actionset(&provider, &cfg2, &cli);
            runtime::historical::run_events(provider, addrs, range, Some(Arc::new(set)))
                .await
                .map_err(|e| AppError::General(e.to_string()))
        }
        HistoricalWhichCmd::Blocks(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                cfg2 = config::load_and_validate_config(p)?;
                if let Some(ep) = &cfg2.event_sigs_path {
                    abi::set_event_sigs_path(ep.clone());
                }
                if let Some(fp) = &cfg2.func_sigs_path {
                    abi::set_func_sigs_path(fp.clone());
                }
            }
            crate::throttle::init(cfg2.max_requests_per_second);
            let provider = provider::connect_auto(&cfg2.rpcurl).await?;
            let addrs =
                config::collect_enabled_addresses(&cfg2)?;
            let set2 = app::build_actionset(&provider, &cfg2, &cli);
            runtime::historical::run_blocks(provider, addrs, range, Some(Arc::new(set2)))
                .await
                .map_err(|e| AppError::General(e.to_string()))
        }
    }
}

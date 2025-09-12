use std::sync::Arc;

use crate::{
    abi,
    app,
    cli::{CommonFlags, HistoricalWhichCmd, RealtimeCmd, TrackWhichCmd},
    config,
    context::RuntimeContext,
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
    
    // 创建统一的运行时上下文
    let ctx = RuntimeContext::new(cli, cfg.clone())?;
    let track_ctx = ctx.create_sub_context("track");
    
    track_ctx.verbose_log("🚀 Starting realtime tracking...");
    track_ctx.debug_log(&format!("Config loaded from: {}", cfg_path.display()));
    
    crate::throttle::init(cfg.max_requests_per_second);
    
    // 使用上下文进行条件性ABI设置
    if let Some(p) = &cli.event_sigs {
        track_ctx.debug_log(&format!("Setting event signatures from CLI: {}", p.display()));
        abi::set_event_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cli.func_sigs {
        track_ctx.debug_log(&format!("Setting function signatures from CLI: {}", p.display()));
        abi::set_func_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cfg.event_sigs_path {
        track_ctx.debug_log(&format!("Setting event signatures from config: {}", p));
        abi::set_event_sigs_path(p.clone());
    }
    if let Some(p) = &cfg.func_sigs_path {
        track_ctx.debug_log(&format!("Setting function signatures from config: {}", p));
        abi::set_func_sigs_path(p.clone());
    }
    
    let provider = provider::connect_auto(&cfg.rpcurl).await?;
    track_ctx.verbose_log(&format!("Connected to provider: {}", cfg.rpcurl));
    
    let addrs = config::collect_enabled_addresses(&cfg)?;
    track_ctx.verbose_log(&format!("Monitoring {} addresses", addrs.len()));
    
    let set = Arc::new(app::build_actionset_v2(&provider, &cfg, &cli).await?);
    track_ctx.verbose_log(&format!("ActionSet built with {} actions", ctx.get_enabled_actions().len()));
    
    if rt.deployments {
        track_ctx.verbose_log("Running contract deployment tracking");
        runtime::realtime::run_contract_deployments(provider, Some(set))
            .await
            .map_err(|e| AppError::General(e.to_string()))
    } else if rt.blocks {
        if rt.pending_blocks {
            track_ctx.verbose_log("Running pending transactions tracking");
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
        track_ctx.verbose_log("Running block tracking");
        let set2 = app::build_actionset_v2(&provider, &cfg, &cli).await?;
        runtime::realtime::run_blocks(provider, addrs, Some(Arc::new(set2)))
            .await
            .map_err(|e| AppError::General(e.to_string()))
    } else {
        track_ctx.verbose_log("Running event tracking");
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
    
    let cfg = config::load_and_validate_config(base_cfg_path)?;
    let ctx = RuntimeContext::new(cli, cfg.clone())?;
    let hist_ctx = ctx.create_sub_context("historical");
    
    hist_ctx.verbose_log("🏛️  Starting historical tracking...");
    hist_ctx.debug_log(&format!("Base config loaded from: {}", base_cfg_path.display()));
    
    // 配置ABI路径
    if let Some(p) = &cli.event_sigs {
        hist_ctx.debug_log(&format!("Setting event signatures from CLI: {}", p.display()));
        abi::set_event_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cli.func_sigs {
        hist_ctx.debug_log(&format!("Setting function signatures from CLI: {}", p.display()));
        abi::set_func_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cfg.event_sigs_path {
        hist_ctx.debug_log(&format!("Setting event signatures from config: {}", p));
        abi::set_event_sigs_path(p.clone());
    }
    if let Some(p) = &cfg.func_sigs_path {
        hist_ctx.debug_log(&format!("Setting function signatures from config: {}", p));
        abi::set_func_sigs_path(p.clone());
    }
    
    match hist.which {
        HistoricalWhichCmd::Events(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                hist_ctx.debug_log(&format!("Override config from: {}", p.display()));
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
            hist_ctx.verbose_log(&format!("Connected to provider: {}", cfg2.rpcurl));
            
            let addrs = config::collect_enabled_addresses(&cfg2)?;
            hist_ctx.verbose_log(&format!("Monitoring {} addresses for events", addrs.len()));
            
            let set = app::build_actionset_v2(&provider, &cfg2, &cli).await?;
            hist_ctx.verbose_log("ActionSet built for historical events");
            
            runtime::historical::run_events(provider, addrs, range, Some(Arc::new(set)))
                .await
                .map_err(|e| AppError::General(e.to_string()))
        }
        HistoricalWhichCmd::Blocks(ref range) => {
            let mut cfg2 = cfg;
            if let Some(ref p) = range.config {
                hist_ctx.debug_log(&format!("Override config from: {}", p.display()));
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
            hist_ctx.verbose_log(&format!("Connected to provider: {}", cfg2.rpcurl));
            
            let addrs = config::collect_enabled_addresses(&cfg2)?;
            hist_ctx.verbose_log(&format!("Monitoring {} addresses for blocks", addrs.len()));
            
            let set2 = app::build_actionset_v2(&provider, &cfg2, &cli).await?;
            hist_ctx.verbose_log("ActionSet built for historical blocks");
            
            runtime::historical::run_blocks(provider, addrs, range, Some(Arc::new(set2)))
                .await
                .map_err(|e| AppError::General(e.to_string()))
        }
    }
}

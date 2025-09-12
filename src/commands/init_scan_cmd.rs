use crate::{abi, actions, cli, config, context::RuntimeContext, error::{AppError, Result}, provider};
use std::sync::Arc;

pub async fn run(cli: &cli::Cli, cmd: &cli::InitScanCmd) -> Result<()> {
    let cfg_path = cmd
        .config
        .as_ref()
        .ok_or_else(|| AppError::Config("--config is required for initscan".to_string()))?;
    
    let cfg = config::load_and_validate_config(cfg_path)?;
    let ctx = RuntimeContext::new(cli, cfg.clone())?;
    let init_ctx = ctx.create_sub_context("initscan");
    
    init_ctx.verbose_log("🔍 Starting init-scan command...");
    init_ctx.debug_log(&format!("Config loaded from: {}", cfg_path.display()));
    init_ctx.debug_log(&format!("RPC URL: {}", cfg.rpcurl));
    init_ctx.debug_log(&format!("Max requests per second: {}", cfg.max_requests_per_second));
    
    // 配置ABI路径，使用上下文记录
    if let Some(p) = &cli.event_sigs {
        init_ctx.debug_log(&format!("Setting event sigs path from CLI: {}", p.display()));
        abi::set_event_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cli.func_sigs {
        init_ctx.debug_log(&format!("Setting func sigs path from CLI: {}", p.display()));
        abi::set_func_sigs_path(p.display().to_string());
    }
    if let Some(p) = &cfg.event_sigs_path {
        init_ctx.debug_log(&format!("Setting event sigs path from config: {}", p));
        abi::set_event_sigs_path(p.clone());
    }
    if let Some(p) = &cfg.func_sigs_path {
        init_ctx.debug_log(&format!("Setting func sigs path from config: {}", p));
        abi::set_func_sigs_path(p.clone());
    }
    
    if cli.verbose {
        println!("[DEBUG] Initializing throttle");
    }
    crate::throttle::init(cfg.max_requests_per_second);
    
    if cli.verbose {
        println!("[DEBUG] Connecting to provider: {}", cfg.rpcurl);
    }
    let provider = provider::connect_auto(&cfg.rpcurl).await?;
    if cli.verbose {
        println!("[DEBUG] Provider connected successfully");
    }

    let ac = cfg
        .actions
        .get("Initscan")
        .ok_or_else(|| AppError::Config("Config must include actions.Initscan".to_string()))?;

    if !ac.enabled {
        return Err(AppError::Config("actions.Initscan must be enabled".to_string()));
    }
    
    if cli.verbose {
        println!("[DEBUG] Initscan action is enabled");
        println!("[DEBUG] Parsing Initscan options...");
    }

    let o = &ac.options;
    let from = o.get("from-address").and_then(|v| v.as_str()).and_then(|s| s.parse().ok());
    if cli.verbose {
        // You can add debug print here if needed
    }
        
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
    if cli.verbose {
        println!("[DEBUG] Check addresses: {:?}", check_addrs);
    }
    
    let mut func_sigs: Vec<(String, Vec<u8>)> = vec![];
    if let Some(map) = o.get("function-signature-calldata").and_then(|v| v.as_object()) {
        for (k, v) in map {
            if let Some(s) = v.as_str() {
                let h = s.trim_start_matches("0x");
                if let Ok(b) = hex::decode(h) {
                    func_sigs.push((k.clone(), b));
                }
            }
        }
    }
    if cli.verbose {
        println!("[DEBUG] Function signatures: {:?}", func_sigs);
    }
    
    let init_after = o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
    let usd_threshold = o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let webhook_url = o
        .get("webhook-url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| cli.webhook_url.clone());
    let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
    let known_path = o
        .get("initializable-contracts-filepath")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let max_inflight_inits = o.get("init-concurrency").and_then(|v| v.as_u64()).map(|v| v as usize);
    let debug_enabled = o.get("debug").and_then(|v| v.as_bool()).unwrap_or(cli.verbose);
    
    if cli.verbose {
        println!("[DEBUG] Init after delay: {} seconds", init_after);
        println!("[DEBUG] USD threshold: {}", usd_threshold);
        println!("[DEBUG] Webhook URL: {:?}", webhook_url);
        println!("[DEBUG] Known contracts frequency: {:?}", init_known_freq);
        println!("[DEBUG] Known contracts file: {:?}", known_path);
        println!("[DEBUG] Max inflight inits: {:?}", max_inflight_inits);
        println!("[DEBUG] Debug enabled: {}", debug_enabled);
    }
    
    let is_opts = actions::initscan::InitscanOptions {
        from,
        check_addresses: check_addrs,
        init_after_delay_secs: init_after,
        usd_threshold,
        func_sigs,
        webhook_url,
        initializable_contracts_filepath: known_path,
        init_known_contracts_frequency_secs: init_known_freq,
        max_inflight_inits,
        debug: debug_enabled,
    };
    
    let opts = actions::history_init_scan::HistoryInitScanOptions {
        from_block: cmd.from_block,
        to_block: cmd.to_block,
        initscan: is_opts,
        progress_every: cmd.progress_every,
        progress_percent: cmd.progress_percent,
        concurrency: cmd.concurrency,
    };
    
    if cli.verbose {
        println!("[DEBUG] Scan options configured:");
        println!("[DEBUG]   From block: {}", opts.from_block);
        println!("[DEBUG]   To block: {}", opts.to_block);
        println!("[DEBUG]   Progress every: {:?}", opts.progress_every);
        println!("[DEBUG]   Progress percent: {:?}", opts.progress_percent);
        println!("[DEBUG]   Concurrency: {}", opts.concurrency);
        
        println!("[DEBUG] Starting history init scan...");
    }
    let provider = Arc::new(provider);
    actions::history_init_scan::run(provider, opts)
        .await
        .map_err(|e| AppError::General(e.to_string()))
}


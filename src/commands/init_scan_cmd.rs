use std::sync::Arc;
use anyhow::Result;
use crate::{abi, actions, cli, config, provider};

pub async fn run(cli: &cli::Cli, cmd: &cli::InitScanCmd) -> Result<()> {
    let cfg_path = cmd
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--config is required for initscan"))?;
    let cfg = config::load_config(cfg_path)?;
    if let Some(p) = &cli.event_sigs { abi::set_event_sigs_path(p.display().to_string()); }
    if let Some(p) = &cli.func_sigs { abi::set_func_sigs_path(p.display().to_string()); }
    if let Some(p) = &cfg.event_sigs_path { abi::set_event_sigs_path(p.clone()); }
    if let Some(p) = &cfg.func_sigs_path { abi::set_func_sigs_path(p.clone()); }
    crate::throttle::init(cfg.max_requests_per_second);
    let provider = provider::connect_ws(&cfg.rpcurl).await?;

    let ac = cfg
        .actions
        .get("Initscan")
        .ok_or_else(|| anyhow::anyhow!("Config must include actions.Initscan"))?;
    anyhow::ensure!(ac.enabled, "actions.Initscan must be enabled");
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
            if let Some(s) = v.as_str() { let h = s.trim_start_matches("0x"); if let Ok(b) = hex::decode(h) { func_sigs.push((k.clone(), b)); } }
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
        max_inflight_inits,
        debug: o.get("debug").and_then(|v| v.as_bool()).unwrap_or(false),
    };
    let opts = actions::history_init_scan::HistoryInitScanOptions {
        from_block: cmd.from_block,
        to_block: cmd.to_block,
        initscan: is_opts,
        progress_every: cmd.progress_every,
        progress_percent: cmd.progress_percent,
        concurrency: cmd.concurrency,
    };
    let provider = Arc::new(provider);
    actions::history_init_scan::run(provider, opts).await
}


use std::sync::Arc;
use anyhow::Result;
use crate::{abi, actions, cli, config, provider, runtime};

pub async fn run(cli: &cli::Cli, cmd: &cli::SelScanCmd) -> Result<()> {
    let cfg_path = cmd
        .config
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--config is required for sel-scan"))?;
    let cfg = config::load_config(cfg_path)?;
    if let Some(p) = &cli.event_sigs { abi::set_event_sigs_path(p.display().to_string()); }
    if let Some(p) = &cli.func_sigs { abi::set_func_sigs_path(p.display().to_string()); }
    if let Some(p) = &cfg.event_sigs_path { abi::set_event_sigs_path(p.clone()); }
    if let Some(p) = &cfg.func_sigs_path { abi::set_func_sigs_path(p.clone()); }
    crate::throttle::init(cfg.max_requests_per_second);
    let provider = provider::connect_auto(&cfg.rpcurl).await?;
    let mut set = actions::ActionSet::new();
    let s = cmd.selector.trim_start_matches("0x");
    anyhow::ensure!(s.len() == 8, "selector must be 4 bytes (8 hex chars)");
    let bytes = hex::decode(s)?;
    let mut sel = [0u8; 4]; sel.copy_from_slice(&bytes);
    set.add(actions::selector_scan::SelectorScanAction::new(actions::selector_scan::SelectorScanOptions { selector: sel, print_receipts: cmd.print_receipts }));
    let set = Arc::new(set);
    let range = cli::RangeFlags { config: None, from_block: cmd.from_block, to_block: Some(cmd.to_block), step_blocks: cmd.step_blocks };
    runtime::historical::run_blocks(provider, vec![], &range, Some(set)).await
}


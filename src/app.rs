use std::sync::Arc;

use crate::{
    actions::{self, ActionSet},
    cli::Cli,
    config::{ActionConfig, Config},
};
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;

fn logging_cfg<'a>(cli: &Cli, cfg: &'a Config) -> (bool, bool, bool, bool, bool, Option<String>) {
    let log_cfg: Option<&ActionConfig> = cfg.actions.get("Logging");
    if let Some(ac) = log_cfg {
        let o = &ac.options;
        (
            o.get("log-events").and_then(|v| v.as_bool()).unwrap_or(true),
            o.get("log-transactions").and_then(|v| v.as_bool()).unwrap_or(true),
            o.get("log-blocks").and_then(|v| v.as_bool()).unwrap_or(true),
            o.get("enable-terminal-logs").and_then(|v| v.as_bool()).unwrap_or(true),
            o.get("enable-discord-logs").and_then(|v| v.as_bool()).unwrap_or(false) || cli.webhook_url.is_some(),
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
    }
}

fn add_common_actions(set: &mut ActionSet, prov_arc: Arc<RootProvider<BoxTransport>>, cli: &Cli, cfg: &Config) {
    // LoggingAction: now respect optional "Logging".enabled flag (default true)
    let logging_enabled = cfg
        .actions
        .get("Logging")
        .map(|ac| ac.enabled)
        .unwrap_or(true);
    if logging_enabled {
        let (log_events, log_txs, log_blocks, enable_term, enable_disc, disc_url) = logging_cfg(cli, cfg);
        let log_opts = actions::logging::LoggingOptions {
            enable_terminal_logs: enable_term,
            enable_discord_logs: enable_disc,
            discord_webhook_url: disc_url.clone(),
            log_events,
            log_transactions: log_txs,
            log_blocks,
        };
        set.add(actions::logging::LoggingAction::new(log_opts));
    }
    if cli.json {
        set.add(actions::jsonlog::JsonLogAction);
    }
    // --- TransferAction disabled (commented out) ---
    // 原来这里无条件 / 或默认根据配置启用 TransferAction，会打印所有 Transfer 日志。
    // 现在按你的需求注释掉，防止出现小额 [transfer] 噪声。
    // 如果以后需要恢复，只需取消下面整段注释。
    // if cfg
    //     .actions
    //     .get("Transfer")
    //     .map(|ac| ac.enabled)
    //     .unwrap_or(true)
    // {
    //     set.add(actions::transfer::TransferAction::new(prov_arc.clone()));
    // }
    // --- end TransferAction disabled ---
    set.add(actions::ownership::OwnershipAction);
    set.add(actions::proxy::ProxyUpgradeAction::new(prov_arc.clone()));
    // Deployment output to file if configured
    let dep_out = cfg
        .actions
        .get("Deployment")
        .and_then(|ac| ac.options.get("output-filepath"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let dep_opts = actions::deployment::DeploymentOptions { output_filepath: dep_out };
    set.add(actions::deployment::DeploymentScanAction::new(prov_arc.clone(), dep_opts));

    // LargeTransfer optional
    if let Some(ac) = cfg.actions.get("LargeTransfer") {
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
            actions::large_transfer::LargeTransferOptions { min_amount_human: min_h, decimals_default: dec_default },
        ));
    }

    // Tornado optional
    if let Some(path) = cfg
        .actions
        .get("TornadoCash")
        .and_then(|ac| ac.options.get("output-filepath"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        set.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: Some(path) }));
    } else {
        set.add(actions::tornado::TornadoAction::new(actions::tornado::TornadoOptions { output_filepath: None }));
    }
}

fn try_add_initscan(set: &mut ActionSet, prov_arc: Arc<RootProvider<BoxTransport>>, cli: &Cli, cfg: &Config) {
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
            let init_after = o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
            let usd_threshold = o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let webhook_url = o
                .get("webhook-url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| cli.webhook_url.clone());
            let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
            let known_path = o.get("initializable-contracts-filepath").and_then(|v| v.as_str()).map(|s| s.to_string());
            let max_inflight_inits = o.get("init-concurrency").and_then(|v| v.as_u64()).map(|v| v as usize);
            let debug = o.get("debug").and_then(|v| v.as_bool()).unwrap_or(false);
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
                debug,
            };
            set.add(actions::initscan::InitscanAction::new(prov_arc.clone(), is_opts));
        }
    }
}

pub fn build_actionset(provider: &RootProvider<BoxTransport>, cfg: &Config, cli: &Cli) -> ActionSet {
    let prov_arc = Arc::new(provider.clone());
    let mut set = ActionSet::new();
    add_common_actions(&mut set, prov_arc.clone(), cli, cfg);
    try_add_initscan(&mut set, prov_arc.clone(), cli, cfg);
    set
}

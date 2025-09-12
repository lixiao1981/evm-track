use crate::registry::ActionFactory;
use crate::config::ActionConfig;
use crate::error::{AppError, Result};
use crate::output::GlobalOutputManager;
use crate::actions::{Action, initscan::{InitscanAction, InitscanOptions}};
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;
use tracing::debug;

/// Initscan Action工厂
pub struct InitscanActionFactory;

impl ActionFactory for InitscanActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        cli: &crate::cli::Cli,
        _output_manager: Option<GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        debug!("Creating InitscanAction with config: {:#?}", config);
        
        if !config.enabled {
            return Err(AppError::Config("Initscan action is not enabled".to_string()));
        }
        
        let o = &config.options;
        
        // 解析from-address
        let from = o
            .get("from-address")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse().ok());
        
        // 解析check-addresses数组
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
        
        // 解析function-signature-calldata
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
        
        // 解析其他配置选项
        let init_after = o.get("init-after-delay").and_then(|v| v.as_u64()).unwrap_or(1);
        let usd_threshold = o.get("alert-usd-threshold").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let webhook_url = o
            .get("webhook-url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| cli.webhook_url.clone());
        let init_known_freq = o.get("init-known-contracts-frequency").and_then(|v| v.as_u64());
        let known_path = o.get("initializable-contracts-filepath")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let max_inflight_inits = o.get("init-concurrency")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let debug = o.get("debug").and_then(|v| v.as_bool()).unwrap_or(false);
        
        let is_opts = InitscanOptions {
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
        
        debug!("Creating InitscanAction with options: {:#?}", is_opts);
        
        Ok(Box::new(InitscanAction::new(provider, is_opts)))
    }
    
    fn description(&self) -> &str {
        "Monitors contract deployments and automatically initializes them based on configured patterns"
    }
    
    fn dependencies(&self) -> Vec<String> {
        vec![] // Initscan doesn't depend on other actions
    }
    
    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {
                "from-address": "0x4b20993bc481177ec7e8f571cecae8a9e22c02db",
                "check-addresses": [
                    "0x4b20993bc481177ec7e8f571cecae8a9e22c02db",
                    "0xca35b7d915458ef540ade6068dfe2f44e8fa733c"
                ],
                "init-after-delay": 3,
                "init-concurrency": 20,
                "alert-usd-threshold": 0,
                "function-signature-calldata": {
                    "initialize()": "0x8129fc1c",
                    "init()": "0xe1c7392a",
                    "init(address)": "0x19ab453c0000000000000000000000004b20993bc481177ec7e8f571cecae8a9e22c02db"
                },
                "webhook-url": "",
                "initializable-contracts-filepath": "./data/eth_initializable_contracts.json",
                "init-known-contracts-frequency": 86400,
                "debug": true
            }
        })
    }
}

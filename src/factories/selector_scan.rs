use crate::registry::ActionFactory;
use crate::actions::{Action, selector_scan::{SelectorScanAction, SelectorScanOptions}};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use alloy_primitives::hex;
use std::sync::Arc;

/// Selector Scan Action 工厂
pub struct SelectorScanActionFactory;

impl ActionFactory for SelectorScanActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        // 获取第一个选择器（SelectorScanAction只支持单个选择器）
        let selector_str = options.get("selector")
            .and_then(|v| v.as_str())
            .or_else(|| {
                // 如果配置了selectors数组，取第一个
                options.get("selectors")
                    .and_then(|v| v.as_array())
                    .and_then(|arr| arr.get(0))
                    .and_then(|v| v.as_str())
            })
            .unwrap_or("0x00000000"); // 默认选择器
            
        // 解析选择器字符串为字节数组
        let selector_bytes = if selector_str.starts_with("0x") {
            hex::decode(&selector_str[2..])
        } else {
            hex::decode(selector_str)
        }.map_err(|e| crate::error::AppError::Config(format!("Invalid selector hex: {}", e)))?;
        
        if selector_bytes.len() != 4 {
            return Err(crate::error::AppError::Config("Selector must be exactly 4 bytes".to_string()).into());
        }
        
        let mut selector = [0u8; 4];
        selector.copy_from_slice(&selector_bytes);
            
        let print_receipts = options.get("print-receipts")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let opts = SelectorScanOptions {
            selector,
            print_receipts,
        };

        Ok(Box::new(SelectorScanAction::new(opts)))
    }

    fn description(&self) -> &str {
        "Monitor transactions calling specific function selectors"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {
                "selector": "0xa9059cbb",
                "print-receipts": false
            }
        })
    }
}

/// Tornado Cash Action 工厂
pub struct TornadoActionFactory;

impl ActionFactory for TornadoActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let output_filepath = options.get("output-filepath")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let opts = crate::actions::tornado::TornadoOptions { output_filepath };
        Ok(Box::new(crate::actions::tornado::TornadoAction::new(opts)))
    }

    fn description(&self) -> &str {
        "Monitor Tornado Cash deposits and withdrawals"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {
                "0x12D66f87A04A9E220743712cE6d9bB1B5616B8Fc": {},
                "0x47CE0C6eD5B0Ce3d3A51fdb1C52DC66a7c3c2936": {}
            },
            "options": {
                "output-filepath": "./tornado_activity.log"
            }
        })
    }
}

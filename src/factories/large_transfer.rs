use crate::registry::ActionFactory;
use crate::actions::{Action, large_transfer::{LargeTransferAction, LargeTransferOptions}};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

/// Large Transfer Action 工厂
pub struct LargeTransferActionFactory;

impl ActionFactory for LargeTransferActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let min_amount_human = options.get("min-amount")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| {
                options.get("min_amount")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            });
            
        let decimals_default = options.get("decimals-default")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8)
            .unwrap_or(18);

        let opts = LargeTransferOptions {
            min_amount_human,
            decimals_default,
        };

        Ok(Box::new(LargeTransferAction::new(opts)))
    }

    fn description(&self) -> &str {
        "Monitor and log large ERC-20 token transfers above specified thresholds"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {
                "min-amount": "1000000000000000000000",
                "decimals-default": 18
            }
        })
    }
}

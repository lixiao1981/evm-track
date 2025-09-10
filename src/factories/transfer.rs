use crate::registry::ActionFactory;
use crate::actions::{Action, transfer::TransferAction};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

/// Transfer Action 工厂
pub struct TransferActionFactory;

impl ActionFactory for TransferActionFactory {
    fn create_action(
        &self,
        _config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        Ok(Box::new(TransferAction::new(provider)))
    }

    fn description(&self) -> &str {
        "Monitor and log ERC-20 token transfers"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {
                "0xA0b86a33E6418de4bE4C96D4c3c1EbcDFf0aA78E": {},
                "0xdAC17F958D2ee523a2206206994597C13D831ec7": {}
            },
            "options": {
                "min_amount": "1000000000000000000"
            }
        })
    }
}

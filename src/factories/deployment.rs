use crate::registry::ActionFactory;
use crate::actions::{Action, deployment::{DeploymentScanAction, DeploymentOptions}};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

/// Deployment Action 工厂
pub struct DeploymentActionFactory;

impl ActionFactory for DeploymentActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
        _output_manager: Option<crate::output::GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let output_filepath = options.get("output-filepath")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let deployment_opts = DeploymentOptions { output_filepath };

        Ok(Box::new(DeploymentScanAction::new(provider, deployment_opts)))
    }

    fn description(&self) -> &str {
        "Monitor and log smart contract deployments"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {
                "output-filepath": "./deployments.json"
            }
        })
    }
}

/// Ownership Action 工厂
pub struct OwnershipActionFactory;

impl ActionFactory for OwnershipActionFactory {
    fn create_action(
        &self,
        _config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
        _output_manager: Option<crate::output::GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        Ok(Box::new(crate::actions::ownership::OwnershipAction))
    }

    fn description(&self) -> &str {
        "Monitor ownership changes in smart contracts"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {
                "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984": {}
            },
            "options": {}
        })
    }
}

/// Proxy Upgrade Action 工厂
pub struct ProxyUpgradeActionFactory;

impl ActionFactory for ProxyUpgradeActionFactory {
    fn create_action(
        &self,
        _config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
        _output_manager: Option<crate::output::GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        Ok(Box::new(crate::actions::proxy::ProxyUpgradeAction::new(provider)))
    }

    fn description(&self) -> &str {
        "Monitor proxy contract upgrades and implementation changes"
    }

    fn dependencies(&self) -> Vec<String> {
        vec!["Logging".to_string()]
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {
                "0x1f9840a85d5aF5bf1D1762F925BDADdC4201F984": {}
            },
            "options": {}
        })
    }
}

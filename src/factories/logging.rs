use crate::registry::ActionFactory;
use crate::actions::{Action, logging::LoggingAction, logging::LoggingOptions};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

/// Logging Action 工厂
pub struct LoggingActionFactory;

impl ActionFactory for LoggingActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let log_events = options.get("log-events")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
            
        let log_transactions = options.get("log-transactions")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
            
        let log_blocks = options.get("log-blocks")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
            
        let enable_terminal_logs = options.get("enable-terminal-logs")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
            
        let enable_discord_logs = options.get("enable-discord-logs")
            .and_then(|v| v.as_bool())
            .unwrap_or(false) || cli.webhook_url.is_some();
            
        let discord_webhook_url = options.get("discord-webhook-url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| cli.webhook_url.clone());

        let logging_opts = LoggingOptions {
            enable_terminal_logs,
            enable_discord_logs,
            discord_webhook_url,
            log_events,
            log_transactions: log_transactions,
            log_blocks,
        };

        Ok(Box::new(LoggingAction::new(logging_opts)))
    }

    fn description(&self) -> &str {
        "Log blockchain events, transactions, and blocks to terminal and/or Discord"
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {
                "log-events": true,
                "log-transactions": true,
                "log-blocks": false,
                "enable-terminal-logs": true,
                "enable-discord-logs": false,
                "discord-webhook-url": "https://discord.com/api/webhooks/..."
            }
        })
    }
}

/// JsonLog Action 工厂
pub struct JsonLogActionFactory;

impl ActionFactory for JsonLogActionFactory {
    fn create_action(
        &self,
        _config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        Ok(Box::new(crate::actions::jsonlog::JsonLogAction))
    }

    fn description(&self) -> &str {
        "Output events and transactions in JSON format"
    }

    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {}
        })
    }
}

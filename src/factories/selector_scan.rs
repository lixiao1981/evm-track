use crate::registry::ActionFactory;
use crate::actions::{Action, selector_scan::SelectorScanAction, selector_scan::SelectorScanOptions};
use crate::actions::{tornado::TornadoAction, tornado::TornadoOptions};
use crate::config::ActionConfig;
use crate::error::Result;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

/// SelectorScan Action 工厂
pub struct SelectorScanActionFactory;

impl ActionFactory for SelectorScanActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
        _output_manager: Option<crate::output::GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let selector_str = options.get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("0x00000000");
            
        let print_receipts = options.get("print-receipts")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
            
        // Parse selector from hex string
        let selector_bytes = if selector_str.starts_with("0x") {
            hex::decode(&selector_str[2..]).unwrap_or_default()
        } else {
            hex::decode(selector_str).unwrap_or_default()
        };
        
        let mut selector = [0u8; 4];
        if selector_bytes.len() >= 4 {
            selector[..4].copy_from_slice(&selector_bytes[..4]);
        }
            
        let opts = SelectorScanOptions {
            selector,
            print_receipts,
        };
        
        Ok(Box::new(SelectorScanAction::new(opts)))
    }
    
    fn description(&self) -> &str {
        "扫描交易中的函数选择器"
    }
    
    fn dependencies(&self) -> Vec<String> {
        vec![]
    }
}

/// Tornado Action 工厂
pub struct TornadoActionFactory;

impl ActionFactory for TornadoActionFactory {
    fn create_action(
        &self,
        config: &ActionConfig,
        _provider: Arc<RootProvider<BoxTransport>>,
        _cli: &crate::cli::Cli,
        _output_manager: Option<crate::output::GlobalOutputManager>,
    ) -> Result<Box<dyn Action>> {
        let options = &config.options;
        
        let output_filepath = options.get("output-file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
            
        let opts = TornadoOptions {
            output_filepath,
            verbose: false, // 在工厂模式中默认不开启verbose
        };
        
        Ok(Box::new(TornadoAction::new(opts)))
    }
    
    fn description(&self) -> &str {
        "检测Tornado Cash相关的隐私交易"
    }
    
    fn dependencies(&self) -> Vec<String> {
        vec![]
    }
}

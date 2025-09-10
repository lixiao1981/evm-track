/// Action 工厂模块
/// 
/// 这个模块包含了所有Action的工厂实现，用于动态创建Action实例

pub mod logging;
pub mod transfer;
pub mod large_transfer;
pub mod deployment;
pub mod selector_scan;

// 重新导出所有工厂
pub use logging::{LoggingActionFactory, JsonLogActionFactory};
pub use transfer::TransferActionFactory;
pub use large_transfer::LargeTransferActionFactory;
pub use deployment::{DeploymentActionFactory, OwnershipActionFactory, ProxyUpgradeActionFactory};
pub use selector_scan::{SelectorScanActionFactory, TornadoActionFactory};

use crate::registry::ActionRegistry;

/// 创建并初始化默认的Action注册表
pub fn create_default_registry() -> ActionRegistry {
    let mut registry = ActionRegistry::new();
    
    // 注册所有内置Actions
    registry.register("Logging", LoggingActionFactory);
    registry.register("JsonLog", JsonLogActionFactory);
    registry.register("Transfer", TransferActionFactory);
    registry.register("LargeTransfer", LargeTransferActionFactory);
    registry.register("Deployment", DeploymentActionFactory);
    registry.register("Ownership", OwnershipActionFactory);
    registry.register("ProxyUpgrade", ProxyUpgradeActionFactory);
    registry.register("SelectorScan", SelectorScanActionFactory);
    registry.register("Tornado", TornadoActionFactory);
    
    tracing::info!("🔧 Initialized action registry with {} factories", registry.list_actions().len());
    
    registry
}

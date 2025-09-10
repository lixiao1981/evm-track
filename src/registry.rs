use crate::actions::{Action, ActionSet};
use crate::config::ActionConfig;
use crate::error::{AppError, Result};
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn, error};

/// Action工厂特征，用于创建Action实例
pub trait ActionFactory: Send + Sync {
    /// 创建Action实例
    fn create_action(
        &self,
        config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>>;
    
    /// 获取Action的描述信息
    fn description(&self) -> &str;
    
    /// 获取Action的依赖列表
    fn dependencies(&self) -> Vec<String> { 
        vec![] 
    }
    
    /// 获取Action的配置示例
    fn config_example(&self) -> serde_json::Value {
        serde_json::json!({
            "enabled": true,
            "addresses": {},
            "options": {}
        })
    }
}

/// Action注册表
pub struct ActionRegistry {
    factories: HashMap<String, Box<dyn ActionFactory>>,
}

impl ActionRegistry {
    /// 创建新的注册表
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }
    
    /// 注册一个Action工厂
    pub fn register<F>(&mut self, name: &str, factory: F)
    where
        F: ActionFactory + 'static,
    {
        debug!("Registering action factory: {}", name);
        self.factories.insert(name.to_string(), Box::new(factory));
    }
    
    /// 创建Action实例
    pub fn create_action(
        &self,
        name: &str,
        config: &ActionConfig,
        provider: Arc<RootProvider<BoxTransport>>,
        cli: &crate::cli::Cli,
    ) -> Result<Box<dyn Action>> {
        if let Some(factory) = self.factories.get(name) {
            debug!("Creating action instance: {}", name);
            factory.create_action(config, provider, cli)
        } else {
            Err(AppError::Config(format!("Unknown action: {}", name)))
        }
    }
    
    /// 列出所有注册的Action
    pub fn list_actions(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
    
    /// 获取Action的描述信息
    pub fn get_description(&self, name: &str) -> Option<&str> {
        self.factories.get(name).map(|f| f.description())
    }
    
    /// 获取Action的依赖关系
    pub fn get_dependencies(&self, name: &str) -> Option<Vec<String>> {
        self.factories.get(name).map(|f| f.dependencies())
    }
    
    /// 获取Action的配置示例
    pub fn get_config_example(&self, name: &str) -> Option<serde_json::Value> {
        self.factories.get(name).map(|f| f.config_example())
    }
    
    /// 检查Action是否已注册
    pub fn is_registered(&self, name: &str) -> bool {
        self.factories.contains_key(name)
    }
    
    /// 解析依赖关系并返回排序后的Action列表
    pub fn resolve_dependencies(&self, action_names: &[String]) -> Result<Vec<String>> {
        let mut resolved = Vec::new();
        let mut visiting = std::collections::HashSet::new();
        let mut visited = std::collections::HashSet::new();
        
        for name in action_names {
            if !visited.contains(name) {
                self.visit_action(name, &mut resolved, &mut visiting, &mut visited)?;
            }
        }
        
        Ok(resolved)
    }
    
    /// 深度优先遍历解决依赖关系
    fn visit_action(
        &self,
        name: &str,
        resolved: &mut Vec<String>,
        visiting: &mut std::collections::HashSet<String>,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        if visiting.contains(name) {
            return Err(AppError::Config(format!(
                "Circular dependency detected involving action: {}", name
            )));
        }
        
        if visited.contains(name) {
            return Ok(());
        }
        
        visiting.insert(name.to_string());
        
        if let Some(deps) = self.get_dependencies(name) {
            for dep in deps {
                if !self.is_registered(&dep) {
                    warn!("Action '{}' depends on unregistered action '{}'", name, dep);
                    continue;
                }
                self.visit_action(&dep, resolved, visiting, visited)?;
            }
        }
        
        visiting.remove(name);
        visited.insert(name.to_string());
        resolved.push(name.to_string());
        
        Ok(())
    }
}

/// 构建ActionSet的主要函数
pub fn build_actionset_dynamic(
    registry: &ActionRegistry,
    provider: &RootProvider<BoxTransport>,
    config: &crate::config::Config,
    cli: &crate::cli::Cli,
) -> Result<ActionSet> {
    let mut set = ActionSet::new();
    let provider_arc = Arc::new(provider.clone());
    
    info!("🚀 Building ActionSet using dynamic registry...");
    
    // 收集所有启用的Actions
    let mut enabled_actions = Vec::new();
    for (action_name, action_config) in &config.actions {
        if action_config.enabled {
            if registry.is_registered(action_name) {
                enabled_actions.push(action_name.clone());
                debug!("Found enabled action: {}", action_name);
            } else {
                warn!("Action '{}' is enabled in config but not registered", action_name);
            }
        }
    }
    
    // 处理CLI参数添加的特殊Actions
    if cli.json {
        enabled_actions.push("JsonLog".to_string());
    }
    
    // 解析依赖关系
    let sorted_actions = registry.resolve_dependencies(&enabled_actions)?;
    info!("Action loading order (with dependencies): {:?}", sorted_actions);
    
    // 按依赖顺序创建Actions
    for action_name in sorted_actions {
        // 跳过CLI特殊Actions（它们在后面单独处理）
        if action_name == "JsonLog" {
            continue;
        }
        
        if let Some(action_config) = config.actions.get(&action_name) {
            match registry.create_action(&action_name, action_config, provider_arc.clone(), cli) {
                Ok(action) => {
                    info!("✅ Loaded action: {}", action_name);
                    set.add_boxed(action);
                },
                Err(e) => {
                    error!("❌ Failed to load action '{}': {}", action_name, e);
                    return Err(e);
                }
            }
        }
    }
    
    // 处理CLI特殊Actions
    if cli.json {
        if registry.is_registered("JsonLog") {
            let dummy_config = crate::config::ActionConfig::default();
            match registry.create_action("JsonLog", &dummy_config, provider_arc.clone(), cli) {
                Ok(action) => {
                    info!("✅ Loaded CLI action: JsonLog");
                    set.add_boxed(action);
                },
                Err(e) => {
                    warn!("Failed to load JsonLog action: {}", e);
                }
            }
        }
    }
    
    info!("🎉 ActionSet built successfully with {} actions", set.len());
    Ok(set)
}

/// 用于注册Action的便利宏
#[macro_export]
macro_rules! register_action {
    ($registry:expr, $name:expr, $factory:expr) => {
        $registry.register($name, $factory);
        tracing::debug!("Registered action: {}", $name);
    };
}

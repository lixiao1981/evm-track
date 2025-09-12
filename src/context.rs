/// 统一配置上下文系统
/// 
/// 这个模块提供了一个中心化的配置管理系统，解决了CLI参数在多层函数调用中容易丢失的问题
/// 
/// 核心特性：
/// - 统一的配置上下文传递机制
/// - 配置验证和调试输出
/// - 构建器模式支持
/// - 层级配置合并

use crate::{cli::Cli, config::Config, error::{AppError, Result}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// 运行时上下文，包含所有运行时配置信息
#[derive(Debug, Clone)]
pub struct RuntimeContext {
    /// CLI参数
    pub cli: CliContext,
    /// 配置文件内容
    pub config: Config,
    /// 运行时标志
    pub runtime: RuntimeFlags,
    /// 扩展配置
    pub extensions: HashMap<String, serde_json::Value>,
}

/// CLI上下文，从原始CLI提取的结构化信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliContext {
    /// 详细输出模式
    pub verbose: bool,
    /// JSON输出模式
    pub json: bool,
    /// Webhook URL
    pub webhook_url: Option<String>,
    /// 自定义函数签名路径
    pub func_sigs_path: Option<String>,
    /// 自定义事件签名路径
    pub event_sigs_path: Option<String>,
    /// 调试模式（从verbose推导）
    pub debug: bool,
    /// 日志级别
    pub log_level: LogLevel,
}

/// 运行时标志
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeFlags {
    /// 是否处于测试模式
    pub test_mode: bool,
    /// 是否启用性能监控
    pub performance_monitoring: bool,
    /// 最大并发数
    pub max_concurrency: Option<usize>,
    /// 请求限制
    pub rate_limit: Option<u64>,
}

/// 日志级别枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl RuntimeContext {
    /// 创建新的运行时上下文
    pub fn new(cli: &Cli, config: Config) -> Result<Self> {
        let cli_context = CliContext::from_cli(cli);
        let runtime = RuntimeFlags::default();
        let extensions = HashMap::new();
        
        let context = Self {
            cli: cli_context,
            config,
            runtime,
            extensions,
        };
        
        // 验证配置
        context.validate()?;
        
        // 调试输出
        if context.cli.debug {
            context.debug_print();
        }
        
        Ok(context)
    }
    
    /// 配置验证
    pub fn validate(&self) -> Result<()> {
        info!("🔍 Validating runtime configuration...");
        
        // 验证基本配置
        if self.config.rpcurl.is_empty() {
            return Err(AppError::Config("RPC URL cannot be empty".to_string()));
        }
        
        // 验证Action配置
        if self.config.actions.is_empty() {
            warn!("No actions configured - system will not process any events");
        }
        
        // 验证CLI和配置的一致性
        if self.cli.verbose && self.config.actions.values().all(|a| !a.enabled) {
            warn!("Verbose mode enabled but no actions are enabled");
        }
        
        info!("✅ Configuration validation passed");
        Ok(())
    }
    
    /// 调试输出配置信息
    pub fn debug_print(&self) {
        debug!("=== Runtime Context Debug Info ===");
        debug!("CLI Context: {:#?}", self.cli);
        debug!("Runtime Flags: {:#?}", self.runtime);
        debug!("Enabled Actions: {:?}", self.get_enabled_actions());
        debug!("RPC URL: {}", self.config.rpcurl);
        debug!("Max Requests/Second: {}", self.config.max_requests_per_second);
        debug!("Extensions: {:?}", self.extensions.keys().collect::<Vec<_>>());
        debug!("=== End Debug Info ===");
    }
    
    /// 获取启用的Action列表
    pub fn get_enabled_actions(&self) -> Vec<String> {
        self.config
            .actions
            .iter()
            .filter(|(_, config)| config.enabled)
            .map(|(name, _)| name.clone())
            .collect()
    }
    
    /// 检查Action是否启用
    pub fn is_action_enabled(&self, action_name: &str) -> bool {
        self.config
            .actions
            .get(action_name)
            .map(|config| config.enabled)
            .unwrap_or(false)
    }
    
    /// 获取Action配置
    pub fn get_action_config(&self, action_name: &str) -> Option<&crate::config::ActionConfig> {
        self.config.actions.get(action_name)
    }
    
    /// 设置扩展配置
    pub fn set_extension<T: Serialize>(&mut self, key: &str, value: T) -> Result<()> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| AppError::Config(format!("Failed to serialize extension '{}': {}", key, e)))?;
        self.extensions.insert(key.to_string(), json_value);
        Ok(())
    }
    
    /// 获取扩展配置
    pub fn get_extension<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        self.extensions
            .get(key)
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }
    
    /// 创建子上下文（用于特定组件）
    pub fn create_sub_context(&self, component_name: &str) -> ComponentContext {
        ComponentContext {
            parent: self,
            component_name: component_name.to_string(),
        }
    }
    
    /// 检查是否应该输出详细信息
    pub fn should_verbose(&self, component: Option<&str>) -> bool {
        match component {
            Some(comp) => {
                // 检查特定组件的verbose配置
                if let Some(action_config) = self.get_action_config(comp) {
                    action_config.options
                        .get("verbose")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(self.cli.verbose)
                } else {
                    self.cli.verbose
                }
            }
            None => self.cli.verbose,
        }
    }
    
    /// 检查是否应该输出调试信息
    pub fn should_debug(&self, component: Option<&str>) -> bool {
        match component {
            Some(comp) => {
                if let Some(action_config) = self.get_action_config(comp) {
                    action_config.options
                        .get("debug")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(self.cli.debug)
                } else {
                    self.cli.debug
                }
            }
            None => self.cli.debug,
        }
    }
}

impl CliContext {
    /// 从CLI参数创建上下文
    pub fn from_cli(cli: &Cli) -> Self {
        let log_level = if cli.verbose {
            LogLevel::Debug
        } else {
            LogLevel::Info
        };
        
        Self {
            verbose: cli.verbose,
            json: cli.json,
            webhook_url: cli.webhook_url.clone(),
            func_sigs_path: cli.func_sigs.as_ref().map(|p| p.display().to_string()),
            event_sigs_path: cli.event_sigs.as_ref().map(|p| p.display().to_string()),
            debug: cli.verbose, // debug模式从verbose推导
            log_level,
        }
    }
}

impl Default for RuntimeFlags {
    fn default() -> Self {
        Self {
            test_mode: false,
            performance_monitoring: false,
            max_concurrency: None,
            rate_limit: None,
        }
    }
}

/// 组件专用上下文，包含对父上下文的引用和组件特定信息
pub struct ComponentContext<'a> {
    parent: &'a RuntimeContext,
    component_name: String,
}

impl<'a> ComponentContext<'a> {
    /// 获取组件名称
    pub fn component_name(&self) -> &str {
        &self.component_name
    }
    
    /// 检查是否应该详细输出
    pub fn verbose(&self) -> bool {
        self.parent.should_verbose(Some(&self.component_name))
    }
    
    /// 检查是否应该调试输出
    pub fn debug(&self) -> bool {
        self.parent.should_debug(Some(&self.component_name))
    }
    
    /// 获取父上下文
    pub fn parent(&self) -> &RuntimeContext {
        self.parent
    }
    
    /// 获取组件配置
    pub fn config(&self) -> Option<&crate::config::ActionConfig> {
        self.parent.get_action_config(&self.component_name)
    }
    
    /// 记录调试信息
    pub fn debug_log(&self, message: &str) {
        if self.debug() {
            debug!("[{}] {}", self.component_name, message);
        }
    }
    
    /// 记录详细信息
    pub fn verbose_log(&self, message: &str) {
        if self.verbose() {
            info!("[{}] {}", self.component_name, message);
        }
    }
}

/// 配置构建器，用于创建复杂的配置
pub struct RuntimeContextBuilder {
    cli: Option<Cli>,
    config: Option<Config>,
    runtime_flags: RuntimeFlags,
    extensions: HashMap<String, serde_json::Value>,
}

impl RuntimeContextBuilder {
    /// 创建新的构建器
    pub fn new() -> Self {
        Self {
            cli: None,
            config: None,
            runtime_flags: RuntimeFlags::default(),
            extensions: HashMap::new(),
        }
    }
    
    /// 设置CLI参数
    pub fn cli(mut self, cli: Cli) -> Self {
        self.cli = Some(cli);
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }
    
    /// 启用测试模式
    pub fn test_mode(mut self, enabled: bool) -> Self {
        self.runtime_flags.test_mode = enabled;
        self
    }
    
    /// 启用性能监控
    pub fn performance_monitoring(mut self, enabled: bool) -> Self {
        self.runtime_flags.performance_monitoring = enabled;
        self
    }
    
    /// 设置最大并发数
    pub fn max_concurrency(mut self, max: usize) -> Self {
        self.runtime_flags.max_concurrency = Some(max);
        self
    }
    
    /// 设置请求限制
    pub fn rate_limit(mut self, limit: u64) -> Self {
        self.runtime_flags.rate_limit = Some(limit);
        self
    }
    
    /// 添加扩展配置
    pub fn extension<T: Serialize>(mut self, key: &str, value: T) -> Result<Self> {
        let json_value = serde_json::to_value(value)
            .map_err(|e| AppError::Config(format!("Failed to serialize extension '{}': {}", key, e)))?;
        self.extensions.insert(key.to_string(), json_value);
        Ok(self)
    }
    
    /// 构建运行时上下文
    pub fn build(self) -> Result<RuntimeContext> {
        let cli = self.cli.ok_or_else(|| AppError::Config("CLI is required".to_string()))?;
        let config = self.config.ok_or_else(|| AppError::Config("Config is required".to_string()))?;
        
        let cli_context = CliContext::from_cli(&cli);
        
        let context = RuntimeContext {
            cli: cli_context,
            config,
            runtime: self.runtime_flags,
            extensions: self.extensions,
        };
        
        context.validate()?;
        
        if context.cli.debug {
            context.debug_print();
        }
        
        Ok(context)
    }
}

impl Default for RuntimeContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 上下文感知的宏，用于统一的日志记录
#[macro_export]
macro_rules! ctx_debug {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug() {
            tracing::debug!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! ctx_info {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.verbose() {
            tracing::info!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! ctx_warn {
    ($ctx:expr, $($arg:tt)*) => {
        tracing::warn!($($arg)*);
    };
}

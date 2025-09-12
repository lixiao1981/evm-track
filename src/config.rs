use crate::error::{AppError, Result};
use crate::output::OutputConfig;
use alloy_primitives::Address;
use serde::Deserialize;
use std::{collections::HashMap, path::{Path, PathBuf}, str::FromStr, fs};
use tracing::{warn, debug};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub rpcurl: String,
    #[serde(default)]
    pub actions: HashMap<String, ActionConfig>,
    #[serde(default)]
    pub event_sigs_path: Option<String>,
    #[serde(default)]
    pub func_sigs_path: Option<String>,
    #[serde(rename = "max-requests-per-second")]
    #[serde(default)]
    pub max_requests_per_second: u32,
    #[serde(default)]
    pub output: Option<OutputConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpcurl: "http://localhost:8545".to_string(),
            actions: HashMap::new(),
            event_sigs_path: None,
            func_sigs_path: None,
            max_requests_per_second: 10,
            output: None,
        }
    }
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ActionConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub addresses: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub options: serde_json::Value,
    #[serde(default)]
    pub output: Option<OutputConfig>,
}

pub fn load_config(path: &PathBuf) -> Result<Config> {
    let data = std::fs::read_to_string(path)?;
    let cfg: Config = serde_json::from_str(&data)?;
    Ok(cfg)
}

pub fn collect_enabled_addresses(cfg: &Config) -> Result<Vec<Address>> {
    let mut set = std::collections::BTreeSet::new();
    for (_name, action) in cfg.actions.iter() {
        if action.enabled {
            for (addr_str, _props) in action.addresses.iter() {
                let addr = Address::from_str(addr_str)
                    .map_err(|e| AppError::Config(format!("invalid address in config: {} ({})", addr_str, e)))?;
                set.insert(addr);
            }
        }
    }
    if set.is_empty() {
        warn!("No enabled actions with addresses; filters will be empty");
    }
    Ok(set.into_iter().collect())
}

// ========== 统一配置加载和验证系统 ==========

/// 通用配置加载器，支持多种配置文件格式
pub struct ConfigLoader;

impl ConfigLoader {
    /// 统一的配置文件加载方法
    /// 支持 JSON, TOML 格式自动检测
    pub fn load_config<T>(config_path: &Path) -> Result<T> 
    where 
        T: for<'de> Deserialize<'de>
    {
        debug!("Loading config from: {:?}", config_path);
        
        if !config_path.exists() {
            return Err(AppError::Config(format!(
                "Configuration file not found: {}", 
                config_path.display()
            )));
        }

        let content = fs::read_to_string(config_path)
            .map_err(|e| AppError::Config(format!(
                "Failed to read config file {}: {}", 
                config_path.display(), e
            )))?;

        // 根据文件扩展名选择解析器
        let config = match config_path.extension().and_then(|s| s.to_str()) {
            Some("json") => {
                serde_json::from_str(&content)
                    .map_err(|e| AppError::Config(format!(
                        "Invalid JSON in {}: {}", 
                        config_path.display(), e
                    )))?
            },
            Some("toml") => {
                toml::from_str(&content)
                    .map_err(|e| AppError::Config(format!(
                        "Invalid TOML in {}: {}", 
                        config_path.display(), e
                    )))?
            },
            _ => {
                // 尝试JSON优先，失败则尝试TOML
                serde_json::from_str(&content)
                    .or_else(|_| toml::from_str(&content))
                    .map_err(|e| AppError::Config(format!(
                        "Failed to parse {} as JSON or TOML: {}", 
                        config_path.display(), e
                    )))?
            }
        };

        debug!("Successfully loaded config from: {:?}", config_path);
        Ok(config)
    }

    /// 加载动作特定配置
    pub fn load_action_config<T>(action_name: &str, config_dir: Option<&Path>) -> Result<T> 
    where 
        T: for<'de> Deserialize<'de>
    {
        let config_dir = config_dir.unwrap_or_else(|| Path::new("."));
        
        // 尝试多种配置文件命名模式
        let possible_paths = vec![
            config_dir.join(format!("{}.json", action_name)),
            config_dir.join(format!("{}.config.json", action_name)),
            config_dir.join(format!("config.{}.json", action_name)),
            config_dir.join(format!("{}.toml", action_name)),
            config_dir.join(format!("{}.config.toml", action_name)),
            config_dir.join(format!("config.{}.toml", action_name)),
        ];

        for path in possible_paths {
            if path.exists() {
                debug!("Found config for action '{}' at: {:?}", action_name, path);
                return Self::load_config(&path);
            }
        }

        Err(AppError::Config(format!(
            "No configuration file found for action '{}' in directory: {}", 
            action_name, config_dir.display()
        )))
    }
}

/// 配置验证器，提供统一的验证规则
pub struct ConfigValidator;

impl ConfigValidator {
    /// 验证主配置文件
    pub fn validate_main_config(config: &Config) -> Result<()> {
        debug!("Validating main configuration");
        
        // 验证 RPC URL
        Self::validate_rpc_url(&config.rpcurl)?;
        
        // 验证动作配置
        for (action_name, action_config) in &config.actions {
            Self::validate_action_config(action_name, action_config)?;
        }

        debug!("Main configuration validation passed");
        Ok(())
    }

    /// 验证 RPC URL 格式
    pub fn validate_rpc_url(rpc_url: &str) -> Result<()> {
        if rpc_url.trim().is_empty() {
            return Err(AppError::Config("RPC URL cannot be empty".to_string()));
        }

        // 基本URL格式验证
        if !rpc_url.starts_with("http://") && !rpc_url.starts_with("https://") && !rpc_url.starts_with("ws://") && !rpc_url.starts_with("wss://") {
            return Err(AppError::Config(format!(
                "Invalid RPC URL format: '{}'. Must start with http://, https://, ws://, or wss://", 
                rpc_url
            )));
        }

        Ok(())
    }

    /// 验证动作配置
    pub fn validate_action_config(action_name: &str, config: &ActionConfig) -> Result<()> {
        debug!("Validating action config: {}", action_name);

        // 验证地址列表
        for (addr_str, _) in &config.addresses {
            Address::from_str(addr_str)
                .map_err(|e| AppError::Config(format!(
                    "Invalid address '{}' in action '{}': {}", 
                    addr_str, action_name, e
                )))?;
        }

        // 根据动作类型进行特定验证
        Self::validate_action_specific(action_name, config)?;

        debug!("Action config validation passed: {}", action_name);
        Ok(())
    }

    /// 动作特定验证规则
    fn validate_action_specific(action_name: &str, config: &ActionConfig) -> Result<()> {
        match action_name {
            "transfer" => {
                // 验证转账动作的特定参数
                if let Some(threshold) = config.options.get("min_amount") {
                    threshold.as_str()
                        .ok_or_else(|| AppError::Config("min_amount must be a string".to_string()))?
                        .parse::<f64>()
                        .map_err(|_| AppError::Config(format!(
                            "Invalid min_amount in transfer config: {}", threshold
                        )))?;
                }
            },
            "large_transfer" => {
                // 验证大额转账的阈值参数（支持大数值，使用 f64 解析以支持科学记数法）
                if let Some(threshold) = config.options.get("threshold") {
                    let threshold_str = threshold.as_str()
                        .ok_or_else(|| AppError::Config("threshold must be a string".to_string()))?;
                    
                    // 尝试解析为数字（支持大数值）
                    threshold_str.parse::<f64>()
                        .map_err(|_| AppError::Config(format!(
                            "Invalid threshold in large_transfer config: {} (must be a valid number)", 
                            threshold_str
                        )))
                        .and_then(|val| {
                            if val < 0.0 {
                                Err(AppError::Config("threshold cannot be negative".to_string()))
                            } else {
                                Ok(val)
                            }
                        })?;
                }
            },
            "ownership" => {
                // 验证所有权变更的参数
                if config.addresses.is_empty() {
                    return Err(AppError::Config(
                        "Ownership action requires at least one contract address".to_string()
                    ));
                }
            },
            "deployment" => {
                // 部署监控可以没有地址（监控所有部署）
                debug!("Deployment action: monitoring all deployments");
            },
            _ => {
                // 其他动作的通用验证
                debug!("No specific validation rules for action: {}", action_name);
            }
        }
        Ok(())
    }

    /// 验证配置完整性（跨动作依赖检查）
    pub fn validate_config_integrity(config: &Config) -> Result<()> {
        debug!("Validating configuration integrity");

        let enabled_actions: Vec<_> = config.actions.iter()
            .filter(|(_, cfg)| cfg.enabled)
            .collect();

        if enabled_actions.is_empty() {
            warn!("No actions are enabled in configuration");
        }

        // 检查动作间的依赖关系
        for (action_name, _) in &enabled_actions {
            match action_name.as_str() {
                "large_transfer" => {
                    // 大额转账通常需要transfer动作也启用
                    if !config.actions.get("transfer").map_or(false, |cfg| cfg.enabled) {
                        warn!("large_transfer is enabled but transfer is not - consider enabling transfer for better coverage");
                    }
                },
                _ => {}
            }
        }

        debug!("Configuration integrity validation passed");
        Ok(())
    }
}

// 便利函数：统一的配置加载入口
pub fn load_and_validate_config(config_path: &Path) -> Result<Config> {
    debug!("Loading and validating configuration from: {:?}", config_path);
    
    let config = ConfigLoader::load_config(config_path)?;
    ConfigValidator::validate_main_config(&config)?;
    ConfigValidator::validate_config_integrity(&config)?;
    
    debug!("Configuration loaded and validated successfully");
    Ok(config)
}

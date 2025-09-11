// 动态Action注册机制演示示例

use evm_track::config::{Config, ActionConfig};
use evm_track::output::{OutputConfig, OutputFormat};
use evm_track::registry::ActionRegistry;
use evm_track::output::GlobalOutputManager;
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 EVM-Track 动态Action注册机制演示");
    println!("=====================================");
    
    // 1. 创建动态注册表
    let mut registry = ActionRegistry::new();
    println!("✅ 创建了动态Action注册表");
    
    // 2. 注册所有Action工厂
    registry.register_all_factories();
    println!("✅ 注册了 {} 个Action工厂", registry.get_factory_names().len());
    
    // 3. 显示可用的Action类型
    println!("\n📋 可用的Action类型:");
    for name in registry.get_factory_names() {
        if let Some(factory) = registry.get_factory(&name) {
            println!("   - {}: {}", name, factory.description());
        }
    }
    
    // 4. 创建示例配置
    let mut actions = HashMap::new();
    
    // Transfer Action 配置
    actions.insert("transfer".to_string(), ActionConfig {
        enabled: true,
        addresses: HashMap::new(),
        options: json!({
            "min-value": "1000000000000000000", // 1 ETH
            "output-file": "/tmp/transfers.json"
        }),
        output: Some(OutputConfig {
            format: OutputFormat::Json,
            file_path: Some("/tmp/transfer_results.json".into()),
            buffer_size: 1000,
            rotate_size_mb: Some(10), // 10MB
            compress: false,
            auto_flush_interval_secs: 30,
        }),
    });
    
    // Tornado Action 配置
    actions.insert("tornado".to_string(), ActionConfig {
        enabled: true,
        addresses: HashMap::new(),
        options: json!({
            "output-file": "/tmp/tornado.json"
        }),
        output: None, // 使用全局输出配置
    });
    
    // Selector Scan Action 配置
    actions.insert("selector_scan".to_string(), ActionConfig {
        enabled: true,
        addresses: HashMap::new(),
        options: json!({
            "selector": "0xa9059cbb", // transfer(address,uint256)
            "print-receipts": true
        }),
        output: Some(OutputConfig {
            format: OutputFormat::Csv,
            file_path: Some("/tmp/selector_scan.csv".into()),
            buffer_size: 500,
            rotate_size_mb: Some(5), // 5MB
            compress: false,
            auto_flush_interval_secs: 30,
        }),
    });
    
    let config = Config {
        actions,
        // 全局输出配置
        output: Some(OutputConfig {
            format: OutputFormat::JsonLines,
            file_path: Some("/tmp/evm_track_global.jsonl".into()),
            buffer_size: 2000,
            rotate_size_mb: Some(20), // 20MB
            compress: false,
            auto_flush_interval_secs: 30,
        }),
        ..Default::default()
    };
    
    println!("\n⚙️  配置了 {} 个Action:", config.actions.len());
    for (name, action_config) in &config.actions {
        let output_info = if action_config.output.is_some() {
            "独立输出配置"
        } else {
            "使用全局输出"
        };
        println!("   - {}: {} ({})", name, 
               if action_config.enabled { "启用" } else { "禁用" }, 
               output_info);
    }
    
    // 5. 创建全局输出管理器
    let global_output = if let Some(output_config) = &config.output {
        GlobalOutputManager::new(output_config.clone()).await?
    } else {
        GlobalOutputManager::new(OutputConfig::default()).await?
    };
    let output_format = config.output.as_ref()
        .map(|o| format!("{:?}", o.format))
        .unwrap_or("默认".to_string());
    println!("\n📄 创建了全局输出管理器: {}", output_format);
    
    // 6. 动态构建Action集合（需要Provider和CLI，这里仅作演示）
    println!("\n🔧 动态Action构建过程演示:");
    
    // 检查依赖关系
    let enabled_actions: Vec<String> = config.actions.iter()
        .filter(|(_, cfg)| cfg.enabled)
        .map(|(name, _)| name.clone())
        .collect();
    
    println!("   - 启用的Action: {:?}", enabled_actions);
    
    // 解析依赖（这里只是演示，实际的依赖解析在build_actionset_dynamic中）
    for action_name in &enabled_actions {
        if let Some(factory) = registry.get_factory(action_name) {
            let deps = factory.dependencies();
            if !deps.is_empty() {
                println!("   - {}: 依赖 {:?}", action_name, deps);
            } else {
                println!("   - {}: 无依赖", action_name);
            }
        }
    }
    
    // 7. 展示配置示例
    println!("\n📖 Action配置示例:");
    for action_name in &enabled_actions {
        if let Some(factory) = registry.get_factory(action_name) {
            let example = factory.config_example();
            println!("   - {}:", action_name);
            println!("     {}", serde_json::to_string_pretty(&example)?);
        }
    }
    
    println!("\n✨ 动态Action注册机制演示完成！");
    println!("\n🎯 主要特点:");
    println!("   • 插件式架构：Action通过工厂模式动态加载");
    println!("   • 依赖管理：自动解析和排序Action依赖关系");
    println!("   • 统一输出：支持多种格式和文件轮转");
    println!("   • 配置灵活：支持全局和单独的输出配置");
    println!("   • 可扩展性：新增Action只需实现ActionFactory trait");
    
    Ok(())
}

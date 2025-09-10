use evm_track::config::{load_and_validate_config, ConfigValidator};
use std::path::Path;

fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 测试统一配置加载和验证
    let config_path = Path::new("./test_unified_config.json");
    
    println!("🚀 Testing unified configuration loading system...");
    
    match load_and_validate_config(config_path) {
        Ok(config) => {
            println!("✅ Configuration loaded and validated successfully!");
            println!("📊 Configuration summary:");
            println!("  - RPC URL: {}", config.rpcurl);
            println!("  - Max requests per second: {}", config.max_requests_per_second);
            println!("  - Total actions: {}", config.actions.len());
            
            let enabled_actions: Vec<_> = config.actions.iter()
                .filter(|(_, cfg)| cfg.enabled)
                .map(|(name, _)| name)
                .collect();
            
            println!("  - Enabled actions: {:?}", enabled_actions);
            
            // 测试配置完整性验证
            match ConfigValidator::validate_config_integrity(&config) {
                Ok(_) => println!("✅ Configuration integrity validation passed!"),
                Err(e) => println!("❌ Configuration integrity validation failed: {}", e),
            }
        },
        Err(e) => {
            println!("❌ Configuration loading/validation failed: {}", e);
        }
    }
    
    // 测试无效配置
    println!("\n🧪 Testing invalid configuration handling...");
    let invalid_config_path = Path::new("./nonexistent_config.json");
    
    match load_and_validate_config(invalid_config_path) {
        Ok(_) => println!("❌ Should have failed for nonexistent config!"),
        Err(e) => println!("✅ Correctly handled invalid config: {}", e),
    }
}

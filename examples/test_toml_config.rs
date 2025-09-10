use evm_track::config::{load_and_validate_config, ConfigLoader};
use std::path::Path;

fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("🚀 Testing TOML configuration support...");
    
    let toml_config_path = Path::new("./test_config.toml");
    
    match load_and_validate_config(toml_config_path) {
        Ok(config) => {
            println!("✅ TOML Configuration loaded and validated successfully!");
            println!("📊 TOML Configuration summary:");
            println!("  - RPC URL: {}", config.rpcurl);
            println!("  - Max requests per second: {}", config.max_requests_per_second);
            println!("  - Total actions: {}", config.actions.len());
            
            let enabled_actions: Vec<_> = config.actions.iter()
                .filter(|(_, cfg)| cfg.enabled)
                .map(|(name, _)| name)
                .collect();
            
            println!("  - Enabled actions: {:?}", enabled_actions);
        },
        Err(e) => {
            println!("❌ TOML Configuration loading failed: {}", e);
        }
    }
    
    // 测试动作特定配置加载
    println!("\n🧪 Testing action-specific configuration loading...");
    
    // 测试不存在的动作配置
    match ConfigLoader::load_action_config::<serde_json::Value>("nonexistent_action", None) {
        Ok(_) => println!("❌ Should have failed for nonexistent action config!"),
        Err(e) => println!("✅ Correctly handled missing action config: {}", e),
    }
}

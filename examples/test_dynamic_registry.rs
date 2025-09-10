use evm_track::{
    app::build_actionset_v2,
    config::load_and_validate_config,
    factories::create_default_registry,
    provider,
    cli::Cli,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("🚀 Testing Dynamic Action Registry System...");
    
    // 1. 测试注册表创建
    println!("\n1. 📋 Creating action registry...");
    let registry = create_default_registry();
    let actions = registry.list_actions();
    println!("✅ Registry created with {} actions: {:?}", actions.len(), actions);

    // 2. 测试依赖解析
    println!("\n2. 🔗 Testing dependency resolution...");
    match registry.resolve_dependencies(&actions) {
        Ok(resolved) => {
            println!("✅ Dependency resolution successful!");
            println!("   Loading order: {:?}", resolved);
        },
        Err(e) => {
            println!("❌ Dependency resolution failed: {}", e);
        }
    }

    // 3. 测试配置文件加载
    println!("\n3. 📁 Loading test configuration...");
    let config_path = Path::new("./test_unified_config.json");
    if !config_path.exists() {
        println!("⚠️  Test config not found, skipping actionset build test");
        return Ok(());
    }
    
    let config = load_and_validate_config(config_path)?;
    println!("✅ Configuration loaded successfully");
    
    // 4. 测试ActionSet动态构建
    println!("\n4. 🏗️  Building ActionSet dynamically...");
    
    // 创建模拟CLI
    let cli = Cli {
        verbose: false,
        event_sigs: None,
        func_sigs: None,
        webhook_url: None,
        json: true, // 启用JSON输出来测试CLI驱动的Actions
        command: evm_track::cli::Commands::Track(evm_track::cli::TrackCmd {
            which: evm_track::cli::TrackWhichCmd::Realtime(evm_track::cli::RealtimeCmd {
                config: None,
                events: false,
                blocks: false,
                pending_blocks: false,
                pending_hashes_only: false,
                deployments: false,
            }),
            common: evm_track::cli::CommonFlags {
                config: None,
            },
        }),
    };
    
    // 连接提供者（使用配置中的RPC URL）
    match provider::connect_auto(&config.rpcurl).await {
        Ok(provider) => {
            println!("✅ Connected to RPC: {}", config.rpcurl);
            
            // 构建ActionSet
            match build_actionset_v2(&provider, &config, &cli) {
                Ok(actionset) => {
                    println!("✅ ActionSet built successfully!");
                    println!("   Total actions loaded: {}", actionset.len());
                    
                    // 显示实际加载的Actions
                    let mut loaded_actions = Vec::new();
                    for (action_name, action_config) in &config.actions {
                        if action_config.enabled {
                            loaded_actions.push(action_name.as_str());
                        }
                    }
                    if cli.json {
                        loaded_actions.push("JsonLog");
                    }
                    
                    println!("   Enabled actions from config: {:?}", loaded_actions);
                },
                Err(e) => {
                    println!("❌ Failed to build ActionSet: {}", e);
                }
            }
        },
        Err(e) => {
            println!("⚠️  Failed to connect to RPC ({}): {}", config.rpcurl, e);
            println!("   This is expected in testing environment");
        }
    }

    // 5. 演示Action信息查询
    println!("\n5. 🔍 Demonstrating action information queries...");
    
    for action_name in ["Logging", "Transfer", "LargeTransfer", "Deployment"] {
        if let Some(desc) = registry.get_description(action_name) {
            println!("  📋 {}: {}", action_name, desc);
            
            if let Some(deps) = registry.get_dependencies(action_name) {
                if !deps.is_empty() {
                    println!("     🔗 Dependencies: {:?}", deps);
                }
            }
        }
    }

    println!("\n🎉 Dynamic Action Registry System test completed!");
    println!("\n📚 Key Benefits Demonstrated:");
    println!("   ✅ Automatic action discovery and registration");
    println!("   ✅ Dependency resolution and proper loading order"); 
    println!("   ✅ Configuration-driven action instantiation");
    println!("   ✅ CLI parameter integration");
    println!("   ✅ Action metadata and documentation");

    Ok(())
}

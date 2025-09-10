use evm_track::factories::create_default_registry;
use clap::{Parser, Subcommand};
use serde_json;

#[derive(Parser)]
#[command(name = "action-registry")]
#[command(about = "Action registry management tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all registered actions
    List,
    /// Show action details
    Info { 
        /// Action name to inspect
        action: String 
    },
    /// Show config example for an action
    Example { 
        /// Action name
        action: String 
    },
    /// Show dependency graph
    Dependencies,
}

fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    let registry = create_default_registry();

    match &cli.command {
        Commands::List => {
            println!("🚀 Registered Actions:");
            let actions = registry.list_actions();
            for (i, action) in actions.iter().enumerate() {
                if let Some(desc) = registry.get_description(action) {
                    println!("  {}. {} - {}", i + 1, action, desc);
                } else {
                    println!("  {}. {}", i + 1, action);
                }
            }
            println!("\nTotal: {} actions registered", actions.len());
        },

        Commands::Info { action } => {
            if let Some(desc) = registry.get_description(action) {
                println!("📋 Action: {}", action);
                println!("📝 Description: {}", desc);
                
                if let Some(deps) = registry.get_dependencies(action) {
                    if !deps.is_empty() {
                        println!("🔗 Dependencies: {:?}", deps);
                    } else {
                        println!("🔗 Dependencies: None");
                    }
                }
            } else {
                println!("❌ Action '{}' not found", action);
                println!("Available actions: {:?}", registry.list_actions());
            }
        },

        Commands::Example { action } => {
            if let Some(example) = registry.get_config_example(action) {
                println!("📋 Configuration Example for '{}':", action);
                println!("{}", serde_json::to_string_pretty(&example).unwrap());
            } else {
                println!("❌ Action '{}' not found", action);
                println!("Available actions: {:?}", registry.list_actions());
            }
        },

        Commands::Dependencies => {
            println!("🔗 Action Dependency Graph:");
            let actions = registry.list_actions();
            
            for action in &actions {
                if let Some(deps) = registry.get_dependencies(action) {
                    if !deps.is_empty() {
                        println!("  {} -> {:?}", action, deps);
                    } else {
                        println!("  {} (no dependencies)", action);
                    }
                }
            }

            // 测试依赖解析
            println!("\n🧪 Testing dependency resolution...");
            match registry.resolve_dependencies(&actions) {
                Ok(resolved) => {
                    println!("✅ Loading order: {:?}", resolved);
                },
                Err(e) => {
                    println!("❌ Dependency resolution failed: {}", e);
                }
            }
        }
    }
}

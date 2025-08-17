use anyhow::Result;
use clap::Parser;
use cli::Commands;
use tracing_subscriber::EnvFilter;

mod abi;
mod actions;
mod app;
mod commands;
mod cli;
mod config;
mod data_cmd;
mod provider;
mod runtime;
mod throttle;
mod public_provider;
#[tokio::main]
async fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    let filter_layer = if cli.verbose {
        EnvFilter::new("info")
    } else {
        EnvFilter::new("warn")
    };
    tracing_subscriber::fmt()
        .with_env_filter(filter_layer)
        .init();

    match &cli.command {
        Commands::Track(track) => {
            return commands::track::run(&cli, &track.which, &track.common).await;
        }
        Commands::Data(cmd) => match &cmd.which {
            cli::DataWhichCmd::Event(args) => {
                crate::data_cmd::add_events_from_abi(&args.abi, &args.output)?;
                Ok(())
            }
            cli::DataWhichCmd::FetchAbi(args) => {
                let s = crate::data_cmd::fetch_abi_from_scanner(
                    &args.address,
                    &args.scanner_url,
                    args.api_key.as_deref(),
                )
                .await?;
                std::fs::write(&args.output, s)?;
                println!("wrote ABI to {}", args.output.display());
                Ok(())
            }
        },
        Commands::InitScan(cmd) => {
            // 配置加载（仅此子命令层）
            return commands::init_scan_cmd::run(&cli, cmd).await;
        }
        Commands::SelScan(cmd) => {
            return commands::sel_scan_cmd::run(&cli, cmd).await;
        }
    }
}

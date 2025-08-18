use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;
use evm_track::cli::{Cli, Commands, DataWhichCmd};
use evm_track::commands::{track, init_scan_cmd, sel_scan_cmd};
use evm_track::data_cmd;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
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
            return track::run(&cli, &track.which, &track.common).await;
        }
        Commands::Data(cmd) => match &cmd.which {
            DataWhichCmd::Event(args) => {
                data_cmd::add_events_from_abi(&args.abi, &args.output)?;
                Ok(())
            }
            DataWhichCmd::FetchAbi(args) => {
                let s = data_cmd::fetch_abi_from_scanner(
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
            return init_scan_cmd::run(&cli, cmd).await;
        }
        Commands::SelScan(cmd) => {
            return sel_scan_cmd::run(&cli, cmd).await;
        }
    }
}

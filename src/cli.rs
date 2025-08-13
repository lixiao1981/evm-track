use std::path::PathBuf;

pub use clap::Parser;
use clap::{Args, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "evm-track", version, about = "Track BSC/EVM events and blocks")]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output JSON lines to stdout (adds a JSON logging action)
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,

    /// Optional webhook URL for logging (Discord-style JSON)
    #[arg(long, global = true)]
    pub webhook_url: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Track(TrackCmd),
}

#[derive(Debug, Args)]
pub struct CommonFlags {
    #[arg(long)]
    pub config: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum TrackCmd {
    Realtime {
        #[command(flatten)]
        common: CommonFlags,
        #[command(subcommand)]
        which: RealtimeCmd,
    },
    Historical {
        #[command(flatten)]
        common: CommonFlags,
        #[command(subcommand)]
        which: HistoricalCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum RealtimeCmd {
    Events,
    Blocks {
        #[arg(long, default_value_t = false)]
        pending_blocks: bool,
    },
}

#[derive(Debug, Args, Clone, Copy)]
pub struct RangeFlags {
    #[arg(long)]
    pub from_block: u64,
    #[arg(long)]
    pub to_block: Option<u64>,
    #[arg(long, default_value_t = 10_000)]
    pub step_blocks: u64,
}

#[derive(Debug, Subcommand)]
pub enum HistoricalCmd {
    Events(RangeFlags),
    Blocks(RangeFlags),
}

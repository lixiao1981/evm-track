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

    /// Override path to function signatures JSON
    #[arg(long, global = true)]
    pub func_sigs: Option<PathBuf>,

    /// Override path to event signatures JSON
    #[arg(long, global = true)]
    pub event_sigs: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Track(TrackCmd),
    Data(DataCmd),
}

#[derive(Debug, Args)]
pub struct CommonFlags {
    #[arg(long)]
    pub config: PathBuf,
}

#[derive(Debug, Args)]
pub struct TrackCmd {
    #[command(flatten)]
    pub common: CommonFlags,
    #[command(subcommand)]
    pub which: TrackWhichCmd,
}

#[derive(Debug, Subcommand)]
pub enum TrackWhichCmd {
    Realtime(RealtimeCmd),
    Historical(HistoricalCmd),
}

#[derive(Debug, Args)]
pub struct RealtimeCmd {
    #[arg(long, default_value_t = false)]
    pub events: bool,
    #[arg(long, default_value_t = false)]
    pub blocks: bool,
    #[arg(long, default_value_t = false)]
    pub pending_blocks: bool,
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
#[derive(Debug, Args)]
pub struct HistoricalCmd {
    #[command(subcommand)]
    pub which: HistoricalWhichCmd,
}

#[derive(Debug, Subcommand)]
pub enum HistoricalWhichCmd {
    Events(RangeFlags),
    Blocks(RangeFlags),
}

#[derive(Debug, Args)]
pub struct DataCmd {
    #[command(subcommand)]
    pub which: DataWhichCmd,
}

#[derive(Debug, Subcommand)]
pub enum DataWhichCmd {
    /// Add event signatures to JSON data file from ABI file
    Event(EventArgs),
    /// Fetch contract ABI from a block scanner API
    FetchAbi(FetchAbiArgs),
}

#[derive(Debug, Args)]
pub struct EventArgs {
    /// ABI file path (JSON array of ABI items)
    #[arg(long)]
    pub abi: PathBuf,
    /// Output JSON path (default ./data/event_sigs.json)
    #[arg(long, default_value = "./data/event_sigs.json")]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct FetchAbiArgs {
    /// Contract address (0x...)
    #[arg(long)]
    pub address: String,
    /// Scanner URL template containing %v for address, e.g. https://api.bscscan.com/api?module=contract&action=getabi&address=%v&format=raw
    #[arg(long)]
    pub scanner_url: String,
    /// Optional API key appended as &apikey=KEY if not already in scanner_url
    #[arg(long)]
    pub api_key: Option<String>,
    /// Output ABI JSON file path
    #[arg(long)]
    pub output: PathBuf,
}

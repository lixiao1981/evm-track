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
    /// Run historical init-scan over a block range
    InitScan(InitScanCmd),
    /// Scan transactions from null.json and get their traces
    HistoryTxScan(HistoryTxScanCmd),
}

#[derive(Debug, Args)]
pub struct CommonFlags {
    #[arg(long)]
    pub config: Option<PathBuf>,
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
    /// 可在此处提供配置路径，优先级高于 `track --config`
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, default_value_t = false)]
    pub events: bool,
    #[arg(long, default_value_t = false)]
    pub blocks: bool,
    #[arg(long, default_value_t = false)]
    pub pending_blocks: bool,
    /// 仅订阅待打包交易的哈希，避免某些节点 full-pending 缺字段导致的反序列化错误
    #[arg(long, default_value_t = false)]
    pub pending_hashes_only: bool,
}

#[derive(Debug, Args, Clone)]
pub struct RangeFlags {
    /// 可在此处提供配置路径，优先级高于 `historical --config` 与 `track --config`
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long)]
    pub from_block: u64,
    #[arg(long)]
    pub to_block: Option<u64>,
    #[arg(long, default_value_t = 10_000)]
    pub step_blocks: u64,
}
#[derive(Debug, Args)]
pub struct HistoricalCmd {
    /// 可在此处提供配置路径，优先级高于 `track --config`
    #[arg(long)]
    pub config: Option<PathBuf>,
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

#[derive(Debug, Args)]
pub struct InitScanCmd {
    /// 配置路径（包含 Initscan 的配置项）
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// 起始区块（包含）
    #[arg(long)]
    pub from_block: u64,
    /// 结束区块（包含）
    #[arg(long)]
    pub to_block: u64,
    /// 进度每 N 个区块打印一次（优先于 percent）
    #[arg(long)]
    pub progress_every: Option<u64>,
    /// 进度按百分比打印（例如 1 表示每 1% 打印一次）
    #[arg(long)]
    pub progress_percent: Option<u64>,
    /// 并发数量
    #[arg(long, default_value_t = 10)]
    pub concurrency: usize,
}

#[derive(Debug, Args)]
pub struct HistoryTxScanCmd {
    /// Number of concurrent tasks
    #[arg(long, default_value_t = 10)]
    pub concurrency: usize,
    /// Path to config file
    #[arg(long)]
    pub config: Option<PathBuf>,
    /// Print progress every N transactions
    #[arg(long)]
    pub progress_every: Option<u64>,
    /// Print progress every P percent
    #[arg(long)]
    pub progress_percent: Option<u64>,
}

use crate::error::Result;
use reqwest::Client;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::{Action, BlockRecord, EventRecord, TxRecord};

#[derive(Clone, Default)]
pub struct LoggingOptions {
    pub enable_terminal_logs: bool,
    pub enable_discord_logs: bool,
    pub discord_webhook_url: Option<String>,
    pub log_events: bool,
    pub log_transactions: bool,
    pub log_blocks: bool,
}

pub struct LoggingAction {
    opts: LoggingOptions,
    http: Option<Arc<Client>>, // reused client
    queue: Arc<Mutex<Vec<String>>>,
}

impl LoggingAction {
    pub fn new(opts: LoggingOptions) -> Self {
        let http = if opts.enable_discord_logs {
            Some(Arc::new(Client::new()))
        } else {
            None
        };
        Self {
            opts,
            http,
            queue: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn send_discord(&self, content: String) {
        if !self.opts.enable_discord_logs {
            return;
        }
        if let (Some(client), Some(url)) = (&self.http, &self.opts.discord_webhook_url) {
            let payload = DiscordMessage { content };
            let _ = client.post(url).json(&payload).send().await;
        }
    }
}

impl Action for LoggingAction {
    fn on_event(&self, e: &EventRecord) -> Result<()> {
        if self.opts.enable_terminal_logs && self.opts.log_events {
            println!(
                " [event] block={:?} addr={:?} tx={:?} name={:?}",
                e.block_number, e.address, e.tx_hash, e.name
            );
            if e.name.is_none() {
                println!("  [decode] unknown_topic0 (未匹配到事件签名)");
            }
            for f in &e.fields {
                println!("  {} = {:?}", f.name, f.value);
            }
        }
        if self.opts.enable_discord_logs && self.opts.log_events {
            if let (Some(client), Some(url)) = (&self.http, &self.opts.discord_webhook_url) {
                let s = format!(
                    "aa [event] block={:?} addr={:?} tx={:?} name={:?}",
                    e.block_number, e.address, e.tx_hash, e.name
                );
                let client = client.clone();
                let url = url.clone();
                tokio::spawn(async move {
                    let payload = DiscordMessage { content: s };
                    let _ = client.post(&url).json(&payload).send().await;
                });
            }
        }
         Ok(())
    }

    fn on_tx(&self, t: &TxRecord) -> Result<()> {
        if self.opts.enable_terminal_logs && self.opts.log_transactions {
            println!(
                "[tx] hash={:?} to={:?} from={:?} func={:?}",
                t.hash, t.to, t.from, t.func_name
            );
            if t.input_selector.is_some() && t.func_name.is_none() {
                println!("  [decode] unknown_selector (未匹配到函数签名)");
            }
        }
        if self.opts.enable_discord_logs && self.opts.log_transactions {
            if let (Some(client), Some(url)) = (&self.http, &self.opts.discord_webhook_url) {
                let s = format!(
                    "[tx] hash={:?} to={:?} from={:?} func={:?}",
                    t.hash, t.to, t.from, t.func_name
                );
                let client = client.clone();
                let url = url.clone();
                tokio::spawn(async move {
                    let payload = DiscordMessage { content: s };
                    let _ = client.post(&url).json(&payload).send().await;
                });
            }
        }
        Ok(())
    }

    fn on_block(&self, b: &BlockRecord) -> Result<()> {
        if self.opts.enable_terminal_logs && self.opts.log_blocks {
            println!("[block] number={}", b.number);
        }
        if self.opts.enable_discord_logs && self.opts.log_blocks {
            if let (Some(client), Some(url)) = (&self.http, &self.opts.discord_webhook_url) {
                let s = format!("[block] number={}", b.number);
                let client = client.clone();
                let url = url.clone();
                tokio::spawn(async move {
                    let payload = DiscordMessage { content: s };
                    let _ = client.post(&url).json(&payload).send().await;
                });
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct DiscordMessage {
    content: String,
}

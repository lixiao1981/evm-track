use std::fs::OpenOptions;
use std::io::Write;

use super::{Action, EventRecord};

// Minimal TornadoCash-like detector with optional file output
#[derive(Clone, Default)]
pub struct TornadoOptions {
    pub output_filepath: Option<String>,
}

pub struct TornadoAction {
    opts: TornadoOptions,
}

impl TornadoAction {
    pub fn new(opts: TornadoOptions) -> Self {
        Self { opts }
    }
}

impl Action for TornadoAction {
    fn on_event(&self, e: &EventRecord) -> anyhow::Result<()> {
        if let Some(name) = &e.name {
            match name.as_str() {
                "Deposit" | "Deposited" => {
                    let line = format!(
                        "[tornado] deposit addr={} tx={:?} block={:?}",
                        e.address, e.tx_hash, e.block_number
                    );
                    println!("{}", line);
                    if let Some(path) = &self.opts.output_filepath {
                        let _ = append_line(path, &line);
                    }
                }
                "Withdrawal" | "Withdraw" => {
                    let line = format!(
                        "[tornado] withdrawal addr={} tx={:?} block={:?}",
                        e.address, e.tx_hash, e.block_number
                    );
                    println!("{}", line);
                    if let Some(path) = &self.opts.output_filepath {
                        let _ = append_line(path, &line);
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn append_line(path: &str, s: &str) -> std::io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", s)?;
    Ok(())
}

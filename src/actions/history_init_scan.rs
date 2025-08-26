use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use tracing::{info, warn};

use crate::provider::public_provider_get_receipt;

use super::initscan::{InitscanAction, InitscanOptions};
use std::io::Write;
use super::TxLite;

#[derive(Debug, Clone)]
pub struct HistoryInitScanOptions {
    pub from_block: u64,
    pub to_block: u64,
    pub initscan: InitscanOptions,
    pub progress_every: Option<u64>,
    pub progress_percent: Option<u64>,
    pub concurrency: usize,
}

pub async fn run(
    provider: Arc<RootProvider<BoxTransport>>,
    opts: HistoryInitScanOptions,
) -> Result<()> {
    let initscan = Arc::new(InitscanAction::new(
        Arc::clone(&provider),
        opts.initscan.clone(),
    ));
    let from = opts.from_block;
    let to = opts.to_block;
    let total = to.saturating_sub(from).saturating_add(1);
    let concurrency = if opts.concurrency == 0 {
        10
    } else {
        opts.concurrency
    };

    let tick = if let Some(n) = opts.progress_every {
        n.max(1)
    } else if let Some(p) = opts.progress_percent {
        ((total.saturating_mul(p.max(1))) / 100).max(1)
    } else {
        (total / 100).max(1)
    };

    println!(
        "[initscan] starting historical scan: from={} to={} total={} blocks concurrency={}",
        from,
        to,
        total,
        concurrency
    );

    let processed = Arc::new(AtomicU64::new(0));

    #[derive(Debug, Deserialize, serde::Serialize)]
    struct TxLite {
        hash: alloy_primitives::B256,
        #[serde(default)]
        to: Option<alloy_primitives::Address>,
    }
    #[derive(Deserialize)]
    struct BlockLite {
        #[allow(dead_code)]
        number: Option<String>,
        #[serde(default)]
        transactions: Vec<TxLite>,
    }

    let block_stream = stream::iter(from..=to);

    block_stream
        .for_each_concurrent(concurrency, |n| {
            let provider = Arc::clone(&provider);
            let _initscan = Arc::clone(&initscan);
            let processed = Arc::clone(&processed);

            async move {
                let result: Result<()> = async {
                    let block_hex = format!("0x{:x}", n);
                    let v: serde_json::Value = provider
                        .client()
                        .request("eth_getBlockByNumber", serde_json::json!([block_hex, true]))
                        .await?;

                    if v.is_null() {
                        return Ok(());
                    }

                    let b: BlockLite = serde_json::from_value(v)?;

                    for tx in b.transactions {
                        if tx.to.is_none() {
                            // Action 1: Log the transaction to a file, with robust error handling.
                            match serde_json::to_string(&tx) {
                                Ok(json_string) => {
                                    match std::fs::OpenOptions::new()
                                        .create(true)
                                        .append(true)
                                        .open("data/null.json")
                                    {
                                        Ok(mut file) => {
                                            if let Err(e) = writeln!(file, "{}", json_string) {
                                                warn!(
                                                    "[data-log] Failed to write to data/null.json: {}",
                                                    e
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            warn!(
                                                "[data-log] Failed to open or create data/null.json: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "[data-log] Failed to serialize transaction {:?} to JSON: {}",
                                        tx.hash,
                                        e
                                    );
                                }
                            }

                            // Action 2: Perform the initscan vulnerability check.
                            // let receipt = match provider.get_transaction_receipt(tx.hash).await {
                            //     Ok(r) => r,
                            //     Err(e) => {
                            //         let err_str = e.to_string();
                            //         if err_str.contains("-32000") {
                            //             info!(
                            //                 "Got -32000 error, retrying with public provider for tx: {}",
                            //                 tx.hash
                            //             );
                            //             match public_provider_get_receipt(tx.hash).await {
                            //                 Ok(Some(receipt_from_public)) => {
                            //                     Some(receipt_from_public)
                            //                 }
                            //                 Ok(None) => None,
                            //                 Err(public_err) => {
                            //                     warn!(
                            //                         "Public provider also failed for tx {}: {}",
                            //                         tx.hash,
                            //                         public_err
                            //                     );
                            //                     None
                            //                 }
                            //             }
                            //         } else {
                            //             warn!(
                            //                 "get_transaction_receipt {:?} error: {}; skipping",
                            //                 tx.hash,
                            //                 err_str
                            //             );
                            //             None
                            //         }
                            //     }
                            // };

                            // if let Some(r) = receipt {
                            //     if let Some(addr) = r.contract_address {
                            //         initscan.try_init_for_contract(addr, Some(n)).await;
                            //     }
                            // }
                        }
                    }
                    Ok(())
                }
                .await;

                if let Err(e) = result {
                    warn!("error processing block {}: {}; skipping", n, e);
                }

                let current_processed = processed.fetch_add(1, Ordering::SeqCst) + 1;
                if current_processed % tick == 0 || current_processed == total {
                    let pct = (current_processed as f64 / total as f64) * 100.0;
                    println!(
                        "[initscan] block progress: {}/{} ({:.0}%)",
                        current_processed,
                        total,
                        pct
                    );
                }
            }
        })
        .await;

    println!("[initscan] historical scan finished.");
    Ok(())
}
use std::sync::Arc;
use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::Result;
use tracing::info;
use crate::provider::public_provider_get_receipt;
// use crate::actions::ActionSet;

use super::initscan::{InitscanAction, InitscanOptions};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct HistoryInitScanOptions {
    pub from_block: u64,
    pub to_block: u64,
    pub initscan: InitscanOptions,
    pub progress_every: Option<u64>,
    pub progress_percent: Option<u64>,
}

pub async fn run(
    provider: Arc<RootProvider<BoxTransport>>,
    opts: HistoryInitScanOptions,
) -> Result<()> {
    // Explicit block walking and CREATE detection (independent of logs)
    let initscan = InitscanAction::new(Arc::new(provider.as_ref().clone()), opts.initscan.clone());
    let from = opts.from_block;
    let to = opts.to_block;
    let total = to.saturating_sub(from).saturating_add(1);
    let tick = if let Some(n) = opts.progress_every {
        n.max(1)
    } else if let Some(p) = opts.progress_percent {
        ((total.saturating_mul(p.max(1))) / 100).max(1)
    } else {
        (total / 100).max(1)
    };
    println!(
        "[initscan] starting historical scan: from={} to={} total={} blocks",
        from, to, total
    );
    let mut processed: u64 = 0;

    #[derive(Debug, Deserialize)]
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

    let mut n = from;
    while n <= to {
        // Using a closure to make error handling and progress updates cleaner
        let result: Result<()> = async {
            let block_hex = format!("0x{:x}", n);
            let v: serde_json::Value = provider
                .client()
                .request("eth_getBlockByNumber", serde_json::json!([block_hex, true]))
                .await?;

            if v.is_null() {
                return Ok(()); // Block not found, but not a fatal error
            }

            let b: BlockLite = serde_json::from_value(v)?;

            for tx in b.transactions {
                // Fixed the stray '"' syntax error here
                info!("processing transaction: {:?}", tx.to);
                if tx.to.is_none() {
                    let receipt = match provider.get_transaction_receipt(tx.hash).await {
                        Ok(r) => r,
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("-32000") {
                                // If we get a common provider error, try to fetch from the public provider as a fallback.
                                info!("Got -32000 error, retrying with public provider for tx: {}", tx.hash);
                                match public_provider_get_receipt(tx.hash).await {
                                    Ok(Some(receipt_from_public)) => Some(receipt_from_public),
                                    Ok(None) => None,
                                    Err(public_err) => {
                                        warn!("Public provider also failed for tx {}: {}", tx.hash, public_err);
                                        None
                                    }
                                }
                            } else {
                                warn!(
                                    "get_transaction_receipt {:?} error: {}; skipping",
                                    tx.hash, err_str
                                );
                                None
                            }
                        }
                    };

                    if let Some(r) = receipt {
                        if let Some(addr) = r.contract_address {
                            initscan.try_init_for_contract(addr, Some(n)).await;
                        }
                    }
                }
            }
            Ok(())
        }
        .await;

        if let Err(e) = result {
            warn!("error processing block {}: {}; skipping", n, e);
        }

        processed = processed.saturating_add(1);
        if processed % tick == 0 || processed == total {
            let pct = (processed as f64 / total as f64) * 100.0;
            println!(
                "[initscan] block progress: {}/{} ({:.0}%) current={}",
                processed, total, pct, n
            );
        }
        n = n.saturating_add(1);
    }
    Ok(())
}

use std::sync::Arc;

use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::Result;
use tracing::info;

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
    struct TxLite { hash: alloy_primitives::B256, #[serde(default)] to: Option<alloy_primitives::Address> }
    #[derive(Deserialize)]
    struct BlockLite { #[allow(dead_code)] number: Option<String>, #[serde(default)] transactions: Vec<TxLite> }

    let mut n = from;
    while n <= to {
        let block_hex = format!("0x{:x}", n);
        let v: serde_json::Value = match provider
            .client()
            .request(
                "eth_getBlockByNumber",
                serde_json::json!([block_hex, true])
            )
            .await
        
        {
            Ok(v) => {
                info!("block is {:?}", &v);
                v
            },
            Err(e) => {
                warn!(
                    "eth_getBlockByNumber {} error: {}; skipping",
                    n, e
                );
                processed = processed.saturating_add(1);
                if processed % tick == 0 || processed == total {
                    let pct = (processed as f64 / total as f64) * 100.0;
                    println!(
                        "[initscan] block progress: {}/{} ({:.0}%) current={}",
                        processed, total, pct, n
                    );
                }
                n = n.saturating_add(1);
                continue;
            }
        };

        if v.is_null() {
            processed = processed.saturating_add(1);
            if processed % tick == 0 || processed == total {
                let pct = (processed as f64 / total as f64) * 100.0;
                println!(
                    "[initscan] block progress: {}/{} ({:.0}%) current={}",
                    processed, total, pct, n
                );
            }
            n = n.saturating_add(1);
            continue;
        }

        let b: BlockLite = match serde_json::from_value(v) {
            Ok(b) => b,
            Err(e) => {
                warn!(
                    "parse block {} error: {}; skipping",
                    n, e
                );
                processed = processed.saturating_add(1);
                if processed % tick == 0 || processed == total {
                    let pct = (processed as f64 / total as f64) * 100.0;
                    println!(
                        "[initscan] block progress: {}/{} ({:.0}%) current={}",
                        processed, total, pct, n
                    );
                }
                n = n.saturating_add(1);
                continue;
            }
        };

        for tx in b.transactions {
            info!("processing transaction: {:?}", tx.to);
            if tx.to.is_none() {
                let receipt = match provider
                    .get_transaction_receipt(tx.hash)
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(
                            "get_transaction_receipt {:?} error: {}; skipping",
                            tx.hash, e
                        );
                        None
                    }
                };
                if let Some(r) = receipt {
                    if let Some(addr) = r.contract_address {
                        initscan
                            .try_init_for_contract(addr, Some(n))
                            .await;
                    }
                }
            }
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

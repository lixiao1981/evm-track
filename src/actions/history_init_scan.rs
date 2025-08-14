use std::sync::Arc;

use alloy_primitives::{Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::{Context, Result};

use super::initscan::{InitscanAction, InitscanOptions};

#[derive(Debug, Clone)]
pub struct HistoryInitScanOptions {
    pub from_block: u64,
    pub to_block: u64,
    pub initscan: InitscanOptions,
}

#[derive(serde::Deserialize)]
struct TxLite {
    hash: B256,
    #[serde(default)]
    to: Option<Address>,
}

#[derive(serde::Deserialize)]
struct BlockLite {
    #[allow(dead_code)]
    number: Option<String>,
    #[serde(default)]
    transactions: Vec<TxLite>,
}

/// Scan a historical block range for contract creations and attempt initialization using Initscan.
pub async fn run(
    provider: Arc<RootProvider<BoxTransport>>,
    opts: HistoryInitScanOptions,
) -> Result<()> {
    let initscan = InitscanAction::new(provider.clone(), opts.initscan.clone());
    let from = opts.from_block;
    let to = opts.to_block;
    let mut n = from;
    while n <= to {
        // Fetch block with full transactions via raw RPC
        let block_hex = format!("0x{:x}", n);
        let v: serde_json::Value = provider
            .client()
            .request("eth_getBlockByNumber", serde_json::json!([block_hex, true]))
            .await
            .with_context(|| format!("eth_getBlockByNumber {}", n))?;
        if v.is_null() {
            n = n.saturating_add(1);
            continue;
        }
        let b: BlockLite = serde_json::from_value(v).context("parse block")?;
        for tx in b.transactions {
            // CREATE if `to` is null
            if tx.to.is_none() {
                // get contract address from receipt
                let h = tx.hash;
                let receipt = provider.get_transaction_receipt(h).await.ok().flatten();
                if let Some(r) = receipt {
                    if let Some(addr) = r.contract_address {
                        // Try init this contract
                        initscan.try_init_for_contract(addr, Some(n)).await;
                    }
                }
            }
        }
        n = n.saturating_add(1);
    }
    Ok(())
}


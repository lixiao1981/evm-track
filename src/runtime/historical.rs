use alloy_primitives::{Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::Filter;
use alloy_transport::BoxTransport;
use crate::error::Result;
use serde::Deserialize;
use tracing::warn;

use crate::{
    abi,
    actions::{ActionSet, BlockRecord},
    cli::RangeFlags,
    runtime::public,
};
use alloy_rpc_types_eth::TransactionTrait;
use std::sync::Arc;
use crate::throttle;

pub async fn run_events(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    range: &RangeFlags,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    let events = abi::load_event_sigs_default().unwrap_or_default();
    let from = range.from_block;
    let to = range.to_block.unwrap_or_else(|| from);
    let step = range.step_blocks.max(1);
    let mut cur = from;
    while cur <= to {
        let end = cur.saturating_add(step - 1).min(to);
        let filter = Filter::new()
            .address(addrs.clone())
            .from_block(cur)
            .to_block(end);
        throttle::acquire().await;
        let logs = provider.get_logs(&filter).await?;
        for v in logs {
            let _er = public::process_log(&v, &events, &actions);
        }
        cur = end.saturating_add(1);
    }
    Ok(())
}

pub async fn run_blocks(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    range: &RangeFlags,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    let events = abi::load_event_sigs_default().unwrap_or_default();
    let funcs = abi::load_func_sigs_default().unwrap_or_default();
    let from = range.from_block;
    let to = range.to_block.unwrap_or_else(|| from);
    if addrs.is_empty() {
        #[derive(Deserialize)]
        struct BlockTxHashes { transactions: Vec<String> }
        let mut num = from;
        while num <= to {
            if let Some(a) = &actions { a.on_block(&BlockRecord { number: num }); }
            let hexnum = format!("0x{:x}", num);
            throttle::acquire().await;
            let v: serde_json::Value = match provider.client().request("eth_getBlockByNumber", serde_json::json!([hexnum, false])).await {
                Ok(v) => v,
                Err(e) => { warn!("eth_getBlockByNumber {} error: {}; skipping", num, e); num = num.saturating_add(1); continue }
            };
            if v.is_null() { num = num.saturating_add(1); continue; }
            let b: BlockTxHashes = match serde_json::from_value(v) {
                Ok(b) => b,
                Err(e) => { warn!("parse block {} error: {}; skipping", num, e); num = num.saturating_add(1); continue }
            };
            for hs in b.transactions {
                let txh: B256 = match hs.parse() { Ok(h) => h, Err(_) => { warn!("invalid tx hash {} at block {}", hs, num); continue } };
                throttle::acquire().await;
                let tx_opt = match provider.get_transaction_by_hash(txh).await { Ok(v) => v, Err(e) => { warn!("get_transaction_by_hash {:?} error: {}; skipping tx", txh, e); None } };
                if let Some(tx) = tx_opt {
                    let input = tx.input().as_ref();
                    let (fname, args, input_selector) = public::decode_transaction_function(input, &funcs);
                    throttle::acquire().await;
                    let receipt = provider.get_transaction_receipt(txh).await.ok().flatten();
                    
                    // 使用公共函数创建 TxRecord
                    let tr = public::create_tx_record_from_standard_tx(
                        &tx, 
                        txh, 
                        &receipt, 
                        fname, 
                        args, 
                        input_selector
                    );
                    
                    if let Some(a) = &actions { 
                        a.on_tx(&tr); 
                    }
                }
            }
            num = num.saturating_add(1);
        }
        return Ok(());
    }
    let mut num = from;
    while num <= to {
        if let Some(a) = &actions {
            a.on_block(&BlockRecord { number: num });
        }
        let filter = Filter::new()
            .address(addrs.clone())
            .from_block(num)
            .to_block(num);
        throttle::acquire().await;
        let logs = match provider.get_logs(&filter).await {
            Ok(v) => v,
            Err(e) => {
                warn!("get_logs error at block {}: {}; skipping", num, e);
                num = num.saturating_add(1);
                continue;
            }
        };
        for v in logs {
            let _er = public::process_log(&v, &events, &actions);
            if let Some(txh) = v.transaction_hash {
                throttle::acquire().await;
                let tx_opt = match provider.get_transaction_by_hash(txh).await {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("get_transaction_by_hash {:?} error: {}; skipping tx", txh, e);
                        None
                    }
                };
                if let Some(tx) = tx_opt {
                    let input = tx.input().as_ref();
                    let (fname, args, input_selector) = public::decode_transaction_function(input, &funcs);
                    throttle::acquire().await;
                    let receipt = provider.get_transaction_receipt(txh).await.ok().flatten();
                    
                    // 使用公共函数创建 TxRecord
                    let tr = public::create_tx_record_from_standard_tx(
                        &tx, 
                        txh, 
                        &receipt, 
                        fname, 
                        args, 
                        input_selector
                    );
                    
                    if let Some(a) = &actions {
                        a.on_tx(&tr);
                    }
                }
            }
        }
        num = num.saturating_add(1);
    }
    Ok(())
}

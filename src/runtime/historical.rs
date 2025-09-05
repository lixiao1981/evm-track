use alloy_primitives::{Address, B256, hex};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::Filter;
use alloy_transport::BoxTransport;
use crate::error::Result;
use serde::Deserialize;
use tracing::warn;

use crate::{
    abi,
    actions::{ActionSet, BlockRecord, EventRecord, TxRecord},
    cli::RangeFlags,
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
            let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
            let topic0_hex = format!("0x{}", hex::encode(topic0));
            let (name, fields) = if let Some((nm, fs)) =
                abi::try_decode_event(&topic0_hex, v.topics(), v.data().data.as_ref(), &events)
            {
                (Some(nm), fs)
            } else {
                (None, vec![])
            };
            let er = EventRecord {
                address: v.address(),
                tx_hash: v.transaction_hash,
                block_number: v.block_number,
                topic0: v.topic0().cloned(),
                name,
                fields,
                tx_index: v.transaction_index,
                log_index: v.log_index,
                topics: v.topics().to_vec(),
                removed: Some(v.removed),
            };
            if let Some(a) = &actions {
                a.on_event(&er);
            }
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
                    let sel_opt = if input.len() >= 4 { Some(input[0..4].try_into().ok()).flatten() } else { None };
                    let sel_hex = sel_opt.map(|s: [u8;4]| format!("0x{}", hex::encode(s)));
                    let (fname, args) = if let (Some(h), true) = (sel_hex.as_ref(), input.len() >= 4) {
                        if let Some((f, a)) = abi::try_decode_function(h, input, &funcs) { (Some(f), a) } else { (None, vec![]) }
                    } else { (None, vec![]) };
                    throttle::acquire().await;
                    let receipt = provider.get_transaction_receipt(txh).await.ok().flatten();
                    let (status, gas_used, cumulative_gas_used, effective_gas_price, block_number, tx_index, contract_address, receipt_logs) = if let Some(r) = &receipt {
                        let logs_vec = Some(r.inner.logs().iter().map(|l| crate::actions::SimpleLog { address: l.address(), topics: l.topics().to_vec(), data: l.data().data.as_ref().to_vec(), log_index: l.log_index, removed: None }).collect());
                        (Some(if r.status() { 1 } else { 0 }), Some(r.gas_used as u64), Some(r.inner.cumulative_gas_used() as u64), Some(alloy_primitives::U256::from(r.effective_gas_price)), r.block_number, r.transaction_index, r.contract_address, logs_vec)
                    } else { (None, None, None, None, None, None, None, None) };
                    let tr = TxRecord { hash: txh, from: Some(tx.from), to: match tx.kind() { alloy_primitives::TxKind::Call(a) => Some(a), _ => None }, input_selector: sel_opt, func_name: fname, func_args: args, gas: Some(tx.gas_limit()), gas_price: tx.gas_price().map(alloy_primitives::U256::from), effective_gas_price, status, gas_used, cumulative_gas_used, block_number, tx_index, contract_address, receipt_logs };
                    if let Some(a) = &actions { a.on_tx(&tr); }
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
            let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
            let topic0_hex = format!("0x{}", hex::encode(topic0));
            let (name, fields) = if let Some((nm, fs)) =
                abi::try_decode_event(&topic0_hex, v.topics(), v.data().data.as_ref(), &events)
            {
                (Some(nm), fs)
            } else {
                (None, vec![])
            };
            let er = EventRecord {
                address: v.address(),
                tx_hash: v.transaction_hash,
                block_number: v.block_number,
                topic0: v.topic0().cloned(),
                name,
                fields,
                tx_index: v.transaction_index,
                log_index: v.log_index,
                topics: v.topics().to_vec(),
                removed: Some(v.removed),
            };
            if let Some(a) = &actions {
                a.on_event(&er);
            }
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
                    if input.len() >= 4 {
                        let sel_hex = format!("0x{}", hex::encode(&input[0..4]));
                        let (fname, args) = if let Some((f, a)) =
                            abi::try_decode_function(&sel_hex, input, &funcs)
                        {
                            (Some(f), a)
                        } else {
                            (None, vec![])
                        };
                        throttle::acquire().await;
                        let receipt = provider.get_transaction_receipt(txh).await.ok().flatten();
                        let (
                            status,
                            gas_used,
                            cumulative_gas_used,
                            effective_gas_price,
                            block_number,
                            tx_index,
                            contract_address,
                            receipt_logs,
                        ) = if let Some(r) = &receipt {
                            let logs_vec = Some(
                                r.inner
                                    .logs()
                                    .iter()
                                    .map(|l| crate::actions::SimpleLog {
                                        address: l.address(),
                                        topics: l.topics().to_vec(),
                                        data: l.data().data.as_ref().to_vec(),
                                        log_index: l.log_index,
                                        removed: None,
                                    })
                                    .collect(),
                            );
                            (
                                Some(if r.status() { 1 } else { 0 }),
                                Some(r.gas_used as u64),
                                Some(r.inner.cumulative_gas_used() as u64),
                                Some(alloy_primitives::U256::from(r.effective_gas_price)),
                                r.block_number,
                                r.transaction_index,
                                r.contract_address,
                                logs_vec,
                            )
                        } else {
                            (None, None, None, None, None, None, None, None)
                        };
                        let tr = TxRecord {
                            hash: txh,
                            from: Some(tx.from),
                            to: match tx.kind() {
                                alloy_primitives::TxKind::Call(a) => Some(a),
                                _ => None,
                            },
                            input_selector: input[0..4].try_into().ok(),
                            func_name: fname,
                            func_args: args,
                            gas: Some(tx.gas_limit()),
                            gas_price: tx.gas_price().map(alloy_primitives::U256::from),
                            effective_gas_price,
                            status,
                            gas_used,
                            cumulative_gas_used,
                            block_number,
                            tx_index,
                            contract_address,
                            receipt_logs,
                        };
                        if let Some(a) = &actions {
                            a.on_tx(&tr);
                        }
                    }
                }
            }
        }
        num = num.saturating_add(1);
    }
    Ok(())
}

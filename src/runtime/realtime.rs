use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result};
use alloy_primitives::{hex, Address, B256};
use alloy_provider::RootProvider;
use alloy_rpc_types::Filter;
use alloy_transport_ws::WsClient;
use futures::StreamExt;
use tracing::{info, warn};

use crate::{abi, actions::{ActionSet, BlockRecord, EventRecord, TxRecord}};

pub async fn run_events(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    match run_events_subscribe(provider.clone(), addrs.clone(), actions.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn!("subscribe logs failed: {e}; fallback to polling");
            run_events_poll(provider, addrs, actions).await
        }
    }
}

async fn run_events_subscribe(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Subscribing to logs via eth_subscribe");
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let filter = Filter::new().address(addrs.clone());
    let mut last_seen: u64 = provider.get_block_number().await.unwrap_or(0);
    let mut backoff = 1u64; // seconds
    const MAX_BACKOFF: u64 = 30;
    const MAX_BACKFILL: u64 = 500;
    loop {
        let mut sub = provider.subscribe_logs(&filter).await?;
        while let Some(item) = sub.next().await {
            let v = item?;
            let topic0 = v.topics.get(0).cloned().unwrap_or(B256::ZERO);
            let topic0_hex = format!("0x{}", hex::encode(topic0));
            let (name, fields) = if let Some((n, f)) = abi::try_decode_event(&topic0_hex, &v.topics, v.data.as_ref(), &events) { (Some(n), f) } else { (None, vec![]) };
            let rec = EventRecord { address: v.address, tx_hash: v.transaction_hash, block_number: v.block_number, topic0: v.topics.get(0).cloned(), name, fields, tx_index: v.transaction_index, log_index: v.log_index, topics: v.topics.clone(), removed: v.removed };
            println!("[event] block={:?} addr={:?} tx={:?} name={:?}", rec.block_number, rec.address, rec.tx_hash, rec.name);
            if let Some(a) = &actions { a.on_event(&rec); }
            last_seen = rec.block_number.unwrap_or(last_seen);
        }
        warn!("log subscription ended; attempting backfill and resubscribe");
        let cur = provider.get_block_number().await.unwrap_or(last_seen);
        if cur > last_seen {
            let start = if cur - last_seen > MAX_BACKFILL { cur - MAX_BACKFILL + 1 } else { last_seen + 1 };
            let f = Filter::new().address(addrs.clone()).from_block(start).to_block(cur);
            if let Ok(logs) = provider.get_logs(&f).await { for _ in logs { /* backfill */ } }
            last_seen = cur;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_events_poll(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Polling for new logs via latest block");
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let mut last = provider.get_block_number().await.context("get_block_number")?;
    loop {
        let cur = provider.get_block_number().await.context("get_block_number")?;
        if cur > last {
            let filter = Filter::new().address(addrs.clone()).from_block(last + 1).to_block(cur);
            let logs = provider.get_logs(&filter).await.context("get_logs")?;
            for v in logs {
                let topic0 = v.topics.get(0).cloned().unwrap_or(B256::ZERO);
                let topic0_hex = format!("0x{}", hex::encode(topic0));
                let (name, fields) = if let Some((n, f)) = abi::try_decode_event(&topic0_hex, &v.topics, v.data.as_ref(), &events) { (Some(n), f) } else { (None, vec![]) };
                let rec = EventRecord { address: v.address, tx_hash: v.transaction_hash, block_number: v.block_number, topic0: v.topics.get(0).cloned(), name, fields, tx_index: v.transaction_index, log_index: v.log_index, topics: v.topics.clone(), removed: v.removed };
                if let Some(a) = &actions { a.on_event(&rec); }
            }
            last = cur;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn run_blocks(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    match run_blocks_subscribe(provider.clone(), addrs.clone(), actions.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => {
            warn!("subscribe newHeads failed: {e}; fallback to polling");
            run_blocks_poll(provider, addrs, actions).await
        }
    }
}

async fn run_blocks_subscribe(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Subscribing to new heads via eth_subscribe");
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let funcs = abi::load_func_sigs("./data/func_sigs.json").unwrap_or_default();
    let mut last_seen = provider.get_block_number().await.unwrap_or(0);
    let mut backoff = 1u64; // seconds
    const MAX_BACKOFF: u64 = 30;
    const MAX_BACKFILL: u64 = 500;
    loop {
        let mut sub = provider.subscribe_new_heads().await?;
        while let Some(item) = sub.next().await {
            let header = item?;
            let n = header.number.unwrap_or_default();
            println!("block: number={}", n);
            let br = BlockRecord { number: n };
            if let Some(a) = &actions { a.on_block(&br); }
            let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
            let logs = provider.get_logs(&filter).await.context("get_logs")?;
            for v in logs {
                let topic0 = v.topics.get(0).cloned().unwrap_or(B256::ZERO);
                let topic0_hex = format!("0x{}", hex::encode(topic0));
                let (name, fields) = if let Some((nm, fs)) = abi::try_decode_event(&topic0_hex, &v.topics, v.data.as_ref(), &events) { (Some(nm), fs) } else { (None, vec![]) };
                let er = EventRecord { address: v.address, tx_hash: v.transaction_hash, block_number: v.block_number, topic0: v.topics.get(0).cloned(), name, fields, tx_index: v.transaction_index, log_index: v.log_index, topics: v.topics.clone(), removed: v.removed };
                if let Some(a) = &actions { a.on_event(&er); }

                if let Some(txh) = v.transaction_hash {
                    if let Some(tx) = provider.get_transaction_by_hash(txh).await.context("get_tx_by_hash")? {
                        let input = tx.input.as_ref();
                        if input.len() >= 4 {
                            let sel = &input[0..4];
                            let sel_hex = format!("0x{}", hex::encode(sel));
                            let (fname, args) = if let Some((f, a)) = abi::try_decode_function(&sel_hex, input, &funcs) { (Some(f), a) } else { (None, vec![]) };
                            // fetch receipt for gas/fee info
                            let receipt = provider.get_transaction_receipt(txh).await.ok().flatten();
                            let (status, gas_used, cumulative_gas_used, effective_gas_price, block_number, tx_index, contract_address, receipt_logs) = if let Some(r) = &receipt {
                                (
                                    r.status.map(|s| s.as_u64()),
                                    r.gas_used.map(|g| g.as_u64()),
                                    r.cumulative_gas_used.map(|g| g.as_u64()),
                                    r.effective_gas_price,
                                    r.block_number.map(|n| n.as_u64()),
                                    r.transaction_index.map(|i| i.as_u64()),
                                    r.contract_address,
                                    Some(r.logs.iter().map(|lg| crate::actions::SimpleLog {
                                        address: lg.address,
                                        topics: lg.topics.clone(),
                                        data: lg.data.to_vec(),
                                        log_index: lg.log_index,
                                        removed: lg.removed,
                                    }).collect()),
                                )
                            } else { (None, None, None, None, None, None, None, None) };
                            let tr = TxRecord {
                                hash: txh,
                                from: tx.from,
                                to: tx.to,
                                input_selector: sel.try_into().ok(),
                                func_name: fname,
                                func_args: args,
                                gas: tx.gas.map(|g| g.as_u64()),
                                gas_price: tx.gas_price,
                                effective_gas_price,
                                status,
                                gas_used,
                                cumulative_gas_used,
                                block_number,
                                tx_index,
                                contract_address,
                                receipt_logs,
                            };
                            if let Some(a) = &actions { a.on_tx(&tr); }
                        }
                    }
                }
            }
            last_seen = n;
        }
        warn!("newHeads subscription ended; attempting backfill and resubscribe");
        let cur = provider.get_block_number().await.unwrap_or(last_seen);
        if cur > last_seen {
            let start = if cur - last_seen > MAX_BACKFILL { cur - MAX_BACKFILL + 1 } else { last_seen + 1 };
            for n in start..=cur {
                let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
                let _ = provider.get_logs(&filter).await;
            }
            last_seen = cur;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_blocks_poll(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Polling new heads");
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let mut last = provider.get_block_number().await.context("get_block_number")?;
    loop {
        let cur = provider.get_block_number().await.context("get_block_number")?;
        if cur > last {
            for n in (last + 1)..=cur {
                println!("block: number={}", n);
                if let Some(a) = &actions { a.on_block(&BlockRecord { number: n }); }
                let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
                let logs = provider.get_logs(&filter).await.context("get_logs")?;
                for v in logs {
                    let topic0 = v.topics.get(0).cloned().unwrap_or(B256::ZERO);
                    let topic0_hex = format!("0x{}", hex::encode(topic0));
                    let (name, fields) = if let Some((nm, fs)) = abi::try_decode_event(&topic0_hex, &v.topics, v.data.as_ref(), &events) { (Some(nm), fs) } else { (None, vec![]) };
                    let er = EventRecord { address: v.address, tx_hash: v.transaction_hash, block_number: v.block_number, topic0: v.topics.get(0).cloned(), name, fields, tx_index: v.transaction_index, log_index: v.log_index, topics: v.topics.clone(), removed: v.removed };
                    if let Some(a) = &actions { a.on_event(&er); }
                }
            }
            last = cur;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}


use crate::throttle;
use crate::error::Result;
use crate::{
    abi,
    actions::{ActionSet, BlockRecord, EventRecord, TxRecord},
};
use alloy_network_primitives::TransactionResponse;
use alloy_primitives::{hex, Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::Filter;
use alloy_rpc_types_eth::TransactionTrait;
use alloy_transport::BoxTransport;
use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use tracing::{info, warn};

pub async fn run_events(
    provider: RootProvider<BoxTransport>,
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
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Subscribing to logs via eth_subscribe");
    let events = abi::load_event_sigs_default().unwrap_or_default();
    let filter = Filter::new().address(addrs.clone());
    throttle::acquire().await;
    let mut last_seen: u64 = provider.get_block_number().await?;
    let mut backoff = 1u64; // seconds
    const MAX_BACKOFF: u64 = 30;
    const MAX_BACKFILL: u64 = 500;
    loop {
        throttle::acquire().await;
        let sub = provider.subscribe_logs(&filter).await?;
        let mut stream = sub.into_stream();
        while let Some(v) = stream.next().await {
            let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
            let topic0_hex = format!("0x{}", hex::encode(topic0));
            let (name, fields) = if let Some((n, f)) =
                abi::try_decode_event(&topic0_hex, v.topics(), v.data().data.as_ref(), &events)
            {
                (Some(n), f)
            } else {
                (None, vec![])
            };
            let rec = EventRecord {
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
                a.on_event(&rec);
            }
            last_seen = rec.block_number.unwrap_or(last_seen);
        }
        warn!("log subscription ended; attempting backfill and resubscribe");
        throttle::acquire().await;
        let cur = provider.get_block_number().await?;
        if cur > last_seen {
            let start = if cur - last_seen > MAX_BACKFILL {
                cur - MAX_BACKFILL + 1
            } else {
                last_seen + 1
            };
            let f = Filter::new().address(addrs.clone()).from_block(start).to_block(cur);
            throttle::acquire().await;
            if let Ok(logs) = provider.get_logs(&f).await {
                for _ in logs {
                    /* backfill */
                }
            }
            last_seen = cur;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_events_poll(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Polling for new logs via latest block");
    let events = abi::load_event_sigs_default().unwrap_or_default();
    throttle::acquire().await;
    let mut last = provider.get_block_number().await?;
    loop {
        throttle::acquire().await;
        let cur = provider.get_block_number().await?;
        if cur > last {
            let filter = Filter::new().address(addrs.clone()).from_block(last + 1).to_block(cur);
            throttle::acquire().await;
            let logs = provider.get_logs(&filter).await?;
            for v in logs {
                let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
                let topic0_hex = format!("0x{}", hex::encode(topic0));
                let (name, fields) = if let Some((n, f)) =
                    abi::try_decode_event(&topic0_hex, v.topics(), v.data().data.as_ref(), &events)
                {
                    (Some(n), f)
                } else {
                    (None, vec![])
                };
                let rec = EventRecord {
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
                    a.on_event(&rec);
                }
            }
            last = cur;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn run_blocks(
    provider: RootProvider<BoxTransport>,
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

pub async fn run_pending_transactions(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
    hashes_only: bool,
) -> Result<()> {
    let funcs = abi::load_func_sigs("./data/func_sigs.json").unwrap_or_default();
    // Try full pending tx subscription first unless hashes_only
    if !hashes_only {
        throttle::acquire().await;
        if let Ok(sub) = provider.subscribe_full_pending_transactions().await {
            let mut stream = sub.into_stream();
            while let Some(tx) = stream.next().await {
                let to_addr = match tx.kind() {
                    alloy_primitives::TxKind::Call(a) => Some(a),
                    _ => None,
                };
                if !addrs.is_empty() {
                    if let Some(to) = to_addr {
                        if !addrs.contains(&to) {
                            continue;
                        }
                    }
                }
                let input = tx.input().as_ref();
                let (fname, args) = if input.len() >= 4 {
                    let sel_hex = format!("0x{}", hex::encode(&input[0..4]));
                    if let Some((f, a)) = abi::try_decode_function(&sel_hex, input, &funcs) {
                        (Some(f), a)
                    } else {
                        (None, vec![])
                    }
                } else {
                    (None, vec![])
                };
                let tr = TxRecord {
                    hash: tx.tx_hash(),
                    from: Some(tx.from()),
                    to: to_addr,
                    input_selector: if input.len() >= 4 {
                        input[0..4].try_into().ok()
                    } else {
                        None
                    },
                    func_name: fname,
                    func_args: args,
                    gas: Some(tx.gas_limit()),
                    gas_price: alloy_rpc_types_eth::TransactionTrait::gas_price(&tx)
                        .map(alloy_primitives::U256::from),
                    effective_gas_price: None,
                    status: None,
                    gas_used: None,
                    cumulative_gas_used: None,
                    block_number: None,
                    tx_index: None,
                    contract_address: None,
                    receipt_logs: None,
                };
                if let Some(a) = &actions {
                    a.on_tx(&tr);
                }
            }
            return Ok(());
        }
    }
    // Fallback to hashes
    throttle::acquire().await;
    let sub = provider.subscribe_pending_transactions().await?;
    let mut stream = sub.into_stream();
    while let Some(h) = stream.next().await {
        throttle::acquire().await;
        if let Some(tx) = provider.get_transaction_by_hash(h).await? {
            let to_addr = match tx.kind() {
                alloy_primitives::TxKind::Call(a) => Some(a),
                _ => None,
            };
            if !addrs.is_empty() {
                if let Some(to) = to_addr {
                    if !addrs.contains(&to) {
                        continue;
                    }
                }
            }
            let input = tx.input().as_ref();
            let (fname, args) = if input.len() >= 4 {
                let sel_hex = format!("0x{}", hex::encode(&input[0..4]));
                if let Some((f, a)) = abi::try_decode_function(&sel_hex, input, &funcs) {
                    (Some(f), a)
                } else {
                    (None, vec![])
                }
            } else {
                (None, vec![])
            };
            let tr = TxRecord {
                hash: h,
                from: Some(tx.from),
                to: to_addr,
                input_selector: if input.len() >= 4 {
                    input[0..4].try_into().ok()
                } else {
                    None
                },
                func_name: fname,
                func_args: args,
                gas: Some(tx.gas_limit()),
                gas_price: alloy_rpc_types_eth::TransactionTrait::gas_price(&tx)
                    .map(alloy_primitives::U256::from),
                effective_gas_price: None,
                status: None,
                gas_used: None,
                cumulative_gas_used: None,
                block_number: None,
                tx_index: None,
                contract_address: None,
                receipt_logs: None,
            };
            if let Some(a) = &actions {
                a.on_tx(&tr);
            }
        }
    }
    Ok(())
}

async fn run_blocks_subscribe(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Subscribing to new heads via eth_subscribe");
    let events = abi::load_event_sigs_default().unwrap_or_default();
    let funcs = abi::load_func_sigs_default().unwrap_or_default();
    throttle::acquire().await;
    let mut last_seen = provider.get_block_number().await?;
    let mut backoff = 1u64; // seconds
    const MAX_BACKOFF: u64 = 30;
    const MAX_BACKFILL: u64 = 500;
    loop {
        throttle::acquire().await;
        let sub = provider.subscribe_blocks().await?;
        let mut stream = sub.into_stream();
        while let Some(header) = stream.next().await {
            let n = header.number;
            println!("block: number={}", n);
            let br = BlockRecord { number: n };
            if let Some(a) = &actions {
                a.on_block(&br);
            }
            let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
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

                if let Some(txh) = v.transaction_hash {
                    throttle::acquire().await;
                    if let Some(tx) = provider.get_transaction_by_hash(txh).await? {
                        let input = tx.input().as_ref();
                        if input.len() >= 4 {
                            let sel = &input[0..4];
                            let sel_hex = format!("0x{}", hex::encode(sel));
                            let (fname, args) = if let Some((f, a)) =
                                abi::try_decode_function(&sel_hex, input, &funcs)
                            {
                                (Some(f), a)
                            } else {
                                (None, vec![])
                            };
                            // fetch receipt for gas/fee info
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
                                input_selector: sel.try_into().ok(),
                                func_name: fname,
                                func_args: args,
                                gas: Some(tx.gas_limit()),
                                gas_price: alloy_rpc_types_eth::TransactionTrait::gas_price(&tx)
                                    .map(alloy_primitives::U256::from),
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
            last_seen = n;
        }
        warn!("newHeads subscription ended; attempting backfill and resubscribe");
        throttle::acquire().await;
        let cur = provider.get_block_number().await?;
        if cur > last_seen {
            let start = if cur - last_seen > MAX_BACKFILL {
                cur - MAX_BACKFILL + 1
            } else {
                last_seen + 1
            };
            for n in start..=cur {
                let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
                throttle::acquire().await;
                let _ = provider.get_logs(&filter).await;
            }
            last_seen = cur;
        }
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_blocks_poll(
    provider: RootProvider<BoxTransport>,
    addrs: Vec<Address>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Polling new heads");
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    throttle::acquire().await;
    let mut last = provider.get_block_number().await?;
    loop {
        throttle::acquire().await;
        let cur = provider.get_block_number().await?;
        if cur > last {
            for n in (last + 1)..=cur {
                println!("block: number={}", n);
                if let Some(a) = &actions {
                    a.on_block(&BlockRecord { number: n });
                }
                let filter = Filter::new().address(addrs.clone()).from_block(n).to_block(n);
                throttle::acquire().await;
                let logs = provider.get_logs(&filter).await?;
                for v in logs {
                    let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
                    let topic0_hex = format!("0x{}", hex::encode(topic0));
                    let (name, fields) = if let Some((nm, fs)) = abi::try_decode_event(
                        &topic0_hex,
                        v.topics(),
                        v.data().data.as_ref(),
                        &events,
                    ) {
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
            }
            last = cur;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

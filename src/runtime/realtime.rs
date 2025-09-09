
use crate::throttle;
use crate::error::Result;
use crate::{
    abi,
    actions::{ActionSet, BlockRecord, TxRecord},
};
use super::{cache, public};
use alloy_network_primitives::TransactionResponse;
use alloy_primitives::Address;
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
            let rec = public::process_log(&v, &events, &actions);
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
                public::process_log(&v, &events, &actions);
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

pub async fn run_contract_deployments(
    provider: RootProvider<BoxTransport>,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    info!("Starting contract deployment monitoring...");
    let mut last_seen = provider.get_block_number().await?;
    let mut backoff = 1u64;
    const MAX_BACKOFF: u64 = 30;
    
    loop {
        match run_deployments_subscribe(provider.clone(), actions.clone(), last_seen).await {
            Ok(new_last_seen) => {
                last_seen = new_last_seen;
                backoff = 1; // 重置退避
            }
            Err(e) => {
                warn!("deployment subscription failed: {e}; fallback to polling");
                last_seen = run_deployments_poll(provider.clone(), actions.clone(), last_seen).await?;
                backoff = 1;
            }
        }
        
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

async fn run_deployments_subscribe(
    provider: RootProvider<BoxTransport>,
    actions: Option<Arc<ActionSet>>,
    mut last_seen: u64,
) -> Result<u64> {
    let sub = provider.subscribe_blocks().await?;
    let mut stream = sub.into_stream();
    
    while let Some(header) = stream.next().await {
        let n = header.number;
        
        // 使用统一的缓存处理函数
        if let Err(e) = cache::process_block_unified(
            &provider,
            n,
            &[], // 不需要监控特定地址的事件
            &actions,
            false, // process_events
            true,  // process_deployments
        ).await {
            warn!("Error processing deployments for block {}: {}", n, e);
        }
        
        last_seen = n;
    }
    
    Ok(last_seen)
}

async fn run_deployments_poll(
    provider: RootProvider<BoxTransport>,
    actions: Option<Arc<ActionSet>>,
    mut last_seen: u64,
) -> Result<u64> {
    loop {
        throttle::acquire().await;
        let cur = provider.get_block_number().await?;
        
        if cur > last_seen {
            for n in (last_seen + 1)..=cur {
                // 使用统一的缓存处理函数
                if let Err(e) = cache::process_block_unified(
                    &provider,
                    n,
                    &[], // 不需要监控特定地址的事件
                    &actions,
                    false, // process_events
                    true,  // process_deployments
                ).await {
                    warn!("Error processing deployments for block {}: {}", n, e);
                }
            }
            last_seen = cur;
        }
        
        tokio::time::sleep(Duration::from_secs(2)).await;
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
                let (fname, args, input_selector) = public::decode_transaction_function(input, &funcs);
                let tr = TxRecord {
                    hash: tx.tx_hash(),
                    from: Some(tx.from()),
                    to: to_addr,
                    input_selector,
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
            let (fname, args, input_selector) = public::decode_transaction_function(input, &funcs);
            let tr = TxRecord {
                hash: h,
                from: Some(tx.from),
                to: to_addr,
                input_selector,
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
            // 使用统一的缓存处理函数
            if let Err(e) = cache::process_block_unified(
                &provider,
                n,
                &addrs,
                &actions,
                true,  // process_events
                false, // process_deployments (在这个函数中不处理合约创建)
            ).await {
                warn!("Error processing block {}: {}", n, e);
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
                    public::process_log(&v, &events, &actions);
                }
            }
            last = cur;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

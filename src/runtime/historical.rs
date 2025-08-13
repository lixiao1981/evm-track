use anyhow::{Context, Result};
use alloy_primitives::{hex, Address, B256};
use alloy_provider::RootProvider;
use alloy_rpc_types::Filter;
use alloy_transport_ws::WsClient;

use std::sync::Arc;
use crate::{abi, cli::RangeFlags, actions::{ActionSet, EventRecord, TxRecord, BlockRecord}};

pub async fn run_events(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    range: &RangeFlags,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let from = range.from_block;
    let to = range.to_block.unwrap_or_else(|| from);
    let step = range.step_blocks.max(1);
    let mut cur = from;
    while cur <= to {
        let end = cur.saturating_add(step - 1).min(to);
        let filter = Filter::new().address(addrs.clone()).from_block(cur).to_block(end);
        let logs = provider
            .get_logs(&filter)
            .await
            .with_context(|| format!("get_logs {}-{}", cur, end))?;
        for v in logs {
            let topic0 = v.topics.get(0).cloned().unwrap_or(B256::ZERO);
            let topic0_hex = format!("0x{}", hex::encode(topic0));
            let (name, fields) = if let Some((nm, fs)) = abi::try_decode_event(&topic0_hex, &v.topics, v.data.as_ref(), &events) { (Some(nm), fs) } else { (None, vec![]) };
            let er = EventRecord { address: v.address, tx_hash: v.transaction_hash, block_number: v.block_number, topic0: v.topics.get(0).cloned(), name, fields, tx_index: v.transaction_index, log_index: v.log_index, topics: v.topics.clone(), removed: v.removed };
            if let Some(a) = &actions { a.on_event(&er); }
        }
        cur = end.saturating_add(1);
    }
    Ok(())
}

pub async fn run_blocks(
    provider: RootProvider<WsClient>,
    addrs: Vec<Address>,
    range: &RangeFlags,
    actions: Option<Arc<ActionSet>>,
) -> Result<()> {
    let events = abi::load_event_sigs("./data/event_sigs.json").unwrap_or_default();
    let funcs = abi::load_func_sigs("./data/func_sigs.json").unwrap_or_default();
    let from = range.from_block;
    let to = range.to_block.unwrap_or_else(|| from);
    let mut num = from;
    while num <= to {
        if let Some(a) = &actions { a.on_block(&BlockRecord { number: num }); }
        let filter = Filter::new().address(addrs.clone()).from_block(num).to_block(num);
        let logs = provider.get_logs(&filter).await?;
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
                        let sel_hex = format!("0x{}", hex::encode(&input[0..4]));
                        let (fname, args) = if let Some((f, a)) = abi::try_decode_function(&sel_hex, input, &funcs) { (Some(f), a) } else { (None, vec![]) };
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
                            input_selector: input[0..4].try_into().ok(),
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
        num = num.saturating_add(1);
    }
    Ok(())
}

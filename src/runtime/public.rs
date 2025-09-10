use crate::{
    abi,
    actions::{ActionSet, EventRecord, TxRecord},
};
use alloy_primitives::{hex, B256, U256, Address};
use alloy_rpc_types_eth::{Transaction, TransactionReceipt, TransactionTrait};
use alloy_provider::{RootProvider, Provider};
use alloy_transport::BoxTransport;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use futures::future::join_all;
use tracing::{info, warn};

/// 公共事件解码函数
pub fn decode_log_event(
    log: &alloy_rpc_types_eth::Log,
    events: &abi::EventSigMap,
) -> (Option<String>, Vec<crate::abi::DecodedField>) {
    let topic0 = log.topic0().cloned().unwrap_or(B256::ZERO);
    let topic0_hex = format!("0x{}", hex::encode(topic0));
    if let Some((name, fields)) = abi::try_decode_event(&topic0_hex, log.topics(), log.data().data.as_ref(), events) {
        (Some(name), fields)
    } else {
        (None, vec![])
    }
}

/// 公共事件记录创建函数
pub fn create_event_record(
    log: &alloy_rpc_types_eth::Log,
    name: Option<String>,
    fields: Vec<crate::abi::DecodedField>,
) -> EventRecord {
    EventRecord {
        address: log.address(),
        tx_hash: log.transaction_hash,
        block_number: log.block_number,
        topic0: log.topic0().cloned(),
        name,
        fields,
        tx_index: log.transaction_index,
        log_index: log.log_index,
        topics: log.topics().to_vec(),
        removed: Some(log.removed),
    }
}

/// 公共处理日志函数
pub fn process_log(
    log: &alloy_rpc_types_eth::Log,
    events: &abi::EventSigMap,
    actions: &Option<Arc<ActionSet>>,
) -> EventRecord {
    let (name, fields) = decode_log_event(log, events);
    let rec = create_event_record(log, name, fields);
    
    if let Some(a) = actions {
        a.on_event(&rec);
    }
    
    rec
}

/// 公共交易解码函数
pub fn decode_transaction_function(
    input: &[u8],
    funcs: &abi::FuncSigMap,
) -> (Option<String>, Vec<crate::abi::DecodedValue>, Option<[u8; 4]>) {
    if input.len() >= 4 {
        let sel = &input[0..4];
        let sel_hex = format!("0x{}", hex::encode(sel));
        let (fname, args) = if let Some((f, a)) = abi::try_decode_function(&sel_hex, input, funcs) {
            (Some(f), a)
        } else {
            (None, vec![])
        };
        let selector = sel.try_into().ok();
        (fname, args, selector)
    } else {
        (None, vec![], None)
    }
}

/// 处理交易 Receipt 的公共函数
pub fn process_transaction_receipt(
    receipt: &Option<TransactionReceipt>,
) -> (
    Option<u64>,                             // status
    Option<u64>,                             // gas_used  
    Option<u64>,                             // cumulative_gas_used
    Option<U256>,                            // effective_gas_price
    Option<u64>,                             // block_number
    Option<u64>,                             // tx_index
    Option<Address>,                         // contract_address
    Option<Vec<crate::actions::SimpleLog>>,  // receipt_logs
) {
    if let Some(r) = receipt {
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
            Some(if r.status() { 1u64 } else { 0u64 }),
            Some(r.gas_used as u64),
            Some(r.inner.cumulative_gas_used() as u64),
            Some(U256::from(r.effective_gas_price)),
            r.block_number,
            r.transaction_index,
            r.contract_address,
            logs_vec,
        )
    } else {
        (None, None, None, None, None, None, None, None)
    }
}

/// 从标准 Transaction 创建 TxRecord 的公共函数
pub fn create_tx_record_from_standard_tx(
    tx: &Transaction,
    tx_hash: B256,
    receipt: &Option<TransactionReceipt>,
    func_name: Option<String>,
    func_args: Vec<crate::abi::DecodedValue>,
    input_selector: Option<[u8; 4]>,
) -> TxRecord {
    let (
        status,
        gas_used,
        cumulative_gas_used,
        effective_gas_price,
        block_number,
        tx_index,
        contract_address,
        receipt_logs,
    ) = process_transaction_receipt(receipt);
    
    TxRecord {
        hash: tx_hash,
        from: Some(tx.from),
        to: match tx.kind() {
            alloy_primitives::TxKind::Call(a) => Some(a),
            _ => None,
        },
        input_selector,
        func_name,
        func_args,
        gas: Some(tx.gas_limit()),
        gas_price: tx.gas_price().map(U256::from),
        effective_gas_price,
        status,
        gas_used,
        cumulative_gas_used,
        block_number,
        tx_index,
        contract_address,
        receipt_logs,
    }
}

/// 批量处理日志和交易的优化函数
pub async fn process_logs_batch(
    logs: Vec<alloy_rpc_types_eth::Log>,
    provider: &RootProvider<BoxTransport>,
    events: &abi::EventSigMap,
    funcs: &abi::FuncSigMap,
    actions: &Option<Arc<ActionSet>>,
) -> crate::error::Result<()> {
    if logs.is_empty() {
        return Ok(());
    }

    info!("Processing {} logs in batch mode", logs.len());
    
    // 第一步：批量处理所有事件（无网络调用）
    for log in &logs {
        let _er = process_log(log, events, actions);
    }
    
    // 第二步：收集所有需要的交易哈希（去重）
    let mut unique_tx_hashes: HashSet<B256> = HashSet::new();
    for log in &logs {
        if let Some(tx_hash) = log.transaction_hash {
            unique_tx_hashes.insert(tx_hash);
        }
    }
    
    if unique_tx_hashes.is_empty() {
        info!("No transactions to process");
        return Ok(());
    }
    
    info!("Found {} unique transactions to process", unique_tx_hashes.len());
    
    // 第三步：批量并发获取交易数据
    let tx_futures: Vec<_> = unique_tx_hashes.iter().map(|&tx_hash| {
        async move {
            crate::throttle::acquire().await;
            let tx_result = provider.get_transaction_by_hash(tx_hash).await;
            (tx_hash, tx_result)
        }
    }).collect();
    
    let tx_results = join_all(tx_futures).await;
    
    // 第四步：构建交易缓存
    let mut tx_cache: HashMap<B256, Transaction> = HashMap::new();
    for (tx_hash, tx_result) in tx_results {
        match tx_result {
            Ok(Some(tx)) => {
                tx_cache.insert(tx_hash, tx);
            }
            Ok(None) => {
                warn!("Transaction {:?} not found", tx_hash);
            }
            Err(e) => {
                warn!("Error fetching transaction {:?}: {}", tx_hash, e);
            }
        }
    }
    
    info!("Successfully cached {} transactions", tx_cache.len());
    
    // 第五步：批量并发获取收据数据
    let receipt_futures: Vec<_> = tx_cache.keys().map(|&tx_hash| {
        async move {
            crate::throttle::acquire().await;
            let receipt_result = provider.get_transaction_receipt(tx_hash).await;
            (tx_hash, receipt_result)
        }
    }).collect();
    
    let receipt_results = join_all(receipt_futures).await;
    
    // 第六步：构建收据缓存
    let mut receipt_cache: HashMap<B256, TransactionReceipt> = HashMap::new();
    for (tx_hash, receipt_result) in receipt_results {
        match receipt_result {
            Ok(Some(receipt)) => {
                receipt_cache.insert(tx_hash, receipt);
            }
            Ok(None) => {
                warn!("Transaction receipt {:?} not found", tx_hash);
            }
            Err(e) => {
                warn!("Error fetching receipt {:?}: {}", tx_hash, e);
            }
        }
    }
    
    info!("Successfully cached {} receipts", receipt_cache.len());
    
    // 第七步：批量处理交易（使用缓存数据）
    let mut processed_count = 0;
    for log in logs {
        if let Some(tx_hash) = log.transaction_hash {
            if let Some(tx) = tx_cache.get(&tx_hash) {
                let input = tx.input().as_ref();
                let (fname, args, input_selector) = decode_transaction_function(input, funcs);
                let receipt = receipt_cache.get(&tx_hash);
                
                let tr = create_tx_record_from_standard_tx(
                    tx, 
                    tx_hash, 
                    &receipt.cloned(), 
                    fname, 
                    args, 
                    input_selector
                );
                
                if let Some(a) = actions {
                    a.on_tx(&tr);
                }
                processed_count += 1;
            }
        }
    }
    
    info!("Successfully processed {} transactions in batch", processed_count);
    Ok(())
}

/// 按区块智能分组批处理日志
pub async fn process_logs_by_blocks(
    logs: Vec<alloy_rpc_types_eth::Log>,
    provider: &RootProvider<BoxTransport>,
    events: &abi::EventSigMap,
    funcs: &abi::FuncSigMap,
    actions: &Option<Arc<ActionSet>>,
) -> crate::error::Result<()> {
    if logs.is_empty() {
        return Ok(());
    }

    // 按区块号分组日志
    let mut logs_by_block: HashMap<u64, Vec<alloy_rpc_types_eth::Log>> = HashMap::new();
    
    for log in logs {
        if let Some(block_num) = log.block_number {
            logs_by_block.entry(block_num).or_default().push(log);
        }
    }
    
    info!("Processing {} blocks with grouped logs", logs_by_block.len());
    
    // 为每个区块并发处理
    let block_futures: Vec<_> = logs_by_block.into_iter().map(|(block_num, block_logs)| {
        async move {
            info!("Processing {} logs from block {}", block_logs.len(), block_num);
            process_logs_batch(block_logs, provider, events, funcs, actions).await
        }
    }).collect();
    
    let results = join_all(block_futures).await;
    
    // 检查是否有错误
    let mut error_count = 0;
    for result in results {
        if let Err(e) = result {
            warn!("Block batch processing error: {}", e);
            error_count += 1;
        }
    }
    
    if error_count > 0 {
        warn!("Encountered {} errors during batch processing", error_count);
    } else {
        info!("All blocks processed successfully");
    }
    
    Ok(())
}



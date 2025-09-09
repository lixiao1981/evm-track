use crate::{
    abi,
    actions::{ActionSet, BlockRecord, EventRecord, TxRecord, ContractCreationRecord},
    throttle,
    error::Result,
};
use alloy_primitives::{hex, Address, B256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::{Filter, BlockTransactionsKind, TransactionTrait};
use alloy_transport::BoxTransport;
use alloy_network_primitives::TransactionResponse;
use std::{sync::Arc, collections::{HashMap, HashSet}};

/// 缓存的交易数据，避免重复获取 receipt
#[derive(Clone, Debug)]
pub struct CachedTxData {
    pub transaction: alloy_rpc_types_eth::Transaction,
    pub receipt: Option<alloy_rpc_types_eth::TransactionReceipt>,
}

/// 交易缓存管理器
pub struct TxCache {
    cache: HashMap<B256, CachedTxData>,
}

impl TxCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// 批量获取交易和收据数据，避免重复调用
    pub async fn fetch_transactions(
        &mut self,
        provider: &RootProvider<BoxTransport>,
        tx_hashes: &HashSet<B256>,
    ) -> Result<()> {
        for &tx_hash in tx_hashes {
            if !self.cache.contains_key(&tx_hash) {
                throttle::acquire().await;
                if let Some(tx) = provider.get_transaction_by_hash(tx_hash).await? {
                    throttle::acquire().await;
                    let receipt = provider.get_transaction_receipt(tx_hash).await.ok().flatten();
                    
                    self.cache.insert(tx_hash, CachedTxData {
                        transaction: tx,
                        receipt,
                    });
                }
            }
        }
        Ok(())
    }

    /// 获取缓存的交易数据
    pub fn get(&self, tx_hash: &B256) -> Option<&CachedTxData> {
        self.cache.get(tx_hash)
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// 统一的区块处理函数，避免重复获取 transaction_receipt
pub async fn process_block_unified(
    provider: &RootProvider<BoxTransport>,
    block_number: u64,
    addrs: &[Address],
    actions: &Option<Arc<ActionSet>>,
    process_events: bool,
    process_deployments: bool,
) -> Result<()> {
    let events = abi::load_event_sigs_default().unwrap_or_default();
    let funcs = abi::load_func_sigs_default().unwrap_or_default();

    // 1. 收集所有需要的交易哈希
    let mut required_tx_hashes = HashSet::new();
    let mut logs = Vec::new();
    let mut block = None;
    
    // 从事件日志收集交易哈希
    if process_events && !addrs.is_empty() {
        let filter = Filter::new()
            .address(addrs.to_vec())
            .from_block(block_number)
            .to_block(block_number);
        throttle::acquire().await;
        logs = provider.get_logs(&filter).await?;
        for log in &logs {
            if let Some(tx_hash) = log.transaction_hash {
                required_tx_hashes.insert(tx_hash);
            }
        }
    }
    
    // 从区块交易收集合约创建交易哈希
    if process_deployments {
        throttle::acquire().await;
        block = provider.get_block_by_number(block_number.into(), BlockTransactionsKind::Full).await?;
        if let Some(ref block_data) = block {
            if let Some(transactions) = block_data.transactions.as_transactions() {
                for tx in transactions {
                    if tx.to().is_none() {  // 合约创建交易
                        required_tx_hashes.insert(tx.tx_hash());
                    }
                }
            }
        }
    }
    
    // 2. 批量获取所有需要的交易和收据（避免重复）
    let mut tx_cache = TxCache::new();
    tx_cache.fetch_transactions(provider, &required_tx_hashes).await?;
    
    // 3. 处理区块记录
    println!("block: number={}", block_number);
    let br = BlockRecord { number: block_number };
    if let Some(a) = actions {
        a.on_block(&br);
    }
    
    // 4. 处理事件（使用缓存的数据）
    if process_events {
        process_events_with_cache(&logs, &tx_cache, actions, &events, &funcs);
    }
    
    // 5. 处理合约创建（使用缓存的数据）
    if process_deployments {
        process_deployments_with_cache(&block, block_number, &tx_cache, actions);
    }
    
    Ok(())
}

/// 使用缓存处理事件
fn process_events_with_cache(
    logs: &[alloy_rpc_types_eth::Log],
    tx_cache: &TxCache,
    actions: &Option<Arc<ActionSet>>,
    events: &abi::EventSigMap,
    funcs: &abi::FuncSigMap,
) {
    for v in logs {
        let topic0 = v.topic0().cloned().unwrap_or(B256::ZERO);
        let topic0_hex = format!("0x{}", hex::encode(topic0));
        let (name, fields) = if let Some((nm, fs)) =
            abi::try_decode_event(&topic0_hex, v.topics(), v.data().data.as_ref(), events)
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
        
        if let Some(a) = actions {
            a.on_event(&er);
        }
        
        // 处理关联的交易（使用缓存）
        if let Some(txh) = v.transaction_hash {
            if let Some(tx_data) = tx_cache.get(&txh) {
                process_transaction(&tx_data.transaction, &tx_data.receipt, actions, funcs);
            }
        }
    }
}

/// 使用缓存处理合约创建
fn process_deployments_with_cache(
    block: &Option<alloy_rpc_types_eth::Block>,
    block_number: u64,
    tx_cache: &TxCache,
    actions: &Option<Arc<ActionSet>>,
) {
    if let Some(block) = block {
        if let Some(transactions) = block.transactions.as_transactions() {
            for tx in transactions {
                if tx.to().is_none() {  // 合约创建
                    if let Some(tx_data) = tx_cache.get(&tx.tx_hash()) {
                        if let Some(receipt) = &tx_data.receipt {
                            if let Some(contract_addr) = receipt.contract_address {
                                let deployment_record = ContractCreationRecord {
                                    contract_address: contract_addr,
                                    deployer: tx.from,
                                    tx_hash: tx.tx_hash(),
                                    block_number: block_number,
                                    tx_index: receipt.transaction_index.unwrap_or(0) as u64,
                                    gas_used: Some(receipt.gas_used as u64),
                                    constructor_args: if tx.input().is_empty() { 
                                        None 
                                    } else { 
                                        Some(tx.input().to_vec()) 
                                    },
                                };
                                
                                println!(
                                    "[deployment] contract={} deployer={} tx={} block={} gas_used={} status={}",
                                    contract_addr,
                                    tx.from,
                                    tx.tx_hash(),
                                    block_number,
                                    receipt.gas_used,
                                    if receipt.status() { "success" } else { "failed" }
                                );
                                
                                if let Some(a) = actions {
                                    a.on_contract_creation(&deployment_record);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 处理单个交易
fn process_transaction(
    tx: &alloy_rpc_types_eth::Transaction,
    receipt: &Option<alloy_rpc_types_eth::TransactionReceipt>,
    actions: &Option<Arc<ActionSet>>,
    funcs: &abi::FuncSigMap,
) {
    let input = tx.input().as_ref();
    if input.len() >= 4 {
        let sel = &input[0..4];
        let sel_hex = format!("0x{}", hex::encode(sel));
        let (fname, args) = if let Some((f, a)) =
            abi::try_decode_function(&sel_hex, input, funcs)
        {
            (Some(f), a)
        } else {
            (None, vec![])
        };
        
        let (
            status,
            gas_used,
            cumulative_gas_used,
            effective_gas_price,
            block_number,
            tx_index,
            contract_address,
            receipt_logs,
        ) = if let Some(r) = receipt {
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
            hash: tx.tx_hash(),
            from: Some(tx.from),
            to: match tx.kind() {
                alloy_primitives::TxKind::Call(a) => Some(a),
                _ => None,
            },
            input_selector: sel.try_into().ok(),
            func_name: fname,
            func_args: args,
            gas: Some(tx.gas_limit()),
            gas_price: alloy_rpc_types_eth::TransactionTrait::gas_price(tx)
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
        
        if let Some(a) = actions {
            a.on_tx(&tr);
        }
    }
}

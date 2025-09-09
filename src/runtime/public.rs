use crate::{
    abi,
    actions::{ActionSet, EventRecord, TxRecord},
};
use alloy_primitives::{hex, B256, U256, Address};
use alloy_rpc_types_eth::{Transaction, TransactionReceipt, TransactionTrait};
use std::sync::Arc;

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



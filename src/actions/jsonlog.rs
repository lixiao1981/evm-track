use super::{Action, BlockRecord, EventRecord, TxRecord};
use serde::Serialize;

#[derive(Serialize)]
struct JsonEvent {
    kind: &'static str,
    address: String,
    tx_hash: Option<String>,
    block_number: Option<u64>,
    name: Option<String>,
    decode_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    decode_error: Option<String>,
    fields: Vec<(String, String)>,
    tx_index: Option<u64>,
    log_index: Option<u64>,
    topics: Vec<String>,
    removed: Option<bool>,
}

#[derive(Serialize)]
struct JsonTx {
    kind: &'static str,
    hash: String,
    from: Option<String>,
    to: Option<String>,
    func: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decode_ok: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    decode_error: Option<String>,
    gas: Option<u64>,
    gas_price: Option<String>,
    effective_gas_price: Option<String>,
    status: Option<u64>,
    gas_used: Option<u64>,
    cumulative_gas_used: Option<u64>,
    block_number: Option<u64>,
    tx_index: Option<u64>,
    contract_address: Option<String>,
    receipt_logs: Option<Vec<JsonReceiptLog>>,
}

#[derive(Serialize)]
struct JsonReceiptLog {
    address: String,
    topics: Vec<String>,
    data: String,
    log_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    removed: Option<bool>,
}

#[derive(Serialize)]
struct JsonBlock {
    kind: &'static str,
    number: u64,
}

fn value_to_string(v: &crate::abi::DecodedValue) -> String {
    use crate::abi::DecodedValue::*;
    match v {
        Address(a) => format!("0x{}", hex::encode(a.0)),
        Uint(u) => u.to_string(),
        Int(i) => i.to_string(),
        Bool(b) => b.to_string(),
        Bytes32(b) => format!("0x{}", hex::encode(b)),
        Bytes(b) => format!("0x{}", hex::encode(b)),
        String(s) => s.clone(),
        Array(arr) => format!(
            "[{}]",
            arr.iter()
                .map(value_to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Unsupported(s) => format!("<unsupported:{}>", s),
    }
}

pub struct JsonLogAction;

impl Action for JsonLogAction {
    fn on_event(&self, e: &EventRecord) -> anyhow::Result<()> {
        let fields = e
            .fields
            .iter()
            .map(|f| (f.name.clone(), value_to_string(&f.value)))
            .collect();
        let j = JsonEvent {
            kind: "event",
            address: format!("0x{}", hex::encode(e.address.0)),
            tx_hash: e.tx_hash.map(|h| format!("0x{}", hex::encode(h))),
            block_number: e.block_number,
            name: e.name.clone(),
            decode_ok: e.name.is_some(),
            decode_error: if e.name.is_none() { Some("unknown_topic0".to_string()) } else { None },
            fields,
            tx_index: e.tx_index,
            log_index: e.log_index,
            topics: e
                .topics
                .iter()
                .map(|t| format!("0x{}", hex::encode(t)))
                .collect(),
            removed: e.removed,
        };
        println!("{}", serde_json::to_string(&j)?);
        Ok(())
    }

    fn on_tx(&self, t: &TxRecord) -> anyhow::Result<()> {
        let j = JsonTx {
            kind: "tx",
            hash: format!("0x{}", hex::encode(t.hash)),
            from: t.from.map(|a| format!("0x{}", hex::encode(a.0))),
            to: t.to.map(|a| format!("0x{}", hex::encode(a.0))),
            func: t.func_name.clone(),
            decode_ok: Some(t.func_name.is_some()).filter(|_| t.input_selector.is_some()),
            decode_error: if t.input_selector.is_some() && t.func_name.is_none() {
                Some("unknown_selector".to_string())
            } else {
                None
            },
            gas: t.gas,
            gas_price: t.gas_price.as_ref().map(|u| u.to_string()),
            effective_gas_price: t.effective_gas_price.as_ref().map(|u| u.to_string()),
            status: t.status,
            gas_used: t.gas_used,
            cumulative_gas_used: t.cumulative_gas_used,
            block_number: t.block_number,
            tx_index: t.tx_index,
            contract_address: t
                .contract_address
                .map(|a| format!("0x{}", hex::encode(a.0))),
            receipt_logs: t.receipt_logs.as_ref().map(|logs| {
                logs.iter()
                    .map(|l| JsonReceiptLog {
                        address: format!("0x{}", hex::encode(l.address.0)),
                        topics: l
                            .topics
                            .iter()
                            .map(|tp| format!("0x{}", hex::encode(tp)))
                            .collect(),
                        data: format!("0x{}", hex::encode(&l.data)),
                        log_index: l.log_index,
                        removed: l.removed,
                    })
                    .collect()
            }),
        };
        println!("{}", serde_json::to_string(&j)?);
        Ok(())
    }

    fn on_block(&self, b: &BlockRecord) -> anyhow::Result<()> {
        let j = JsonBlock {
            kind: "block",
            number: b.number,
        };
        println!("{}", serde_json::to_string(&j)?);
        Ok(())
    }
}

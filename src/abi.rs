use std::{collections::HashMap, fs, path::Path};

use alloy_json_abi::{Event, Function};
use alloy_primitives::{Address, B256, U256};
use anyhow::{Context, Result};
use once_cell::sync::OnceCell;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct EventSigEntry {
    pub abi: Event,
    pub name: String,
    pub sig: String,
}

#[derive(Debug, Deserialize)]
pub struct FuncSigEntry {
    pub abi: Function,
    pub name: String,
    pub sig: String,
}

pub type EventSigMap = HashMap<String, EventSigEntry>; // topic0 hex -> entry
pub type FuncSigMap = HashMap<String, FuncSigEntry>; // selector hex -> entry

static EVENT_SIGS_PATH: OnceCell<String> = OnceCell::new();
static FUNC_SIGS_PATH: OnceCell<String> = OnceCell::new();

pub fn set_event_sigs_path(path: String) {
    let _ = EVENT_SIGS_PATH.set(path);
}

pub fn set_func_sigs_path(path: String) {
    let _ = FUNC_SIGS_PATH.set(path);
}

pub fn load_event_sigs_default() -> Result<EventSigMap> {
    let p = EVENT_SIGS_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| "./data/event_sigs.json".to_string());
    load_event_sigs(p)
}

pub fn load_func_sigs_default() -> Result<FuncSigMap> {
    let p = FUNC_SIGS_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| "./data/func_sigs.json".to_string());
    load_func_sigs(p)
}

pub fn load_event_sigs<P: AsRef<Path>>(path: P) -> Result<EventSigMap> {
    let s = fs::read_to_string(path).context("reading event_sigs.json")?;
    let m: EventSigMap = serde_json::from_str(&s).context("parsing event_sigs.json")?;
    Ok(m)
}

pub fn load_func_sigs<P: AsRef<Path>>(path: P) -> Result<FuncSigMap> {
    let s = fs::read_to_string(path).context("reading func_sigs.json")?;
    let m: FuncSigMap = serde_json::from_str(&s).context("parsing func_sigs.json")?;
    Ok(m)
}

#[derive(Debug, Clone)]
pub enum DecodedValue {
    Address(Address),
    Uint(U256),
    Int(U256),
    Bool(bool),
    Bytes32([u8; 32]),
    Bytes(Vec<u8>),
    String(String),
    Array(Vec<DecodedValue>),
    Unsupported(&'static str),
}

#[derive(Debug, Clone)]
pub struct DecodedField {
    pub name: String,
    pub value: DecodedValue,
    pub indexed: bool,
}

pub fn decode_indexed(topic: &B256, typ: &str) -> DecodedValue {
    let bytes = topic.0;
    match typ {
        "address" => {
            let mut a = [0u8; 20];
            a.copy_from_slice(&bytes[12..]);
            DecodedValue::Address(Address::from(a))
        }
        "bool" => DecodedValue::Bool(bytes[31] != 0),
        t if t.starts_with("uint") => DecodedValue::Uint(U256::from_be_bytes(bytes)),
        t if t.starts_with("int") => DecodedValue::Int(U256::from_be_bytes(bytes)),
        "bytes32" => DecodedValue::Bytes32(bytes),
        _ => DecodedValue::Unsupported("indexed dynamic or unsupported type"),
    }
}

pub fn decode_static_word(word: &[u8], typ: &str) -> DecodedValue {
    match typ {
        "address" => {
            let mut a = [0u8; 20];
            a.copy_from_slice(&word[12..]);
            DecodedValue::Address(Address::from(a))
        }
        "bool" => DecodedValue::Bool(word[31] != 0),
        t if t.starts_with("uint") => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(word);
            DecodedValue::Uint(U256::from_be_bytes(arr))
        }
        t if t.starts_with("int") => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(word);
            DecodedValue::Int(U256::from_be_bytes(arr))
        }
        "bytes32" => DecodedValue::Bytes32(word.try_into().unwrap()),
        _ => DecodedValue::Unsupported("dynamic or unsupported type"),
    }
}

fn is_dynamic_type(typ: &str) -> bool {
    matches!(typ, "string" | "bytes") || typ.ends_with("[]")
}

fn decode_dynamic<'a>(data: &'a [u8], offset: usize, elem_type: &str) -> Option<DecodedValue> {
    if offset + 32 > data.len() {
        return None;
    }
    if elem_type == "string" || elem_type == "bytes" {
        // offset -> length -> bytes
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&data[offset..offset + 32]);
        let len = U256::from_be_bytes(arr).to::<usize>();
        let start = offset + 32;
        let _end = start + ((len + 31) / 32) * 32; // padded end (unused)
        if start + len > data.len() {
            return None;
        }
        let raw = &data[start..start + len];
        return Some(if elem_type == "string" {
            DecodedValue::String(String::from_utf8_lossy(raw).into_owned())
        } else {
            DecodedValue::Bytes(raw.to_vec())
        });
    }
    // dynamic array: T[]
    if let Some(base) = elem_type.strip_suffix("[]") {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&data[offset..offset + 32]);
        let count = U256::from_be_bytes(arr).to::<usize>();
        let start = offset + 32;
        // static element size = 32 bytes for primitive statics
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let off = start + i * 32;
            if off + 32 > data.len() {
                return None;
            }
            if is_dynamic_type(base) {
                // nested dynamic not supported in this simple decoder
                return Some(DecodedValue::Unsupported("nested dynamic array"));
            } else {
                out.push(decode_static_word(&data[off..off + 32], base));
            }
        }
        return Some(DecodedValue::Array(out));
    }
    None
}

pub fn try_decode_event(
    topic0_hex: &str,
    topics: &[B256],
    data: &[u8],
    events: &EventSigMap,
) -> Option<(String, Vec<DecodedField>)> {
    let entry = events.get(topic0_hex)?;
    let mut fields = Vec::new();
    // indexed decoding from topics[1..]
    let mut ti = 1usize;
    // head area is 32 bytes per non-indexed input
    let _non_indexed: Vec<_> = entry.abi.inputs.iter().filter(|i| !i.indexed).collect();
    // decode
    let mut head_index = 0usize;
    for input in &entry.abi.inputs {
        let name = input.name.clone();
        if input.indexed {
            if ti < topics.len() {
                let v = decode_indexed(&topics[ti], &input.ty);
                ti += 1;
                fields.push(DecodedField {
                    name,
                    value: v,
                    indexed: true,
                });
            }
        } else {
            let typ = input.ty.as_str();
            // head word
            let head_off = head_index * 32;
            head_index += 1;
            if head_off + 32 > data.len() {
                continue;
            }
            if is_dynamic_type(typ) {
                // read offset relative to start of data
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[head_off..head_off + 32]);
                let off = U256::from_be_bytes(arr).to::<usize>();
                if let Some(v) = decode_dynamic(data, off, typ) {
                    fields.push(DecodedField {
                        name,
                        value: v,
                        indexed: false,
                    });
                } else {
                    fields.push(DecodedField {
                        name,
                        value: DecodedValue::Unsupported("dynamic decode failed"),
                        indexed: false,
                    });
                }
            } else {
                let word = &data[head_off..head_off + 32];
                let v = decode_static_word(word, typ);
                fields.push(DecodedField {
                    name,
                    value: v,
                    indexed: false,
                });
            }
        }
    }
    Some((entry.name.clone(), fields))
}

pub fn try_decode_function(
    selector_hex: &str,
    calldata: &[u8],
    funcs: &FuncSigMap,
) -> Option<(String, Vec<DecodedValue>)> {
    let entry = funcs.get(selector_hex)?;
    let head_base = 4; // skip selector
    let count = entry.abi.inputs.len();
    let mut values = Vec::with_capacity(count);
    for (i, input) in entry.abi.inputs.iter().enumerate() {
        let typ = input.ty.as_str();
        let off = head_base + i * 32;
        if off + 32 > calldata.len() {
            break;
        }
        if is_dynamic_type(typ) {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&calldata[off..off + 32]);
            let rel = U256::from_be_bytes(arr).to::<usize>();
            let data_off = head_base + rel;
            if let Some(v) = decode_dynamic(&calldata, data_off, typ) {
                values.push(v);
            } else {
                values.push(DecodedValue::Unsupported("dynamic decode failed"));
            }
        } else {
            let word = &calldata[off..off + 32];
            values.push(decode_static_word(word, typ));
        }
    }
    Some((entry.name.clone(), values))
}

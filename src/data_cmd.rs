use crate::error::Result;
use alloy_json_abi::{AbiItem, Event};
use alloy_primitives::{hex, keccak256, B256};
use reqwest::Client;
use serde_json::json;
use std::fs;
use std::path::Path;

// Merge event signatures from an ABI file into an output JSON map.
// Format: { "0x<topic0>": { name, sig, abi } }
pub fn add_events_from_abi<P: AsRef<Path>>(abi_path: P, output_path: P) -> Result<()> {
    let abi_text = fs::read_to_string(&abi_path)?;
    let items: Vec<AbiItem<'_>> = serde_json::from_str(&abi_text)?;

    // Load existing map if present
    let mut out_map: serde_json::Map<String, serde_json::Value> = if output_path.as_ref().exists() {
        let s = fs::read_to_string(&output_path)?;
        serde_json::from_str(&s).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    for item in items {
        if let AbiItem::Event(ev_cow) = item {
            let ev: Event = ev_cow.into_owned();
            // Build signature string Name(type1,type2,...)
            let sig = format!(
                "{}({})",
                ev.name,
                ev.inputs.iter().map(|p| p.ty.as_str()).collect::<Vec<_>>().join(",")
            );
            let topic0: B256 = keccak256(sig.as_bytes());
            let key = format!("0x{}", hex::encode(topic0));
            // JSON encode the Event
            let entry = json!({
                "name": ev.name,
                "sig": sig,
                "abi": ev,
            });
            out_map.insert(key, entry);
        }
    }

    let pretty = serde_json::to_string_pretty(&out_map)?;
    if let Some(parent) = output_path.as_ref().parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&output_path, pretty)?;
    Ok(())
}

pub async fn fetch_abi_from_scanner(
    address: &str,
    scanner_url: &str,
    api_key: Option<&str>,
) -> Result<String> {
    let mut url = scanner_url.replace("%v", address);
    if let Some(k) = api_key {
        if !url.to_ascii_lowercase().contains("apikey=") {
            if url.contains('?') {
                url.push_str(&format!("&apikey={}", k));
            } else {
                url.push_str(&format!("?apikey={}", k));
            }
        }
    }
    let cli = Client::new();
    let resp = cli.get(url).send().await?;
    let text = resp.text().await?;
    Ok(text)
}

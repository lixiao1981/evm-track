use base64::Engine;
use std::{collections::HashSet, fs, path::Path, sync::Arc, time::Duration};

use alloy_primitives::Address;
use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use crate::error::AppError;
type Result<T> = std::result::Result<T, AppError>;
use serde::{Deserialize, Serialize};

use super::{Action, TxRecord};
use tokio::sync::{RwLock, Semaphore};

#[derive(Clone, Debug, Default)]
pub struct InitscanOptions {
    pub from: Option<Address>,
    pub check_addresses: Vec<Address>,
    pub init_after_delay_secs: u64,
    pub usd_threshold: f64,
    pub func_sigs: Vec<(String, Vec<u8>)>,
    pub webhook_url: Option<String>,
    // persistence + retry
    pub initializable_contracts_filepath: Option<String>,
    pub init_known_contracts_frequency_secs: Option<u64>,
    // limit concurrent init attempts; None or 0 => unlimited
    pub max_inflight_inits: Option<usize>,
    // enable verbose debug logs
    pub debug: bool,
}

pub struct InitscanAction {
    provider: Arc<RootProvider<BoxTransport>>,
    opts: InitscanOptions,
    known: Arc<RwLock<Vec<KnownInit>>>,
    sem: Option<Arc<Semaphore>>,
}

impl InitscanAction {
    pub fn new(provider: Arc<RootProvider<BoxTransport>>, mut opts: InitscanOptions) -> Self {
        // Ensure from address appended to check_addresses if provided
        if let Some(f) = opts.from {
            if !opts.check_addresses.contains(&f) {
                opts.check_addresses.push(f);
            }
        }
        let known = if let Some(p) = &opts.initializable_contracts_filepath {
            Arc::new(RwLock::new(load_known_from_file(p).unwrap_or_default()))
        } else {
            Arc::new(RwLock::new(vec![]))
        };

        let sem = opts
            .max_inflight_inits
            .and_then(|n| if n > 0 { Some(Arc::new(Semaphore::new(n))) } else { None });

        let action = Self { provider: provider.clone(), opts: opts.clone(), known: known.clone(), sem };

        if let (Some(path), Some(freq)) = (
            opts.initializable_contracts_filepath.clone(),
            opts.init_known_contracts_frequency_secs,
        ) {
            if freq > 0 {
                let this = action.clone_for_task();
                tokio::spawn(async move {
                    let dur = Duration::from_secs(freq);
                    loop {
                        tokio::time::sleep(dur).await;
                        if let Err(e) = this.retry_known_and_save(&path).await {
                            eprintln!("[initscan] periodic retry error: {e}");
                        }
                    }
                });
            }
        }

        action
    }

    fn clone_for_task(&self) -> Self { Self { provider: self.provider.clone(), opts: self.opts.clone(), known: self.known.clone(), sem: self.sem.clone() } }
    #[inline]
    fn dbg<S: AsRef<str>>(&self, s: S) { if self.opts.debug { println!("[initscan][debug] {}", s.as_ref()); } }

    pub async fn retry_known_and_save(&self, path: &str) -> Result<()> {
        let snapshot = { self.known.read().await.clone() };
        if snapshot.is_empty() { return Ok(()); }
        if self.opts.debug { println!("[initscan][debug] retry_known_and_save: {} entries", snapshot.len()); }
        let mut kept: Vec<KnownInit> = Vec::with_capacity(snapshot.len());
        for item in snapshot.iter() {
            match self.evaluate_once(item.contract, None, &item.calldata).await {
                Ok(true) => kept.push(item.clone()),
                Ok(false) => { /* drop */ }
                Err(e) => { eprintln!("[initscan] retry error on {:?}: {e}", item.contract); kept.push(item.clone()); }
            }
        }
        {
            let mut w = self.known.write().await;
            *w = kept.clone();
        }
        save_known_to_file(path, &kept)?;
        Ok(())
    }

    async fn add_known_and_save(&self, contract: Address, calldata: &[u8]) -> Result<()> {
        if let Some(path) = &self.opts.initializable_contracts_filepath {
            let mut w = self.known.write().await;
            if !w.iter().any(|k| k.contract == contract) {
                w.push(KnownInit { contract, calldata: calldata.to_vec() });
                save_known_to_file(path, &w)?;
                println!("[initscan] added {} to known list", format!("0x{}", hex::encode(contract.0)));
                self.dbg(format!("persisted to {} ({} entries)", path, w.len()));
            }
        }
        Ok(())
    }

    async fn try_init_with_calldata(&self, contract: Address, block_number: Option<u64>, calldata: &[u8]) -> Result<()> {
        // Eth call check
        self.dbg(format!("try_init_with_calldata: contract=0x{} block={:?} calldata_len={} head=0x{}", hex::encode(contract.0), block_number, calldata.len(), hex::encode(&calldata[..calldata.len().min(8)])));
        let ok = eth_call_ok(self.provider.as_ref(), self.opts.from, contract, calldata, block_number).await?;
        self.dbg(format!("eth_call ok={} (no revert)", ok));
        if !ok { return Ok(()); }
        // Trace
        let tr = trace_call(self.provider.as_ref(), self.opts.from, contract, calldata, block_number).await?;
        self.dbg(format!("trace_call returned: traces={} state_diff_len={}", tr.traces.len(), serde_json::to_string(&tr.state_diff).unwrap_or_default().len()));
        if !trace_success(&tr) { self.dbg("trace_call had error in traces"); return Ok(()); }
        let contains = state_diff_contains_any_addr(&tr, &self.opts.check_addresses);
        self.dbg(format!("stateDiff contains check address = {}", contains));
        if !contains { return Ok(()); }
        // Fallback sanity
        let random_sel = hex::decode("6fcb831b").unwrap_or_default();
        let tr2 = trace_call(self.provider.as_ref(), self.opts.from, contract, &random_sel, block_number).await?;
        let contains2 = trace_success(&tr2) && state_diff_contains_any_addr(&tr2, &self.opts.check_addresses);
        self.dbg(format!("random selector check contains = {}", contains2));
        if contains && !contains2 {
            // Passed heuristics: alert + persist
            let msg = format!(
                "# Interesting contract\nAddress: 0x{}\ncalldataLen: {}\n",
                hex::encode(contract.0), calldata.len()
            );
            if let Some(url) = &self.opts.webhook_url { self.dbg(format!("sending webhook to {}", url)); let _ = send_webhook(url, &msg).await; } else { println!("[initscan] {}", msg.replace('\n', " ")); }
            let _ = self.add_known_and_save(contract, calldata).await;
        }
        Ok(())
    }

    async fn evaluate_once(&self, contract: Address, block_number: Option<u64>, calldata: &[u8]) -> Result<bool> {
        let ok = eth_call_ok(self.provider.as_ref(), self.opts.from, contract, calldata, block_number).await?;
        if !ok { return Ok(false); }
        let tr = trace_call(self.provider.as_ref(), self.opts.from, contract, calldata, block_number).await?;
        if !trace_success(&tr) { return Ok(false); }
        let contains = state_diff_contains_any_addr(&tr, &self.opts.check_addresses);
        if !contains { return Ok(false); }
        let random_sel = hex::decode("6fcb831b").unwrap_or_default();
        let tr2 = trace_call(self.provider.as_ref(), self.opts.from, contract, &random_sel, block_number).await?;
        let contains2 = trace_success(&tr2) && state_diff_contains_any_addr(&tr2, &self.opts.check_addresses);
        Ok(contains && !contains2)
    }

    // Public helper for external callers (e.g. history scanner)
    pub async fn try_init_for_contract(&self, contract: Address, block_number: Option<u64>) {
        // concurrency gate (optional)
        let _permit = match &self.sem {
            Some(s) => Some(s.clone().acquire_owned().await.expect("semaphore closed")),
            None => None,
        };
        self.dbg(format!("try_init_for_contract: contract=0x{} block={:?} func_variants={}", hex::encode(contract.0), block_number, self.opts.func_sigs.len()));
        if self.opts.init_after_delay_secs > 0 {
            tokio::time::sleep(Duration::from_secs(self.opts.init_after_delay_secs)).await;
        }
        for (_sig, calldata) in &self.opts.func_sigs {
            let _ = self
                .try_init_with_calldata(contract, block_number, calldata)
                .await;
        }
        drop(_permit);
    }
}

// Move this block outside of the impl InitscanAction
impl Action for InitscanAction {
    fn on_tx(&self, t: &TxRecord) -> Result<()> {
        // Only react to deployments (receipt has contract address)
        if let Some(contract) = t.contract_address {
            let this = self.clone_for_task();
            let block_number = t.block_number; // Option<u64>
            tokio::spawn(async move {
                // concurrency gate (optional)
                let _permit = match &this.sem {
                    Some(s) => Some(s.clone().acquire_owned().await.expect("semaphore closed")),
                    None => None,
                };
                this.dbg(format!("on_tx: deployment detected contract=0x{} block={:?}", hex::encode(contract.0), block_number));
                if this.opts.init_after_delay_secs > 0 {
                    tokio::time::sleep(Duration::from_secs(this.opts.init_after_delay_secs)).await;
                }
                for (_sig, calldata) in &this.opts.func_sigs {
                    if let Err(e) = this.try_init_with_calldata(contract, block_number, calldata).await {
                        eprintln!("[initscan] error on {contract:?}: {e}");
                    }
                }
                drop(_permit);
            });
        }
        Ok(())
    }
}


// Persistence types/helpers
#[derive(Clone, Debug)]
struct KnownInit { contract: Address, calldata: Vec<u8> }

#[derive(Serialize, Deserialize)]
struct KnownInitSerde { contract: String, calldata: String }

fn load_known_from_file(path: &str) -> Result<Vec<KnownInit>> {
    if !Path::new(path).exists() {
        return Ok(vec![]);
    }
    let s = fs::read_to_string(path)?;
    if s.trim().is_empty() {
        return Ok(vec![]);
    }
    let arr: Vec<KnownInitSerde> = serde_json::from_str(&s)?;
    let mut out = vec![];
    for it in arr.into_iter() {
        let contract: Address = it
            .contract
            .parse()
            .map_err(|e| AppError::General(format!("failed to parse address: {}", e)))?;
        let data = if let Ok(b) = hex::decode(it.calldata.trim_start_matches("0x")) {
            b
        } else if let Ok(b) = base64::engine::general_purpose::STANDARD.decode(&it.calldata) {
            b
        } else {
            vec![]
        };
        out.push(KnownInit { contract, calldata: data });
    }
    Ok(out)
}

fn save_known_to_file(path: &str, list: &Vec<KnownInit>) -> Result<()> {
    let arr: Vec<KnownInitSerde> = list.iter().map(|k| KnownInitSerde {
        contract: format!("0x{}", hex::encode(k.contract.0)),
        calldata: format!("0x{}", hex::encode(&k.calldata)),
    }).collect();
    let data = serde_json::to_string_pretty(&arr).map_err(|e| AppError::from(e))?;
    fs::write(path, data).map_err(|e| AppError::from(e))?;
    Ok(())
}

async fn send_webhook(url: &str, content: &str) -> Result<()> {
    #[derive(Serialize)]
    struct Payload<'a> { content: &'a str }
    let client = reqwest::Client::new();
    let _resp = client
        .post(url)
        .json(&Payload { content })
        .send()
        .await
        .map_err(|e| AppError::from(e))?;
    Ok(())
}

async fn eth_call_ok(
    provider: &RootProvider<BoxTransport>,
    from: Option<Address>,
    to: Address,
    data: &[u8],
    block_number: Option<u64>,
) -> Result<bool> {
    let call = serde_json::json!({
        "from": from.map(|a| format!("0x{}", hex::encode(a.0))),
        "to": format!("0x{}", hex::encode(to.0)),
        "data": format!("0x{}", hex::encode(data)),
        "value": "0x0",
    });
    let block = block_number
        .map(|n| format!("0x{:x}", n))
        .unwrap_or_else(|| "latest".to_string());
    // If eth_call returns without RPC error, we treat as ok
    let _: String = provider
        .client()
        .request("eth_call", serde_json::json!([call, block]))
        .await
        .map_err(|e| AppError::from(e))?;
    Ok(true)
}

#[derive(Debug, Deserialize)]
struct TraceCallResult {
    #[serde(default, rename = "trace")]
    traces: Vec<TraceNode>,
    #[serde(default, rename = "stateDiff")]
    state_diff: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct TraceNode {
    #[serde(default)]
    error: String,
}

async fn trace_call(
    provider: &RootProvider<BoxTransport>,
    from: Option<Address>,
    to: Address,
    data: &[u8],
    block_number: Option<u64>,
) -> Result<TraceCallResult> {
    let call = serde_json::json!({
        "from": from.map(|a| format!("0x{}", hex::encode(a.0))),
        "to": format!("0x{}", hex::encode(to.0)),
        "data": format!("0x{}", hex::encode(data)),
        "value": "0x0",
    });
    let block = block_number
        .map(|n| format!("0x{:x}", n))
        .unwrap_or_else(|| "latest".to_string());
    let params = serde_json::json!([call, ["trace", "stateDiff"], block]);
    let v: serde_json::Value = provider
        .client()
        .request("trace_call", params)
        .await
        .map_err(|e| AppError::from(e))?;
    // Some clients wrap response; try direct deserialize
    let r: TraceCallResult = serde_json::from_value(v.clone())
        .or_else(|_| {
            // Some nodes return {result: {...}}
            serde_json::from_value(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
        })
        .map_err(|e| AppError::from(e))?;
    Ok(r)
}

fn trace_success(tr: &TraceCallResult) -> bool {
    tr.traces.iter().all(|t| t.error.is_empty())
}

fn state_diff_contains_any_addr(tr: &TraceCallResult, addrs: &[Address]) -> bool {
    if addrs.is_empty() {
        return false;
    }
    let s = serde_json::to_string(&tr.state_diff).unwrap_or_default().to_lowercase();
    let mut set = HashSet::new();
    for a in addrs {
        // search address hex (without 0x) in state diff JSON
        let needle = hex::encode(a.0).to_lowercase();
        if !needle.is_empty() {
            set.insert(needle);
        }
    }
    set.into_iter().any(|n| s.contains(&n))
}

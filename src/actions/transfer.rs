use std::{collections::HashMap, sync::Arc};

use alloy_primitives::{Address, Bytes, U256};
use alloy_provider::{Provider, RootProvider};
use alloy_rpc_types_eth::{TransactionRequest, transaction::TransactionInput};
use alloy_transport::BoxTransport;
use tokio::sync::{Mutex, Semaphore};

use super::{Action, EventRecord};
use crate::throttle;

pub struct TransferAction {
    provider: Arc<RootProvider<BoxTransport>>,
    cache: Arc<Mutex<HashMap<Address, (String, u8)>>>, // token -> (symbol, decimals)
    limiter: Arc<Semaphore>,
}

impl TransferAction {
    pub fn new(provider: Arc<RootProvider<BoxTransport>>) -> Self {
        Self {
            provider,
            cache: Arc::new(Mutex::new(HashMap::new())),
            limiter: Arc::new(Semaphore::new(5)),
        }
    }
}

fn scale_amount(v: &U256, decimals: u8) -> String {
    let mut denom = U256::from(1);
    for _ in 0..decimals {
        denom = denom * U256::from(10u8);
    }
    let int = *v / denom;
    let frac = *v % denom;
    if decimals == 0 {
        return int.to_string();
    }
    // left pad fractional to decimals digits
    let mut frac_str = frac.to_string();
    let width = decimals as usize;
    if frac_str.len() < width {
        frac_str = format!("{}{}", "0".repeat(width - frac_str.len()), frac_str);
    }
    format!("{}.{}", int, frac_str)
}

async fn eth_call_str(
    provider: &RootProvider<BoxTransport>,
    to: Address,
    data: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let tx = TransactionRequest::default()
        .to(to)
        .input(TransactionInput::new(Bytes::from(data.to_vec())));
    throttle::acquire().await;
    let out: Bytes = provider.call(&tx).await?;
    Ok(out.to_vec())
}

fn decode_string_return(data: &[u8]) -> Option<String> {
    if data.len() < 64 {
        return None;
    }
    // expect offset 0x20 at first word
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&data[32..64]);
    let len = U256::from_be_bytes(arr).to::<usize>();
    if 64 + len > data.len() {
        return None;
    }
    let raw = &data[64..64 + len];
    Some(String::from_utf8_lossy(raw).into_owned())
}

fn decode_bytes32_symbol(data: &[u8]) -> Option<String> {
    if data.len() < 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&data[..32]);
    // trim trailing nulls
    let end = buf
        .iter()
        .rposition(|&b| b != 0)
        .map(|i| i + 1)
        .unwrap_or(0);
    if end == 0 {
        return None;
    }
    let s = String::from_utf8_lossy(&buf[..end]).into_owned();
    Some(s)
}

impl Action for TransferAction {
    fn on_event(&self, e: &EventRecord) -> anyhow::Result<()> {
        if let Some(name) = &e.name {
            if name == "Transfer" {
                let token = e.address;
                let mut from_addr = None;
                let mut to_addr = None;
                let mut amount_u256: Option<U256> = None;
                for f in &e.fields {
                    match f.name.as_str() {
                        "from" | "_from" => {
                            if let crate::abi::DecodedValue::Address(a) = &f.value {
                                from_addr = Some(*a);
                            }
                        }
                        "to" | "_to" => {
                            if let crate::abi::DecodedValue::Address(a) = &f.value {
                                to_addr = Some(*a);
                            }
                        }
                        "value" | "amount" => {
                            if let crate::abi::DecodedValue::Uint(u) = &f.value {
                                amount_u256 = Some(*u);
                            }
                        }
                        _ => {}
                    }
                }

                let provider = self.provider.clone();
                let cache = self.cache.clone();
                tokio::spawn(async move {
                    let _permit = provider.clone();
                    let (symbol, decimals) = {
                        let mut guard = cache.lock().await;
                        if let Some(v) = guard.get(&token) {
                            v.clone()
                        } else {
                            // decimals(): 0x313ce567, symbol(): 0x95d89b41
                            let dec =
                                match eth_call_str(&provider, token, &[0x31, 0x3c, 0xe5, 0x67])
                                    .await
                                {
                                    Ok(ret) => ret.get(31).cloned().unwrap_or(18u8),
                                    Err(_) => 18u8,
                                };
                            let sym =
                                match eth_call_str(&provider, token, &[0x95, 0xd8, 0x9b, 0x41])
                                    .await
                                {
                                    Ok(ret) => decode_string_return(&ret)
                                        .or_else(|| decode_bytes32_symbol(&ret))
                                        .unwrap_or_else(|| "TKN".to_string()),
                                    Err(_) => "TKN".to_string(),
                                };
                            guard.insert(token, (sym.clone(), dec));
                            (sym, dec)
                        }
                    };

                    let human = amount_u256.map(|u| scale_amount(&u, decimals));
                    println!(
                        "[transfer] token={}({}) from={:?} to={:?} value_raw={:?} value={:?}",
                        token, symbol, from_addr, to_addr, amount_u256, human
                    );
                });
            }
        }
        Ok(())
    }
}

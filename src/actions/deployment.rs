use std::sync::Arc;

use alloy_primitives::{Address, B256, keccak256};
use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use crate::throttle;

use super::{Action, TxRecord};
use serde::Serialize;

#[derive(Clone, Default)]
pub struct DeploymentOptions {
    pub output_filepath: Option<String>,
}

pub struct DeploymentScanAction {
    provider: Arc<RootProvider<BoxTransport>>,
    opts: DeploymentOptions,
}

impl DeploymentScanAction {
    pub fn new(provider: Arc<RootProvider<BoxTransport>>, opts: DeploymentOptions) -> Self {
        Self { provider, opts }
    }
}

impl Action for DeploymentScanAction {
    fn on_tx(&self, t: &TxRecord) -> anyhow::Result<()> {
        if let Some(addr) = t.contract_address {
            let provider = self.provider.clone();
            let opts = self.opts.clone();
            tokio::spawn(async move {
                if let Err(err) = scan_code(provider, addr, &opts).await {
                    eprintln!("[deploy-scan] error: {err}");
                }
            });
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct DeploymentRecord {
    kind: &'static str,
    contract: String,
    code_size: usize,
    code_keccak: String,
    head: String,
    empty: bool,
    eip1167_min_proxy: bool,
    eip1167_impl: Option<String>,
    eip1967_impl_slot_ref: bool,
    eip1967_admin_slot_ref: bool,
}

async fn scan_code(
    provider: Arc<RootProvider<BoxTransport>>,
    addr: Address,
    opts: &DeploymentOptions,
) -> anyhow::Result<()> {
    // Fetch runtime bytecode
    throttle::acquire().await;
    let code = provider.get_code_at(addr).await?; // Bytes
    let len = code.len();
    let hash: B256 = keccak256(&code);
    let head = if len >= 16 { &code[..16] } else { &code[..] };

    // Heuristics
    let is_empty = len == 0;
    let (is_min_proxy, impl_addr) = detect_eip1167_minimal_proxy(&code);
    let eip1967_impl_ref = contains_slice(&code, &hex::decode("360894A13BA1A3210667C828492DB98DCA3E2076CC3735A920A3CA505D382BBC").unwrap());
    let eip1967_admin_ref = contains_slice(&code, &hex::decode("b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103").unwrap());

    println!(
        "[deploy-scan] contract={} code_size={} code_keccak=0x{} head=0x{} empty={} min_proxy={} impl_addr={:?} eip1967_impl_ref={} eip1967_admin_ref={}",
        addr,
        len,
        hex::encode(hash),
        hex::encode(head),
        is_empty,
        is_min_proxy,
        impl_addr,
        eip1967_impl_ref,
        eip1967_admin_ref,
    );

    if let Some(path) = &opts.output_filepath {
        let rec = DeploymentRecord {
            kind: "deployment",
            contract: format!("0x{}", hex::encode(addr)),
            code_size: len,
            code_keccak: format!("0x{}", hex::encode(hash)),
            head: format!("0x{}", hex::encode(head)),
            empty: is_empty,
            eip1167_min_proxy: is_min_proxy,
            eip1167_impl: impl_addr.map(|a| format!("0x{}", hex::encode(a.0))),
            eip1967_impl_slot_ref: eip1967_impl_ref,
            eip1967_admin_slot_ref: eip1967_admin_ref,
        };
        if let Ok(s) = serde_json::to_string(&rec) {
            let _ = append_line(path, &s);
        }
    }
    Ok(())
}

fn detect_eip1167_minimal_proxy(code: &[u8]) -> (bool, Option<Address>) {
    // EIP-1167 minimal proxy runtime bytecode pattern:
    // 0x363d3d373d3d3d363d73 <20-byte implementation> 0x5af43d82803e903d91602b57fd5bf3
    const PREFIX: [u8; 10] = [0x36, 0x3d, 0x3d, 0x37, 0x3d, 0x3d, 0x3d, 0x36, 0x3d, 0x73];
    const SUFFIX: [u8; 15] = [
        0x5a, 0xf4, 0x3d, 0x82, 0x80, 0x3e, 0x90, 0x3d, 0x91, 0x60, 0x2b, 0x57, 0xfd, 0x5b, 0xf3,
    ];
    if code.len() == PREFIX.len() + 20 + SUFFIX.len()
        && code.starts_with(&PREFIX)
        && code[code.len() - SUFFIX.len()..] == SUFFIX
    {
        let mut a = [0u8; 20];
        a.copy_from_slice(&code[PREFIX.len()..PREFIX.len() + 20]);
        return (true, Some(Address::from(a)));
    }
    (false, None)
}

fn contains_slice(hay: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || hay.len() < needle.len() {
        return false;
    }
    hay.windows(needle.len()).any(|w| w == needle)
}

fn append_line(path: &str, s: &str) -> std::io::Result<()> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{}", s)?;
    Ok(())
}

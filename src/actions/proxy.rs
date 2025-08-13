use std::sync::Arc;
use alloy_primitives::{B256, Address};
use alloy_provider::RootProvider;
use super::{Action, EventRecord};

// Minimal proxy upgrade detector: handles common OZ events and verifies EIP-1967 impl slot
// - Upgraded(address indexed implementation)
// - ImplementationUpgraded(address indexed newImplementation)
// - AdminChanged(address previousAdmin, address newAdmin)

pub struct ProxyUpgradeAction {
    provider: Arc<RootProvider<alloy_transport_ws::WsClient>>,
}

impl ProxyUpgradeAction {
    pub fn new(provider: Arc<RootProvider<alloy_transport_ws::WsClient>>) -> Self { Self { provider } }
}

fn eip1967_impl_slot() -> B256 {
    // 0x360894A13BA1A3210667C828492DB98DCA3E2076CC3735A920A3CA505D382BBC
    B256::from_slice(&hex::decode("360894A13BA1A3210667C828492DB98DCA3E2076CC3735A920A3CA505D382BBC").unwrap())
}

fn right_most_20(bytes: &[u8]) -> Address {
    let mut a = [0u8; 20];
    a.copy_from_slice(&bytes[12..32]);
    Address::from(a)
}

impl Action for ProxyUpgradeAction {
    fn on_event(&self, e: &EventRecord) -> anyhow::Result<()> {
        if let Some(name) = &e.name {
            match name.as_str() {
                "Upgraded" | "ImplementationUpgraded" => {
                    let mut impl_addr = None;
                    for f in &e.fields {
                        let key = f.name.to_lowercase();
                        if key.contains("implementation") {
                            impl_addr = Some(format!("{:?}", f.value));
                            break;
                        }
                    }
                    let provider = self.provider.clone();
                    let proxy = e.address;
                    tokio::spawn(async move {
                        // verify EIP-1967 implementation slot
                        let slot = eip1967_impl_slot();
                        match provider.get_storage_at(proxy, slot, None).await {
                            Ok(bytes) => {
                                let onchain = right_most_20(bytes.as_ref());
                                println!(
                                    "[proxy-upgrade] proxy={} new_impl={:?} impl_slot={} tx={:?} block={:?}",
                                    proxy,
                                    impl_addr,
                                    onchain,
                                    e.tx_hash,
                                    e.block_number
                                );
                            }
                            Err(err) => {
                                println!(
                                    "[proxy-upgrade] proxy={} new_impl={:?} (slot read error: {}) tx={:?} block={:?}",
                                    proxy, impl_addr, err, e.tx_hash, e.block_number
                                );
                            }
                        }
                    });
                }
                "AdminChanged" => {
                    let mut prev = None;
                    let mut newa = None;
                    for f in &e.fields {
                        let key = f.name.to_lowercase();
                        match key.as_str() {
                            "previousadmin" | "previous_admin" | "from" => prev = Some(format!("{:?}", f.value)),
                            "newadmin" | "new_admin" | "to" => newa = Some(format!("{:?}", f.value)),
                            _ => {}
                        }
                    }
                    let provider = self.provider.clone();
                    let proxy = e.address;
                    tokio::spawn(async move {
                        // EIP-1967 admin slot: 0xb53127684a568b3173ae13b9f8a6016e...
                        let admin_slot = B256::from_slice(&hex::decode("b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103").unwrap());
                        let onchain_admin = match provider.get_storage_at(proxy, admin_slot, None).await {
                            Ok(bytes) => Some(super::proxy::right_most_20(bytes.as_ref())),
                            Err(_) => None,
                        };
                        println!(
                            "[proxy-admin-changed] proxy={} prev={:?} new={:?} onchain_admin={:?} tx={:?} block={:?}",
                            proxy, prev, newa, onchain_admin, e.tx_hash, e.block_number
                        );
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

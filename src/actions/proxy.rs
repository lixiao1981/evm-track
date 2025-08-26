use super::{Action, EventRecord};
use alloy_primitives::{Address, B256};
use alloy_provider::Provider;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use crate::throttle;
use std::sync::Arc;
// Minimal proxy upgrade detector: handles common OZ events and verifies EIP-1967 impl slot
// - Upgraded(address indexed implementation)
// - ImplementationUpgraded(address indexed newImplementation)
// - AdminChanged(address previousAdmin, address newAdmin)

pub struct ProxyUpgradeAction {
    provider: Arc<RootProvider<BoxTransport>>, // 修改类型
}

impl ProxyUpgradeAction {
    pub fn new(provider: Arc<RootProvider<BoxTransport>>) -> Self {
        Self { provider }
    } // 修改类型
}

fn eip1967_impl_slot() -> B256 {
    // 0x360894A13BA1A3210667C828492DB98DCA3E2076CC3735A920A3CA505D382BBC
    B256::from_slice(
        &hex::decode("360894A13BA1A3210667C828492DB98DCA3E2076CC3735A920A3CA505D382BBC").unwrap(),
    )
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
                    let txh = e.tx_hash;
                    let bn = e.block_number;
                    tokio::spawn(async move {
                        // verify EIP-1967 implementation slot
                        let slot = eip1967_impl_slot();
                        let slot_u256 = alloy_primitives::U256::from_be_slice(slot.as_slice());
                        throttle::acquire().await;
                        match provider.get_storage_at(proxy, slot_u256).await {
                            // storage word
                            Ok(bytes) => {
                                let be: [u8; 32] = bytes.to_be_bytes::<32>();
                                let onchain = right_most_20(&be);
                                println!(
                                    "[proxy-upgrade] proxy={} new_impl={:?} impl_slot={} tx={:?} block={:?}",
                                    proxy, impl_addr, onchain, txh, bn
                                );
                            }
                            Err(err) => {
                                println!(
                                    "[proxy-upgrade] proxy={} new_impl={:?} (slot read error: {}) tx={:?} block={:?}",
                                    proxy, impl_addr, err, txh, bn
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
                            "previousadmin" | "previous_admin" | "from" => {
                                prev = Some(format!("{:?}", f.value))
                            }
                            "newadmin" | "new_admin" | "to" => {
                                newa = Some(format!("{:?}", f.value))
                            }
                            _ => {}
                        }
                    }
                    let provider = self.provider.clone();
                    let proxy = e.address;
                    let txh = e.tx_hash;
                    let bn = e.block_number;
                    tokio::spawn(async move {
                        // EIP-1967 admin slot: 0xb53127684a568b3173ae13b9f8a6016e...
                        let admin_slot = B256::from_slice(
                            &hex::decode(
                                "b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103",
                            )
                            .unwrap(),
                        );
                        let admin_slot_u256 =
                            alloy_primitives::U256::from_be_slice(admin_slot.as_slice());
                        throttle::acquire().await;
                        let onchain_admin =
                            match provider.get_storage_at(proxy, admin_slot_u256).await {
                                Ok(bytes) => {
                                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                                    Some(super::proxy::right_most_20(&be))
                                }
                                Err(_) => None,
                            };
                        println!(
                            "[proxy-admin-changed] proxy={} prev={:?} new={:?} onchain_admin={:?} tx={:?} block={:?}",
                            proxy, prev, newa, onchain_admin, txh, bn
                        );
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

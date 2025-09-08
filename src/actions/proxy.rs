use super::{Action, EventRecord};
use crate::error::Result;
use crate::throttle;
use alloy_primitives::{Address, B256, U256};
use alloy_provider::Provider;
use alloy_provider::RootProvider;
use alloy_transport::BoxTransport;
use std::sync::Arc;

// Complete ERC-1967 Proxy Storage Slots detector
// - Upgraded(address indexed implementation)
// - ImplementationUpgraded(address indexed newImplementation)  
// - AdminChanged(address previousAdmin, address newAdmin)
// - BeaconUpgraded(address indexed beacon)

pub struct ProxyUpgradeAction {
    provider: Arc<RootProvider<BoxTransport>>,
}

impl ProxyUpgradeAction {
    pub fn new(provider: Arc<RootProvider<BoxTransport>>) -> Self {
        Self { provider }
    }
}

// ERC-1967 Standard Storage Slots
fn eip1967_implementation_slot() -> B256 {
    // keccak256("eip1967.proxy.implementation") - 1
    // 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc
    B256::from_slice(
        &hex::decode("360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc").unwrap()
    )
}

fn eip1967_admin_slot() -> B256 {
    // keccak256("eip1967.proxy.admin") - 1
    // 0xb53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103
    B256::from_slice(
        &hex::decode("b53127684a568b3173ae13b9f8a6016e243e63b6e8ee1178d6a717850b5d6103").unwrap()
    )
}

fn eip1967_beacon_slot() -> B256 {
    // keccak256("eip1967.proxy.beacon") - 1
    // 0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50
    B256::from_slice(
        &hex::decode("a3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50").unwrap()
    )
}

fn eip1967_rollback_slot() -> B256 {
    // keccak256("eip1967.proxy.rollback") - 1 
    // 0x4910fdfa16fed3260ed0e7147f7cc6da11a60208b5b9406d12a635614ffd9143
    B256::from_slice(
        &hex::decode("4910fdfa16fed3260ed0e7147f7cc6da11a60208b5b9406d12a635614ffd9143").unwrap()
    )
}

fn right_most_20(bytes: &[u8]) -> Address {
    let mut a = [0u8; 20];
    a.copy_from_slice(&bytes[12..32]);
    Address::from(a)
}

impl Action for ProxyUpgradeAction {
    fn on_event(&self, e: &EventRecord) -> Result<()> {
        if let Some(name) = &e.name {
            match name.as_str() {
                "Upgraded" | "ImplementationUpgraded" => {
                    self.handle_implementation_upgrade(e);
                }
                "AdminChanged" => {
                    self.handle_admin_change(e);
                }
                "BeaconUpgraded" => {
                    self.handle_beacon_upgrade(e);
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl ProxyUpgradeAction {
    fn handle_implementation_upgrade(&self, e: &EventRecord) {
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
            // Read all ERC-1967 slots for comprehensive proxy state
            let impl_slot_u256 = U256::from_be_slice(eip1967_implementation_slot().as_slice());
            let admin_slot_u256 = U256::from_be_slice(eip1967_admin_slot().as_slice());
            let beacon_slot_u256 = U256::from_be_slice(eip1967_beacon_slot().as_slice());
            
            throttle::acquire().await;
            
            // Read implementation slot
            let onchain_impl = match provider.get_storage_at(proxy, impl_slot_u256).await {
                Ok(bytes) => {
                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                    Some(right_most_20(&be))
                }
                Err(_) => None,
            };
            
            // Read admin slot 
            let onchain_admin = match provider.get_storage_at(proxy, admin_slot_u256).await {
                Ok(bytes) => {
                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                    let addr = right_most_20(&be);
                    if addr == Address::ZERO { None } else { Some(addr) }
                }
                Err(_) => None,
            };
            
            // Read beacon slot
            let onchain_beacon = match provider.get_storage_at(proxy, beacon_slot_u256).await {
                Ok(bytes) => {
                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                    let addr = right_most_20(&be);
                    if addr == Address::ZERO { None } else { Some(addr) }
                }
                Err(_) => None,
            };
            
            println!(
                "[proxy-upgrade] proxy={} new_impl={:?} onchain_impl={:?} admin={:?} beacon={:?} tx={:?} block={:?}",
                proxy, impl_addr, onchain_impl, onchain_admin, onchain_beacon, txh, bn
            );
        });
    }
    
    fn handle_admin_change(&self, e: &EventRecord) {
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
            let admin_slot_u256 = U256::from_be_slice(eip1967_admin_slot().as_slice());
            throttle::acquire().await;
            
            let onchain_admin = match provider.get_storage_at(proxy, admin_slot_u256).await {
                Ok(bytes) => {
                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                    let addr = right_most_20(&be);
                    if addr == Address::ZERO { None } else { Some(addr) }
                }
                Err(_) => None,
            };
            
            println!(
                "[proxy-admin-changed] proxy={} prev={:?} new={:?} onchain_admin={:?} tx={:?} block={:?}",
                proxy, prev, newa, onchain_admin, txh, bn
            );
        });
    }
    
    fn handle_beacon_upgrade(&self, e: &EventRecord) {
        let mut beacon_addr = None;
        for f in &e.fields {
            let key = f.name.to_lowercase();
            if key.contains("beacon") {
                beacon_addr = Some(format!("{:?}", f.value));
                break;
            }
        }
        
        let provider = self.provider.clone();
        let proxy = e.address;
        let txh = e.tx_hash;
        let bn = e.block_number;
        
        tokio::spawn(async move {
            let beacon_slot_u256 = U256::from_be_slice(eip1967_beacon_slot().as_slice());
            throttle::acquire().await;
            
            let onchain_beacon = match provider.get_storage_at(proxy, beacon_slot_u256).await {
                Ok(bytes) => {
                    let be: [u8; 32] = bytes.to_be_bytes::<32>();
                    let addr = right_most_20(&be);
                    if addr == Address::ZERO { None } else { Some(addr) }
                }
                Err(_) => None,
            };
            
            println!(
                "[proxy-beacon-upgrade] proxy={} new_beacon={:?} onchain_beacon={:?} tx={:?} block={:?}",
                proxy, beacon_addr, onchain_beacon, txh, bn
            );
        });
    }
}

use crate::error::Result;
use serde::{Deserialize, Serialize};

use crate::abi::{DecodedField, DecodedValue};
use alloy_primitives::{Address, B256, U256};

#[derive(Debug, Clone)]
pub struct SimpleLog {
    pub address: Address,
    pub topics: Vec<B256>,
    pub data: Vec<u8>,
    pub log_index: Option<u64>,
    pub removed: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub address: Address,
    pub tx_hash: Option<B256>,
    pub block_number: Option<u64>,
    pub topic0: Option<B256>,
    pub name: Option<String>,
    pub fields: Vec<DecodedField>,
    pub tx_index: Option<u64>,
    pub log_index: Option<u64>,
    pub topics: Vec<B256>,
    pub removed: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct TxRecord {
    pub hash: B256,
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub input_selector: Option<[u8; 4]>,
    pub func_name: Option<String>,
    pub func_args: Vec<DecodedValue>,
    // receipt/fee info
    pub gas: Option<u64>,
    pub gas_price: Option<U256>,
    pub effective_gas_price: Option<U256>,
    pub status: Option<u64>,
    pub gas_used: Option<u64>,
    pub cumulative_gas_used: Option<u64>,
    pub block_number: Option<u64>,
    pub tx_index: Option<u64>,
    pub contract_address: Option<Address>,
    pub receipt_logs: Option<Vec<SimpleLog>>,
}

#[derive(Debug, Clone)]
pub struct BlockRecord {
    pub number: u64,
}

#[derive(Debug, Clone)]
pub struct ContractCreationRecord {
    pub tx_hash: B256,
    pub contract_address: Address,
    pub deployer: Address,
    pub block_number: u64,
    pub tx_index: u64,
    pub gas_used: Option<u64>,
    pub constructor_args: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TxLite {
    pub hash: alloy_primitives::B256,
    #[serde(default)]
    pub to: Option<alloy_primitives::Address>,
}

pub trait Action: Send + Sync {
    fn on_event(&self, _e: &EventRecord) -> Result<()> {
        Ok(())
    }
    fn on_tx(&self, _t: &TxRecord) -> Result<()> {
        Ok(())
    }
    fn on_block(&self, _b: &BlockRecord) -> Result<()> {
        Ok(())
    }
    fn on_contract_creation(&self, _c: &ContractCreationRecord) -> Result<()> {
        Ok(())
    }
}

pub struct ActionSet {
    pub list: Vec<Box<dyn Action>>,
}

impl ActionSet {
    pub fn new() -> Self {
        Self { list: vec![] }
    }
    pub fn add<A: Action + 'static>(&mut self, a: A) {
        self.list.push(Box::new(a));
    }
    pub fn on_event(&self, e: &EventRecord) {
        for a in &self.list {
            let _ = a.on_event(e);
        }
    }
    pub fn on_tx(&self, t: &TxRecord) {
        for a in &self.list {
            let _ = a.on_tx(t);
        }
    }
    pub fn on_block(&self, b: &BlockRecord) {
        for a in &self.list {
            let _ = a.on_block(b);
        }
    }
    pub fn on_contract_creation(&self, c: &ContractCreationRecord) {
        for a in &self.list {
            let _ = a.on_contract_creation(c);
        }
    }
}

pub mod deployment;
pub mod jsonlog;
pub mod logging;
pub mod ownership;
pub mod proxy;
pub mod tornado;
pub mod transfer;
pub mod large_transfer;
pub mod initscan;
pub mod history_init_scan;
pub mod selector_scan;
pub mod history_tx_scan;
pub mod db_log;

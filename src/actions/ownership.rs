use super::{Action, EventRecord};
use crate::error::Result;
use alloy_primitives::Address;
pub struct OwnershipAction;

impl Action for OwnershipAction {
    fn on_event(&self, e: &EventRecord) -> Result<()> {
        if e.name.as_deref() != Some("OwnershipTransferred") {
            return Ok(());
        }

        let mut previous_owner: Option<Address> = None;
        let mut new_owner: Option<Address> = None;

        for field in &e.fields {
            match field.name.as_str() {
                "previousOwner" => {
                    if let crate::abi::DecodedValue::Address(addr) = field.value {
                        previous_owner = Some(addr);
                    }
                }
                "newOwner" => {
                    if let crate::abi::DecodedValue::Address(addr) = field.value {
                        new_owner = Some(addr);
                    }
                }
                _ => {}
            }
        }

        println!(
            "[ownership] contract={} previous={:?} new={:?} tx={:?} block={:?}",
            e.address, previous_owner, new_owner, e.tx_hash, e.block_number
        );

        Ok(())
    }
}

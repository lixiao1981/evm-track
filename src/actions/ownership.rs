use super::{Action, EventRecord};
use crate::error::Result;

pub struct OwnershipAction;

impl Action for OwnershipAction {
    fn on_event(&self, e: &EventRecord) -> Result<()> {
        if let Some(name) = &e.name {
            if name == "OwnershipTransferred" || name == "OwnershipTransfer" {
                let mut previous = None;
                let mut new_owner = None;
                for f in &e.fields {
                    let key = f.name.to_lowercase();
                    match key.as_str() {
                        "previousowner" | "previous_owner" | "from" => {
                            previous = Some(format!("{:?}", f.value))
                        }
                        "newowner" | "new_owner" | "to" => {
                            new_owner = Some(format!("{:?}", f.value))
                        }
                        _ => {}
                    }
                }
                println!(
                    "[ownership] contract={} previous={:?} new={:?}",
                    e.address, previous, new_owner
                );
            }
        }
        Ok(())
    }
}

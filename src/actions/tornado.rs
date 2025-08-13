use super::{Action, EventRecord};

// Minimal TornadoCash-like detector: looks for common Deposit/Withdrawal events
// This is heuristic and intended as a placeholder for full porting.
pub struct TornadoAction;

impl Action for TornadoAction {
    fn on_event(&self, e: &EventRecord) -> anyhow::Result<()> {
        if let Some(name) = &e.name {
            match name.as_str() {
                "Deposit" | "Deposited" => {
                    println!("[tornado] deposit addr={} tx={:?} block={:?}", e.address, e.tx_hash, e.block_number);
                }
                "Withdrawal" | "Withdraw" => {
                    println!("[tornado] withdrawal addr={} tx={:?} block={:?}", e.address, e.tx_hash, e.block_number);
                }
                _ => {}
            }
        }
        Ok(())
    }
}


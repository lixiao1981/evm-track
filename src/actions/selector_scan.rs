use super::{Action, TxRecord};
use alloy_primitives::hex;

#[derive(Clone)]
pub struct SelectorScanOptions {
    pub selector: [u8; 4],
    pub print_receipts: bool,
}

pub struct SelectorScanAction {
    opts: SelectorScanOptions,
}

impl SelectorScanAction {
    pub fn new(opts: SelectorScanOptions) -> Self { Self { opts } }
}

impl Action for SelectorScanAction {
    fn on_tx(&self, t: &TxRecord) -> anyhow::Result<()> {
        if let Some(sel) = t.input_selector {
            if sel == self.opts.selector {
                println!(
                    "[selector] hit selector=0x{} block={:?} tx=0x{} from={:?} to={:?}",
                    hex::encode(sel),
                    t.block_number,
                    hex::encode(t.hash),
                    t.from,
                    t.to
                );
                if self.opts.print_receipts {
                    if let Some(status) = t.status {
                        println!(
                            "[selector] receipt status={} gas_used={:?} logs={}",
                            status,
                            t.gas_used,
                            t.receipt_logs.as_ref().map(|v| v.len()).unwrap_or(0)
                        );
                    }
                }
            }
        }
        Ok(())
    }
}


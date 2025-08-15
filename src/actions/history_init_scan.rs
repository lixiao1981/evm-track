use std::sync::Arc;

use alloy_provider::{Provider, RootProvider};
use alloy_transport::BoxTransport;
use anyhow::Result;

use crate::{actions::{Action, ActionSet, BlockRecord}, cli::RangeFlags, runtime};

use super::initscan::{InitscanAction, InitscanOptions};

#[derive(Debug, Clone)]
pub struct HistoryInitScanOptions {
    pub from_block: u64,
    pub to_block: u64,
    pub initscan: InitscanOptions,
    pub progress_every: Option<u64>,
    pub progress_percent: Option<u64>,
}

/// Reuse the unified blocks pipeline: build an ActionSet with only Initscan
/// and run runtime::historical::run_blocks over the requested range.
pub async fn run(
    provider: Arc<RootProvider<BoxTransport>>,
    opts: HistoryInitScanOptions,
) -> Result<()> {
    let prov_arc = Arc::new(provider.as_ref().clone());
    let mut set = ActionSet::new();
    set.add(ProgressAction::new_with_tick(
        opts.from_block,
        opts.to_block,
        opts.progress_every,
        opts.progress_percent,
    ));
    set.add(InitscanAction::new(prov_arc.clone(), opts.initscan.clone()));
    let set = Arc::new(set);
    let range = RangeFlags {
        config: None,
        from_block: opts.from_block,
        to_block: Some(opts.to_block),
        step_blocks: 1,
    };
    runtime::historical::run_blocks(provider.as_ref().clone(), vec![], &range, Some(set)).await
}

// Simple block progress reporter action
struct ProgressAction {
    from: u64,
    to: u64,
    total: u64,
    processed: std::sync::Mutex<u64>,
    tick: u64,
}

impl ProgressAction {
    fn new_with_tick(from: u64, to: u64, every: Option<u64>, percent: Option<u64>) -> Self {
        let total = to.saturating_sub(from).saturating_add(1);
        let tick = if let Some(n) = every {
            n.max(1)
        } else if let Some(p) = percent {
            ((total.saturating_mul(p.max(1))) / 100).max(1)
        } else {
            (total / 100).max(1)
        };
        println!(
            "[initscan] starting historical scan: from={} to={} total={} blocks",
            from, to, total
        );
        Self { from, to, total, processed: std::sync::Mutex::new(0), tick }
    }
}

impl Action for ProgressAction {
    fn on_block(&self, b: &BlockRecord) -> anyhow::Result<()> {
        let mut p = self.processed.lock().unwrap();
        *p = p.saturating_add(1);
        if *p % self.tick == 0 || *p == self.total {
            let pct = (*p as f64 / self.total as f64) * 100.0;
            println!(
                "[initscan] block progress: {}/{} ({:.0}%) current={}",
                *p, self.total, pct, b.number
            );
        }
        Ok(())
    }
}

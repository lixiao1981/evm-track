use once_cell::sync::OnceCell;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::time::{interval, Duration};

static THROTTLE: OnceCell<Option<Arc<Throttle>>> = OnceCell::new();

pub struct Throttle {
    sem: Arc<Semaphore>,
    capacity: u32,
}

impl Throttle {
    fn new(capacity: u32) -> Arc<Self> {
        Arc::new(Self {
            sem: Arc::new(Semaphore::new(capacity as usize)),
            capacity,
        })
    }
}

/// Initialize global throttle with max requests per second.
/// 0 or missing disables throttling.
pub fn init(max_rps: u32) {
    // If already set, do nothing.
    if THROTTLE.get().is_some() {
        return;
    }
    if max_rps == 0 {
        let _ = THROTTLE.set(None);
        return;
    }
    let thr = Throttle::new(max_rps);
    let sem = thr.sem.clone();
    let cap = thr.capacity;
    // Refill task: every 1s, top-up permits back to capacity.
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            let available = sem.available_permits() as u32;
            if available < cap {
                let add = (cap - available) as usize;
                sem.add_permits(add);
            }
        }
    });
    let _ = THROTTLE.set(Some(thr));
}

/// Acquire one permit if throttling enabled.
pub async fn acquire() {
    if let Some(Some(t)) = THROTTLE.get() {
        // Acquire one permit and forget it, consuming capacity until下一次补充。
        if let Ok(permit) = t.sem.acquire().await {
            permit.forget();
        }
    }
}

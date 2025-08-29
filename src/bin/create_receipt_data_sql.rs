//! A standalone binary to fetch full transaction receipts and store them in PostgreSQL.
//! This version is robust, interruptible, and uses multiple nodes for fetching.

use anyhow::Result;
use evm_track::{db, provider};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::env;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use alloy_provider::{Provider, RootProvider};
use alloy_primitives::B256;
use alloy_transport::BoxTransport;
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load configuration
    dotenv::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let node_list_path = "data/node_list.json";

    const NUM_WORKERS: u32 = 10;
    const BATCH_SIZE: i64 = 100;

    println!("Starting robust receipt fetching process...");

    // 2. Connect to Database
    let db = db::connect(&db_url).await?;
    println!("Successfully connected to database.");

    // 3. Connect to all WebSocket providers
    let node_urls: Vec<String> = serde_json::from_str(&fs::read_to_string(node_list_path)?)?;
    if node_urls.is_empty() {
        panic!("Node list file is empty!");
    }
    let mut providers = Vec::new();
    for url in &node_urls {
        let provider = provider::connect_auto(url).await?;
        providers.push(Arc::new(provider));
    }
    let shared_providers = Arc::new(providers);
    let round_robin_counter = Arc::new(AtomicUsize::new(0));
    println!("Successfully connected to {} RPC nodes.", shared_providers.len());

    // 4. Prepare database tables
    db::create_receipts_table(&db.pool).await?;
    let stuck_jobs = db::reset_stuck_jobs(&db.pool).await?;
    if stuck_jobs > 0 {
        println!("Reset {} stuck jobs from previous run.", stuck_jobs);
    }

    // 5. Setup Progress Bar
    let total_pending = db::count_pending_jobs(&db.pool).await?;
    if total_pending == 0 {
        println!("No pending jobs to process. Exiting.");
        return Ok(());
    }
    let pb = ProgressBar::new(total_pending as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
            .progress_chars("##-"),
    );

    // 6. Set up and run parallel workers
    let mut tasks = JoinSet::new();
    for i in 0..NUM_WORKERS {
        let pool = db.pool.clone();
        let providers = Arc::clone(&shared_providers);
        let counter = Arc::clone(&round_robin_counter);
        let pb = pb.clone();

        tasks.spawn(async move {
            loop {
                // Atomically claim a batch of jobs
                let hashes = match db::claim_batch_for_processing(&pool, BATCH_SIZE).await {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("Worker {}: DB error claiming batch: {}. Retrying...", i, e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if hashes.is_empty() {
                    break; // No more work
                }

                for hash_str in &hashes {
                    let tx_hash = B256::from_str(hash_str).unwrap();

                    // Select a provider in round-robin fashion
                    let provider_index = counter.fetch_add(1, Ordering::SeqCst) % providers.len();
                    let provider = &providers[provider_index];

                    match provider.get_transaction_receipt(tx_hash).await {
                        Ok(Some(receipt)) => {
                            if let Err(e) = db::insert_receipt(&pool, &receipt).await {
                                eprintln!("Worker {}: DB error inserting receipt {}: {}", i, hash_str, e);
                            }
                        }
                        Ok(None) => { /* Tx not found or pending, will be retried later if status is not updated */ }
                        Err(e) => eprintln!("Worker {}: RPC error for hash {}: {}", i, hash_str, e),
                    }

                    // Mark job as complete regardless of outcome to avoid retrying failed RPC calls indefinitely.
                    // A more complex system could use a different status for RPC errors.
                    if let Err(e) = db::set_job_status(&pool, hash_str, 2).await {
                         eprintln!("Worker {}: DB error setting status for hash {}: {}", i, hash_str, e);
                    }
                }
                pb.inc(hashes.len() as u64);
            }
        });
    }

    while let Some(res) = tasks.join_next().await {
        if let Err(e) = res {
            eprintln!("A worker task panicked: {}", e);
        }
    }

    pb.finish_with_message("All jobs processed!");

    Ok(())
}

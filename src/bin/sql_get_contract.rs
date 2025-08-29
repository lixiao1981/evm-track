//! A standalone binary to find contract creation transactions and update the database.
//! This version is robust against interruptions.

use anyhow::Result;
use evm_track::{db, provider};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::Row;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use alloy_provider::Provider;
use alloy_primitives::B256;
use std::str::FromStr;

/// The main function for the robust contract discovery program.
#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load configuration from .env file
    dotenv::dotenv().ok();
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let rpc_url = env::var("RPC_URL").expect("RPC_URL for a node must be set");

    const NUM_WORKERS: u32 = 8;
    const BATCH_SIZE: i64 = 100;

    println!("Starting robust contract discovery process...");

    // 2. Connect to services
    let db = db::connect(&db_url).await?;
    let provider = Arc::new(provider::connect_auto(&rpc_url).await?);
    println!("Successfully connected to database and RPC node.");

    // 3. Reset any jobs that were stuck in 'processing' state from a previous run
    let stuck_jobs = db::reset_stuck_jobs(&db.pool).await?;
    if stuck_jobs > 0 {
        println!("Reset {} stuck jobs from previous run.", stuck_jobs);
    }

    // 4. Get total number of pending jobs for progress bar
    let total_pending = db::count_pending_jobs(&db.pool).await?;
    if total_pending == 0 {
        println!("No pending jobs to process. Exiting.");
        return Ok(());
    }
    println!("Found {} pending transactions to check.", total_pending);

    let pb = ProgressBar::new(total_pending as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")?
            .progress_chars("##-"),
    );

    // 5. Set up and run parallel workers
    let mut tasks = JoinSet::new();
    for i in 0..NUM_WORKERS {
        let pool = db.pool.clone();
        let provider = Arc::clone(&provider);
        let pb = pb.clone();

        tasks.spawn(async move {
            loop {
                // Atomically claim a batch of jobs
                let hashes = match db::claim_batch_for_processing(&pool, BATCH_SIZE).await {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("Worker {} DB error claiming batch: {}. Retrying in 5s...", i, e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if hashes.is_empty() {
                    // No more work to claim, this worker is done.
                    break;
                }

                let num_in_batch = hashes.len();

                // Process each hash in the claimed batch
                for hash_str in &hashes {
                    let tx_hash = match B256::from_str(hash_str) {
                        Ok(h) => h,
                        Err(_) => continue, // Skip if hash is invalid
                    };

                    let receipt_result = provider.get_transaction_receipt(tx_hash).await;

                    let contract_address = match receipt_result {
                        Ok(Some(receipt)) => receipt.contract_address.map(|a| format!("{:?}", a)),
                        Ok(None) => None, // No receipt found
                        Err(e) => {
                            eprintln!("Worker {}: RPC error for hash {}: {}", i, hash_str, e);
                            None // Treat RPC errors as if no address was found
                        }
                    };

                    // Mark the job as complete, saving the address if found.
                    if let Err(e) = db::set_contract_job_complete(&pool, hash_str, contract_address).await {
                        eprintln!("Worker {} failed to update DB for hash {}: {}", i, hash_str, e);
                    }
                }
                // Update progress bar
                pb.inc(hashes.len() as u64);
            }
        });
    }

    // Wait for all workers to finish
    while let Some(res) = tasks.join_next().await {
        if let Err(e) = res {
            eprintln!("A worker task panicked: {}", e);
        }
    }

    pb.finish_with_message("All jobs processed!");

    Ok(())
}
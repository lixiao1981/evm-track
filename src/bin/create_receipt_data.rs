//! A standalone binary to fetch full transaction receipts for hashes from a file.

use anyhow::Result;
use evm_track::provider;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::env;
use std::sync::Arc;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

use alloy_provider::Provider;
use alloy_primitives::B256;
use alloy_rpc_types::TransactionReceipt;
use std::str::FromStr;

// A simplified TxLite struct to parse the input file.
#[derive(Debug, Clone, Deserialize)]
pub struct TxLite {
    pub hash: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup: Configuration and file paths
    dotenv::dotenv().ok();
    let rpc_url = env::var("RPC_URL").expect("RPC_URL for a node must be set");
    let input_file_path = "data/null.json";
    let output_file_path = "data/create_receipt.json";

    const NUM_WORKERS: usize = 10;
    const CHANNEL_BUFFER_SIZE: usize = 200; // How many items can be in-flight

    println!("Starting receipt fetching process...");

    // 2. Prepare connections and files
    let provider = Arc::new(provider::connect_auto(&rpc_url).await?);
    let output_file = OpenOptions::new().create(true).write(true).truncate(true).open(output_file_path).await?;
    let writer = BufWriter::new(output_file);

    // 3. Setup communication channels
    let (hash_tx, hash_rx) = mpsc::channel::<B256>(CHANNEL_BUFFER_SIZE);
    let (receipt_tx, mut receipt_rx) = mpsc::channel::<TransactionReceipt>(CHANNEL_BUFFER_SIZE);
    let shared_hash_rx = Arc::new(Mutex::new(hash_rx));

    // 4. Setup Progress Bar
    println!("Counting total lines in file...");
    let mut line_count = 0u64;
    let mut stream = LinesStream::new(BufReader::new(File::open(input_file_path).await?).lines());
    while stream.next().await.is_some() {
        line_count += 1;
    }
    let total_lines = line_count;
    println!("Found {} transactions to process.", total_lines);

    let pb = ProgressBar::new(total_lines);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("##-"),
    );

    // --- Task Spawning ---

    // 5. Spawn the single WRITER task
    let writer_pb = pb.clone();
    let writer_handle = tokio::spawn(async move {
        let mut file_writer = writer;
        while let Some(receipt) = receipt_rx.recv().await {
            match serde_json::to_string(&receipt) {
                Ok(json_line) => {
                    if let Err(e) = file_writer.write_all(json_line.as_bytes()).await {
                        eprintln!("Error writing to output file: {}", e);
                        break;
                    }
                    if let Err(e) = file_writer.write_all(b"\n").await {
                        eprintln!("Error writing newline: {}", e);
                        break;
                    }
                }
                Err(e) => eprintln!("Error serializing receipt: {}", e),
            }
            writer_pb.inc(1);
        }
        let _ = file_writer.flush().await;
    });

    // 6. Spawn multiple WORKER tasks (Consumers)
    let mut worker_handles = Vec::new();
    for i in 0..NUM_WORKERS {
        let rx = Arc::clone(&shared_hash_rx);
        let tx = receipt_tx.clone();
        let provider = Arc::clone(&provider);
        let handle = tokio::spawn(async move {
            loop {
                let mut rx_guard = rx.lock().await;
                let hash_option = rx_guard.recv().await;
                drop(rx_guard);

                if let Some(hash) = hash_option {
                    match provider.get_transaction_receipt(hash).await {
                        Ok(Some(receipt)) => {
                            if tx.send(receipt).await.is_err() {
                                break; // Channel closed, exit
                            }
                        }
                        Ok(None) => { /* Silently ignore, tx not found or pending */ }
                        Err(e) => eprintln!("Worker {}: RPC error for hash {}: {}", i, hash, e),
                    }
                } else {
                    break; // Channel closed, exit
                }
            }
        });
        worker_handles.push(handle);
    }
    drop(receipt_tx);

    // 7. Spawn the single PRODUCER task
    let producer_handle = tokio::spawn(async move {
        let file = File::open(input_file_path).await.unwrap();
        let reader = BufReader::new(file);
        let mut lines_stream = LinesStream::new(reader.lines());

        while let Some(Ok(line)) = lines_stream.next().await {
            match serde_json::from_str::<TxLite>(&line) {
                Ok(tx_lite) => {
                    // Safely strip "0x" prefix and parse into a B256 hash
                    let hash_str = tx_lite.hash.strip_prefix("0x").unwrap_or(&tx_lite.hash);
                    match B256::from_str(hash_str) {
                        Ok(hash) => {
                            if hash_tx.send(hash).await.is_err() {
                                break; // Channel closed
                            }
                        }
                        Err(_) => eprintln!("Failed to parse hash: {}", tx_lite.hash),
                    }
                }
                Err(_) => eprintln!("Failed to parse JSON line: {}", line),
            }
        }
    });

    // 8. Wait for all tasks to complete
    producer_handle.await?;
    for handle in worker_handles {
        handle.await?;
    }
    writer_handle.await?;

    pb.finish_with_message("Receipt fetching complete!");

    Ok(())
}

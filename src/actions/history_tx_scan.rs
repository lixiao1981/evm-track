use super::TxLite;
use crate::cli;
use anyhow::Result;
use futures::stream::StreamExt;
use std::io::{self, BufRead};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::fs::File as TokioFile;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::Mutex;
use tokio_stream::wrappers::LinesStream;
use tracing::{info, warn};

use alloy_provider::RootProvider;
use alloy_rpc_types::trace::geth::{
    GethDebugBuiltInTracerType, GethDebugTracerType, GethDebugTracingOptions,
};
use alloy_transport::BoxTransport;

async fn process_line(
    line: String,
    provider: Arc<RootProvider<BoxTransport>>,
    trace_options: GethDebugTracingOptions,
    writer: Arc<Mutex<BufWriter<TokioFile>>>,
) {
    match serde_json::from_str::<TxLite>(&line) {
        Ok(tx) => {
            // info!("Scanning tx: {}", tx.hash);
            match crate::provider::public_provider_get_transactions_trace(
                provider,
                tx.hash,
                trace_options,
            )
            .await {
                Ok(Some(trace)) => {
                    match serde_json::to_string(&trace) {
                        Ok(json_string) => {
                            let mut writer_guard = writer.lock().await;
                            if let Err(e) = writer_guard.write_all(json_string.as_bytes()).await {
                                warn!("Failed to write to output file: {}", e);
                            }
                            if let Err(e) = writer_guard.write_all(b"\n").await {
                                warn!("Failed to write newline: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Failed to serialize trace for {}: {}", tx.hash, e);
                        }
                    }
                }
                Ok(None) => {
                    // This can be noisy, so we comment it out.
                    // info!("No trace found for {}", tx.hash);
                }
                Err(e) => {
                    warn!("Failed to get trace for {}: {}", tx.hash, e);
                }
            }
        }
        Err(e) => {
            warn!("Failed to parse line: {}", e);
        }
    }
}

pub async fn run(
    provider: Arc<RootProvider<BoxTransport>>,
    cmd: &cli::HistoryTxScanCmd,
) -> Result<()> {
    info!("[history_tx_scan] starting");

    let total_lines = io::BufReader::new(std::fs::File::open("data/null.json")?).lines().count();
    info!("Total transactions to scan: {}", total_lines);

    let processed = Arc::new(AtomicUsize::new(0));
    let tick = if let Some(n) = cmd.progress_every {
        n as usize
    } else if let Some(p) = cmd.progress_percent {
        ((total_lines * p as usize) / 100).max(1)
    } else {
        (total_lines / 100).max(1)
    };

    let input_file = TokioFile::open("data/null.json").await?;
    let reader = BufReader::new(input_file);
    let lines_stream = LinesStream::new(reader.lines());

    let output_file = TokioFile::create("data/create_transactions_data.json").await?;
    let writer = Arc::new(Mutex::new(BufWriter::new(output_file)));

    let trace_options = GethDebugTracingOptions {
        tracer: Some(GethDebugTracerType::BuiltInTracer(
            GethDebugBuiltInTracerType::CallTracer,
        )),
        ..Default::default()
    };

    lines_stream
        .for_each_concurrent(cmd.concurrency, |line_result| {
            let provider = Arc::clone(&provider);
            let trace_options = trace_options.clone();
            let writer = Arc::clone(&writer);
            let processed = Arc::clone(&processed);

            async move {
                if let Ok(line) = line_result {
                    process_line(line, provider, trace_options, writer).await;
                } else if let Err(e) = line_result {
                    warn!("Failed to read line from input file: {}", e);
                }

                let current_processed = processed.fetch_add(1, Ordering::SeqCst) + 1;
                if tick > 0 && (current_processed % tick == 0 || current_processed == total_lines) {
                    let pct = (current_processed as f64 / total_lines as f64) * 100.0;
                    println!(
                        "[history-tx-scan] progress: {}/{} ({:.2}%)",
                        current_processed,
                        total_lines,
                        pct
                    );
                }
            }
        })
        .await;

    let mut writer_guard = writer.lock().await;
    if let Err(e) = writer_guard.flush().await {
        warn!("Failed to flush output file: {}", e);
    }

    info!("[history_tx_scan] finished");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider;
    use alloy_rpc_types::trace::geth::CallFrame;
    use std::io::{self, BufRead};

    #[tokio::test]
    #[ignore]
    async fn test_fetch_first_20_traces() {
        let rpcurl = "ws://192.168.2.58:8646";
        let provider =
            Arc::new(provider::connect_ws(rpcurl).await.expect("Failed to connect to provider"));

        let file = std::fs::File::open("data/null.json").expect("Failed to open null.json");
        let reader = io::BufReader::new(file);
        let lines: Vec<String> =
            reader.lines().take(20).collect::<Result<_, _>>().expect("Failed to read lines");

        let trace_options = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::BuiltInTracer(
                GethDebugBuiltInTracerType::CallTracer,
            )),
            ..Default::default()
        };

        for line in lines {
            let tx: TxLite = serde_json::from_str(&line).expect("Failed to parse TxLite");
            println!("Testing hash: {}", tx.hash);

            let result = crate::provider::public_provider_get_transactions_trace(
                Arc::clone(&provider),
                tx.hash,
                trace_options.clone(),
            )
            .await;

            assert!(result.is_ok(), "Failed to fetch trace for hash {}", tx.hash);
            let trace_opt = result.unwrap();
            assert!(trace_opt.is_some(), "Trace should not be None for hash {}", tx.hash);
            let trace = trace_opt.unwrap();
            println!(
                "Successfully fetched trace for {}: Type={}, From={}, To={{:?}}, Input={}",
                tx.hash,
                trace.typ,
                trace.from,
                trace.to,
                trace.input
            );
        }
    }
}
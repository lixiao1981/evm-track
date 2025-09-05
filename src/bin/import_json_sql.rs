//! A standalone binary to import transaction data from a JSON file into PostgreSQL.

use evm_track::db;
use evm_track::error::{AppError, Result, DbError};
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use sqlx::PgPool;
use std::env;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

// A simplified TxLite struct for this binary. It must match the JSON structure.
#[derive(Debug, Clone, Deserialize)]
pub struct TxLite {
    pub hash: String,
    #[serde(default)]
    pub to: Option<String>,
}

/// Creates the necessary table in the database to store the imported transactions.
async fn setup_table(pool: &PgPool) -> Result<()> {
    println!("Setting up 'imported_txs' table...");
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS imported_txs (
            hash TEXT PRIMARY KEY,
            to_address TEXT
        )
        "#
    )
    .execute(pool)
    .await
    .map_err(|e| AppError::Db(DbError(e)))?;
    println!("Table setup complete.");
    Ok(())
}

/// Inserts a batch of transactions into the database using a single query.
async fn batch_insert(pool: &PgPool, batch: &[TxLite]) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }

    // Use QueryBuilder to construct a bulk insert query
    let mut query_builder = sqlx::QueryBuilder::new("INSERT INTO imported_txs (hash, to_address) ");

    query_builder.push_values(batch.iter(), |mut b, tx| {
        b.push_bind(&tx.hash).push_bind(tx.to.as_ref());
    });

    // Add a conflict clause to ignore duplicates
    query_builder.push(" ON CONFLICT (hash) DO NOTHING");

    let query = query_builder.build();
    query.execute(pool).await.map_err(|e| AppError::Db(DbError::from(e)))?;

    Ok(())
}


#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load configuration from .env file
    dotenv::dotenv().ok();
    let db_url =
        env::var("DATABASE_URL").map_err(|_| AppError::Config("DATABASE_URL must be set".to_string()))?;
    let input_file_path = "data/null.json";
    const BATCH_SIZE: usize = 1000;

    println!(
        "Starting import from '{}' to PostgreSQL...",
        input_file_path
    );

    // 2. Connect to the database
    let db = db::connect(&db_url).await?;
    let pool = &db.pool;

    // 3. Ensure the table exists
    setup_table(pool).await?;

    // 4. Open the input file and prepare for streaming
    let file = File::open(input_file_path).await?;
    let reader = BufReader::new(file);
    let mut lines_stream = LinesStream::new(reader.lines());

    // Count total lines for the progress bar
    println!("Counting total lines in file for progress bar...");
    let mut line_count = 0u64;
    let mut stream = LinesStream::new(BufReader::new(File::open(input_file_path).await?).lines());
    while stream.next().await.is_some() {
        line_count += 1;
    }
    let total_lines = line_count;
    println!("Found {} lines to import.", total_lines);

    let pb = ProgressBar::new(total_lines as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut count = 0;

    // 5. Process the file line by line
    while let Some(line_result) = lines_stream.next().await {
        let line = line_result?;
        match serde_json::from_str::<TxLite>(&line) {
            Ok(tx) => {
                batch.push(tx);
                if batch.len() >= BATCH_SIZE {
                    batch_insert(pool, &batch).await?;
                    batch.clear();
                }
            }
            Err(e) => {
                eprintln!(
                    "Warning: Failed to parse line, skipping. Error: {}, Line: {}",
                    e, line
                );
            }
        }
        count += 1;
        pb.set_position(count);
    }

    // 6. Insert any remaining transactions in the last batch
    if !batch.is_empty() {
        batch_insert(pool, &batch).await?;
    }

    pb.finish_with_message("Import complete!");

    println!("Successfully imported {} records.", count);

    Ok(())
}

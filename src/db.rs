use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::time::Duration;
use alloy_rpc_types::TransactionReceipt;
use alloy_network_primitives::ReceiptResponse;

/// A clonable struct that holds a PostgreSQL connection pool.
#[derive(Clone)]
pub struct Db {
    pub pool: PgPool,
}

/// Creates a new database connection pool.
pub async fn connect(database_url: &str) -> Result<Db> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await?;
    Ok(Db { pool })
}

// --- Robust Job Queue Functions for `imported_txs` table ---

/// Resets jobs that were stuck in a 'processing' state (e.g., from a previous crash).
pub async fn reset_stuck_jobs(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("UPDATE imported_txs SET status = 0 WHERE status = 1")
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Counts the total number of jobs that are yet to be processed.
pub async fn count_pending_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM imported_txs WHERE status = 0")
        .fetch_one(pool)
        .await?;
    Ok(row.0)
}

/// Atomically claims a batch of jobs by marking their status as 'processing'
/// and returns the hashes of the claimed jobs.
pub async fn claim_batch_for_processing(
    pool: &PgPool,
    batch_size: i64,
) -> Result<Vec<String>, sqlx::Error> {
    let hashes = sqlx::query(
        r#"
        UPDATE imported_txs
        SET status = 1
        WHERE hash IN (
            SELECT hash
            FROM imported_txs
            WHERE status = 0
            ORDER BY hash
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING hash;
        "#,
    )
    .bind(batch_size)
    .map(|row: sqlx::postgres::PgRow| row.get("hash"))
    .fetch_all(pool)
    .await?;
    Ok(hashes)
}

/// Updates the status of a job in the `imported_txs` table.
pub async fn set_job_status(pool: &PgPool, hash: &str, status: i16) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE imported_txs SET status = $1 WHERE hash = $2")
        .bind(status)
        .bind(hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// Specifically for the sql_get_contract binary, marks a job as complete and sets the address.
pub async fn set_contract_job_complete(
    pool: &PgPool,
    hash: &str,
    contract_address: Option<String>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE imported_txs SET status = 2, contract_address = $1 WHERE hash = $2")
        .bind(contract_address)
        .bind(hash)
        .execute(pool)
        .await?;
    Ok(())
}


// --- Functions for `transaction_receipts` table ---

/// A struct that maps directly to the `transaction_receipts` table schema.
#[derive(Debug, sqlx::FromRow)]
pub struct DbReceipt {
    transaction_hash: String,
    transaction_index: Option<i64>,
    block_hash: Option<String>,
    block_number: Option<i64>,
    from_address: String,
    to_address: Option<String>,
    cumulative_gas_used: String,
    gas_used: String,
    contract_address: Option<String>,
    status: bool,
    effective_gas_price: String,
}

impl<'a> From<&'a TransactionReceipt> for DbReceipt {
    fn from(receipt: &'a TransactionReceipt) -> Self {
        Self {
            transaction_hash: format!("{:?}", receipt.transaction_hash),
            transaction_index: receipt.transaction_index.map(|idx| idx as i64),
            block_hash: receipt.block_hash.map(|h| format!("{:?}", h)),
            block_number: receipt.block_number.map(|n| n as i64),
            from_address: format!("{:?}", receipt.from),
            to_address: receipt.to.map(|a| format!("{:?}", a)),
            cumulative_gas_used: receipt.cumulative_gas_used().to_string(),
            gas_used: receipt.gas_used.to_string(),
            contract_address: receipt.contract_address.map(|a| format!("{:?}", a)),
            status: receipt.status(),
            effective_gas_price: receipt.effective_gas_price.to_string(),
        }
    }
}

/// Creates the `transaction_receipts` table if it doesn't exist.
pub async fn create_receipts_table(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transaction_receipts (
            transaction_hash TEXT PRIMARY KEY,
            transaction_index BIGINT,
            block_hash TEXT,
            block_number BIGINT,
            from_address TEXT NOT NULL,
            to_address TEXT,
            cumulative_gas_used TEXT NOT NULL,
            gas_used TEXT NOT NULL,
            contract_address TEXT,
            status BOOLEAN NOT NULL,
            effective_gas_price TEXT NOT NULL
        );
        "#
    ).execute(pool).await?;
    Ok(())
}

/// Inserts or updates a transaction receipt in the database.
pub async fn insert_receipt(pool: &PgPool, receipt: &TransactionReceipt) -> Result<(), sqlx::Error> {
    let db_receipt = DbReceipt::from(receipt);

    sqlx::query(
        r#"
        INSERT INTO transaction_receipts (
            transaction_hash, transaction_index, block_hash, block_number, from_address, to_address,
            cumulative_gas_used, gas_used, contract_address, status, effective_gas_price
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (transaction_hash) DO UPDATE SET
            transaction_index = EXCLUDED.transaction_index,
            block_hash = EXCLUDED.block_hash,
            block_number = EXCLUDED.block_number,
            from_address = EXCLUDED.from_address,
            to_address = EXCLUDED.to_address,
            cumulative_gas_used = EXCLUDED.cumulative_gas_used,
            gas_used = EXCLUDED.gas_used,
            contract_address = EXCLUDED.contract_address,
            status = EXCLUDED.status,
            effective_gas_price = EXCLUDED.effective_gas_price;
        "#
    )
    .bind(db_receipt.transaction_hash)
    .bind(db_receipt.transaction_index)
    .bind(db_receipt.block_hash)
    .bind(db_receipt.block_number)
    .bind(db_receipt.from_address)
    .bind(db_receipt.to_address)
    .bind(db_receipt.cumulative_gas_used)
    .bind(db_receipt.gas_used)
    .bind(db_receipt.contract_address)
    .bind(db_receipt.status)
    .bind(db_receipt.effective_gas_price)
    .execute(pool)
    .await?;

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    #[ignore] // This test requires a running PostgreSQL database and a DATABASE_URL env var.
    async fn test_db_connection() {
        dotenv::dotenv().ok();
        let db_url = env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set to run this test");
        let db_result = connect(&db_url).await;
        assert!(db_result.is_ok(), "Failed to connect to the database");
        let db = db_result.unwrap();
        let conn_result = db.pool.acquire().await;
        assert!(conn_result.is_ok(), "Failed to acquire a connection from the pool");
    }
}

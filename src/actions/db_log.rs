use crate::db::Db;
use anyhow::Result;
use serde_json::Value;
use chrono::{DateTime, Utc};

/// Represents a transaction log entry in the database.
pub struct TxLog {
    pub hash: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas: i64,
    pub gas_price: Option<i64>,
    pub max_fee_per_gas: Option<i64>,
    pub max_priority_fee_per_gas: Option<i64>,
    pub block_number: i64,
    pub timestamp: DateTime<Utc>,
    pub trace: Option<Value>, // For storing JSON trace data
}

/// Inserts a transaction log into the database.
///
/// This function demonstrates how to use the shared connection pool (`Db`)
/// to perform database operations. It uses `sqlx::query` which performs runtime
/// validation of the SQL query.
pub async fn log_tx(db: &Db, log: &TxLog) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO transactions (
            hash, from_address, to_address, value, gas, gas_price,
            max_fee_per_gas, max_priority_fee_per_gas, block_number, "timestamp", trace
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (hash) DO NOTHING
        "#,
    )
    .bind(&log.hash)
    .bind(&log.from_address)
    .bind(log.to_address.as_ref())
    .bind(&log.value)
    .bind(log.gas)
    .bind(log.gas_price)
    .bind(log.max_fee_per_gas)
    .bind(log.max_priority_fee_per_gas)
    .bind(log.block_number)
    .bind(log.timestamp)
    .bind(log.trace.as_ref())
    .execute(&db.pool)
    .await?;

    Ok(())
}

/// A function to create the necessary `transactions` table.
///
/// This should be called once during application setup.
pub async fn setup_db_table(db: &Db) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS transactions (
            hash TEXT PRIMARY KEY,
            from_address TEXT NOT NULL,
            to_address TEXT,
            value TEXT NOT NULL,
            gas BIGINT NOT NULL,
            gas_price BIGINT,
            max_fee_per_gas BIGINT,
            max_priority_fee_per_gas BIGINT,
            block_number BIGINT NOT NULL,
            "timestamp" TIMESTAMPTZ NOT NULL,
            trace JSONB,
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
        "#,
    )
    .execute(&db.pool)
    .await?;

    Ok(())
}
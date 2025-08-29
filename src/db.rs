use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::time::Duration;

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

// --- Robust Job Queue Functions ---

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

/// Marks a job as complete, setting its status and updating the contract address.
pub async fn mark_job_complete(
    pool: &PgPool,
    hash: &str,
    contract_address: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE imported_txs
        SET status = 2, contract_address = $1
        WHERE hash = $2
        "#,
    )
    .bind(contract_address)
    .bind(hash)
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
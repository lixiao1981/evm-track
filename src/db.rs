use anyhow::Result;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::time::Duration;

/// A clonable struct that holds a PostgreSQL connection pool.
#[derive(Clone)]
pub struct Db {
    pub pool: PgPool,
}

/// Creates a new database connection pool.
///
/// The pool is safe to share and clone across multiple async tasks.
///
/// # Arguments
///
/// * `database_url` - The connection string for the PostgreSQL database.
///
/// # Returns
///
/// A `Result` containing the `Db` struct if the connection is successful.
pub async fn connect(database_url: &str) -> Result<Db> {
    let pool = PgPoolOptions::new()
        .max_connections(20) // Set the maximum number of connections in the pool
        .acquire_timeout(Duration::from_secs(5)) // Set a timeout for acquiring a connection
        .connect(database_url)
        .await?;

    Ok(Db { pool })
}

// You can add more database-related functions here.
// For example, a function to run migrations or specific queries.

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    #[ignore] // This test requires a running PostgreSQL database and a DATABASE_URL env var.
    async fn test_db_connection() {
        // Ensure you have a .env file or export DATABASE_URL="postgres://user:pass@host/db"
        dotenv::dotenv().ok();
        let db_url = env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set to run this test");

        let db_result = connect(&db_url).await;
        assert!(db_result.is_ok(), "Failed to connect to the database");

        let db = db_result.unwrap();

        // Try to acquire a connection to confirm the pool is working.
        let conn_result = db.pool.acquire().await;
        assert!(conn_result.is_ok(), "Failed to acquire a connection from the pool");
    }
}

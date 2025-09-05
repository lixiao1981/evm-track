// Implement From<sqlx::Error> for DbError
impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        DbError(e)
    }
}
//_! Centralized error types for the application.

use thiserror::Error;
use alloy_transport::{RpcError, TransportErrorKind};

//
// Top-level Error for the application binary
//

/// Top-level error enum for the application.
#[derive(Error, Debug)]
pub enum AppError {
    /// Error related to configuration
    #[error("Configuration error: {0}")]
    Config(String),

    /// Error from an RPC Call
    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    /// Error related to database operations
    #[error("Database error: {0}")]
    Db(#[from] DbError),

    /// Error related to file I/O
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Error related to JSON serialization/deserialization
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Error related to a tokio task
    #[error("Task join error: {0}")]
    Join(#[from] tokio::task::JoinError),

    /// Error related to hex decoding
    #[error("Hex decoding error: {0}")]
    Hex(#[from] hex::FromHexError),

    /// Error from HTTP requests via reqwest
    #[error("HTTP request error: {0}")]
    Reqwest(#[from] reqwest::Error),

    /// Error from the progress bar
    #[error("Progress bar error: {0}")]
    Indicatif(#[from] indicatif::style::TemplateError),

    /// Error from the data collection source
    #[error("Data collection error")]
    Collect(#[from] CollectError),

    /// General error for anything that doesn't fit elsewhere
    #[error("An unexpected error occurred: {0}")]
    General(String),
}

/// A convenience `Result` type for the application.
pub type Result<T, E = AppError> = std::result::Result<T, E>;

//
// Data Collection Errors (`source.rs`)
//

/// Error related to data collection
#[derive(Error, Debug)]
pub enum CollectError {
    /// General Collection error
    #[error("Collect failed: {0}")]
    CollectError(String),

    /// Parse error
    #[error(transparent)]
    ParseError(#[from] ParseError),

    /// Error related to provider operations
    #[error("Provider error")]
    ProviderError(#[from] RpcError<TransportErrorKind>),

    /// Error related to tokio task
    #[error("Task failed: {0}")]
    TaskFailed(#[from] tokio::task::JoinError),

    /// Error related to too many requests
    #[error("Too many requests, try using a rate limit with --requests-per-second or limiting max concurrency with --max-concurrent-requests")]
    TooManyRequestsError,

    /// Generic RPC Error
    #[error("RPC call error: {0}")]
    RPCError(String),
}

/// return basic CollectError from str slice
pub fn err(message: &str) -> CollectError {
    CollectError::CollectError(message.to_string())
}

//
// Parsing Errors
//

/// Error related to parsing
#[derive(Error, Debug)]
pub enum ParseError {
    /// Error related to parsing
    #[error("Parsing error: {0}")]
    ParseError(String),

    /// Parse int error
    #[error("Parsing int error")]
    ParseIntError(#[from] std::num::ParseIntError),

    /// Parse url error
    #[error("Parsing url error")]
    ParseUrlError(#[from] url::ParseError),
}

/// A specific error struct for database-related issues.
/// Using a transparent wrapper around sqlx::Error.
#[derive(Error, Debug)]
#[error(transparent)]
pub struct DbError(pub sqlx::Error);
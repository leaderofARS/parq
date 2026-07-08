//! `parq-error` — Structured error types for the parq toolchain.
//!
//! All other crates in the workspace use these types via:
//! ```rust
//! use parq_error::ParqError;
//! ```

use thiserror::Error;

/// Master error enum for the entire parq pipeline.
#[derive(Error, Debug)]
pub enum ParqError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error on line {line}: {source}")]
    JsonParse {
        line: usize,
        source: serde_json::Error,
    },

    #[error("Schema inference failed: file has {rows} parseable records (need ≥ 1)")]
    InsufficientData { rows: usize },

    #[error("Type mismatch in field '{field}' on line {line}: expected {expected}, found {found}")]
    TypeMismatch {
        field: String,
        expected: String,
        found: String,
        line: usize,
    },

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("Parquet error: {0}")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("Configuration error: {0}")]
    Config(String),
}

//! Configuration error types

use std::path::PathBuf;
use thiserror::Error;

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Config file not found: {0}")]
    NotFound(PathBuf),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("Invalid value for '{field}': {message}")]
    Validation { field: String, message: String },

    #[error("Missing required field: {0}")]
    MissingField(String),
}

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("The selected location does not exist: {0}")]
    NotFound(String),
    #[error("The selected location is not accessible: {0}")]
    AccessDenied(String),
    #[error("The selected location is not a directory: {0}")]
    NotADirectory(String),
    #[error("Scan failed: {0}")]
    Io(#[from] io::Error),
    #[error("Scan was cancelled")]
    Cancelled,
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Could not create application data directory: {0}")]
    DataDir(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum FileOpError {
    #[error("{0}")]
    Invalid(String),
    #[error("Destination already exists: {0}")]
    AlreadyExists(String),
    #[error("Access denied: {0}")]
    AccessDenied(String),
    #[error("Protected location: {0}")]
    Protected(String),
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("Recycle Bin error: {0}")]
    Recycle(String),
}

pub type FileOpResult<T> = Result<T, FileOpError>;

use rusqlite;
use std::env;
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DecapodError {
    #[error("SQLite error: {0}")]
    RusqliteError(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),
    #[error("Failed to initialize database: {0}")]
    DatabaseInitializationError(String),
    #[error("Path error: {0}")]
    PathError(String),
    #[error("Environment variable error: {0}")]
    EnvVarError(#[from] env::VarError),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Not found: {0}")]
    NotFound(String),
}
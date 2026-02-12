//! Error types for Decapod operations.
//!
//! This module defines the canonical error type used throughout Decapod.
//! All subsystems return `Result<T, DecapodError>` for error handling.
//!
//! # For AI Agents
//!
//! - **Validation errors are fatal**: If `decapod validate` fails, do not proceed
//! - **NotFound vs NotImplemented**: NotFound = missing data, NotImplemented = unfinished feature
//! - **Error propagation**: Use `?` operator; errors auto-convert via `From` traits

use rusqlite;
use std::env;
use std::io;
use thiserror::Error;

/// Canonical error type for all Decapod operations.
///
/// Uses `thiserror` for automatic `Display` and `Error` trait implementations.
/// Many variants auto-convert from standard library errors via `#[from]`.
#[derive(Error, Debug)]
pub enum DecapodError {
    /// SQLite database error (auto-converts from `rusqlite::Error`)
    #[error("SQLite error: {0}")]
    RusqliteError(#[from] rusqlite::Error),

    /// I/O error (auto-converts from `std::io::Error`)
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Database initialization failure
    #[error("Failed to initialize database: {0}")]
    DatabaseInitializationError(String),

    /// Path resolution or validation error
    #[error("Path error: {0}")]
    PathError(String),

    /// Environment variable error (auto-converts from `std::env::VarError`)
    #[error("Environment variable error: {0}")]
    EnvVarError(#[from] env::VarError),

    /// Validation harness failure (proof gate, schema check, etc.)
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Resource not found (missing file, task, claim, etc.)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Feature not yet implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Context pack/archive error
    #[error("Context pack error: {0}")]
    ContextPackError(String),
}

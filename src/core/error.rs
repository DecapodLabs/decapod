//! Error types for Decapod operations.
//!
//! This module defines the canonical error type used throughout Decapod.
//! All subsystems return `Result<T, DecapodError>` for error handling.

use std::env;
use std::fmt;
use std::io;

/// Canonical error type for all Decapod operations.
#[derive(Debug)]
pub enum DecapodError {
    /// SQLite database error (auto-converts from `rusqlite::Error`)
    RusqliteError(rusqlite::Error),
    /// I/O error (auto-converts from `std::io::Error`)
    IoError(io::Error),
    /// Database initialization failure
    DatabaseInitializationError(String),
    /// Path resolution or validation error
    PathError(String),
    /// Environment variable error (auto-converts from `std::env::VarError`)
    EnvVarError(env::VarError),
    /// Validation harness failure (proof gate, schema check, etc.)
    ValidationError(String),
    /// Resource not found (missing file, task, claim, etc.)
    NotFound(String),
    /// Feature not yet implemented
    NotImplemented(String),
    /// Context pack/archive error
    ContextPackError(String),
    /// Session token error (not found, invalid, expired, etc.)
    SessionError(String),
}

impl fmt::Display for DecapodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RusqliteError(e) => write!(f, "SQLite error: {e}"),
            Self::IoError(e) => write!(f, "I/O error: {e}"),
            Self::DatabaseInitializationError(s) => write!(f, "Failed to initialize database: {s}"),
            Self::PathError(s) => write!(f, "Path error: {s}"),
            Self::EnvVarError(e) => write!(f, "Environment variable error: {e}"),
            Self::ValidationError(s) => write!(f, "Validation error: {s}"),
            Self::NotFound(s) => write!(f, "Not found: {s}"),
            Self::NotImplemented(s) => write!(f, "Not implemented: {s}"),
            Self::ContextPackError(s) => write!(f, "Context pack error: {s}"),
            Self::SessionError(s) => write!(f, "Session error: {s}"),
        }
    }
}

impl std::error::Error for DecapodError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RusqliteError(e) => Some(e),
            Self::IoError(e) => Some(e),
            Self::EnvVarError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for DecapodError {
    fn from(e: rusqlite::Error) -> Self {
        Self::RusqliteError(e)
    }
}

impl From<io::Error> for DecapodError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<env::VarError> for DecapodError {
    fn from(e: env::VarError) -> Self {
        Self::EnvVarError(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_display() {
        let err = DecapodError::ValidationError("test failed".to_string());
        assert_eq!(format!("{}", err), "Validation error: test failed");
    }

    #[test]
    fn test_not_found_error_display() {
        let err = DecapodError::NotFound("file.txt not found".to_string());
        assert_eq!(format!("{}", err), "Not found: file.txt not found");
    }

    #[test]
    fn test_not_implemented_error_display() {
        let err = DecapodError::NotImplemented("feature X".to_string());
        assert_eq!(format!("{}", err), "Not implemented: feature X");
    }

    #[test]
    fn test_session_error_display() {
        let err = DecapodError::SessionError("token expired".to_string());
        assert_eq!(format!("{}", err), "Session error: token expired");
    }

    #[test]
    fn test_path_error_display() {
        let err = DecapodError::PathError("invalid path".to_string());
        assert_eq!(format!("{}", err), "Path error: invalid path");
    }
}

//! Error types for Decapod operations.
//!
//! This module defines the canonical error type used throughout Decapod.
//! All subsystems return `Result<T, DecapodError>` for error handling.

use serde::Serialize;
use std::env;
use std::fmt;
use std::io;

/// Machine-readable outcomes for the provider-neutral cloud login handoff.
/// These values are part of the CLI contract; credential material is never
/// represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthStatus {
    AuthRequired,
    OnboardingPending,
    Expired,
    Unauthorized,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudAuthDiagnostic {
    pub schema_version: &'static str,
    pub status: CloudAuthStatus,
    pub message: String,
    pub next_action: String,
}

impl CloudAuthDiagnostic {
    pub fn new(
        status: CloudAuthStatus,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "decapod.cloud.auth.v1",
            status,
            message: message.into(),
            next_action: next_action.into(),
        }
    }
}

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
    /// Configuration error (config.toml parsing, etc.)
    Config(String),
    /// Context pack/archive error
    ContextPackError(String),
    /// Session token error (not found, invalid, expired, etc.)
    SessionError(String),
    /// A safe, actionable cloud authentication handoff result.
    CloudAuth(CloudAuthDiagnostic),
}

impl fmt::Display for DecapodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RusqliteError(e) => write!(f, "SQLite error: {e}"),
            Self::IoError(e) => {
                if e.kind() == std::io::ErrorKind::InvalidInput && e.to_string().contains("SUN_LEN")
                {
                    write!(
                        f,
                        "broker path workspace unavailable in this environment (socket path limitation)"
                    )
                } else {
                    write!(f, "I/O error: {e}")
                }
            }
            Self::DatabaseInitializationError(s) => write!(f, "Failed to initialize database: {s}"),
            Self::PathError(s) => write!(f, "Path error: {s}"),
            Self::EnvVarError(e) => write!(f, "Environment variable error: {e}"),
            Self::ValidationError(s) => {
                if let Some(msg) = s.strip_prefix("NEEDS_HUMAN_INPUT: ") {
                    write!(f, "context: NEEDS_HUMAN_INPUT: {msg}")
                } else if s.starts_with("NEEDS_HUMAN_INPUT") {
                    write!(f, "context: NEEDS_HUMAN_INPUT: execution needs human input")
                } else {
                    write!(f, "Validation error: {s}")
                }
            }
            Self::NotFound(s) => write!(f, "Not found: {s}"),
            Self::NotImplemented(s) => write!(f, "Not implemented: {s}"),
            Self::Config(s) => write!(f, "Configuration error: {s}"),
            Self::ContextPackError(s) => write!(f, "Context pack error: {s}"),
            Self::SessionError(s) => write!(f, "Session error: {s}"),
            Self::CloudAuth(diagnostic) => write!(
                f,
                "Cloud authentication {:?}: {}; next action: {}",
                diagnostic.status, diagnostic.message, diagnostic.next_action
            ),
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
#[path = "../../../tests/unit/core/error_tests.rs"]
mod tests;

//! Error types for Decapod operations.
//!
//! This module defines the canonical error type used throughout Decapod.
//! All subsystems return `Result<T, DecapodError>` for error handling.

use serde::Serialize;
use std::env;
use std::fmt;
use std::io;

/// Machine-readable outcomes for the provider-neutral cloud onboarding handoff.
/// These values are part of the CLI contract; credential material is never
/// represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthStatus {
    AuthRequired,
    Missing,
    OnboardingPending,
    Expired,
    RefreshFailed,
    Unauthorized,
    Revoked,
    UnauthorizedIdentity,
    RepositoryDenied,
    Offline,
    ProviderUnavailable,
    NonInteractive,
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

/// Backend-neutral classification for failures that may cross the storage
/// boundary. Dactyl supplies physical errors; Decapod retains retry,
/// validation, and governance policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageFailureKind {
    Contention,
    Io,
    Constraint,
    Query,
    Value,
    Capability,
    Unknown,
}

impl StorageFailureKind {
    /// Whether Decapod may retry the operation under its bounded policy.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Contention | Self::Io)
    }

    pub fn is_contention(self) -> bool {
        self == Self::Contention
    }
}

/// Canonical error type for all Decapod operations.
#[derive(Debug)]
pub enum DecapodError {
    /// Application storage error normalized by the Dactyl facade.
    StorageError(crate::core::db::Error),
    /// Dactyl physical storage error, normalized at the Decapod boundary.
    DactylError(dactyl_db::DactylError),
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
            Self::StorageError(e) => write!(f, "storage error: {e}"),
            Self::DactylError(e) => write!(f, "Dactyl error: {e}"),
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
            Self::StorageError(e) => Some(e),
            Self::DactylError(e) => Some(e),
            Self::IoError(e) => Some(e),
            Self::EnvVarError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::core::db::Error> for DecapodError {
    fn from(e: crate::core::db::Error) -> Self {
        Self::StorageError(e)
    }
}

impl From<dactyl_db::DactylError> for DecapodError {
    fn from(e: dactyl_db::DactylError) -> Self {
        Self::DactylError(e)
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
impl DecapodError {
    /// Classify storage failures without requiring callers to know the active
    /// backend. This is the application-side seam that dactyl can implement
    /// against later.
    pub fn storage_failure_kind(&self) -> StorageFailureKind {
        match self {
            Self::StorageError(err) => classify_storage_error(err),
            Self::DactylError(err) => classify_dactyl_error(err),
            Self::IoError(err)
                if err
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("disk i/o error") =>
            {
                StorageFailureKind::Io
            }
            Self::IoError(_) => StorageFailureKind::Unknown,
            Self::ValidationError(message) => classify_storage_message(message),
            _ => StorageFailureKind::Unknown,
        }
    }
}

fn classify_dactyl_error(err: &dactyl_db::DactylError) -> StorageFailureKind {
    use dactyl_db::AdapterErrorKind;

    match err {
        dactyl_db::DactylError::Adapter { kind, .. } => match kind {
            AdapterErrorKind::Busy | AdapterErrorKind::Locked | AdapterErrorKind::Timeout => {
                StorageFailureKind::Contention
            }
            AdapterErrorKind::Storage
            | AdapterErrorKind::Transport
            | AdapterErrorKind::Unavailable => StorageFailureKind::Io,
            AdapterErrorKind::Constraint
            | AdapterErrorKind::Conflict
            | AdapterErrorKind::VersionConflict
            | AdapterErrorKind::IdempotencyConflict
            | AdapterErrorKind::IdempotencyInProgress => StorageFailureKind::Constraint,
            AdapterErrorKind::Capability | AdapterErrorKind::ReadOnly => {
                StorageFailureKind::Capability
            }
            AdapterErrorKind::Value => StorageFailureKind::Value,
            AdapterErrorKind::Query | AdapterErrorKind::InvalidOperation => {
                StorageFailureKind::Query
            }
            AdapterErrorKind::TransactionAborted => StorageFailureKind::Query,
            AdapterErrorKind::Authentication
            | AdapterErrorKind::Authorization
            | AdapterErrorKind::RateLimited
            | AdapterErrorKind::Quota
            | AdapterErrorKind::NotFound
            | AdapterErrorKind::Cancellation
            | AdapterErrorKind::Protocol
            | AdapterErrorKind::Unknown => StorageFailureKind::Unknown,
        },
        dactyl_db::DactylError::Config(_) | dactyl_db::DactylError::UnsupportedOperation(_) => {
            StorageFailureKind::Capability
        }
        dactyl_db::DactylError::ColumnNotFound(_) | dactyl_db::DactylError::Conversion(_) => {
            StorageFailureKind::Value
        }
    }
}

fn classify_storage_error(err: &crate::core::db::Error) -> StorageFailureKind {
    match err {
        crate::core::db::Error::Dactyl(error) => classify_dactyl_error(error),
        crate::core::db::Error::QueryReturnedNoRows => StorageFailureKind::Query,
        crate::core::db::Error::FromSqlConversionFailure(_, _, _)
        | crate::core::db::Error::ToSqlConversionFailure(_)
        | crate::core::db::Error::InvalidColumnType(_, _, _) => StorageFailureKind::Value,
        crate::core::db::Error::InvalidParameterName(_) | crate::core::db::Error::InvalidQuery => {
            StorageFailureKind::Query
        }
    }
}

fn classify_storage_message(message: &str) -> StorageFailureKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("database is locked")
        || lower.contains("databasebusy")
        || lower.contains("storage contention")
        || lower.contains("extended_code: 522")
    {
        StorageFailureKind::Contention
    } else if lower.contains("disk i/o error") {
        StorageFailureKind::Io
    } else if lower.contains("constraint failed")
        || lower.contains("unique constraint")
        || lower.contains("foreign key constraint")
    {
        StorageFailureKind::Constraint
    } else {
        StorageFailureKind::Unknown
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/error_tests.rs"]
mod tests;

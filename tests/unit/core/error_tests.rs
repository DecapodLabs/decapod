// Moved from src/decapod/core/error.rs
use super::*;

#[test]
fn test_validation_error_display() {
    let err = DecapodError::ValidationError("test failed".to_string());
    assert_eq!(format!("{err}"), "Validation error: test failed");
}

#[test]
fn test_not_found_error_display() {
    let err = DecapodError::NotFound("file.txt not found".to_string());
    assert_eq!(format!("{err}"), "Not found: file.txt not found");
}

#[test]
fn test_not_implemented_error_display() {
    let err = DecapodError::NotImplemented("feature X".to_string());
    assert_eq!(format!("{err}"), "Not implemented: feature X");
}

#[test]
fn test_session_error_display() {
    let err = DecapodError::SessionError("token expired".to_string());
    assert_eq!(format!("{err}"), "Session error: token expired");
}

#[test]
fn test_path_error_display() {
    let err = DecapodError::PathError("invalid path".to_string());
    assert_eq!(format!("{err}"), "Path error: invalid path");
}

#[test]
fn storage_failure_classifies_contention_without_backend_callers() {
    let err = DecapodError::ValidationError("database is locked".to_string());
    assert_eq!(err.storage_failure_kind(), StorageFailureKind::Contention);
    assert!(err.storage_failure_kind().is_retryable());
    assert!(err.storage_failure_kind().is_contention());
}

#[test]
fn storage_failure_does_not_retry_generic_validation_errors() {
    let err = DecapodError::ValidationError("schema contract is invalid".to_string());
    assert_eq!(err.storage_failure_kind(), StorageFailureKind::Unknown);
    assert!(!err.storage_failure_kind().is_retryable());
}

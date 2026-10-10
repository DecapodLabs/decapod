// Moved from src/decapod/lib.rs
use super::*;

fn adapter_error(kind: dactyl_db::AdapterErrorKind, code: Option<&str>) -> error::DecapodError {
    error::DecapodError::DactylError(dactyl_db::DactylError::Adapter {
        kind,
        code: code.map(str::to_owned),
        message: code.unwrap_or("maintenance failure").to_owned(),
    })
}

#[test]
fn maintenance_diagnostics_preserve_dactyl_failure_classes() {
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Corrupt,
            Some("malformed_database"),
        )),
        "corrupt"
    );
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Busy,
            Some("busy"),
        )),
        "locked"
    );
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Conflict,
            Some("open_connections"),
        )),
        "locked"
    );
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Capability,
            Some("unsupported_journal_mode"),
        )),
        "unsupported"
    );
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Storage,
            Some("recovery_failed"),
        )),
        "recovery_failed"
    );
    assert_eq!(
        database_diagnostic_status(&adapter_error(
            dactyl_db::AdapterErrorKind::Storage,
            Some("recovery_rollback_failed"),
        )),
        "recovery_rollback_failed"
    );
}

#[test]
fn secure_recovery_refusals_keep_their_typed_diagnostics() {
    use dactyl_db::AdapterErrorKind;
    for (kind, code, status) in [
        (
            AdapterErrorKind::Unavailable,
            "recovery_reopen_failed",
            "recovery_failed",
        ),
        (
            AdapterErrorKind::Storage,
            "recovery_sync_failed",
            "recovery_failed",
        ),
        (
            AdapterErrorKind::Capability,
            "secure_recovery_unsupported",
            "unsupported",
        ),
        (
            AdapterErrorKind::Capability,
            "recovery_metadata_unsupported",
            "unsupported",
        ),
        (
            AdapterErrorKind::Authorization,
            "recovery_ownership_unavailable",
            "unavailable",
        ),
        (
            AdapterErrorKind::Conflict,
            "recovery_path_changed",
            "conflict",
        ),
        (
            AdapterErrorKind::Conflict,
            "unsafe_recovery_mode",
            "conflict",
        ),
    ] {
        let error = adapter_error(kind, Some(code));
        assert_eq!(database_diagnostic_status(&error), status, "{code}");
        assert_eq!(database_failure_code(&error).as_deref(), Some(code));
    }
}

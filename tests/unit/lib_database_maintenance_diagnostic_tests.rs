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

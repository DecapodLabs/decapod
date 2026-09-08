use super::*;
use tempfile::tempdir;

#[test]
fn paths_inside_project_are_relative_and_portable() {
    let temp = tempdir().unwrap();
    let inside = temp.path().join("src").join("lib.rs");

    assert_eq!(
        normalize_persisted_path(temp.path(), &inside.to_string_lossy()),
        "src/lib.rs"
    );
}

#[test]
fn paths_outside_project_are_redacted() {
    let temp = tempdir().unwrap();
    let outside = temp.path().parent().unwrap().join("private-user-file");

    assert_eq!(
        normalize_persisted_path(temp.path(), &outside.to_string_lossy()),
        "<external-path>"
    );
}

#[test]
fn embedded_absolute_paths_are_redacted_without_affecting_prose() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("src").join("lib.rs");
    let message = format!("failed to inspect {}: check again", path.display());

    assert_eq!(
        redact_text(temp.path(), &message),
        "failed to inspect src/lib.rs: check again"
    );
}

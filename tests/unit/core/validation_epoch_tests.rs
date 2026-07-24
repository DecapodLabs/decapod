// Moved from src/decapod/core/validation_epoch.rs
use super::*;
use tempfile::tempdir;

#[test]
fn specs_manifest_material_hash_ignores_generated_at() {
    let tmp = tempdir().expect("tempdir");
    let path = tmp.path().join("manifest.json");
    fs::write(
            &path,
            r#"{"schema_version":"1.0.0","template_version":"scaffold-v3","generated_at":"1Z","repo_signal_fingerprint":"abc","files":[]}"#,
        )
        .expect("write first manifest");
    let first = hash_specs_manifest_material(&path).expect("first hash");

    fs::write(
            &path,
            r#"{"schema_version":"1.0.0","template_version":"scaffold-v3","generated_at":"2Z","repo_signal_fingerprint":"abc","files":[]}"#,
        )
        .expect("write second manifest");
    let second = hash_specs_manifest_material(&path).expect("second hash");

    assert_eq!(
        first, second,
        "timestamp-only specs refreshes must not create new validation epochs"
    );
}

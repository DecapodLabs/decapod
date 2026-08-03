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

#[test]
fn validation_epoch_binds_living_spec_material_not_just_fingerprints() {
    let tmp = tempdir().expect("tempdir");
    let root = tmp.path();
    let specs = root.join(".decapod/managed/specs");
    fs::create_dir_all(&specs).expect("specs dir");
    let intent = specs.join("INTENT.md");
    let body = "# Intent\n\nAuthored contract.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `aaa`\n<!-- decapod:codebase-attestation:end -->\n";
    fs::write(&intent, body).expect("write intent");

    let epoch = active_validation_epoch(root).expect("epoch");
    let material_key = "living_spec_material:.decapod/managed/specs/INTENT.md";
    let full_key = "generated_spec:.decapod/managed/specs/INTENT.md";
    assert!(
        epoch.material_hashes.contains_key(material_key),
        "epoch must bind authored living-spec material: {:?}",
        epoch.material_hashes.keys().collect::<Vec<_>>()
    );
    let material_before = epoch.material_hashes[material_key].clone();
    let full_before = epoch.material_hashes[full_key].clone();

    // Fingerprint-only attestation churn changes full-file hash but not material.
    let fingerprint_only = body.replace("`aaa`", "`bbb`");
    fs::write(&intent, fingerprint_only).expect("fingerprint only");
    let epoch = active_validation_epoch(root).expect("epoch after fingerprint");
    assert_ne!(
        epoch.material_hashes[full_key], full_before,
        "full-file digest should move with attestation"
    );
    assert_eq!(
        epoch.material_hashes[material_key], material_before,
        "material digest must ignore fingerprint-only attestation churn"
    );

    // Authored rewrite must move the material digest.
    fs::write(
        &intent,
        "# Intent\n\nAuthored contract plus material rewrite for proof completion.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `bbb`\n<!-- decapod:codebase-attestation:end -->\n",
    )
    .expect("material rewrite");
    let epoch = active_validation_epoch(root).expect("epoch after material");
    assert_ne!(
        epoch.material_hashes[material_key], material_before,
        "material digest must move when authored living-spec prose changes"
    );
}

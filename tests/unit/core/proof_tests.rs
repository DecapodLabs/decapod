// Moved from src/decapod/core/proof.rs
use super::*;
use std::fs;
use tempfile::tempdir;

const SAMPLE_CONFIG: &str = r#"schema_version = "1.0.0"

[init]
diagram_style = "ascii"
entrypoints = []

[repo]

[proof]
[[proof.commands]]
name = "lint"
command = "cargo"
args = ["test", "--lib"]
required = true
description = "unit tests"
"#;

fn write_project_with_config(tmp: &tempfile::TempDir, config_body: &str) {
    let decapod_dir = tmp.path().join(".decapod");
    fs::create_dir_all(decapod_dir.join("data")).expect("create decapod data dir");
    fs::write(decapod_dir.join("config.toml"), config_body).expect("write config");
}

#[test]
fn project_config_proof_commands_load_from_project_root() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(&tmp, SAMPLE_CONFIG);

    let config = load_proof_config(tmp.path()).expect("load proof config");
    assert_eq!(config.proof.len(), 1);
    assert_eq!(config.proof[0].name, "lint");
    assert_eq!(config.proof[0].args, vec!["test", "--lib"]);
}

#[test]
fn store_root_resolves_config_toml_proof_commands() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(&tmp, SAMPLE_CONFIG);
    let store_root = tmp.path().join(".decapod").join("data");

    let resolved = resolve_proof_config(&store_root).expect("resolve from store root");
    assert_eq!(resolved.authority, PROOF_CONFIG_AUTHORITY);
    assert_eq!(resolved.config.proof.len(), 1);
    assert_eq!(resolved.config.proof[0].name, "lint");
    assert_eq!(resolved.config.proof[0].command, "cargo");
}

#[test]
fn project_root_and_store_root_agree_on_command_set() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(&tmp, SAMPLE_CONFIG);
    let store_root = tmp.path().join(".decapod").join("data");

    let from_project = resolve_proof_config(tmp.path()).expect("project root");
    let from_store = resolve_proof_config(&store_root).expect("store root");
    assert_eq!(from_project.authority, from_store.authority);
    assert_eq!(
        from_project.config.proof.len(),
        from_store.config.proof.len()
    );
    assert_eq!(
        from_project.config.proof[0].name,
        from_store.config.proof[0].name
    );
}

#[test]
fn dual_live_registries_fail_closed() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(&tmp, SAMPLE_CONFIG);
    fs::write(
        tmp.path().join(".decapod").join("proofs.toml"),
        r#"
[[proof]]
name = "other"
command = "true"
required = true
"#,
    )
    .expect("write legacy registry");

    let err = resolve_proof_config(tmp.path()).expect_err("dual authority must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("PROOF_DUAL_AUTHORITY"),
        "expected dual authority error, got: {msg}"
    );
    assert!(msg.contains(PROOF_CONFIG_AUTHORITY));
    assert!(msg.contains(LEGACY_PROOF_REGISTRY));

    // Same failure via store-root path used by CLI.
    let store_root = tmp.path().join(".decapod").join("data");
    let err = resolve_proof_config(&store_root).expect_err("dual via store root");
    assert!(err.to_string().contains("PROOF_DUAL_AUTHORITY"));
}

#[test]
fn config_sourced_authority_is_config_toml() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(&tmp, SAMPLE_CONFIG);

    let resolved = resolve_proof_config(tmp.path()).expect("resolve");
    assert_eq!(resolved.authority, PROOF_CONFIG_AUTHORITY);
}

#[test]
fn legacy_only_registry_is_transitional_with_correct_provenance() {
    let tmp = tempdir().expect("tempdir");
    let decapod_dir = tmp.path().join(".decapod");
    fs::create_dir_all(decapod_dir.join("data")).expect("create dirs");
    fs::write(
        decapod_dir.join("config.toml"),
        r#"schema_version = "1.0.0"

[init]
diagram_style = "ascii"
entrypoints = []

[repo]

[proof]
commands = []
"#,
    )
    .expect("write empty proof config");
    fs::write(
        decapod_dir.join("proofs.toml"),
        r#"
[[proof]]
name = "legacy-check"
command = "true"
required = true
description = "legacy only"
"#,
    )
    .expect("write legacy");

    let resolved = resolve_proof_config(tmp.path()).expect("legacy only");
    assert_eq!(resolved.authority, LEGACY_PROOF_REGISTRY);
    assert_eq!(resolved.config.proof.len(), 1);
    assert_eq!(resolved.config.proof[0].name, "legacy-check");
}

#[test]
fn empty_config_yields_empty_command_set_with_config_authority() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(
        &tmp,
        r#"schema_version = "1.0.0"

[init]
diagram_style = "ascii"
entrypoints = []

[repo]

[proof]
commands = []
"#,
    );

    let resolved = resolve_proof_config(tmp.path()).expect("empty");
    assert!(resolved.config.proof.is_empty());
    assert_eq!(resolved.authority, PROOF_CONFIG_AUTHORITY);
}

#[test]
fn schema_advertises_config_toml_as_live_authority() {
    let schema = schema();
    assert_eq!(
        schema.get("config_file").and_then(|v| v.as_str()),
        Some(PROOF_CONFIG_AUTHORITY)
    );
    assert_eq!(
        schema.get("authority_policy").and_then(|v| v.as_str()),
        Some("single_source_fail_closed")
    );
}

#[test]
fn malformed_config_proof_entry_fails() {
    let tmp = tempdir().expect("tempdir");
    write_project_with_config(
        &tmp,
        r#"schema_version = "1.0.0"

[init]
diagram_style = "ascii"
entrypoints = []

[repo]

[proof]
[[proof.commands]]
name = ""
command = "cargo"
required = true
"#,
    );

    let err = resolve_proof_config(tmp.path()).expect_err("malformed");
    assert!(
        err.to_string().contains("non-empty"),
        "got: {}",
        err.to_string()
    );
}

#[test]
fn resolve_project_root_rejects_unrelated_path() {
    let tmp = tempdir().expect("tempdir");
    // No .decapod under this path.
    let err = resolve_project_root(tmp.path()).expect_err("unrelated");
    assert!(
        err.to_string().contains("PROOF_PROJECT_ROOT_UNRESOLVED"),
        "got: {}",
        err.to_string()
    );
}

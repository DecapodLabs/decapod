// Moved from src/decapod/core/proof.rs
use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn project_config_proof_commands_feed_legacy_registry_loader() {
    let tmp = tempdir().expect("tempdir");
    let decapod_dir = tmp.path().join(".decapod");
    fs::create_dir_all(&decapod_dir).expect("create decapod dir");
    fs::write(
        decapod_dir.join("config.toml"),
        r#"schema_version = "1.0.0"

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
"#,
    )
    .expect("write config");

    let config = load_proof_config(tmp.path()).expect("load proof config");
    assert_eq!(config.proof.len(), 1);
    assert_eq!(config.proof[0].name, "lint");
    assert_eq!(config.proof[0].args, vec!["test", "--lib"]);
}

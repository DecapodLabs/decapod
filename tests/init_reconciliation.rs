use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(root)
        .env("XDG_CONFIG_HOME", root.join("machine-config"))
        .env("XDG_DATA_HOME", root.join("machine-data"))
        .env("DECAPOD_AGENT_ID", "init_reconciliation_test")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .unwrap()
}
fn init(root: &Path, args: &[&str]) -> String {
    let output = run(root, args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn config(root: &Path) -> toml::Value {
    toml::from_str(&fs::read_to_string(root.join(".decapod/config.toml")).unwrap()).unwrap()
}

#[test]
fn fresh_init_reports_missing_contract_without_implying_readiness() {
    let root = tempdir().unwrap();
    let stdout = init(root.path(), &["init", "--proof", "--no-ci", "--no-specs"]);
    assert!(stdout.contains("MISSING_REQUIRED_PROFILE"), "{stdout}");
    assert!(stdout.contains("NO_BUILD_OR_CI_CONTRACT"), "{stdout}");
    assert!(stdout.contains("merge-ready: false"), "{stdout}");
    assert!(stdout.contains("proof: not_run"), "{stdout}");
    assert!(
        stdout.contains("Initialized; required proof has not run"),
        "{stdout}"
    );
    assert!(
        config(root.path())["proof"]
            .get("required_profile")
            .is_none()
    );
}

#[test]
fn reinit_preserves_declared_profile_and_never_approves_stale_inventory() {
    for surface in [
        vec!["init", "--proof", "--force"],
        vec!["init", "with", "--force"],
    ] {
        let root = tempdir().unwrap();
        init(root.path(), &["init", "--proof", "--no-ci", "--no-specs"]);
        let mut declared = config(root.path());
        let profile = toml::Value::try_from(serde_json::json!({
            "id": "decapod.pr-readiness.v1", "reviewed_sources_digest": format!("sha256:{}", "0".repeat(64))
        })).unwrap();
        declared["proof"]
            .as_table_mut()
            .unwrap()
            .insert("required_profile".into(), profile.clone());
        fs::write(
            root.path().join(".decapod/config.toml"),
            toml::to_string_pretty(&declared).unwrap(),
        )
        .unwrap();
        let stdout = init(root.path(), &surface);
        assert!(stdout.contains("STALE_SOURCE_INVENTORY"), "{stdout}");
        assert!(stdout.contains("MISSING_PROFILE_SOURCE"), "{stdout}");
        assert_eq!(config(root.path())["proof"]["required_profile"], profile);
    }
}

#[test]
fn adoption_reports_conflicting_intent_without_overwriting_authored_ci_or_name() {
    let root = tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname='implementation-name'\nversion='0.1.0'\n",
    )
    .unwrap();
    let workflows = root.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    let authored = "name: Keep authored policy\njobs: {}\n";
    fs::write(workflows.join("decapod-validate.yml"), authored).unwrap();
    init(
        root.path(),
        &[
            "init",
            "--proof",
            "--product-name",
            "Authored Product",
            "--no-specs",
        ],
    );
    let stdout = init(root.path(), &["init", "--proof", "--force", "--no-specs"]);
    assert!(stdout.contains("INTENT_BUILD_MISMATCH"), "{stdout}");
    assert_eq!(
        config(root.path())["repo"]["product_name"].as_str(),
        Some("Authored Product")
    );
    assert_eq!(
        fs::read_to_string(workflows.join("decapod-validate.yml")).unwrap(),
        authored
    );
}

#[test]
fn first_run_agent_init_reports_fresh_reconciliation_after_source_change() {
    let root = tempdir().unwrap();
    init(
        root.path(),
        &[
            "init",
            "--proof",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
    );
    let before = run(root.path(), &["rpc", "--op", "agent.init"]);
    assert!(
        before.status.success(),
        "{}",
        String::from_utf8_lossy(&before.stderr)
    );
    let before: serde_json::Value = serde_json::from_slice(&before.stdout).unwrap();
    assert_eq!(before["result"]["reconciliation"]["merge_ready"], false);
    assert_eq!(
        before["result"]["reconciliation"]["proof_status"],
        "not_run"
    );
    let workflows = root.path().join(".github/workflows");
    fs::create_dir_all(&workflows).unwrap();
    fs::write(workflows.join("unreviewed.yml"), "jobs: {}\n").unwrap();
    let after = run(root.path(), &["rpc", "--op", "agent.init"]);
    assert!(
        after.status.success(),
        "{}",
        String::from_utf8_lossy(&after.stderr)
    );
    let after: serde_json::Value = serde_json::from_slice(&after.stdout).unwrap();
    let report = &after["result"]["reconciliation"];
    assert_ne!(
        before["result"]["reconciliation"]["inventory_digest"],
        report["inventory_digest"]
    );
    assert!(
        report["repair_tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["code"] == "UNKNOWN_CI_CONTRACT")
    );
    assert_eq!(report["merge_ready"], false);
}

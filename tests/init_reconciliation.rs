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

#[test]
fn polyglot_fixtures_reconcile_without_cloud_and_preserve_authored_files() {
    // Cargo (Rust), Node/TypeScript, Go, Python, and mixed monorepo fixtures (#1024).
    let fixtures: &[(&str, &[(&str, &str)])] = &[
        (
            "cargo-rust",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"polyglot-rust\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn ok() -> bool { true }\n"),
            ],
        ),
        (
            "node-ts",
            &[
                (
                    "package.json",
                    "{\"name\":\"polyglot-node\",\"version\":\"1.0.0\",\"scripts\":{\"test\":\"node --test\"}}\n",
                ),
                ("index.mjs", "export const ok = true;\n"),
            ],
        ),
        (
            "go-mod",
            &[
                ("go.mod", "module example.com/polyglotgo\n\ngo 1.22\n"),
                ("main.go", "package main\nfunc main() {}\n"),
            ],
        ),
        (
            "python-pyproject",
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = \"polyglot-py\"\nversion = \"0.1.0\"\n",
                ),
                ("app.py", "def ok():\n    return True\n"),
            ],
        ),
        (
            "mixed-monorepo",
            &[
                ("Cargo.toml", "[workspace]\nmembers = [\"crates/core\"]\n"),
                (
                    "crates/core/Cargo.toml",
                    "[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                (
                    "crates/core/src/lib.rs",
                    "pub fn core_ok() -> bool { true }\n",
                ),
                (
                    "web/package.json",
                    "{\"name\":\"web\",\"version\":\"1.0.0\",\"scripts\":{\"test\":\"node --test\"}}\n",
                ),
                (
                    ".github/workflows/ci.yml",
                    "name: Existing Authored CI\non: [push]\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo authored\n",
                ),
            ],
        ),
    ];

    for (name, files) in fixtures {
        let root = tempdir().unwrap();
        for (rel, body) in *files {
            let full = root.path().join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full, body).unwrap();
        }

        // Dry-run must be strictly non-mutating (#1024).
        let dry = run(
            root.path(),
            &["init", "--dry-run", "--proof", "--no-ci", "--no-specs"],
        );
        assert!(
            dry.status.success(),
            "dry-run failed for {name}: {}",
            String::from_utf8_lossy(&dry.stderr)
        );
        assert!(
            !root.path().join(".decapod").exists(),
            "init --dry-run mutated {name} by creating .decapod/"
        );

        // Local-first init works offline with zero cloud credentials/env.
        let stdout = init(
            root.path(),
            &[
                "init",
                "--proof",
                "--no-ci",
                "--no-specs",
                "--no-container-workspaces",
            ],
        );
        assert!(
            stdout.contains("merge-ready: false"),
            "{name} init must not claim merge readiness before proof: {stdout}"
        );
        assert!(
            stdout.contains("proof: not_run"),
            "{name} init must report proof: not_run: {stdout}"
        );

        // Preserve authored OVERRIDE.md across forced re-init (#1024).
        let custom_override = "# Custom Project Override\nKeep project lifecycle intact.\n";
        fs::write(root.path().join(".decapod/OVERRIDE.md"), custom_override).unwrap();
        let _reinit = init(
            root.path(),
            &[
                "init",
                "--proof",
                "--force",
                "--no-ci",
                "--no-specs",
                "--no-container-workspaces",
            ],
        );
        assert_eq!(
            fs::read_to_string(root.path().join(".decapod/OVERRIDE.md")).unwrap(),
            custom_override,
            "{name} re-init overwrote authored OVERRIDE.md"
        );

        // Verify all authored source and CI files survived untouched.
        for (rel, body) in *files {
            assert_eq!(
                fs::read_to_string(root.path().join(rel)).unwrap(),
                *body,
                "{name} init/re-init mutated authored file {rel}"
            );
        }

        // Verify agent.init produces fresh deterministic reconciliation without cloud auth.
        let agent_init = run(root.path(), &["rpc", "--op", "agent.init"]);
        assert!(
            agent_init.status.success(),
            "{name} agent.init failed offline: {}",
            String::from_utf8_lossy(&agent_init.stderr)
        );
        let parsed: serde_json::Value = serde_json::from_slice(&agent_init.stdout).unwrap();
        assert_eq!(parsed["result"]["reconciliation"]["merge_ready"], false);
        assert_eq!(
            parsed["result"]["reconciliation"]["proof_status"],
            "not_run"
        );
    }
}

//! Governance readers must work without initializing a local datastore/session.
use std::process::Command;
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::TempDir;

fn empty_project() -> TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".decapod")).unwrap();
    root
}

#[test]
fn governance_status_is_sessionless_and_has_no_migration_side_effects() {
    let root = empty_project();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root.path())
        .args(["govern", "artifacts", "status"])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.is_null());
    assert_eq!(
        std::fs::read_dir(root.path().join(".decapod"))
            .unwrap()
            .count(),
        0,
        "read-only status must not create governance, data, or session files"
    );
}

#[test]
fn governance_checkpoint_verifier_fails_without_creating_runtime_state() {
    let root = empty_project();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(root.path())
        .args([
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            "missing-base",
        ])
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_dir(root.path().join(".decapod"))
            .unwrap()
            .count(),
        0,
        "failed read-only verification must not initialize or migrate runtime state"
    );
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn governance_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let key = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                files.insert(format!("{key}/"), Vec::new());
                visit(root, &path, files);
            } else {
                files.insert(key, fs::read(path).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, &root.join(".decapod"), &mut files);
    files
}

/// A fresh branch inheriting accepted governance, with no local runtime store.
fn inherited_branch(legacy: bool) -> TempDir {
    let root = empty_project();
    git(root.path(), &["init", "-b", "master"]);
    git(
        root.path(),
        &["config", "user.name", "Governance Reader Tests"],
    );
    git(
        root.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(root.path().join("source.rs"), "fn main() {}\n").unwrap();
    git(root.path(), &["add", "source.rs"]);
    git(root.path(), &["commit", "-m", "baseline"]);
    let plan = serde_json::json!({
        "schema_version": "1.1.0", "title": "Accepted work", "intent": "Keep accepted evidence",
        "state": "DONE", "constraints": {}, "todo_ids": ["code_accepted"], "updated_at": "0Z"
    });
    if legacy {
        fs::create_dir(root.path().join(".decapod/governance")).unwrap();
        fs::write(
            root.path().join(".decapod/governance/plan.json"),
            serde_json::to_vec_pretty(&plan).unwrap(),
        )
        .unwrap();
    } else {
        let document = serde_json::json!({
            "schema_version": "1.0.0",
            "change": {
                "id": "accepted-reader-change",
                "base_commit": git(root.path(), &["rev-parse", "HEAD"]),
                "target_base": "master"
            },
            "claims": {
                "accepted-proof": {
                    "statement": "Accepted behavior worked", "falsifier": "A contrary result",
                    "status": "supported", "proof_refs": ["proof:accepted"]
                }
            },
            "sections": {"plan": plan}
        });
        fs::write(
            root.path().join(".decapod/governance.json"),
            serde_json::to_vec_pretty(&document).unwrap(),
        )
        .unwrap();
    }
    git(root.path(), &["add", ".decapod"]);
    git(root.path(), &["commit", "-m", "accepted governance"]);
    git(root.path(), &["checkout", "-b", "fresh-reader-branch"]);
    root
}

#[test]
fn governance_readers_do_not_reset_a_fresh_inherited_branch_or_migrate_legacy_files() {
    for legacy in [false, true] {
        let root = inherited_branch(legacy);
        let before = governance_files(root.path());
        let git_before = git(
            root.path(),
            &["status", "--porcelain=v1", "--untracked-files=all"],
        );
        for args in [
            vec!["govern", "artifacts", "status"],
            vec![
                "govern",
                "artifacts",
                "inventory",
                "--base-branch",
                "master",
            ],
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
                .current_dir(root.path())
                .args(&args)
                .env_remove("DECAPOD_SESSION_PASSWORD")
                .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
                .output()
                .unwrap();
            let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
                panic!("reader {args:?} must return its report without a session: {error}; stderr: {}", String::from_utf8_lossy(&output.stderr))
            });
            if args[2] == "status" {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(result["sections"]["plan"]["title"], "Accepted work");
                if !legacy {
                    assert_eq!(result["change"]["id"], "accepted-reader-change");
                    assert_eq!(result["claims"]["accepted-proof"]["status"], "supported");
                }
            } else {
                // Missing trajectory/validation may produce a nonzero readiness
                // result, but inspection must still return the inventory.
                assert_eq!(result["kind"], "governance_artifact_inventory");
                assert_eq!(result["workspace_branch"], "fresh-reader-branch");
            }
            assert_eq!(
                governance_files(root.path()),
                before,
                "reader {args:?} changed control-plane files"
            );
            assert_eq!(
                git(
                    root.path(),
                    &["status", "--porcelain=v1", "--untracked-files=all"]
                ),
                git_before
            );
            assert!(!root.path().join(".decapod/data").exists());
        }
    }
}

#[test]
fn governance_readers_preserve_malformed_governance_without_runtime_initialization() {
    let root = inherited_branch(false);
    let path = root.path().join(".decapod/governance.json");
    fs::write(&path, b"malformed accepted context").unwrap();
    let before = governance_files(root.path());
    for args in [
        vec!["govern", "artifacts", "status"],
        vec![
            "govern",
            "artifacts",
            "inventory",
            "--base-branch",
            "master",
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
            .current_dir(root.path())
            .args(&args)
            .env_remove("DECAPOD_SESSION_PASSWORD")
            .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "malformed governance must not become ready"
        );
        assert_eq!(
            governance_files(root.path()),
            before,
            "reader {args:?} repaired malformed evidence"
        );
        assert!(!root.path().join(".decapod/data").exists());
    }
}

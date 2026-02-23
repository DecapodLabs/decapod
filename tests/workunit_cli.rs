use decapod::core::workunit;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let git_init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git_init.status.success(), "git init failed");

    let init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("password in session acquire output");

    (tmp, dir, password)
}

#[test]
fn workunit_init_creates_manifest_file() {
    let (_tmp, dir, password) = setup_repo();
    let out = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_001",
            "--intent-ref",
            "intent://test",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "workunit init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(payload["marker"], "WORKUNIT_INITIALIZED");
    let manifest_path = dir
        .join(".decapod")
        .join("governance")
        .join("workunits")
        .join("R_001.json");
    assert!(manifest_path.exists(), "manifest file should exist");
}

#[test]
fn workunit_get_returns_expected_manifest_shape() {
    let (_tmp, dir, password) = setup_repo();
    let _ = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_002",
            "--intent-ref",
            "intent://shape",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    let out = run_decapod(
        &dir,
        &["govern", "workunit", "get", "--task-id", "R_002"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "workunit get failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(payload["task_id"], "R_002");
    assert_eq!(payload["intent_ref"], "intent://shape");
    assert!(payload["spec_refs"].is_array());
    assert!(payload["state_refs"].is_array());
    assert!(payload["proof_plan"].is_array());
    assert!(payload["proof_results"].is_array());
    assert_eq!(payload["status"], "DRAFT");
}

#[test]
fn workunit_status_returns_deterministic_manifest_hash() {
    let (_tmp, dir, password) = setup_repo();
    let _ = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_003",
            "--intent-ref",
            "intent://hash",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    let out = run_decapod(
        &dir,
        &["govern", "workunit", "status", "--task-id", "R_003"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "workunit status failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(payload["task_id"], "R_003");
    assert_eq!(payload["workunit_status"], "DRAFT");
    let hash_cli = payload["manifest_hash"].as_str().expect("hash string");

    let manifest = workunit::load_workunit(&dir, "R_003").expect("load workunit");
    let hash_expected = manifest.canonical_hash_hex().expect("hash expected");
    assert_eq!(hash_cli, hash_expected);

    let path = workunit::workunit_path(&dir, "R_003").expect("path");
    let on_disk = fs::read_to_string(path).expect("read manifest");
    assert!(
        !on_disk.is_empty(),
        "manifest content should be present on disk"
    );
}

#[test]
fn workunit_attach_spec_and_state_are_persisted() {
    let (_tmp, dir, password) = setup_repo();
    let _ = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_004",
            "--intent-ref",
            "intent://attach",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    for (subcmd, reference) in [
        ("attach-spec", "spec://a"),
        ("attach-spec", "spec://b"),
        ("attach-state", "state://1"),
        ("attach-state", "state://2"),
    ] {
        let out = run_decapod(
            &dir,
            &[
                "govern",
                "workunit",
                subcmd,
                "--task-id",
                "R_004",
                "--ref",
                reference,
            ],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        );
        assert!(
            out.status.success(),
            "{} failed: {}",
            subcmd,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let manifest = workunit::load_workunit(&dir, "R_004").expect("load workunit");
    assert_eq!(manifest.spec_refs, vec!["spec://a", "spec://b"]);
    assert_eq!(manifest.state_refs, vec!["state://1", "state://2"]);
}

#[test]
fn workunit_set_proof_plan_replaces_and_canonicalizes_gates() {
    let (_tmp, dir, password) = setup_repo();
    let _ = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_005",
            "--intent-ref",
            "intent://proofs",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    let out = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "set-proof-plan",
            "--task-id",
            "R_005",
            "--gate",
            "validate_passes",
            "--gate",
            "state_commit",
            "--gate",
            "validate_passes",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "set-proof-plan failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest = workunit::load_workunit(&dir, "R_005").expect("load workunit");
    assert_eq!(manifest.proof_plan, vec!["state_commit", "validate_passes"]);
}

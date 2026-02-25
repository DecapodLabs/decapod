use serde_json::Value;
use std::path::Path;
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

fn setup_repo() -> (TempDir, std::path::PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&decapod_init.stderr)
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
fn context_capsule_query_is_deterministic() {
    let (_tmp, dir, password) = setup_repo();

    let first = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "validation liveness",
            "--scope",
            "interfaces",
            "--task-id",
            "R_42",
            "--limit",
            "5",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        first.status.success(),
        "first query failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "validation liveness",
            "--scope",
            "interfaces",
            "--task-id",
            "R_42",
            "--limit",
            "5",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        second.status.success(),
        "second query failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let first_out = String::from_utf8_lossy(&first.stdout).to_string();
    let second_out = String::from_utf8_lossy(&second.stdout).to_string();
    assert_eq!(
        first_out, second_out,
        "query output should be byte-identical for same inputs"
    );

    let payload: Value = serde_json::from_str(&first_out).expect("parse output json");
    assert_eq!(payload["topic"], "validation liveness");
    assert_eq!(payload["scope"], "interfaces");
    assert!(
        !payload["capsule_hash"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );
    assert_eq!(payload["policy"]["risk_tier"], "medium");
    assert!(
        !payload["policy"]["policy_hash"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let sources = payload["sources"].as_array().expect("sources array");
    assert!(!sources.is_empty(), "expected at least one source");
    for source in sources {
        let path = source["path"].as_str().unwrap_or_default();
        assert!(
            path.starts_with("interfaces/"),
            "scope filter violated, got source path: {}",
            path
        );
    }
}

#[test]
fn context_capsule_query_rejects_invalid_scope() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "foo",
            "--scope",
            "methodology",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    assert!(!out.status.success(), "query should fail for invalid scope");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid scope"),
        "expected invalid scope error in stderr, got: {}",
        stderr
    );
}

#[test]
fn context_capsule_query_fails_closed_on_policy_scope_violation() {
    let (_tmp, dir, password) = setup_repo();

    let out = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "foo",
            "--scope",
            "plugins",
            "--risk-tier",
            "low",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );

    assert!(
        !out.status.success(),
        "query should fail when risk tier denies scope"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CAPSULE_SCOPE_DENIED"),
        "expected typed policy denial error, got: {}",
        stderr
    );
}

#[test]
fn context_capsule_query_write_persists_deterministic_artifact_path() {
    let (_tmp, dir, password) = setup_repo();

    let run = |task_id: &str| {
        run_decapod(
            &dir,
            &[
                "govern",
                "capsule",
                "query",
                "--topic",
                "proof gates",
                "--scope",
                "core",
                "--task-id",
                task_id,
                "--write",
            ],
            &[
                ("DECAPOD_AGENT_ID", "unknown"),
                ("DECAPOD_SESSION_PASSWORD", &password),
                ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
            ],
        )
    };

    let first = run("R_123");
    assert!(
        first.status.success(),
        "first write query failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_payload: Value = serde_json::from_slice(&first.stdout).expect("parse first payload");
    let first_path = first_payload["path"]
        .as_str()
        .expect("path string in first payload");
    assert!(
        first_path.ends_with(".decapod/generated/context/R_123.json"),
        "unexpected capsule path: {}",
        first_path
    );
    assert!(
        std::path::Path::new(first_path).exists(),
        "expected persisted capsule at {}",
        first_path
    );

    let second = run("R_123");
    assert!(
        second.status.success(),
        "second write query failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    let second_payload: Value =
        serde_json::from_slice(&second.stdout).expect("parse second payload");
    assert_eq!(
        first_payload["path"], second_payload["path"],
        "artifact path should be deterministic for same inputs"
    );
    assert_eq!(
        first_payload["capsule"]["capsule_hash"], second_payload["capsule"]["capsule_hash"],
        "capsule hash should stay stable for same inputs"
    );
}

#[test]
fn context_capsule_query_write_auto_binds_workunit_state_ref() {
    let (_tmp, dir, password) = setup_repo();
    let envs = [
        ("DECAPOD_AGENT_ID", "unknown"),
        ("DECAPOD_SESSION_PASSWORD", &password),
        ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
    ];

    let init_workunit = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "R_321",
            "--intent-ref",
            "intent://capsule-bind",
        ],
        &envs,
    );
    assert!(
        init_workunit.status.success(),
        "workunit init failed: {}",
        String::from_utf8_lossy(&init_workunit.stderr)
    );

    let out = run_decapod(
        &dir,
        &[
            "govern",
            "capsule",
            "query",
            "--topic",
            "bind capsule",
            "--scope",
            "interfaces",
            "--task-id",
            "R_321",
            "--write",
        ],
        &envs,
    );
    assert!(
        out.status.success(),
        "capsule write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse payload");
    let capsule_path = payload["path"].as_str().expect("capsule path");
    assert!(
        payload["workunit_state_ref_binding"].is_string(),
        "expected workunit binding path in output"
    );

    let workunit = run_decapod(
        &dir,
        &["govern", "workunit", "get", "--task-id", "R_321"],
        &envs,
    );
    assert!(
        workunit.status.success(),
        "workunit get failed: {}",
        String::from_utf8_lossy(&workunit.stderr)
    );
    let workunit_payload: Value = serde_json::from_slice(&workunit.stdout).expect("workunit json");
    let state_refs = workunit_payload["state_refs"]
        .as_array()
        .expect("state refs array");
    let expected_rel = ".decapod/generated/context/R_321.json";
    let has_ref = state_refs.iter().any(|v| {
        let s = v.as_str().unwrap_or_default();
        s == expected_rel || s.ends_with(expected_rel) || s == capsule_path
    });
    assert!(
        has_ref,
        "expected workunit state_refs to include capsule path binding"
    );
}

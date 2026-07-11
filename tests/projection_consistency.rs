use serde_json::Value;
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

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let out = run_decapod(&dir, &["init", "--force"], &[]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let acquire = run_decapod(
        &dir,
        &["session", "acquire"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&acquire.stderr)
    );
    let acquire_stdout = String::from_utf8_lossy(&acquire.stdout);
    let password = acquire_stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .expect("session password in output");

    (tmp, dir, password)
}

fn run_validate(dir: &Path, password: &str, projections: bool) -> std::process::Output {
    let mut args = vec!["validate", "--format", "json"];
    if projections {
        args.push("--projections");
    }
    run_decapod(
        dir,
        &args,
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    )
}

#[test]
fn projection_validation_catches_stale_context_capsule_while_normal_passes() {
    let (_tmp, dir, password) = setup_repo();

    // Get a valid context capsule from decapod
    let capsule = get_valid_context_capsule(&dir, &password);
    // Write the valid capsule (with correct current fingerprint and hash)
    write_context_capsule(&dir, &capsule);

    // Normal validate should pass with valid capsule
    let normal = run_validate(&dir, &password, false);
    assert!(
        normal.status.success(),
        "normal validate should pass with valid context capsule; stderr:\n{}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Now mutate a tracked file so the fingerprint changes, making the capsule stale
    // Create a src/lib.rs file which is a tracked path
    let src_dir = dir.join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        src_dir.join("lib.rs"),
        "// This changes the repo fingerprint\npub fn hello() {}\n",
    )
    .expect("write src/lib.rs to mutate repo");

    // Refresh specs so the manifest fingerprint matches the new repo state
    // This isolates the context capsule test from the specs validation
    let specs_refresh = run_decapod(
        &dir,
        &["rpc", "--op", "specs.refresh"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        specs_refresh.status.success(),
        "specs refresh failed: {}",
        String::from_utf8_lossy(&specs_refresh.stderr)
    );

    // Normal validate should still pass (capsule hash is still valid for its original fingerprint)
    let normal = run_validate(&dir, &password, false);
    assert!(
        normal.status.success(),
        "normal validate should pass with stale context capsule fingerprint; stderr:\n{}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Validate with --projections should fail (fingerprint is now stale)
    let projections = run_validate(&dir, &password, true);
    assert!(
        !projections.status.success(),
        "validate --projections should fail with stale context capsule; stdout:\n{}",
        String::from_utf8_lossy(&projections.stdout)
    );

    let payload: Value =
        serde_json::from_slice(&projections.stdout).expect("validate json payload");
    assert_eq!(payload["report"]["status"], "fail");
    assert!(payload["report"]["fail_count"].as_u64().unwrap_or(0) > 0);

    let failures = payload["report"]["failures"]
        .as_array()
        .expect("failures array");
    let projection_failure = failures.iter().find(|f| {
        let msg = f.as_str().unwrap_or("");
        msg.contains("[PROJECTION]")
            && msg.contains(".decapod/generated/context/")
            && msg.contains("repo_signal_fingerprint")
    });
    assert!(
        projection_failure.is_some(),
        "expected projection failure with typed finding; got: {failures:?}"
    );
    let finding = projection_failure.unwrap().as_str().unwrap();
    assert!(finding.contains("[PROJECTION]"));
    assert!(finding.contains("expected: repo_signal_fingerprint"));
    assert!(finding.contains("observed: stale fingerprint"));
    assert!(finding.contains("remediation"));
    assert!(finding.contains("Regenerate context capsule"));
}

#[test]
fn projection_validation_catches_spec_manifest_hash_mismatch_while_normal_passes() {
    let (_tmp, dir, password) = setup_repo();

    // Mutate INTENT.md after manifest was generated
    let spec_path = dir.join(".decapod/generated/specs/INTENT.md");
    let original = fs::read_to_string(&spec_path).expect("read INTENT.md");
    let mutated = original + "\n\n// Mutated after manifest generation\n";
    fs::write(&spec_path, mutated).expect("write mutated INTENT.md");

    // Validate with --projections should fail
    let projections = run_validate(&dir, &password, true);
    assert!(
        !projections.status.success(),
        "validate --projections should fail with spec manifest hash mismatch; stdout:\n{}",
        String::from_utf8_lossy(&projections.stdout)
    );

    let payload: Value =
        serde_json::from_slice(&projections.stdout).expect("validate json payload");
    let failures = payload["report"]["failures"]
        .as_array()
        .expect("failures array");
    let projection_failure = failures.iter().find(|f| {
        let msg = f.as_str().unwrap_or("");
        msg.contains("[PROJECTION]")
            && msg.contains(".decapod/generated/specs/INTENT.md")
            && msg.contains("content hash")
    });
    assert!(
        projection_failure.is_some(),
        "expected projection failure for spec manifest hash mismatch; got: {failures:?}"
    );
    let finding = projection_failure.unwrap().as_str().unwrap();
    assert!(finding.contains("[PROJECTION]"));
    assert!(finding.contains("expected: content hash"));
    assert!(finding.contains("observed: divergent hash"));
    assert!(finding.contains("remediation"));
    assert!(finding.contains("specs.refresh"));
}

#[test]
fn projection_validation_catches_done_task_without_proof_artifacts_while_normal_passes() {
    let (_tmp, dir, password) = setup_repo();

    // Add a task and mark it done
    let task_add = run_decapod(
        &dir,
        &[
            "todo",
            "add",
            "Test task for projection validation",
            "--priority",
            "high",
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        task_add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&task_add.stderr)
    );

    let todo_list = run_decapod(
        &dir,
        &["todo", "list", "--format", "json"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        todo_list.status.success(),
        "todo list failed: {}",
        String::from_utf8_lossy(&todo_list.stderr)
    );
    let list_output: Value =
        serde_json::from_slice(&todo_list.stdout).expect("parse todo list json");
    let task_id = list_output["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|item| item["title"].as_str().unwrap_or("").contains("Test task"))
        .and_then(|item| item["id"].as_str())
        .expect("task id in list output");

    let task_claim = run_decapod(
        &dir,
        &["todo", "claim", "--id", task_id],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        task_claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&task_claim.stderr)
    );

    let task_done = run_decapod(
        &dir,
        &["todo", "done", "--id", task_id],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        task_done.status.success(),
        "todo done failed: {}",
        String::from_utf8_lossy(&task_done.stderr)
    );

    // Ensure provenance directory exists but is empty (no proof artifacts)
    let provenance_dir = dir.join(".decapod/generated/artifacts/provenance");
    fs::create_dir_all(&provenance_dir).expect("create provenance dir");

    // Normal validate should pass (it doesn't check for proof artifacts)
    let normal = run_validate(&dir, &password, false);
    assert!(
        normal.status.success(),
        "normal validate should pass with done task missing proof artifacts; stderr:\n{}",
        String::from_utf8_lossy(&normal.stderr)
    );

    // Validate with --projections should fail
    let projections = run_validate(&dir, &password, true);
    let proj_success = projections.status.success();
    assert!(
        !proj_success,
        "validate --projections should fail with done task missing proof artifacts; stdout:\n{}",
        String::from_utf8_lossy(&projections.stdout)
    );

    let payload: Value =
        serde_json::from_slice(&projections.stdout).expect("validate json payload");
    let failures = payload["report"]["failures"]
        .as_array()
        .expect("failures array");
    let evidence_failure = failures.iter().find(|f| {
        let msg = f.as_str().unwrap_or("");
        msg.contains("[EVIDENCE]")
            && msg.contains(".decapod/generated/artifacts/provenance/")
            && msg.contains("artifact_manifest.json")
    });
    assert!(
        evidence_failure.is_some(),
        "expected evidence failure for missing proof artifacts; got: {failures:?}"
    );
    let finding = evidence_failure.unwrap().as_str().unwrap();
    assert!(finding.contains("[EVIDENCE]"));
    assert!(finding.contains("artifact_manifest.json"));
    assert!(finding.contains("proof_manifest.json"));
    assert!(finding.contains("intent_convergence_checklist.json"));
    assert!(finding.contains("remediation"));
    assert!(finding.contains("qa verify capture") || finding.contains("todo done --validated"));
}

fn get_valid_context_capsule(dir: &Path, password: &str) -> Value {
    let out = run_decapod(
        dir,
        &[
            "rpc",
            "--op",
            "context.capsule.query",
            "--params",
            r#"{"topic":"test","scope":"core","limit":1}"#,
        ],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        out.status.success(),
        "context capsule query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: Value = serde_json::from_slice(&out.stdout).expect("parse capsule result");
    result["result"].clone()
}

fn write_context_capsule(dir: &Path, capsule: &Value) {
    let context_dir = dir.join(".decapod/generated/context");
    fs::create_dir_all(&context_dir).expect("create context dir");
    let capsule_path = context_dir.join("test_task.json");
    // Write the capsule as-is - it already has a valid hash from the RPC
    fs::write(
        &capsule_path,
        serde_json::to_string_pretty(capsule).expect("serialize capsule"),
    )
    .expect("write capsule");
}

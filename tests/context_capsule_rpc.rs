use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run decapod")
}

fn run_decapod_with_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod with env")
}

fn run_git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git")
}

fn setup_repo() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tmpdir");
    let dir = tmp.path().to_path_buf();

    let init = Command::new("git")
        .current_dir(&dir)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(init.status.success(), "git init failed");

    let decapod_init = run_decapod(&dir, &["init", "--force"]);
    assert!(
        decapod_init.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&decapod_init.stderr)
    );
    let git_name = run_git(&dir, &["config", "user.name", "Decapod Test"]);
    assert!(
        git_name.status.success(),
        "git config user.name failed: {}",
        String::from_utf8_lossy(&git_name.stderr)
    );
    let git_email = run_git(&dir, &["config", "user.email", "test@decapod.local"]);
    assert!(
        git_email.status.success(),
        "git config user.email failed: {}",
        String::from_utf8_lossy(&git_email.stderr)
    );
    let git_add = run_git(&dir, &["add", "-A"]);
    assert!(
        git_add.status.success(),
        "git add failed: {}",
        String::from_utf8_lossy(&git_add.stderr)
    );
    let git_commit = run_git(&dir, &["commit", "-m", "test fixture bootstrap"]);
    assert!(
        git_commit.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&git_commit.stderr)
    );

    let validate = run_decapod_with_env(
        &dir,
        &["validate"],
        &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")],
    );
    assert!(
        validate.status.success(),
        "validate failed: {}",
        String::from_utf8_lossy(&validate.stderr)
    );
    let docs_ingest = run_decapod(&dir, &["docs", "ingest"]);
    assert!(
        docs_ingest.status.success(),
        "docs ingest failed: {}",
        String::from_utf8_lossy(&docs_ingest.stderr)
    );
    let session = run_decapod(&dir, &["session", "acquire"]);
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let init_rpc = run_decapod(&dir, &["rpc", "--op", "agent.init"]);
    assert!(
        init_rpc.status.success(),
        "agent.init failed: {}",
        String::from_utf8_lossy(&init_rpc.stderr)
    );
    let resolve_rpc = run_decapod(&dir, &["rpc", "--op", "context.resolve"]);
    assert!(
        resolve_rpc.status.success(),
        "context.resolve failed: {}",
        String::from_utf8_lossy(&resolve_rpc.stderr)
    );

    let todo_add = run_decapod(&dir, &["todo", "add", "context capsule rpc test"]);
    assert!(
        todo_add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&todo_add.stderr)
    );
    let todo_payload: Value = serde_json::from_slice(&todo_add.stdout).expect("parse todo add");
    let task_id = todo_payload["id"].as_str().expect("todo id").to_string();

    let claim = run_decapod(&dir, &["todo", "claim", "--id", &task_id]);
    assert!(
        claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim.stderr)
    );

    let ensure = run_decapod(&dir, &["workspace", "ensure"]);
    assert!(
        ensure.status.success(),
        "workspace ensure failed: {}",
        String::from_utf8_lossy(&ensure.stderr)
    );
    let ensure_payload: Value =
        serde_json::from_slice(&ensure.stdout).expect("parse workspace ensure output");
    let worktree_path = ensure_payload["worktree_path"]
        .as_str()
        .expect("workspace ensure should return worktree_path")
        .to_string();

    (tmp, std::path::PathBuf::from(worktree_path))
}

#[test]
fn rpc_context_capsule_query_is_deterministic() {
    let (_tmp, dir) = setup_repo();
    let params = r#"{"topic":"proof gates","scope":"interfaces","task_id":"test_77","limit":4}"#;

    let first = run_decapod(
        &dir,
        &["rpc", "--op", "context.capsule.query", "--params", params],
    );
    assert!(
        first.status.success(),
        "first rpc call failed: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let second = run_decapod(
        &dir,
        &["rpc", "--op", "context.capsule.query", "--params", params],
    );
    assert!(
        second.status.success(),
        "second rpc call failed: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let first_payload: Value = serde_json::from_slice(&first.stdout).expect("parse first payload");
    let second_payload: Value =
        serde_json::from_slice(&second.stdout).expect("parse second payload");

    assert_eq!(first_payload["success"], true);
    assert_eq!(second_payload["success"], true);

    let first_result = &first_payload["result"];
    let second_result = &second_payload["result"];
    assert_eq!(first_result, second_result, "rpc result must be stable");

    let capsule_hash = first_result["capsule_hash"].as_str().unwrap_or_default();
    assert!(!capsule_hash.is_empty(), "capsule hash missing");
    assert_eq!(
        first_result["policy"]["risk_tier"]
            .as_str()
            .unwrap_or_default(),
        "medium"
    );
}

#[test]
fn rpc_context_capsule_query_write_tracks_touched_path() {
    let (_tmp, dir) = setup_repo();
    let params = r#"{"topic":"workspace rules","scope":"core","task_id":"test_88","write":true}"#;

    let out = run_decapod(
        &dir,
        &["rpc", "--op", "context.capsule.query", "--params", params],
    );
    assert!(
        out.status.success(),
        "rpc write call failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse payload");
    assert_eq!(payload["success"], true);

    let touched = payload["receipt"]["touched_paths"]
        .as_array()
        .expect("touched paths array");
    assert_eq!(touched.len(), 1, "expected one touched capsule path");

    let touched_path = touched[0].as_str().expect("touched path as str");
    assert!(
        touched_path.ends_with(".decapod/generated/context/test_88.json"),
        "unexpected touched path: {touched_path}"
    );
    assert!(
        std::path::Path::new(touched_path).exists(),
        "expected persisted capsule at {touched_path}"
    );
}

#[test]
fn rpc_context_capsule_query_rejects_unknown_risk_tier() {
    let (_tmp, dir) = setup_repo();
    let params = r#"{"topic":"policy","scope":"interfaces","risk_tier":"unknown-tier"}"#;
    let out = run_decapod(
        &dir,
        &["rpc", "--op", "context.capsule.query", "--params", params],
    );
    assert!(
        !out.status.success(),
        "rpc capsule query should fail for unknown risk tier"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CAPSULE_RISK_TIER_UNKNOWN"),
        "expected typed risk-tier error, got: {stderr}"
    );
}

#[test]
fn rpc_context_capsule_query_write_auto_binds_workunit_state_ref() {
    let (_tmp, dir) = setup_repo();

    let init = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "test_654",
            "--intent-ref",
            "intent://rpc-capsule-bind",
        ],
    );
    assert!(
        init.status.success(),
        "workunit init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let params = r#"{"topic":"rpc bind","scope":"interfaces","task_id":"test_654","write":true}"#;
    let out = run_decapod(
        &dir,
        &["rpc", "--op", "context.capsule.query", "--params", params],
    );
    assert!(
        out.status.success(),
        "rpc write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let payload: Value = serde_json::from_slice(&out.stdout).expect("parse payload");
    let touched = payload["receipt"]["touched_paths"]
        .as_array()
        .expect("touched paths array");
    let has_workunit_path = touched.iter().any(|v| {
        v.as_str()
            .unwrap_or_default()
            .ends_with(".decapod/governance/workunits/test_654.json")
    });
    assert!(
        has_workunit_path,
        "expected touched paths to include bound workunit manifest path"
    );
}

#[test]
fn rpc_context_bundle_round_trip_rejects_tamper_and_stale_inputs() {
    let (_tmp, dir) = setup_repo();
    let init = run_decapod(
        &dir,
        &[
            "govern",
            "workunit",
            "init",
            "--task-id",
            "test_bundle",
            "--intent-ref",
            "intent://portable-handoff",
        ],
    );
    assert!(init.status.success(), "workunit init failed");
    let params = r#"{"topic":"portable handoff","scope":"interfaces","task_id":"test_bundle","limit":4,"uncertainty":["z-unresolved","a-unresolved"],"constraints":["must preserve policy","must retain state refs"]}"#;

    let exported = run_decapod(
        &dir,
        &["rpc", "--op", "context.bundle.export", "--params", params],
    );
    assert!(
        exported.status.success(),
        "bundle export failed: {}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let payload: Value = serde_json::from_slice(&exported.stdout).expect("parse export payload");
    let bundle = payload["result"]["bundle"].clone();
    let bundle_path = payload["result"]["path"].as_str().expect("bundle path");
    assert_eq!(
        bundle["uncertainty"],
        serde_json::json!(["z-unresolved", "a-unresolved"])
    );
    assert_eq!(bundle["constraints"].as_array().unwrap().len(), 2);
    assert!(bundle["project_identity"]["declared_capabilities"].is_array());
    assert_eq!(bundle["workunit"]["task_id"], "test_bundle");
    assert_eq!(
        bundle["workunit"]["intent_ref"],
        "intent://portable-handoff"
    );
    assert!(
        !bundle["bundle_hash"]
            .as_str()
            .unwrap_or_default()
            .is_empty()
    );

    let imported = run_decapod(
        &dir,
        &[
            "rpc",
            "--op",
            "context.bundle.import",
            "--params",
            &format!(r#"{{"path":"{bundle_path}"}}"#),
        ],
    );
    assert!(
        imported.status.success(),
        "bundle import failed: {}",
        String::from_utf8_lossy(&imported.stderr)
    );

    let mut incompatible = bundle.clone();
    incompatible["schema_version"] = Value::String("9.0.0".to_string());
    fs::write(
        bundle_path,
        serde_json::to_vec_pretty(&incompatible).unwrap(),
    )
    .expect("write incompatible bundle");
    let incompatible_result = run_decapod(
        &dir,
        &[
            "rpc",
            "--op",
            "context.bundle.import",
            "--params",
            &format!(r#"{{"path":"{bundle_path}"}}"#),
        ],
    );
    assert!(!incompatible_result.status.success());
    assert!(
        String::from_utf8_lossy(&incompatible_result.stderr)
            .contains("CONTEXT_BUNDLE_INCOMPATIBLE")
    );

    let mut tampered = bundle.clone();
    tampered["bundle_hash"] = Value::String("tampered".to_string());
    fs::write(bundle_path, serde_json::to_vec_pretty(&tampered).unwrap()).expect("tamper bundle");
    let rejected = run_decapod(
        &dir,
        &[
            "rpc",
            "--op",
            "context.bundle.import",
            "--params",
            &format!(r#"{{"path":"{bundle_path}"}}"#),
        ],
    );
    assert!(
        !rejected.status.success(),
        "tampered bundle must be rejected"
    );
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("CONTEXT_BUNDLE_TAMPERED"));

    fs::write(bundle_path, serde_json::to_vec_pretty(&bundle).unwrap()).expect("restore bundle");
    let config_path = dir.join(".decapod/config.toml");
    let config_content = fs::read_to_string(&config_path).expect("read config");
    let mut config: toml::Value = toml::from_str(&config_content).expect("parse config");
    config["repo"]["product_summary"] = toml::Value::String("changed after export".to_string());
    fs::write(
        &config_path,
        toml::to_string_pretty(&config).expect("serialize config"),
    )
    .expect("change config");
    let stale = run_decapod(
        &dir,
        &[
            "rpc",
            "--op",
            "context.bundle.import",
            "--params",
            &format!(r#"{{"path":"{bundle_path}"}}"#),
        ],
    );
    assert!(!stale.status.success(), "stale bundle must be rejected");
    assert!(String::from_utf8_lossy(&stale.stderr).contains("CONTEXT_BUNDLE_STALE"));
}

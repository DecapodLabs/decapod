use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], password: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
    command
        .current_dir(dir)
        .args(args)
        .env("DECAPOD_AGENT_ID", "trajectory-test-agent")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1");
    if let Some(password) = password {
        command.env("DECAPOD_SESSION_PASSWORD", password);
    }
    command.output().expect("run decapod")
}

fn setup_repo() -> (TempDir, PathBuf, String) {
    let temp = TempDir::new().expect("create temp repository");
    let root = temp.path().to_path_buf();
    let git = Command::new("git")
        .current_dir(&root)
        .args(["init", "-b", "master"])
        .output()
        .expect("git init");
    assert!(git.status.success());

    let init = run_decapod(&root, &["init", "--force"], None);
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let session = run_decapod(&root, &["session", "acquire"], None);
    assert!(session.status.success());
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_string))
        .expect("session password");
    (temp, root, password)
}

fn json(output: &Output, operation: &str) -> Value {
    assert!(
        output.status.success(),
        "{operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("JSON output")
}

#[test]
fn trajectory_cli_records_scope_actions_checks_and_verdicts() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_1",
            "--task-id",
            "task_cli_1",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "implement bounded change",
            "--destination",
            "published PR",
            "--phase",
            "implementation",
            "--next-transition",
            "validate",
            "--blocker",
            "awaiting proof",
            "--boundary",
            "src/**",
            "--scope",
            "src/decapod/lib.rs",
        ],
        env_password,
    );
    let init_json = json(&init, "trajectory init");
    assert_eq!(init_json["marker"], "TRAJECTORY_INITIALIZED");
    assert_eq!(init_json["trajectory"]["destination"], "published PR");
    assert_eq!(init_json["trajectory"]["current_phase"], "implementation");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_1",
            "--blocker",
            "awaiting proof",
            "--clear-blockers",
            "--inspected-file",
            "src/decapod/lib.rs",
            "--modified-file",
            "src/decapod/lib.rs",
            "--command",
            "cargo test --lib",
            "--tool-call",
            "decapod validate",
            "--check",
            "cargo_test=passed",
            "--evidence",
            ".decapod/managed/artifacts/proof.json",
            "--assumption",
            "existing interface remains compatible",
            "--completion-claim",
            "implemented and verified",
        ],
        env_password,
    );
    let record_json = json(&record, "trajectory record");
    assert_eq!(record_json["proof_status"], "passed");
    assert_eq!(record_json["verdicts"]["completion_proof"], "supported");
    assert_eq!(record_json["inspected_files"][0], "src/decapod/lib.rs");
    assert_eq!(record_json["checks"][0]["status"], "passed");

    let status = run_decapod(
        &root,
        &["govern", "trajectory", "status", "--run-id", "run_cli_1"],
        env_password,
    );
    let status_json = json(&status, "trajectory status");
    assert_eq!(status_json["proof_status"], "passed");
    assert_eq!(status_json["motion_state"], "blocked");
    assert_eq!(status_json["blockers"][0], "awaiting proof");
    assert!(
        root.join(decapod::core::governance_document::GOVERNANCE_PATH)
            .exists()
    );
    assert!(
        decapod::core::governance_document::read_section(&root, "trajectory")
            .unwrap()
            .is_some()
    );
    assert!(!root.join(".decapod/governance/trajectory.json").exists());
}

#[test]
fn trajectory_cli_replaces_current_section_and_preserves_retired_observation_history() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_old",
            "--original-intent",
            "preserve one cookie",
            "--derived-intent",
            "write the current run",
        ],
        env_password,
    );
    json(&init, "old trajectory init");

    let plan_before = decapod::core::governance_document::read_section(&root, "plan").unwrap();
    let claims_before = decapod::core::governance_document::read_section(&root, "claims").unwrap();
    decapod::core::governance_document::write_section(
        &root,
        "jev",
        &serde_json::json!({
            "$schema": "https://decapod.dev/schemas/jev-observations-1.0.0.schema.json",
            "schema_version": "1.0.0",
            "kind": "jev_observation_ledger",
            "trajectory_run_id": "run_cli_old",
            "runs": {}
        }),
    )
    .expect("record Jev logical section");

    let replacement_args = [
        "govern",
        "trajectory",
        "init",
        "--run-id",
        "run_cli_new",
        "--original-intent",
        "preserve one cookie",
        "--derived-intent",
        "replace the current run",
    ];
    let governance_path = root.join(decapod::core::governance_document::GOVERNANCE_PATH);
    let predecessor = fs::read(&governance_path).expect("read previous journal");
    let blocked = run_decapod(&root, &replacement_args, env_password);
    assert!(!blocked.status.success());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("TRAJECTORY_UNARCHIVED_WORK"));
    assert_eq!(fs::read(&governance_path).unwrap(), predecessor);
    for args in [
        vec!["add", ".decapod/governance.json"],
        vec![
            "-c",
            "user.name=Trajectory Test",
            "-c",
            "user.email=trajectory@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Preserve prior trajectory and Jev evidence",
        ],
    ] {
        let output = Command::new("git")
            .current_dir(&root)
            .args(args)
            .output()
            .expect("archive fixture governance");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let replacement = run_decapod(&root, &replacement_args, env_password);
    json(&replacement, "new trajectory init");
    let archived = Command::new("git")
        .current_dir(&root)
        .args(["show", "HEAD:.decapod/governance.json"])
        .output()
        .expect("read archived governance");
    assert!(archived.status.success());
    assert_eq!(archived.stdout, predecessor);

    let raw =
        std::fs::read_to_string(root.join(decapod::core::governance_document::GOVERNANCE_PATH))
            .expect("read normalized governance");
    let _: Value = serde_json::from_str(&raw).expect("governance is one JSON value");
    let trajectory = decapod::core::governance_document::read_section(&root, "trajectory")
        .unwrap()
        .unwrap();
    assert_eq!(trajectory["run_id"], "run_cli_new");
    assert!(
        decapod::core::governance_document::read_section(&root, "jev")
            .unwrap()
            .is_some(),
        "new trajectory must preserve historical observation evidence"
    );
    assert_eq!(
        decapod::core::governance_document::read_section(&root, "plan").unwrap(),
        plan_before
    );
    assert_eq!(
        decapod::core::governance_document::read_section(&root, "claims").unwrap(),
        claims_before
    );
}

#[test]
fn trajectory_cli_refuses_to_overwrite_corrupted_shared_governance() {
    let (_temp, root, password) = setup_repo();
    let path = root.join(decapod::core::governance_document::GOVERNANCE_PATH);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    file.write_all(b"\n{\"unexpected_append\":true}\n").unwrap();
    let damaged = fs::read(&path).unwrap();
    let replacement = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "must-not-overwrite",
            "--original-intent",
            "Preserve all evidence",
            "--derived-intent",
            "Refuse invalid shared state",
        ],
        Some(&password),
    );
    assert!(!replacement.status.success(), "corruption must fail closed");
    assert!(
        String::from_utf8_lossy(&replacement.stderr).contains("Invalid governance document"),
        "{}",
        String::from_utf8_lossy(&replacement.stderr)
    );
    assert_eq!(
        fs::read(path).unwrap(),
        damaged,
        "trajectory init must not discard shared evidence to repair corruption"
    );
}

#[test]
fn local_command_announces_a_detected_version_transition_once() {
    let (_temp, root, _password) = setup_repo();
    let counter_path = root.join(".decapod/managed/version_counter.json");
    let mut counter: Value =
        serde_json::from_str(&std::fs::read_to_string(&counter_path).expect("read counter"))
            .expect("counter json");
    counter["last_seen_version"] = Value::String("0.96.13".to_string());
    std::fs::write(
        &counter_path,
        serde_json::to_string_pretty(&counter).expect("serialize counter"),
    )
    .expect("seed prior release");

    // Binary metadata must not migrate project state or consume its notice.
    let version = run_decapod(&root, &["system", "version"], None);
    assert!(version.status.success());
    assert!(!String::from_utf8_lossy(&version.stderr).contains("Decapod migration notice"));
    let after_version: Value = serde_json::from_slice(&fs::read(&counter_path).unwrap()).unwrap();
    assert_eq!(after_version["last_seen_version"], "0.96.13");

    let first = run_decapod(&root, &["workspace", "status"], None);
    assert!(first.status.success());
    let first_stderr = String::from_utf8_lossy(&first.stderr);
    assert!(first_stderr.contains("Decapod migration notice"));
    assert!(first_stderr.contains("0.96.13"));
    assert!(first_stderr.contains("applied.json"));

    let second = run_decapod(&root, &["workspace", "status"], None);
    assert!(second.status.success());
    assert!(!String::from_utf8_lossy(&second.stderr).contains("Decapod migration notice"));
}

#[test]
fn trajectory_cli_does_not_promote_completion_claim_without_checks() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_2",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "bounded implementation",
            "--boundary",
            "src/**",
            "--scope",
            "src/decapod/lib.rs",
        ],
        env_password,
    );
    json(&init, "trajectory init");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_2",
            "--completion-claim",
            "done",
        ],
        env_password,
    );
    let record_json = json(&record, "trajectory record");
    assert_eq!(record_json["completion_claim"], "done");
    assert_eq!(record_json["proof_status"], "no_checks_run");
    assert_eq!(record_json["verdicts"]["completion_proof"], "unsupported");
}

#[test]
fn trajectory_cli_rejects_unknown_check_status() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_3",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "bounded implementation",
        ],
        env_password,
    );
    json(&init, "trajectory init");

    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_3",
            "--check",
            "cargo_test=maybe",
        ],
        env_password,
    );
    assert!(!record.status.success());
    assert!(String::from_utf8_lossy(&record.stderr).contains("invalid trajectory check status"));
}

#[test]
fn trajectory_cli_records_typed_loop_attempts_and_proof_refs() {
    let (_temp, root, password) = setup_repo();
    let env_password = Some(password.as_str());
    let init = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "init",
            "--run-id",
            "run_cli_loops",
            "--intent-id",
            "intent:cli-request",
            "--original-intent",
            "preserve intent",
            "--derived-intent",
            "record governed loops",
            "--boundary",
            "src/**",
            "--scope",
            "src/decapod/lib.rs",
        ],
        env_password,
    );
    json(&init, "trajectory loop init");

    let failed = r#"{"intent_id":"intent:cli-request","trajectory_id":"run_cli_loops","loop_id":"verify","loop_type":"verification","attempt":1,"trigger":"grader","tool_calls":["cargo test"],"observations":["failed assertion"],"grader_result":"fail","feedback":"rerun the bounded check","proof_refs":[],"mutation_proposal":"none","status":"retrying"}"#;
    let passed = r#"{"intent_id":"intent:cli-request","trajectory_id":"run_cli_loops","loop_id":"verify","loop_type":"verification","attempt":2,"trigger":"grader","tool_calls":["cargo test"],"observations":["all assertions passed"],"grader_result":"pass","feedback":"","proof_refs":["check:cargo-test"],"mutation_proposal":"none","status":"passed"}"#;
    let record = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "record",
            "--run-id",
            "run_cli_loops",
            "--loop-json",
            failed,
            "--loop-json",
            passed,
        ],
        env_password,
    );
    let record_json = json(&record, "trajectory loop record");
    assert_eq!(record_json["loops"].as_array().unwrap().len(), 2);
    assert_eq!(record_json["loops"][0]["attempt"], 1);
    assert_eq!(record_json["loops"][1]["proof_refs"][0], "check:cargo-test");

    let status = run_decapod(
        &root,
        &[
            "govern",
            "trajectory",
            "status",
            "--run-id",
            "run_cli_loops",
        ],
        env_password,
    );
    let status_json = json(&status, "trajectory loop status");
    assert_eq!(status_json["loop_count"], 2);
}

#[test]
fn generated_agent_guidance_mentions_trajectory_proof() {
    let (_temp, root, password) = setup_repo();
    let _ = password;
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).expect("read AGENTS.md");
    assert!(agents.contains("govern trajectory init"));
    assert!(agents.contains("no_checks_run"));
}

#[test]
fn system_migrations_and_doctor_expose_notice_details_without_consuming_it() {
    let (_temp, root, _password) = setup_repo();
    let decapod_root = root.join(".decapod");
    let counter_path = decapod_root.join("managed/version_counter.json");
    let applied_path = decapod_root.join("managed/migrations/applied.json");

    // Trigger initial applied/catalog ledger creation once, then rewind counter + remove one applied entry.
    let prime = run_decapod(&root, &["workspace", "status"], None);
    assert!(prime.status.success());

    let mut counter: Value =
        serde_json::from_str(&fs::read_to_string(&counter_path).expect("read counter"))
            .expect("counter json");
    counter["last_seen_version"] = Value::String("0.143.2".to_string());
    fs::write(
        &counter_path,
        serde_json::to_string_pretty(&counter).expect("serialize counter"),
    )
    .expect("seed prior release");

    let mut applied_json: Value =
        serde_json::from_str(&fs::read_to_string(&applied_path).expect("read applied"))
            .expect("applied json");
    let entries = applied_json["entries"]
        .as_array_mut()
        .expect("applied entries array");
    entries.retain(|entry| entry["id"] != "events.compact_sequence_index.v002");
    fs::write(
        &applied_path,
        serde_json::to_string_pretty(&applied_json).expect("serialize applied"),
    )
    .expect("write trimmed applied ledger");

    let counter_snapshot = fs::read(&counter_path).unwrap();
    let applied_snapshot = fs::read(&applied_path).unwrap();

    // Read-only CLI inspection (no session password) reports details without mutating or consuming notice.
    let inspect_json_out = run_decapod(&root, &["system", "migrations", "--format", "json"], None);
    assert!(inspect_json_out.status.success());
    assert!(inspect_json_out.stderr.is_empty());
    let inspect = json(&inspect_json_out, "system migrations --format json");
    assert_eq!(inspect["action_required"], true);
    assert_eq!(inspect["version_counter"]["last_seen_version"], "0.143.2");
    assert_eq!(
        inspect["version_counter"]["version_changed_from_binary"],
        true
    );
    let pending = inspect["pending_migrations"]
        .as_array()
        .expect("pending migrations array");
    assert!(
        pending
            .iter()
            .any(|m| m["id"] == "events.compact_sequence_index.v002")
    );
    let reason_codes = inspect["reason_codes"]
        .as_array()
        .expect("reason codes array");
    assert!(
        reason_codes
            .iter()
            .any(|c| c == "VERSION_TRANSITION_PENDING")
    );
    assert!(reason_codes.iter().any(|c| c == "MIGRATIONS_PENDING"));
    assert_eq!(fs::read(&counter_path).unwrap(), counter_snapshot);
    assert_eq!(fs::read(&applied_path).unwrap(), applied_snapshot);

    let inspect_text_out = run_decapod(&root, &["system", "migrations", "--format", "text"], None);
    assert!(inspect_text_out.status.success());
    let text = String::from_utf8_lossy(&inspect_text_out.stdout);
    assert!(text.contains("action_required=true"));
    assert!(text.contains("0.143.2"));
    assert!(text.contains("events.compact_sequence_index.v002"));
    assert_eq!(fs::read(&counter_path).unwrap(), counter_snapshot);
    assert_eq!(fs::read(&applied_path).unwrap(), applied_snapshot);

    let doctor_out = run_decapod(
        &root,
        &["system", "doctor", "check", "--format", "json"],
        None,
    );
    assert!(doctor_out.status.success());
    let doctor_json = json(&doctor_out, "system doctor check --format json");
    let mig_check = doctor_json["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Migrations")
        .expect("Migrations check in doctor output");
    assert_eq!(mig_check["status"], "warn");
    assert!(
        mig_check["message"]
            .as_str()
            .unwrap()
            .contains("decapod system migrations --format json")
    );
    assert_eq!(fs::read(&counter_path).unwrap(), counter_snapshot);
    assert_eq!(fs::read(&applied_path).unwrap(), applied_snapshot);

    // First normal command emits the migration notice pointing to the read-only CLI and resolved root.
    let first = run_decapod(&root, &["workspace", "status"], None);
    assert!(first.status.success());
    let first_stderr = String::from_utf8_lossy(&first.stderr);
    assert!(first_stderr.contains("Decapod migration notice"));
    assert!(first_stderr.contains("0.143.2"));
    assert!(first_stderr.contains("events.compact_sequence_index.v002"));
    assert!(first_stderr.contains("decapod system migrations --format json"));
    assert!(first_stderr.contains("applied.json"));
    assert!(first_stderr.contains("catalog.json"));

    // After applying, read-only inspection shows steady state.
    let after = json(
        &run_decapod(&root, &["system", "migrations", "--format", "json"], None),
        "post-migration inspect",
    );
    assert_eq!(after["action_required"], false);
    assert!(after["pending_migrations"].as_array().unwrap().is_empty());
}

#[test]
fn migration_notice_and_system_migrations_report_main_decapod_root_from_linked_worktree() {
    let (temp, root, _password) = setup_repo();
    let decapod_root = root.join(".decapod");
    let prime = run_decapod(&root, &["workspace", "status"], None);
    assert!(prime.status.success());

    for git_args in [
        &["config", "user.email", "test@example.com"][..],
        &["config", "user.name", "Test User"][..],
        &["add", "-A"][..],
        &["commit", "-m", "initial"][..],
    ] {
        let out = Command::new("git")
            .current_dir(&root)
            .args(git_args)
            .output()
            .expect("git setup");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            git_args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let wt_dir = temp.path().join("linked-worktree");
    let wt_add = Command::new("git")
        .current_dir(&root)
        .args([
            "worktree",
            "add",
            "-b",
            "agent-wt-test",
            wt_dir.to_str().unwrap(),
        ])
        .output()
        .expect("git worktree add");
    assert!(
        wt_add.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&wt_add.stderr)
    );

    let counter_path = decapod_root.join("managed/version_counter.json");
    let mut counter: Value =
        serde_json::from_str(&fs::read_to_string(&counter_path).expect("read counter"))
            .expect("counter json");
    counter["last_seen_version"] = Value::String("0.143.2".to_string());
    fs::write(
        &counter_path,
        serde_json::to_string_pretty(&counter).expect("serialize counter"),
    )
    .expect("seed prior release");

    let wt_inspect = json(
        &run_decapod(&wt_dir, &["system", "migrations", "--format", "json"], None),
        "worktree system migrations",
    );
    assert_eq!(wt_inspect["action_required"], true);
    assert_eq!(wt_inspect["differs_from_workspace_decapod"], true);
    assert_eq!(
        fs::canonicalize(wt_inspect["decapod_root"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&decapod_root).unwrap()
    );
    assert_eq!(
        fs::canonicalize(wt_inspect["governance_root"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&root).unwrap()
    );
    assert_eq!(
        fs::canonicalize(wt_inspect["workspace_root"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&wt_dir).unwrap()
    );
    assert!(!wt_dir.join(".decapod/managed/migrations").exists());

    // Counter must still be pending for the first normal command from the worktree.
    let counter_after_inspect: Value =
        serde_json::from_slice(&fs::read(&counter_path).unwrap()).unwrap();
    assert_eq!(counter_after_inspect["last_seen_version"], "0.143.2");

    let wt_doctor = json(
        &run_decapod(
            &wt_dir,
            &["system", "doctor", "check", "--format", "json"],
            None,
        ),
        "worktree doctor check",
    );
    assert_eq!(wt_doctor["failed"], 0);
    let checks = wt_doctor["checks"].as_array().unwrap();
    assert_eq!(
        checks.iter().find(|c| c["name"] == ".decapod").unwrap()["status"],
        "pass"
    );
    assert_eq!(
        checks.iter().find(|c| c["name"] == "Migrations").unwrap()["status"],
        "warn"
    );

    let wt_status = run_decapod(&wt_dir, &["workspace", "status"], None);
    assert!(wt_status.status.success());
    let wt_stderr = String::from_utf8_lossy(&wt_status.stderr);
    let canonical_decapod_root = fs::canonicalize(&decapod_root).unwrap();
    assert!(wt_stderr.contains("Decapod migration notice"));
    assert!(wt_stderr.contains("decapod system migrations --format json"));
    assert!(wt_stderr.contains(&canonical_decapod_root.display().to_string()));
}

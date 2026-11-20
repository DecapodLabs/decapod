//! Regression tests for issue #1760: preserve explicit `paths:[...]` scopes
//! across todo, orientation, worktree seeding, trajectory, custody intent
//! constraints, and state resolution, plus fail-closed archive-gated repair
//! of already-fragmented scope projections.

use decapod::core::{governance_document as governance, trajectory};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const AGENT: &str = "scope-1760-test";

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

fn cli(root: &Path, password: Option<&str>, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(root)
        .args(args)
        .env("DECAPOD_AGENT_ID", AGENT)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .env("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1");
    if let Some(pw) = password {
        cmd.env("DECAPOD_SESSION_PASSWORD", pw);
    }
    cmd.output().expect("run decapod")
}

fn ok_json(out: Output, label: &str) -> Value {
    assert!(
        out.status.success(),
        "{label} failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}

fn init_repo_and_session() -> (TempDir, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    git(root, &["init", "-b", "master"]);
    git(root, &["config", "user.name", "Scope Tests"]);
    git(
        root,
        &["config", "user.email", "scope-tests@example.invalid"],
    );
    fs::write(root.join("README.md"), "Scope test repo\n").unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn ok() {}\n").unwrap();
    fs::write(root.join("docs/a,b.md"), "comma in filename\n").unwrap();
    git(root, &["add", "-A"]);
    git(root, &["commit", "-m", "initial"]);

    let init_out = cli(
        root,
        None,
        &["init", "--force", "--no-container-workspaces"],
    );
    assert!(
        init_out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    git(root, &["add", "-A"]);
    git(root, &["commit", "-m", "decapod init"]);

    let sess_out = cli(root, None, &["session", "acquire"]);
    assert!(
        sess_out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&sess_out.stderr)
    );
    let pw = String::from_utf8_lossy(&sess_out.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_string))
        .expect("session password");
    (temp, pw)
}

#[test]
fn explicit_path_scope_survives_todo_orientation_workspace_trajectory_and_state() {
    let (temp, pw) = init_repo_and_session();
    let root = temp.path();
    let scope_decl = r#"paths:["src","docs/a,b.md",".decapod/managed/specs"]"#;
    let expected_paths = json!([".decapod/managed/specs", "docs/a,b.md", "src"]);

    let added = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "todo",
                "--format",
                "json",
                "add",
                "Preserve explicit path scopes",
                "--description",
                "Verify JSON commas and path commas stay intact",
                "--scope",
                scope_decl,
            ],
        ),
        "todo add",
    );
    let task_id = added["id"].as_str().expect("todo id").to_string();

    let fetched = ok_json(
        cli(
            root,
            Some(&pw),
            &["todo", "--format", "json", "get", "--id", &task_id],
        ),
        "todo get",
    );
    assert_eq!(fetched["item"]["scope"], scope_decl);

    let claimed = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "todo",
                "--format",
                "json",
                "claim",
                "--id",
                &task_id,
                "--lease-seconds",
                "3600",
            ],
        ),
        "todo claim",
    );
    assert_eq!(claimed["status"], "ok");

    let ensured = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "workspace",
                "ensure",
                "--branch",
                &format!("agent/{AGENT}/{task_id}-explicit-scope"),
            ],
        ),
        "workspace ensure",
    );
    let wt = PathBuf::from(ensured["worktree_path"].as_str().expect("worktree_path"));

    let orientation = ok_json(
        cli(
            &wt,
            Some(&pw),
            &[
                "infer",
                "orientation",
                "--task-id",
                &task_id,
                "--format",
                "json",
            ],
        ),
        "infer orientation",
    );
    assert_eq!(orientation["allowed_scope"], expected_paths);

    let doc = governance::load(&wt)
        .expect("load governance")
        .expect("governance document");
    let traj_val = governance::read_section(&wt, "trajectory")
        .expect("read trajectory")
        .expect("trajectory section");
    assert_eq!(traj_val["repo_scope"], expected_paths);

    let run_id = traj_val["run_id"].as_str().expect("run_id").to_string();
    let intent_id = traj_val["intent_id"]
        .as_str()
        .expect("intent_id")
        .to_string();
    assert_eq!(doc.intents[&intent_id]["constraints"], expected_paths);

    let traj_cli = ok_json(
        cli(
            &wt,
            Some(&pw),
            &["govern", "trajectory", "get", "--run-id", &run_id],
        ),
        "govern trajectory get",
    );
    assert_eq!(traj_cli["repo_scope"], expected_paths);
    assert_eq!(
        traj_cli["custody"]["intents"][&intent_id]["constraints"],
        expected_paths
    );

    let state_cli = ok_json(
        cli(
            &wt,
            Some(&pw),
            &["rpc", "--op", "state.resolve", "--params", "{}"],
        ),
        "rpc state.resolve",
    );
    assert_eq!(
        state_cli["result"]["resolution"]["scope"]["repo_scope"],
        expected_paths
    );
    assert_eq!(
        state_cli["result"]["resolution"]["constraints"]["repo_scope"],
        expected_paths
    );
}

#[test]
fn trajectory_init_cli_parses_explicit_paths_declaration() {
    let (temp, pw) = init_repo_and_session();
    let root = temp.path();
    let scope_decl = r#"paths:["src","docs/a,b.md",".decapod/managed/specs"]"#;
    let expected_paths = json!([".decapod/managed/specs", "docs/a,b.md", "src"]);

    let added = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "todo",
                "--format",
                "json",
                "add",
                "Direct trajectory init scope",
                "--scope",
                scope_decl,
            ],
        ),
        "todo add",
    );
    let task_id = added["id"].as_str().expect("todo id").to_string();
    ok_json(
        cli(
            root,
            Some(&pw),
            &["todo", "--format", "json", "claim", "--id", &task_id],
        ),
        "todo claim",
    );
    let ensured = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "workspace",
                "ensure",
                "--branch",
                &format!("agent/{AGENT}/{task_id}-direct"),
            ],
        ),
        "workspace ensure",
    );
    let wt = PathBuf::from(ensured["worktree_path"].as_str().expect("worktree_path"));

    git(&wt, &["add", ".decapod/governance.json"]);
    git(&wt, &["commit", "-m", "archive initial governance"]);

    let init_json = ok_json(
        cli(
            &wt,
            Some(&pw),
            &[
                "govern",
                "trajectory",
                "init",
                "--run-id",
                "run_direct_1760",
                "--task-id",
                &task_id,
                "--original-intent",
                "Preserve explicit paths",
                "--derived-intent",
                "Direct trajectory init check",
                "--scope",
                scope_decl,
            ],
        ),
        "govern trajectory init",
    );
    assert_eq!(init_json["trajectory"]["repo_scope"], expected_paths);
    assert_eq!(
        init_json["trajectory"]["custody"]["intents"]["intent:run_direct_1760"]["constraints"],
        expected_paths
    );
}

#[test]
fn reconcile_scope_requires_archive_then_repairs_fragmented_projections_preserving_history() {
    let (temp, pw) = init_repo_and_session();
    let root = temp.path();
    let scope_decl = r#"paths:["src","docs/a,b.md",".decapod/managed/specs"]"#;
    let expected_paths = json!([".decapod/managed/specs", "docs/a,b.md", "src"]);
    let fragmented_paths: Vec<String> = scope_decl
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let added = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "todo",
                "--format",
                "json",
                "add",
                "Repair fragmented path scope",
                "--description",
                "Legacy fragmented governance recovery test",
                "--scope",
                scope_decl,
            ],
        ),
        "todo add",
    );
    let task_id = added["id"].as_str().expect("todo id").to_string();
    ok_json(
        cli(
            root,
            Some(&pw),
            &["todo", "--format", "json", "claim", "--id", &task_id],
        ),
        "todo claim",
    );
    let ensured = ok_json(
        cli(
            root,
            Some(&pw),
            &[
                "workspace",
                "ensure",
                "--branch",
                &format!("agent/{AGENT}/{task_id}-repair"),
            ],
        ),
        "workspace ensure",
    );
    let wt = PathBuf::from(ensured["worktree_path"].as_str().expect("worktree_path"));

    // Simulate an already-generated fragmented governance journal from pre-fix Decapod:
    // - fragmented repo_scope and custody intent constraints
    // - prior custody events and a failed scope finding check
    // - APPROVED plan state
    // - a recorded governance checkpoint
    let current_traj = governance::read_section(&wt, "trajectory")
        .unwrap()
        .unwrap();
    let run_id = current_traj["run_id"].as_str().unwrap().to_string();
    let intent_id = current_traj["intent_id"].as_str().unwrap().to_string();

    let mut legacy_traj: trajectory::TrajectoryArtifact =
        serde_json::from_value(current_traj).unwrap();
    legacy_traj.repo_scope = fragmented_paths.clone();
    if let Some(intent) = legacy_traj.custody.intents.get_mut(&intent_id) {
        intent.constraints = fragmented_paths.clone();
    }
    legacy_traj.checks.push(trajectory::TrajectoryCheck {
        name: "scope_projection".to_string(),
        status: trajectory::TrajectoryCheckStatus::Failed,
    });
    let legacy_traj = legacy_traj.with_recomputed_hash().unwrap();
    let original_events = legacy_traj.custody.events.clone();
    let original_raw_intent = legacy_traj.original_intent.clone();

    governance::write_section(
        &wt,
        "trajectory",
        &serde_json::to_value(&legacy_traj).unwrap(),
    )
    .unwrap();
    ok_json(
        cli(&wt, Some(&pw), &["govern", "plan", "approve"]),
        "approve legacy plan",
    );
    // Stage authored/refreshed material (excluding governance.json) before recording checkpoint.
    fs::write(wt.join("src/note.txt"), "legacy work\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["reset", "HEAD", "--", governance::GOVERNANCE_PATH]);
    ok_json(
        cli(
            &wt,
            Some(&pw),
            &[
                "govern",
                "artifacts",
                "checkpoint",
                "--id",
                "cp-legacy-1",
                "--summary",
                "Pre-repair legacy fragmented scope checkpoint",
                "--proof-ref",
                "test:legacy-observation",
            ],
        ),
        "checkpoint legacy state",
    );

    // Unarchived repair MUST fail closed with SCOPE_RECONCILIATION_ARCHIVE_REQUIRED.
    let unarchived_bytes = fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap();
    let unarchived_fail = cli(&wt, Some(&pw), &["govern", "trajectory", "reconcile-scope"]);
    assert!(
        !unarchived_fail.status.success(),
        "unarchived reconcile-scope must fail"
    );
    let stderr = String::from_utf8_lossy(&unarchived_fail.stderr);
    assert!(
        stderr.contains("SCOPE_RECONCILIATION_ARCHIVE_REQUIRED"),
        "expected SCOPE_RECONCILIATION_ARCHIVE_REQUIRED, got: {stderr}"
    );
    assert_eq!(
        unarchived_bytes,
        fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap(),
        "failed unarchived repair must not mutate governance.json"
    );

    // Commit the complete fragmented legacy journal at HEAD, then repair.
    git(&wt, &["add", ".decapod/governance.json"]);
    git(&wt, &["commit", "-m", "commit fragmented legacy journal"]);

    let repaired = ok_json(
        cli(&wt, Some(&pw), &["govern", "trajectory", "reconcile-scope"]),
        "reconcile-scope after archive",
    );
    assert_eq!(repaired["status"], "reconciled");
    assert_eq!(repaired["marker"], "SCOPE_RECONCILED");
    assert_eq!(repaired["repaired_repo_scope"], expected_paths);
    assert_eq!(repaired["plan_state"], "DRAFT");

    let doc_after = governance::load(&wt).unwrap().unwrap();
    assert_eq!(doc_after.checkpoints.len(), 1);
    assert_eq!(doc_after.checkpoints[0].id, "cp-legacy-1");
    assert_eq!(doc_after.intents[&intent_id]["constraints"], expected_paths);

    let plan_after = governance::read_section(&wt, "plan").unwrap().unwrap();
    assert_eq!(plan_after["state"], "DRAFT");

    let traj_after = ok_json(
        cli(
            &wt,
            Some(&pw),
            &["govern", "trajectory", "get", "--run-id", &run_id],
        ),
        "trajectory get after repair",
    );
    assert_eq!(traj_after["repo_scope"], expected_paths);
    assert_eq!(traj_after["original_intent"], original_raw_intent);
    assert_eq!(
        traj_after["checks"],
        json!([{"name": "scope_projection", "status": "failed"}])
    );
    assert_eq!(
        serde_json::to_value(&original_events).unwrap(),
        traj_after["custody"]["events"]
    );
    assert_eq!(
        traj_after["custody"]["intents"][&intent_id]["constraints"],
        expected_paths
    );

    // Independent reader B (`rpc --op state.resolve`) and canonical task readback agree.
    let state_after = ok_json(
        cli(
            &wt,
            Some(&pw),
            &["rpc", "--op", "state.resolve", "--params", "{}"],
        ),
        "state.resolve after repair",
    );
    assert_eq!(
        state_after["result"]["resolution"]["scope"]["repo_scope"],
        expected_paths
    );
    assert_eq!(
        state_after["result"]["resolution"]["constraints"]["repo_scope"],
        expected_paths
    );

    let task_after = ok_json(
        cli(
            &wt,
            Some(&pw),
            &["todo", "--format", "json", "get", "--id", &task_id],
        ),
        "todo get after repair",
    );
    assert_eq!(task_after["item"]["scope"], scope_decl);

    // Repeat repair is idempotent.
    let before_repeat = fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap();
    let repeat = ok_json(
        cli(&wt, Some(&pw), &["govern", "trajectory", "reconcile-scope"]),
        "repeat reconcile-scope",
    );
    assert_eq!(repeat["status"], "unchanged");
    assert_eq!(repeat["marker"], "SCOPE_ALREADY_CANONICAL");
    assert_eq!(
        before_repeat,
        fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap()
    );

    // Ambiguous task / unknown scope mismatch fails closed without modifying governance.json.
    let wrong_task = cli(
        &wt,
        Some(&pw),
        &[
            "govern",
            "trajectory",
            "reconcile-scope",
            "--task-id",
            "bugs_0000000000000000",
        ],
    );
    assert!(!wrong_task.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_task.stderr).contains("SCOPE_RECONCILIATION_AMBIGUOUS_TASK")
    );
    assert_eq!(
        before_repeat,
        fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap()
    );

    // Ambiguous foreign scope entries fail closed without modifying governance.json.
    let mut ambiguous_traj = serde_json::from_value::<trajectory::TrajectoryArtifact>(
        governance::read_section(&wt, "trajectory")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    ambiguous_traj.repo_scope = vec!["foreign/unrelated".to_string()];
    let ambiguous_traj = ambiguous_traj.with_recomputed_hash().unwrap();
    governance::write_section(
        &wt,
        "trajectory",
        &serde_json::to_value(&ambiguous_traj).unwrap(),
    )
    .unwrap();
    let ambiguous_bytes = fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap();
    let ambiguous_fail = cli(&wt, Some(&pw), &["govern", "trajectory", "reconcile-scope"]);
    assert!(!ambiguous_fail.status.success());
    assert!(
        String::from_utf8_lossy(&ambiguous_fail.stderr).contains("SCOPE_RECONCILIATION_AMBIGUOUS")
    );
    assert_eq!(
        ambiguous_bytes,
        fs::read(wt.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

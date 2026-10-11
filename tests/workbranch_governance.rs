//! Normal agent/workspace entrypoints must never expose the last accepted PR as
//! the current task. These tests exercise actual Git worktrees and CLI calls.
use decapod::core::{governance_document as governance, todo, workspace};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

const AGENT: &str = "branch-template-test";

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", AGENT)
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES")
        .output()
        .unwrap()
}
fn success(out: Output) -> Value {
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
}
fn fixture() -> (TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-b", "master"]);
    git(root.path(), &["config", "user.name", "Branch Tests"]);
    git(
        root.path(),
        &["config", "user.email", "test@example.invalid"],
    );
    fs::write(
        root.path().join("README.md"),
        "Branch initialization fixture\n",
    )
    .unwrap();
    git(root.path(), &["add", "README.md"]);
    git(root.path(), &["commit", "-m", "initial"]);
    success(cli(
        root.path(),
        &["init", "--force", "--no-container-workspaces"],
    ));
    governance::begin_pr(root.path(), "accepted-change", "master").unwrap();
    governance::write_section(
        root.path(),
        "plan",
        &json!({
            "schema_version":"1.1.0", "title":"Accepted old task", "intent":"Old accepted intent",
            "state":"DONE", "constraints":{}, "updated_at":"0Z"
        }),
    )
    .unwrap();
    governance::record_claim(
        root.path(),
        "old-supported",
        "Old result",
        "Counterexample",
        "supported",
        vec!["historical-proof".into()],
    )
    .unwrap();
    git(root.path(), &["add", "-A"]);
    git(root.path(), &["commit", "-m", "accepted governance"]);
    let task = success(cli(
        root.path(),
        &[
            "todo",
            "add",
            "Implement current task",
            "--description",
            "Current task context only",
            "--scope",
            "src,tests",
            "--format",
            "json",
        ],
    ));
    let id = task["id"].as_str().unwrap().to_string();
    todo::claim_task(
        &root.path().join(".decapod/data"),
        &id,
        AGENT,
        todo::ClaimMode::Exclusive,
    )
    .unwrap();
    (root, id)
}
fn manual_worktree(root: &Path, id: &str) -> PathBuf {
    let path = root.join(".decapod/workspaces/manual");
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            &format!("agent/manual/{id}"),
            path.to_str().unwrap(),
            "master",
        ],
    );
    path
}
fn assert_fresh(path: &Path, id: &str) -> governance::GovernanceDocument {
    let doc = governance::load(path).unwrap().unwrap();
    assert_ne!(doc.change.as_ref().unwrap().id, "accepted-change");
    assert!(doc.claims.is_empty());
    assert!(doc.checkpoints.is_empty());
    assert!(!doc.sections.contains_key("validation"));
    let plan = governance::read_section(path, "plan").unwrap().unwrap();
    assert_eq!(plan["state"], "DRAFT");
    assert_eq!(plan["todo_ids"], json!([id]));
    assert!(
        plan["intent"]
            .as_str()
            .unwrap()
            .contains("Current task context only")
    );
    let trajectory = governance::read_section(path, "trajectory")
        .unwrap()
        .unwrap();
    assert_eq!(trajectory["task_id"], id);
    assert_eq!(trajectory["repo_scope"], json!(["src", "tests"]));
    assert_eq!(trajectory["proof_status"], "no_checks_run");
    doc
}

#[test]
fn workspace_ensure_seeds_current_context_and_repeat_is_noop() {
    let (root, id) = fixture();
    let accepted = fs::read(root.path().join(governance::GOVERNANCE_PATH)).unwrap();
    // The workspace creation event supplies its exact base even when Git does
    // not retain a branch-creation reflog.
    git(root.path(), &["config", "core.logAllRefUpdates", "false"]);
    let config = workspace::WorkspaceConfig {
        branch: Some(format!("agent/current/{id}")),
        use_container: false,
        base_image: None,
        base_branch: Some("master".into()),
    };
    let status = workspace::ensure_workspace(root.path(), Some(config), AGENT).unwrap();
    let path = status.git.worktree_path.unwrap();
    assert_fresh(&path, &id);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    workspace::ensure_workspace(&path, None, AGENT).unwrap();
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    assert_eq!(
        accepted,
        fs::read(root.path().join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn agent_init_seeds_manual_worktree_but_status_does_not() {
    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    success(cli(&path, &["workspace", "status"]));
    success(cli(&path, &["govern", "artifacts", "status"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_fresh(&path, &id);
    assert_eq!(
        before,
        fs::read(root.path().join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn validation_preflight_initializes_without_fabricating_success() {
    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    // Deliberately fail a real gate after initialization. No success receipt may
    // appear simply because a template was generated.
    fs::remove_file(path.join("AGENTS.md")).unwrap();
    let output = cli(&path, &["validate", "--format", "json"]);
    assert!(!output.status.success());
    assert_fresh(&path, &id);
}

#[test]
fn successful_validation_issues_new_bound_receipt_after_template_generation() {
    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    let fresh = assert_fresh(&path, &id);
    success(cli(&path, &["govern", "plan", "approve"]));
    let spec = path.join(".decapod/managed/specs/INTENT.md");
    let mut material = fs::read_to_string(&spec).unwrap();
    material.push_str("\n## Current task\nImplement current task with branch-local governance and measured validation.\n");
    fs::write(&spec, material).unwrap();
    success(cli(&path, &["rpc", "--op", "specs.refresh"]));
    for spec in ["INTERFACES", "ARCHITECTURE", "SECURITY"] {
        success(cli(
            &path,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                &format!(".decapod/managed/specs/{spec}.md"),
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                "Fixture changes only task intent; implementation, interfaces, and security remain unchanged",
            ],
        ));
    }
    git(&path, &["add", "-A"]);
    governance::checkpoint(
        &path,
        "current-work",
        "Current task and material specification",
        vec![],
    )
    .unwrap();
    git(&path, &["add", governance::GOVERNANCE_PATH]);
    git(&path, &["commit", "-m", "current task"]);
    let output = cli(&path, &["validate", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let doc = governance::load(&path).unwrap().unwrap();
    assert_eq!(
        doc.change.as_ref().unwrap().id,
        fresh.change.as_ref().unwrap().id
    );
    let receipt = governance::read_section(&path, "validation")
        .unwrap()
        .unwrap();
    let receipt: decapod::core::validate::ValidationReceipt =
        serde_json::from_value(receipt).unwrap();
    receipt.validate_integrity().unwrap();
    let trajectory = governance::read_section(&path, "trajectory")
        .unwrap()
        .unwrap();
    assert_eq!(trajectory["task_id"], id);
    assert_eq!(trajectory["proof_status"], "passed");
    assert!(
        trajectory["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["name"] == "decapod validate" && check["status"] == "passed")
    );
}

#[test]
fn agent_init_cannot_use_its_own_task_to_reset_another_agents_branch() {
    let (root, _) = fixture();
    let other = success(cli(
        root.path(),
        &["todo", "add", "Other agent task", "--format", "json"],
    ));
    let other = other["id"].as_str().unwrap();
    todo::claim_task(
        &root.path().join(".decapod/data"),
        other,
        "other-agent",
        todo::ClaimMode::Exclusive,
    )
    .unwrap();
    let path = manual_worktree(root.path(), other);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    let output = cli(&path, &["rpc", "--op", "agent.init"]);
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        message.contains("WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT"),
        "{message}"
    );
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn agent_init_does_not_mutate_governance_in_noncanonical_worktree() {
    let (root, id) = fixture();
    let path = root.path().join("noncanonical");
    git(
        root.path(),
        &[
            "worktree",
            "add",
            "-b",
            &format!("agent/external/{id}"),
            path.to_str().unwrap(),
            "master",
        ],
    );
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn explicit_begin_pr_refuses_protected_worktree_branch() {
    let (root, id) = fixture();
    let path = root.path().join(".decapod/workspaces/protected");
    git(
        root.path(),
        &[
            "worktree",
            "add",
            "-b",
            &format!("release/{id}"),
            path.to_str().unwrap(),
            "master",
        ],
    );
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    let output = cli(
        &path,
        &[
            "govern",
            "artifacts",
            "begin-pr",
            "--id",
            "forbidden-reset",
            "--base-branch",
            "master",
        ],
    );
    assert!(!output.status.success());
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn agent_init_establishes_session_before_initializing_current_context() {
    let (root, id) = fixture();
    success(cli(root.path(), &["session", "release"]));
    todo::claim_task(
        &root.path().join(".decapod/data"),
        &id,
        AGENT,
        todo::ClaimMode::Exclusive,
    )
    .unwrap();
    let path = manual_worktree(root.path(), &id);
    let output = cli(&path, &["rpc", "--op", "agent.init"]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("auto-acquired"));
    success(output);
    assert_fresh(&path, &id);
}

#[test]
fn agent_init_does_not_substitute_unrelated_task_for_missing_branch_identity() {
    let (root, _) = fixture();
    let path = manual_worktree(root.path(), "code_01k0000000000000");
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    let output = cli(&path, &["rpc", "--op", "agent.init"]);
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        message.contains("GOVERNANCE_WORK_CONTEXT_REQUIRED"),
        "{message}"
    );
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

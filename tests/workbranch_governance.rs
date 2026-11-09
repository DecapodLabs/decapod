//! Normal agent/workspace entrypoints must never expose the last accepted PR as
//! the current task. These tests exercise actual Git worktrees and CLI calls.
use decapod::core::{governance_document as governance, todo};
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
    recovery_cli(root, AGENT, args)
}

fn admit(root: &Path) {
    success(cli(root, &["workspace", "ensure"]));
    let status = success(cli(root, &["workspace", "status"]));
    assert_eq!(status["workspace"]["can_work"], true, "{status}");
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
    let status = success(cli(
        root.path(),
        &[
            "workspace",
            "ensure",
            "--branch",
            &format!("agent/current/{id}"),
        ],
    ));
    let path = PathBuf::from(status["worktree_path"].as_str().unwrap());
    assert_fresh(&path, &id);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    admit(&path);
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
fn manual_worktree_observation_preserves_history_until_explicit_admission() {
    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    let status = success(cli(&path, &["workspace", "status"]));
    assert_eq!(status["workspace"]["can_work"], false);
    success(cli(&path, &["govern", "artifacts", "status"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    admit(&path);
    assert_fresh(&path, &id);
    let admitted = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_eq!(
        admitted,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    assert_eq!(
        before,
        fs::read(root.path().join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn validation_preflight_initializes_without_fabricating_success() {
    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    admit(&path);
    // Deliberately fail a real gate after initialization. No success receipt may
    // appear simply because a template was generated.
    fs::remove_file(path.join("AGENTS.md")).unwrap();
    let output = cli(&path, &["validate", "--format", "json"]);
    assert!(!output.status.success());
    assert_fresh(&path, &id);
}

#[test]
fn successful_validation_issues_new_bound_receipt_after_template_generation() {
    // Bind the current branch methodology receipt without claiming full readiness.
    // This prerequisite result is not full publication-readiness proof.

    let (root, id) = fixture();
    let path = manual_worktree(root.path(), &id);
    admit(&path);
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
    let output = cli(&path, &["govern", "project-check", "--format", "json"]);
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["scope"], "project_prerequisites");
    assert!(result["readiness"].is_null());
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
    success(output);
    let status = success(cli(&path, &["workspace", "status"]));
    assert_eq!(status["workspace"]["can_work"], false);
    let output = cli(&path, &["workspace", "ensure"]);
    assert!(!output.status.success());
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
fn released_session_requires_fresh_task_custody_before_authoring_resumes() {
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
    admit(&path);
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    success(cli(&path, &["session", "release"]));
    // Session release deliberately releases its task assignments too. An
    // observation must not silently reacquire that custody or reuse the binding.
    let status = success(cli(&path, &["workspace", "status"]));
    assert_eq!(status["workspace"]["can_work"], false);
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
    success(cli(&path, &["todo", "claim", "--id", &id]));
    admit(&path);
    success(cli(&path, &["rpc", "--op", "agent.init"]));
    assert_fresh(&path, &id);
}

#[test]
fn agent_init_does_not_substitute_unrelated_task_for_missing_branch_identity() {
    let (root, _) = fixture();
    let path = manual_worktree(root.path(), "code_01k0000000000000");
    let before = fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap();
    let output = cli(&path, &["rpc", "--op", "agent.init"]);
    success(output);
    let status = success(cli(&path, &["workspace", "status"]));
    assert_eq!(status["workspace"]["can_work"], false);
    let output = cli(&path, &["workspace", "ensure"]);
    assert!(!output.status.success());
    let message = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        message.contains("WORKSPACE_BRANCH_NOT_TODO_SCOPED"),
        "{message}"
    );
    assert_eq!(
        before,
        fs::read(path.join(governance::GOVERNANCE_PATH)).unwrap()
    );
}

fn recovery_cli(root: &Path, actor: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_decapod"));
    for (key, _) in std::env::vars_os() {
        let key_text = key.to_string_lossy();
        if key_text.starts_with("DECAPOD_") || key_text.starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .args(args)
        .current_dir(root)
        .env("DECAPOD_AGENT_ID", actor)
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .output()
        .unwrap()
}

fn recovery_commit(root: &Path, actor: &str, id: &str) -> String {
    git(root, &["add", "-A"]);
    success(recovery_cli(
        root,
        actor,
        &[
            "govern",
            "artifacts",
            "checkpoint",
            "--id",
            id,
            "--summary",
            "Present-time recovery evidence; no retrospective proof",
        ],
    ));
    git(root, &["add", governance::GOVERNANCE_PATH]);
    git(root, &["commit", "-m", id]);
    git(root, &["rev-parse", "HEAD"])
}

#[test]
fn actual_cli_reauthors_gapped_history_with_fresh_custody_and_no_retrospective_proof() {
    let (root, source_task) = fixture();
    let root = root.path();
    let base = git(root, &["rev-parse", "master"]);
    let source_branch = format!("agent/{AGENT}/{source_task}-failed-history");
    let source_workspace = success(recovery_cli(
        root,
        AGENT,
        &["workspace", "ensure", "--branch", &source_branch],
    ));
    let source_path = PathBuf::from(source_workspace["worktree_path"].as_str().unwrap());
    fs::create_dir_all(source_path.join("src")).unwrap();
    fs::write(
        source_path.join("src/recovered.rs"),
        "pub fn answer() -> u8 { 41 }\n",
    )
    .unwrap();
    recovery_commit(&source_path, AGENT, "source-baseline");
    fs::write(
        source_path.join("src/recovered.rs"),
        "pub fn answer() -> u8 { 42 }\n",
    )
    .unwrap();
    git(&source_path, &["add", "src/recovered.rs"]);
    git(
        &source_path,
        &["commit", "-m", "deliberate historical checkpoint gap"],
    );
    let source_head = git(&source_path, &["rev-parse", "HEAD"]);
    let source_tree = git(&source_path, &["rev-parse", "HEAD^{tree}"]);
    let failed = recovery_cli(
        root,
        "unassigned-history-observer",
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            &base,
            "--head-ref",
            &source_head,
            "--target-base",
            "master",
            "--format",
            "json",
        ],
    );
    assert!(!failed.status.success());
    let diagnosis: Value = serde_json::from_slice(&failed.stdout).unwrap();
    assert_eq!(diagnosis["disposition"], "unproven_history");
    assert_eq!(diagnosis["findings"][0]["commit"], source_head);
    assert_eq!(
        diagnosis["findings"][0]["prior_event_can_be_regenerated"],
        false
    );

    let receiver = "history-recovery-independent-agent";
    // Establish the second actor through its own actual session, never by
    // copying the source actor's credentials or using an operator label.
    assert!(
        recovery_cli(root, receiver, &["session", "acquire"])
            .status
            .success()
    );
    let task = success(recovery_cli(
        root,
        receiver,
        &[
            "todo",
            "add",
            "Reauthor reviewed source with fresh proof",
            "--scope",
            "src,tests",
            "--format",
            "json",
        ],
    ));
    let task_id = task["id"].as_str().unwrap();
    let claim = success(recovery_cli(
        root,
        receiver,
        &["todo", "claim", "--id", task_id, "--format", "json"],
    ));
    assert_eq!(claim["result"]["assigned_to"], receiver);
    let branch = format!("agent/{receiver}/{task_id}-fresh-history");
    let destination = success(recovery_cli(
        root,
        receiver,
        &["workspace", "ensure", "--branch", &branch],
    ));
    assert_eq!(destination["can_work"], true);
    let path = PathBuf::from(destination["worktree_path"].as_str().unwrap());
    assert_ne!(path, source_path);
    let foreign = recovery_cli(
        &path,
        AGENT,
        &[
            "govern",
            "artifacts",
            "record-history-recovery",
            "--source-ref",
            &source_head,
            "--reason",
            "not my checkout",
        ],
    );
    assert!(!foreign.status.success());
    let args = [
        "govern",
        "artifacts",
        "record-history-recovery",
        "--source-ref",
        source_head.as_str(),
        "--reason",
        "Preserve old evidence; reauthor and obtain fresh proof",
    ];
    let observation = success(recovery_cli(&path, receiver, &args));
    let id = format!("history-recovery:{source_head}");
    assert_eq!(
        observation["baseline"]["unresolved"][&id]["actor"],
        receiver
    );
    assert_eq!(
        observation["baseline"]["unresolved"][&id]["exception_authorized"],
        false
    );
    let repeat = success(recovery_cli(&path, receiver, &args));
    assert_eq!(
        repeat["baseline"]["unresolved"][&id],
        observation["baseline"]["unresolved"][&id]
    );
    let premature = recovery_cli(
        &path,
        receiver,
        &[
            "govern",
            "artifacts",
            "resolve-obligation",
            "--id",
            &id,
            "--resolution",
            "current observation",
            "--proof-ref",
            &format!("git:{source_head}"),
        ],
    );
    assert!(!premature.status.success());
    assert!(String::from_utf8_lossy(&premature.stderr).contains("NOT_ARCHIVED"));

    // Review and reauthor selected source bytes in the new, verified checkout.
    // Do not merge/cherry-pick the invalid governance history or generated files.
    fs::create_dir_all(path.join("src")).unwrap();
    let reviewed_source = git(
        &source_path,
        &["show", &format!("{source_head}:src/recovered.rs")],
    );
    fs::write(
        path.join("src/recovered.rs"),
        format!("{reviewed_source}\n"),
    )
    .unwrap();
    let intent = path.join(".decapod/managed/specs/INTENT.md");
    fs::write(&intent, format!("{}\n## Recovery fixture outcome\n\nThis consumer reauthors a reviewed Rust function in a fresh admitted workstream. The historical source remains unproven; current checkpoints and explicit spec reviews are required.\n", fs::read_to_string(&intent).unwrap())).unwrap();
    success(recovery_cli(
        &path,
        receiver,
        &["govern", "plan", "approve"],
    ));
    success(recovery_cli(
        &path,
        receiver,
        &["rpc", "--op", "specs.refresh"],
    ));
    let archived = recovery_commit(&path, receiver, "observed-gap-and-reauthored-work");
    let missing_review = recovery_cli(
        &path,
        receiver,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            &base,
            "--head-ref",
            &archived,
            "--target-base",
            "master",
            "--publication-material",
        ],
    );
    assert!(!missing_review.status.success());
    assert!(
        String::from_utf8_lossy(&missing_review.stderr).contains("SPEC_REVIEW_REQUIRED"),
        "{}",
        String::from_utf8_lossy(&missing_review.stderr)
    );
    // A missing review has an actual, reachable resolution before checkpoint
    // verification. Recording review does not require passing publication first.
    for spec in ["INTERFACES", "ARCHITECTURE", "SECURITY"] {
        success(recovery_cli(
            &path,
            receiver,
            &[
                "govern",
                "plan",
                "review-spec",
                "--path",
                &format!(".decapod/managed/specs/{spec}.md"),
                "--disposition",
                "unchanged-with-reason",
                "--reason",
                "Reviewed the single Rust fixture function against the unchanged consumer contract",
            ],
        ));
    }
    success(recovery_cli(
        &path,
        receiver,
        &[
            "govern",
            "artifacts",
            "resolve-obligation",
            "--id",
            &id,
            "--resolution",
            "Reviewed current reauthored work; old history remains unproven; full independent publication proof is still required",
            "--proof-ref",
            &format!("git:{archived}"),
            "--proof-ref",
            &format!("git:{source_head}"),
        ],
    ));
    let final_head = recovery_commit(&path, receiver, "fresh-governed-recovery");
    let passed = success(recovery_cli(
        &path,
        receiver,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            &base,
            "--head-ref",
            &final_head,
            "--target-base",
            "master",
            "--format",
            "json",
        ],
    ));
    assert_eq!(passed["history_proven"], true);
    assert_eq!(passed["publication_ready"], false);
    success(recovery_cli(
        &path,
        receiver,
        &[
            "govern",
            "artifacts",
            "verify-checkpoints",
            "--base-branch",
            &base,
            "--head-ref",
            &final_head,
            "--target-base",
            "master",
            "--publication-material",
        ],
    ));
    assert_eq!(git(&source_path, &["rev-parse", "HEAD"]), source_head);
    assert_eq!(
        git(&source_path, &["rev-parse", "HEAD^{tree}"]),
        source_tree
    );
    assert!(
        !recovery_cli(
            &source_path,
            AGENT,
            &[
                "govern",
                "artifacts",
                "verify-checkpoints",
                "--base-branch",
                &base,
                "--head-ref",
                &source_head,
                "--target-base",
                "master",
            ]
        )
        .status
        .success()
    );
    let retained = git(
        &path,
        &[
            "show",
            &format!("{archived}:{}", governance::GOVERNANCE_PATH),
        ],
    );
    let retained: Value = serde_json::from_str(&retained).unwrap();
    assert_eq!(
        retained["baseline"]["unresolved"][&id]["historical_proof"],
        "not_proven"
    );
    assert_eq!(git(root, &["rev-parse", "master"]), base);
}

use decapod::core::{
    assets, docs_cli,
    readiness_assessment::{Decision, Mode},
    validate,
    validation_runtime::ValidationGuidance,
    workspace,
};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn assert_safe_todo_cleanup_text(text: &str, expected_task_id: Option<&str>) {
    assert!(
        text.contains("flush ONLY") || text.contains("delete ONLY"),
        "expected todo-scoped cleanup directive in: {text}"
    );
    assert!(
        text.contains("wildcard or root-level deletion of /tmp")
            || text.contains("wildcard or root-level deletion of `/tmp`"),
        "expected explicit prohibition on wildcard or root-level deletion of /tmp in: {text}"
    );
    assert!(
        text.contains("/tmp/decapod"),
        "expected protection of shared `/tmp/decapod` controller state in: {text}"
    );
    assert!(
        text.contains("concurrent agent") || text.contains("sibling"),
        "expected protection of concurrent agent / sibling directories in: {text}"
    );
    if let Some(task_id) = expected_task_id {
        assert!(
            text.contains(task_id),
            "expected todo id `{task_id}` in cleanup instruction: {text}"
        );
        assert!(
            text.contains(&format!("decapod_validate/{task_id}")),
            "expected todo-scoped decapod_validate/{task_id} path in: {text}"
        );
    }
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("execute git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn run_decapod(dir: &Path, envs: &[(&str, &str)], args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir)
        .env("DECAPOD_GROUP_BROKER_DISABLE", "1")
        .args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("execute decapod")
}

#[test]
fn todo_tmp_cleanup_helpers_scope_to_task_and_forbid_destructive_globs() {
    let temp = TempDir::new().unwrap();
    let branch_repo = temp.path().join("branch_repo");
    fs::create_dir_all(&branch_repo).unwrap();
    run_git(&branch_repo, &["init", "-q", "-b", "master"]);
    run_git(&branch_repo, &["config", "user.name", "Tmp Cleanup Test"]);
    run_git(
        &branch_repo,
        &["config", "user.email", "tmpclean@example.invalid"],
    );
    fs::write(branch_repo.join("README.md"), "# test\n").unwrap();
    run_git(&branch_repo, &["add", "README.md"]);
    run_git(&branch_repo, &["commit", "-q", "-m", "initial"]);
    run_git(
        &branch_repo,
        &[
            "checkout",
            "-q",
            "-b",
            "agent/swarm-tmpclean/bugs_01m7r4tqkjg4a8nh-tmp-cleanup",
        ],
    );

    let resolved = validate::resolve_validation_task_id(&branch_repo);
    assert_eq!(resolved, "bugs_01m7r4tqkjg4a8nh");
    let root = validate::validation_temp_root(&branch_repo);
    assert!(root.ends_with("decapod_validate/bugs_01m7r4tqkjg4a8nh"));

    let guidance = validate::todo_tmp_cleanup_instruction(&branch_repo);
    assert_safe_todo_cleanup_text(&guidance, Some("bugs_01m7r4tqkjg4a8nh"));

    let explicit = validate::todo_tmp_cleanup_instruction_for_task("code_01abc999xyz", None);
    assert_safe_todo_cleanup_text(&explicit, Some("code_01abc999xyz"));
    assert!(explicit.contains("/tmp/decapod_validate/code_01abc999xyz"));

    let warnings = workspace::pr_admission_warnings(&branch_repo);
    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].contains("PR admission applies to the observed head and base only"));
    assert_safe_todo_cleanup_text(&warnings[1], Some("bugs_01m7r4tqkjg4a8nh"));
}

#[test]
fn validation_guidance_surfaces_tmp_cleanup_only_when_ready_for_pr() {
    let passed = ValidationGuidance::for_outcome(true);
    assert_safe_todo_cleanup_text(passed.instruction, None);

    let accepted =
        ValidationGuidance::for_readiness_outcome(true, Mode::Required, Decision::Accept);
    assert_safe_todo_cleanup_text(accepted.instruction, None);

    let failed = ValidationGuidance::for_outcome(false);
    assert!(
        !failed
            .instruction
            .contains("wildcard or root-level deletion of /tmp")
    );
    assert!(!failed.instruction.contains("Once PR creation completes"));

    let rejected =
        ValidationGuidance::for_readiness_outcome(true, Mode::Required, Decision::Reject);
    assert!(!rejected.instruction.contains("Once PR creation completes"));
}

#[test]
fn embedded_agent_docs_and_docs_ingest_include_todo_tmp_cleanup_contract() {
    let readiness = docs_cli::readiness_contract();
    assert_safe_todo_cleanup_text(readiness, None);

    let command_contracts = include_str!("../docs/agent/command-contracts.md");
    assert!(command_contracts.contains(readiness));
    assert_safe_todo_cleanup_text(command_contracts, None);

    let state_model = include_str!("../docs/agent/state-model.md");
    assert_safe_todo_cleanup_text(state_model, None);

    let embedded_contracts = assets::get_embedded_doc("docs/agent/command-contracts.md").unwrap();
    assert!(embedded_contracts.contains("Todo-scoped `/tmp` cleanup"));

    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".decapod")).unwrap();
    let ingest = run_decapod(temp.path(), &[], &["docs", "ingest"]);
    assert!(
        ingest.status.success(),
        "{}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let stdout = String::from_utf8(ingest.stdout).unwrap();
    assert_safe_todo_cleanup_text(&stdout, None);
    assert!(stdout.contains("Scratch cleanup:"));
}

fn assert_eval_allows(dir: &Path, label: &str, payload: &str) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["eval", "--stdin", "--format", "json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn decapod eval");
    {
        let stdin = child.stdin.as_mut().expect("eval stdin");
        stdin
            .write_all(payload.as_bytes())
            .expect("write eval stdin");
    }
    let out = child.wait_with_output().expect("wait for decapod eval");
    assert!(
        out.status.success(),
        "{label}: decapod eval exited nonzero: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let parsed: Value = serde_json::from_slice(&out.stdout).expect("parse eval JSON");
    assert_eq!(
        parsed["status"], "allow",
        "{label}: expected eval allow, got: {parsed}"
    );
}

#[test]
fn todo_cleanup_guidance_and_docs_ingest_pass_prompt_safety_gate() {
    let temp = TempDir::new().unwrap();
    fs::create_dir_all(temp.path().join(".decapod")).unwrap();

    let instruction =
        validate::todo_tmp_cleanup_instruction_for_task("bugs_01m7r4tqkjg4a8nh", None);
    assert_eval_allows(
        temp.path(),
        "todo_tmp_cleanup_instruction_for_task",
        &instruction,
    );

    let outcome_guidance = ValidationGuidance::for_outcome(true).instruction;
    assert_eval_allows(
        temp.path(),
        "ValidationGuidance::for_outcome(true)",
        outcome_guidance,
    );

    let accepted_guidance =
        ValidationGuidance::for_readiness_outcome(true, Mode::Required, Decision::Accept)
            .instruction;
    assert_eval_allows(
        temp.path(),
        "ValidationGuidance::for_readiness_outcome(Accept)",
        accepted_guidance,
    );

    let ingest = run_decapod(temp.path(), &[], &["docs", "ingest"]);
    assert!(
        ingest.status.success(),
        "docs ingest failed: {}",
        String::from_utf8_lossy(&ingest.stderr)
    );
    let docs_stdout = String::from_utf8(ingest.stdout).unwrap();
    assert_eval_allows(temp.path(), "docs ingest", &docs_stdout);
}

#[test]
fn cli_qa_diagnose_surfaces_todo_scoped_cleanup_guidance_when_ready_to_publish() {
    let temp = TempDir::new().unwrap();
    let repo = temp.path();
    run_git(repo, &["init", "-q", "-b", "master"]);
    run_git(repo, &["config", "user.name", "Diagnose Test"]);
    run_git(repo, &["config", "user.email", "diagnose@example.invalid"]);
    fs::write(repo.join("README.md"), "# fixture\n").unwrap();
    run_git(repo, &["add", "README.md"]);
    run_git(repo, &["commit", "-q", "-m", "initial"]);

    let init = run_decapod(
        repo,
        &[],
        &["init", "--force", "--no-ci", "--no-container-workspaces"],
    );
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    run_git(repo, &["add", "-A"]);
    run_git(repo, &["commit", "-q", "-m", "init decapod"]);

    let session_out = run_decapod(
        repo,
        &[("DECAPOD_AGENT_ID", "agent-1798")],
        &["session", "acquire"],
    );
    assert!(
        session_out.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session_out.stderr)
    );
    let session_text = String::from_utf8_lossy(&session_out.stdout);
    let password = session_text
        .lines()
        .find_map(|line| line.strip_prefix("Password: "))
        .expect("session password")
        .trim()
        .to_string();

    let add = run_decapod(
        repo,
        &[
            ("DECAPOD_AGENT_ID", "agent-1798"),
            ("DECAPOD_SESSION_PASSWORD", &password),
        ],
        &[
            "todo",
            "add",
            "Test #1798 cleanup guidance",
            "--format",
            "json",
        ],
    );
    assert!(
        add.status.success(),
        "todo add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let added: Value = serde_json::from_slice(&add.stdout).unwrap();
    let task_id = added["id"].as_str().unwrap().to_string();

    let claim = run_decapod(
        repo,
        &[
            ("DECAPOD_AGENT_ID", "agent-1798"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CLAIM_AUTORUN", "0"),
        ],
        &["todo", "claim", "--id", &task_id, "--format", "json"],
    );
    assert!(
        claim.status.success(),
        "todo claim failed: {}",
        String::from_utf8_lossy(&claim.stderr)
    );

    let branch = format!("agent/agent-1798/{task_id}-tmpclean");
    let ensure = run_decapod(
        repo,
        &[
            ("DECAPOD_AGENT_ID", "agent-1798"),
            ("DECAPOD_SESSION_PASSWORD", &password),
        ],
        &["workspace", "ensure", "--branch", &branch],
    );
    assert!(
        ensure.status.success(),
        "workspace ensure failed: {}",
        String::from_utf8_lossy(&ensure.stderr)
    );
    let ensured: Value = serde_json::from_slice(&ensure.stdout).unwrap();
    let wt = ensured["worktree_path"].as_str().unwrap();
    let wt_path = Path::new(wt);

    // Before validation passes, diagnose must NOT tell the agent to publish or delete /tmp.
    let pre_diag = run_decapod(
        wt_path,
        &[("DECAPOD_AGENT_ID", "agent-1798")],
        &["qa", "diagnose", "--todo", &task_id, "--json"],
    );
    assert!(
        pre_diag.status.success(),
        "pre diagnose failed: {}",
        String::from_utf8_lossy(&pre_diag.stderr)
    );
    let pre_json: Value = serde_json::from_slice(&pre_diag.stdout).unwrap();
    assert!(pre_json["tasks"][0]["cleanup_guidance"].is_null());
    assert_ne!(
        pre_json["tasks"][0]["next_command"],
        "decapod workspace publish"
    );

    // Failed `decapod validate --format json` must also omit `tmp_cleanup_guidance`
    // and keep PR-creation cleanup guidance absent.
    let failed_validate = run_decapod(
        wt_path,
        &[
            ("DECAPOD_AGENT_ID", "agent-1798"),
            ("DECAPOD_SESSION_PASSWORD", &password),
        ],
        &["validate", "--format", "json"],
    );
    assert!(!failed_validate.status.success());
    if let Ok(val_json) = serde_json::from_slice::<Value>(&failed_validate.stdout) {
        assert!(val_json["tmp_cleanup_guidance"].is_null());
        if let Some(instr) = val_json["agent_guidance"]["instruction"].as_str() {
            assert!(!instr.contains("Once PR creation completes"));
        }
    }
}

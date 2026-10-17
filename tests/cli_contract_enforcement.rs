use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tempfile::TempDir;

static SHARED_REPO: OnceLock<(TempDir, PathBuf)> = OnceLock::new();

fn resolve_decapod_bin() -> std::path::PathBuf {
    let cargo_bin = env!("CARGO_BIN_EXE_decapod");
    if let Ok(p) = std::path::Path::new(cargo_bin).canonicalize() {
        return p;
    }
    if let Ok(runfiles_dir) = std::env::var("RUNFILES_DIR") {
        let p = std::path::Path::new(&runfiles_dir)
            .join("_main")
            .join("decapod");
        if p.exists() {
            return p;
        }
    }
    if let Some(parent) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
    {
        let p = parent.join("decapod");
        if p.exists() {
            return p;
        }
    }
    std::path::PathBuf::from(cargo_bin)
}

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    let exe = resolve_decapod_bin();
    Command::new(exe)
        .current_dir(dir)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .args(args)
        .output()
        .expect("run decapod")
}

#[test]
fn public_task_transfer_preserves_identity_without_importing_a_live_lease() {
    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn cli(dir: &Path, actor: &str, args: &[&str]) -> serde_json::Value {
        let out = Command::new(resolve_decapod_bin())
            .current_dir(dir)
            .env("DECAPOD_AGENT_ID", actor)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    }
    fn init(dir: &Path) {
        let out = Command::new(resolve_decapod_bin())
            .current_dir(dir)
            .env("DECAPOD_CLAIM_AUTORUN", "0")
            .args([
                "init",
                "--force",
                "--no-ci",
                "--no-specs",
                "--no-container-workspaces",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let source = TempDir::new().unwrap();
    git(source.path(), &["init", "-q", "-b", "transfer-fixture"]);
    git(
        source.path(),
        &[
            "-c",
            "user.name=Transfer Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    git(
        source.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:fixture/transfer.git",
        ],
    );
    init(source.path());
    let task = cli(
        source.path(),
        "source-agent",
        &[
            "todo",
            "add",
            "Preserve transfer identity",
            "--format",
            "json",
        ],
    );
    let id = task["id"].as_str().unwrap();
    cli(
        source.path(),
        "source-agent",
        &["todo", "claim", "--id", id, "--format", "json"],
    );
    cli(
        source.path(),
        "source-agent",
        &[
            "todo",
            "yield",
            "--id",
            id,
            "--reason",
            "explicit transfer",
            "--format",
            "json",
        ],
    );
    let artifacts = TempDir::new().unwrap();
    let package = artifacts.path().join("task.json");
    let exported = cli(
        source.path(),
        "source-agent",
        &[
            "todo",
            "transfer",
            "export",
            "--id",
            id,
            "--to",
            "receiver-agent",
            "--output",
            package.to_str().unwrap(),
        ],
    );
    let dest = TempDir::new().unwrap();
    let clone = Command::new("git")
        .args(["clone", "-q", "--local"])
        .arg(source.path())
        .arg(dest.path())
        .output()
        .unwrap();
    assert!(
        clone.status.success(),
        "{}",
        String::from_utf8_lossy(&clone.stderr)
    );
    git(
        dest.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/fixture/transfer.git",
        ],
    );
    init(dest.path());
    let input = [
        "todo",
        "transfer",
        "import",
        "--input",
        package.to_str().unwrap(),
        "--expected-digest",
        exported["artifact_sha256"].as_str().unwrap(),
    ];
    let rejected = Command::new(resolve_decapod_bin())
        .current_dir(dest.path())
        .env("DECAPOD_AGENT_ID", "receiver-agent")
        .args(input)
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    let mut accepted = input.to_vec();
    accepted.push("--accept-source-transfer");
    let imported = cli(dest.path(), "receiver-agent", &accepted);
    assert_eq!(imported["task_id"], id);
    assert_eq!(imported["execution_ready"], false);
    let replay = Command::new(resolve_decapod_bin())
        .current_dir(dest.path())
        .env("DECAPOD_AGENT_ID", "receiver-agent")
        .args(&accepted)
        .output()
        .unwrap();
    assert!(!replay.status.success());
    let sealed = Command::new(resolve_decapod_bin())
        .current_dir(source.path())
        .env("DECAPOD_AGENT_ID", "source-agent")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .args(["todo", "claim", "--id", id])
        .output()
        .unwrap();
    assert!(!sealed.status.success());
    assert!(String::from_utf8_lossy(&sealed.stderr).contains("source task is sealed"));
    cli(
        dest.path(),
        "receiver-agent",
        &["todo", "claim", "--id", id, "--format", "json"],
    );
    let status = cli(
        dest.path(),
        "receiver-agent",
        &["todo", "claim-status", "--id", id, "--format", "json"],
    );
    assert!(status.to_string().contains("receiver-agent"));
}

fn setup_repo() -> &'static (TempDir, PathBuf) {
    SHARED_REPO.get_or_init(|| {
        let tmp = TempDir::new().expect("tmpdir");
        let dir = tmp.path().to_path_buf();

        Command::new("git")
            .current_dir(&dir)
            .args(["init", "-b", "master"])
            .output()
            .expect("git init");

        let init = run_decapod(&dir, &["init", "--force"]);
        assert!(
            init.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&init.stderr)
        );

        (tmp, dir)
    })
}

#[test]
fn test_capabilities_schema_stability() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(dir, &["capabilities", "--format", "json"]);
    assert!(out.status.success(), "capabilities failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    let required_fields = ["version", "capabilities", "subsystems", "interlock_codes"];
    for field in required_fields {
        assert!(
            json.get(field).is_some(),
            "capabilities must have '{field}' field"
        );
    }

    let caps = json["capabilities"].as_array().expect("capabilities array");
    assert!(!caps.is_empty(), "capabilities must not be empty");
}

#[test]
fn bootstrap_commands_remain_available_when_project_state_needs_repair() {
    let (_tmp, dir) = setup_repo();

    let version = run_decapod(dir, &["system", "version"]);
    assert!(
        version.status.success(),
        "system version failed: {}",
        String::from_utf8_lossy(&version.stderr)
    );

    for args in [
        vec!["capabilities", "--format", "json"],
        vec!["system", "capabilities", "--format", "json"],
        vec!["constitution", "get", "core/DECAPOD"],
        vec!["docs", "list"],
        vec!["docs", "ingest"],
    ] {
        let output = run_decapod(dir, &args);
        assert!(
            output.status.success(),
            "bootstrap command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_schema_determinism_command() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(
        dir,
        &["data", "schema", "--format", "json", "--deterministic"],
    );
    assert!(out.status.success(), "schema failed");

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    assert!(
        json.get("command_registry").is_some(),
        "schema must have command_registry"
    );

    let commands = json["command_registry"].as_array().expect("commands array");
    assert!(!commands.is_empty(), "commands must not be empty");

    let has_essential = commands.iter().any(|c| {
        c.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.contains("validate"))
            .unwrap_or(false)
    });
    assert!(has_essential, "must have validate command");
}

#[test]
fn test_interlock_codes_present() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let codes = json["interlock_codes"]
        .as_array()
        .expect("interlock_codes array");

    let required_codes = [
        "workspace_required",
        "verification_required",
        "store_boundary_violation",
    ];

    for code in &required_codes {
        assert!(
            codes.iter().any(|c| c == code),
            "required interlock code '{code}' must be present"
        );
    }
}

#[test]
fn test_validate_command_works() {
    let (_tmp, dir) = setup_repo();

    let out = run_decapod(dir, &["validate"]);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        out.status.success() || combined.contains("gate") || combined.contains("validate"),
        "validate should execute and report gates. Got: {combined}"
    );
}

#[test]
fn test_workspace_command_structure() {
    let (_tmp, dir) = setup_repo();

    let status = run_decapod(dir, &["workspace", "status"]);
    assert!(status.status.success(), "workspace status should succeed");

    let output = String::from_utf8_lossy(&status.stdout);
    assert!(
        output.contains("branch") || output.contains("workspace") || output.contains("git"),
        "workspace status should report branch info"
    );
    assert!(
        output.contains("\"validation\": null"),
        "workspace status must remain observational instead of running full validation"
    );
}

#[test]
fn test_selective_test_command_structure() {
    let (_tmp, dir) = setup_repo();

    let help = run_decapod(dir, &["qa", "selective-test", "--help"]);
    assert!(
        help.status.success(),
        "qa selective-test help should succeed: {}",
        String::from_utf8_lossy(&help.stderr)
    );

    let output = String::from_utf8_lossy(&help.stdout);
    assert!(
        output.contains("Run only the integration tests affected by changed files")
            && output.contains("--reflex"),
        "qa selective-test should expose its selective and reflex modes: {output}"
    );
}

#[test]
fn test_todo_command_structure() {
    let (_tmp, dir) = setup_repo();

    run_decapod(dir, &["session", "acquire"]);

    let add = run_decapod(
        dir,
        &["todo", "add", "test contract task", "--format", "json"],
    );
    assert!(
        add.status.success(),
        "todo add should succeed: {}",
        String::from_utf8_lossy(&add.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&add.stdout)).expect("valid JSON");

    assert!(json.get("id").is_some(), "todo add should return id");
}

#[test]
fn test_agent_docs_accessible() {
    let (_tmp, dir) = setup_repo();

    let docs = run_decapod(dir, &["docs", "show", "docs/agent/api-index.md"]);
    assert!(
        docs.status.success(),
        "docs show should succeed: {}",
        String::from_utf8_lossy(&docs.stderr)
    );

    let output = String::from_utf8_lossy(&docs.stdout);
    assert!(
        output.contains("Agent API"),
        "docs show should return Agent API content"
    );
}

#[test]
fn test_agent_docs_short_names_are_accessible_after_fresh_init() {
    let (_tmp, dir) = setup_repo();

    for short_name in ["api-index", "command-contracts", "payload-examples"] {
        let docs = run_decapod(dir, &["docs", "show", short_name]);
        assert!(
            docs.status.success(),
            "docs show {short_name} should resolve the embedded agent document: {}",
            String::from_utf8_lossy(&docs.stderr)
        );
        assert!(
            String::from_utf8_lossy(&docs.stdout).len() > 50,
            "docs show {short_name} should return the embedded document"
        );
    }
}

#[test]
fn test_session_required_for_mutation() {
    let (_tmp, dir) = setup_repo();

    let add_without_session = run_decapod(dir, &["todo", "add", "should fail"]);
    let output = String::from_utf8_lossy(&add_without_session.stderr);

    assert!(
        !add_without_session.status.success() || output.contains("session"),
        "mutation without session should fail or mention session"
    );
}

#[test]
fn test_capabilities_includes_core_commands() {
    let (_tmp, dir) = setup_repo();

    let caps = run_decapod(dir, &["capabilities", "--format", "json"]);
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&caps.stdout)).expect("valid JSON");

    let caps_list = json["capabilities"].as_array().expect("capabilities array");

    let required_commands = [
        "validate",
        "workspace",
        "todo",
        "session",
        "constitution.get",
    ];

    for cmd in &required_commands {
        let has_cmd = caps_list.iter().any(|c| {
            c.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.contains(cmd))
                .unwrap_or(false)
        });
        assert!(has_cmd, "capabilities must include '{cmd}' command");
    }
}

#[test]
fn test_interlock_drift_detection_capability() {
    let (_tmp, dir) = setup_repo();

    let preflight = run_decapod(
        dir,
        &[
            "context",
            "preflight",
            "--op",
            "validate",
            "--format",
            "json",
        ],
    );
    assert!(
        preflight.status.success(),
        "preflight should work: {}",
        String::from_utf8_lossy(&preflight.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&preflight.stdout)).expect("valid JSON");

    assert!(
        json.get("risk_flags").is_some(),
        "preflight must report risk_flags"
    );

    assert!(
        json.get("likely_failures").is_some(),
        "preflight must predict failures"
    );
}

#[test]
fn test_agent_docs_are_accessible() {
    let (_tmp, dir) = setup_repo();

    let list_output = run_decapod(dir, &["docs", "list"]);
    assert!(list_output.status.success(), "docs list should succeed");

    let docs_list = String::from_utf8_lossy(&list_output.stdout);

    let required_docs = [
        "docs/agent/api-index.md",
        "docs/agent/command-contracts.md",
        "docs/agent/payload-examples.md",
    ];

    for required in &required_docs {
        assert!(
            docs_list.contains(required),
            "docs list should include {required}"
        );

        let show_output = run_decapod(dir, &["docs", "show", required]);
        assert!(
            show_output.status.success(),
            "docs show {required} should succeed"
        );

        let content = String::from_utf8_lossy(&show_output.stdout);
        assert!(
            !content.is_empty() && content.len() > 50,
            "docs show {required} should return content"
        );
    }
}

#[test]
fn test_constitution_nodes_are_accessible() {
    let (_tmp, dir) = setup_repo();

    let list_output = run_decapod(dir, &["constitution", "list"]);
    assert!(
        list_output.status.success(),
        "constitution list should succeed"
    );

    let nodes_list = String::from_utf8_lossy(&list_output.stdout);

    let required_nodes = [
        "core/DECAPOD",
        "interfaces/CLAIMS",
        "specs/INTENT",
        "methodology/ARCHITECTURE",
        "architecture/SECURITY",
        "plugins/TODO",
    ];

    for required in &required_nodes {
        assert!(
            nodes_list.contains(required),
            "constitution list should include {required}"
        );

        let get_output = run_decapod(dir, &["constitution", "get", required]);
        assert!(
            get_output.status.success(),
            "constitution get {required} should succeed"
        );

        let content = String::from_utf8_lossy(&get_output.stdout);
        assert!(
            !content.is_empty() && content.len() > 50,
            "constitution get {required} should return content"
        );
    }
}

#[test]
fn test_agent_docs_ingest_works() {
    let (_tmp, dir) = setup_repo();

    // Run docs ingest which dumps all docs
    let ingest = run_decapod(dir, &["docs", "ingest"]);
    assert!(ingest.status.success(), "docs ingest should succeed");

    let output = String::from_utf8_lossy(&ingest.stdout);

    // Verify each embedded doc appears in ingest output
    let required_docs = [
        "docs/agent/api-index.md",
        "docs/agent/command-contracts.md",
        "docs/agent/payload-examples.md",
        "docs/agent/error-recovery.md",
        "docs/agent/state-model.md",
    ];

    for doc_path in &required_docs {
        assert!(
            output.contains(doc_path),
            "docs ingest should include {doc_path}"
        );
    }
}

// Keep plan revision regressions in the branch-enforced CLI contract target.
mod plan_governed_execution {
    use super::{Command, PathBuf, TempDir, run_decapod};

    fn setup_repo() -> (TempDir, PathBuf, String) {
        let tmp = TempDir::new().expect("tmpdir");
        let repo_dir = tmp.path().to_path_buf();

        let init = Command::new("git")
            .current_dir(&repo_dir)
            .args(["init", "-b", "master"])
            .output()
            .expect("git init");
        assert!(init.status.success(), "git init failed");

        let out = run_decapod(&repo_dir, &["init", "--force"]);
        assert!(
            out.status.success(),
            "decapod init failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        Command::new("git")
            .current_dir(&repo_dir)
            .args(["config", "user.name", "Test User"])
            .output()
            .expect("git config user.name");
        Command::new("git")
            .current_dir(&repo_dir)
            .args(["config", "user.email", "test@example.com"])
            .output()
            .expect("git config user.email");

        let add = Command::new("git")
            .current_dir(&repo_dir)
            .args(["add", "."])
            .output()
            .expect("git add");
        assert!(add.status.success(), "git add failed");
        let commit = Command::new("git")
            .current_dir(&repo_dir)
            .args(["commit", "-m", "init"])
            .output()
            .expect("git commit");
        assert!(commit.status.success(), "git commit failed");

        let worktree_dir = tmp.path().join("worktree");
        let worktree = Command::new("git")
            .current_dir(&repo_dir)
            .args([
                "worktree",
                "add",
                "-b",
                "agent/test/plan-governed",
                worktree_dir
                    .to_str()
                    .expect("tempdir path should be valid unicode"),
                "HEAD",
            ])
            .output()
            .expect("git worktree add");
        assert!(worktree.status.success(), "git worktree add failed");

        let add_todo = run_decapod(
            &worktree_dir,
            &["todo", "add", "Wire plan-governed execution test fixture"],
        );
        assert!(
            add_todo.status.success(),
            "todo add failed: {}",
            String::from_utf8_lossy(&add_todo.stderr)
        );
        let todo_json: serde_json::Value =
            serde_json::from_slice(&add_todo.stdout).expect("todo add json");
        let todo_id = todo_json["id"].as_str().expect("todo id").to_string();

        (tmp, worktree_dir, todo_id)
    }

    #[test]
    fn plan_gate_returns_needs_human_input_until_questions_cleared() {
        let (_tmp, dir, todo_id) = setup_repo();

        let init_plan = run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "MVP slice",
                "--intent",
                "Enforce plan-governed execution",
                "--todo-id",
                &todo_id,
                "--question",
                "Which acceptance test should be mandatory?",
            ],
        );
        assert!(
            init_plan.status.success(),
            "plan init failed: {}",
            String::from_utf8_lossy(&init_plan.stderr)
        );

        let approve = run_decapod(&dir, &["govern", "plan", "approve"]);
        assert!(
            approve.status.success(),
            "plan approve failed: {}",
            String::from_utf8_lossy(&approve.stderr)
        );

        let blocked = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(
            !blocked.status.success(),
            "check-execute should fail while human questions remain"
        );
        let stderr = String::from_utf8_lossy(&blocked.stderr);
        assert!(
            stderr.contains("NEEDS_HUMAN_INPUT"),
            "expected NEEDS_HUMAN_INPUT marker; got: {stderr}"
        );

        let update = run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "update",
                "--clear-questions",
                "--clear-unknowns",
            ],
        );
        assert!(
            update.status.success(),
            "plan update failed: {}",
            String::from_utf8_lossy(&update.stderr)
        );

        let stale = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(!stale.status.success());
        assert!(String::from_utf8_lossy(&stale.stderr).contains("NEEDS_PLAN_APPROVAL"));
        assert!(
            run_decapod(&dir, &["govern", "plan", "approve"])
                .status
                .success()
        );

        let ok = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(
            ok.status.success(),
            "check-execute should pass after the revised plan is approved: {}",
            String::from_utf8_lossy(&ok.stderr)
        );
    }

    #[test]
    fn missing_todo_reports_coordination_recovery_without_reasking_intent() {
        let (_tmp, dir, todo_id) = setup_repo();
        let init = run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "init",
                "--title",
                "Preserve request context",
                "--intent",
                "Implement the already understood request",
                "--todo-id",
                &todo_id,
                "--proof-hook",
                "focused tests pass",
            ],
        );
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        assert!(
            run_decapod(&dir, &["govern", "plan", "approve"])
                .status
                .success()
        );
        let before = run_decapod(&dir, &["govern", "plan", "status"]);
        let missing = run_decapod(
            &dir,
            &[
                "govern",
                "plan",
                "check-execute",
                "--todo-id",
                "absent-task",
            ],
        );
        assert_eq!(missing.status.code(), Some(1));
        let stderr = String::from_utf8_lossy(&missing.stderr);
        assert!(stderr.contains("TODO_PROJECTION_MISSING"), "{stderr}");
        assert!(!stderr.contains("NEEDS_HUMAN_INPUT"), "{stderr}");
        let payload: serde_json::Value =
            serde_json::from_str(stderr.split_once("payload=").unwrap().1.trim()).unwrap();
        assert_eq!(payload["kind"], "coordination_projection_missing");
        assert_eq!(payload["todo_ids"], serde_json::json!(["absent-task"]));
        assert_eq!(payload["execution_ready"], false);
        assert_eq!(
            before.stdout,
            run_decapod(&dir, &["govern", "plan", "status"]).stdout
        );

        // Select the existing coordination record; no new user answer or plan
        // approval is needed, and the ordinary execution check still runs.
        let recovered = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(
            recovered.status.success(),
            "{}",
            String::from_utf8_lossy(&recovered.stderr)
        );
        assert!(String::from_utf8_lossy(&recovered.stdout).contains("EXECUTION_READY"));
    }

    #[test]
    fn public_plan_edits_require_reapproval_and_phase_completion_cannot_supply_it() {
        let (_tmp, dir, todo_id) = setup_repo();
        let run = |args: &[&str]| {
            let out = run_decapod(&dir, args);
            assert!(
                out.status.success(),
                "{args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
        };
        run(&[
            "govern",
            "plan",
            "init",
            "--title",
            "Accepted work",
            "--intent",
            "Original contract",
            "--todo-id",
            &todo_id,
        ]);
        run(&[
            "govern",
            "plan",
            "phase-add",
            "--id",
            "build",
            "--name",
            "Build",
            "--description",
            "Perform accepted work",
        ]);
        for command in ["enter-phase", "exit-phase"] {
            let blocked = run_decapod(&dir, &["govern", "plan", command, "--phase-id", "build"]);
            assert!(!blocked.status.success());
            assert!(String::from_utf8_lossy(&blocked.stderr).contains("NEEDS_PLAN_APPROVAL"));
        }
        run(&["govern", "plan", "approve"]);
        run(&["govern", "plan", "enter-phase", "--phase-id", "build"]);
        assert_eq!(
            run(&["govern", "plan", "exit-phase", "--phase-id", "build"])["state"],
            "DONE"
        );
        // A byte-identical update preserves completion, but a changed contract cannot.
        assert_eq!(
            run(&["govern", "plan", "update", "--intent", "Original contract"])["state"],
            "DONE"
        );
        let changed = run(&["govern", "plan", "update", "--intent", "Different contract"]);
        assert_eq!(changed["state"], "DRAFT");
        assert_eq!(changed["phases"][0]["completed"], false);
        let blocked = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(!blocked.status.success());
        assert!(String::from_utf8_lossy(&blocked.stderr).contains("NEEDS_PLAN_APPROVAL"));
        run(&["govern", "plan", "approve"]);
        let blocked = run_decapod(
            &dir,
            &["govern", "plan", "check-execute", "--todo-id", &todo_id],
        );
        assert!(!blocked.status.success());
        assert!(String::from_utf8_lossy(&blocked.stderr).contains("PHASE_REQUIRED"));
        run(&["govern", "plan", "enter-phase", "--phase-id", "build"]);
        run(&["govern", "plan", "exit-phase", "--phase-id", "build"]);
        assert!(
            run_decapod(
                &dir,
                &["govern", "plan", "check-execute", "--todo-id", &todo_id]
            )
            .status
            .success()
        );
    }
}

mod workunit_recovery {
    use std::fs;
    use std::path::Path;
    use std::process::{Command, Output};

    fn run(root: &Path, args: &[&str]) -> Output {
        run_authenticated(root, args, "")
    }

    fn run_authenticated(root: &Path, args: &[&str], password: &str) -> Output {
        Command::new(super::resolve_decapod_bin())
            .current_dir(root)
            .env("DECAPOD_AGENT_ID", "workunit-recovery-test")
            .env("DECAPOD_SESSION_PASSWORD", password)
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .args(args)
            .output()
            .expect("run CLI")
    }

    fn success(output: Output) -> serde_json::Value {
        assert!(
            output.status.success(),
            "CLI failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output")
    }

    #[test]
    fn workunit_projection_recovery_is_idempotent_through_real_cli() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let init = Command::new("git")
            .current_dir(root)
            .args(["init", "-b", "master"])
            .output()
            .unwrap();
        assert!(init.status.success());
        assert!(run(root, &["init", "--force"]).status.success());
        let acquired = run(root, &["session", "acquire"]);
        assert!(acquired.status.success());
        let password = String::from_utf8_lossy(&acquired.stdout)
            .lines()
            .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
            .expect("fixture session password");
        let run = |root: &Path, args: &[&str]| run_authenticated(root, args, &password);
        let task = "test_recovery_cli";
        success(run(
            root,
            &[
                "govern",
                "workunit",
                "init",
                "--task-id",
                task,
                "--intent-ref",
                "intent:recovery-cli",
            ],
        ));
        success(run(
            root,
            &[
                "govern",
                "workunit",
                "attach-state",
                "--task-id",
                task,
                "--ref",
                "evidence:observed",
            ],
        ));
        let original = success(run(root, &["govern", "workunit", "get", "--task-id", task]));
        let journal_before = success(run(
            root,
            &["govern", "workunit", "journal-status", "--task-id", task],
        ));
        let projection = root
            .join(".decapod/data/workunits")
            .join(format!("{task}.json"));
        fs::remove_file(&projection).unwrap();
        assert!(
            !run(root, &["govern", "workunit", "get", "--task-id", task])
                .status
                .success()
        );
        success(run(
            root,
            &["govern", "workunit", "recover", "--task-id", task],
        ));
        let restored = fs::read(&projection).unwrap();
        assert_eq!(
            success(run(root, &["govern", "workunit", "get", "--task-id", task])),
            original
        );
        success(run(
            root,
            &["govern", "workunit", "recover", "--task-id", task],
        ));
        assert_eq!(fs::read(&projection).unwrap(), restored);
        assert_eq!(
            success(run(
                root,
                &["govern", "workunit", "journal-status", "--task-id", task]
            )),
            journal_before
        );
        // Projection recovery cannot upgrade this draft to verified work.
        assert_eq!(original["status"], "DRAFT");
    }

    #[test]
    fn checkpoint_and_resume_contracts_require_explicit_request_identity() {
        let temp = tempfile::tempdir().unwrap();
        for args in [
            vec!["govern", "workunit", "checkpoint", "--task-id", "test_a"],
            vec!["govern", "workunit", "resume", "--task-id", "test_a"],
            vec!["govern", "trajectory", "checkpoint", "--run-id", "run_a"],
            vec!["govern", "trajectory", "verify-resume", "--run-id", "run_a"],
        ] {
            let output = run(temp.path(), &args);
            assert_eq!(output.status.code(), Some(2));
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains("--operation-id") || error.contains("--checkpoint-id"),
                "{error}"
            );
        }
        assert!(!temp.path().join(".decapod").exists());
    }
}

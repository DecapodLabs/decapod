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

// The always-selected cli-contracts proof runs this target, including test-only PRs.
mod documented_bootstrap {
    //! Execute the generated agent instructions, not a bypassed orientation fixture.
    use std::path::Path;
    use std::process::{Command, Output};

    fn contract_fixture() -> tempfile::TempDir {
        let mut builder = tempfile::Builder::new();
        builder.prefix("decapod-bootstrap-contract-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        builder.tempdir().expect("private bootstrap fixture")
    }

    fn isolated_command(program: &Path, cwd: &Path, home: &Path) -> Command {
        let mut command = Command::new(program);
        command.current_dir(cwd);
        // CI and a developer's active session must not change this fixture's gates,
        // datastore, Git checkout, or identity. In particular, presence of
        // DECAPOD_VALIDATE_SKIP_GIT_GATES would invalidate the negative controls.
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("DECAPOD_") || name.starts_with("GIT_") {
                command.env_remove(key);
            }
        }
        command
            .env_remove("GITHUB_ACTIONS")
            .env_remove("BUILDKITE")
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join("config"))
            .env("XDG_DATA_HOME", home.join("data"))
            .env("DECAPOD_AGENT_ID", "bootstrap-test")
            // Claiming still acquires a real lease. Only its optional container
            // launch is disabled: this fixture exercises local Git worktrees.
            .env("DECAPOD_CLAIM_AUTORUN", "false");
        command
    }

    fn success(output: Output, step: &str) -> Output {
        assert!(
            output.status.success(),
            "{step} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    #[test]
    fn independent_cli_processes_claim_and_admit_separate_worktrees() {
        use std::process::Stdio;
        let fixture = contract_fixture();
        let root = fixture.path().join("repo");
        let home = fixture.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let root = root.canonicalize().unwrap();
        let home = home.canonicalize().unwrap();
        let binary = super::resolve_decapod_bin();
        let cli = |agent: &str, cwd: &Path, args: &[&str]| {
            isolated_command(&binary, cwd, &home)
                .env("DECAPOD_AGENT_ID", agent)
                .args(args)
                .output()
                .expect("execute decapod CLI")
        };
        let git = |args: &[&str]| {
            success(
                isolated_command(Path::new("git"), &root, &home)
                    .args(args)
                    .output()
                    .unwrap(),
                "fixture git",
            )
        };
        success(
            cli(
                "agent-a",
                &root,
                &["init", "--no-git", "--no-ci", "--no-container-workspaces"],
            ),
            "fixture init",
        );
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.name", "Concurrent CLI Test"]);
        git(&["config", "user.email", "concurrent@example.invalid"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "Initialize governed fixture"]);
        let add = |title: &str| -> String {
            let output = success(
                cli(
                    "agent-a",
                    &root,
                    &[
                        "todo",
                        "add",
                        title,
                        "--scope",
                        "workspace",
                        "--format",
                        "json",
                    ],
                ),
                "todo add",
            );
            let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            json["id"].as_str().unwrap().to_owned()
        };
        let a = add("Independent concurrent parser task");
        let b = add("Independent concurrent renderer task");
        assert_ne!(a, b);
        let spawn = |agent: &str, args: &[&str]| {
            isolated_command(&binary, &root, &home)
                .env("DECAPOD_AGENT_ID", agent)
                .args(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        };
        let claim_a = spawn(
            "agent-a",
            &["todo", "claim", "--id", &a, "--format", "json"],
        );
        let claim_b = spawn(
            "agent-b",
            &["todo", "claim", "--id", &b, "--format", "json"],
        );
        for output in [
            claim_a.wait_with_output().unwrap(),
            claim_b.wait_with_output().unwrap(),
        ] {
            let output = success(output, "concurrent claim");
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["status"], "ok");
        }
        let branch_a = format!("agent/agent-a/{a}");
        let branch_b = format!("agent/agent-b/{b}");
        let ensure_a = spawn("agent-a", &["workspace", "ensure", "--branch", &branch_a]);
        let ensure_b = spawn("agent-b", &["workspace", "ensure", "--branch", &branch_b]);
        let a_out = success(ensure_a.wait_with_output().unwrap(), "agent-a workspace");
        let b_out = success(ensure_b.wait_with_output().unwrap(), "agent-b workspace");
        let a_json: serde_json::Value = serde_json::from_slice(&a_out.stdout).unwrap();
        let b_json: serde_json::Value = serde_json::from_slice(&b_out.stdout).unwrap();
        assert_eq!(a_json["can_work"], true);
        assert_eq!(b_json["can_work"], true);
        let path_a = Path::new(a_json["worktree_path"].as_str().unwrap());
        let path_b = Path::new(b_json["worktree_path"].as_str().unwrap());
        assert_ne!(path_a, path_b);
        let denied = success(
            cli("agent-b", path_a, &["workspace", "status"]),
            "cross-agent status",
        );
        let denied: serde_json::Value = serde_json::from_slice(&denied.stdout).unwrap();
        assert_eq!(denied["workspace"]["can_work"], false);
        fixture.close().unwrap();
    }

    #[test]
    fn generated_agent_sequence_orients_only_after_entering_claimed_workspace() {
        let fixture = contract_fixture();
        let root = fixture.path().join("repo");
        let home = fixture.path().join("home");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        // macOS temporary paths may enter through /var while Git returns /private/var.
        let root = root.canonicalize().unwrap();
        let home = home.canonicalize().unwrap();
        let binary = super::resolve_decapod_bin();
        let cli = |cwd: &Path, args: &[&str]| {
            isolated_command(&binary, cwd, &home)
                .args(args)
                .output()
                .expect("execute real decapod CLI")
        };
        let git = |cwd: &Path, args: &[&str]| {
            success(
                isolated_command(Path::new("git"), cwd, &home)
                    .args(args)
                    .output()
                    .expect("execute git"),
                &format!("git {args:?}"),
            )
        };

        // Human provisioning is deliberately outside the agent sequence. This is a
        // local-backend Git-worktree test, not Docker or an installer/network test.
        success(
            cli(
                &root,
                &["init", "--no-git", "--no-ci", "--no-container-workspaces"],
            ),
            "human init",
        );
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Bootstrap Test"]);
        git(
            &root,
            &["config", "user.email", "bootstrap@example.invalid"],
        );
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "Initialize governed fixture"]);
        let initial_head = git(&root, &["rev-parse", "HEAD"]).stdout;

        let instructions = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
        let initialization = instructions
            .split("## Mandatory Initialization")
            .nth(1)
            .expect("generated initialization section")
            .split("```bash\n")
            .nth(1)
            .unwrap()
            .split("```")
            .next()
            .unwrap();
        // Only the published agent commands are interpreted, without invoking a
        // shell. Package installation/update is a separate human provisioning concern.
        let commands: Vec<_> = initialization
            .lines()
            .filter(|line| line.starts_with("decapod ") || line.starts_with("cd "))
            .flat_map(|line| line.split(" && "))
            .collect();
        assert_eq!(commands.first(), Some(&"decapod docs ingest"));
        assert_eq!(commands.last(), Some(&"decapod rpc --op context.resolve"));
        let mut cwd = root.clone();
        let mut task_id = String::new();
        let mut workspace = None;
        let mut oriented = false;
        let task = "document-bootstrap-order";
        let deny_root_orientation = |task_id: &str| {
            let output = cli(&root, &["infer", "orientation", "--task-id", task_id]);
            assert!(
                !output.status.success(),
                "main checkout accepted orientation"
            );
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("requires isolated git worktree")
                    && stderr.contains("current checkout is not a worktree"),
                "wrong denial: {stderr}"
            );
        };

        for template in commands {
            if template == "cd .decapod/workspaces/<your-worktree>" {
                cwd = workspace.clone().expect("ensure must precede cd");
                assert!(cwd.join(".git").is_file(), "must enter a real Git worktree");
                continue;
            }
            let expanded = template
                .replace("\"<task>\"", task)
                .replace("<task-id>", &task_id)
                .replace("<agent-id>", "bootstrap-test")
                .replace("<topic>", "bootstrap-order");
            assert!(!expanded.contains('<'), "unhandled instruction: {expanded}");
            let args: Vec<_> = expanded.split_whitespace().skip(1).collect();
            if args.starts_with(&["workspace", "ensure"]) {
                assert!(!task_id.is_empty());
                deny_root_orientation(&task_id);
            }
            let output = success(cli(&cwd, &args), &expanded);
            match args.as_slice() {
                ["todo", "add", ..] => {
                    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    task_id = value["id"].as_str().expect("created task id").to_owned();
                }
                ["todo", "claim", ..] => {
                    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(value["id"], task_id);
                    assert_eq!(value["result"]["assigned_to"], "bootstrap-test");
                    assert_eq!(value["result"]["lease_state"], "active");
                }
                ["workspace", "ensure", ..] => {
                    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(value["can_work"], true);
                    assert_eq!(value["blockers"], serde_json::json!([]));
                    let path = std::path::PathBuf::from(value["worktree_path"].as_str().unwrap());
                    assert!(path.starts_with(root.join(".decapod/workspaces")));
                    workspace = Some(path);
                    // Creating a workspace alone is insufficient: the caller must cd.
                    deny_root_orientation(&task_id);
                }
                ["infer", "orientation", ..] => {
                    assert_ne!(cwd, root);
                    assert_eq!(Some(&cwd), workspace.as_ref());
                    assert!(!cwd.join("implementation.txt").exists());
                    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
                    assert_eq!(value["task_id"], task_id);
                    assert_eq!(value["user_goal"], task);
                    assert_eq!(value["governed_plan"]["task_binding"], "bound");
                    assert_eq!(value["decision_gates"], serde_json::json!([]));
                    oriented = true;
                }
                ["rpc", "--op", "context.resolve"] => assert!(oriented),
                _ => {}
            }
        }
        assert!(oriented, "generated instructions must execute orientation");
        // Implementation starts only after the complete documented initialization.
        std::fs::write(cwd.join("implementation.txt"), "workspace-only change\n").unwrap();
        assert!(!root.join("implementation.txt").exists());
        assert_eq!(git(&root, &["rev-parse", "HEAD"]).stdout, initial_head);
        assert!(git(&root, &["status", "--porcelain"]).stdout.is_empty());
        assert!(
            !home.join("data/containers/storage").exists(),
            "local bootstrap must not initialize container image storage"
        );
        fixture.close().expect("remove isolated bootstrap fixture");
    }
}

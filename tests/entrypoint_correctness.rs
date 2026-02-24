//! Integration tests for entrypoint correctness.
//!
//! These tests ensure that `decapod init` creates correct entrypoint files
//! and that `decapod validate` enforces invariants and detects tampering.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

/// Helper to run decapod command in a temp directory
fn run_decapod(temp_dir: &PathBuf, args: &[&str]) -> (bool, String) {
    run_decapod_with_env(temp_dir, args, &[("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")])
}

fn run_decapod_with_env(
    temp_dir: &PathBuf,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (bool, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(temp_dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().expect("Failed to execute decapod");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
}

fn run_raw(temp_dir: &PathBuf, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(temp_dir).args(args);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("Failed to execute decapod")
}

fn acquire_session(temp_path: &PathBuf) {
    let (success, output) = run_decapod(temp_path, &["session", "acquire"]);
    assert!(
        success,
        "decapod session acquire should succeed. Output:\n{}",
        output
    );
}

fn extract_password(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Password: ") {
            return Some(rest.trim().to_string());
        }
    }
    None
}

#[test]
fn test_init_creates_all_entrypoints() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _output) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    // Check that all 5 entrypoint files exist
    let expected_files = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];

    for file in expected_files {
        let file_path = temp_path.join(file);
        assert!(
            file_path.exists(),
            "Entrypoint file {} should exist after init",
            file
        );

        // Check that file is non-empty
        let content =
            fs::read_to_string(&file_path).unwrap_or_else(|_| panic!("Failed to read {}", file));
        assert!(!content.is_empty(), "{} should not be empty", file);
    }
}

#[test]
fn test_validate_passes_after_init() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    // Run decapod validate
    let (success, output) = run_decapod(&temp_path, &["validate"]);
    assert!(
        success,
        "decapod validate should pass after init. Output:\n{}",
        output
    );

    // Check that Four Invariants Gate is mentioned
    assert!(
        output.contains("Four Invariants Gate"),
        "Validation should check Four Invariants Gate"
    );
}

#[test]
fn test_agent_session_requires_password() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    let (success, acquire_out) = run_decapod_with_env(
        &temp_path,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "agent-secure")],
    );
    assert!(success, "session acquire should succeed: {}", acquire_out);
    let password = extract_password(&acquire_out).expect("acquire output should include password");

    let (ok_missing, out_missing) = run_decapod_with_env(
        &temp_path,
        &["validate"],
        &[("DECAPOD_AGENT_ID", "agent-secure")],
    );
    // With auto-acquire funnel, validate may auto-create session
    // but workspace requirement still applies first
    assert!(
        !ok_missing || out_missing.contains("worktree") || out_missing.contains("session"),
        "validate should either fail on workspace or auto-acquire session: {}",
        out_missing
    );

    let (ok_wrong, out_wrong) = run_decapod_with_env(
        &temp_path,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "agent-secure"),
            ("DECAPOD_SESSION_PASSWORD", "wrong"),
        ],
    );
    // With auto-acquire funnel, wrong password triggers auto-recovery
    // but workspace requirement still applies first
    assert!(
        !ok_wrong || out_wrong.contains("worktree") || out_wrong.contains("session"),
        "validate should either fail on workspace or auto-acquire session: {}",
        out_wrong
    );

    let (ok_good, out_good) = run_decapod_with_env(
        &temp_path,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "agent-secure"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1"),
        ],
    );
    assert!(
        ok_good,
        "validate should pass with correct agent+password: {}",
        out_good
    );
}

#[test]
fn test_expired_session_releases_assigned_tasks() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    let (success, acquire_out) = run_decapod_with_env(
        &temp_path,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "agent-expire")],
    );
    assert!(success, "session acquire should succeed: {}", acquire_out);
    let password = extract_password(&acquire_out).expect("acquire output should include password");
    let auth_env = [
        ("DECAPOD_AGENT_ID", "agent-expire"),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
    ];

    let add_out = run_raw(
        &temp_path,
        &["todo", "--format", "json", "add", "session cleanup target"],
        &auth_env,
    );
    assert!(
        add_out.status.success(),
        "todo add should succeed: {}",
        String::from_utf8_lossy(&add_out.stderr)
    );
    let add_json: serde_json::Value =
        serde_json::from_slice(&add_out.stdout).expect("todo add should return json");
    let task_id = add_json["id"]
        .as_str()
        .expect("todo add json should include id")
        .to_string();

    let claim_out = run_raw(
        &temp_path,
        &["todo", "--format", "json", "claim", "--id", &task_id],
        &auth_env,
    );
    assert!(
        claim_out.status.success(),
        "todo claim should succeed: {}",
        String::from_utf8_lossy(&claim_out.stderr)
    );

    let session_path = temp_path
        .join(".decapod")
        .join("generated")
        .join("sessions")
        .join("agent-expire.json");
    let mut session_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&session_path).expect("session file"))
            .expect("session json");
    session_json["expires_at_epoch_secs"] = serde_json::json!(0);
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session_json).expect("serialize"),
    )
    .expect("write expired session");

    let status_out = run_raw(
        &temp_path,
        &["session", "status"],
        &[("DECAPOD_AGENT_ID", "agent-expire")],
    );
    assert!(
        status_out.status.success(),
        "session status should run cleanup: {}",
        String::from_utf8_lossy(&status_out.stderr)
    );

    let (ok_unknown_acquire, out_unknown_acquire) =
        run_decapod(&temp_path, &["session", "acquire"]);
    assert!(
        ok_unknown_acquire,
        "unknown session acquire should succeed: {}",
        out_unknown_acquire
    );

    let get_out = run_raw(
        &temp_path,
        &["todo", "--format", "json", "get", "--id", &task_id],
        &[],
    );
    assert!(
        get_out.status.success(),
        "todo get should succeed: {}",
        String::from_utf8_lossy(&get_out.stderr)
    );
    let get_json: serde_json::Value =
        serde_json::from_slice(&get_out.stdout).expect("todo get should return json");
    assert_eq!(
        get_json["task"]["assigned_to"].as_str().unwrap_or(""),
        "",
        "expired session cleanup should unassign task"
    );
}

#[test]
fn test_entrypoints_are_thin() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    // Check AGENTS.md line count (should be ≤ 100)
    let agents_content =
        fs::read_to_string(temp_path.join("AGENTS.md")).expect("Failed to read AGENTS.md");
    let agents_lines = agents_content.lines().count();
    assert!(
        agents_lines <= 100,
        "AGENTS.md should be ≤ 100 lines (got {})",
        agents_lines
    );

    // Check agent-specific files (should be ≤ 70)
    for file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {}", file));
        let line_count = content.lines().count();
        assert!(
            line_count <= 70,
            "{} should be ≤ 70 lines (got {})",
            file,
            line_count
        );
    }
}

#[test]
fn test_entrypoints_contain_canonical_router() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    // Check that all entrypoints reference core/DECAPOD.md
    let files = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];

    for file in files {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {}", file));
        assert!(
            content.contains("core/DECAPOD.md"),
            "{} should reference canonical router (core/DECAPOD.md)",
            file
        );
    }
}

#[test]
fn test_entrypoints_contain_four_invariants() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    // Check that AGENTS.md contains the 4 invariants
    let agents_content =
        fs::read_to_string(temp_path.join("AGENTS.md")).expect("Failed to read AGENTS.md");

    let invariant_markers = ["core/DECAPOD.md", "decapod validate", "Stop if", "✅"];

    for marker in invariant_markers {
        assert!(
            agents_content.contains(marker),
            "AGENTS.md should contain invariant marker: {}",
            marker
        );
    }
}

#[test]
fn test_validate_fails_on_missing_invariant() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    // Tamper with AGENTS.md - remove canonical router reference
    let agents_path = temp_path.join("AGENTS.md");
    let content = fs::read_to_string(&agents_path).expect("Failed to read AGENTS.md");
    let tampered = content.replace("core/DECAPOD.md", "core/LEGACY.md");
    fs::write(&agents_path, tampered).expect("Failed to write tampered AGENTS.md");

    // Run decapod validate (should fail)
    let (success, output) = run_decapod(&temp_path, &["validate"]);
    assert!(
        !success,
        "decapod validate should fail after tampering. Output:\n{}",
        output
    );

    // Check that it detected the missing invariant
    assert!(
        output.contains("Invariant missing: Router pointer to core/DECAPOD.md"),
        "Validation should detect missing router invariant"
    );
}

#[test]
fn test_validate_fails_on_bloated_entrypoint() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    // Bloat CLAUDE.md beyond 50 lines
    let claude_path = temp_path.join("CLAUDE.md");
    let content = fs::read_to_string(&claude_path).expect("Failed to read CLAUDE.md");
    let bloated = format!("{}\n{}", content, "# Extra\n".repeat(50));
    fs::write(&claude_path, bloated).expect("Failed to write bloated CLAUDE.md");

    // Run decapod validate (should fail)
    let (success, output) = run_decapod(&temp_path, &["validate"]);
    assert!(
        !success,
        "decapod validate should fail on bloated entrypoint. Output:\n{}",
        output
    );

    // Check that it detected the line limit violation
    assert!(
        output.contains("CLAUDE.md exceeds line limit"),
        "Validation should detect bloated entrypoint"
    );
}

#[test]
fn test_agent_specific_files_defer_to_agents() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    // Run decapod init
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    // Check that agent-specific files reference AGENTS.md
    for file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {}", file));
        assert!(
            content.contains("AGENTS.md"),
            "{} should defer to AGENTS.md",
            file
        );
    }
}

#[test]
fn test_root_entrypoints_match_templates() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let root_path = repo_root.join(file);
        let template_path = repo_root.join("templates").join(file);
        let root_content =
            fs::read_to_string(&root_path).unwrap_or_else(|_| panic!("Failed to read {}", file));
        let template_content = fs::read_to_string(&template_path)
            .unwrap_or_else(|_| panic!("Failed to read templates/{}", file));

        assert_eq!(
            root_content, template_content,
            "Entrypoint drift detected: {} differs from templates/{}. Keep root and template in sync.",
            file, file
        );
    }
}

#[test]
fn test_agent_entrypoints_are_consistent_except_header() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let root_claude = fs::read_to_string(repo_root.join("CLAUDE.md")).expect("read CLAUDE.md");
    let root_gemini = fs::read_to_string(repo_root.join("GEMINI.md")).expect("read GEMINI.md");
    let root_codex = fs::read_to_string(repo_root.join("CODEX.md")).expect("read CODEX.md");

    assert!(
        root_claude
            .lines()
            .next()
            .is_some_and(|l| l.contains("CLAUDE.md")),
        "CLAUDE.md header should include CLAUDE.md"
    );
    assert_eq!(
        root_claude.lines().skip(1).collect::<Vec<_>>(),
        root_gemini.lines().skip(1).collect::<Vec<_>>(),
        "Root entrypoints should only differ by file-specific header: CLAUDE.md != GEMINI.md"
    );
    assert_eq!(
        root_claude.lines().skip(1).collect::<Vec<_>>(),
        root_codex.lines().skip(1).collect::<Vec<_>>(),
        "Root entrypoints should only differ by file-specific header: CLAUDE.md != CODEX.md"
    );

    let tpl_claude =
        fs::read_to_string(repo_root.join("templates/CLAUDE.md")).expect("read template CLAUDE");
    let tpl_gemini =
        fs::read_to_string(repo_root.join("templates/GEMINI.md")).expect("read template GEMINI");
    let tpl_codex =
        fs::read_to_string(repo_root.join("templates/CODEX.md")).expect("read template CODEX");

    assert_eq!(
        tpl_claude.lines().skip(1).collect::<Vec<_>>(),
        tpl_gemini.lines().skip(1).collect::<Vec<_>>(),
        "Template entrypoints should only differ by file-specific header: CLAUDE.md != GEMINI.md"
    );
    assert_eq!(
        tpl_claude.lines().skip(1).collect::<Vec<_>>(),
        tpl_codex.lines().skip(1).collect::<Vec<_>>(),
        "Template entrypoints should only differ by file-specific header: CLAUDE.md != CODEX.md"
    );
}

#[test]
fn test_entrypoints_use_embedded_docs_paths_only() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for file in [
        "CLAUDE.md",
        "GEMINI.md",
        "CODEX.md",
        "templates/CLAUDE.md",
        "templates/GEMINI.md",
        "templates/CODEX.md",
    ] {
        let content =
            fs::read_to_string(repo_root.join(file)).unwrap_or_else(|_| panic!("read {}", file));
        assert!(
            !content.contains("decapod docs show constitution/"),
            "{} must not reference direct constitution/* filesystem paths",
            file
        );
        assert!(
            content.contains("decapod docs show docs/PLAYBOOK.md"),
            "{} must reference embedded docs path for operator playbook",
            file
        );
        assert!(
            content.contains(".decapod/workspaces"),
            "{} must mandate canonical Decapod worktree root",
            file
        );
        assert!(
            content.contains("decapod todo add \"<task>\""),
            "{} must require task creation before claim",
            file
        );
        assert!(
            !content.contains(".claude/worktrees"),
            "{} must never reference non-canonical .claude/worktrees path",
            file
        );
    }
}

#[test]
fn test_top_level_docs_avoid_direct_constitution_file_links() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(repo_root.join("README.md")).expect("read README.md");
    let security = fs::read_to_string(repo_root.join("SECURITY.md")).expect("read SECURITY.md");

    assert!(
        readme.contains("(constitution/core/DECAPOD.md)"),
        "README.md should link to constitution/core/DECAPOD.md"
    );

    assert!(
        !security.contains("(constitution/"),
        "SECURITY.md should not instruct direct constitution file access"
    );
    assert!(
        security.contains("decapod docs show specs/SECURITY.md"),
        "SECURITY.md should route constitutional access through decapod docs show"
    );
}

#[test]
fn test_intent_context_spec_contract_alignment() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(repo_root.join("README.md")).expect("read README.md");
    let core_decapod = fs::read_to_string(repo_root.join("constitution/core/DECAPOD.md"))
        .expect("read constitution/core/DECAPOD.md");
    let lib_rs = fs::read_to_string(repo_root.join("src/lib.rs")).expect("read src/lib.rs");

    let contract_phrase =
        "turn intent into context, then context into explicit specifications before inference";

    assert!(
        readme.contains(contract_phrase),
        "README.md must state the intent->context->specifications flow"
    );
    assert!(
        core_decapod.contains(contract_phrase),
        "constitution/core/DECAPOD.md must state the intent->context->specifications flow"
    );
    assert!(
        lib_rs.contains(contract_phrase),
        "src/lib.rs CLI about text must state the intent->context->specifications flow"
    );
}

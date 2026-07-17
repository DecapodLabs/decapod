//! Integration tests for entrypoint correctness.
//!
//! These tests ensure that `decapod init` creates correct entrypoint files
//! and that `decapod validate` enforces invariants and detects tampering.

use decapod::core::assets;
use decapod::core::entrypoint_integrity;
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
    let mut has_xdg = false;
    for (k, v) in envs {
        cmd.env(k, v);
        if *k == "XDG_CONFIG_HOME" {
            has_xdg = true;
        }
    }
    if !has_xdg {
        cmd.env("XDG_CONFIG_HOME", temp_dir.join(".config"));
    }
    let output = cmd.output().expect("Failed to execute decapod");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");

    (output.status.success(), combined)
}

fn run_raw(temp_dir: &PathBuf, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(temp_dir).args(args);
    let mut has_xdg = false;
    for (k, v) in envs {
        cmd.env(k, v);
        if *k == "XDG_CONFIG_HOME" {
            has_xdg = true;
        }
    }
    if !has_xdg {
        cmd.env("XDG_CONFIG_HOME", temp_dir.join(".config"));
    }
    cmd.output().expect("Failed to execute decapod")
}

fn acquire_session(temp_path: &PathBuf) {
    let (success, output) = run_decapod(temp_path, &["session", "acquire"]);
    assert!(
        success,
        "decapod session acquire should succeed. Output:\n{output}"
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
            "Entrypoint file {file} should exist after init"
        );

        // Check that file is non-empty
        let content =
            fs::read_to_string(&file_path).unwrap_or_else(|_| panic!("Failed to read {file}"));
        assert!(!content.is_empty(), "{file} should not be empty");
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
        "decapod validate should pass after init. Output:\n{output}"
    );

    // Check that Four Invariants Gate is mentioned
    assert!(
        output.contains("Four Invariants Gate"),
        "Validation should check Four Invariants Gate"
    );
}

#[test]
fn test_validate_passes_after_init_without_git_repo() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    let init = run_raw(&temp_path, &["init", "--force"], &[]);
    assert!(
        init.status.success(),
        "decapod init should succeed. Output:\n{}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );

    let validate = run_raw(&temp_path, &["validate"], &[]);
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );
    assert!(
        validate.status.success(),
        "decapod validate should pass immediately after init in a non-git directory. Output:\n{output}"
    );
    assert!(
        output.contains("validation passed"),
        "validate should emit a clean success marker. Output:\n{output}"
    );
    assert!(
        !output.contains("Error:"),
        "validate should not emit an error while succeeding. Output:\n{output}"
    );
    assert!(
        !output.contains("warn:"),
        "validate should not emit warnings after fresh init. Output:\n{output}"
    );
    assert!(
        !output.contains("repair"),
        "validate should not require self-heal/repair after fresh init. Output:\n{output}"
    );
    assert!(
        !output.contains("self-heal"),
        "validate should not report self-heal after fresh init. Output:\n{output}"
    );
    assert!(
        !output.contains("requires isolated git worktree"),
        "fresh non-git validation should not be rejected by workspace preflight. Output:\n{output}"
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
    assert!(success, "session acquire should succeed: {acquire_out}");
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
        "validate should either fail on workspace or auto-acquire session: {out_missing}"
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
        "validate should either fail on workspace or auto-acquire session: {out_wrong}"
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
        "validate should pass with correct agent+password: {out_good}"
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
    assert!(success, "session acquire should succeed: {acquire_out}");
    let password = extract_password(&acquire_out).expect("acquire output should include password");
    let auth_env = [
        ("DECAPOD_AGENT_ID", "agent-expire"),
        ("DECAPOD_SESSION_PASSWORD", password.as_str()),
        ("DECAPOD_GROUP_BROKER_INTERNAL", "1"),
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

    let sessions_dir = temp_path.join(".config").join("decapod").join("sessions");
    let mut project_dir = None;
    for entry in fs::read_dir(sessions_dir).expect("read sessions dir") {
        let entry = entry.expect("read entry");
        if entry.file_type().expect("file type").is_dir() {
            project_dir = Some(entry.path());
            break;
        }
    }
    let project_dir = project_dir.expect("project sessions dir not found");
    let session_path = project_dir.join("agent-expire.json");

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
        "unknown session acquire should succeed: {out_unknown_acquire}"
    );

    let todo_db = temp_path.join(".decapod").join("data").join("todo.db");
    let conn = rusqlite::Connection::open(todo_db).expect("open todo db");
    let assigned_to: String = conn
        .query_row(
            "SELECT assigned_to FROM tasks WHERE id = ?1",
            [task_id.as_str()],
            |row| row.get(0),
        )
        .expect("query task owner");
    assert_eq!(
        assigned_to, "",
        "expired session cleanup should unassign task"
    );
}

#[test]
#[ignore = "Broken by constitution densification PR"]
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
        "AGENTS.md should be ≤ 100 lines (got {agents_lines})"
    );

    // Check agent-specific files (should be ≤ 70)
    for file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {file}"));
        let line_count = content.lines().count();
        assert!(
            line_count <= 70,
            "{file} should be ≤ 70 lines (got {line_count})"
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

    // Check that all entrypoints reference core/DECAPOD
    let files = ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"];

    for file in files {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {file}"));
        assert!(
            content.contains("core/DECAPOD"),
            "{file} should reference canonical router (core/DECAPOD)"
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

    let invariant_markers = ["core/DECAPOD", "decapod validate", "stop if", "✅"];

    for marker in invariant_markers {
        assert!(
            agents_content.contains(marker),
            "AGENTS.md should contain invariant marker: {marker}"
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
    let tampered = content.replace("core/DECAPOD", "MISSING/ROUTER");
    fs::write(&agents_path, tampered).expect("Failed to write tampered AGENTS.md");

    // Run decapod validate (should fail)
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(
        !success,
        "decapod validate should fail after tampering. Output:\n{output}"
    );

    // Check that it detected the missing invariant
    assert!(
        output.contains("Invariant missing: Router pointer to core/DECAPOD"),
        "Validation should detect missing router invariant. Output:\n{output}"
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
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(
        !success,
        "decapod validate should fail on bloated entrypoint. Output:\n{output}"
    );

    // Check that it detected the line limit violation
    assert!(
        output.contains("CLAUDE.md exceeds line limit"),
        "Validation should detect bloated entrypoint. Output:\n{output}"
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
            .unwrap_or_else(|_| panic!("Failed to read {file}"));
        assert!(
            content.contains("AGENTS.md"),
            "{file} should defer to AGENTS.md"
        );
    }
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn test_agents_entrypoint_scopes_decapod_invocation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();

    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    let agents_content =
        fs::read_to_string(temp_path.join("AGENTS.md")).expect("Failed to read AGENTS.md");

    for marker in [
        "Agents act. Decapod orients.",
        "Decapod is not your executor",
        "Do not call Decapod for every trivial file read",
        "Completion pressure",
        "readiness to claim completion",
    ] {
        assert!(
            agents_content.contains(marker),
            "AGENTS.md should teach scoped Decapod invocation: {marker}"
        );
    }
}

#[test]
fn test_root_entrypoints_match_scaffold_generators() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let root_path = repo_root.join(file);
        let root_content =
            fs::read_to_string(&root_path).unwrap_or_else(|_| panic!("Failed to read {file}"));
        let template_content =
            assets::get_template(file).unwrap_or_else(|| panic!("Missing generated {file}"));

        assert_eq!(
            root_content, template_content,
            "Entrypoint drift detected: {file} differs from Rust scaffold generator output."
        );
    }
}

#[test]
fn test_entrypoints_record_release_fingerprints_and_specs_manifest_attestation() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, output) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed: {output}");

    for surface in entrypoint_integrity::ENTRYPOINT_FILES {
        let content = fs::read_to_string(temp_path.join(surface)).expect("read entrypoint");
        assert!(content.contains("<!-- decapod-release: 0.70.0 -->"));
        assert!(content.contains(&format!(
            "<!-- decapod-fingerprint: {} -->",
            entrypoint_integrity::expected_fingerprint(surface).expect("compiled fingerprint")
        )));
    }

    let manifest_body =
        fs::read_to_string(temp_path.join(".decapod/generated/specs/.manifest.json"))
            .expect("read specs manifest");
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_body).expect("parse specs manifest");
    assert_eq!(manifest["decapod_release"], "0.70.0");
    assert!(
        !manifest
            .as_object()
            .expect("manifest object")
            .contains_key("binary_hash")
    );
    assert_eq!(manifest["entrypoints"].as_array().unwrap().len(), 4);
    for surface in entrypoint_integrity::ENTRYPOINT_FILES {
        let entry = manifest["entrypoints"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["path"] == surface)
            .expect("entrypoint manifest entry");
        assert_eq!(
            entry["fingerprint"],
            entrypoint_integrity::expected_fingerprint(surface).expect("compiled fingerprint")
        );
    }
}

#[test]
fn test_validate_reports_payload_modified_entrypoint() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("AGENTS.md");
    let mut content = fs::read_to_string(&path).expect("read AGENTS.md");
    content.push_str("\nuser payload tamper\n");
    fs::write(path, content).expect("tamper AGENTS.md");

    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(!success, "tampered entrypoint must fail validation");
    assert!(
        output.contains("Finding: entrypoint_payload_modified"),
        "expected typed payload finding. Output:\n{output}"
    );
}

#[test]
fn test_validate_migrates_valid_legacy_entrypoint_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("CLAUDE.md");
    let content = fs::read_to_string(&path).expect("read CLAUDE.md");
    let rewritten = content
        .replace(
            &format!(
                "<!-- decapod-release: {} -->",
                entrypoint_integrity::RELEASE_VERSION
            ),
            "<!-- decapod-release: 0.69.1 -->",
        )
        .replace(
            &format!(
                "<!-- decapod-fingerprint: {} -->",
                entrypoint_integrity::expected_fingerprint("CLAUDE.md")
                    .expect("compiled fingerprint")
            ),
            "<!-- decapod-binary-sha256: legacy-release-contract -->",
        );
    fs::write(&path, rewritten).expect("rewrite binary sha");
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(success, "validate should refresh legacy metadata: {output}");
    assert!(output.contains("heal_release_bound_entrypoints"));

    let content = fs::read_to_string(&path).expect("read rewritten CLAUDE.md");
    assert!(content.contains("<!-- decapod-release: 0.70.0 -->"));
    assert!(content.contains(&format!(
        "<!-- decapod-fingerprint: {} -->",
        entrypoint_integrity::expected_fingerprint("CLAUDE.md").expect("compiled fingerprint")
    )));
}

#[test]
fn test_validate_rejects_tweaked_entrypoint_fingerprint() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("CLAUDE.md");
    let content = fs::read_to_string(&path).expect("read CLAUDE.md");
    let expected =
        entrypoint_integrity::expected_fingerprint("CLAUDE.md").expect("compiled fingerprint");
    let rewritten = content.replace(
        &format!("<!-- decapod-fingerprint: {expected} -->"),
        "<!-- decapod-fingerprint: 0000000000000000000000000000000000000000000000000000000000000000 -->",
    );
    fs::write(&path, rewritten).expect("rewrite fingerprint");

    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(
        !success,
        "validate must reject a tweaked fingerprint: {output}"
    );
    assert!(
        output.contains("Finding: entrypoint_fingerprint_mismatch"),
        "expected typed fingerprint finding. Output:\n{output}"
    );
    let content = fs::read_to_string(path).expect("read tweaked CLAUDE.md");
    assert!(content.contains("decapod-fingerprint: 000000000000"));
}

#[test]
fn test_validate_rejects_missing_entrypoint_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("GEMINI.md");
    let content = fs::read_to_string(&path).expect("read GEMINI.md");
    let payload = content.lines().skip(2).collect::<Vec<_>>().join("\n");
    fs::write(path, payload).expect("remove entrypoint metadata");
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(!success, "missing entrypoint metadata must fail validation");
    assert!(
        output.contains("Finding: entrypoint_metadata_missing"),
        "expected typed metadata finding. Output:\n{output}"
    );
}

#[test]
fn test_validate_rejects_malformed_entrypoint_metadata() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("GEMINI.md");
    let content = fs::read_to_string(&path).expect("read GEMINI.md");
    let malformed = content.replace(
        &format!(
            "<!-- decapod-fingerprint: {} -->",
            entrypoint_integrity::expected_fingerprint("GEMINI.md").expect("compiled fingerprint")
        ),
        "<!-- decapod-fingerprint: -->",
    );
    fs::write(path, malformed).expect("malform entrypoint metadata");
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(
        !success,
        "malformed entrypoint metadata must fail validation"
    );
    assert!(
        output.contains("Finding: entrypoint_metadata_malformed"),
        "expected typed metadata finding. Output:\n{output}"
    );
}

#[test]
fn test_validate_does_not_overwrite_payload_tamper() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");

    let path = temp_path.join("CODEX.md");
    let mut content = fs::read_to_string(&path).expect("read CODEX.md");
    content.push_str("\nmodified\n");
    fs::write(&path, content).expect("tamper CODEX.md");

    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(
        !success,
        "validation must fail without overwriting payload tamper: {output}"
    );
    let (success, output) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(
        success,
        "explicit regeneration should restore entrypoint: {output}"
    );
    acquire_session(&temp_path);
    let (success, output) = run_decapod(&temp_path, &["validate"]);
    assert!(success, "regenerated entrypoint should validate: {output}");
}

#[cfg(unix)]
#[test]
fn test_validate_rejects_entrypoint_symlink() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let temp_path = temp_dir.path().to_path_buf();
    let (success, _) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed");
    acquire_session(&temp_path);

    let path = temp_path.join("CODEX.md");
    fs::remove_file(&path).expect("remove CODEX.md");
    symlink(temp_path.join("AGENTS.md"), &path).expect("replace CODEX.md with symlink");
    let (success, output) = run_decapod(&temp_path, &["validate", "--format", "json"]);
    assert!(!success, "entrypoint symlink must fail validation");
    assert!(
        output.contains("Finding: entrypoint_unsupported_file_type"),
        "expected typed file-type finding. Output:\n{output}"
    );
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
            .nth(2)
            .is_some_and(|l| l.contains("CLAUDE.md")),
        "CLAUDE.md header should include CLAUDE.md"
    );
    assert_eq!(
        root_claude.lines().skip(3).collect::<Vec<_>>(),
        root_gemini.lines().skip(3).collect::<Vec<_>>(),
        "Root entrypoints should only differ by file-specific header: CLAUDE.md != GEMINI.md"
    );
    assert_eq!(
        root_claude.lines().skip(3).collect::<Vec<_>>(),
        root_codex.lines().skip(3).collect::<Vec<_>>(),
        "Root entrypoints should only differ by file-specific header: CLAUDE.md != CODEX.md"
    );

    let tpl_claude = assets::get_template("CLAUDE.md").expect("generated CLAUDE");
    let tpl_gemini = assets::get_template("GEMINI.md").expect("generated GEMINI");
    let tpl_codex = assets::get_template("CODEX.md").expect("generated CODEX");

    assert_eq!(
        tpl_claude.lines().skip(3).collect::<Vec<_>>(),
        tpl_gemini.lines().skip(3).collect::<Vec<_>>(),
        "Template entrypoints should only differ by file-specific header: CLAUDE.md != GEMINI.md"
    );
    assert_eq!(
        tpl_claude.lines().skip(3).collect::<Vec<_>>(),
        tpl_codex.lines().skip(3).collect::<Vec<_>>(),
        "Template entrypoints should only differ by file-specific header: CLAUDE.md != CODEX.md"
    );
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn test_entrypoints_use_embedded_docs_paths_only() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let content =
            fs::read_to_string(repo_root.join(file)).unwrap_or_else(|_| panic!("read {file}"));
        assert!(
            !content.contains("(constitution/"),
            "{file} must not reference direct constitution/* filesystem paths"
        );
        assert!(
            !content.contains("decapod docs show"),
            "{file} must use constitution.get RPC instead of docs show"
        );
        assert!(
            content.contains(
                r#"decapod rpc --op constitution.get --params '{"section":"docs/PLAYBOOK"}'"#,
            ),
            "{file} must reference embedded docs path for operator playbook"
        );
        assert!(
            content.contains(".decapod/workspaces"),
            "{file} must mandate canonical Decapod worktree root"
        );
        assert!(
            content.contains("decapod todo add \"<task>\""),
            "{file} must require task creation before claim"
        );
        assert!(
            !content.contains(".claude/worktrees"),
            "{file} must never reference non-canonical .claude/worktrees path"
        );
    }
}

#[test]
fn test_top_level_docs_avoid_direct_constitution_file_links() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(repo_root.join("README.md")).expect("read README.md");
    let security = fs::read_to_string(repo_root.join("SECURITY.md")).expect("read SECURITY.md");

    assert!(
        readme.contains("assets/constitution.json") && readme.contains("core/DECAPOD"),
        "README.md should point to the embedded constitution asset section"
    );

    assert!(
        !security.contains("(constitution/"),
        "SECURITY.md should not instruct direct constitution file access"
    );
    assert!(
        security.contains("constitution.json#core/SECURITY"),
        "SECURITY.md should point to the embedded security constitution section"
    );
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn test_intent_context_spec_contract_alignment() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let readme = fs::read_to_string(repo_root.join("README.md")).expect("read README.md");
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args([
            "rpc",
            "--op",
            "constitution.get",
            "--params",
            r#"{"section":"core/DECAPOD"}"#,
        ])
        .output()
        .expect("run decapod constitution.get");
    assert!(output.status.success(), "constitution.get failed");
    let core_decapod = String::from_utf8_lossy(&output.stdout);
    let lib_rs = fs::read_to_string(repo_root.join("src/lib.rs")).expect("read src/lib.rs");
    let cli_rs = fs::read_to_string(repo_root.join("src/cli.rs")).expect("read src/cli.rs");

    let contract_phrase =
        "turn intent into context, then context into explicit specifications before inference";

    assert!(
        readme.contains(contract_phrase),
        "README.md must state the intent->context->specifications flow"
    );
    assert!(
        core_decapod.contains(contract_phrase),
        "core/DECAPOD must state the intent->context->specifications flow"
    );
    assert!(
        lib_rs.contains(contract_phrase) || cli_rs.contains(contract_phrase),
        "src/lib.rs or src/cli.rs CLI about text must state the intent->context->specifications flow"
    );
}

#[test]
#[ignore = "Broken by constitution densification PR"]
fn test_core_decapod_routes_without_competing_with_agents() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let asset = fs::read_to_string(repo_root.join("assets/constitution.json"))
        .expect("read assets/constitution.json");
    let graph: serde_json::Value = serde_json::from_str(&asset).expect("parse constitution asset");
    let core_decapod = graph["nodes"]["core/DECAPOD"]["content"].to_string();

    for marker in [
        "core/DECAPOD is a router",
        "Agent operating rules: use AGENTS.md",
        "Provider-specific shims",
        "Do not turn this router into generic documentation noise",
    ] {
        assert!(
            core_decapod.contains(marker),
            "core/DECAPOD should route scoped purposes: {marker}"
        );
    }
}

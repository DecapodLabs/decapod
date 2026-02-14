//! Integration tests for entrypoint correctness.
//!
//! These tests ensure that `decapod init` creates correct entrypoint files
//! and that `decapod validate` enforces invariants and detects tampering.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run decapod command in a temp directory
fn run_decapod(temp_dir: &PathBuf, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(temp_dir)
        .args(args)
        .output()
        .expect("Failed to execute decapod");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{}\n{}", stdout, stderr);

    (output.status.success(), combined)
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

    // Check agent-specific files (should be ≤ 50)
    for file in ["CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let content = fs::read_to_string(temp_path.join(file))
            .unwrap_or_else(|_| panic!("Failed to read {}", file));
        let line_count = content.lines().count();
        assert!(
            line_count <= 50,
            "{} should be ≤ 50 lines (got {})",
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

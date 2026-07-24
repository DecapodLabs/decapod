// Moved from src/decapod/core/validate.rs
use super::{
    SurfaceKind, ValidationContext, is_allowed_non_code_path, is_decapod_isolated_worktree,
    is_non_code_path, predict_ci_outcome, strip_git_quotes, validate_git_workspace_context,
    validate_root_dockerfile_seed_detection,
};
use super::{is_protected_git_branch, parse_ahead_behind_counts};
use std::fs;
use std::sync::atomic::Ordering;
use tempfile::tempdir;

fn git_init(dir: &std::path::Path) {
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .expect("git config name");
}

fn git_commit(dir: &std::path::Path, msg: &str) {
    fs::write(dir.join(".gitkeep"), "").expect("write gitkeep");
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", msg])
        .current_dir(dir)
        .output()
        .expect("git commit");
}

#[test]
fn test_validate_root_dockerfile_seed_detection() {
    let tmp = tempdir().expect("temporary repository");
    fs::write(
            tmp.path().join("Dockerfile"),
            "ARG DECAPOD_IMAGE=ghcr.io/decapodlabs/decapod:v0.72.13\nFROM $DECAPOD_IMAGE\nLABEL org.decapod.managed=\"workspace\"\n",
        )
        .expect("write root Dockerfile");

    let ctx = ValidationContext::new();
    validate_root_dockerfile_seed_detection(&ctx, tmp.path())
        .expect("root Dockerfile validation should complete");

    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 1);
    assert!(ctx.fails.lock().unwrap().iter().any(|failure| {
        failure.contains("Root Dockerfile contains Decapod workspace image markers")
    }));
}

fn decapod_worktree_fixture() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main");
    fs::create_dir_all(&main_root).expect("create main");
    git_init(&main_root);
    git_commit(&main_root, "init");

    let wt_path = main_root
        .join(".decapod")
        .join("workspaces")
        .join("agent-test-task");
    fs::create_dir_all(wt_path.parent().unwrap()).expect("create workspaces dir");
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test/task",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");
    assert!(
        output.status.success(),
        "git worktree add failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    (tmp, main_root, wt_path)
}

#[test]
fn protected_branch_matching_is_limited_to_protected_refs() {
    assert!(is_protected_git_branch("master"));
    assert!(is_protected_git_branch("main"));
    assert!(is_protected_git_branch("release/2026.05"));
    assert!(!is_protected_git_branch("agent/codex/fix"));
    assert!(!is_protected_git_branch("feature/main-cleanup"));
}

#[test]
fn parses_git_ahead_behind_counts() {
    assert_eq!(parse_ahead_behind_counts("3\t1\n"), Some((3, 1)));
    assert_eq!(parse_ahead_behind_counts("0 12"), Some((0, 12)));
    assert_eq!(parse_ahead_behind_counts("bad 12"), None);
}

#[test]
fn ci_prediction_preserves_actionable_validation_signals() {
    let failures = vec!["gate failed".to_string()];
    let warnings = vec!["review this warning".to_string()];

    let failed = predict_ci_outcome(1, 1, &failures, &warnings);
    assert_eq!(failed.result, "fail");
    assert_eq!(failed.confidence, "high");
    assert_eq!(failed.reasons, failures);
    assert!(!failed.recommendations.is_empty());

    let review = predict_ci_outcome(0, 1, &[], &warnings);
    assert_eq!(review.result, "review");
    assert_eq!(review.confidence, "medium");
    assert_eq!(review.reasons, warnings);

    let passing = predict_ci_outcome(0, 0, &[], &[]);
    assert_eq!(passing.result, "pass");
    assert_eq!(passing.confidence, "high");
}

#[test]
fn isolated_worktree_detection_uses_owning_main_repo() {
    let (_tmp, main_root, wt_path) = decapod_worktree_fixture();

    assert!(is_decapod_isolated_worktree(&main_root, &wt_path));
    assert!(
        is_decapod_isolated_worktree(&wt_path, &wt_path),
        "validation can pass the worktree as main_root; detection must still resolve the owner repo"
    );
}

#[test]
fn issue_757_workspace_status_and_validate_agree_on_decapod_worktree() {
    let (_tmp, _main_root, wt_path) = decapod_worktree_fixture();
    let ctx = ValidationContext::new();

    let status = crate::workspace::get_workspace_status(&wt_path)
        .expect("workspace status should resolve the isolated worktree");
    assert!(status.git.in_worktree);
    assert!(!status.git.is_main_repo);
    assert!(status.can_work);

    validate_git_workspace_context(&ctx, &wt_path, &wt_path).expect("workspace context validation");

    assert_eq!(
        ctx.fail_count.load(Ordering::Relaxed),
        0,
        "unexpected validation failures: {:?}",
        ctx.fails.lock().unwrap()
    );
}

#[test]
fn external_worktree_reports_context_mismatch_instead_of_generic_failure() {
    let (_tmp, main_root, _decapod_path) = decapod_worktree_fixture();
    let external_path = main_root
        .parent()
        .expect("fixture parent")
        .join("external-worktree");
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "external/agent",
            external_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("external worktree should start");
    assert!(output.status.success());

    let ctx = ValidationContext::new();
    validate_git_workspace_context(&ctx, &main_root, &external_path)
        .expect("workspace context validation");
    let failures = ctx.fails.lock().unwrap();
    assert_eq!(ctx.fail_count.load(Ordering::Relaxed), 1);
    assert!(failures.iter().any(|failure| {
        failure.contains("workspace_context_mismatch") && failure.contains("not Decapod-owned")
    }));
}

// --- is_allowed_non_code_path ---

#[test]
fn non_code_allows_docs_directory() {
    assert!(is_allowed_non_code_path("docs/agent/api-index.md"));
    assert!(is_allowed_non_code_path("docs/sub/deep/file.txt"));
    assert!(!is_allowed_non_code_path("src/main.rs"));
}

#[test]
fn non_code_allows_markdown_anywhere() {
    // .md extension is always allowed regardless of directory
    assert!(is_allowed_non_code_path("README.md"));
    assert!(is_allowed_non_code_path("docs/guide.md"));
    assert!(is_allowed_non_code_path("sub/dir/notes.md"));
    assert!(is_allowed_non_code_path("src/lib.md"));
    assert!(!is_allowed_non_code_path("src/lib.rs"));
}

#[test]
fn non_code_allows_decapod_config_and_override() {
    assert!(is_allowed_non_code_path(".decapod/config.toml"));
    assert!(is_allowed_non_code_path(".decapod/OVERRIDE.md"));
    assert!(!is_allowed_non_code_path(".decapod/other.toml"));
    assert!(!is_allowed_non_code_path(".decapod/data/store.jsonl"));
}

#[test]
fn non_code_allows_generated_specs_and_artifacts() {
    assert!(is_allowed_non_code_path(".decapod/managed/specs/INTENT.md"));
    assert!(is_allowed_non_code_path(
        ".decapod/managed/artifacts/plan.json"
    ));
    assert!(is_allowed_non_code_path(
        ".decapod/managed/specs/deep/nested.md"
    ));
    // .md files always pass via the ends_with(".md") rule, so this is allowed too
    assert!(is_allowed_non_code_path(".decapod/managed/other.md"));
    // A non-.md file in generated/ but outside specs/ or artifacts/ is rejected
    assert!(!is_allowed_non_code_path(".decapod/managed/other.json"));
}

#[test]
fn non_code_allows_contracts() {
    assert!(is_allowed_non_code_path(".decapod/contracts/interface.md"));
    assert!(!is_allowed_non_code_path(".decapod/contracts")); // no trailing slash/file
}

#[test]
fn non_code_rejects_source_files() {
    assert!(!is_allowed_non_code_path("src/lib.rs"));
    assert!(!is_allowed_non_code_path("lib/python/main.py"));
    assert!(!is_allowed_non_code_path("package.json"));
    assert!(!is_allowed_non_code_path("Cargo.toml"));
}

// --- is_non_code_path (porcelain line parsing) ---

#[test]
fn non_code_path_ordinary_modified() {
    assert!(is_non_code_path(" M docs/guide.md"));
    assert!(is_non_code_path("M  .decapod/config.toml"));
    assert!(!is_non_code_path(" M src/main.rs"));
}

#[test]
fn non_code_path_short_line_rejected() {
    assert!(!is_non_code_path(" M ")); // too short
    assert!(!is_non_code_path(""));
}

#[test]
fn non_code_path_rename_both_sides_must_qualify() {
    assert!(is_non_code_path("R  docs/old.md -> docs/new.md"));
    assert!(!is_non_code_path("R  docs/old.md -> src/new.rs"));
    assert!(!is_non_code_path("R  src/old.rs -> docs/new.md"));
}

#[test]
fn non_code_path_copy_both_sides_must_qualify() {
    assert!(is_non_code_path("C  docs/template.md -> docs/copy.md"));
    assert!(!is_non_code_path("C  docs/template.md -> src/copy.rs"));
}

// --- strip_git_quotes ---

#[test]
fn strip_quotes_removes_surrounding_double_quotes() {
    assert_eq!(strip_git_quotes("\"weird file.md\""), "weird file.md");
    assert_eq!(strip_git_quotes("normal.md"), "normal.md");
    assert_eq!(strip_git_quotes("\"\""), "");
}

#[test]
fn non_code_path_quoted_filename() {
    assert!(is_non_code_path(" M \"weird name.md\""));
    assert!(!is_non_code_path(" M \"weird name.rs\""));
}

#[test]
fn surface_kind_variants_are_correct() {
    assert_eq!(SurfaceKind::Authority, SurfaceKind::Authority);
    assert_eq!(SurfaceKind::Evidence, SurfaceKind::Evidence);
    assert_eq!(SurfaceKind::Projection, SurfaceKind::Projection);
    assert_ne!(SurfaceKind::Authority, SurfaceKind::Projection);
    assert_ne!(SurfaceKind::Evidence, SurfaceKind::Projection);
}

#[test]
fn surface_kind_serialization() {
    use serde_json::json;
    let authority = json!(SurfaceKind::Authority);
    let evidence = json!(SurfaceKind::Evidence);
    let projection = json!(SurfaceKind::Projection);
    assert_eq!(authority, "Authority");
    assert_eq!(evidence, "Evidence");
    assert_eq!(projection, "Projection");
}

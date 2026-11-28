//! Integration tests for entrypoint validation contracts, feature-branch
//! commit discipline, and governed docs admission.

use decapod::core::entrypoint_integrity;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

#[path = "support/cli_bin.rs"]
mod cli_bin;
use cli_bin::{
    acquire_session, resolve_decapod_bin, resolve_repo_root, run_decapod, run_decapod_with_env,
};

/// Nested path under `.decapod/workspaces/` so workspace protection treats the
/// tree as an agent worktree (plain TempDir is classified as the main checkout).
fn worktree_like_temp() -> (TempDir, PathBuf) {
    let outer = TempDir::new().expect("temp dir");
    let path = outer
        .path()
        .join(".decapod")
        .join("workspaces")
        .join("test-entrypoint-discipline");
    fs::create_dir_all(&path).expect("create nested worktree path");
    (outer, path)
}

/// Feature-branch repo with init artifacts committed on `main`.
fn git_init_feature_repo(temp_path: &PathBuf) {
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(temp_path)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-b", "main"]);
    git(&["config", "user.email", "test@example.com"]);
    git(&["config", "user.name", "Decapod Test"]);
    let (success, output) = run_decapod(temp_path, &["init", "--force"]);
    assert!(success, "decapod init should succeed: {output}");
    git(&["add", "-A"]);
    git(&["commit", "-m", "chore: initial decapod scaffold"]);
    git(&["checkout", "-b", "feature/entrypoint-discipline"]);
}

#[test]
fn test_feature_branch_rejects_hand_edited_entrypoint_commit() {
    let (_outer, temp_path) = worktree_like_temp();
    git_init_feature_repo(&temp_path);
    acquire_session(&temp_path);

    let path = temp_path.join("AGENTS.md");
    let mut content = fs::read_to_string(&path).expect("read AGENTS.md");
    content.push_str("\nhand-edited in a PR\n");
    fs::write(&path, content).expect("hand edit");
    let status = Command::new("git")
        .args(["add", "AGENTS.md"])
        .current_dir(&temp_path)
        .status()
        .expect("git add");
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "bad: hand edit entrypoint"])
        .current_dir(&temp_path)
        .status()
        .expect("git commit");
    assert!(status.success());

    // Enable git gates (do not set DECAPOD_VALIDATE_SKIP_GIT_GATES).
    let (success, output) =
        run_decapod_with_env(&temp_path, &["validate", "--format", "json"], &[]);
    assert!(
        !success,
        "hand-edited entrypoint commit must fail: {output}"
    );
    // No verified task/worktree binding exists in this scratch source branch.
    // The CLI must fail closed at its authoring gate, before execution.
    assert!(
        output.contains("AUTHORING_ACTOR_REQUIRED"),
        "unbound source checkout must not be admitted: {output}"
    );
    // Independently prove the precise tampered payload is still rejected by
    // the read-only entrypoint integrity primitive.
    assert!(entrypoint_integrity::validate_entrypoint(&temp_path, "AGENTS.md").is_err());
}

#[cfg(unix)]
#[test]
fn test_feature_branch_rejects_mode_only_entrypoint_touch() {
    use std::os::unix::fs::PermissionsExt;

    let (_outer, temp_path) = worktree_like_temp();
    git_init_feature_repo(&temp_path);
    acquire_session(&temp_path);

    let path = temp_path.join("CLAUDE.md");
    let mut perms = fs::metadata(&path).expect("meta").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod");
    let status = Command::new("git")
        .args(["update-index", "--chmod=+x", "CLAUDE.md"])
        .current_dir(&temp_path)
        .status()
        .expect("git update-index");
    assert!(status.success(), "update-index chmod");
    // Ensure the mode change is staged for commit.
    let status = Command::new("git")
        .args(["add", "CLAUDE.md"])
        .current_dir(&temp_path)
        .status()
        .expect("git add");
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "bad: mode-only entrypoint touch"])
        .current_dir(&temp_path)
        .status()
        .expect("git commit");
    assert!(status.success(), "mode-only commit");

    let (success, output) =
        run_decapod_with_env(&temp_path, &["validate", "--format", "json"], &[]);
    assert!(!success, "mode-only entrypoint touch must fail: {output}");
    // The CLI refuses the unbound checkout before it can claim validation.
    assert!(
        output.contains("AUTHORING_ACTOR_REQUIRED"),
        "unbound source checkout must not be admitted: {output}"
    );
    // Preserve the executable-bit fixture for the immutable Git-history mode
    // check, which is also covered by history::tests in the core suite.
    let mode = Command::new("git")
        .args(["ls-tree", "HEAD", "--", "CLAUDE.md"])
        .current_dir(&temp_path)
        .output()
        .expect("read immutable commit mode");
    assert!(mode.status.success());
    assert!(
        String::from_utf8_lossy(&mode.stdout).starts_with("100755 blob "),
        "mode-only fixture must contain a committed executable-bit change"
    );
}

#[test]
fn test_feature_branch_allows_canonical_entrypoint_rewrite() {
    let (_outer, temp_path) = worktree_like_temp();
    git_init_feature_repo(&temp_path);
    acquire_session(&temp_path);

    // Simulate a stale body then force-regenerate via init --force (Decapod path).
    let path = temp_path.join("GEMINI.md");
    fs::write(&path, "stale\n").expect("stale write");
    let (success, output) = run_decapod(&temp_path, &["init", "--force"]);
    assert!(success, "init --force should restore template: {output}");
    let status = Command::new("git")
        .args(["add", "GEMINI.md"])
        .current_dir(&temp_path)
        .status()
        .expect("git add");
    assert!(status.success());
    // Commit only if GEMINI.md actually changed.
    let porcelain = Command::new("git")
        .args(["status", "--porcelain", "GEMINI.md"])
        .current_dir(&temp_path)
        .output()
        .expect("git status");
    if !porcelain.stdout.is_empty() {
        let status = Command::new("git")
            .args(["commit", "-m", "chore: regenerate entrypoint via decapod"])
            .current_dir(&temp_path)
            .status()
            .expect("git commit");
        assert!(status.success());
    }

    let (_success, output) =
        run_decapod_with_env(&temp_path, &["validate", "--format", "json"], &[]);
    // Other gates may still fail (publication bundle, specs, etc.); discipline must not.
    assert!(
        !output.contains("ENTRYPOINT_COMMIT_DISCIPLINE"),
        "canonical rewrite must not trip discipline gate: {output}"
    );
}

#[test]
fn test_feature_branch_allows_canonical_rewrite_from_prior_release() {
    let (_outer, temp_path) = worktree_like_temp();
    git_init_feature_repo(&temp_path);
    acquire_session(&temp_path);

    let prior_release = entrypoint_integrity::render_entrypoint_for_release("AGENTS.md", "0.98.1")
        .expect("prior release should render the canonical entrypoint");
    fs::write(temp_path.join("AGENTS.md"), prior_release).expect("write prior release entrypoint");
    let status = Command::new("git")
        .args(["add", "AGENTS.md"])
        .current_dir(&temp_path)
        .status()
        .expect("git add");
    assert!(status.success());
    let status = Command::new("git")
        .args(["commit", "-m", "chore: restore prior release entrypoint"])
        .current_dir(&temp_path)
        .status()
        .expect("git commit");
    assert!(status.success());

    let (_success, output) =
        run_decapod_with_env(&temp_path, &["validate", "--format", "json"], &[]);
    assert!(
        !output.contains("ENTRYPOINT_COMMIT_DISCIPLINE"),
        "canonical prior-release rewrite must not trip discipline gate: {output}"
    );
}

// This required target has a separate unchanged aggregate budget from CLI
// contracts; retain the genuine governed flow without duplicating API cases.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "common/consumer_readiness_fixture.rs"]
mod consumer_readiness_fixture;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/fresh_docs_readiness.rs"]
mod fresh_docs_readiness;

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fresh_docs_cli_admission_then_complete_runtime_proves_current_source() {
    fresh_docs_readiness::fresh_governed_docs_cli_admission_then_runtime_executes_first_proof();
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn canonical_local_target_cli_readiness_is_source_bound() {
    fresh_docs_readiness::canonical_local_cli_proves_and_rejects_stale_authority();
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn canonical_workunit_withdraws_superseded_proof_and_recovers_after_lease_renewal() {
    fresh_docs_readiness::canonical_local_workunit_rejects_superseded_proof_and_recovers();
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "acceptance/publication_admission.rs"]
mod publication_admission;

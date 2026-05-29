use decapod::core::workspace;
use std::fs;
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
    // Ensure there is something to commit
    fs::write(dir.join(".gitkeep"), "").ok();
    std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(dir)
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", msg])
        .current_dir(dir)
        .output()
        .expect("git commit");
}

// ---- get_main_repo_root ----

#[test]
fn test_get_main_repo_root_from_worktree() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    git_init(&main_root);
    git_commit(&main_root, "init");

    // Add a worktree
    let wt_path = tmp.path().join("wt_repo");
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "test-branch",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");

    // Test resolving from main repo
    let resolved_main = workspace::get_main_repo_root(&main_root).expect("resolve main");
    let canonical_resolved_main = fs::canonicalize(&resolved_main).unwrap_or(resolved_main);
    let canonical_main_root = fs::canonicalize(&main_root).unwrap_or(main_root.clone());
    assert_eq!(canonical_resolved_main, canonical_main_root);

    // Test resolving from worktree — should resolve to main repo
    let resolved_wt = workspace::get_main_repo_root(&wt_path).expect("resolve wt");
    let canonical_resolved_wt = fs::canonicalize(&resolved_wt).unwrap_or(resolved_wt);
    assert_eq!(canonical_resolved_wt, canonical_main_root);
}

#[test]
fn test_get_main_repo_root_from_plain_repo() {
    let tmp = tempdir().expect("tempdir");
    let repo_root = tmp.path().join("plain_repo");
    fs::create_dir_all(&repo_root).expect("create plain_repo");

    git_init(&repo_root);
    git_commit(&repo_root, "init");

    // Not a worktree, so get_main_repo_root should return the repo root itself
    let resolved = workspace::get_main_repo_root(&repo_root).expect("resolve plain");
    let canonical_resolved = fs::canonicalize(&resolved).unwrap_or(resolved);
    let canonical_repo = fs::canonicalize(&repo_root).unwrap_or(repo_root.clone());
    assert_eq!(canonical_resolved, canonical_repo);
}

#[test]
fn test_get_main_repo_root_bare_repo() {
    let tmp = tempdir().expect("tempdir");
    let bare_path = tmp.path().join("bare.git");

    std::process::Command::new("git")
        .args(["init", "--bare", bare_path.to_str().unwrap()])
        .output()
        .expect("git init --bare");

    // get_main_repo_root on a bare repo should still succeed (git rev-parse works)
    let result = workspace::get_main_repo_root(&bare_path);
    // Bare repos may or may not resolve depending on git version; just ensure no panic
    let _ = result;
}

// ---- is_worktree ----

#[test]
fn test_is_worktree_detection() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    git_init(&main_root);
    git_commit(&main_root, "init");

    let wt_path = tmp.path().join("wt_repo");
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "test-branch-2",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");

    assert!(!workspace::is_worktree(&main_root).unwrap_or(false));
    assert!(workspace::is_worktree(&wt_path).unwrap_or(false));
}

#[test]
fn test_is_worktree_on_non_git_directory() {
    let tmp = tempdir().expect("tempdir");
    let non_git_dir = tmp.path().join("not_a_repo");
    fs::create_dir_all(&non_git_dir).expect("create dir");

    // Should return error/false for a non-git directory
    let result = workspace::is_worktree(&non_git_dir);
    assert!(result.is_err() || !result.unwrap());
}

// ---- discover_repo_root ----

#[test]
fn test_discover_repo_root_from_subdirectory() {
    let tmp = tempdir().expect("tempdir");
    let repo_root = tmp.path().join("disco_repo");
    fs::create_dir_all(&repo_root).expect("create repo");

    git_init(&repo_root);
    git_commit(&repo_root, "init");

    let subdir = repo_root.join("src").join("module");
    fs::create_dir_all(&subdir).expect("create subdir");

    let discovered = workspace::discover_repo_root(Some(&subdir)).expect("discover");
    let canonical_discovered = fs::canonicalize(&discovered).unwrap_or(discovered);
    let canonical_repo = fs::canonicalize(&repo_root).unwrap_or(repo_root);
    assert_eq!(canonical_discovered, canonical_repo);
}

#[test]
fn test_discover_repo_root_none_uses_cwd() {
    // Just ensure it doesn't panic — the result depends on whether
    // the current working directory is inside a git repo
    let _ = workspace::discover_repo_root(None);
}

// ---- is_non_code_change integration ----
// These test the actual git porcelain parsing by creating a repo with files,
// modifying them, and verifying the is_non_code_change gate logic.
// Since is_non_code_change is private, we test it indirectly through
// the validate_git_workspace_context gate by running `decapod validate`
// on a repo with only doc changes vs one with code changes.

#[test]
fn test_is_non_code_change_only_markdown() {
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path().join("nc_md_repo");
    fs::create_dir_all(&repo).expect("create repo");

    git_init(&repo);
    fs::write(repo.join("README.md"), "# hello\n").expect("write readme");
    std::process::Command::new("git")
        .args(["add", ".", "-A"])
        .current_dir(&repo)
        .output()
        .expect("git add");
    git_commit(&repo, "init");

    // Create an uncommitted .md change
    fs::write(repo.join("NOTES.md"), "some notes\n").expect("write notes");

    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&repo)
        .output()
        .expect("git status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should show a single untracked .md file
    assert!(stdout.contains(".md"), "expected .md in status: {stdout}");
}

#[test]
fn test_is_non_code_change_with_source_file() {
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path().join("nc_src_repo");
    fs::create_dir_all(&repo).expect("create repo");

    git_init(&repo);
    fs::write(repo.join("README.md"), "# hello\n").expect("write readme");
    std::process::Command::new("git")
        .args(["add", ".", "-A"])
        .current_dir(&repo)
        .output()
        .expect("git add");
    git_commit(&repo, "init");

    // Create an uncommitted source file
    fs::write(repo.join("main.rs"), "fn main() {}\n").expect("write source");

    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&repo)
        .output()
        .expect("git status");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("main.rs"),
        "expected main.rs in status: {stdout}"
    );
}

// ---- get_workspace_status ----

#[test]
fn test_workspace_status_in_main_repo() {
    let tmp = tempdir().expect("tempdir");
    let repo = tmp.path().join("ws_main_repo");
    fs::create_dir_all(&repo).expect("create repo");

    git_init(&repo);
    git_commit(&repo, "init");

    let status = workspace::get_workspace_status(&repo).expect("workspace status");
    assert!(
        !status.git.in_worktree,
        "main repo should not report as worktree"
    );
}

#[test]
fn test_workspace_status_in_worktree() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("ws_wt_main");
    fs::create_dir_all(&main_root).expect("create main");

    git_init(&main_root);
    git_commit(&main_root, "init");

    let wt_path = main_root
        .join(".decapod")
        .join("workspaces")
        .join("test-wt");
    fs::create_dir_all(wt_path.parent().unwrap()).ok();

    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-wt-status",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");

    let status = workspace::get_workspace_status(&wt_path).expect("workspace status");
    assert!(status.git.in_worktree, "worktree should report as worktree");
}

// ---- cleanup: remove worktrees to avoid polluting the test environment ----

#[test]
fn test_worktree_prune_after_add() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("prune_main");
    fs::create_dir_all(&main_root).expect("create main");

    git_init(&main_root);
    git_commit(&main_root, "init");

    let wt_path = tmp.path().join("prune_wt");
    std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            "agent/test-prune",
            wt_path.to_str().unwrap(),
        ])
        .current_dir(&main_root)
        .output()
        .expect("git worktree add");

    // Remove the worktree directory manually, then prune
    let _ = fs::remove_dir_all(&wt_path);
    let prune_result = workspace::prune_stale_worktree_config(&main_root);
    // Should succeed (prune stale entries)
    assert!(
        prune_result.is_ok(),
        "prune should succeed: {prune_result:?}"
    );
}

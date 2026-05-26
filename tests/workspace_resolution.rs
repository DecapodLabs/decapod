use decapod::core::workspace;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_get_main_repo_root_from_worktree() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    // Initialize main repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&main_root)
        .output()
        .expect("git init main");
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&main_root)
        .output()
        .expect("git commit main");

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
    // Canonicalize both for stable comparison
    let canonical_resolved_main = fs::canonicalize(&resolved_main).unwrap_or(resolved_main);
    let canonical_main_root = fs::canonicalize(&main_root).unwrap_or(main_root.clone());
    assert_eq!(canonical_resolved_main, canonical_main_root);

    // Test resolving from worktree
    let resolved_wt = workspace::get_main_repo_root(&wt_path).expect("resolve wt");
    let canonical_resolved_wt = fs::canonicalize(&resolved_wt).unwrap_or(resolved_wt);
    assert_eq!(canonical_resolved_wt, canonical_main_root);
}

#[test]
fn test_is_worktree_detection() {
    let tmp = tempdir().expect("tempdir");
    let main_root = tmp.path().join("main_repo");
    fs::create_dir_all(&main_root).expect("create main_repo");

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(&main_root)
        .output()
        .expect("git init main");
    std::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", "init"])
        .current_dir(&main_root)
        .output()
        .expect("git commit main");

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

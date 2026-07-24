// Moved from src/decapod/core/workspace.rs
use super::*;
use tempfile::tempdir;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git command should start");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_extract_task_ids_from_branch() {
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/bugs-01kvtvsvteg1t4ds"),
        vec!["bugs_01kvtvsvteg1t4ds".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/feat-01kvtvsvteg1t4ds-1782239277"),
        vec!["feat_01kvtvsvteg1t4ds".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/todo-01kvtr-plus-2-1782239277"),
        vec!["todo_01kvtr".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/bugs_01kvtvsvteg1t4ds"),
        vec!["bugs_01kvtvsvteg1t4ds".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/code-01kvw852x5g72pmc"),
        vec!["code_01kvw852x5g72pmc".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/aiml-01kvw852x5g72pmc"),
        vec!["aiml_01kvw852x5g72pmc".to_string()]
    );
    assert_eq!(
        extract_task_ids_from_branch("agent/unknown/some-feature-branch"),
        Vec::<String>::new()
    );
}

#[test]
fn detects_remote_default_base_branch() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("README.md"), "# project\n").expect("write readme");
    git(tmp.path(), &["add", "README.md"]);
    git(tmp.path(), &["commit", "-m", "initial"]);
    git(tmp.path(), &["branch", "-M", "main"]);
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:example/project.git",
        ],
    );
    git(
        tmp.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git(
        tmp.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );

    assert_eq!(detect_base_branch(tmp.path()).as_deref(), Some("main"));
}

#[test]
fn detects_no_base_branch_outside_git_without_failing() {
    let tmp = tempdir().expect("tempdir");

    assert_eq!(detect_base_branch(tmp.path()), None);
}

#[test]
fn merge_preflight_detects_conflicts_against_configured_base() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("file.txt"), "base\n").expect("write base");
    git(tmp.path(), &["add", "file.txt"]);
    git(tmp.path(), &["commit", "-m", "initial"]);
    git(tmp.path(), &["branch", "-M", "main"]);
    git(tmp.path(), &["checkout", "-b", "feature"]);
    std::fs::write(tmp.path().join("file.txt"), "feature\n").expect("write feature");
    git(tmp.path(), &["commit", "-am", "feature"]);
    git(tmp.path(), &["checkout", "main"]);
    std::fs::write(tmp.path().join("file.txt"), "base-update\n").expect("write base update");
    git(tmp.path(), &["commit", "-am", "base update"]);
    git(tmp.path(), &["checkout", "feature"]);

    let error = check_merge_conflicts(tmp.path(), "main").expect_err("conflict expected");
    assert!(error.to_string().contains("PR_MERGE_CONFLICT"));
    assert!(error.to_string().contains("rebase or merge main"));
}

#[test]
fn resolve_base_branch_prefers_explicit_value_then_repository_metadata() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("README.md"), "# project\n").expect("write readme");
    git(tmp.path(), &["add", "README.md"]);
    git(tmp.path(), &["commit", "-m", "initial"]);
    git(tmp.path(), &["branch", "-M", "main"]);

    assert_eq!(resolve_base_branch(tmp.path(), Some("release")), "release");
    assert_eq!(resolve_base_branch(tmp.path(), None), "main");
}

#[test]
fn worktree_creation_starts_from_resolved_base_branch() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("README.md"), "main\n").expect("write readme");
    git(tmp.path(), &["add", "README.md"]);
    git(tmp.path(), &["commit", "-m", "initial"]);
    git(tmp.path(), &["branch", "-M", "main"]);
    git(tmp.path(), &["checkout", "-b", "unrelated"]);
    std::fs::write(tmp.path().join("README.md"), "unrelated\n").expect("write unrelated");
    git(tmp.path(), &["commit", "-am", "unrelated"]);

    let worktree = create_worktree(tmp.path(), "agent/test-base", "agent", "scope", "main")
        .expect("worktree should be created from base");
    let head = Command::new("git")
        .args(["-C", worktree.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .expect("read worktree head");
    let base = Command::new("git")
        .args(["-C", tmp.path().to_str().unwrap(), "rev-parse", "main"])
        .output()
        .expect("read base head");
    assert_eq!(head.stdout, base.stdout);
}

#[test]
fn test_resolve_publish_remote_skips_local_clone_origin() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(
        tmp.path(),
        &["remote", "add", "origin", "/tmp/decapod-root-clone"],
    );
    git(
        tmp.path(),
        &[
            "remote",
            "add",
            "upstream",
            "git@github.com:DecapodLabs/decapod.git",
        ],
    );

    let remote = resolve_publish_remote(tmp.path()).expect("network remote");
    assert_eq!(
        remote,
        PublishRemote {
            name: "upstream".to_string(),
            url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        }
    );
}

#[test]
fn test_resolve_publish_remote_fails_closed_without_network_remote() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(
        tmp.path(),
        &["remote", "add", "origin", "/tmp/decapod-root-clone"],
    );

    let error = resolve_publish_remote(tmp.path()).expect_err("local remote must not publish");
    let message = error.to_string();
    assert!(message.contains("no network-capable git remote"));
    assert!(message.contains("No commit was pushed"));
}

#[test]
fn validation_artifact_publish_gate_requires_trajectory_and_receipt() {
    let tmp = tempdir().expect("tempdir");

    let error = verify_validation_artifacts_for_publish(tmp.path())
        .expect_err("publication must require the trajectory artifact");

    assert!(error.to_string().contains("missing trajectory cookie"));
}

#[test]
fn required_governance_artifacts_must_all_be_in_pr_diff() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);
    std::fs::write(tmp.path().join("README.md"), "base\n").expect("write base");
    git(tmp.path(), &["add", "README.md"]);
    git(tmp.path(), &["commit", "-m", "base"]);
    git(tmp.path(), &["checkout", "-q", "-b", "feature"]);

    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        if *path == ".decapod/governance/claims.json" {
            continue;
        }
        let artifact = tmp.path().join(path);
        std::fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("create artifact parent");
        std::fs::write(artifact, "{}\n").expect("write artifact");
    }
    git(tmp.path(), &["add", "."]);
    git(
        tmp.path(),
        &["commit", "-m", "proof artifacts without claims"],
    );

    let error = ensure_required_governance_artifacts_in_pr(tmp.path(), "master")
        .expect_err("publication must reject a PR missing claims.json");
    let message = error.to_string();
    assert!(
        message.contains(".decapod/governance/claims.json"),
        "{message}"
    );
    assert!(message.contains("all four"), "{message}");

    let claims = tmp.path().join(".decapod/governance/claims.json");
    std::fs::write(claims, "{}\n").expect("write claims");
    git(tmp.path(), &["add", ".decapod/governance/claims.json"]);
    git(
        tmp.path(),
        &["commit", "-m", "include claims proof artifact"],
    );
    ensure_required_governance_artifacts_in_pr(tmp.path(), "master")
        .expect("all four governance artifacts must be in the PR diff");
}

#[test]
fn test_github_repo_slug_supports_common_remote_forms() {
    assert_eq!(
        github_repo_slug("git@github.com:DecapodLabs/decapod.git"),
        Some("DecapodLabs/decapod".to_string())
    );
    assert_eq!(
        github_repo_slug("https://github.com/DecapodLabs/decapod.git"),
        Some("DecapodLabs/decapod".to_string())
    );
    assert_eq!(github_repo_slug("/tmp/decapod-root-clone"), None);
}

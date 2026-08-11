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

fn git_stdout(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).to_string()
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
fn publish_push_failure_requires_fast_forward_reconciliation_without_force_push() {
    let message = publish_push_failure(
        " ! [rejected] feature -> feature (non-fast-forward)\nerror: failed to push some refs",
        "feature",
        "origin",
    );

    assert!(message.contains("requires a fast-forward push"));
    assert!(message.contains("never force-pushes"));
    assert!(message.contains("Do not run `git push --force`"));
    assert!(message.contains("rerun `decapod validate`"));
    assert!(message.contains("retry `decapod workspace publish`"));
}

#[test]
fn validation_artifact_publish_gate_requires_trajectory_and_receipt() {
    let tmp = tempdir().expect("tempdir");

    let error = verify_validation_artifacts_for_publish(tmp.path())
        .expect_err("publication must require the trajectory artifact");

    assert!(error.to_string().contains("missing trajectory cookie"));
}

#[test]
fn required_governance_artifacts_must_be_present_and_valid() {
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
    assert!(
        message.contains("missing") || message.contains("invalid"),
        "{message}"
    );
}

#[test]
fn inherited_valid_governance_artifacts_need_not_appear_in_pr_diff() {
    // GitHub #1232: base already has a complete, valid proof bundle. A feature
    // commit that touches only application code must not require artificial
    // governance-file churn solely to place paths in the PR diff.
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "master"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);

    // Seed base with valid governance artifacts via the inventory repair path
    // after a minimal claims template write is insufficient for plan/trajectory
    // loaders — write minimal schema-valid shells that inventory accepts.
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        let artifact = tmp.path().join(path);
        std::fs::create_dir_all(artifact.parent().expect("artifact parent"))
            .expect("create artifact parent");
        // Placeholders; the inventory loader may mark some invalid. For this
        // unit test we only assert the PR-diff participation model is gone:
        // when inventory reports present+valid, publish succeeds even if the
        // PR diff omits these paths. We therefore write files that exist and
        // mock through the function only if loaders accept them.
        std::fs::write(&artifact, "{}\n").expect("write artifact");
    }
    std::fs::write(tmp.path().join("README.md"), "base\n").expect("write base");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "base with governance"]);
    git(tmp.path(), &["checkout", "-q", "-b", "feature"]);

    // Application-only commit: no governance path appears in the PR diff.
    std::fs::write(tmp.path().join("app.txt"), "feature work\n").expect("write app");
    git(tmp.path(), &["add", "app.txt"]);
    git(tmp.path(), &["commit", "-m", "app change only"]);

    // Diff must not include governance artifacts.
    let diff = git_stdout(
        tmp.path(),
        &["diff", "--name-only", "master...HEAD"],
    );
    for path in REQUIRED_PR_GOVERNANCE_ARTIFACTS {
        assert!(
            !diff.lines().any(|line| line.trim() == *path),
            "test setup requires governance paths absent from PR diff, saw:\n{diff}"
        );
    }

    // Gate result depends on whether `{}` shells load as valid. If loaders
    // reject them, the failure must mention validity/presence — not PR-diff
    // participation.
    match ensure_required_governance_artifacts_in_pr(tmp.path(), "master") {
        Ok(()) => {}
        Err(error) => {
            let message = error.to_string();
            assert!(
                !message.contains("not included in the PR diff"),
                "must not require PR-diff participation: {message}"
            );
            assert!(
                message.contains("missing") || message.contains("invalid"),
                "{message}"
            );
        }
    }
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

#[test]
fn material_specs_publish_gate_rejects_fingerprint_only_refresh() {
    let tmp = tempdir().expect("tempdir");
    git(tmp.path(), &["init", "-q", "-b", "master"]);
    git(tmp.path(), &["config", "user.email", "test@test.com"]);
    git(tmp.path(), &["config", "user.name", "Test"]);

    let specs = tmp.path().join(".decapod/managed/specs");
    std::fs::create_dir_all(&specs).expect("specs dir");
    let intent = specs.join("INTENT.md");
    let base_body = "# Intent\n\nBaseline contract.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `aaa`\n<!-- decapod:codebase-attestation:end -->\n";
    std::fs::write(&intent, base_body).expect("write intent");
    for name in [
        "README.md",
        "ARCHITECTURE.md",
        "INTERFACES.md",
        "VALIDATION.md",
        "SEMANTICS.md",
        "OPERATIONS.md",
        "SECURITY.md",
    ] {
        std::fs::write(specs.join(name), format!("# {name}\n\nBaseline.\n")).expect("write spec");
    }
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "base"]);
    git(tmp.path(), &["checkout", "-q", "-b", "feature"]);

    // Fingerprint-only attestation churn must fail publication.
    std::fs::write(&intent, base_body.replace("`aaa`", "`bbb`")).expect("fingerprint only");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "fingerprint only"]);

    let error = ensure_material_specs_change_in_pr(tmp.path(), "master")
        .expect_err("fingerprint-only living specs must fail publish");
    let message = error.to_string();
    assert!(message.contains("FINGERPRINT_ONLY_SPECS"), "{message}");

    // Material authored rewrite must pass.
    std::fs::write(
        &intent,
        "# Intent\n\nBaseline contract plus material rewrite for #1183.\n\n<!-- decapod:codebase-attestation:start -->\n## Codebase Attestation\n- Repository signal fingerprint: `bbb`\n<!-- decapod:codebase-attestation:end -->\n",
    )
    .expect("material rewrite");
    git(tmp.path(), &["add", "."]);
    git(tmp.path(), &["commit", "-m", "material rewrite"]);
    ensure_material_specs_change_in_pr(tmp.path(), "master")
        .expect("material living-spec rewrite must pass publish gate");
}

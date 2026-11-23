//! Both PR and pre-PR adapters must use this exact, read-only docs proof.
#[allow(dead_code)]
#[path = "../assets/tools/docs_sync.rs"]
mod docs_sync;

const WORKFLOW: &str = include_str!("../.github/workflows/docs_sync.yml");
const ACTION: &str = include_str!("../.github/actions/docs-drift/action.yml");
const RECIPE: &str = include_str!("../assets/tools/source_ci/docs-drift.sh");
const TOOL: &str = include_str!("../assets/tools/docs_sync.rs");

#[test]
fn pr_documentation_sync_uses_least_privilege_immutable_shared_proof() {
    for forbidden in [
        "contents: write",
        "create-github-app-token",
        "secrets.",
        "APP_TOKEN",
        "github.head_ref",
        "git fetch",
        "git commit",
        "git push",
        "git-auto-commit",
        "create-pull-request",
        "xargs",
        "ref:",
    ] {
        assert!(
            !WORKFLOW.contains(forbidden),
            "PR docs workflow contains {forbidden}"
        );
        assert!(
            !ACTION.contains(forbidden),
            "Shared docs action contains {forbidden}"
        );
    }
    assert!(WORKFLOW.contains("contents: read"));
    assert!(WORKFLOW.contains("uses: actions/checkout@v4"));
    assert!(WORKFLOW.contains("fetch-depth: 0"));
    assert!(WORKFLOW.contains("persist-credentials: false"));
    assert!(WORKFLOW.contains("uses: ./.github/actions/docs-drift"));
    assert!(WORKFLOW.contains("base-sha: ${{ steps.context.outputs.base }}"));
    assert!(WORKFLOW.contains("head-sha: ${{ steps.context.outputs.head }}"));
    assert!(
        !WORKFLOW.contains("cargo build"),
        "adapter must not fork shared implementation"
    );
    assert!(
        !WORKFLOW.contains("docs build"),
        "adapter must not fork shared implementation"
    );
    assert!(ACTION.contains("BASE_SHA: ${{ inputs.base-sha }}"));
    assert!(ACTION.contains("HEAD_SHA: ${{ inputs.head-sha }}"));
    assert!(ACTION.contains("run-ci --check docs-drift"));
    assert!(RECIPE.contains("cargo build --all-features"));
    assert!(RECIPE.contains("select --repo . --base \"$BASE_SHA\" --head \"$HEAD_SHA\""));
    assert!(RECIPE.contains(
        "verify --repo . --base \"$BASE_SHA\" --head \"$HEAD_SHA\" --decapod target/debug/decapod"
    ));
    assert!(
        RECIPE.find("select --repo").unwrap() < RECIPE.find("cargo build --all-features").unwrap()
    );
    assert!(
        RECIPE.find("cargo build --all-features").unwrap() < RECIPE.find("verify --repo").unwrap()
    );
    assert!(TOOL.contains("\"--no-renames\""));
    assert!(TOOL.contains("\"-z\""));
    assert!(TOOL.contains("command.arg(\"--touched\").args(paths)"));
    assert!(!TOOL.contains("Command::new(\"bash\")"));
    assert!(!TOOL.contains("Command::new(\"sh\")"));
    assert!(TOOL.contains("--cached"));
    assert!(TOOL.contains("decapod docs build"));
    assert!(TOOL.contains("decapod validate"));
    assert!(TOOL.contains("decapod govern artifacts checkpoint"));
    assert!(TOOL.contains("stage .decapod/governance.json"));
    assert!(!TOOL.contains("DECAPOD_VALIDATE_SKIP"));
}

#[test]
fn scheduled_docs_autosync_workflow_uses_governed_lifecycle_not_raw_pr_action() {
    let autosync = include_str!("../.github/workflows/docs_autosync.yml");
    let script = include_str!("../assets/tools/docs_autosync_governed.sh");
    assert!(
        !autosync.contains("peter-evans/create-pull-request"),
        "docs_autosync.yml must not delegate uncheckpointed commits to create-pull-request (#1388)"
    );
    assert!(
        !autosync.contains("pull_request:"),
        "docs_autosync.yml remains schedule/workflow_dispatch only"
    );
    assert!(autosync.contains("bash assets/tools/docs_autosync_governed.sh"));
    assert!(autosync.contains("docs-autosync-drift-report"));
    for required in [
        "govern artifacts begin-pr",
        "govern trajectory init",
        "govern trajectory record",
        "rpc --op specs.refresh",
        "govern artifacts checkpoint",
        "govern artifacts verify-checkpoints",
        "--publication-material",
    ] {
        assert!(
            script.contains(required),
            "governed docs_autosync script missing required step: {required}"
        );
    }
    assert!(
        !script.contains("git push --force") && !script.contains("git push -f"),
        "governed docs_autosync script must never force-push"
    );
}

#[cfg(unix)]
#[test]
fn scheduled_docs_autosync_governed_lifecycle_covers_all_six_scenarios() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    let raw_bin = Path::new(env!("CARGO_BIN_EXE_decapod"));
    let bin = fs::canonicalize(raw_bin).unwrap_or_else(|_| raw_bin.to_path_buf());
    let tmp = tempfile::tempdir().unwrap();
    // Materialize the governed script into the fixture scratch directory so
    // relocated Bazel integration shards never depend on repo-root layout.
    let script = tmp.path().join("docs_autosync_governed.sh");
    fs::write(
        &script,
        include_str!("../assets/tools/docs_autosync_governed.sh"),
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();

    let git = |dir: &Path, args: &[&str]| -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(args)
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    let bare = tmp.path().join("origin.git");
    let work = tmp.path().join("work");
    fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--bare", "-b", "master"]);
    git(
        tmp.path(),
        &["clone", bare.to_str().unwrap(), work.to_str().unwrap()],
    );
    git(&work, &["config", "user.name", "Test Bot"]);
    git(&work, &["config", "user.email", "bot@decapod.invalid"]);

    // Seed a real Decapod-initialized repo baseline on master so all governed
    // invariants (todo claim, workspace ensure, begin-pr, trajectory,
    // specs.refresh, checkpoint, verify-checkpoints --publication-material)
    // execute end-to-end against real CLI state.
    fs::create_dir_all(work.join("docs")).unwrap();
    fs::write(work.join("docs/guide.md"), "# Guide v1\n").unwrap();
    let init_out = Command::new(&bin)
        .current_dir(&work)
        .env("DECAPOD_AGENT_ID", "docs-autosync-bot")
        .env("DECAPOD_SESSION_PASSWORD", "docs-autosync-test-session")
        .args([
            "init",
            "--proof",
            "--no-ci",
            "--no-container-workspaces",
            "--isolation-mode",
            "worktree",
            "--product-name",
            "autosync-demo",
            "--product-summary",
            "Demo repo",
        ])
        .output()
        .expect("run decapod init in fixture");
    assert!(
        init_out.status.success(),
        "fixture decapod init failed: {}\n{}",
        String::from_utf8_lossy(&init_out.stdout),
        String::from_utf8_lossy(&init_out.stderr)
    );
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-qm", "chore: seed baseline"]);
    git(&work, &["push", "-u", "origin", "master"]);

    let run_sync = |build_cmd: &str, report: &Path, extra_env: &[(&str, &str)]| {
        let mut cmd = Command::new("bash");
        cmd.arg(&script)
            .current_dir(&work)
            .env("DECAPOD_BIN", &bin)
            .env("DECAPOD_AGENT_ID", "docs-autosync-bot")
            .env("DECAPOD_SESSION_PASSWORD", "docs-autosync-test-session")
            .env("DOCS_AUTOSYNC_BUILD_CMD", build_cmd)
            .env("DOCS_AUTOSYNC_REPORT_PATH", report)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        cmd.output().expect("run governed docs autosync")
    };

    // Scenario 1: no-drift run exits 0 and does not create docs-autosync branch.
    let report1 = tmp.path().join("r1.md");
    let out1 = run_sync("true", &report1, &[]);
    assert!(
        out1.status.success(),
        "no-drift failed: {}",
        String::from_utf8_lossy(&out1.stderr)
    );
    assert!(fs::read_to_string(&report1).unwrap().contains("no_drift"));
    assert!(
        !Command::new("git")
            .current_dir(&work)
            .args([
                "ls-remote",
                "--exit-code",
                "--heads",
                "origin",
                "docs-autosync"
            ])
            .status()
            .unwrap()
            .success(),
        "no-drift run must not push a branch"
    );

    // Scenario 2: first PR creation pushes a verified checkpointed commit on docs-autosync.
    let report2 = tmp.path().join("r2.md");
    let out2 = run_sync("printf '# Guide v2\\n' > docs/guide.md", &report2, &[]);
    assert!(
        out2.status.success(),
        "first PR failed: {}\n{}",
        String::from_utf8_lossy(&out2.stdout),
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(
        fs::read_to_string(&report2)
            .unwrap()
            .contains("verified_sync")
    );
    let first_tip = git(&work, &["rev-parse", "origin/docs-autosync"]);

    // Scenario 3: repeated update to the same open PR appends a second verified checkpointed commit.
    let report3 = tmp.path().join("r3.md");
    let out3 = run_sync("printf '# Guide v3\\n' > docs/guide.md", &report3, &[]);
    assert!(
        out3.status.success(),
        "repeated open PR update failed: {}\n{}",
        String::from_utf8_lossy(&out3.stdout),
        String::from_utf8_lossy(&out3.stderr)
    );
    let second_tip = git(&work, &["rev-parse", "origin/docs-autosync"]);
    assert_ne!(first_tip, second_tip);
    assert_eq!(
        git(&work, &["rev-parse", &format!("{second_tip}^")]),
        first_tip
    );

    // Scenario 4: accepted/merged PR reset starts a fresh governed PR identity from new master.
    git(&work, &["checkout", "-q", "master"]);
    git(
        &work,
        &[
            "merge",
            "--no-ff",
            "-q",
            "origin/docs-autosync",
            "-m",
            "Merge docs-autosync",
        ],
    );
    git(&work, &["push", "origin", "master", "-q"]);
    let report4 = tmp.path().join("r4.md");
    let out4 = run_sync("printf '# Guide v4\\n' > docs/guide.md", &report4, &[]);
    assert!(
        out4.status.success(),
        "reset after merge failed: {}\n{}",
        String::from_utf8_lossy(&out4.stdout),
        String::from_utf8_lossy(&out4.stderr)
    );
    let after_merge_tip = git(&work, &["rev-parse", "origin/docs-autosync"]);
    assert_eq!(
        git(&work, &["rev-parse", &format!("{after_merge_tip}^")]),
        git(&work, &["rev-parse", "origin/master"])
    );

    // Scenario 5: validation failure (bad binary) fails closed with actionable drift report and no push.
    let report5 = tmp.path().join("r5.md");
    let out5 = run_sync(
        "printf '# Guide v5\\n' > docs/guide.md",
        &report5,
        &[("DECAPOD_BIN", "/nonexistent/decapod")],
    );
    assert!(!out5.status.success(), "missing binary must fail closed");
    assert!(
        fs::read_to_string(&report5)
            .unwrap()
            .contains("failed_closed")
    );
    assert_eq!(
        git(&work, &["rev-parse", "origin/docs-autosync"]),
        after_merge_tip
    );

    // Scenario 6: concurrent branch movement / diverged master fails closed without force-push.
    git(&work, &["checkout", "-q", "master"]);
    fs::write(work.join("README.md"), "concurrent master change\n").unwrap();
    git(&work, &["add", "README.md"]);
    git(&work, &["commit", "-qm", "chore: advance master"]);
    git(&work, &["push", "origin", "master", "-q"]);
    // Now origin/docs-autosync (at after_merge_tip) is diverged from new origin/master.
    let report6 = tmp.path().join("r6.md");
    let out6 = run_sync("printf '# Guide v6\\n' > docs/guide.md", &report6, &[]);
    assert!(
        !out6.status.success(),
        "diverged branch must fail closed without force-push"
    );
    assert!(
        fs::read_to_string(&report6)
            .unwrap()
            .contains("failed_closed")
    );
    assert_eq!(
        git(&work, &["rev-parse", "origin/docs-autosync"]),
        after_merge_tip
    );
}

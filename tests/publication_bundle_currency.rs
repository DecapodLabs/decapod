//! Regression coverage for GitHub #1232: publication-bundle currency.
//!
//! Unchanged artifacts whose fingerprints/provenance still validate must pass
//! without artificial mutation. When the Decapod release advances past the base
//! pin, release-bound surfaces must be refreshed on the branch.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_decapod"));
    cmd.current_dir(dir).args(args);
    cmd.env_remove("DECAPOD_VALIDATE_SKIP_GIT_GATES");
    cmd.env_remove("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run decapod")
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn combined(out: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Green-field init, commit base, branch, then return worktree-like dir on the feature branch.
fn setup_feature_branch() -> (TempDir, PathBuf, String) {
    let tmp = TempDir::new().expect("tmpdir");
    let repo = tmp.path().to_path_buf();

    git(&repo, &["init", "-b", "master"]);
    git(&repo, &["config", "user.name", "Test User"]);
    git(&repo, &["config", "user.email", "test@example.com"]);

    let init = run_decapod(
        &repo,
        &["init", "--force", "--no-container-workspaces"],
        &[],
    );
    assert!(
        init.status.success(),
        "decapod init failed: {}",
        combined(&init)
    );

    // Ensure governance shells exist for currency presence checks.
    let _ = run_decapod(
        &repo,
        &["govern", "artifacts", "inventory", "--repair"],
        &[],
    );

    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);

    git(&repo, &["checkout", "-b", "agent/test/publication-bundle"]);

    let acquire = run_decapod(
        &repo,
        &["session", "acquire"],
        &[("DECAPOD_AGENT_ID", "unknown")],
    );
    assert!(
        acquire.status.success(),
        "session acquire failed: {}",
        combined(&acquire)
    );
    let password = String::from_utf8_lossy(&acquire.stdout)
        .lines()
        .find_map(|line| {
            line.strip_prefix("Password: ")
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_default();

    (tmp, repo, password)
}

#[test]
fn same_version_app_commit_does_not_require_bundle_diff_churn() {
    let (_tmp, dir, password) = setup_feature_branch();

    // Ordinary application change + material living-spec rewrite only.
    fs::write(dir.join("app-feature.txt"), "feature work\n").expect("write app");
    let intent = dir.join(".decapod/managed/specs/INTENT.md");
    let body = fs::read_to_string(&intent).expect("read intent");
    fs::write(
        &intent,
        format!(
            "{body}\n\n## Application feature note (#1232)\n\nMaterial rewrite for currency-gate regression.\n"
        ),
    )
    .expect("material intent rewrite");

    // Do NOT touch entrypoints, Dockerfile, governance files, or manifest.
    git(
        &dir,
        &["add", "app-feature.txt", ".decapod/managed/specs/INTENT.md"],
    );
    git(
        &dir,
        &["commit", "-m", "feat: app change with material intent only"],
    );

    // Second incremental commit also omits the bundle — must not re-require churn.
    fs::write(dir.join("app-feature-2.txt"), "more work\n").expect("write app 2");
    git(&dir, &["add", "app-feature-2.txt"]);
    git(&dir, &["commit", "-m", "feat: incremental app commit"]);

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            // Skip tooling/proof gates that need a full project build surface.
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
        ],
    );
    let text = combined(&validate);
    assert!(
        !text.contains("PER_COMMIT_PUBLICATION_BUNDLE"),
        "old per-commit participation gate must not fire: {text}"
    );
    // Currency gate may still fail for other reasons (e.g. missing validation
    // receipt integrity in a fresh fixture). It must not demand that every
    // commit list the full bundle.
    if text.contains("PUBLICATION_BUNDLE_CURRENCY") {
        assert!(
            !text.contains("each commit must carry"),
            "currency failures must not demand per-commit textual participation: {text}"
        );
        assert!(
            !text.contains("missing AGENTS.md")
                || text.contains("release advanced")
                || text.contains("missing .decapod/governance"),
            "same-version current entrypoints should not be reported as missing from commits: {text}"
        );
    }
}

#[test]
fn release_advance_requires_release_bound_refresh_on_branch() {
    let (_tmp, dir, password) = setup_feature_branch();

    // Simulate base pin at an older release by rewriting the committed AGENTS.md
    // marker on master, then branching a feature that only changes app code.
    // First move back to master and forge an older release pin in history.
    git(&dir, &["checkout", "master"]);
    let agents = dir.join("AGENTS.md");
    let agents_body = fs::read_to_string(&agents).expect("read agents");
    // Replace the release marker with a deliberately stale version string while
    // leaving the rest of the file intact enough for git history purposes.
    let stale = agents_body.replacen(
        &format!("<!-- decapod-release: {} -->", env!("CARGO_PKG_VERSION")),
        "<!-- decapod-release: 0.0.0-stale -->",
        1,
    );
    assert_ne!(stale, agents_body, "fixture must alter the release marker");
    fs::write(&agents, &stale).expect("write stale agents");
    git(&dir, &["add", "AGENTS.md"]);
    git(
        &dir,
        &["commit", "-m", "chore: simulate older base release pin"],
    );

    git(&dir, &["checkout", "-b", "agent/test/release-advance"]);
    fs::write(dir.join("only-app.txt"), "app\n").expect("write app");
    // Material living-spec rewrite so material gate is not the failure mode.
    let intent = dir.join(".decapod/managed/specs/INTENT.md");
    let body = fs::read_to_string(&intent).unwrap_or_default();
    fs::write(
        &intent,
        format!("{body}\n\n## Release advance note\n\nMaterial rewrite.\n"),
    )
    .expect("intent");
    git(
        &dir,
        &["add", "only-app.txt", ".decapod/managed/specs/INTENT.md"],
    );
    git(&dir, &["commit", "-m", "feat: app without release refresh"]);

    // Working tree still has stale AGENTS from base; do not heal before validate.
    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
            // Keep fingerprint gates on so release-bound currency is evaluated.
        ],
    );
    let text = combined(&validate);
    // Either PUBLICATION_BUNDLE_CURRENCY (release advanced without refresh) or
    // the entrypoint integrity gate must block publication.
    assert!(
        !validate.status.success()
            || text.contains("PUBLICATION_BUNDLE_CURRENCY")
            || text.contains("entrypoint_release_mismatch")
            || text.contains("release advanced")
            || text.contains("STALE_ENTRYPOINT"),
        "release advance without refresh must fail validation, got success with: {text}"
    );
}

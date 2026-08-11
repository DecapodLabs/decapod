//! Composition-level regression for GitHub #1232 publication-bundle model.
//!
//! These tests exercise the full binary where unit tests already pin the
//! PUBLICATION_BUNDLE_CURRENCY gate predicate. Focus here is sibling-gate
//! composition: material living-spec invalidation and multi-commit history.

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

    let _ = run_decapod(
        &repo,
        &["govern", "artifacts", "inventory", "--repair"],
        &[],
    );

    // Significant path so STALE_SPECS / repo_signal composition is exercisable.
    fs::create_dir_all(repo.join("src")).expect("src");
    fs::write(repo.join("src/lib.rs"), "pub fn f() {}\n").expect("lib");

    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);

    // Validate treats paths under `.decapod/workspaces/` as isolated agent
    // workspaces; put the feature branch there so workspace protection does not
    // short-circuit before publication/spec gates run.
    let worktree = repo
        .join(".decapod")
        .join("workspaces")
        .join("test-publication-bundle");
    fs::create_dir_all(worktree.parent().unwrap()).expect("workspaces parent");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "agent/test/publication-bundle",
            worktree.to_str().expect("utf8 path"),
            "HEAD",
        ],
    );

    let acquire = run_decapod(
        &worktree,
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

    (tmp, worktree, password)
}

/// A/B/G (integration): multi-commit app history without full-bundle path
/// participation must not resurrect PER_COMMIT_PUBLICATION_BUNDLE.
#[test]
fn multi_commit_history_does_not_require_full_bundle_participation() {
    let (_tmp, dir, password) = setup_feature_branch();

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

    git(
        &dir,
        &["add", "app-feature.txt", ".decapod/managed/specs/INTENT.md"],
    );
    git(
        &dir,
        &["commit", "-m", "feat: app change with material intent only"],
    );

    fs::write(dir.join("app-feature-2.txt"), "more work\n").expect("write app 2");
    git(&dir, &["add", "app-feature-2.txt"]);
    git(&dir, &["commit", "-m", "feat: incremental app commit"]);

    let changed = Command::new("git")
        .current_dir(&dir)
        .args(["log", "master..HEAD", "--name-only", "--pretty=format:"])
        .output()
        .expect("git log");
    let names = String::from_utf8_lossy(&changed.stdout);
    for forbidden in [
        "AGENTS.md",
        "CLAUDE.md",
        "CODEX.md",
        "GEMINI.md",
        ".decapod/managed/Dockerfile.decapod",
    ] {
        assert!(
            !names.lines().any(|l| l.trim() == forbidden),
            "commits must not require {forbidden} churn; log paths:\n{names}"
        );
    }

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
        ],
    );
    let text = combined(&validate);
    assert!(
        !text.contains("PER_COMMIT_PUBLICATION_BUNDLE"),
        "old per-commit participation gate must not fire: {text}"
    );
    assert!(
        !text.contains("each commit must carry"),
        "must not demand per-commit textual participation: {text}"
    );
}

/// D (composition): governed `src/**` change without material living-spec rewrite
/// must fail (material mutation / FINGERPRINT_ONLY_SPECS), even though default
/// validate auto-refreshes attestation fingerprints.
#[test]
fn src_change_without_material_living_spec_rewrite_fails() {
    let (_tmp, dir, password) = setup_feature_branch();

    fs::write(dir.join("src/lib.rs"), "pub fn f() { let _x = 1; }\n").expect("code change");
    git(&dir, &["add", "src/lib.rs"]);
    git(
        &dir,
        &["commit", "-m", "feat: change governed src without specs"],
    );

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
        ],
    );
    let text = combined(&validate);
    assert!(
        !validate.status.success(),
        "src change without material living-spec rewrite must fail validate; got success: {text}"
    );
    assert!(
        text.contains("FINGERPRINT_ONLY_SPECS")
            || text.contains("material")
            || text.contains("Living Specs Material"),
        "expected material living-spec / fingerprint-only failure, got: {text}"
    );
}

/// E (composition): after material rewrite, the material-mutation sibling allows
/// the PR delta (auto-refresh handles attestation). Still must not require
/// release-bound path churn on the same version.
#[test]
fn src_change_with_material_rewrite_does_not_require_entrypoint_churn() {
    let (_tmp, dir, password) = setup_feature_branch();

    fs::write(dir.join("src/lib.rs"), "pub fn f() { let _x = 2; }\n").expect("code");
    let intent = dir.join(".decapod/managed/specs/INTENT.md");
    let body = fs::read_to_string(&intent).expect("intent");
    fs::write(
        &intent,
        format!("{body}\n\n## Material rewrite after src change (#1232)\n"),
    )
    .expect("material");
    git(
        &dir,
        &["add", "src/lib.rs", ".decapod/managed/specs/INTENT.md"],
    );
    git(&dir, &["commit", "-m", "feat: src + material intent"]);

    let show = Command::new("git")
        .current_dir(&dir)
        .args(["show", "--name-only", "--pretty=format:", "HEAD"])
        .output()
        .expect("git show");
    let names = String::from_utf8_lossy(&show.stdout);
    assert!(
        !names.lines().any(|l| l.trim() == "AGENTS.md"),
        "material+src commit must not force AGENTS.md churn"
    );

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
        ],
    );
    let text = combined(&validate);
    assert!(
        !text.contains("PER_COMMIT_PUBLICATION_BUNDLE"),
        "must not revive per-commit participation: {text}"
    );
    // Material gate must not be the failure mode when a rewrite is present.
    assert!(
        !text.contains("FINGERPRINT_ONLY_SPECS"),
        "material rewrite should satisfy living-spec material gate: {text}"
    );
}

/// Committing a non-canonical entrypoint pin (not byte-identical to
/// `render_entrypoint`) hard-fails ENTRYPOINT_COMMIT_DISCIPLINE.
#[test]
fn non_canonical_entrypoint_commit_fails_discipline() {
    let (_tmp, dir, password) = setup_feature_branch();

    let agents = dir.join("AGENTS.md");
    let agents_body = fs::read_to_string(&agents).expect("read agents");
    let stale = agents_body.replacen(
        &format!("<!-- decapod-release: {} -->", env!("CARGO_PKG_VERSION")),
        "<!-- decapod-release: 0.0.0-stale -->",
        1,
    );
    assert_ne!(stale, agents_body, "fixture must alter the release marker");
    fs::write(&agents, &stale).expect("write stale agents");
    let intent = dir.join(".decapod/managed/specs/INTENT.md");
    let body = fs::read_to_string(&intent).unwrap_or_default();
    fs::write(
        &intent,
        format!("{body}\n\n## Discipline note\n\nMaterial rewrite.\n"),
    )
    .expect("intent");
    git(
        &dir,
        &["add", "AGENTS.md", ".decapod/managed/specs/INTENT.md"],
    );
    git(
        &dir,
        &["commit", "-m", "feat: non-canonical entrypoint pin"],
    );

    let validate = run_decapod(
        &dir,
        &["validate"],
        &[
            ("DECAPOD_AGENT_ID", "unknown"),
            ("DECAPOD_SESSION_PASSWORD", &password),
            ("DECAPOD_CONTAINER", "1"),
            ("DECAPOD_VALIDATE_SKIP_TOOLING_GATES", "1"),
        ],
    );
    let text = combined(&validate);
    assert!(
        !validate.status.success(),
        "non-canonical entrypoint commit must fail validate"
    );
    assert!(
        text.contains("ENTRYPOINT_COMMIT_DISCIPLINE"),
        "expected ENTRYPOINT_COMMIT_DISCIPLINE, got: {text}"
    );
}

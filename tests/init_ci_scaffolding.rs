use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn run_decapod(dir: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run decapod")
}

#[test]
fn init_scaffolds_github_action_workflow() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "--proof"]);
    assert!(
        out.status.success(),
        "decapod init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let workflow_path = tmp.path().join(".github/workflows/decapod-validate.yml");
    assert!(
        workflow_path.exists(),
        "expected .github/workflows/decapod-validate.yml to exist"
    );

    let content = fs::read_to_string(workflow_path).expect("read workflow file");
    assert!(content.contains("name: Decapod Validate"));
    assert!(content.contains("decapod validate"));
    // Bootstrap only when .decapod is missing; --force rewrites living specs and
    // invents drift against the PR drift gate.
    assert!(content.contains("decapod init --proof"));
    assert!(
        !content.contains("decapod init --proof --force"),
        "scaffold must not force-init in CI (rewrites living specs)"
    );
    assert!(content.contains("uses: cargo-bins/cargo-binstall@main"));
    assert!(content.contains("cargo binstall --no-confirm decapod"));
    assert!(
        !content.contains("cargo install --path ."),
        "project validation must install the published Decapod binary instead of compiling the checked-out project"
    );
    assert!(content.contains("~/.cargo/bin/cargo-binstall"));
    assert!(content.contains("~/.cargo/bin/decapod"));
    assert!(content.contains("steps.cache_decapod_tools.outputs.cache-hit"));
    assert!(content.contains("hashFiles('AGENTS.md')"));
    assert!(content.contains("DECAPOD_VALIDATE_SKIP_GIT_GATES: 1"));
    assert!(
        content.contains("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES"),
        "scaffolded workflow must skip fingerprint hard-fails on post-merge push"
    );
    assert!(
        content.contains("if: github.event_name == 'pull_request'"),
        "scaffolded drift gate must be PR-only"
    );
    assert!(content.contains("on:"));
    assert!(content.contains("push:"));
    assert!(content.contains("pull_request:"));
}

#[test]
fn init_force_updates_existing_workflow() {
    let tmp = tempdir().expect("tempdir");
    let workflow_dir = tmp.path().join(".github/workflows");
    fs::create_dir_all(&workflow_dir).expect("create workflow dir");
    let workflow_path = workflow_dir.join("decapod-validate.yml");
    fs::write(&workflow_path, "old content").expect("write old content");

    // Without --force, it should not overwrite (but decapod init might not fail if it's just one file)
    // Actually decapod init --proof might skip it or fail.
    // Let's check if --force overwrites it.
    let out = run_decapod(tmp.path(), &["init", "--proof", "--force"]);
    assert!(
        out.status.success(),
        "decapod init --force failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let content = fs::read_to_string(workflow_path).expect("read workflow file");
    assert!(
        content.contains("name: Decapod Validate"),
        "workflow should be updated with --force"
    );
}

#[test]
fn init_force_preserves_existing_living_specs() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "--proof"]);
    assert!(out.status.success(), "initial init should succeed");

    let specs_path = tmp.path().join(".decapod/managed/specs/README.md");
    let mut authored = fs::read_to_string(&specs_path).expect("read generated README");
    authored.push_str("\nProject-authored contract.\n");
    fs::write(&specs_path, &authored).expect("write authored README");

    let out = run_decapod(tmp.path(), &["init", "--proof", "--force"]);
    assert!(
        out.status.success(),
        "force init should preserve living specs: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        fs::read_to_string(specs_path).expect("read preserved README"),
        authored
    );
}

#[test]
fn init_no_ci_skips_workflow() {
    let tmp = tempdir().expect("tempdir");
    let out = run_decapod(tmp.path(), &["init", "--proof", "--no-ci"]);
    assert!(
        out.status.success(),
        "decapod init --no-ci failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let workflow_path = tmp.path().join(".github/workflows/decapod-validate.yml");
    assert!(
        !workflow_path.exists(),
        "expected .github/workflows/decapod-validate.yml NOT to exist with --no-ci"
    );
}

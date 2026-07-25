use decapod::core::{governance_artifacts, research_claims};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn run_decapod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run decapod")
}

fn run_git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run git")
}

#[test]
fn proof_init_creates_and_refresh_preserves_claims_ledger() {
    let temp = TempDir::new().expect("tempdir");
    let first = run_decapod(
        temp.path(),
        &["init", "--proof", "--no-container-workspaces"],
    );
    assert!(first.status.success(), "initialization failed");
    let claims_path = temp.path().join(research_claims::CLAIMS_PATH);
    let original = fs::read_to_string(&claims_path).expect("claims template");
    assert!(original.contains("replace-template-claim"));
    research_claims::load_and_validate(temp.path()).expect("template should validate");

    let project_claims = original.replace(
        "Project Falsifiable Research Claims",
        "Project-Specific Claims",
    );
    fs::write(&claims_path, &project_claims).expect("write project claims");
    let refresh = run_decapod(
        temp.path(),
        &["init", "--proof", "--force", "--no-container-workspaces"],
    );
    assert!(refresh.status.success(), "refresh failed");
    assert_eq!(
        fs::read_to_string(&claims_path).expect("preserved claims"),
        project_claims
    );
}

#[test]
fn inventory_distinguishes_health_claims_and_reports_pr_diff() {
    let temp = TempDir::new().expect("tempdir");
    let init = run_decapod(
        temp.path(),
        &["init", "--proof", "--no-container-workspaces"],
    );
    assert!(init.status.success(), "initialization failed");
    let git_name = run_git(temp.path(), &["config", "user.name", "test"]);
    assert!(git_name.status.success());
    let git_email = run_git(temp.path(), &["config", "user.email", "test@example.com"]);
    assert!(git_email.status.success());
    assert!(run_git(temp.path(), &["add", "."]).status.success());
    assert!(
        run_git(temp.path(), &["commit", "-m", "fixture"])
            .status
            .success()
    );

    let inventory = governance_artifacts::inventory(temp.path(), Some("master"), false)
        .expect("inventory should be deterministic");
    let claims = inventory
        .artifacts
        .iter()
        .find(|artifact| artifact.path == research_claims::CLAIMS_PATH)
        .expect("claims entry");
    assert!(claims.present);
    assert!(claims.valid);
    assert!(!claims.in_pr_diff);
    assert!(inventory.claims_source.contains("health.db"));
    assert!(!inventory.all_in_pr_diff);
}

#[test]
fn repair_creates_missing_claims_without_overwriting_other_artifacts() {
    let temp = TempDir::new().expect("tempdir");
    let init = run_decapod(
        temp.path(),
        &["init", "--proof", "--no-container-workspaces"],
    );
    assert!(init.status.success(), "initialization failed");
    let claims_path = temp.path().join(research_claims::CLAIMS_PATH);
    fs::remove_file(&claims_path).expect("remove legacy-missing claims");
    let marker = temp.path().join("project-marker.txt");
    fs::write(&marker, "preserve").expect("marker");
    let repaired =
        governance_artifacts::inventory(temp.path(), None, true).expect("repair inventory");
    assert!(
        repaired
            .artifacts
            .iter()
            .any(|artifact| artifact.path == research_claims::CLAIMS_PATH && artifact.valid)
    );
    assert_eq!(
        fs::read_to_string(marker).expect("marker preserved"),
        "preserve"
    );
}

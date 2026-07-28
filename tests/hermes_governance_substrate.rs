use decapod::core::atomic;
use decapod::core::dirty_classification::{self, DirtyFileClass};
use decapod::core::governance_artifacts::{self, WorkspaceTargetState};
use decapod::core::workspace;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repo() -> (TempDir, PathBuf) {
    let temp = TempDir::new().expect("tempdir");
    let dir = temp.path().to_path_buf();
    git(&dir, &["init", "-b", "master"]);
    git(&dir, &["config", "user.name", "Hermes Test"]);
    git(&dir, &["config", "user.email", "hermes@example.test"]);
    (temp, dir)
}

#[test]
fn dirty_classification_is_deterministic_and_groups_governance_files() {
    let (_temp, dir) = repo();
    fs::create_dir_all(dir.join(".decapod/governance")).expect("governance directory");
    fs::create_dir_all(dir.join(".decapod/managed/specs")).expect("spec directory");
    fs::create_dir_all(dir.join(".decapod/data/sessions")).expect("session directory");
    fs::write(dir.join("user.rs"), "fn main() {}\n").expect("user file");
    fs::write(dir.join(".decapod/governance/plan.json"), "{}\n").expect("plan");
    fs::write(dir.join(".decapod/managed/specs/INTENT.md"), "projection\n").expect("projection");
    fs::write(
        dir.join(".decapod/data/sessions/runtime.json"),
        "ephemeral\n",
    )
    .expect("session");

    let report = dirty_classification::classify(&dir, 1).expect("classification");
    assert!(
        !report.blocked,
        "classified files should not block the user limit"
    );
    assert_eq!(report.files[0].path, ".decapod/data/sessions/runtime.json");
    assert_eq!(
        dirty_classification::classify_path(".decapod/governance/plan.json", &[]),
        DirtyFileClass::GovernanceTracked
    );
    assert_eq!(
        dirty_classification::classify_path(".decapod/managed/specs/INTENT.md", &[]),
        DirtyFileClass::DeterministicProjection
    );
    assert_eq!(
        dirty_classification::classify_path(".decapod/unknown.json", &[]),
        DirtyFileClass::Unknown
    );
    let first = serde_json::to_string(&report).expect("stable JSON");
    let second = serde_json::to_string(&dirty_classification::classify(&dir, 1).unwrap())
        .expect("stable JSON");
    assert_eq!(first, second);
}

#[test]
fn clean_and_pre_existing_classification_are_explicit() {
    let (_temp, dir) = repo();
    let clean = dirty_classification::classify(&dir, 6).expect("clean classification");
    assert!(clean.files.is_empty());
    assert!(!clean.blocked);
    fs::write(dir.join("old.txt"), "pre-existing\n").expect("old file");
    let report =
        dirty_classification::classify_with_pre_existing(&dir, 0, &["old.txt".to_string()])
            .expect("pre-existing classification");
    assert_eq!(report.files[0].class, DirtyFileClass::PreExistingUnrelated);
    assert!(!report.blocked);
}

#[test]
fn inventory_reports_workspace_target_divergence_missing_base_and_does_not_mutate() {
    let (_temp, dir) = repo();
    fs::create_dir_all(dir.join(".decapod/governance")).expect("governance");
    fs::write(
        dir.join(".decapod/governance/plan.json"),
        "{\n  \"schema_version\": \"1.0.0\"\n}\n",
    )
    .expect("plan");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "fixture"]);
    fs::write(
        dir.join(".decapod/governance/plan.json"),
        "{\n  \"schema_version\": \"1.0.0\",\n  \"title\": \"changed\"\n}\n",
    )
    .expect("divergent plan");
    let before = Command::new("git")
        .current_dir(&dir)
        .args(["status", "--porcelain"])
        .output()
        .expect("status")
        .stdout;
    let report = governance_artifacts::inventory(&dir, Some("missing-target"), false)
        .expect("read-only inventory");
    let plan = report
        .artifacts
        .iter()
        .find(|artifact| artifact.path == ".decapod/governance/plan.json")
        .expect("plan artifact");
    assert_eq!(plan.workspace_target_state, WorkspaceTargetState::Divergent);
    assert!(plan.schema_error.is_some());
    assert_eq!(report.base_ref, None);
    assert!(!report.all_in_pr_diff);
    let after = Command::new("git")
        .current_dir(&dir)
        .args(["status", "--porcelain"])
        .output()
        .expect("status")
        .stdout;
    assert_eq!(
        before, after,
        "inventory must never mutate repository state"
    );
}

#[test]
fn protected_root_status_is_a_blocker_and_json_has_stable_shape() {
    let (_temp, dir) = repo();
    fs::write(dir.join("README.md"), "fixture\n").expect("readme");
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "fixture"]);
    let status = workspace::get_workspace_status(&dir).expect("workspace status");
    assert!(status.git.is_protected);
    assert!(!status.can_work);

    let report = dirty_classification::classify(&dir, 6).expect("classification");
    let json = serde_json::to_string_pretty(&report).expect("JSON");
    assert!(json.contains("\"schema_version\""));
    assert!(json.contains("\"blocker_classes\""));
    assert!(!json.contains("token"));
    assert!(!json.contains("Authorization"));
}

#[test]
fn atomic_writer_replaces_complete_content_without_leftover_temp_files() {
    let (_temp, dir) = repo();
    let path = dir.join(".decapod/governance/receipt.json");
    atomic::write_atomic(&path, b"{\"status\":\"ok\"}\n").expect("atomic write");
    assert_eq!(fs::read_to_string(&path).unwrap(), "{\"status\":\"ok\"}\n");
    let leftovers = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|item| {
            item.file_name()
                .to_string_lossy()
                .contains(".receipt.json.tmp-")
        })
        .count();
    assert_eq!(leftovers, 0);
}

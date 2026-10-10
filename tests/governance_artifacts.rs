use decapod::core::{governance_artifacts, governance_document, research_claims};
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

fn initialize() -> TempDir {
    let temp = TempDir::new().expect("tempdir");
    let init = run_decapod(
        temp.path(),
        &["init", "--proof", "--no-container-workspaces"],
    );
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    temp
}

fn commit_baseline(root: &Path) {
    assert!(
        run_git(root, &["config", "user.name", "test"])
            .status
            .success()
    );
    assert!(
        run_git(root, &["config", "user.email", "test@example.com"])
            .status
            .success()
    );
    assert!(run_git(root, &["add", "."]).status.success());
    let commit = run_git(root, &["commit", "-m", "fixture baseline"]);
    assert!(
        commit.status.success(),
        "{}",
        String::from_utf8_lossy(&commit.stderr)
    );
    assert!(run_git(root, &["branch", "-M", "master"]).status.success());
}

#[test]
fn proof_init_creates_empty_claims_and_refresh_preserves_current_pr_claims() {
    let temp = initialize();
    let original = governance_document::read_section(temp.path(), "claims")
        .expect("read claims")
        .expect("claims section");
    assert_eq!(
        original,
        serde_json::json!({}),
        "init must not invent a global claim catalog"
    );
    assert!(!temp.path().join(".decapod/governance/claims.json").exists());
    commit_baseline(temp.path());
    governance_document::begin_pr(temp.path(), "claims-refresh", "master").unwrap();
    governance_document::record_claim(
        temp.path(),
        "project-claim",
        "Refresh preserves project claims",
        "A refresh erases or rewrites a claim",
        "open",
        vec![],
    )
    .unwrap();
    let project_claims = governance_document::read_section(temp.path(), "claims").unwrap();
    let refresh = run_decapod(
        temp.path(),
        &["init", "--proof", "--force", "--no-container-workspaces"],
    );
    assert!(
        refresh.status.success(),
        "{}",
        String::from_utf8_lossy(&refresh.stderr)
    );
    assert_eq!(
        governance_document::read_section(temp.path(), "claims").unwrap(),
        project_claims
    );
    assert_eq!(
        governance_document::load(temp.path())
            .unwrap()
            .unwrap()
            .change
            .unwrap()
            .id,
        "claims-refresh"
    );
    research_claims::load_and_validate(temp.path()).expect("preserved claims validate");
}

#[test]
fn claims_note_update_requires_pr_identity_and_is_atomic_semantic_and_idempotent() {
    let temp = initialize();
    let path = temp.path().join(governance_document::GOVERNANCE_PATH);
    let original = fs::read(&path).unwrap();
    let note = "Hermes governance substrate test note.";
    assert!(research_claims::append_change_note(temp.path(), note).is_err());
    assert_eq!(
        fs::read(&path).unwrap(),
        original,
        "missing PR identity must not mutate evidence"
    );
    commit_baseline(temp.path());
    governance_document::begin_pr(temp.path(), "claims-note", "master").unwrap();
    assert!(run_git(temp.path(), &["add", "."]).status.success());
    let before = governance_document::read_section(temp.path(), "claims").unwrap();
    assert!(research_claims::append_change_note(temp.path(), note).expect("append note"));
    let updated = fs::read(&path).unwrap();
    assert!(!research_claims::append_change_note(temp.path(), note).expect("idempotent note"));
    assert_eq!(fs::read(&path).unwrap(), updated);
    let document = governance_document::load(temp.path()).unwrap().unwrap();
    assert_eq!(document.checkpoints.len(), 1);
    assert_eq!(document.checkpoints[0].summary, note);
    assert_eq!(
        governance_document::read_section(temp.path(), "claims").unwrap(),
        before,
        "a note records work, it must not invent a research claim"
    );
    research_claims::load_and_validate(temp.path()).expect("updated claims remain valid");
}

#[test]
fn legacy_claims_compaction_is_explicit_semantic_and_idempotent() {
    let temp = TempDir::new().unwrap();
    let legacy_dir = temp.path().join(".decapod/governance");
    fs::create_dir_all(&legacy_dir).unwrap();
    let legacy = include_str!("../assets/templates/claims.json");
    fs::write(legacy_dir.join("claims.json"), legacy).unwrap();
    let expected: serde_json::Value = serde_json::from_str(legacy).unwrap();
    assert!(research_claims::compact(temp.path()).expect("explicit migration/compaction"));
    let path = temp.path().join(governance_document::GOVERNANCE_PATH);
    let compacted = fs::read(&path).unwrap();
    assert_eq!(
        governance_document::read_section(temp.path(), "claims")
            .unwrap()
            .unwrap(),
        expected
    );
    assert!(
        !legacy_dir.join("claims.json").exists(),
        "legacy authority must be retired after successful migration"
    );
    assert!(!research_claims::compact(temp.path()).expect("idempotent compaction"));
    assert_eq!(
        fs::read(path).unwrap(),
        compacted,
        "repeat compaction preserves canonical bytes"
    );
    research_claims::load_and_validate(temp.path()).expect("legacy claim semantics remain valid");
}

#[test]
fn inventory_distinguishes_health_claims_and_reports_pr_diff() {
    let temp = initialize();
    commit_baseline(temp.path());
    let inventory = governance_artifacts::inventory(temp.path(), Some("master"), false)
        .expect("inventory should be deterministic");
    let claims = inventory
        .artifacts
        .iter()
        .find(|artifact| artifact.role.contains("research claims"))
        .expect("logical claims entry");
    assert!(claims.present);
    assert!(claims.valid);
    assert!(!claims.in_pr_diff);
    assert!(inventory.claims_source.contains("decapod.db"));
    assert!(!inventory.claims_source.contains("health.db"));
    assert!(inventory.claims_ledger_bytes.is_some());
    assert!(!inventory.all_in_pr_diff);
}

#[test]
fn repair_initializes_empty_claims_without_overwriting_existing_evidence() {
    let temp = TempDir::new().unwrap();
    assert!(
        run_git(temp.path(), &["init", "-b", "master"])
            .status
            .success()
    );
    let marker = temp.path().join("project-marker.txt");
    fs::write(&marker, "preserve").unwrap();
    let repaired =
        governance_artifacts::inventory(temp.path(), None, true).expect("repair inventory");
    assert!(
        repaired
            .artifacts
            .iter()
            .any(|artifact| artifact.role.contains("research claims") && artifact.valid)
    );
    assert_eq!(fs::read_to_string(marker).unwrap(), "preserve");
    assert_eq!(
        governance_document::read_section(temp.path(), "claims")
            .unwrap()
            .unwrap(),
        serde_json::json!({})
    );
    let path = temp.path().join(governance_document::GOVERNANCE_PATH);
    let before = fs::read(&path).unwrap();
    governance_artifacts::inventory(temp.path(), None, true).expect("repeat repair");
    assert_eq!(
        fs::read(path).unwrap(),
        before,
        "repair must not overwrite existing evidence"
    );
}

#[test]
fn inventory_remediation_follows_logical_sections_not_shared_physical_path() {
    let temp = initialize();
    let inventory = governance_artifacts::inventory(temp.path(), None, false).unwrap();
    let plan = inventory
        .artifacts
        .iter()
        .find(|entry| entry.role.contains("phase plan"))
        .unwrap();
    let claims = inventory
        .artifacts
        .iter()
        .find(|entry| entry.role.contains("research claims"))
        .unwrap();
    let trajectory = inventory
        .artifacts
        .iter()
        .find(|entry| entry.role.contains("agent-run trajectory"))
        .unwrap();
    let validation = inventory
        .artifacts
        .iter()
        .find(|entry| entry.role.contains("validation receipt"))
        .unwrap();
    assert_eq!(plan.path, ".decapod/governance.json#/sections/plan");
    assert_eq!(claims.path, ".decapod/governance.json#/claims");
    assert_eq!(
        trajectory.path,
        ".decapod/governance.json#/sections/trajectory"
    );
    assert_eq!(
        validation.path,
        ".decapod/governance.json#/sections/validation"
    );
    assert!(plan.remediation.contains("govern plan"));
    assert!(claims.remediation.contains("govern artifacts claim"));
    assert!(trajectory.remediation.contains("govern trajectory"));
    assert!(validation.remediation.contains("decapod validate"));
    for entry in [plan, claims, trajectory] {
        assert!(
            !entry
                .freshness_reasons
                .iter()
                .any(|reason| reason == "receipt_must_bind_current_trajectory")
        );
    }
    assert!(
        validation
            .freshness_reasons
            .iter()
            .any(|reason| reason == "receipt_must_bind_current_trajectory")
    );
}

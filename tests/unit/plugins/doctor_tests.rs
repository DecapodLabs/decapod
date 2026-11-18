// Moved from src/decapod/plugins/doctor.rs
use super::*;
use tempfile::tempdir;

#[test]
fn test_check_version() {
    let result = check_version();
    assert_eq!(result.status, CheckStatus::Pass);
    assert!(result.message.starts_with("decapod v"));
}

#[test]
fn test_check_decapod_dir_missing() {
    let tmp = tempdir().unwrap();
    let result = check_decapod_dir(tmp.path());
    assert_eq!(result.status, CheckStatus::Fail);
}

#[test]
fn test_check_decapod_dir_present() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".decapod/data")).unwrap();
    let result = check_decapod_dir(tmp.path());
    assert_eq!(result.status, CheckStatus::Pass);
}

#[test]
fn test_check_required_files() {
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("AGENTS.md"), "# Agents").unwrap();
    let results = check_required_files(tmp.path());
    assert_eq!(results[0].status, CheckStatus::Pass); // AGENTS.md present
    assert_eq!(results[1].status, CheckStatus::Fail); // CLAUDE.md missing
}

#[test]
fn test_run_preflight_checks_resolves_worktree_store_root_and_reports_migrations() {
    let workspace = tempdir().unwrap();
    let governance = tempdir().unwrap();
    let decapod_root = governance.path().join(".decapod");
    let store_root = decapod_root.join("data");
    std::fs::create_dir_all(&store_root).unwrap();
    crate::core::migration::check_and_migrate(&decapod_root).unwrap();

    std::fs::create_dir_all(workspace.path().join(".decapod")).unwrap();
    std::fs::write(workspace.path().join("AGENTS.md"), "# Agents").unwrap();
    std::fs::write(workspace.path().join("CLAUDE.md"), "# Claude").unwrap();
    let git = std::process::Command::new("git")
        .current_dir(workspace.path())
        .args(["init", "-b", "master"])
        .output()
        .unwrap();
    assert!(git.status.success());

    let store = Store {
        kind: crate::core::store::StoreKind::Repo,
        root: store_root,
    };
    let report = run_preflight_checks(&store, workspace.path()).unwrap();
    assert_eq!(report.failed, 0, "unexpected failures: {:?}", report.checks);
    assert!(
        !workspace.path().join(".decapod/data").exists(),
        "doctor must not materialize local data dir when governance store is external"
    );

    let decapod_check = report
        .checks
        .iter()
        .find(|c| c.name == ".decapod")
        .expect(".decapod check present");
    assert_eq!(decapod_check.status, CheckStatus::Pass);

    let migrations_check = report
        .checks
        .iter()
        .find(|c| c.name == "Migrations")
        .expect("Migrations check present");
    assert_eq!(migrations_check.status, CheckStatus::Pass);
    assert!(migrations_check.message.contains("Up to date"));
    assert!(
        migrations_check
            .message
            .contains(&decapod_root.display().to_string())
    );
}

#[test]
fn test_check_migrations_reports_warn_when_action_required_and_fail_on_corrupt_ledger() {
    let tmp = tempdir().unwrap();
    let decapod_root = tmp.path().join(".decapod");
    std::fs::create_dir_all(decapod_root.join("managed/migrations")).unwrap();

    let warn_res = check_migrations(tmp.path(), tmp.path(), &decapod_root);
    assert_eq!(warn_res.status, CheckStatus::Warn);
    assert!(warn_res.message.contains("Action required"));
    assert!(
        warn_res
            .message
            .contains("decapod system migrations --format json")
    );

    std::fs::write(
        decapod_root.join("managed/migrations/applied.json"),
        b"{not valid json",
    )
    .unwrap();
    let fail_res = check_migrations(tmp.path(), tmp.path(), &decapod_root);
    assert_eq!(fail_res.status, CheckStatus::Fail);
    assert!(
        fail_res
            .message
            .contains("decapod system migrations --format json")
    );
}

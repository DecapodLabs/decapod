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

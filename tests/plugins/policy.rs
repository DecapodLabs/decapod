use decapod::core::store::{Store, StoreKind};
use decapod::plugins::policy::{
    RiskLevel, RiskMap, RiskZone, approve_action, check_approval, eval_risk, initialize_policy_db,
};
use tempfile::tempdir;

#[test]
fn test_eval_risk() {
    let risk_map = RiskMap {
        zones: vec![RiskZone {
            path: ".decapod/".to_string(),
            level: RiskLevel::CRITICAL,
            rules: vec!["NO_AGENT_WRITE".to_string()],
        }],
    };

    // Command based
    let (level, _) = eval_risk("todo.delete", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);

    // Path based
    let (level, _) = eval_risk("todo.add", Some(".decapod/todo.db"), &risk_map);
    assert_eq!(level, RiskLevel::CRITICAL);

    // Safe
    let (level, _) = eval_risk("todo.list", Some("src/main.rs"), &risk_map);
    assert_eq!(level, RiskLevel::LOW);
}

#[test]
fn test_approval_lifecycle() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();

    let cmd = "todo.archive";
    let path = Some("docs/specs/INTENT.md");

    // Initially not approved
    assert!(!check_approval(&store, cmd, path, "global").unwrap());

    // Approve
    approve_action(&store, cmd, path, "operator", "global").unwrap();

    // Now approved
    assert!(check_approval(&store, cmd, path, "global").unwrap());

    // Different scope not approved
    assert!(!check_approval(&store, cmd, path, "local").unwrap());
}

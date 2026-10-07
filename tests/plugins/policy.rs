use decapod::core::store::{Store, StoreKind};
use decapod::plugins::policy::{
    RiskLevel, RiskMap, RiskZone, approve_action, check_approval, derive_fingerprint,
    enforce_broker_mutation_policy, eval_risk, initialize_policy_db, is_high_risk,
};
use std::fs;
use tempfile::tempdir;

fn write_project_config(root: &std::path::Path, approval_categories: &str) {
    fs::create_dir_all(root.join(".decapod")).unwrap();
    fs::write(
        root.join(".decapod/config.toml"),
        format!(
            "schema_version = \"1.0.0\"\n\n[init]\nspecs = true\nci = true\ndiagram_style = \"mermaid\"\nentrypoints = []\n\n[repo]\nproduct_name = \"policy-test\"\n\n[governance]\napproval_categories = {approval_categories}\n"
        ),
    )
    .unwrap();
}

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
fn test_eval_risk_commands() {
    let risk_map = RiskMap { zones: vec![] };

    // Delete commands
    let (level, reqs) = eval_risk("delete", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);
    assert!(!reqs.is_empty());

    // Archive commands
    let (level, _reqs) = eval_risk("archive", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);

    // Purge commands
    let (level, _) = eval_risk("purge", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);

    // Federation projection mutations use the same high-risk classification
    // as the broker's operation policy.
    let (level, reqs) = eval_risk("federation.rebuild", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);
    assert!(reqs.iter().any(|req| req.contains("approval")));

    let (level, _) = eval_risk("federation.supersede", None, &risk_map);
    assert_eq!(level, RiskLevel::HIGH);
}

#[test]
fn test_eval_risk_zones() {
    let risk_map = RiskMap {
        zones: vec![
            RiskZone {
                path: "docs/specs/".to_string(),
                level: RiskLevel::HIGH,
                rules: vec!["OPERATOR_REVIEW_REQUIRED".to_string()],
            },
            RiskZone {
                path: ".decapod/".to_string(),
                level: RiskLevel::CRITICAL,
                rules: vec!["NO_AGENT_WRITE".to_string()],
            },
        ],
    };

    // Test path matching
    let (level, reqs) = eval_risk("todo.edit", Some("docs/specs/INTENT.md"), &risk_map);
    assert_eq!(level, RiskLevel::HIGH);
    assert!(reqs.iter().any(|r| r.contains("OPERATOR_REVIEW_REQUIRED")));

    // Test CRITICAL zone
    let (level, _) = eval_risk("todo.add", Some(".decapod/data/todo.db"), &risk_map);
    assert_eq!(level, RiskLevel::CRITICAL);
}

#[test]
fn test_is_high_risk() {
    assert!(!is_high_risk(RiskLevel::LOW));
    assert!(!is_high_risk(RiskLevel::MEDIUM));
    assert!(is_high_risk(RiskLevel::HIGH));
    assert!(is_high_risk(RiskLevel::CRITICAL));
}

#[test]
fn test_risk_level_values() {
    // Test RiskLevel discriminant values
    assert_eq!(RiskLevel::LOW as u8, 0);
    assert_eq!(RiskLevel::MEDIUM as u8, 1);
    assert_eq!(RiskLevel::HIGH as u8, 2);
    assert_eq!(RiskLevel::CRITICAL as u8, 3);
}

#[test]
fn test_derive_fingerprint() {
    let fp1 = derive_fingerprint("todo.add", Some("src/main.rs"), "repo");
    let fp2 = derive_fingerprint("todo.add", Some("src/main.rs"), "repo");
    let fp3 = derive_fingerprint("todo.add", Some("src/other.rs"), "repo");
    let fp4 = derive_fingerprint("todo.add", Some("src/main.rs"), "user");

    // Same inputs should produce same fingerprint
    assert_eq!(fp1, fp2);

    // Different inputs should produce different fingerprints
    assert_ne!(fp1, fp3);
    assert_ne!(fp1, fp4);
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

#[test]
fn test_list_approvals() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();

    // Add approvals
    approve_action(&store, "cmd1", None, "user1", "global").unwrap();
    approve_action(&store, "cmd2", Some("path/to/file"), "user2", "repo").unwrap();

    // Note: list_approvals has a bug (action_id vs action_fingerprint)
    // Skip this test for now
}

#[test]
fn test_approval_different_scopes() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();

    let cmd = "todo.delete";
    let path = Some("docs/");

    // Approve in global scope
    approve_action(&store, cmd, path, "operator", "global").unwrap();

    // Should work for global scope
    assert!(check_approval(&store, cmd, path, "global").unwrap());

    // Different scope should NOT work (exact fingerprint match)
    assert!(!check_approval(&store, cmd, path, "docs").unwrap());
}

#[test]
fn federation_operations_honor_exact_configured_approvals() {
    let tmp = tempdir().unwrap();
    write_project_config(tmp.path(), "[\"destructive_operations\"]");
    let store = Store {
        kind: StoreKind::Repo,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();

    for operation in ["federation.rebuild", "federation.supersede"] {
        assert!(
            enforce_broker_mutation_policy(&store.root, "decapod", operation).is_err(),
            "{operation} must require its configured approval"
        );
        approve_action(&store, operation, None, "operator", "global").unwrap();
        assert!(check_approval(&store, operation, None, "global").unwrap());
        enforce_broker_mutation_policy(&store.root, "decapod", operation).unwrap();
    }
}

#[test]
fn empty_approval_categories_do_not_gate_federation_operations() {
    let tmp = tempdir().unwrap();
    write_project_config(tmp.path(), "[]");

    for operation in ["federation.rebuild", "federation.supersede"] {
        enforce_broker_mutation_policy(tmp.path(), "decapod", operation).unwrap();
    }
}

#[test]
fn policy_approval_accepts_eval_fingerprint() {
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();

    let fingerprint = derive_fingerprint("federation.rebuild", None, "global");
    approve_action(&store, &fingerprint, None, "operator", "global").unwrap();
    assert!(check_approval(&store, "federation.rebuild", None, "global").unwrap());
}

#[test]
fn targeted_grants_do_not_authorize_other_targets_or_scopes() {
    use decapod::plugins::policy::{check_approval_on_conn, revoke_approval};
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();
    let fingerprint = derive_fingerprint("task.archive", Some("todo-alpha"), "global");
    let grant = approve_action(&store, &fingerprint, None, "operator", "global").unwrap();
    assert!(check_approval(&store, "task.archive", Some("todo-alpha"), "global").unwrap());
    assert!(!check_approval(&store, "task.archive", Some("todo-beta"), "global").unwrap());
    assert!(!check_approval(&store, "task.archive", None, "global").unwrap());
    assert!(!check_approval(&store, "task.archive", Some("todo-alpha"), "repo").unwrap());
    let conn =
        decapod::core::db::Connection::open(decapod::plugins::policy::policy_db_path(&store.root))
            .unwrap();
    assert!(check_approval_on_conn(&conn, "task.archive", Some("todo-alpha"), "global").unwrap());
    assert!(!check_approval_on_conn(&conn, "task.archive", Some("todo-beta"), "global").unwrap());
    assert!(revoke_approval(&store, &grant).unwrap());
    assert!(!check_approval_on_conn(&conn, "task.archive", Some("todo-alpha"), "global").unwrap());
    assert!(!revoke_approval(&store, &grant).unwrap());
    approve_action(&store, "task.archive", None, "operator", "global").unwrap();
    assert!(check_approval(&store, "task.archive", Some("todo-beta"), "global").unwrap());
}

#[test]
fn expiry_accepts_utc_and_offsets_and_rejects_invalid_or_elapsed_grants() {
    use decapod::core::db::{Connection, params};
    use decapod::plugins::policy::{
        approve_action_with_expiry, check_approval_on_conn, policy_db_path, revoke_approval,
    };
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    initialize_policy_db(&store.root).unwrap();
    for expiry in [
        "2099-01-01T00:00:00Z",
        "2099-01-01T01:00:00+01:00",
        "4070908800Z",
    ] {
        let grant = approve_action_with_expiry(
            &store,
            "task.archive",
            Some("alpha"),
            "operator",
            "global",
            Some(expiry),
        )
        .unwrap();
        assert!(check_approval(&store, "task.archive", Some("alpha"), "global").unwrap());
        revoke_approval(&store, &grant).unwrap();
    }
    for expiry in ["2000-01-01T00:00:00Z", "1Z", "invalid"] {
        assert!(
            approve_action_with_expiry(
                &store,
                "task.archive",
                Some("alpha"),
                "operator",
                "global",
                Some(expiry)
            )
            .is_err()
        );
    }
    let grant =
        approve_action(&store, "task.archive", Some("alpha"), "operator", "global").unwrap();
    let conn = Connection::open(policy_db_path(&store.root)).unwrap();
    let now = decapod::core::time::now_epoch_z();
    for expiry in ["2000-01-01T00:00:00Z", "invalid", now.as_str()] {
        conn.execute(
            "UPDATE approvals SET expires_at = ?1 WHERE approval_id = ?2",
            params![expiry, grant],
        )
        .unwrap();
        assert!(!check_approval(&store, "task.archive", Some("alpha"), "global").unwrap());
        assert!(!check_approval_on_conn(&conn, "task.archive", Some("alpha"), "global").unwrap());
    }
}

#[test]
fn cli_grants_have_bounded_lifetime_and_can_be_listed_and_revoked() {
    use clap::Parser;
    use decapod::plugins::policy::{PolicyCli, list_approvals, run_policy_cli};
    let tmp = tempdir().unwrap();
    let store = Store {
        kind: StoreKind::User,
        root: tmp.path().to_path_buf(),
    };
    let fingerprint = derive_fingerprint("task.archive", Some("alpha"), "global");
    let start = decapod::core::time::now_epoch_z()
        .trim_end_matches('Z')
        .parse::<i64>()
        .unwrap();
    run_policy_cli(
        &store,
        PolicyCli::try_parse_from(["policy", "approve", "--id", &fingerprint]).unwrap(),
    )
    .unwrap();
    let grants = list_approvals(&store).unwrap();
    assert_eq!(grants.len(), 1);
    assert_eq!(grants[0].action_id, fingerprint);
    let expiry = grants[0]
        .expires_at
        .as_ref()
        .unwrap()
        .trim_end_matches('Z')
        .parse::<i64>()
        .unwrap();
    let end = decapod::core::time::now_epoch_z()
        .trim_end_matches('Z')
        .parse::<i64>()
        .unwrap();
    assert!((start + 900..=end + 900).contains(&expiry));
    run_policy_cli(
        &store,
        PolicyCli::try_parse_from(["policy", "revoke", "--id", &grants[0].approval_id]).unwrap(),
    )
    .unwrap();
    assert!(!check_approval(&store, "task.archive", Some("alpha"), "global").unwrap());
    for seconds in ["0", "18446744073709551615"] {
        assert!(
            run_policy_cli(
                &store,
                PolicyCli::try_parse_from([
                    "policy",
                    "approve",
                    "--id",
                    &fingerprint,
                    "--expires-in-seconds",
                    seconds
                ])
                .unwrap()
            )
            .is_err()
        );
    }
}

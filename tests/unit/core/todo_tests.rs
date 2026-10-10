// Moved from src/decapod/core/todo.rs
#[test]
fn claim_container_error_summary_hides_preflight_dump() {
    let summary = super::summarize_claim_container_error(
        "Validation error: AUTOREMEDIABLE_VALIDATION_ERROR code=container_runtime_preflight_failed\nstderr:\nvery long host-specific output",
    );

    assert_eq!(
        summary,
        "Container runtime preflight failed. Check Docker/Podman availability and permissions."
    );
    assert!(!summary.contains("AUTOREMEDIABLE"));
    assert!(!summary.contains("stderr"));
}

#[test]
fn read_task_listing_does_not_attempt_schema_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    super::initialize_todo_db(&root).unwrap();

    let tasks = super::list_tasks(&root, None, None, None, None, None).unwrap();

    assert!(tasks.is_empty());
}

#[test]
fn completion_dependency_gate_rechecks_proof_readiness() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    super::initialize_todo_db(&root).unwrap();
    let add = |title: &str, depends_on: &str| {
        super::add_task(
            &root,
            &super::TodoCommand::Add {
                title: title.to_string(),
                description: String::new(),
                priority: "medium".to_string(),
                tags: "houseboat,dependency".to_string(),
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: String::new(),
                dir: Some(root.to_string_lossy().to_string()),
                depends_on: depends_on.to_string(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let dependency_id = add("Completion dependency unique", "");
    let target_id = add("Completion target unique", &dependency_id);

    let blocked = super::dependency_completion_gate(&root, &target_id)
        .unwrap()
        .expect("unfinished dependency must block completion");
    assert_eq!(
        blocked["result"]["dependency_readiness"]["state"],
        "waiting"
    );

    let db = root.join(crate::core::schemas::LOCAL_DB_NAME);
    let conn = decapod::core::db::Connection::open(db).unwrap();
    conn.execute(
        "UPDATE tasks SET status = 'done' WHERE id = ?1",
        [&dependency_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_verification(
            todo_id, proof_plan, last_verified_at, last_verified_status,
            last_verified_notes, verification_policy_days, updated_at
         ) VALUES(?1, '[]', '100Z', 'passed', '', 90, '100Z')",
        [&dependency_id],
    )
    .unwrap();
    assert!(
        super::dependency_completion_gate(&root, &target_id)
            .unwrap()
            .is_none()
    );
}

#[test]
fn committed_claim_followup_releases_database_and_runs_only_once() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    super::initialize_todo_db(root).unwrap();
    let result = super::claim_followup_once(root, "same-request", || {
        // Another thread must access the store while preparation is running.
        let path = root.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            tx.send(super::list_tasks(&path, None, None, None, None, None).is_ok())
                .unwrap();
        });
        assert!(rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap());
        reader.join().unwrap();
        // A concurrent retry sees uncertainty rather than dispatching again.
        let retry = super::claim_followup_once(root, "same-request", || panic!("duplicate launch"))
            .unwrap();
        assert_eq!(retry["status"], "warning");
        super::claim_container_warning("mock preparation failed")
    })
    .unwrap();
    assert_eq!(result["status"], "warning");
    assert_eq!(
        super::claim_followup_once(root, "same-request", || panic!("duplicate launch")).unwrap(),
        result
    );
    super::DbBroker::new(root).verify_replay().unwrap();
}

#[test]
fn interrupted_claim_followup_is_not_reexecuted() {
    let temp = tempfile::tempdir().unwrap();
    super::initialize_todo_db(temp.path()).unwrap();
    let interrupted = std::panic::catch_unwind(|| {
        let _ = super::claim_followup_once(temp.path(), "interrupted", || {
            panic!("simulated interruption")
        });
    });
    assert!(interrupted.is_err());
    let retry =
        super::claim_followup_once(temp.path(), "interrupted", || panic!("must not relaunch"))
            .unwrap();
    assert_eq!(retry["status"], "warning");
    assert!(retry["detail"].as_str().unwrap().contains("interrupted"));
}

#[test]
fn post_commit_followup_read_failure_preserves_successful_claim_envelope() {
    use clap::Parser;
    let temp = tempfile::tempdir().unwrap();
    let cli = super::TodoCli::parse_from(["todo", "claim", "--id", "task-missing"]);
    let committed = serde_json::json!({"cmd":"todo.claim", "id":"task-missing", "status":"ok",
        "container":{"status":"pending", "code":"claim_container_followup"}})
    .to_string();
    for unavailable in [false, true] {
        let root = temp
            .path()
            .join(if unavailable { "blocked" } else { "empty" });
        if unavailable {
            std::fs::write(&root, "not a datastore directory").unwrap();
        } else {
            super::initialize_todo_db(&root).unwrap();
        }
        let result = super::finish_broker_claim_response(
            &super::Store {
                root,
                kind: crate::core::store::StoreKind::Repo,
            },
            &cli,
            &committed,
            "post-commit-read-failure",
        );
        let value: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(value["status"], "ok");
        assert_eq!(value["id"], "task-missing");
        assert_eq!(value["container"]["status"], "warning");
    }
}

#[test]
fn stale_acknowledged_claim_cannot_start_container_preparation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    super::initialize_todo_db(root).unwrap();
    let added = super::add_task(
        root,
        &super::TodoCommand::Add {
            title: "claim freshness".into(),
            description: String::new(),
            tags: String::new(),
            owner: "agent".into(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: Some(root.to_string_lossy().into_owned()),
            priority: "medium".into(),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = added["id"].as_str().unwrap();
    let conn = crate::core::db::db_connect(&super::todo_db_path(root).to_string_lossy()).unwrap();
    conn.execute("UPDATE tasks SET assigned_to='agent', lease_generation=2, lease_expires_at='9999999999Z' WHERE id=?1", [id]).unwrap();
    drop(conn);
    let ack = serde_json::json!({"result":{"assigned_to":"agent", "lease_generation":2}});
    assert!(super::claim_preparation_is_current(root, id, "agent", &ack).unwrap());
    for mutation in [
        "UPDATE tasks SET lease_expires_at='1Z' WHERE id=?1",
        "UPDATE tasks SET lease_expires_at='9999999999Z', assigned_to='other' WHERE id=?1",
        "UPDATE tasks SET assigned_to='agent', lease_generation=3 WHERE id=?1",
    ] {
        let conn =
            crate::core::db::db_connect(&super::todo_db_path(root).to_string_lossy()).unwrap();
        conn.execute(mutation, [id]).unwrap();
        drop(conn);
        assert!(!super::claim_preparation_is_current(root, id, "agent", &ack).unwrap());
    }
}

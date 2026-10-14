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

    let current = super::get_task(&root, &target_id).unwrap().unwrap();
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.clone(),
    };
    let blocked_mutation = super::update_status(
        &store,
        &target_id,
        current.revision,
        &current.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(
        blocked_mutation["status"], "conflict",
        "unfinished dependency may not complete through direct API"
    );
    assert_eq!(
        super::get_task(&root, &target_id)
            .unwrap()
            .unwrap()
            .revision,
        current.revision
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
            last_verified_notes, verification_policy_days, updated_at, verification_artifacts
         ) VALUES(?1, '[]', '100Z', 'passed', '', 90, '100Z', ?2)",
        crate::core::db::params![dependency_id, serde_json::json!({
            "task_id": dependency_id,
            "task_revision": super::get_task(&root, &dependency_id).unwrap().unwrap().revision,
            "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": "sha256:test-proof"}]
        }).to_string()],
    )
    .unwrap();
    assert!(
        super::dependency_completion_gate(&root, &target_id)
            .unwrap()
            .is_none()
    );

    let proven_revision = super::get_task(&root, &dependency_id)
        .unwrap()
        .unwrap()
        .revision;
    super::comment_task(&root, &dependency_id, "Evidence observation only").unwrap();
    assert_eq!(
        super::get_task(&root, &dependency_id)
            .unwrap()
            .unwrap()
            .revision,
        proven_revision
    );
    assert!(
        super::dependency_completion_gate(&root, &target_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        super::get_work_claim(&root, &dependency_id)
            .unwrap()
            .unwrap()
            .status,
        crate::core::work_claim::WorkClaimStatus::Complete
    );
    assert!(
        !crate::plan_governance::collect_unverified_done_todos(&root)
            .unwrap()
            .contains(&dependency_id)
    );

    // Editing completed intent leaves historical evidence intact but cannot reuse it.
    super::edit_task(
        &root,
        &dependency_id,
        None,
        Some("Changed acceptance intent"),
        None,
        None,
    )
    .unwrap();
    let stale = super::get_work_claim(&root, &dependency_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stale.status,
        crate::core::work_claim::WorkClaimStatus::Blocked
    );
    assert_eq!(
        stale.validation_status,
        crate::core::work_claim::WorkClaimValidationStatus::Stale
    );
    assert!(!stale.proof_refs.is_empty());
    assert!(
        crate::plan_governance::collect_unverified_done_todos(&root)
            .unwrap()
            .contains(&dependency_id)
    );
    let blocked = super::dependency_completion_gate(&root, &target_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        blocked["result"]["dependency_readiness"]["state"],
        "proof_blocked"
    );
    let blocked_claim = super::claim_task(
        &root,
        &target_id,
        "revision-agent",
        super::ClaimMode::Exclusive,
    )
    .unwrap();
    assert_eq!(blocked_claim["status"], "conflict");
    let current = super::get_task(&root, &target_id).unwrap().unwrap();
    let blocked_done = super::update_status(
        &store,
        &target_id,
        current.revision,
        &current.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(blocked_done["status"], "conflict");
    assert_eq!(
        super::get_task(&root, &target_id)
            .unwrap()
            .unwrap()
            .revision,
        current.revision
    );

    // Public owner mutations also advance custody; comments above deliberately do not.
    let before_owner = super::get_task(&root, &dependency_id)
        .unwrap()
        .unwrap()
        .revision;
    super::add_task_owner(&root, &dependency_id, "observer", "watcher").unwrap();
    assert_eq!(
        super::get_task(&root, &dependency_id)
            .unwrap()
            .unwrap()
            .revision,
        before_owner + 1
    );
    super::remove_task_owner(&root, &dependency_id, "observer").unwrap();
    assert_eq!(
        super::get_task(&root, &dependency_id)
            .unwrap()
            .unwrap()
            .revision,
        before_owner + 2
    );

    // A successful earlier check is not authority when the dependency changes.
    let task = super::get_task(&root, &target_id).unwrap().unwrap();
    conn.execute(
        "UPDATE task_verification SET last_verified_status='failed' WHERE todo_id=?1",
        [&dependency_id],
    )
    .unwrap();
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.clone(),
    };
    let result = super::update_status(
        &store,
        &target_id,
        task.revision,
        &task.status,
        "done",
        "task.done",
        serde_json::json!({}),
    )
    .unwrap();
    assert_eq!(
        result["status"], "conflict",
        "revoked dependency proof must block: {result}"
    );
    let after = super::get_task(&root, &target_id).unwrap().unwrap();
    assert_eq!(after.revision, task.revision);
    assert_eq!(after.status, task.status);
    let count: i64 = conn.query_row("SELECT count(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='task.done'", [&target_id], |r|r.get(0)).unwrap();
    assert_eq!(count, 0);
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

#[test]
fn worker_missing_artifact_never_completes_or_archives_task() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".decapod/data");
    super::initialize_todo_db(&root).unwrap();
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: root.clone(),
    };
    let id = "bugs_worker_missing_artifact";
    let conn = crate::core::db::Connection::open(super::todo_db_path(&root)).unwrap();
    conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,revision) VALUES(?1,'worker','Worker proof regression','open','1Z','1Z','.', '',0)", [id]).unwrap();
    let result = super::run_worker_loop(&store, "worker-proof", Some(id), 1, true, true).unwrap();
    let task = super::get_task(&root, id).unwrap().unwrap();
    assert_ne!(task.status, "done", "{result}");
    assert_ne!(task.status, "archived", "{result}");
    let success_events: i64 = conn.query_row("SELECT count(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type IN ('task.done','task.archive','task.verify.capture')",[id],|r|r.get(0)).unwrap();
    assert_eq!(success_events, 0);
    let failures: i64 = conn.query_row("SELECT count(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='task.verify.capture_failed'",[id],|r|r.get(0)).unwrap();
    assert_eq!(failures, 1);
}

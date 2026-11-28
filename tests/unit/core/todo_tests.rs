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
    // Seed legacy authority before journal genesis; this test's proof setup is
    // intentionally historical. The later production edit must create a
    // baseline/transition without blessing drift in an existing journal.
    let add = |title: &str, depends_on: &str| {
        let id = format!("test_{}", title.replace(' ', "_").to_lowercase());
        let conn = crate::core::db::Connection::open(super::todo_db_path(&root)).unwrap();
        conn.execute(
            "INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,depends_on,revision) VALUES(?1,?1,?2,'open','1Z','1Z',?3,'',?4,0)",
            crate::core::db::params![id,title,root.to_string_lossy(),depends_on],
        ).unwrap();
        if !depends_on.is_empty() {
            conn.execute("INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at) VALUES(?1,?2,?3,'1Z')", crate::core::db::params![format!("edge:{id}"),id,depends_on]).unwrap();
        }
        id
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
    let conn = crate::core::db::Connection::open(db).unwrap();
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

#[test]
fn claim_task_rejects_unknown_and_empty_actor_with_guidance() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".decapod/data");
    super::initialize_todo_db(&root).unwrap();
    let added = super::add_task(
        &root,
        &super::TodoCommand::Add {
            title: "Reject unknown claim".into(),
            description: String::new(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: Some(temp.path().to_string_lossy().into_owned()),
            priority: "medium".into(),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = added["id"].as_str().unwrap();

    for actor in ["unknown", "UNKNOWN", "", "   "] {
        for mode in [super::ClaimMode::Exclusive, super::ClaimMode::Shared] {
            let err = super::claim_task_with_lease(&root, id, actor, mode, None, None)
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("AUTHORING_ACTOR_REQUIRED"),
                "expected AUTHORING_ACTOR_REQUIRED for actor={actor:?}, got: {err}"
            );
            assert!(
                err.contains("DECAPOD_AGENT_ID=<actor>"),
                "expected DECAPOD_AGENT_ID guidance for actor={actor:?}, got: {err}"
            );
        }
        let renew_err = super::renew_claim_lease(&root, id, actor, Some(600))
            .unwrap_err()
            .to_string();
        assert!(renew_err.contains("AUTHORING_ACTOR_REQUIRED"));
        let yield_err = super::yield_claim_lease(&root, id, actor, "done")
            .unwrap_err()
            .to_string();
        assert!(yield_err.contains("AUTHORING_ACTOR_REQUIRED"));
    }

    let task = super::get_task(&root, id).unwrap().unwrap();
    assert_eq!(task.assigned_to, "");
    assert!(task.owners.is_empty());
}

#[test]
fn claim_task_recovers_legacy_unknown_claim_without_real_custody_and_blocks_if_bound() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(".decapod/data");
    super::initialize_todo_db(&root).unwrap();

    let clean_id = "code_01legacyunknownclean";
    let bound_id = "code_01legacyunknownbound";

    let conn = crate::core::db::Connection::open(super::todo_db_path(&root)).unwrap();
    for (id, hash, title) in [
        (
            clean_id,
            "01lega",
            "Legacy unknown claim without checkout custody",
        ),
        (
            bound_id,
            "01legb",
            "Legacy unknown claim with real checkout binding",
        ),
    ] {
        conn.execute(
            "INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,owner,assigned_to,assigned_at,lease_expires_at,lease_generation,lease_lifecycle,intent_anchor,revision)
             VALUES(?1,?2,?3,'open','100Z','100Z',?,'root','unknown','unknown','100Z','4102444800Z',1,'claimed',?5,1)",
            crate::core::db::params![
                id,
                hash,
                title,
                temp.path().to_string_lossy().as_ref(),
                format!("intent:todo:{id}")
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO task_owners(id, task_id, agent_id, claimed_at, claim_type)
             VALUES(?1, ?2, 'unknown', '100Z', 'primary')",
            crate::core::db::params![format!("own_{id}"), id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO authoring_bindings(worktree_id, task_id, agent_id, lease_generation, branch, repository_id, updated_at)
         VALUES('wt-real-custody', ?1, 'real-custodian', 1, 'agent/real-custodian/bound', '/repo/.git', '100Z')",
        [bound_id],
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        super::task_journal_checkpoint(&root, None).unwrap()["status"],
        "ok"
    );

    // Explicit real actor cleanly adopts legacy 'unknown' claim with no real checkout custody.
    let adopted = super::claim_task_with_lease(
        &root,
        clean_id,
        "alice",
        super::ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();
    assert_eq!(adopted["status"], "ok", "{adopted}");
    assert_eq!(adopted["result"]["assigned_to"], "alice");
    assert_eq!(adopted["result"]["reclaimed_from"], "unknown");
    assert_eq!(adopted["result"]["lease_generation"], 2);
    assert_eq!(adopted["result"]["lease_lifecycle"], "reclaimed");

    // Cross-owner custody between real actors remains blocked.
    let cross_owner = super::claim_task_with_lease(
        &root,
        clean_id,
        "bob",
        super::ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();
    assert_eq!(cross_owner["status"], "conflict", "{cross_owner}");
    assert_eq!(cross_owner["result"]["assigned_to"], "alice");

    // Pre-existing 'unknown' claim with verified real-actor checkout custody fails closed.
    let blocked = super::claim_task_with_lease(
        &root,
        bound_id,
        "dave",
        super::ClaimMode::Exclusive,
        Some(1800),
        None,
    )
    .unwrap();
    assert_eq!(blocked["status"], "conflict", "{blocked}");
    assert_eq!(blocked["result"]["assigned_to"], "unknown");
}

#[test]
fn cloud_proof_capture_request_requires_bound_custody_and_binds_real_record() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path().join("repo");
    let custody_dir = tmp.path().join("custody");
    std::fs::create_dir_all(&repo_root).unwrap();
    let run_git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(&repo_root)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run_git(&["init", "-b", "main"]);
    run_git(&["config", "user.email", "test@decapodlabs.ai"]);
    run_git(&["config", "user.name", "Test"]);
    std::fs::write(repo_root.join("README.md"), b"hello\n").unwrap();
    run_git(&["add", "README.md"]);
    run_git(&["commit", "-m", "init"]);

    let store = crate::core::cloud_workspace::CloudCustodyStore::at(&custody_dir);
    // Unbound checkout must fail closed with CLOUD_CUSTODY_UNBOUND (never fabricate ws_/custody_).
    let err = super::build_cloud_proof_capture_request_with_store(
        &repo_root,
        "DecapodLabs/decapod",
        "feat_1809_test",
        99,
        99,
        &[],
        None,
        Some(&store),
    )
    .expect_err("unbound checkout must not fabricate custody ids");
    assert!(
        err.to_string().contains("CLOUD_CUSTODY_UNBOUND"),
        "err={err}"
    );

    let bound = crate::core::cloud_workspace::CloudCustodyRecord {
        version: 1,
        origin: "https://api.dactyl.example".into(),
        project_id: "DecapodLabs/decapod".into(),
        task_id: "feat_1809_test".into(),
        task_title: "Test bound custody".into(),
        native_task_version: 3,
        workspace_id: "ws_real_bound_1809".into(),
        worktree_path: repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.clone()),
        branch: "main".into(),
        actor: "alice".into(),
        custody_id: "custody_real_bound_1809".into(),
        pending_operation_id: None,
        pending_custody_id: None,
        pending_native_task_version: None,
        task_revision: Some(7),
        fencing_generation: Some(4),
        expires_at: Some("2099-01-01T00:00:00Z".into()),
        invocation_id: None,
        container_expected: false,
        container_runtime: None,
        container_name: None,
        container_id: None,
    };
    store.save(&repo_root, &bound).unwrap();

    let req = super::build_cloud_proof_capture_request_with_store(
        &repo_root,
        "DecapodLabs/decapod",
        "feat_1809_test",
        999,
        999,
        &[],
        None,
        Some(&store),
    )
    .expect("bound custody must produce candidate-bound ProofCaptureRequest");
    assert_eq!(req.subject.workspace_id, "ws_real_bound_1809");
    assert_eq!(req.subject.custody_id, "custody_real_bound_1809");
    assert_eq!(req.subject.task_revision, 7);
    assert_eq!(req.subject.fencing_generation, 4);
}

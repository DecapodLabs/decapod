use decapod::core::db::Connection;
use decapod::core::todo;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn todo_rebuild_accepts_task_proof_claimed_events() {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path();
    std::fs::create_dir_all(root).expect("create root");

    let events_path = root.join("todo.events.jsonl");
    let add_event = json!({
        "ts": "1770000001Z",
        "event_id": "E_ADD_1",
        "event_type": "task.add",
        "status": "success",
        "task_id": "T_1",
        "payload": {
            "title": "Compatibility task"
        },
        "actor": "test"
    });
    let proof_claimed = json!({
        "ts": "1770000002Z",
        "event_id": "E_PROOF_1",
        "event_type": "task.proof.claimed",
        "status": "success",
        "task_id": "T_1",
        "payload": {
            "last_verified_status": "CLAIMED",
            "last_verified_notes": "Proof hooks pending verification"
        },
        "actor": "test"
    });

    std::fs::write(&events_path, format!("{add_event}\n{proof_claimed}\n")).expect("write events");

    let out_db = root.join("decapod.db");
    let count =
        todo::rebuild_db_from_events(&events_path, &out_db).expect("rebuild should succeed");
    assert_eq!(count, 2, "expected both events to replay");

    let conn = Connection::open(out_db).expect("open rebuilt db");
    let task_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE id='T_1'", [], |row| {
            row.get(0)
        })
        .expect("query tasks");
    assert_eq!(task_count, 1, "task must exist after rebuild");
}

#[test]
fn legacy_replay_retains_proof_history_without_inventing_revision_custody() {
    let tmp = TempDir::new().unwrap();
    let events_path = tmp.path().join("todo.events.jsonl");
    let event = |event_id: &str, event_type: &str, payload: serde_json::Value| {
        json!({
            "ts": "1770000001Z", "event_id": event_id, "event_type": event_type,
            "status": "success", "task_id": "T_REPLAY", "payload": payload, "actor": "test"
        })
    };
    let records = [
        event(
            "R_ADD",
            "task.add",
            json!({"title": "Replay proof subject"}),
        ),
        event("R_DONE", "task.done", json!({"revision": 9})),
        event(
            "R_PROOF",
            "task.verify.capture",
            json!({
                "proof_plan": ["validate_passes"], "last_verified_status": "pass",
                "verification_artifacts": {
                    "task_id": "T_REPLAY", "task_revision": 9,
                    "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": "sha256:historical"}]
                }
            }),
        ),
    ];
    std::fs::write(
        &events_path,
        records
            .iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let db = tmp.path().join("decapod.db");
    todo::rebuild_db_from_events(&events_path, &db).unwrap();
    let claim = todo::get_work_claim(tmp.path(), "T_REPLAY")
        .unwrap()
        .unwrap();
    assert_eq!(
        claim.status,
        decapod::core::work_claim::WorkClaimStatus::Blocked
    );
    assert_eq!(
        claim.validation_status,
        decapod::core::work_claim::WorkClaimValidationStatus::Stale
    );
    assert_eq!(
        claim.proof_refs,
        ["proof:T_REPLAY:validate_passes:sha256:historical"]
    );
    let conn = Connection::open(db).unwrap();
    let raw: String = conn
        .query_row(
            "SELECT verification_artifacts FROM task_verification WHERE todo_id='T_REPLAY'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let artifacts: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(artifacts.get("task_revision").is_none());
    let original: String = conn
        .query_row(
            "SELECT payload FROM events WHERE event_id='R_PROOF'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        original.contains("task_revision"),
        "canonical historical event remains intact"
    );
}

#[test]
fn legacy_replay_retains_basic_shared_claim_audit_without_new_ownership() {
    let tmp = TempDir::new().unwrap();
    let events_path = tmp.path().join("todo.events.jsonl");
    let event = |event_id: &str, event_type: &str, payload: serde_json::Value| {
        json!({
            "ts": "1770000001Z", "event_id": event_id, "event_type": event_type,
            "status": "success", "task_id": "T_SHARED", "payload": payload, "actor": "member"
        })
    };
    let records = [
        event("S_ADD", "task.add", json!({"title": "Shared task"})),
        event(
            "S_OWNER",
            "ownership.claim",
            json!({"agent_id": "owner", "claim_type": "primary", "claim_id": "OWNER"}),
        ),
        event(
            "S_MEMBER",
            "ownership.claim",
            json!({"agent_id": "member", "claim_type": "secondary", "claim_id": "MEMBER"}),
        ),
        event(
            "S_AUDIT",
            "policy.basic_shared_claim.used",
            json!({"policy": "governance.allow_basic_shared_claims", "config_digest": "sha256:example", "agent_label": "member", "primary_owner_label": "owner", "identity_assurance": "cooperative_not_authenticated"}),
        ),
    ];
    std::fs::write(
        &events_path,
        records
            .iter()
            .map(|event| format!("{event}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let db = tmp.path().join("decapod.db");
    assert_eq!(todo::rebuild_db_from_events(&events_path, &db).unwrap(), 4);
    let conn = Connection::open(db).unwrap();
    let owners: i64 = conn
        .query_row(
            "SELECT count(*) FROM task_owners WHERE task_id='T_SHARED'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let audits: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE event_type='policy.basic_shared_claim.used'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(owners, 2);
    assert_eq!(audits, 1);
}

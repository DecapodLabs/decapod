use decapod::core::todo;
use rusqlite::Connection;
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

    std::fs::write(&events_path, format!("{}\n{}\n", add_event, proof_claimed))
        .expect("write events");

    let out_db = root.join("todo.db");
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

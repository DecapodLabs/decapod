use super::*;

fn fixture() -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().unwrap();
    initialize_todo_db(temp.path()).unwrap();
    let added = add_task(
        temp.path(),
        &TodoCommand::Add {
            title: "Release custody regression".to_string(),
            description: String::new(),
            priority: "medium".to_string(),
            tags: String::new(),
            owner: String::new(),
            due: None,
            r#ref: String::new(),
            scope: String::new(),
            dir: Some(temp.path().to_string_lossy().into_owned()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_string();
    (temp, id)
}

fn claim(root: &Path, id: &str) {
    let result = claim_task_with_lease(
        root,
        id,
        "release-owner",
        ClaimMode::Exclusive,
        Some(3600),
        Some("intent:release-regression".to_string()),
    )
    .unwrap();
    assert_eq!(result["status"], "ok", "{result}");
}

fn snapshot(root: &Path, id: &str) -> serde_json::Value {
    let conn = Connection::open(todo_db_path(root)).unwrap();
    let state = conn
        .query_row(
            "SELECT status, assigned_to, assigned_at, lease_expires_at,
                    lease_lifecycle, lease_generation, intent_anchor, revision, updated_at
             FROM tasks WHERE id = ?1",
            [id],
            |row| {
                Ok(serde_json::json!({
                    "status": row.get::<_, String>(0)?,
                    "assigned_to": row.get::<_, String>(1)?,
                    "assigned_at": row.get::<_, Option<String>>(2)?,
                    "lease_expires_at": row.get::<_, Option<String>>(3)?,
                    "lease_lifecycle": row.get::<_, String>(4)?,
                    "lease_generation": row.get::<_, i64>(5)?,
                    "intent_anchor": row.get::<_, String>(6)?,
                    "revision": row.get::<_, i64>(7)?,
                    "updated_at": row.get::<_, String>(8)?,
                }))
            },
        )
        .unwrap();
    let owners: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_owners WHERE task_id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    let events: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE stream = 'todo' AND subject_id = ?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    serde_json::json!({"task": state, "owners": owners, "events": events})
}

#[test]
fn non_owner_cannot_release_an_active_exclusive_claim() {
    let (temp, id) = fixture();
    claim(temp.path(), &id);
    let before = snapshot(temp.path(), &id);
    let result = release_task_as_actor(temp.path(), &id, "unrelated-agent").unwrap();
    assert_eq!(result["status"], "conflict", "{result}");
    assert_eq!(result["result"]["reason"], "not_assignee");
    assert_eq!(snapshot(temp.path(), &id), before);
    let stolen = claim_task(temp.path(), &id, "unrelated-agent", ClaimMode::Exclusive).unwrap();
    assert_eq!(stolen["status"], "conflict", "{stolen}");
}

#[test]
fn owner_release_clears_custody_once_and_preserves_generation() {
    let (temp, id) = fixture();
    claim(temp.path(), &id);
    let before = snapshot(temp.path(), &id);
    let result = release_task_as_actor(temp.path(), &id, "release-owner").unwrap();
    assert_eq!(result["status"], "ok", "{result}");
    let after = snapshot(temp.path(), &id);
    assert_eq!(after["task"]["assigned_to"], "");
    assert!(after["task"]["assigned_at"].is_null());
    assert!(after["task"]["lease_expires_at"].is_null());
    assert_eq!(after["task"]["lease_lifecycle"], "released");
    assert_eq!(
        after["task"]["lease_generation"],
        before["task"]["lease_generation"]
    );
    assert_eq!(
        after["task"]["intent_anchor"],
        before["task"]["intent_anchor"]
    );
    assert_eq!(
        after["task"]["revision"].as_i64(),
        Some(before["task"]["revision"].as_i64().unwrap() + 1)
    );
    assert_eq!(after["owners"], 0);
    assert_eq!(
        after["events"].as_i64(),
        Some(before["events"].as_i64().unwrap() + 1)
    );
    assert_eq!(
        release_task_as_actor(temp.path(), &id, "release-owner").unwrap()["status"],
        "ok"
    );
    assert_eq!(snapshot(temp.path(), &id), after);
}

#[test]
fn releasing_unclaimed_task_is_an_idempotent_noop() {
    let (temp, id) = fixture();
    let before = snapshot(temp.path(), &id);
    assert_eq!(
        release_task_as_actor(temp.path(), &id, "unrelated-agent").unwrap()["status"],
        "ok"
    );
    assert_eq!(snapshot(temp.path(), &id), before);
}

#[test]
fn release_preserves_terminal_status_and_invalidates_claim_cache() {
    for status in ["done", "archived"] {
        let (temp, id) = fixture();
        claim(temp.path(), &id);
        let conn = Connection::open(todo_db_path(temp.path())).unwrap();
        conn.execute(
            "UPDATE tasks SET status = ?1 WHERE id = ?2",
            crate::core::db::params![status, id],
        )
        .unwrap();
        cache_put_claim_status(
            &todo_db_path(temp.path()),
            &id,
            status,
            "release-owner",
            "1Z",
        );
        assert_eq!(
            release_task_as_actor(temp.path(), &id, "release-owner").unwrap()["status"],
            "ok"
        );
        assert_eq!(snapshot(temp.path(), &id)["task"]["status"], status);
        let projected = claim_status(temp.path(), &id).unwrap();
        assert_eq!(
            projected["claim_status"]["task_status"], status,
            "{projected}"
        );
        assert_eq!(projected["claim_status"]["assigned_to"], "");
    }
}

use super::*;
use crate::core::db::Connection;
use serde_json::json;

#[test]
fn budget_validation_rejects_zero_and_over_ceiling_bounds() {
    assert!(ProjectionBudget::default().validate().is_ok());

    let cases: Vec<ProjectionBudget> = vec![
        ProjectionBudget {
            max_rows: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_rows: HARD_MAX_ROWS + 1,
            ..Default::default()
        },
        ProjectionBudget {
            max_response_bytes: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_response_bytes: HARD_MAX_RESPONSE_BYTES + 1,
            ..Default::default()
        },
        ProjectionBudget {
            max_scan_rows: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_scan_rows: HARD_MAX_SCAN_ROWS + 1,
            ..Default::default()
        },
        ProjectionBudget {
            max_worktrees: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_worktrees: HARD_MAX_WORKTREES + 1,
            ..Default::default()
        },
        ProjectionBudget {
            max_files_per_worktree: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_files_per_worktree: HARD_MAX_FILES_PER_WORKTREE + 1,
            ..Default::default()
        },
        ProjectionBudget {
            max_wall_time_ms: 0,
            ..Default::default()
        },
        ProjectionBudget {
            max_wall_time_ms: HARD_MAX_WALL_TIME_MS + 1,
            ..Default::default()
        },
        ProjectionBudget {
            reader_lock_timeout_ms: 0,
            ..Default::default()
        },
        ProjectionBudget {
            reader_lock_timeout_ms: HARD_MAX_READER_LOCK_TIMEOUT_MS + 1,
            ..Default::default()
        },
    ];

    for bad in cases {
        let err = bad.validate().unwrap_err();
        assert!(
            err.to_string().contains("PROJECTION_BUDGET_INVALID"),
            "unexpected error: {err}"
        );
    }
}

#[test]
fn canonical_json_bytes_is_key_order_invariant() {
    let a = json!({
        "z": 1,
        "a": {"b": true, "a": [1, {"y": "two", "x": "one"}]},
        "m": null
    });
    let b = json!({
        "m": null,
        "a": {"a": [1, {"x": "one", "y": "two"}], "b": true},
        "z": 1
    });
    assert_eq!(canonical_json_bytes(&a), canonical_json_bytes(&b));
}

#[test]
fn parse_worktree_porcelain_sorts_and_preserves_flags() {
    let sample = "\
worktree /repo/wt-z
HEAD 2222222222222222222222222222222222222222
detached
prunable gitdir file points to non-existent location

worktree /repo/wt-a
HEAD 1111111111111111111111111111111111111111
branch refs/heads/agent/a/task-1

worktree /repo/bare.git
bare
";
    let entries = parse_worktree_porcelain(sample);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].path, "/repo/bare.git");
    assert!(entries[0].bare);
    assert_eq!(entries[1].path, "/repo/wt-a");
    assert_eq!(entries[1].branch.as_deref(), Some("agent/a/task-1"));
    assert!(!entries[1].detached);
    assert_eq!(entries[2].path, "/repo/wt-z");
    assert!(entries[2].detached);
    assert!(entries[2].prunable);
}

#[test]
fn oversized_first_record_degrades_without_skipping_cursor() {
    let conn = Connection::open(":memory:").unwrap();
    events::ensure_tables(&conn).unwrap();
    events::append_on_conn(
        &conn,
        events::TODO,
        &json!({
            "event_id": "evt-big-1",
            "ts": "2026-11-18T00:00:00Z",
            "event_type": "task.add",
            "task_id": "feat_01",
            "actor": "agent-a",
            "payload": {"blob": "x".repeat(4096)}
        }),
    )
    .unwrap();

    let mut query = EventProjectionQuery::for_stream(events::TODO);
    query.budget.max_response_bytes = 256;

    let page = project_events_on_conn(&conn, &query).unwrap();
    assert_eq!(page.status, ProjectionStatus::Degraded);
    assert!(page.items.is_empty());
    assert!(page.cursor.has_more);
    assert_eq!(page.cursor.next_seq, 0);
    assert!(page.degraded_reasons.iter().any(|r| matches!(
        r,
        DegradedReason::ResponseByteBudgetExceeded {
            budget_bytes: 256,
            ..
        }
    )));
}

#[test]
fn expired_deadline_returns_typed_wall_time_degradation() {
    let conn = Connection::open(":memory:").unwrap();
    events::ensure_tables(&conn).unwrap();
    events::append_on_conn(
        &conn,
        events::TODO,
        &json!({
            "event_id": "evt-1",
            "ts": "2026-11-18T00:00:00Z",
            "event_type": "task.add",
            "task_id": "feat_01",
            "actor": "agent-a"
        }),
    )
    .unwrap();

    let query = EventProjectionQuery::for_stream(events::TODO);
    let now = Instant::now();
    let page = project_events_on_conn_with_deadline(&conn, &query, now, now).unwrap();
    assert_eq!(page.status, ProjectionStatus::Degraded);
    assert!(page.cursor.has_more);
    assert!(
        page.degraded_reasons
            .iter()
            .any(|r| matches!(r, DegradedReason::WallTimeBudgetExceeded { .. }))
    );
}

#[test]
fn query_rejects_unknown_stream_and_half_subject_filter() {
    let conn = Connection::open(":memory:").unwrap();
    events::ensure_tables(&conn).unwrap();

    let bad_stream = EventProjectionQuery::for_stream("not_a_real_stream");
    assert!(project_events_on_conn(&conn, &bad_stream).is_err());

    let mut half_subject = EventProjectionQuery::for_stream(events::TODO);
    half_subject.subject_kind = Some("task".to_string());
    let err = project_events_on_conn(&conn, &half_subject).unwrap_err();
    assert!(err.to_string().contains("PROJECTION_QUERY_INVALID"));
}

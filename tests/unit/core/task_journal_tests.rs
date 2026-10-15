use super::*;

const TASK: &str = "task-journal-subject";
const DEPENDENCY: &str = "task-journal-dependency";
const CREATED: &str = "2026-01-02T03:04:05Z";
const UPDATED: &str = "2026-02-03T04:05:06Z";
const RAW_PLAN: &str = "[ { \"gate\": \"validate_passes\" } ]\n";
const RAW_ARTIFACTS: &str =
    "{ \"task_id\": \"task-journal-subject\", \"task_revision\": 3, \"proof_plan_results\": [] }\n";

fn fixture() -> (tempfile::TempDir, Connection) {
    let temp = tempfile::tempdir().unwrap();
    super::super::initialize_todo_db(temp.path()).unwrap();
    let conn = Connection::open(super::super::todo_db_path(temp.path())).unwrap();
    (temp, conn)
}

fn seed_task(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO tasks(id, hash, title, description, tags, owner, due, ref,
            status, created_at, updated_at, completed_at, closed_at, dir_path,
            scope, parent_task_id, priority, depends_on, blocks, category,
            component, assigned_to, assigned_at, one_shot, lease_expires_at,
            lease_generation, lease_lifecycle, intent_anchor, revision)
         VALUES(?1, ?2, 'Journal subject', 'Exact description\nsecond line',
            'zeta,alpha', 'legacy-owner', '2026-12-31', 'issue:852', 'open',
            ?3, ?4, NULL, NULL, '/work/journal', 'repo', NULL, 'high', '', '',
            'code', 'kernel', 'agent-a', ?3, 1, '9999999999Z',
            2, 'claimed', 'intent:journal', 7)",
        params![id, format!("hash:{id}"), CREATED, UPDATED],
    )
    .unwrap();
}

fn seed_complete_projection(conn: &Connection) -> TaskProjection {
    seed_task(conn, DEPENDENCY);
    seed_task(conn, TASK);
    conn.execute(
        "UPDATE tasks SET parent_task_id=?1, depends_on=?1, blocks='task-blocked',
            completed_at='2026-02-01T00:00:00Z', closed_at='2026-02-02T00:00:00Z'
         WHERE id=?2",
        params![DEPENDENCY, TASK],
    )
    .unwrap();
    for (id, agent, kind) in [
        ("owner-z", "agent-z", "secondary"),
        ("owner-a", "agent-a", "primary"),
    ] {
        conn.execute(
            "INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type)
             VALUES(?1,?2,?3,?4,?5)",
            params![id, TASK, agent, CREATED, kind],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at)
         VALUES('dependency-edge',?1,?2,?3)",
        params![TASK, DEPENDENCY, CREATED],
    )
    .unwrap();
    for tag in ["zeta", "alpha"] {
        conn.execute(
            "INSERT INTO task_tags(task_id,tag) VALUES(?1,?2)",
            params![TASK, tag],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,
            last_verified_at,last_verified_status,last_verified_notes,
            verification_policy_days,updated_at)
         VALUES(?1,?2,?3,?4,'passed','Historical proof\nkeep exactly',17,?5)",
        params![TASK, RAW_PLAN, RAW_ARTIFACTS, CREATED, UPDATED],
    )
    .unwrap();
    TaskProjection {
        task: TaskRow {
            id: TASK.into(),
            hash: format!("hash:{TASK}"),
            title: "Journal subject".into(),
            description: Some("Exact description\nsecond line".into()),
            tags: Some("zeta,alpha".into()),
            owner: Some("legacy-owner".into()),
            due: Some("2026-12-31".into()),
            r#ref: Some("issue:852".into()),
            status: "open".into(),
            created_at: CREATED.into(),
            updated_at: UPDATED.into(),
            completed_at: Some("2026-02-01T00:00:00Z".into()),
            closed_at: Some("2026-02-02T00:00:00Z".into()),
            dir_path: "/work/journal".into(),
            scope: "repo".into(),
            parent_task_id: Some(DEPENDENCY.into()),
            priority: Some("high".into()),
            depends_on: Some(DEPENDENCY.into()),
            blocks: Some("task-blocked".into()),
            category: Some("code".into()),
            component: Some("kernel".into()),
            assigned_to: Some("agent-a".into()),
            assigned_at: Some(CREATED.into()),
            one_shot: Some(1),
            lease_expires_at: Some("9999999999Z".into()),
            lease_generation: Some(2),
            lease_lifecycle: Some("claimed".into()),
            intent_anchor: Some("intent:journal".into()),
            revision: 7,
        },
        owners: vec![
            OwnerRow {
                id: "owner-a".into(),
                agent_id: "agent-a".into(),
                claimed_at: CREATED.into(),
                claim_type: "primary".into(),
            },
            OwnerRow {
                id: "owner-z".into(),
                agent_id: "agent-z".into(),
                claimed_at: CREATED.into(),
                claim_type: "secondary".into(),
            },
        ],
        dependencies: vec![DependencyRow {
            id: "dependency-edge".into(),
            depends_on_task_id: DEPENDENCY.into(),
            created_at: CREATED.into(),
        }],
        tags: vec!["alpha".into(), "zeta".into()],
        verification: Some(VerificationRow {
            proof_plan: RAW_PLAN.into(),
            verification_artifacts: Some(RAW_ARTIFACTS.into()),
            last_verified_at: Some(CREATED.into()),
            last_verified_status: Some("passed".into()),
            last_verified_notes: Some("Historical proof\nkeep exactly".into()),
            verification_policy_days: 17,
            updated_at: UPDATED.into(),
        }),
    }
}

fn transaction<T>(
    conn: &Connection,
    action: impl FnOnce() -> Result<T, DecapodError>,
) -> Result<T, DecapodError> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    match action() {
        Ok(value) => {
            conn.execute_batch("COMMIT")?;
            Ok(value)
        }
        Err(error) => {
            conn.execute_batch("ROLLBACK")?;
            Err(error)
        }
    }
}

fn baseline(conn: &Connection) -> serde_json::Value {
    transaction(conn, || checkpoint(conn, "journal-test")).unwrap()
}

fn domain_event(conn: &Connection, id: &str, operation: &str) -> Result<(), DecapodError> {
    events::append_on_conn(
        conn,
        events::TODO,
        &serde_json::json!({
            "ts": UPDATED,
            "event_id": format!("domain:{id}:{operation}:{}", watermark(conn)?),
            "event_type": operation,
            "status": "success",
            "task_id": id,
            "actor": "journal-test",
            "payload": {"observation": "domain evidence alone is not replay authority"}
        }),
    )?;
    Ok(())
}

fn mutate(
    conn: &Connection,
    id: &str,
    operation: &str,
    action: impl FnOnce() -> Result<(), DecapodError>,
) -> Result<(), DecapodError> {
    transaction(conn, || {
        let ids = vec![id.to_owned()];
        let before = capture_selected(conn, &ids)?;
        let before_watermark = watermark(conn)?;
        action()?;
        domain_event(conn, id, operation)?;
        record_changes(
            conn,
            &before,
            &ids,
            before_watermark,
            operation,
            "journal-test",
        )
    })
}

fn edit_title(conn: &Connection, title: &str) {
    mutate(conn, TASK, "task.edit", || {
        conn.execute(
            "UPDATE tasks SET title=?1, revision=revision+1 WHERE id=?2",
            params![title, TASK],
        )?;
        Ok(())
    })
    .unwrap();
}

fn journal_events(conn: &Connection) -> Vec<events::StoredEvent> {
    checkpoint_events(conn, None)
        .unwrap()
        .into_iter()
        .map(|record| record.event)
        .collect()
}

fn records(conn: &Connection) -> Vec<Checkpoint> {
    journal_events(conn)
        .into_iter()
        .map(|event| serde_json::from_value(event.payload["payload"].clone()).unwrap())
        .collect()
}

fn event_count(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM events WHERE stream='todo'", [], |r| {
        r.get(0)
    })
    .unwrap()
}

fn remove_task(conn: &Connection, id: &str) {
    for (table, column) in [
        ("task_owners", "task_id"),
        ("task_dependencies", "task_id"),
        ("task_tags", "task_id"),
        ("task_verification", "todo_id"),
    ] {
        conn.execute(&format!("DELETE FROM {table} WHERE {column}=?1"), [id])
            .unwrap();
    }
    conn.execute("DELETE FROM tasks WHERE id=?1", [id]).unwrap();
}

fn rewrite_last_record(conn: &Connection, change: impl FnOnce(&mut Checkpoint)) {
    let event = journal_events(conn).pop().unwrap();
    let mut record: Checkpoint = serde_json::from_value(event.payload["payload"].clone()).unwrap();
    change(&mut record);
    record.projection_digest = digest(&record.projection).unwrap();
    record.record_digest = record.hash().unwrap();
    let new_id = format!("task-journal:{}", record.record_digest);
    let mut envelope = event.payload;
    envelope["event_id"] = serde_json::json!(new_id);
    envelope["payload"] = serde_json::to_value(record).unwrap();
    conn.execute(
        "UPDATE events SET event_id=?1, payload=?2 WHERE event_id=?3",
        params![new_id, envelope.to_string(), event.event_id],
    )
    .unwrap();
}

fn assert_conflict(error: DecapodError, expected: &str) {
    let message = error.to_string();
    assert!(message.contains("TASK_JOURNAL_CONFLICT"), "{message}");
    assert!(
        message.contains(expected),
        "expected {expected:?}: {message}"
    );
}

#[test]
fn explicit_baseline_observes_full_projection_without_inventing_legacy_history() {
    let (_temp, conn) = fixture();
    let expected = seed_complete_projection(&conn);
    domain_event(&conn, TASK, "task.legacy_observation").unwrap();
    let before_watermark = watermark(&conn).unwrap();
    let before = verify(&conn).unwrap();
    assert_eq!(before["status"], "conflict");
    assert_eq!(before["records"], 0);
    assert_eq!(before["uncheckpointed_tasks"].as_array().unwrap().len(), 2);
    assert_eq!(capture(&conn).unwrap()[TASK], expected);

    let result = baseline(&conn);
    assert_eq!(result["status"], "ok");
    assert_eq!(result["history_origin"], "observed_current_state_baselines");
    assert_eq!(result["proof_policy"], "preserve_bytes_no_refresh");
    let journal = read_journal(&conn, None).unwrap();
    let record = &journal.heads[TASK];
    assert_eq!(record.projection, expected);
    assert_eq!(record.kind, CheckpointKind::ObservedBaseline);
    assert_eq!(record.ordinal, 1);
    assert_eq!(record.predecessor, None);
    assert_eq!(record.previous_digest, None);
    assert_eq!(record.source_watermark, before_watermark);
    assert_eq!(record.operation, "explicit_current_state_baseline");
    assert_eq!(record.projection_digest, digest(&expected).unwrap());
    assert_eq!(record.record_digest, record.hash().unwrap());
}

#[test]
fn selected_capture_is_canonical_and_excludes_unselected_tasks() {
    let (_temp, conn) = fixture();
    let expected = seed_complete_projection(&conn);
    let selected = capture_selected(&conn, &[TASK.to_owned()]).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[TASK], expected);
    assert!(
        capture_selected(&conn, &["absent".to_owned()])
            .unwrap()
            .is_empty()
    );
    assert!(capture_selected(&conn, &[]).unwrap().is_empty());
}

#[test]
fn mutation_of_legacy_task_records_before_image_then_transition() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    let before = capture(&conn).unwrap()[TASK].clone();
    edit_title(&conn, "First journaled edit");
    let records = records(&conn);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].projection, before);
    assert_eq!(records[0].operation, "observed_before_mutation");
    assert_eq!(records[0].source_watermark, 0);
    assert_eq!(records[1].kind, CheckpointKind::Transition);
    assert_eq!(records[1].ordinal, 2);
    assert_eq!(
        records[1].predecessor.as_deref(),
        Some(records[0].record_digest.as_str())
    );
    assert_eq!(
        records[1].previous_digest.as_deref(),
        Some(records[0].projection_digest.as_str())
    );
    assert_eq!(records[1].projection, capture(&conn).unwrap()[TASK]);
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
}

#[test]
fn new_task_creation_starts_with_observed_current_baseline() {
    let (_temp, conn) = fixture();
    mutate(&conn, TASK, "task.add", || {
        seed_task(&conn, TASK);
        Ok(())
    })
    .unwrap();
    let records = records(&conn);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].kind, CheckpointKind::ObservedBaseline);
    assert_eq!(records[0].operation, "task.add");
    assert_eq!(records[0].projection, capture(&conn).unwrap()[TASK]);
}

#[test]
fn lifecycle_snapshots_track_custody_proof_and_terminal_state_exactly() {
    let (_temp, conn) = fixture();
    seed_complete_projection(&conn);
    baseline(&conn);
    let steps = [
        (
            "task.claim",
            "UPDATE tasks SET status='open', lease_generation=3, lease_lifecycle='claimed', revision=8 WHERE id=?1",
        ),
        (
            "task.lease.renew",
            "UPDATE tasks SET lease_expires_at='10000000000Z', revision=9 WHERE id=?1",
        ),
        (
            "task.handoff",
            "UPDATE tasks SET assigned_to='agent-z', lease_generation=4, revision=10 WHERE id=?1",
        ),
        (
            "task.release",
            "UPDATE tasks SET assigned_to='', assigned_at=NULL, lease_expires_at=NULL, lease_lifecycle='released', revision=11 WHERE id=?1",
        ),
        (
            "task.claim",
            "UPDATE tasks SET assigned_to='agent-b', assigned_at='2026-03-01T00:00:00Z', lease_generation=5, lease_lifecycle='reclaimed', revision=12 WHERE id=?1",
        ),
        (
            "task.verify.capture",
            "UPDATE tasks SET revision=13 WHERE id=?1",
        ),
        (
            "task.done",
            "UPDATE tasks SET status='done', completed_at='2026-03-02T00:00:00Z', revision=14 WHERE id=?1",
        ),
        (
            "task.archive",
            "UPDATE tasks SET status='archived', closed_at='2026-03-03T00:00:00Z', revision=15 WHERE id=?1",
        ),
    ];
    for (index, (operation, sql)) in steps.iter().enumerate() {
        mutate(&conn, TASK, operation, || {
            conn.execute(sql, [TASK])?;
            if *operation == "task.handoff" {
                conn.execute("DELETE FROM task_owners WHERE task_id=?1 AND agent_id='agent-a'", [TASK])?;
                conn.execute("UPDATE task_owners SET claim_type='primary' WHERE task_id=?1", [TASK])?;
            }
            if *operation == "task.release" {
                conn.execute("DELETE FROM task_owners WHERE task_id=?1", [TASK])?;
            }
            if index == 4 {
                conn.execute("INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type) VALUES('owner-b',?1,'agent-b',?2,'primary')", params![TASK, UPDATED])?;
            }
            if *operation == "task.verify.capture" {
                conn.execute("UPDATE task_verification SET verification_artifacts=?1, last_verified_at=?2 WHERE todo_id=?3", params!["{ \"task_id\": \"task-journal-subject\", \"task_revision\": 13 }\n", UPDATED, TASK])?;
            }
            Ok(())
        })
        .unwrap();
        let journal = read_journal(&conn, None).unwrap();
        let head = &journal.heads[TASK];
        assert_eq!(head.ordinal, index as u64 + 2);
        assert_eq!(head.operation, *operation);
        assert_eq!(head.projection, capture(&conn).unwrap()[TASK]);
        assert_eq!(verify(&conn).unwrap()["status"], "ok");
    }
    let journal = read_journal(&conn, None).unwrap();
    let head = &journal.heads[TASK];
    assert_eq!(head.projection.task.status, "archived");
    assert_eq!(head.projection.task.lease_generation, Some(5));
    assert_eq!(head.projection.owners[0].agent_id, "agent-b");
    assert_eq!(head.projection.dependencies.len(), 1);
    assert_eq!(head.projection.tags, ["alpha", "zeta"]);
}

#[test]
fn checkpoint_and_unchanged_mutation_are_idempotent() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    let first = baseline(&conn);
    let events_before = event_count(&conn);
    let second = transaction(&conn, || checkpoint(&conn, "different-observer")).unwrap();
    assert_eq!(first, second);
    assert_eq!(event_count(&conn), events_before);
    transaction(&conn, || {
        let before = capture(&conn)?;
        record_changes(
            &conn,
            &before,
            &[TASK.into()],
            watermark(&conn)?,
            "task.noop",
            "observer",
        )
    })
    .unwrap();
    assert_eq!(event_count(&conn), events_before);
}

#[test]
fn identical_event_retry_is_accepted_but_reusing_identity_with_changed_bytes_fails() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    let event = journal_events(&conn).pop().unwrap();
    assert_eq!(
        events::append_on_conn(&conn, events::TODO, &event.payload).unwrap(),
        event.seq
    );
    assert_eq!(records(&conn).len(), 1);
    let mut changed = event.payload.clone();
    changed["payload"]["operation"] = serde_json::json!("changed-operation");
    let error = events::append_on_conn(&conn, events::TODO, &changed).unwrap_err();
    assert!(error.to_string().contains("EVENT_ID_CONFLICT"));
    assert_eq!(journal_events(&conn)[0].payload, event.payload);
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
}

#[test]
fn malformed_json_and_unmatched_projection_digest_fail_closed() {
    for corruption in ["json", "digest"] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        baseline(&conn);
        let event = journal_events(&conn).pop().unwrap();
        let payload = if corruption == "json" {
            "{\"payload\":".to_owned()
        } else {
            let mut envelope = event.payload;
            envelope["payload"]["projection"]["task"]["title"] =
                serde_json::json!("Changed without digest");
            envelope.to_string()
        };
        conn.execute(
            "UPDATE events SET payload=?1 WHERE event_id=?2",
            params![payload, event.event_id],
        )
        .unwrap();
        assert!(verify(&conn).is_err(), "{corruption}");
        let before = capture(&conn).unwrap();
        assert!(transaction(&conn, || restore(&conn, &digest(&before)?)).is_err());
        assert_eq!(capture(&conn).unwrap(), before);
    }
}

#[test]
fn unsupported_checkpoint_schema_is_rejected_even_with_correct_digests() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    rewrite_last_record(&conn, |record| record.schema_version = SCHEMA_VERSION + 1);
    assert_conflict(verify(&conn).unwrap_err(), "schema");
}

#[test]
fn unknown_checkpoint_fields_are_not_silently_discarded() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    let event = journal_events(&conn).pop().unwrap();
    let mut envelope = event.payload;
    envelope["payload"]["unknown_authority"] = serde_json::json!(true);
    conn.execute(
        "UPDATE events SET payload=?1 WHERE event_id=?2",
        params![envelope.to_string(), event.event_id],
    )
    .unwrap();
    assert_conflict(verify(&conn).unwrap_err(), "invalid checkpoint");
}

#[test]
fn unsupported_projection_table_columns_require_explicit_schema_migration() {
    for table in [
        "tasks",
        "task_owners",
        "task_dependencies",
        "task_tags",
        "task_verification",
    ] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        baseline(&conn);
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN unversioned_field TEXT"),
            [],
        )
        .unwrap();
        assert_conflict(capture(&conn).unwrap_err(), "unsupported");
        assert!(verify(&conn).is_err(), "{table}");
    }
}

#[test]
fn missing_baseline_or_middle_checkpoint_is_not_reconstructible() {
    for removed in [0, 1] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        baseline(&conn);
        edit_title(&conn, "Second state");
        edit_title(&conn, "Third state");
        let event = &journal_events(&conn)[removed];
        conn.execute("DELETE FROM events WHERE event_id=?1", [&event.event_id])
            .unwrap();
        assert!(verify(&conn).is_err());
        let before = capture(&conn).unwrap();
        assert!(transaction(&conn, || restore(&conn, &digest(&before)?)).is_err());
        assert_eq!(capture(&conn).unwrap(), before);
    }
}

#[test]
fn rehashed_revision_generation_watermark_and_predecessor_regressions_are_rejected() {
    for field in [
        "revision",
        "generation",
        "watermark",
        "predecessor",
        "previous_digest",
        "ordinal",
        "kind",
    ] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        domain_event(&conn, TASK, "task.legacy").unwrap();
        baseline(&conn);
        edit_title(&conn, "Second state");
        rewrite_last_record(&conn, |record| match field {
            "revision" => record.projection.task.revision = 6,
            "generation" => record.projection.task.lease_generation = Some(1),
            "watermark" => record.source_watermark = 0,
            "predecessor" => record.predecessor = Some("0".repeat(64)),
            "previous_digest" => record.previous_digest = Some("0".repeat(64)),
            "ordinal" => record.ordinal = 9,
            "kind" => record.kind = CheckpointKind::ObservedBaseline,
            _ => unreachable!(),
        });
        assert_conflict(verify(&conn).unwrap_err(), "chain");
    }
}

#[test]
fn checkpoint_envelope_and_canonical_row_custody_must_agree() {
    for column in ["actor", "subject_id", "subject_kind", "ts"] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        baseline(&conn);
        conn.execute(
            &format!("UPDATE events SET {column}='tampered' WHERE event_type=?1"),
            [EVENT_TYPE],
        )
        .unwrap();
        assert!(
            verify(&conn).is_err(),
            "canonical {column} mismatch was accepted"
        );
    }
}

#[test]
fn negative_revision_and_generation_are_rejected_before_checkpoint() {
    for column in ["revision", "lease_generation"] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        conn.execute(&format!("UPDATE tasks SET {column}=-1 WHERE id=?1"), [TASK])
            .unwrap();
        assert_conflict(capture(&conn).unwrap_err(), "negative");
        assert!(transaction(&conn, || checkpoint(&conn, "observer")).is_err());
        assert_eq!(event_count(&conn), 0);
    }
}

#[test]
fn a_failed_transition_rolls_back_domain_mutation_baseline_and_success_event() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    let before = capture(&conn).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_transition BEFORE INSERT ON events
         WHEN NEW.event_type='task.journal.checkpoint'
           AND json_extract(NEW.payload,'$.payload.kind')='transition'
         BEGIN SELECT RAISE(ABORT,'injected transition failure'); END",
    )
    .unwrap();
    let result = mutate(&conn, TASK, "task.edit", || {
        conn.execute(
            "UPDATE tasks SET title='Must roll back', revision=revision+1 WHERE id=?1",
            [TASK],
        )?;
        Ok(())
    });
    assert!(result.is_err());
    assert!(conn.is_autocommit());
    assert_eq!(capture(&conn).unwrap(), before);
    assert_eq!(event_count(&conn), 0);
    assert_eq!(read_journal(&conn, None).unwrap().records, 0);
    conn.execute_batch("DROP TRIGGER reject_transition")
        .unwrap();
    edit_title(&conn, "Retry succeeds");
    assert_eq!(read_journal(&conn, None).unwrap().records, 2);
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
}

#[test]
fn divergent_before_image_fails_and_rolls_back_attempted_mutation() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    conn.execute(
        "UPDATE tasks SET title='Unjournaled authoritative edit' WHERE id=?1",
        [TASK],
    )
    .unwrap();
    let before = capture(&conn).unwrap();
    let events_before = event_count(&conn);
    let result = mutate(&conn, TASK, "task.edit", || {
        conn.execute(
            "UPDATE tasks SET title='Attempted overwrite', revision=revision+1 WHERE id=?1",
            [TASK],
        )?;
        Ok(())
    });
    assert_conflict(result.unwrap_err(), "diverged");
    assert_eq!(capture(&conn).unwrap(), before);
    assert_eq!(event_count(&conn), events_before);
    assert!(transaction(&conn, || checkpoint(&conn, "observer")).is_err());
}

#[test]
fn task_deletion_requires_a_tombstone_contract_and_rolls_back() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    let before = capture(&conn).unwrap();
    let events_before = event_count(&conn);
    let result = mutate(&conn, TASK, "task.delete", || {
        remove_task(&conn, TASK);
        Ok(())
    });
    assert_conflict(result.unwrap_err(), "tombstone");
    assert_eq!(capture(&conn).unwrap(), before);
    assert_eq!(event_count(&conn), events_before);
}

#[test]
fn restore_recreates_absent_projection_exactly_and_is_idempotent() {
    let (_temp, conn) = fixture();
    let expected = seed_complete_projection(&conn);
    baseline(&conn);
    let all_before = capture(&conn).unwrap();
    let events_before = event_count(&conn);
    remove_task(&conn, TASK);
    let reviewed = digest(&capture(&conn).unwrap()).unwrap();
    let result = transaction(&conn, || restore(&conn, &reviewed)).unwrap();
    assert_eq!(result["restored_tasks"], serde_json::json!([TASK]));
    assert_eq!(result["status"], "ok");
    assert_eq!(capture(&conn).unwrap()[TASK], expected);
    assert_eq!(capture(&conn).unwrap(), all_before);
    let reviewed = digest(&all_before).unwrap();
    let repeated = transaction(&conn, || restore(&conn, &reviewed)).unwrap();
    assert_eq!(repeated["restored_tasks"], serde_json::json!([]));
    assert_eq!(event_count(&conn), events_before);
}

#[test]
fn restore_accepts_an_exact_known_predecessor_without_changing_unrelated_tasks() {
    let (_temp, conn) = fixture();
    seed_complete_projection(&conn);
    baseline(&conn);
    let predecessor = capture(&conn).unwrap()[TASK].clone();
    edit_title(&conn, "Current journal head");
    let expected = capture(&conn).unwrap();
    write_task(&conn, &predecessor.task).unwrap();
    write_children(&conn, &predecessor).unwrap();
    let reviewed = digest(&capture(&conn).unwrap()).unwrap();
    let result = transaction(&conn, || restore(&conn, &reviewed)).unwrap();
    assert_eq!(result["restored_tasks"], serde_json::json!([TASK]));
    assert_eq!(capture(&conn).unwrap(), expected);
}

#[test]
fn restore_refuses_divergent_live_authority_before_restoring_any_missing_task() {
    let (_temp, conn) = fixture();
    seed_task(&conn, "a-missing");
    seed_task(&conn, "z-divergent");
    baseline(&conn);
    remove_task(&conn, "a-missing");
    conn.execute(
        "UPDATE tasks SET title='Independent live authority' WHERE id='z-divergent'",
        [],
    )
    .unwrap();
    let before = capture(&conn).unwrap();
    let reviewed = digest(&before).unwrap();
    let error = transaction(&conn, || restore(&conn, &reviewed)).unwrap_err();
    assert_conflict(error, "divergent live authority");
    assert_eq!(capture(&conn).unwrap(), before);
    let report = verify(&conn).unwrap();
    assert_eq!(report["missing_tasks"], serde_json::json!(["a-missing"]));
    assert_eq!(
        report["divergent_tasks"],
        serde_json::json!(["z-divergent"])
    );
}

#[test]
fn restore_requires_the_exact_current_projection_review_digest() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    let old_review = digest(&capture(&conn).unwrap()).unwrap();
    remove_task(&conn, TASK);
    let before = capture(&conn).unwrap();
    for stale_or_invalid in [old_review, String::new(), "0".repeat(64)] {
        assert_conflict(
            transaction(&conn, || restore(&conn, &stale_or_invalid)).unwrap_err(),
            "changed since recovery review",
        );
        assert_eq!(capture(&conn).unwrap(), before);
    }
}

#[test]
fn restore_does_not_refresh_historical_stale_proof_bytes_or_authority() {
    let (_temp, conn) = fixture();
    let expected = seed_complete_projection(&conn);
    baseline(&conn);
    remove_task(&conn, TASK);
    let reviewed = digest(&capture(&conn).unwrap()).unwrap();
    transaction(&conn, || restore(&conn, &reviewed)).unwrap();
    let recovered = capture(&conn).unwrap()[TASK].clone();
    assert_eq!(recovered.verification, expected.verification);
    let proof = recovered.verification.unwrap();
    assert_eq!(proof.proof_plan, RAW_PLAN);
    assert_eq!(proof.verification_artifacts.as_deref(), Some(RAW_ARTIFACTS));
    assert_eq!(proof.last_verified_at.as_deref(), Some(CREATED));
    assert_eq!(proof.updated_at, UPDATED);
    let artifacts = serde_json::from_str::<serde_json::Value>(RAW_ARTIFACTS).unwrap();
    assert_eq!(
        super::super::effective_verification_status(
            proof.last_verified_status.as_deref(),
            Some(&artifacts),
            TASK,
            recovered.task.revision,
        ),
        Some("stale")
    );
    assert_eq!(
        super::super::effective_verification_status(None, None, TASK, recovered.task.revision),
        None
    );
}

#[test]
fn mutations_require_caller_transaction_even_for_noop_inputs() {
    let (_temp, conn) = fixture();
    let before = capture(&conn).unwrap();
    assert_conflict(checkpoint(&conn, "observer").unwrap_err(), "caller");
    assert_conflict(
        record_changes(&conn, &before, &[], 0, "noop", "observer").unwrap_err(),
        "caller",
    );
    assert_conflict(
        restore(&conn, &digest(&before).unwrap()).unwrap_err(),
        "caller",
    );
    assert!(conn.is_autocommit());
    assert_eq!(event_count(&conn), 0);
}

#[test]
fn bounded_task_selection_and_oversized_journal_fail_before_replay() {
    let (_temp, conn) = fixture();
    let oversized = vec![TASK.to_owned(); MAX_TASKS + 1];
    assert_conflict(capture_selected(&conn, &oversized).unwrap_err(), "bounded");
    seed_task(&conn, TASK);
    baseline(&conn);
    // This intentionally malformed record exceeds the byte budget; size rejection
    // must happen before JSON parsing or materializing a replay projection.
    conn.execute(
        "UPDATE events SET payload=?1 WHERE event_type=?2",
        params!["x".repeat(MAX_RECORD_BYTES + 1), EVENT_TYPE],
    )
    .unwrap();
    assert_conflict(verify(&conn).unwrap_err(), "bounded");
}

#[test]
fn rehashed_noncanonical_children_cannot_be_replayed() {
    for child in ["owners", "dependencies", "tags"] {
        let (_temp, conn) = fixture();
        seed_complete_projection(&conn);
        baseline(&conn);
        // TASK sorts after DEPENDENCY and therefore owns the last record.
        rewrite_last_record(&conn, |record| match child {
            "owners" => record.projection.owners.reverse(),
            "dependencies" => record
                .projection
                .dependencies
                .push(record.projection.dependencies[0].clone()),
            "tags" => record.projection.tags.reverse(),
            _ => unreachable!(),
        });
        assert_conflict(verify(&conn).unwrap_err(), "noncanonical");
    }
}

#[test]
fn restore_refuses_missing_dependency_targets_before_writing() {
    let (_temp, conn) = fixture();
    let expected = seed_complete_projection(&conn);
    transaction(&conn, || {
        append_checkpoint(
            &conn,
            &mut Journal::default(),
            &expected,
            0,
            "explicit_current_state_baseline",
            "observer",
        )
    })
    .unwrap();
    remove_task(&conn, TASK);
    remove_task(&conn, DEPENDENCY);
    let before = capture(&conn).unwrap();
    assert_conflict(
        transaction(&conn, || restore(&conn, &digest(&before)?)).unwrap_err(),
        "dependency target is missing",
    );
    assert_eq!(capture(&conn).unwrap(), before);
}

fn seed_active_execution_custody(conn: &Connection) {
    seed_task(conn, TASK);
    conn.execute(
        "UPDATE tasks SET status='open', lease_lifecycle='claimed',
            lease_expires_at='9999999999Z' WHERE id=?1",
        [TASK],
    )
    .unwrap();
}

#[test]
fn execution_custody_capture_is_stable_read_only_and_bound_to_current_actor() {
    let (temp, conn) = fixture();
    seed_active_execution_custody(&conn);
    baseline(&conn);
    let before = capture(&conn).unwrap();
    let events_before = event_count(&conn);
    let first = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
    let second = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
    assert_eq!(first, second);
    assert_eq!(first["task_id"], TASK);
    assert_eq!(first["assignee"], "agent-a");
    assert_eq!(first["revision"], 7);
    assert_eq!(first["lease_generation"], 2);
    assert_eq!(first["lease_expires_at"], "9999999999Z");
    assert_eq!(first["lease_lifecycle"], "claimed");
    assert_eq!(first["intent_anchor"], "intent:journal");
    assert_eq!(first["dependency_readiness"]["state"], "ready");
    for actor in ["agent-b", "", " "] {
        let error = super::super::capture_execution_custody(temp.path(), TASK, actor).unwrap_err();
        assert!(error.to_string().contains("EXECUTION_CUSTODY_STALE"));
    }
    assert_eq!(capture(&conn).unwrap(), before);
    assert_eq!(event_count(&conn), events_before);
}

#[test]
fn execution_custody_rejects_absent_expired_yielded_and_unleased_tasks() {
    let invalid_states = [
        "lease_expires_at='1Z'",
        "lease_expires_at=NULL",
        "lease_expires_at='not-a-timestamp'",
        "lease_lifecycle='yielded'",
        "lease_lifecycle='released'",
        "lease_lifecycle='unspecified'",
        "lease_generation=0",
        "assigned_to=''",
        "status='done'",
        "status='archived'",
    ];
    for state in invalid_states {
        let (temp, conn) = fixture();
        seed_active_execution_custody(&conn);
        conn.execute(&format!("UPDATE tasks SET {state} WHERE id=?1"), [TASK])
            .unwrap();
        baseline(&conn);
        let before = capture(&conn).unwrap();
        let error =
            super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
        assert!(
            error.to_string().contains("EXECUTION_CUSTODY_STALE"),
            "{state}: {error}"
        );
        assert_eq!(capture(&conn).unwrap(), before);
    }
    let (temp, _conn) = fixture();
    let error = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
    assert!(error.to_string().contains("EXECUTION_CUSTODY_MISSING"));
}

#[test]
fn execution_custody_rechecks_dependency_readiness() {
    let (temp, conn) = fixture();
    seed_active_execution_custody(&conn);
    seed_task(&conn, DEPENDENCY);
    conn.execute(
        "UPDATE tasks SET depends_on=?1 WHERE id=?2",
        params![DEPENDENCY, TASK],
    )
    .unwrap();
    conn.execute("INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at) VALUES('execution-dependency',?1,?2,?3)", params![TASK, DEPENDENCY, CREATED]).unwrap();
    baseline(&conn);
    let events_before = event_count(&conn);
    let error = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
    assert!(error.to_string().contains("EXECUTION_DEPENDENCIES_BLOCKED"));
    assert_eq!(event_count(&conn), events_before);

    mutate(&conn, DEPENDENCY, "task.done", || {
        conn.execute("UPDATE tasks SET status='done', revision=revision+1 WHERE id=?1", [DEPENDENCY])?;
        conn.execute("INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_status,updated_at) VALUES(?1,'[]',?2,'passed',?3)", params![DEPENDENCY, serde_json::json!({"task_id": DEPENDENCY, "task_revision": 8, "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": "sha256:dependency-proof"}]}).to_string(), UPDATED])?;
        Ok(())
    })
    .unwrap();
    let ready = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
    assert_eq!(ready["dependency_readiness"]["state"], "ready");

    mutate(&conn, DEPENDENCY, "task.edit", || {
        conn.execute("UPDATE tasks SET description='Changed dependency acceptance', revision=revision+1 WHERE id=?1", [DEPENDENCY])?;
        Ok(())
    })
    .unwrap();
    let stale_proof =
        super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
    assert!(
        stale_proof
            .to_string()
            .contains("EXECUTION_DEPENDENCIES_BLOCKED")
    );
}

#[test]
fn live_revision_or_generation_regression_rolls_back_without_extending_journal() {
    for assignment in ["revision=6", "lease_generation=1"] {
        let (_temp, conn) = fixture();
        seed_task(&conn, TASK);
        baseline(&conn);
        let before = capture(&conn).unwrap();
        let events_before = event_count(&conn);
        let result = mutate(&conn, TASK, "task.regression", || {
            conn.execute(
                &format!("UPDATE tasks SET {assignment} WHERE id=?1"),
                [TASK],
            )?;
            Ok(())
        });
        assert_conflict(result.unwrap_err(), "regressed");
        assert_eq!(capture(&conn).unwrap(), before);
        assert_eq!(event_count(&conn), events_before);
    }
}

#[test]
fn todo_mutation_entrypoints_keep_replay_head_current_through_completion_and_archive() {
    let (temp, conn) = fixture();
    let added = super::super::add_task(
        temp.path(),
        &super::super::TodoCommand::Add {
            title: "Journal lifecycle integration unique".into(),
            description: "Exercise governed todo mutation integration".into(),
            priority: "medium".into(),
            tags: "journal,integration".into(),
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
    let id = added["id"].as_str().unwrap();
    let assert_current = || {
        let journal = read_journal(&conn, None).unwrap();
        assert_eq!(journal.heads[id].projection, capture(&conn).unwrap()[id]);
        assert_eq!(verify(&conn).unwrap()["status"], "ok");
    };
    assert_current();
    assert_eq!(read_journal(&conn, None).unwrap().heads[id].ordinal, 1);

    let edited = super::super::edit_task(
        temp.path(),
        id,
        None,
        Some("Updated acceptance"),
        None,
        None,
    )
    .unwrap();
    assert_eq!(edited["status"], "ok");
    assert_current();
    let claimed = super::super::claim_task_with_lease(
        temp.path(),
        id,
        "journal-owner",
        super::super::ClaimMode::Exclusive,
        Some(3600),
        Some("intent:journal-integration".into()),
    )
    .unwrap();
    assert_eq!(claimed["status"], "ok", "{claimed}");
    assert_current();
    let renewed =
        super::super::renew_claim_lease(temp.path(), id, "journal-owner", Some(7200)).unwrap();
    assert_eq!(renewed["status"], "ok", "{renewed}");
    assert_current();
    let released = super::super::release_task_as_actor(temp.path(), id, "journal-owner").unwrap();
    assert_eq!(released["status"], "ok", "{released}");
    assert_current();
    let store = crate::core::store::Store {
        kind: crate::core::store::StoreKind::Repo,
        root: temp.path().to_path_buf(),
    };
    for (status, event_type) in [("done", "task.done"), ("archived", "task.archive")] {
        let before = capture(&conn).unwrap()[id].task.clone();
        let result = super::super::update_status(
            &store,
            id,
            before.revision,
            &before.status,
            status,
            event_type,
            serde_json::json!({"actor": "journal-owner"}),
        )
        .unwrap();
        assert_eq!(result["status"], "ok", "{result}");
        assert_current();
    }
    let head = read_journal(&conn, None).unwrap().heads[id].clone();
    assert_eq!(head.ordinal, 7);
    assert_eq!(head.projection.task.status, "archived");
    assert!(head.projection.task.completed_at.is_some());
    assert!(head.projection.task.closed_at.is_some());
}

#[test]
fn missing_latest_checkpoint_with_surviving_mutation_cannot_restore_older_authority() {
    let (_temp, conn) = fixture();
    seed_task(&conn, TASK);
    baseline(&conn);
    edit_title(&conn, "Newer committed authority");
    let latest = journal_events(&conn).pop().unwrap();
    assert_eq!(records(&conn).len(), 2);
    // Simulate interrupted/corrupt storage: the newer domain mutation survives,
    // but its replay checkpoint and the materialized task are both absent.
    conn.execute("DELETE FROM events WHERE event_id=?1", [&latest.event_id])
        .unwrap();
    remove_task(&conn, TASK);
    let surviving_edit: i64 = conn.query_row(
        "SELECT COUNT(*) FROM events WHERE stream='todo' AND subject_id=?1 AND event_type='task.edit'",
        [TASK], |row| row.get(0),
    ).unwrap();
    assert_eq!(surviving_edit, 1);
    let before = capture(&conn).unwrap();
    let reviewed = digest(&before).unwrap();
    let verification_error = verify(&conn).unwrap_err();
    assert!(
        verification_error
            .to_string()
            .contains("TASK_JOURNAL_INCOMPLETE_HISTORY")
    );
    let restoration_error = transaction(&conn, || restore(&conn, &reviewed)).unwrap_err();
    assert!(
        restoration_error
            .to_string()
            .contains("TASK_JOURNAL_INCOMPLETE_HISTORY")
    );
    assert_eq!(capture(&conn).unwrap(), before);
    assert!(!capture(&conn).unwrap().contains_key(TASK));
}

// Capture every fixture column dynamically, including deliberately incompatible
// task schemas. SQL quoting preserves nulls and the exact text of proof/events.
fn initialization_snapshot(conn: &Connection) -> serde_json::Value {
    let mut tables = BTreeMap::new();
    for table in [
        "tasks",
        "task_owners",
        "task_dependencies",
        "task_tags",
        "task_verification",
        "events",
        "meta",
    ] {
        let columns = conn.columns(table).unwrap();
        let select = columns
            .iter()
            .map(|column| format!("quote(\"{}\")", column.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let mut stmt = conn
            .prepare(&format!("SELECT {select} FROM {table} ORDER BY rowid"))
            .unwrap();
        let rows = stmt
            .query_map([], |row| {
                (0..columns.len())
                    .map(|index| row.get::<_, String>(index))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        tables.insert(table, serde_json::json!({"columns":columns,"rows":rows}));
    }
    let mut stmt = conn
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")
        .unwrap();
    let schema = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    serde_json::json!({"tables":tables,"schema":schema})
}

#[test]
fn unchanged_schema_initialization_preserves_current_projection_and_journal() {
    let (_temp, conn) = fixture();
    seed_complete_projection(&conn);
    baseline(&conn);
    let before = initialization_snapshot(&conn);
    super::super::ensure_schema(&conn).unwrap();
    assert_eq!(initialization_snapshot(&conn), before);
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
}

#[test]
fn journaled_schema_downgrade_and_duplicate_owners_fail_before_any_repair() {
    for corruption in ["metadata", "owners", "both"] {
        let (_temp, conn) = fixture();
        seed_complete_projection(&conn);
        baseline(&conn);
        if corruption != "owners" {
            conn.execute(
                "UPDATE meta SET value='0' WHERE namespace=?1 AND key='schema_version'",
                [crate::core::schemas::TODO_META_NAMESPACE],
            )
            .unwrap();
        }
        if corruption != "metadata" {
            conn.execute_batch("DROP INDEX idx_task_owners_task_agent")
                .unwrap();
            conn.execute(
                "INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type)
                 VALUES('duplicate-owner',?1,'agent-a','different-claim-time','secondary')",
                [TASK],
            )
            .unwrap();
        }
        let before = initialization_snapshot(&conn);
        let error = super::super::ensure_schema(&conn).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("TASK_JOURNAL_MIGRATION_REQUIRED"),
            "{corruption}: {error}"
        );
        assert_eq!(initialization_snapshot(&conn), before, "{corruption}");
        if corruption != "owners" {
            let version: String = conn
                .query_row(
                    "SELECT value FROM meta WHERE namespace=?1 AND key='schema_version'",
                    [crate::core::schemas::TODO_META_NAMESPACE],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(version, "0");
        }
        if corruption != "metadata" {
            let duplicates: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM task_owners WHERE task_id=?1 AND agent_id='agent-a'",
                    [TASK],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                duplicates, 2,
                "initialization must not delete conflicting custody"
            );
        }
    }
}

#[test]
fn missing_journaled_task_columns_fail_before_automatic_alter() {
    for column in ["lease_lifecycle", "revision"] {
        let (_temp, conn) = fixture();
        seed_complete_projection(&conn);
        baseline(&conn);
        conn.execute(&format!("ALTER TABLE tasks DROP COLUMN {column}"), [])
            .unwrap();
        let before = initialization_snapshot(&conn);
        let error = super::super::ensure_schema(&conn).unwrap_err();
        assert_conflict(error, "unsupported tasks schema");
        assert!(!conn.has_column("tasks", column).unwrap());
        assert_eq!(initialization_snapshot(&conn), before, "{column}");
    }
}

#[test]
fn migration_guard_allows_only_prejournal_state_and_never_changes_evidence() {
    let (_temp, conn) = fixture();
    seed_complete_projection(&conn);
    domain_event(&conn, TASK, "task.legacy").unwrap();
    let before_genesis = initialization_snapshot(&conn);
    assert!(!super::super::has_task_journal(&conn).unwrap());
    super::super::refuse_task_journal_migration(&conn, "legacy owner migration").unwrap();
    assert_eq!(initialization_snapshot(&conn), before_genesis);

    baseline(&conn);
    let after_genesis = initialization_snapshot(&conn);
    assert!(super::super::has_task_journal(&conn).unwrap());
    let error =
        super::super::refuse_task_journal_migration(&conn, "legacy owner migration").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("TASK_JOURNAL_MIGRATION_REQUIRED"));
    assert!(message.contains("legacy owner migration"));
    assert_eq!(initialization_snapshot(&conn), after_genesis);

    // Corrupt evidence still establishes genesis; a parse failure cannot reopen
    // the implicit historical-migration path or authorize dropping the journal.
    conn.execute(
        "UPDATE events SET payload='corrupt checkpoint' WHERE event_type=?1",
        [EVENT_TYPE],
    )
    .unwrap();
    let corrupt = initialization_snapshot(&conn);
    assert!(super::super::refuse_task_journal_migration(&conn, "legacy owner migration").is_err());
    assert_eq!(initialization_snapshot(&conn), corrupt);
}

#[test]
fn execution_custody_refuses_raw_task_drift_after_journal_genesis() {
    let (temp, conn) = fixture();
    seed_active_execution_custody(&conn);
    baseline(&conn);
    super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
    // The lease still looks valid, but the task has diverged from its committed
    // journal authority. Observation must neither accept nor repair that drift.
    conn.execute(
        "UPDATE tasks SET description='Unjournaled acceptance change' WHERE id=?1",
        [TASK],
    )
    .unwrap();
    let before = initialization_snapshot(&conn);
    let error = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
    assert_conflict(error, "divergent journaled authority");
    assert_eq!(initialization_snapshot(&conn), before);
}

#[test]
fn execution_custody_refuses_direct_dependency_proof_byte_drift() {
    let (temp, conn) = fixture();
    seed_active_execution_custody(&conn);
    seed_task(&conn, DEPENDENCY);
    conn.execute("UPDATE tasks SET status='done' WHERE id=?1", [DEPENDENCY])
        .unwrap();
    conn.execute(
        "UPDATE tasks SET depends_on=?1 WHERE id=?2",
        params![DEPENDENCY, TASK],
    )
    .unwrap();
    conn.execute("INSERT INTO task_dependencies(id,task_id,depends_on_task_id,created_at) VALUES('custody-proof-dependency',?1,?2,?3)", params![TASK, DEPENDENCY, CREATED]).unwrap();
    let artifacts = serde_json::json!({
        "task_id": DEPENDENCY,
        "task_revision": 7,
        "proof_plan_results": [{"proof_gate": "validate_passes", "output_hash": "sha256:dependency-proof"}]
    });
    conn.execute("INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_status,updated_at) VALUES(?1,'[]',?2,'passed',?3)", params![DEPENDENCY, artifacts.to_string(), UPDATED]).unwrap();
    baseline(&conn);
    let initial = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
    assert_eq!(initial["dependency_readiness"]["state"], "ready");

    // Whitespace keeps the parsed proof binding valid. Only the journal's exact
    // proof-byte custody detects this deliberately unjournaled modification.
    conn.execute(
        "UPDATE task_verification SET verification_artifacts=verification_artifacts || ' ' WHERE todo_id=?1",
        [DEPENDENCY],
    ).unwrap();
    let proof_raw: String = conn
        .query_row(
            "SELECT verification_artifacts FROM task_verification WHERE todo_id=?1",
            [DEPENDENCY],
            |row| row.get(0),
        )
        .unwrap();
    let parsed = serde_json::from_str::<serde_json::Value>(&proof_raw).unwrap();
    assert_eq!(
        super::super::effective_verification_status(Some("passed"), Some(&parsed), DEPENDENCY, 7),
        Some("passed")
    );
    let before = initialization_snapshot(&conn);
    let error = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap_err();
    assert_conflict(error, "divergent journaled authority");
    assert_eq!(initialization_snapshot(&conn), before);
}

#[test]
fn execution_custody_allows_unjournaled_legacy_subject_without_writing_events() {
    for unrelated_journal in [false, true] {
        let (temp, conn) = fixture();
        seed_active_execution_custody(&conn);
        domain_event(&conn, TASK, "task.claim").unwrap();
        if unrelated_journal {
            seed_task(&conn, DEPENDENCY);
            let observed = capture_selected(&conn, &[DEPENDENCY.to_owned()]).unwrap();
            transaction(&conn, || {
                append_checkpoint(
                    &conn,
                    &mut Journal::default(),
                    &observed[DEPENDENCY],
                    watermark(&conn)?,
                    "explicit_current_state_baseline",
                    "observer",
                )
            })
            .unwrap();
        }
        assert!(!read_journal(&conn, None).unwrap().heads.contains_key(TASK));
        let before = initialization_snapshot(&conn);
        let result = super::super::capture_execution_custody(temp.path(), TASK, "agent-a").unwrap();
        assert_eq!(result["task_id"], TASK);
        assert_eq!(result["assignee"], "agent-a");
        assert_eq!(result["lease_generation"], 2);
        assert_eq!(result["dependency_readiness"]["state"], "ready");
        assert_eq!(initialization_snapshot(&conn), before);
        assert!(!read_journal(&conn, None).unwrap().heads.contains_key(TASK));
    }
}

#[test]
fn legacy_jsonl_reducer_cannot_write_a_modern_journaled_target() {
    let (temp, conn) = fixture();
    seed_complete_projection(&conn);
    baseline(&conn);
    let mut before = initialization_snapshot(&conn);
    before["tables"].as_object_mut().unwrap().remove("events");
    let journal_before = records(&conn);
    let todo_events_before = event_count(&conn);
    let input = temp.path().join("legacy-replay.jsonl");
    std::fs::write(
        &input,
        serde_json::json!({
            "ts": UPDATED, "event_id": "legacy-replay-edit", "event_type": "task.edit",
            "status": "success", "task_id": TASK, "actor": "test",
            "payload": {"title":"Must not replace modern authority"}
        })
        .to_string(),
    )
    .unwrap();
    let error =
        super::super::rebuild_db_from_events(&input, &super::super::todo_db_path(temp.path()))
            .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("TASK_JOURNAL_MIGRATION_REQUIRED")
    );
    let mut after = initialization_snapshot(&conn);
    after["tables"].as_object_mut().unwrap().remove("events");
    assert_eq!(after, before);
    assert_eq!(records(&conn), journal_before);
    assert_eq!(event_count(&conn), todo_events_before);
    // Broker rejection audit is intentionally separate from task authority.
}

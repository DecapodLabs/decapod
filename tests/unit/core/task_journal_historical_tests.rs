use super::*;

type Binding = super::super::HistoricalTaskBinding;

const TASK: &str = "historical-validation-task";
const ACTOR: &str = "historical-validation-owner";
const CREATED: &str = "2026-01-02T03:04:05Z";
const UPDATED: &str = "2026-02-03T04:05:06Z";
const RAW_ARTIFACTS: &str = "{ \"original\": \"proof bytes stay immutable\" }\n";

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

fn fixture() -> (tempfile::TempDir, Connection, Binding) {
    fixture_with_lease("9999999999Z")
}

fn fixture_with_lease(expires_at: &str) -> (tempfile::TempDir, Connection, Binding) {
    let temp = tempfile::tempdir().unwrap();
    super::super::initialize_todo_db(temp.path()).unwrap();
    let conn = Connection::open(super::super::todo_db_path(temp.path())).unwrap();
    conn.execute(
        "INSERT INTO tasks(id, hash, title, description, tags, owner, due, ref,
            status, created_at, updated_at, completed_at, closed_at, dir_path,
            scope, parent_task_id, priority, depends_on, blocks, category,
            component, assigned_to, assigned_at, one_shot, lease_expires_at,
            lease_generation, lease_lifecycle, intent_anchor, revision)
         VALUES(?1, 'historical-task-hash', 'Validated task intent', 'Original scope',
            '', ?2, NULL, 'issue:1610', 'in_progress', ?3, ?3, NULL, NULL,
            '/work/historical-validation', 'repo', NULL, 'high', '', '',
            'code', 'kernel', ?2, ?3, 1, ?4,
            2, 'claimed', 'intent:historical-validation', 7)",
        params![TASK, ACTOR, CREATED, expires_at],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_owners(id,task_id,agent_id,claimed_at,claim_type)
         VALUES('historical-owner',?1,?2,?3,'primary')",
        params![TASK, ACTOR, CREATED],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,
            last_verified_at,last_verified_status,last_verified_notes,
            verification_policy_days,updated_at)
         VALUES(?1,'[\"validate_passes\"]',?2,?3,'pass','original baseline',90,?3)",
        params![TASK, RAW_ARTIFACTS, CREATED],
    )
    .unwrap();
    transaction(&conn, || checkpoint(&conn, ACTOR)).unwrap();

    // These tests isolate replay of an already authenticated binding. They do
    // not mutate the process-global agent identity to bypass capture custody.
    let journal = read_journal(&conn, Some(&[TASK.to_owned()])).unwrap();
    let head = &journal.heads[TASK];
    let binding = Binding {
        task_id: TASK.into(),
        revision: head.projection.task.revision,
        status: head.projection.task.status.clone(),
        contract_digest: validation_contract_digest(&head.projection).unwrap(),
        checkpoint_digest: head.record_digest.clone(),
        assignee: ACTOR.into(),
        lease_generation: 2,
    };
    (temp, conn, binding)
}

fn domain_event(
    conn: &Connection,
    task_id: Option<&str>,
    operation: &str,
) -> Result<(), DecapodError> {
    events::append_on_conn(
        conn,
        events::TODO,
        &serde_json::json!({
            "ts": UPDATED,
            "event_id": format!("historical-test:{operation}:{}", watermark(conn)?),
            "event_type": operation,
            "status": "success",
            "task_id": task_id,
            "actor": ACTOR,
            "payload": {"test": "task chronology"}
        }),
    )?;
    Ok(())
}

fn mutate(conn: &Connection, operation: &str, action: impl FnOnce() -> Result<(), DecapodError>) {
    transaction(conn, || {
        let ids = vec![TASK.to_owned()];
        let before = capture_selected(conn, &ids)?;
        let before_watermark = watermark(conn)?;
        action()?;
        domain_event(conn, Some(TASK), operation)?;
        record_changes(conn, &before, &ids, before_watermark, operation, ACTOR)
    })
    .unwrap();
}

fn release(conn: &Connection, operation: &str) {
    mutate(conn, operation, || {
        conn.execute(
            "UPDATE tasks SET assigned_to='', assigned_at=NULL,
                lease_expires_at=NULL, lease_lifecycle='released', owner=NULL,
                updated_at=?1, revision=revision+1 WHERE id=?2",
            params![UPDATED, TASK],
        )?;
        conn.execute("DELETE FROM task_owners WHERE task_id=?1", [TASK])?;
        Ok(())
    });
}

fn complete(conn: &Connection) {
    mutate(conn, "task.done", || {
        conn.execute(
            "UPDATE tasks SET status='done', completed_at=?1,
                updated_at=?1, revision=revision+1 WHERE id=?2",
            params![UPDATED, TASK],
        )?;
        Ok(())
    });
}

fn event_snapshot(conn: &Connection) -> Vec<(i64, String, String, String)> {
    let mut stmt = conn
        .prepare("SELECT seq,event_id,event_type,payload FROM events ORDER BY seq,event_id")
        .unwrap();
    stmt.query_map([], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn assert_stale(conn: &Connection, binding: &Binding) {
    let before_projection = capture(conn).unwrap();
    let before_events = event_snapshot(conn);
    let error = check_validation_binding(conn, binding, true).unwrap_err();
    assert!(
        error.to_string().contains("TASK_JOURNAL_CONFLICT"),
        "{error}"
    );
    assert_eq!(capture(conn).unwrap(), before_projection);
    assert_eq!(event_snapshot(conn), before_events);
}

#[test]
fn historical_binding_replay_is_read_only_and_preserves_original_proof_bytes() {
    let (_temp, conn, binding) = fixture();
    let projection = capture(&conn).unwrap();
    let events = event_snapshot(&conn);
    for _ in 0..3 {
        check_validation_binding(&conn, &binding, false).unwrap();
        check_validation_binding(&conn, &binding, true).unwrap();
    }
    assert_eq!(capture(&conn).unwrap(), projection);
    assert_eq!(event_snapshot(&conn), events);
    assert_eq!(
        projection[TASK]
            .verification
            .as_ref()
            .unwrap()
            .verification_artifacts
            .as_deref(),
        Some(RAW_ARTIFACTS)
    );
}

#[test]
fn exact_archival_check_rejects_expired_custody_but_historical_replay_remains_valid() {
    // The checkpoint and captured binding already contain the expired numeric
    // lease. Clock expiry requires no task mutation or new journal transition.
    let (_temp, conn, binding) = fixture_with_lease("1Z");
    let projection = capture(&conn).unwrap();
    let events = event_snapshot(&conn);
    let error = check_validation_binding(&conn, &binding, false).unwrap_err();
    assert!(error.to_string().contains("custody expired"), "{error}");
    check_validation_binding(&conn, &binding, true).unwrap();
    assert_eq!(capture(&conn).unwrap(), projection);
    assert_eq!(event_snapshot(&conn), events);
}

#[test]
fn release_plain_done_and_proof_recapture_preserve_the_original_task_binding() {
    for operation in ["todo.yield", "todo.release", "todo.session.cleanup"] {
        let (_temp, conn, binding) = fixture();
        release(&conn, operation);
        check_validation_binding(&conn, &binding, true).unwrap();
        assert!(check_validation_binding(&conn, &binding, false).is_err());
        complete(&conn);
        mutate(&conn, "todo.proof.claimed", || {
            conn.execute(
                "UPDATE task_verification SET last_verified_status='claimed',
                    last_verified_notes='awaiting proof', updated_at=?1 WHERE todo_id=?2",
                params![UPDATED, TASK],
            )?;
            Ok(())
        });
        check_validation_binding(&conn, &binding, true).unwrap();
        mutate(&conn, "verify.history.complete", || {
            conn.execute(
                "UPDATE task_verification SET proof_plan='[\"validated_commit\"]',
                    last_verified_status='pass', last_verified_notes='historical completion',
                    updated_at=?1 WHERE todo_id=?2",
                params![UPDATED, TASK],
            )?;
            Ok(())
        });
        mutate(&conn, "verify.persist", || {
            conn.execute(
                "UPDATE task_verification SET last_verified_at=?1 WHERE todo_id=?2",
                params![UPDATED, TASK],
            )?;
            Ok(())
        });
        let after = capture(&conn).unwrap();
        let events = event_snapshot(&conn);
        for _ in 0..3 {
            check_validation_binding(&conn, &binding, true).unwrap();
        }
        assert_eq!(after[TASK].task.status, "done");
        assert_eq!(after[TASK].task.revision, binding.revision + 2);
        assert_eq!(after[TASK].task.completed_at.as_deref(), Some(UPDATED));
        assert_eq!(
            after[TASK].task.lease_generation,
            Some(binding.lease_generation)
        );
        assert_eq!(capture(&conn).unwrap(), after);
        assert_eq!(event_snapshot(&conn), events);
    }
}

#[test]
fn declared_proof_obligations_cannot_change_under_a_proof_result_operation() {
    let (_temp, conn, binding) = fixture();
    mutate(&conn, "verify.persist", || {
        conn.execute(
            "UPDATE task_verification SET proof_plan='[\"validate_passes\",\"state_commit\"]'
             WHERE todo_id=?1",
            [TASK],
        )?;
        Ok(())
    });
    let current = capture(&conn).unwrap();
    assert_ne!(
        validation_contract_digest(&current[TASK]).unwrap(),
        binding.contract_digest
    );
    assert_stale(&conn, &binding);
}

#[test]
fn restoring_the_original_proof_plan_does_not_erase_changed_obligations() {
    let (_temp, conn, binding) = fixture();
    for plan in [
        "[\"validate_passes\",\"state_commit\"]",
        "[\"validate_passes\"]",
    ] {
        mutate(&conn, "verify.persist", || {
            conn.execute(
                "UPDATE task_verification SET proof_plan=?1 WHERE todo_id=?2",
                params![plan, TASK],
            )?;
            Ok(())
        });
    }
    let current = capture(&conn).unwrap();
    assert_eq!(
        validation_contract_digest(&current[TASK]).unwrap(),
        binding.contract_digest
    );
    assert_stale(&conn, &binding);
}

#[test]
fn restored_task_intent_does_not_revalidate_the_original_proof() {
    let (_temp, conn, binding) = fixture();
    for title in ["Unvalidated replacement scope", "Validated task intent"] {
        mutate(&conn, "task.edit", || {
            conn.execute(
                "UPDATE tasks SET title=?1, revision=revision+1 WHERE id=?2",
                params![title, TASK],
            )?;
            Ok(())
        });
    }
    let current = capture(&conn).unwrap();
    assert_eq!(
        validation_contract_digest(&current[TASK]).unwrap(),
        binding.contract_digest
    );
    assert_stale(&conn, &binding);
}

#[test]
fn lifecycle_operation_cannot_smuggle_a_changed_dependency_contract() {
    let (_temp, conn, binding) = fixture();
    mutate(&conn, "todo.release", || {
        conn.execute(
            "UPDATE tasks SET assigned_to='', assigned_at=NULL, lease_expires_at=NULL,
                lease_lifecycle='released', depends_on='new-unvalidated-dependency',
                revision=revision+1 WHERE id=?1",
            [TASK],
        )?;
        Ok(())
    });
    assert_stale(&conn, &binding);
}

#[test]
fn real_lease_renewal_invalidates_proof_from_the_previous_generation() {
    let (_temp, conn, binding) = fixture();
    let generation =
        super::super::fleet_coord::extended_generation(binding.lease_generation as u32);
    assert!(i64::from(generation) > binding.lease_generation);
    mutate(&conn, "todo.renew", || {
        conn.execute(
            "UPDATE tasks SET lease_expires_at='10000000000Z',
                lease_generation=?1, lease_lifecycle='extended', updated_at=?2,
                revision=revision+1 WHERE id=?3",
            params![i64::from(generation), UPDATED, TASK],
        )?;
        Ok(())
    });
    assert_stale(&conn, &binding);
}

#[test]
fn release_and_same_owner_reclaim_cannot_reuse_old_validation() {
    let (_temp, conn, binding) = fixture();
    release(&conn, "todo.release");
    mutate(&conn, "todo.claim", || {
        conn.execute(
            "UPDATE tasks SET assigned_to=?1, assigned_at=?2,
                lease_expires_at='10000000000Z', lease_generation=lease_generation+1,
                lease_lifecycle='reclaimed', revision=revision+1 WHERE id=?3",
            params![ACTOR, UPDATED, TASK],
        )?;
        Ok(())
    });
    assert_stale(&conn, &binding);
}

#[test]
fn release_cannot_be_disguised_as_same_generation_custody_reacquisition() {
    let (_temp, conn, binding) = fixture();
    release(&conn, "todo.release");
    mutate(&conn, "todo.session.cleanup", || {
        conn.execute(
            "UPDATE tasks SET assigned_to=?1, assigned_at=?2,
                lease_expires_at='10000000000Z', lease_lifecycle='claimed',
                revision=revision+1 WHERE id=?3",
            params![ACTOR, UPDATED, TASK],
        )?;
        Ok(())
    });
    assert_stale(&conn, &binding);
}

#[test]
fn a_different_assignee_cannot_inherit_the_original_validation() {
    let (_temp, conn, binding) = fixture();
    mutate(&conn, "todo.session.cleanup", || {
        conn.execute(
            "UPDATE tasks SET assigned_to='another-agent', revision=revision+1 WHERE id=?1",
            [TASK],
        )?;
        Ok(())
    });
    assert_stale(&conn, &binding);
}

#[test]
fn proof_only_operation_cannot_mutate_task_revision_or_terminal_state() {
    for sql in [
        "UPDATE tasks SET revision=revision+1 WHERE id=?1",
        "UPDATE tasks SET status='done', completed_at='2026-02-03T04:05:06Z' WHERE id=?1",
    ] {
        let (_temp, conn, binding) = fixture();
        mutate(&conn, "verify.persist", || {
            conn.execute(sql, [TASK])?;
            Ok(())
        });
        assert_stale(&conn, &binding);
    }
}

#[test]
fn live_state_without_a_matching_checkpoint_is_never_repaired_by_replay() {
    let (_temp, conn, binding) = fixture();
    conn.execute(
        "UPDATE tasks SET title='Unjournaled replacement scope' WHERE id=?1",
        [TASK],
    )
    .unwrap();
    assert_stale(&conn, &binding);
}

#[test]
fn incomplete_or_exported_history_cannot_be_used_for_completion() {
    for operation in ["task.edit", "task.transfer.exported"] {
        let (_temp, conn, binding) = fixture();
        domain_event(&conn, Some(TASK), operation).unwrap();
        assert_stale(&conn, &binding);
    }
}

#[test]
fn restored_or_gap_accepted_history_requires_new_proof_even_when_state_matches() {
    for operation in ["task.journal.restored", "task.journal.gap_accepted"] {
        for subject in [Some(TASK), None] {
            let (_temp, conn, binding) = fixture();
            domain_event(&conn, subject, operation).unwrap();
            assert_stale(&conn, &binding);
        }
    }
}

#[test]
fn corrupt_or_missing_original_checkpoints_fail_closed_without_writing() {
    let (_temp, conn, binding) = fixture();
    let event = checkpoint_events(&conn, Some(&[TASK.to_owned()]))
        .unwrap()
        .pop()
        .unwrap()
        .event;
    let mut envelope = event.payload;
    envelope["payload"]["projection"]["task"]["title"] = serde_json::json!("Tampered checkpoint");
    conn.execute(
        "UPDATE events SET payload=?1 WHERE event_id=?2",
        params![envelope.to_string(), event.event_id],
    )
    .unwrap();
    assert_stale(&conn, &binding);

    let (_temp, conn, binding) = fixture();
    complete(&conn);
    conn.execute(
        "DELETE FROM events WHERE event_id=?1",
        [format!("task-journal:{}", binding.checkpoint_digest)],
    )
    .unwrap();
    assert_stale(&conn, &binding);
}

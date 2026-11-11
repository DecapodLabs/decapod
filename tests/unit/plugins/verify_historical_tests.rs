use super::*;
use crate::core::db::{Connection, params};
use crate::core::readiness_evidence::MaterialIdentity;
use crate::core::store::StoreKind;
use tempfile::TempDir;

const ID: &str = "code_historical_completion";

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
    store: Store,
    proof: HistoricalValidationProof,
    integration: IntegrationWitness,
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Historical Test",
            "-c",
            "user.email=history@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn connection<T>(store: &Store, f: impl FnOnce(&Connection) -> T) -> T {
    DbBroker::new(&store.root)
        .with_conn(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.history.read",
            |conn| Ok(f(conn)),
        )
        .unwrap()
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=master"]);
    git(
        &repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/historical.git",
        ],
    );
    fs::create_dir(repo.join(".decapod")).unwrap();
    fs::write(
        repo.join(".decapod/config.toml"),
        toml::to_string(&crate::cli::DecapodProjectConfig::default()).unwrap(),
    )
    .unwrap();
    fs::write(repo.join("tracked.txt"), "accepted base").unwrap();
    git(&repo, &["add", "tracked.txt", ".decapod/config.toml"]);
    git(&repo, &["commit", "-m", "accepted base"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join("tracked.txt"), "validated candidate").unwrap();
    git(&repo, &["add", "tracked.txt"]);
    git(&repo, &["commit", "-m", "validated candidate"]);
    let candidate = git(&repo, &["rev-parse", "HEAD"]);
    let store = Store {
        kind: StoreKind::Repo,
        root: temp.path().join("store"),
    };
    todo::initialize_todo_db(&store.root).unwrap();
    connection(&store, |conn| {
        conn.execute(
            "INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,
                owner,assigned_to,assigned_at,lease_expires_at,lease_generation,lease_lifecycle,
                intent_anchor,one_shot,revision)
             VALUES(?1,'history-fixture','Original governed task','in_progress','1Z','1Z','.',
                '',?2,?2,'1Z','9999999999Z',2,'claimed','intent:original',1,7)",
            params![ID, crate::current_agent_id()],
        )
        .unwrap();
    });
    todo::task_journal_checkpoint(&store.root, None).unwrap();
    let binding = todo::capture_validation_binding(&store.root, ID).unwrap();
    let epoch = active_validation_epoch(&repo).unwrap();
    let digest = format!("sha256:{}", "a".repeat(64));
    let material = MaterialIdentity {
        repository: "example/historical".into(),
        work_branch: "agent/test/history".into(),
        head_revision: candidate.clone(),
        base_branch: "master".into(),
        accepted_base_revision: base.clone(),
        history_digest: digest.clone(),
        tree_digest: digest.clone(),
        configuration_digest: digest.clone(),
        workflow_digest: digest.clone(),
        toolchain_digest: digest.clone(),
        candidate_digest: digest,
    };
    let proof = DbBroker::new(&store.root)
        .with_transaction(
            &todo::todo_db_path(&store.root),
            "test",
            None,
            "test.history.fixture",
            |conn| {
                validation_history::test_fixture::persist(
                    conn,
                    &store.root,
                    material,
                    binding,
                    epoch,
                )
            },
        )
        .unwrap();
    let integration = IntegrationWitness {
        candidate_sha: candidate.clone(),
        accepted_sha: candidate.clone(),
        integration_sha: candidate,
        accepted_base: base,
        mode: "ancestor".into(),
        repository: "example/historical".into(),
        base_branch: "master".into(),
    };
    Fixture {
        _temp: temp,
        repo,
        store,
        proof,
        integration,
    }
}

fn target(fixture: &Fixture) -> VerifyTarget {
    load_targets(&fixture.store, Some(ID)).unwrap().remove(0)
}

fn prepare(fixture: &Fixture) -> PreparedTodoVerification {
    // This is the production immutable-preparation boundary, after a remote
    // integration observation. The record itself uses the full production
    // verifier; no fake success boolean or current validation child is used.
    prepare_historical_completion_from_evidence(
        &fixture.store,
        &fixture.repo,
        target(fixture),
        fixture.proof.clone(),
        fixture.integration.clone(),
    )
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
struct TransactionalEventSnapshot {
    stream: String,
    seq: i64,
    event_id: String,
    ts: String,
    subject_kind: Option<String>,
    subject_id: Option<String>,
    event_type: String,
    payload: String,
    actor: String,
}

fn transactional_events(fixture: &Fixture) -> Vec<TransactionalEventSnapshot> {
    // Broker audits record attempted operations and these observations. The
    // rollback contract preserves complete task and verification event rows.
    connection(&fixture.store, |conn| {
        let mut statement = conn
            .prepare(
                "SELECT stream,seq,event_id,ts,subject_kind,subject_id,event_type,payload,actor
                 FROM events WHERE stream IN (?1,?2) ORDER BY stream,seq,event_id",
            )
            .unwrap();
        statement
            .query_map([events::TODO, events::VERIFICATION], |row| {
                Ok(TransactionalEventSnapshot {
                    stream: row.get(0)?,
                    seq: row.get(1)?,
                    event_id: row.get(2)?,
                    ts: row.get(3)?,
                    subject_kind: row.get(4)?,
                    subject_id: row.get(5)?,
                    event_type: row.get(6)?,
                    payload: row.get(7)?,
                    actor: row.get(8)?,
                })
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    })
}

fn mutate(
    fixture: &Fixture,
    operation: &str,
    f: impl FnOnce(&Connection) -> Result<(), error::DecapodError>,
) {
    todo::with_task_journal(
        &DbBroker::new(&fixture.store.root),
        &todo::todo_db_path(&fixture.store.root),
        "decapod",
        None,
        operation,
        todo::TaskJournalSelection::Ids(vec![ID.into()]),
        |conn| {
            f(conn)?;
            append_verification_event_on_connection(
                conn,
                ID,
                operation,
                "2Z",
                serde_json::json!({}),
            )
        },
    )
    .unwrap();
}

fn claimed_done(fixture: &Fixture) {
    mutate(fixture, "todo.yield", |conn| {
        conn.execute(
            "UPDATE tasks SET assigned_to='',assigned_at=NULL,lease_expires_at=NULL,
            lease_lifecycle='yielded',revision=revision+1,updated_at='2Z' WHERE id=?1",
            [ID],
        )?;
        Ok(())
    });
    mutate(fixture, "task.done", |conn| {
        conn.execute("UPDATE tasks SET status='done',revision=revision+1,completed_at='2Z',updated_at='2Z' WHERE id=?1", [ID])?;
        Ok(())
    });
    mutate(fixture, "todo.proof.claimed", |conn| {
        conn.execute(
            "INSERT INTO task_verification(todo_id,proof_plan,last_verified_status,updated_at)
            VALUES(?1,'[\"validate_passes\"]','CLAIMED','2Z')",
            [ID],
        )?;
        Ok(())
    });
}

#[test]
fn historical_completion_commits_actual_task_cas_proof_and_journal_together() {
    let fixture = fixture();
    let spawns = validation_spawn_count();
    let prepared = prepare(&fixture);
    let result = todo::complete_prepared_task_for_test(&fixture.store, ID, &prepared).unwrap();
    assert_eq!(result["status"], "ok");
    let done = target(&fixture);
    assert_eq!(done.status, "done");
    assert_eq!(done.revision, 8);
    assert_eq!(done.proof_plan.as_deref(), Some("[\"validated_commit\"]"));
    let artifacts: VerificationArtifacts =
        serde_json::from_str(done.artifacts.as_ref().unwrap()).unwrap();
    assert_eq!(artifacts.task_revision, Some(8));
    assert_eq!(
        artifacts.historical_validation.unwrap().record_id,
        fixture.proof.record_id()
    );
    let result = verify_target(&done, &fixture.store.root, &fixture.repo, None).unwrap();
    assert_eq!(result.status, "pass", "{:?}", result.notes);
    assert_eq!(validation_spawn_count(), spawns);
}

#[test]
fn done_task_with_original_active_custody_can_capture_a_fresh_validation_binding() {
    let fixture = fixture();
    let prepared = prepare(&fixture);
    todo::complete_prepared_task_for_test(&fixture.store, ID, &prepared).unwrap();
    let binding = todo::capture_validation_binding(&fixture.store.root, ID).unwrap();
    assert_eq!(binding.status, "done");
    assert_eq!(binding.revision, 8);
    assert_eq!(binding.assignee, fixture.proof.task_binding().assignee);
    assert_eq!(
        binding.lease_generation,
        fixture.proof.task_binding().lease_generation
    );
    connection(&fixture.store, |conn| {
        todo::check_validation_binding_on_conn(conn, &binding, false).unwrap();
    });
    mutate(&fixture, "todo.session.cleanup", |conn| {
        conn.execute(
            "UPDATE tasks SET assigned_to='',assigned_at=NULL,lease_expires_at=NULL,
            lease_lifecycle='released',revision=revision+1,updated_at='3Z' WHERE id=?1",
            [ID],
        )?;
        Ok(())
    });
    assert!(
        todo::capture_validation_binding(&fixture.store.root, ID).is_err(),
        "done status must not grant fresh validation custody after release"
    );
}

#[test]
fn completed_claimed_task_repairs_at_same_revision_then_replays_after_new_epoch() {
    let fixture = fixture();
    claimed_done(&fixture);
    let before = todo::get_task(&fixture.store.root, ID).unwrap().unwrap();
    let prepared = prepare(&fixture);
    complete_historical_done_task(&fixture.store, &prepared).unwrap();
    let after = todo::get_task(&fixture.store.root, ID).unwrap().unwrap();
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.completed_at, before.completed_at);
    assert_eq!(
        target(&fixture).last_verified_status.as_deref(),
        Some("pass")
    );

    fs::write(fixture.repo.join("AGENTS.md"), "later task governance").unwrap();
    fs::write(fixture.repo.join("tracked.txt"), "later source work").unwrap();
    assert_ne!(
        active_validation_epoch(&fixture.repo).unwrap(),
        *fixture.proof.validation_epoch()
    );
    let spawns = validation_spawn_count();
    let cli = VerifyCli::try_parse_from(["verify", "--json"]).unwrap();
    run_verify_cli(&fixture.store, &fixture.repo, cli).unwrap();
    assert_eq!(
        validation_spawn_count(),
        spawns,
        "aggregate replay must not spawn current validation"
    );
    let current = target(&fixture);
    let result = verify_target(&current, &fixture.store.root, &fixture.repo, None).unwrap();
    assert_eq!(result.status, "pass", "{:?}", result.notes);
    assert!(
        crate::plan_governance::collect_unverified_done_todos(&fixture.store.root)
            .unwrap()
            .is_empty(),
        "the actual aggregate proof hook must not block another task on immutable historical evidence"
    );
    let claim = todo::get_work_claim(&fixture.store.root, ID)
        .unwrap()
        .unwrap();
    assert_eq!(
        claim.validation_status,
        crate::core::work_claim::WorkClaimValidationStatus::Passed
    );
    assert_eq!(
        claim.status,
        crate::core::work_claim::WorkClaimStatus::Complete
    );
    connection(&fixture.store, |conn| {
        conn.execute("INSERT INTO tasks(id,hash,title,status,created_at,updated_at,dir_path,scope,depends_on,revision)
            VALUES('code_successor','successor','New dependent work','open','3Z','3Z','.','',?1,1)", [ID]).unwrap();
        assert!(
            todo::dependency_completion_gate_on_conn(conn, &fixture.store.root, "code_successor")
                .unwrap()
                .is_none(),
            "new dependent work must accept completed historical proof"
        );
    });
}

#[test]
fn failed_final_journal_append_rolls_back_status_proof_and_capture_events() {
    let fixture = fixture();
    let prepared = prepare(&fixture);
    let before = transactional_events(&fixture);
    connection(&fixture.store, |conn| {
        conn.execute(
            "CREATE TRIGGER reject_history_checkpoint BEFORE INSERT ON events
            WHEN NEW.stream='todo' AND NEW.event_type='task.journal.checkpoint'
            BEGIN SELECT RAISE(ABORT,'injected final checkpoint failure'); END",
            [],
        )
        .unwrap();
    });
    assert!(todo::complete_prepared_task_for_test(&fixture.store, ID, &prepared).is_err());
    let task = target(&fixture);
    assert_eq!(task.status, "in_progress");
    assert_eq!(task.revision, 7);
    assert!(task.artifacts.is_none());
    assert_eq!(transactional_events(&fixture), before);
}

#[test]
fn changed_task_cas_rejects_prepared_history_without_any_proof_write() {
    let fixture = fixture();
    let prepared = prepare(&fixture);
    mutate(&fixture, "todo.edit", |conn| {
        conn.execute(
            "UPDATE tasks SET title='New intent',revision=revision+1 WHERE id=?1",
            [ID],
        )?;
        Ok(())
    });
    let before = transactional_events(&fixture);
    let result = todo::complete_prepared_task_for_test(&fixture.store, ID, &prepared).unwrap();
    assert_eq!(result["status"], "conflict");
    assert!(target(&fixture).artifacts.is_none());
    assert_eq!(transactional_events(&fixture), before);
}

#[test]
fn completed_task_repair_rechecks_same_revision_baseline_before_writing() {
    let fixture = fixture();
    claimed_done(&fixture);
    let prepared = prepare(&fixture);
    mutate(&fixture, "verify.persist", |conn| {
        conn.execute(
            "UPDATE task_verification SET verification_artifacts='{}' WHERE todo_id=?1",
            [ID],
        )?;
        Ok(())
    });
    let before = transactional_events(&fixture);
    let error = complete_historical_done_task(&fixture.store, &prepared).unwrap_err();
    assert!(error.to_string().contains("STALE_TASK_VERIFICATION"));
    assert_eq!(target(&fixture).artifacts.as_deref(), Some("{}"));
    assert_eq!(
        target(&fixture).last_verified_status.as_deref(),
        Some("CLAIMED")
    );
    assert_eq!(transactional_events(&fixture), before);
}

#[test]
fn historical_completion_never_erases_additional_task_proof_obligations() {
    for raw in [
        "[]",
        "[\"state_commit\"]",
        "[\"validate_passes\",\"state_commit\"]",
        "[\"validated_commit\",\"validate_passes\"]",
        "[\"validate_passes\",\"validate_passes\"]",
        "{}",
    ] {
        assert!(ensure_historical_proof_plan(Some(raw)).is_err(), "{raw}");
    }
    assert!(ensure_historical_proof_plan(None).is_ok());
    assert!(ensure_historical_proof_plan(Some("[\"validate_passes\"]")).is_ok());
    assert!(ensure_historical_proof_plan(Some("[\"validated_commit\"]")).is_ok());
    let fixture = fixture();
    let mut target = target(&fixture);
    target.proof_plan = Some("[\"validate_passes\",\"state_commit\"]".into());
    let before = transactional_events(&fixture);
    let error = prepare_historical_completion_from_evidence(
        &fixture.store,
        &fixture.repo,
        target,
        fixture.proof.clone(),
        fixture.integration.clone(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("TASK_PROOF_UNSUPPORTED"));
    assert_eq!(transactional_events(&fixture), before);
}

#[test]
fn historical_commit_cli_is_read_only_and_rejects_stale_listing_combination() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: temp.path().join("absent"),
    };
    let sha = "a".repeat(40);
    for args in [
        vec!["verify", "commit", &sha],
        vec!["verify", "commit", &sha, "--stale"],
    ] {
        let cli = VerifyCli::try_parse_from(args).unwrap();
        assert!(is_commit_lookup(&cli));
        let spawns = validation_spawn_count();
        assert!(run_verify_cli(&store, temp.path(), cli).is_err());
        assert_eq!(validation_spawn_count(), spawns);
        assert!(
            !store.root.exists(),
            "lookup must not initialize a missing database"
        );
    }
    assert!(todo::TodoCli::try_parse_from(["todo", "done", ID, "--commit", &sha]).is_err());
    assert!(
        todo::TodoCli::try_parse_from([
            "todo",
            "done",
            ID,
            "--validated",
            "--commit",
            &sha,
            "--artifact",
            "x"
        ])
        .is_err()
    );
    assert!(
        todo::TodoCli::try_parse_from(["todo", "done", ID, "--validated", "--commit", &sha])
            .is_ok()
    );
}

#[test]
fn malformed_historical_baseline_cannot_fall_back_to_current_validation() {
    let fixture = fixture();
    let mut target = target(&fixture);
    target.status = "done".into();
    target.proof_plan = Some("[\"validated_commit\"]".into());
    for artifact in [None, Some("{}".to_owned()), Some("not-json".to_owned())] {
        target.artifacts = artifact;
        assert!(!requires_current_validation(&target));
        let spawns = validation_spawn_count();
        assert_ne!(
            verify_target(&target, &fixture.store.root, &fixture.repo, None)
                .unwrap()
                .status,
            "pass"
        );
        assert_eq!(validation_spawn_count(), spawns);
    }
}

#[test]
fn historical_witness_cannot_substitute_another_target_branch() {
    let fixture = fixture();
    let mut witness = fixture.integration.clone();
    witness.base_branch = "unreviewed-branch".into();
    assert!(
        prepare_historical_completion_from_evidence(
            &fixture.store,
            &fixture.repo,
            target(&fixture),
            fixture.proof.clone(),
            witness,
        )
        .is_err()
    );
    assert!(target(&fixture).artifacts.is_none());
}

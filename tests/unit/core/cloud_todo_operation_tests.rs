use super::*;

fn record() -> OperationRecord {
    OperationRecord::new(
        fingerprint(
            "https://service.example",
            "owner/repo",
            &serde_json::json!({"title": "private title"}),
        ),
        None,
    )
    .unwrap()
}

#[test]
fn fresh_identical_adds_are_distinct_but_explicit_retries_bind_every_input() {
    let one = record();
    let two = record();
    assert_ne!(one.operation_id, two.operation_id);
    assert_ne!(one.task_id, two.task_id);
    assert_eq!(
        OperationRecord::new(one.request_fingerprint.clone(), Some(&one.operation_id))
            .unwrap()
            .task_id,
        one.task_id
    );
    for (endpoint, repo, title) in [
        ("https://other.example", "owner/repo", "private title"),
        ("https://service.example", "fork/repo", "private title"),
        ("https://service.example", "owner/repo", "changed"),
    ] {
        assert!(
            OperationRecord::new(
                fingerprint(endpoint, repo, &serde_json::json!({"title": title})),
                Some(&one.operation_id)
            )
            .is_err()
        );
    }
}

#[test]
fn dispatch_binding_survives_preflight_and_fences_project_changes() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let record = journal.prepare(&record()).unwrap();
    journal
        .finish(
            &record,
            Outcome::BlockedBeforeSubmit,
            Some(FailureKind::Authentication),
        )
        .unwrap();
    let selected = digest(b"selected-project");
    let legacy = digest(b"git-repository");
    let bound = journal.bind_route(&record, &selected, &legacy).unwrap();
    assert_eq!(bound.operation_id, record.operation_id);
    assert!(journal.finish(&record, Outcome::Succeeded, None).is_err());
    journal
        .finish(
            &bound,
            Outcome::OutcomeUnknown,
            Some(FailureKind::Transport),
        )
        .unwrap();
    let recovered = Journal::at(temp.path().join("operations"));
    assert!(
        recovered
            .bind_route(&bound, &digest(b"another-project"), &legacy)
            .is_err()
    );
    let unchanged = recovered.bind_route(&bound, &selected, &legacy).unwrap();
    assert_eq!(unchanged.status, Outcome::OutcomeUnknown);
    journal
        .finish(&unchanged, Outcome::Succeeded, None)
        .unwrap();
    assert!(recovered.bind_route(&unchanged, &legacy, &legacy).is_err());
}

#[test]
fn older_unknown_records_cannot_be_reinterpreted_as_selected_project_writes() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let old = journal.prepare(&record()).unwrap();
    journal.finish(&old, Outcome::OutcomeUnknown, None).unwrap();
    let legacy = digest(b"git-repository");
    assert!(
        journal
            .bind_route(&old, &digest(b"selected-project"), &legacy)
            .is_err()
    );
    assert!(journal.bind_route(&old, &legacy, &legacy).is_ok());
}

#[test]
fn concurrent_first_dispatches_cannot_bind_one_operation_to_two_projects() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("operations");
    let record = Journal::at(&path).prepare(&record()).unwrap();
    let workers: Vec<_> = [digest(b"one"), digest(b"two")]
        .into_iter()
        .map(|route| {
            let path = path.clone();
            let record = record.clone();
            std::thread::spawn(move || {
                Journal::at(path).bind_route(&record, &route, &digest(b"legacy"))
            })
        })
        .collect();
    assert_eq!(
        workers
            .into_iter()
            .map(|w| w.join().unwrap().is_ok())
            .filter(|passed| *passed)
            .count(),
        1
    );
}

#[test]
fn diagnostic_is_non_secret_and_separates_unknown_outcome_from_preflight() {
    let operation = record();
    let diagnostic = Diagnostic::new(&operation, Outcome::OutcomeUnknown, FailureKind::Transport);
    let encoded = serde_json::to_string(&diagnostic).unwrap();
    assert!(encoded.contains("outcome_unknown"));
    assert!(encoded.contains(&operation.operation_id));
    assert!(encoded.contains("--operation-id"));
    assert!(!encoded.contains("private title"));
    assert!(!encoded.contains("service.example"));
    assert!(diagnostic.retryable);
    assert_ne!(
        Diagnostic::new(
            &operation,
            Outcome::BlockedBeforeSubmit,
            FailureKind::Authentication
        )
        .status,
        diagnostic.status
    );
}

#[test]
fn physical_error_categories_are_not_flattened_to_validation() {
    use dactyl_db::AdapterErrorKind as K;
    for (kind, expected) in [
        (K::Authentication, FailureKind::Authentication),
        (K::Authorization, FailureKind::Authorization),
        (K::Transport, FailureKind::Transport),
        (K::Conflict, FailureKind::Conflict),
        (K::Query, FailureKind::BackendValidation),
        (K::Cancellation, FailureKind::Cancelled),
    ] {
        let error = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind,
            code: None,
            message: "secret bearer and private backend details".into(),
        });
        assert_eq!(failure_kind(&error), expected);
        assert!(
            !serde_json::to_string(&Diagnostic::new(
                &record(),
                Outcome::OutcomeUnknown,
                failure_kind(&error)
            ))
            .unwrap()
            .contains("secret")
        );
    }
    let offline = DecapodError::CloudAuth(crate::core::error::CloudAuthDiagnostic::new(
        CloudAuthStatus::Offline,
        "private",
        "private",
    ));
    assert_eq!(failure_kind(&offline), FailureKind::Offline);
}

#[test]
fn journal_survives_interruption_and_never_downgrades_a_confirmed_success() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let record = record();
    journal.prepare(&record).unwrap();
    journal
        .finish(&record, Outcome::OutcomeUnknown, None)
        .unwrap();
    let recovered = Journal::at(temp.path().join("operations"));
    assert_eq!(recovered.list().unwrap()[0].status, Outcome::OutcomeUnknown);
    recovered
        .finish(
            &record,
            Outcome::BlockedBeforeSubmit,
            Some(FailureKind::Authentication),
        )
        .unwrap();
    assert_eq!(recovered.list().unwrap()[0].status, Outcome::OutcomeUnknown);
    journal.finish(&record, Outcome::Succeeded, None).unwrap();
    recovered
        .finish(
            &record,
            Outcome::OutcomeUnknown,
            Some(FailureKind::Transport),
        )
        .unwrap();
    assert_eq!(recovered.list().unwrap()[0].status, Outcome::Succeeded);
    assert!(
        !std::fs::read_to_string(temp.path().join("operations/operations.json"))
            .unwrap()
            .contains("private title")
    );
}

#[test]
fn receipt_envelope_migrates_legacy_rows_and_blocks_older_writers() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let mut old = record();
    old.status = Outcome::OutcomeUnknown;
    journal.prepare(&old).unwrap();
    let path = temp.path().join("operations/operations.json");
    std::fs::write(&path, serde_json::to_vec(&vec![old.clone()]).unwrap()).unwrap();
    assert_eq!(journal.list().unwrap(), vec![old.clone()]);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        CACHE_VERSION
    );
    // This is the reader shape used by the released array-only client. It
    // cannot rewrite v2 and silently remove the new route binding.
    assert!(serde_json::from_slice::<Vec<OperationRecord>>(&bytes).is_err());
    let legacy_route = digest(b"original-repository");
    let bound = journal
        .bind_route(&old, &legacy_route, &legacy_route)
        .unwrap();
    assert_eq!(bound.status, Outcome::OutcomeUnknown);
    assert_eq!(
        journal.list().unwrap()[0].routing_fingerprint,
        Some(legacy_route)
    );
}

#[test]
fn future_receipt_versions_are_preserved_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let original = record();
    journal.prepare(&original).unwrap();
    let path = temp.path().join("operations/operations.json");
    let future =
        serde_json::to_vec(&serde_json::json!({"version":99,"records":[original]})).unwrap();
    std::fs::write(&path, &future).unwrap();
    assert!(journal.prepare(&record()).is_err());
    assert_eq!(std::fs::read(path).unwrap(), future);
}

#[test]
fn receipt_upgrade_checks_encoded_bytes_before_replacing_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    let mut old = record();
    journal.prepare(&old).unwrap();
    old.task_id.clear();
    let overhead = serde_json::to_vec(&vec![old.clone()]).unwrap().len();
    old.task_id = "x".repeat(MAX_METADATA_BYTES - overhead - 4);
    let original = serde_json::to_vec(&vec![old]).unwrap();
    assert_eq!(original.len(), MAX_METADATA_BYTES - 4);
    let path = temp.path().join("operations/operations.json");
    std::fs::write(&path, &original).unwrap();
    assert!(journal.list().is_err());
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn journal_retains_unresolved_records_when_at_capacity() {
    let temp = tempfile::tempdir().unwrap();
    let journal = Journal::at(temp.path().join("operations"));
    journal
        .with_records(|records| {
            for _ in 0..MAX_RECORDS {
                let mut record = record();
                record.status = Outcome::OutcomeUnknown;
                record.updated_at = 0;
                records.push(record);
            }
            Ok(())
        })
        .unwrap();
    assert!(journal.prepare(&record()).is_err());
    assert_eq!(journal.list().unwrap().len(), MAX_RECORDS);
}

#[test]
fn concurrent_receipt_writes_preserve_all_operations() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("operations");
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || Journal::at(path).prepare(&record()).unwrap())
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(Journal::at(path).list().unwrap().len(), 8);
}

#[cfg(unix)]
#[test]
fn journal_creation_and_replacement_are_private_under_permissive_umask() {
    const CHILD: &str = "DECAPOD_TODO_JOURNAL_UMASK_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "journal-test"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "core::cloud_todo_operation::tests::journal_creation_and_replacement_are_private_under_permissive_umask", "--nocapture"])
            .env(CHILD, "1").output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    // The fixture is an already trusted home, not a permission repair target.
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = temp.path().join("operations");
    let journal = Journal::at(&path);
    let operation = record();
    journal.prepare(&operation).unwrap();
    for state in [Outcome::OutcomeUnknown, Outcome::Succeeded] {
        journal.finish(&operation, state, None).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        for name in ["operations.json", "operations.lock"] {
            assert_eq!(
                std::fs::metadata(path.join(name))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}

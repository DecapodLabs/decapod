use super::super::super as todo;
use super::*;
use std::process::Command;

const SOURCE: &str = "transfer-source";
const RECEIVER: &str = "transfer-receiver";

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn source() -> (tempfile::TempDir, PathBuf, String) {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Transfer Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "fixture",
        ],
    );
    git(
        repo,
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:fixture/transfer.git",
        ],
    );
    let root = repo.join(".decapod/data");
    todo::initialize_todo_db(&root).unwrap();
    let added = todo::add_task(
        &root,
        &todo::TodoCommand::Add {
            title: "Transfer an original task".into(),
            description: "Original intent\nwith provenance".into(),
            priority: "high".into(),
            tags: "custody,test".into(),
            owner: String::new(),
            due: None,
            r#ref: "issue:1452".into(),
            scope: "root".into(),
            dir: Some(repo.into()),
            depends_on: String::new(),
            blocks: String::new(),
            parent: None,
            one_shot: 0,
        },
    )
    .unwrap();
    let id = added["id"].as_str().unwrap().to_owned();
    let claim = todo::claim_task(&root, &id, SOURCE, todo::ClaimMode::Exclusive).unwrap();
    assert_eq!(claim["status"], "ok");
    let yielded = todo::yield_claim_lease(&root, &id, SOURCE, "move to another Mac").unwrap();
    assert_eq!(yielded["status"], "ok");
    (temp, root, id)
}
fn destination(source: &Path) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let out = Command::new("git")
        .args(["clone", "-q", "--local"])
        .arg(source)
        .arg(temp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    git(
        temp.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/fixture/transfer.git",
        ],
    );
    let root = temp.path().join(".decapod/data");
    todo::initialize_todo_db(&root).unwrap();
    (temp, root)
}
fn projection(root: &Path, id: &str) -> TaskProjection {
    let conn = Connection::open(todo_db_path(root)).unwrap();
    capture_selected(&conn, &[id.into()])
        .unwrap()
        .remove(id)
        .unwrap()
}
fn command_import(input: PathBuf, digest: String) -> TodoTransferCommand {
    TodoTransferCommand::Import {
        input,
        expected_digest: digest,
        accept_source_transfer: true,
    }
}

#[test]
fn transfer_preserves_identity_intent_history_and_requires_new_proof() {
    let (source, root, id) = source();
    // Seed historical proof only in a disposable fixture through the journaled
    // mutation boundary. Its exact bytes must survive as history, never a pass.
    with_task_journal(&DbBroker::new(&root), &todo_db_path(&root), SOURCE, None,
        "fixture.proof", TaskJournalSelection::Ids(vec![id.clone()]), |conn| {
        conn.execute("INSERT INTO task_verification(todo_id,proof_plan,verification_artifacts,last_verified_status,verification_policy_days,updated_at) VALUES(?1,'[]','original proof bytes','passed',90,?2)",
            params![id,now_iso()])?;
        Ok(())
    }).unwrap();
    let before = projection(&root, &id);
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    assert_eq!(bundle.projection, before);
    let (dest, dest_root) = destination(source.path());
    let result = import_bundle(&dest_root, &bundle, RECEIVER).unwrap();
    assert_eq!(result["execution_ready"], false);
    let after = projection(&dest_root, &id);
    assert_eq!(after.task.id, before.task.id);
    assert_eq!(after.task.hash, before.task.hash);
    assert_eq!(after.task.title, before.task.title);
    assert_eq!(after.task.description, before.task.description);
    assert_eq!(after.task.intent_anchor, before.task.intent_anchor);
    assert_eq!(after.task.created_at, before.task.created_at);
    assert_eq!(after.task.lease_generation, before.task.lease_generation);
    assert_eq!(after.task.revision, before.task.revision + 2);
    assert_eq!(after.task.dir_path, dest.path().to_string_lossy());
    let proof = after.verification.unwrap();
    assert_eq!(proof.last_verified_status.as_deref(), Some("unavailable"));
    assert!(proof.verification_artifacts.is_none());
    let conn = Connection::open(todo_db_path(&dest_root)).unwrap();
    let events = events::query_subject_on_conn(&conn, events::TODO, "task", &id, 100).unwrap();
    let imported = events.iter().find(|e| e.event_type == IMPORTED).unwrap();
    assert_eq!(
        imported.payload["payload"]["bundle"]["projection"]["verification"]["verification_artifacts"],
        "original proof bytes"
    );
    assert_eq!(
        events.iter().filter(|e| e.event_type == EVENT_TYPE).count(),
        1
    );
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
    drop(conn);
    assert!(import_bundle(&dest_root, &bundle, RECEIVER).is_err());
    assert_eq!(
        projection(&dest_root, &id).task.revision,
        after.task.revision
    );
    assert_eq!(
        todo::claim_task(&dest_root, &id, RECEIVER, todo::ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
}

#[test]
fn source_seal_rejects_reclaim_edit_release_yield_and_second_export() {
    let (_source, root, id) = source();
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    let before = projection(&root, &id);
    assert!(todo::claim_task(&root, &id, SOURCE, todo::ClaimMode::Exclusive).is_err());
    assert!(todo::yield_claim_lease(&root, &id, SOURCE, "again").is_err());
    assert!(todo::release_task_as_actor(&root, &id, SOURCE).is_err());
    assert!(export_bundle(&root, &id, RECEIVER, SOURCE).is_err());
    assert!(
        with_task_journal(
            &DbBroker::new(&root),
            &todo_db_path(&root),
            SOURCE,
            None,
            "todo.edit",
            TaskJournalSelection::Ids(vec![id.clone()]),
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(projection(&root, &id), before);
    let conn = Connection::open(todo_db_path(&root)).unwrap();
    assert_eq!(verify(&conn).unwrap()["status"], "ok");
    assert!(check_execution_authority(&conn, &id).is_err());
    assert_eq!(
        recover_bundle(&root, &id, &bundle.transfer_id, SOURCE)
            .unwrap()
            .transfer_id,
        bundle.transfer_id
    );
    assert!(recover_bundle(&root, &id, &bundle.transfer_id, RECEIVER).is_err());
}

#[test]
fn export_requires_actual_yield_and_its_original_actor() {
    let (_source, root, id) = source();
    let before = projection(&root, &id);
    assert!(export_bundle(&root, &id, RECEIVER, "other-source").is_err());
    assert!(export_bundle(&root, &id, SOURCE, SOURCE).is_err());
    assert_eq!(projection(&root, &id), before);
    assert_eq!(
        todo::claim_task(&root, &id, SOURCE, todo::ClaimMode::Exclusive).unwrap()["status"],
        "ok"
    );
    let claimed = projection(&root, &id);
    assert!(export_bundle(&root, &id, RECEIVER, SOURCE).is_err());
    assert_eq!(projection(&root, &id), claimed);
}

#[test]
fn import_rejects_wrong_repository_receiver_conflicts_and_replayed_history() {
    let (source, root, id) = source();
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    let (dest, dest_root) = destination(source.path());
    assert!(import_bundle(&dest_root, &bundle, "wrong-receiver").is_err());
    git(
        dest.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:fixture/wrong.git",
        ],
    );
    assert!(import_bundle(&dest_root, &bundle, RECEIVER).is_err());
    git(
        dest.path(),
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:fixture/transfer.git",
        ],
    );
    let mut changed = bundle.clone();
    changed.source_commit = "f".repeat(40);
    assert!(import_bundle(&dest_root, &changed, RECEIVER).is_err());
    import_bundle(&dest_root, &bundle, RECEIVER).unwrap();
    let before = projection(&dest_root, &id);
    changed = bundle.clone();
    changed.transfer_id = crate::core::ulid::new_ulid();
    assert!(import_bundle(&dest_root, &changed, RECEIVER).is_err());
    assert_eq!(projection(&dest_root, &id), before);
}

#[test]
fn malformed_checkpoint_history_fails_without_any_destination_task() {
    let (source, root, id) = source();
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    let (_dest, dest_root) = destination(source.path());
    let mut cases = Vec::new();
    let mut b = bundle.clone();
    let first_checkpoint = b
        .history
        .iter()
        .position(|e| e.event_type == EVENT_TYPE)
        .unwrap();
    b.history.remove(first_checkpoint);
    cases.push(b);
    let mut b = bundle.clone();
    b.history.reverse();
    cases.push(b);
    let mut b = bundle.clone();
    b.source_head = "0".repeat(64);
    cases.push(b);
    let mut b = bundle.clone();
    b.projection.task.description = Some("forged".into());
    cases.push(b);
    let mut b = bundle.clone();
    b.history[0].seq = u64::MAX;
    cases.push(b);
    let mut b = bundle.clone();
    b.history[0].payload["actor"] = json!("forged");
    cases.push(b);
    let mut b = bundle.clone();
    b.history.push(b.history.last().unwrap().clone());
    cases.push(b);
    let mut b = bundle.clone();
    b.sealed_revision = i64::MAX;
    cases.push(b);
    for b in cases {
        assert!(import_bundle(&dest_root, &b, RECEIVER).is_err());
    }
    let conn = Connection::open(todo_db_path(&dest_root)).unwrap();
    assert!(capture_selected(&conn, &[id]).unwrap().is_empty());
    assert_eq!(verify(&conn).unwrap()["records"], 0);
}

#[test]
fn shared_or_connected_custody_requires_separate_review() {
    let (_source, root, id) = source();
    for change in ["owner", "outgoing", "incoming"] {
        let conn = Connection::open(todo_db_path(&root)).unwrap();
        let mut p = projection(&root, &id);
        match change {
            "owner" => p.owners.push(OwnerRow {
                id: "shared".into(),
                agent_id: "other".into(),
                claimed_at: now_iso(),
                claim_type: "secondary".into(),
            }),
            "outgoing" => p.task.depends_on = Some("other-task".into()),
            _ => {
                // Even an inbound multi-ID legacy declaration must be rejected.
                let mut other = p.task.clone();
                other.id = "other-task".into();
                other.hash = "other-hash".into();
                other.blocks = Some(format!("unrelated,{id}"));
                write_task(&conn, &other).unwrap();
            }
        }
        if change != "incoming" {
            assert!(standalone(&p).is_err());
        } else {
            assert!(export_bundle(&root, &id, RECEIVER, SOURCE).is_err());
        }
    }
}

#[test]
fn artifact_recovery_is_exact_private_and_never_overwrites() {
    let (_source, root, id) = source();
    let output_dir = tempfile::tempdir().unwrap();
    let output = output_dir.path().join("transfer.json");
    fs::write(&output, b"existing").unwrap();
    assert!(
        run(
            &root,
            &TodoTransferCommand::Export {
                id: id.clone(),
                to: RECEIVER.into(),
                output: output.clone()
            },
            SOURCE
        )
        .is_err()
    );
    let conn = Connection::open(todo_db_path(&root)).unwrap();
    ensure_not_exported(&conn, &id).unwrap();
    drop(conn);
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    assert!(publish(&output, &bundle).is_err());
    assert_eq!(fs::read(&output).unwrap(), b"existing");
    let recovered = recover_bundle(&root, &id, &bundle.transfer_id, SOURCE).unwrap();
    let new_output = output_dir.path().join("recovered.json");
    let report = publish(&new_output, &recovered).unwrap();
    assert_eq!(fs::read(&new_output).unwrap(), encode(&bundle).unwrap());
    assert_eq!(report["artifact_sha256"], sha(&encode(&bundle).unwrap()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let meta = fs::metadata(new_output).unwrap();
        assert_eq!(meta.permissions().mode() & 0o077, 0);
        assert_eq!(meta.nlink(), 1);
    }
}

#[test]
fn input_digest_unknown_schema_oversize_and_missing_acceptance_fail_closed() {
    let (source, root, id) = source();
    let bundle = export_bundle(&root, &id, RECEIVER, SOURCE).unwrap();
    let (_dest, dest_root) = destination(source.path());
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("package.json");
    let bytes = encode(&bundle).unwrap();
    fs_permissions::write_private(&input, &bytes).unwrap();
    assert!(
        run(
            &dest_root,
            &command_import(input.clone(), sha(b"wrong")),
            RECEIVER
        )
        .is_err()
    );
    assert!(
        run(
            &dest_root,
            &TodoTransferCommand::Import {
                input: input.clone(),
                expected_digest: sha(&bytes),
                accept_source_transfer: false
            },
            RECEIVER
        )
        .is_err()
    );
    let mut unknown = serde_json::to_value(&bundle).unwrap();
    unknown["credentials"] = json!("never accepted");
    let bytes = serde_json::to_vec(&unknown).unwrap();
    fs_permissions::write_private(&input, &bytes).unwrap();
    assert!(
        run(
            &dest_root,
            &command_import(input.clone(), sha(&bytes)),
            RECEIVER
        )
        .is_err()
    );
    let huge = vec![b' '; MAX_PACKAGE + 1];
    fs_permissions::write_private(&input, &huge).unwrap();
    assert!(run(&dest_root, &command_import(input, sha(&huge)), RECEIVER).is_err());
    let conn = Connection::open(todo_db_path(&dest_root)).unwrap();
    assert!(capture_selected(&conn, &[id]).unwrap().is_empty());
}

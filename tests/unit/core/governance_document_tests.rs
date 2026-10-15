use super::*;
use tempfile::TempDir;

fn repo() -> TempDir {
    let root = TempDir::new().unwrap();
    git(root.path(), &["init", "-b", "master"]).unwrap();
    fs::write(root.path().join("source.rs"), "fn main() {}\n").unwrap();
    fs::write(
        root.path().join(".gitignore"),
        ".decapod/data/\n.decapod/managed/governance-document.lock\n",
    )
    .unwrap();
    git(root.path(), &["add", "source.rs", ".gitignore"]).unwrap();
    commit(root.path(), "baseline");
    root
}
fn commit(root: &Path, message: &str) {
    git(
        root,
        &[
            "-c",
            "user.name=Governance Tests",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-m",
            message,
        ],
    )
    .unwrap();
}
fn plan(title: &str) -> Value {
    json!({"schema_version":"1.1.0","title":title,"intent":"bounded work","state":"DRAFT","constraints":{},"updated_at":"0Z"})
}

#[test]
fn newer_schema_and_malformed_legacy_do_not_mutate() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".decapod/governance")).unwrap();
    let old = root.path().join(".decapod/governance/plan.json");
    fs::write(&old, b"not-json").unwrap();
    assert!(migrate(root.path()).is_err());
    assert_eq!(fs::read(&old).unwrap(), b"not-json");
    assert!(!root.path().join(GOVERNANCE_PATH).exists());
    fs::write(
        root.path().join(GOVERNANCE_PATH),
        br#"{"schema_version":"99.0.0"}"#,
    )
    .unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    assert!(migrate(root.path()).is_err());
    assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
}

#[test]
fn readonly_legacy_import_and_explicit_migration_preserve_data() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".decapod/governance")).unwrap();
    let old = root.path().join(".decapod/governance/plan.json");
    let value = plan("legacy intent");
    fs::write(&old, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(
        read_section(root.path(), "plan").unwrap(),
        Some(value.clone())
    );
    assert!(old.exists());
    assert!(!root.path().join(GOVERNANCE_PATH).exists());
    migrate(root.path()).unwrap();
    assert!(!old.exists());
    assert_eq!(read_section(root.path(), "plan").unwrap(), Some(value));
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::write(
        &old,
        serde_json::to_vec(&plan("conflicting intent")).unwrap(),
    )
    .unwrap();
    assert!(
        load(root.path())
            .unwrap_err()
            .to_string()
            .contains("AUTHORITY_CONFLICT")
    );
}

#[test]
fn current_claims_reset_only_at_archived_explicit_new_pr() {
    let root = repo();
    begin_pr(root.path(), "change-one", "master").unwrap();
    record_claim(
        root.path(),
        "bounded",
        "Works",
        "A counterexample",
        "supported",
        vec!["proof:one".into()],
    )
    .unwrap();
    record_claim(
        root.path(),
        "unresolved",
        "Pending",
        "A counterexample",
        "blocked",
        vec![],
    )
    .unwrap();
    assert!(begin_pr(root.path(), "change-two", "master").is_err());
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "first record");
    let next = begin_pr(root.path(), "change-two", "master").unwrap();
    assert!(next.claims.is_empty());
    assert_eq!(next.baseline.unresolved.len(), 1);
    assert!(next.baseline.unresolved.contains_key("claim:unresolved"));
    assert!(next.checkpoints.is_empty());
    assert!(next.baseline.release.is_none());
    assert_eq!(
        begin_pr(root.path(), "change-two", "master")
            .unwrap()
            .change
            .unwrap()
            .id,
        "change-two"
    );
}

#[test]
fn shared_document_writers_do_not_lose_other_sections_or_updates() {
    let root = TempDir::new().unwrap();
    write_section(root.path(), "plan", &plan("0")).unwrap();
    let mut threads = Vec::new();
    for _ in 0..8 {
        let root = root.path().to_path_buf();
        threads.push(std::thread::spawn(move || {
            for _ in 0..10 {
                with_lock(&root, || {
                    let mut value = read_section(&root, "plan")?.unwrap();
                    let n: u32 = value["title"].as_str().unwrap().parse().unwrap();
                    value["title"] = json!((n + 1).to_string());
                    write_section(&root, "plan", &value)
                })
                .unwrap();
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(
        read_section(root.path(), "plan").unwrap().unwrap()["title"],
        "80"
    );
}

#[test]
fn claims_require_explicit_pr_and_falsifiable_supported_evidence() {
    let root = repo();
    assert!(
        record_claim(
            root.path(),
            "x",
            "works",
            "fails",
            "supported",
            vec!["proof".into()]
        )
        .is_err()
    );
    begin_pr(root.path(), "c", "master").unwrap();
    assert!(record_claim(root.path(), "x", "works", "", "open", vec![]).is_err());
    assert!(record_claim(root.path(), "x", "works", "fails", "supported", vec![]).is_err());
    assert!(load(root.path()).unwrap().unwrap().claims.is_empty());
}

#[test]
fn per_commit_checkpoints_bind_material_and_keep_prior_records() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "series", "master").unwrap();
    write_section(root.path(), "plan", &plan("first")).unwrap();
    checkpoint(root.path(), "first", "Initial governed change", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "first");
    verify_pr_checkpoints(root.path(), &base, "HEAD").unwrap();
    fs::write(
        root.path().join("source.rs"),
        "fn main() { println!(\"changed\"); }\n",
    )
    .unwrap();
    assert!(checkpoint(root.path(), "second", "More code", vec![]).is_err());
    git(root.path(), &["add", "source.rs"]).unwrap();
    checkpoint(root.path(), "second", "More code", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "second");
    verify_pr_checkpoints(root.path(), &base, "HEAD").unwrap();
    let mut document = load(root.path()).unwrap().unwrap();
    document.checkpoints.remove(0);
    save(root.path(), &document).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "erase historical checkpoint");
    assert!(
        verify_pr_checkpoints(root.path(), &base, "HEAD")
            .unwrap_err()
            .to_string()
            .contains("HISTORY_CHANGED")
    );
}

#[test]
fn checkpoint_identical_retry_preserves_bytes_before_and_after_commit() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "series", "master").unwrap();
    checkpoint(
        root.path(),
        "first",
        "Initial work",
        vec!["proof:b".into(), "proof:a".into(), "proof:a".into()],
    )
    .unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    for committed in [false, true] {
        if committed {
            git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
            commit(root.path(), "first");
        }
        let retried = checkpoint(
            root.path(),
            "first",
            "Initial work",
            vec!["proof:a".into(), "proof:b".into()],
        )
        .unwrap();
        assert_eq!(retried.checkpoints.len(), 1);
        assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
    }
    verify_pr_checkpoints(root.path(), &base, "HEAD").unwrap();
}

#[test]
fn checkpoint_retry_preserves_noncanonical_stored_proof_reference_bytes() {
    let root = repo();
    begin_pr(root.path(), "series", "master").unwrap();
    let mut document = checkpoint(
        root.path(),
        "first",
        "Initial work",
        vec!["proof:a".into(), "proof:b".into()],
    )
    .unwrap();
    document.checkpoints[0].proof_refs = vec!["proof:b".into(), "proof:a".into(), "proof:a".into()];
    save(root.path(), &document).unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    checkpoint(
        root.path(),
        "first",
        "Initial work",
        vec!["proof:a".into(), "proof:b".into()],
    )
    .unwrap();
    assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
}

#[test]
fn checkpoint_conflicting_summary_or_proof_refs_preserves_history() {
    for (summary, refs) in [
        ("Rewritten work", vec!["proof:original".into()]),
        ("Initial work", vec!["proof:replacement".into()]),
        ("Initial work", vec![]),
    ] {
        let root = repo();
        begin_pr(root.path(), "series", "master").unwrap();
        checkpoint(
            root.path(),
            "first",
            "Initial work",
            vec!["proof:original".into()],
        )
        .unwrap();
        let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
        let error = checkpoint(root.path(), "first", summary, refs).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("GOVERNANCE_CHECKPOINT_ID_CONFLICT")
        );
        assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
    }
}

#[test]
fn checkpoint_changed_staged_material_requires_new_id_and_keeps_publishable_history() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "series", "master").unwrap();
    checkpoint(root.path(), "first", "Initial work", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "first");
    fs::write(root.path().join("source.rs"), "fn changed() {}\n").unwrap();
    git(root.path(), &["add", "source.rs"]).unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    assert!(
        checkpoint(root.path(), "first", "Initial work", vec![])
            .unwrap_err()
            .to_string()
            .contains("GOVERNANCE_CHECKPOINT_ID_CONFLICT")
    );
    assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
    let next = checkpoint(root.path(), "second", "Updated work", vec![]).unwrap();
    assert_eq!(next.checkpoints.len(), 2);
    assert_ne!(
        next.checkpoints[0].input_digest,
        next.checkpoints[1].input_digest
    );
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    verify_staged_checkpoint(root.path()).unwrap();
    commit(root.path(), "second");
    verify_pr_checkpoints(root.path(), &base, "HEAD").unwrap();
}

#[test]
fn checkpoint_changed_governance_inputs_require_new_id() {
    let root = repo();
    begin_pr(root.path(), "series", "master").unwrap();
    write_section(root.path(), "plan", &plan("original intent")).unwrap();
    checkpoint(root.path(), "first", "Initial work", vec![]).unwrap();
    write_section(root.path(), "plan", &plan("revised intent")).unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    assert!(
        checkpoint(root.path(), "first", "Initial work", vec![])
            .unwrap_err()
            .to_string()
            .contains("GOVERNANCE_CHECKPOINT_ID_CONFLICT")
    );
    assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
}

#[test]
fn concurrent_checkpoint_id_conflicts_do_not_overwrite_winner() {
    let root = repo();
    begin_pr(root.path(), "series", "master").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = ["First writer", "Second writer"]
        .into_iter()
        .map(|summary| {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                checkpoint(&root, "shared", summary, vec![])
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let winner = results
        .iter()
        .find_map(|result| result.as_ref().ok())
        .unwrap();
    let error = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .unwrap();
    assert!(
        error
            .to_string()
            .contains("GOVERNANCE_CHECKPOINT_ID_CONFLICT")
    );
    let final_document = load(root.path()).unwrap().unwrap();
    assert_eq!(final_document.checkpoints.len(), 1);
    assert_eq!(
        final_document.checkpoints[0].summary,
        winner.checkpoints[0].summary
    );
    assert_eq!(
        serde_json::to_value(&final_document).unwrap(),
        serde_json::to_value(winner).unwrap()
    );
}

#[test]
fn concurrent_identical_checkpoint_requests_are_idempotent() {
    let root = repo();
    begin_pr(root.path(), "series", "master").unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                checkpoint(&root, "shared", "Identical work", vec![])
            })
        })
        .collect();
    for handle in handles {
        assert_eq!(handle.join().unwrap().unwrap().checkpoints.len(), 1);
    }
    assert_eq!(load(root.path()).unwrap().unwrap().checkpoints.len(), 1);
}

#[test]
fn a_new_name_does_not_bless_an_old_material_digest() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "series", "master").unwrap();
    checkpoint(root.path(), "first", "First", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "first");
    fs::write(root.path().join("source.rs"), "fn different() {}\n").unwrap();
    let mut document = load(root.path()).unwrap().unwrap();
    let mut forged = document.checkpoints[0].clone();
    forged.id = "forged".into();
    forged.summary = "Changed summary".into();
    document.checkpoints.push(forged);
    save(root.path(), &document).unwrap();
    git(root.path(), &["add", "source.rs", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "unproven change");
    assert!(verify_pr_checkpoints(root.path(), &base, "HEAD").is_err());
}

fn history_checkpoint_commit(root: &Path, id: &str) -> String {
    git(root, &["add", "-A"]).unwrap();
    checkpoint(root, id, "Measured history fixture", vec![]).unwrap();
    git(root, &["add", GOVERNANCE_PATH]).unwrap();
    commit(root, id);
    git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn history_tree_commit(root: &Path, parents: &[&str]) -> String {
    let tree = git(root, &["write-tree"]).unwrap();
    let mut args = vec![
        "-c",
        "user.name=Governance Tests",
        "-c",
        "user.email=test@example.invalid",
        "commit-tree",
        &tree,
        "-m",
        "arbitrary history message, not a trusted transport marker",
    ];
    for parent in parents {
        args.extend(["-p", parent]);
    }
    git(root, &args).unwrap()
}

#[test]
fn history_same_tree_transport_reuses_only_verified_source_checkpoint() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "current", "master").unwrap();
    let source = history_checkpoint_commit(root.path(), "source");
    let transport = history_tree_commit(root.path(), &[&source]);
    assert_ne!(source, transport);
    verify_pr_checkpoints(root.path(), &base, &transport).unwrap();

    fs::write(
        root.path().join("source.rs"),
        "fn uncheckpointed_edit() {}\n",
    )
    .unwrap();
    git(root.path(), &["add", "source.rs"]).unwrap();
    let changed = history_tree_commit(root.path(), &[&source]);
    assert!(
        verify_pr_checkpoints(root.path(), &base, &changed)
            .unwrap_err()
            .to_string()
            .contains("CHECKPOINT_MISSING")
    );
}

#[test]
fn history_same_tree_transport_does_not_bless_uncheckpointed_source() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "current", "master").unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "source without checkpoint");
    let source = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    let transport = history_tree_commit(root.path(), &[&source]);
    assert!(
        verify_pr_checkpoints(root.path(), &base, &transport)
            .unwrap_err()
            .to_string()
            .contains("CHECKPOINT_MISSING")
    );
}

#[test]
fn history_transport_equality_includes_governance_and_merges_need_new_proof() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "current", "master").unwrap();
    let source = history_checkpoint_commit(root.path(), "source");
    let merge = history_tree_commit(root.path(), &[&source, &base]);
    assert!(
        verify_pr_checkpoints(root.path(), &base, &merge)
            .unwrap_err()
            .to_string()
            .contains("CHECKPOINT_MISSING")
    );
    let mut document = load(root.path()).unwrap().unwrap();
    document.checkpoints[0]
        .proof_refs
        .push("proof:rewritten".into());
    save(root.path(), &document).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    let changed_governance = history_tree_commit(root.path(), &[&source]);
    assert!(
        verify_pr_checkpoints(root.path(), &base, &changed_governance)
            .unwrap_err()
            .to_string()
            .contains("HISTORY_CHANGED")
    );
}

fn history_independent_lineage_fixture(
    side_checkpoint: bool,
    side_target: &str,
) -> (TempDir, String, String) {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    if side_target != "master" {
        git(root.path(), &["branch", side_target, &base]).unwrap();
    }
    git(root.path(), &["checkout", "-b", "imported"]).unwrap();
    begin_pr(root.path(), "imported-change", side_target).unwrap();
    fs::write(root.path().join("source.rs"), "fn imported() {}\n").unwrap();
    let side = if side_checkpoint {
        history_checkpoint_commit(root.path(), "side-source")
    } else {
        git(root.path(), &["add", "-A"]).unwrap();
        commit(root.path(), "unproven side source");
        git(root.path(), &["rev-parse", "HEAD"]).unwrap()
    };
    git(root.path(), &["checkout", "-b", "current", &base]).unwrap();
    begin_pr(root.path(), "current-change", "master").unwrap();
    let current = history_checkpoint_commit(root.path(), "current-source");
    checkpoint(
        root.path(),
        "current-merge",
        "Current merge checkpoint",
        vec![],
    )
    .unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    let merged = history_tree_commit(root.path(), &[&current, &side]);
    (root, base, merged)
}

#[test]
fn history_imported_lineage_keeps_its_own_verified_journal() {
    let (root, base, merged) = history_independent_lineage_fixture(true, "master");
    let document = load(root.path()).unwrap().unwrap();
    assert!(
        !document
            .checkpoints
            .iter()
            .any(|entry| entry.id == "side-source")
    );
    verify_pr_checkpoints_for_target(root.path(), &base, &merged, "master").unwrap();
}

#[test]
fn history_imported_lineage_requires_source_proof_and_matching_target() {
    let (root, base, merged) = history_independent_lineage_fixture(false, "master");
    assert!(
        verify_pr_checkpoints_for_target(root.path(), &base, &merged, "master")
            .unwrap_err()
            .to_string()
            .contains("CHECKPOINT_MISSING")
    );
    let (root, base, merged) = history_independent_lineage_fixture(true, "other-target");
    assert!(
        verify_pr_checkpoints_for_target(root.path(), &base, &merged, "master")
            .unwrap_err()
            .to_string()
            .contains("TARGET_MISMATCH")
    );
}

#[test]
fn history_first_parent_identity_remains_strict() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "first-change", "master").unwrap();
    history_checkpoint_commit(root.path(), "first");
    // The supported lifecycle rejects replacing an unaccepted journal.
    let error = begin_pr_from_context(root.path(), "second-change", "master", Some(&base), None)
        .unwrap_err();
    assert!(error.to_string().contains("GOVERNANCE_PR_NOT_ACCEPTED"));

    // Deliberately construct malformed history in this disposable fixture to
    // exercise the verifier independently of the lifecycle guard above.
    let mut document = load(root.path()).unwrap().unwrap();
    document.change.as_mut().unwrap().id = "second-change".into();
    save(root.path(), &document).unwrap();
    history_checkpoint_commit(root.path(), "second");
    assert!(
        verify_pr_checkpoints(root.path(), &base, "HEAD")
            .unwrap_err()
            .to_string()
            .contains("IDENTITY_CONFLICT")
    );
}

#[test]
fn history_accepted_squash_equivalent_lineage_still_verifies_original_proof() {
    let root = repo();
    let original_base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    git(root.path(), &["checkout", "-b", "historical"]).unwrap();
    begin_pr(root.path(), "historical-change", "master").unwrap();
    let historical = history_checkpoint_commit(root.path(), "historical-source");
    // The accepted commit has exactly the historical tree but a different ID.
    let accepted = history_tree_commit(root.path(), &[&original_base]);
    assert_ne!(historical, accepted);
    git(root.path(), &["checkout", "-B", "master", &accepted]).unwrap();
    git(root.path(), &["checkout", "-b", "current"]).unwrap();
    begin_pr(root.path(), "current-change", "master").unwrap();
    let current = history_checkpoint_commit(root.path(), "current-source");
    checkpoint(
        root.path(),
        "current-merge",
        "Merge original chronology",
        vec![],
    )
    .unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    let merged = history_tree_commit(root.path(), &[&current, &historical]);
    verify_pr_checkpoints_for_target(root.path(), &accepted, &merged, "master").unwrap();
}

#[test]
fn stored_cleanup_paths_cannot_delete_other_files() {
    let root = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".decapod")).unwrap();
    let victim = root.path().join("important.txt");
    fs::write(&victim, "keep").unwrap();
    for path in [
        "important.txt".to_string(),
        "../outside.json".to_string(),
        victim.to_string_lossy().into_owned(),
        ".decapod/governance/recursive_passes/../../important.txt".to_string(),
    ] {
        let mut document = GovernanceDocument::default();
        document.legacy_digests.insert(path, bytes_digest(b"keep"));
        fs::write(
            root.path().join(GOVERNANCE_PATH),
            serde_json::to_vec(&document).unwrap(),
        )
        .unwrap();
        let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
        assert!(migrate(root.path()).is_err());
        assert_eq!(fs::read(&victim).unwrap(), b"keep");
        assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
    }
}

#[test]
fn migrate_then_begin_pr_does_not_require_a_bootstrap_commit() {
    let root = repo();
    fs::create_dir_all(root.path().join(".decapod/governance")).unwrap();
    fs::write(
        root.path().join(".decapod/governance/plan.json"),
        serde_json::to_vec(&plan("old accepted work")).unwrap(),
    )
    .unwrap();
    git(root.path(), &["add", ".decapod/governance/plan.json"]).unwrap();
    commit(root.path(), "legacy accepted state");
    migrate(root.path()).unwrap();
    let next = begin_pr(root.path(), "new", "master").unwrap();
    assert!(next.sections.is_empty());
    assert_eq!(next.change.unwrap().id, "new");
}

#[test]
fn unmerged_record_and_conflicting_target_cannot_be_relabelled_accepted() {
    let root = repo();
    git(root.path(), &["checkout", "-b", "feature"]).unwrap();
    begin_pr(root.path(), "one", "master").unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "open PR record");
    assert!(
        begin_pr(root.path(), "two", "master")
            .unwrap_err()
            .to_string()
            .contains("NOT_ACCEPTED")
    );
    assert!(
        begin_pr(root.path(), "one", "feature")
            .unwrap_err()
            .to_string()
            .contains("IDENTITY_CONFLICT")
    );
}

#[cfg(unix)]
#[test]
fn shared_datastore_directory_allows_document_updates() {
    let root = TempDir::new().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "core::governance_document::tests::shared_directory_child",
            "--nocapture",
        ])
        .env("DECAPOD_STORAGE_SHARED_GROUP", "1")
        .env("GOVERNANCE_SHARED_TEST_ROOT", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
fn shared_directory_child() {
    use std::os::unix::fs::PermissionsExt;
    let Some(root) = std::env::var_os("GOVERNANCE_SHARED_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let data = root.join(".decapod/data");
    fs::create_dir_all(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o2770)).unwrap();
    write_section(&root, "plan", &plan("group writable datastore preserved")).unwrap();
    assert_eq!(
        fs::metadata(data).unwrap().permissions().mode() & 0o7777,
        0o2770
    );
}

#[test]
fn fresh_empty_init_can_begin_without_a_bootstrap_commit() {
    let root = repo();
    migrate(root.path()).unwrap();
    begin_pr(root.path(), "first", "master").unwrap();
    checkpoint(root.path(), "first", "First work", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    verify_staged_checkpoint(root.path()).unwrap();
}

#[test]
fn accepted_pr_identity_cannot_be_reused() {
    let root = repo();
    begin_pr(root.path(), "one", "master").unwrap();
    checkpoint(root.path(), "first", "Accepted work", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "accepted");
    let accepted = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    git(root.path(), &["checkout", "-b", "next"]).unwrap();
    checkpoint(
        root.path(),
        "second",
        "More work without new identity",
        vec![],
    )
    .unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "reused");
    assert!(
        verify_pr_checkpoints(root.path(), &accepted, "HEAD")
            .unwrap_err()
            .to_string()
            .contains("RESET_REQUIRED")
    );
}

#[test]
fn merge_only_heads_require_their_own_material_checkpoint() {
    let root = repo();
    let base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    begin_pr(root.path(), "one", "master").unwrap();
    checkpoint(root.path(), "first", "Work", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "first");
    let prior = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    let tree = git(root.path(), &["write-tree"]).unwrap();
    let merged = git(
        root.path(),
        &[
            "-c",
            "user.name=Tests",
            "-c",
            "user.email=test@example.invalid",
            "commit-tree",
            &tree,
            "-p",
            &prior,
            "-p",
            &base,
            "-m",
            "merge without checkpoint",
        ],
    )
    .unwrap();
    assert!(verify_pr_checkpoints(root.path(), &prior, &merged).is_err());
    git(root.path(), &["rm", "--cached", GOVERNANCE_PATH]).unwrap();
    let deleted_tree = git(root.path(), &["write-tree"]).unwrap();
    let removed = git(
        root.path(),
        &[
            "-c",
            "user.name=Tests",
            "-c",
            "user.email=test@example.invalid",
            "commit-tree",
            &deleted_tree,
            "-p",
            &prior,
            "-p",
            &base,
            "-m",
            "merge removes governance",
        ],
    )
    .unwrap();
    assert!(verify_pr_checkpoints(root.path(), &base, &removed).is_err());
}

#[test]
fn resolved_obligations_retire_with_evidence_in_the_current_pr() {
    let root = repo();
    begin_pr(root.path(), "first", "master").unwrap();
    record_claim(
        root.path(),
        "open",
        "Unproven",
        "Counterexample",
        "blocked",
        vec![],
    )
    .unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "accepted with obligation");
    begin_pr(root.path(), "second", "master").unwrap();
    assert!(resolve_obligation(root.path(), "claim:open", "Answer", vec![]).is_err());
    let resolved = resolve_obligation(
        root.path(),
        "claim:open",
        "Counterexample addressed",
        vec!["proof:measured".into()],
    )
    .unwrap();
    assert!(resolved.baseline.unresolved.is_empty());
    assert_eq!(resolved.resolutions.len(), 1);
    assert!(
        resolved.resolutions["claim:open"]
            .obligation_digest
            .starts_with("sha256:")
    );
}

#[cfg(unix)]
#[test]
fn direct_workunit_write_cannot_follow_a_legacy_ancestor_symlink() {
    use std::os::unix::fs::symlink;
    let root = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(root.path().join(".decapod")).unwrap();
    fs::create_dir_all(outside.path().join("workunits")).unwrap();
    fs::write(
        outside.path().join("workunits/do-not-touch.json"),
        "preserve",
    )
    .unwrap();
    symlink(outside.path(), root.path().join(".decapod/governance")).unwrap();
    assert!(crate::core::workunit::init_workunit(root.path(), "task_new", "intent:new").is_err());
    assert_eq!(
        fs::read(outside.path().join("workunits/do-not-touch.json")).unwrap(),
        b"preserve"
    );
}

#[test]
fn epoch_pruning_preserves_all_live_consumers_and_rejects_dangling_references() {
    let root = repo();
    let mut document = begin_pr(root.path(), "epoch-consumers", "master").unwrap();
    let epoch = crate::core::validation_epoch::active_validation_epoch(root.path()).unwrap();
    let id = epoch.epoch_id.clone();
    document
        .epochs
        .insert(id.clone(), serde_json::to_value(epoch).unwrap());
    document
        .epochs
        .insert("unused".into(), json!({"epoch_id":"unused"}));
    document.claims.insert(
        "observed".into(),
        ActiveClaim {
            statement: "Observed result".into(),
            falsifier: "Contrary observation".into(),
            status: "supported".into(),
            proof_refs: vec![format!("epoch:{id}")],
        },
    );
    prune_epochs(&mut document).unwrap();
    assert_eq!(document.epochs.len(), 1);
    assert!(document.epochs.contains_key(&id));
    save(root.path(), &document).unwrap();
    write_section(root.path(), "plan", &plan("next validation context")).unwrap();
    assert!(load(root.path()).unwrap().unwrap().epochs.contains_key(&id));
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    assert!(
        record_claim(
            root.path(),
            "bad",
            "Statement",
            "Falsifier",
            "supported",
            vec!["epoch:missing".into()]
        )
        .is_err()
    );
    assert_eq!(before, fs::read(root.path().join(GOVERNANCE_PATH)).unwrap());
    document.claims.clear();
    document
        .baseline
        .unresolved
        .insert("loop".into(), json!({"proof_refs":[format!("epoch:{id}")]}));
    prune_epochs(&mut document).unwrap();
    assert!(document.epochs.contains_key(&id));
    document.baseline.unresolved.clear();
    prune_epochs(&mut document).unwrap();
    assert!(document.epochs.is_empty());
}

#[test]
fn workunit_initialization_cannot_overwrite_existing_legacy_proof() {
    use crate::core::workunit::{self, WorkUnitStatus};
    let root = TempDir::new().unwrap();
    let mut manifest = workunit::init_workunit(root.path(), "task_old", "intent:original").unwrap();
    manifest.status = WorkUnitStatus::Verified;
    workunit::write_workunit(root.path(), &manifest).unwrap();
    let source = workunit::workunit_path(root.path(), "task_old").unwrap();
    let legacy = root
        .path()
        .join(".decapod/governance/workunits/task_old.json");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::rename(source, &legacy).unwrap();
    assert!(
        workunit::init_workunit(root.path(), "task_old", "intent:replacement")
            .unwrap_err()
            .to_string()
            .contains("already exists")
    );
    assert_eq!(
        workunit::load_workunit(root.path(), "task_old").unwrap(),
        manifest
    );
}

#[test]
fn changed_recursive_source_rejects_the_entire_canonical_write() {
    let root = TempDir::new().unwrap();
    migrate(root.path()).unwrap();
    let canonical = root.path().join(GOVERNANCE_PATH);
    let mut document = load(root.path()).unwrap().unwrap();
    let relative = ".decapod/governance/recursive_passes/pass.json";
    let path = root.path().join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"old source").unwrap();
    document
        .legacy_digests
        .insert(relative.into(), bytes_digest(b"old source"));
    fs::write(&canonical, serde_json::to_vec(&document).unwrap()).unwrap();
    let before = fs::read(&canonical).unwrap();
    fs::write(&path, b"changed source").unwrap();
    assert!(write_section(root.path(), "plan", &plan("must not save")).is_err());
    assert_eq!(before, fs::read(&canonical).unwrap());
    assert_eq!(fs::read(path).unwrap(), b"changed source");
}

#[test]
fn strict_envelope_schema_matches_empty_and_populated_documents() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../assets/schemas/governance.schema.json"
    ))
    .unwrap();
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        SCHEMA_VERSION
    );
    assert_eq!(schema["additionalProperties"], false);
    let value = serde_json::to_value(GovernanceDocument::default()).unwrap();
    for key in value.as_object().unwrap().keys() {
        assert!(
            schema["properties"].get(key).is_some(),
            "schema misses {key}"
        );
    }
    assert!(parse(br#"{"schema_version":"1.0.0","unexpected":true}"#).is_err());
}

#[test]
fn related_sections_are_atomic_and_identical_updates_do_not_rewrite() {
    let root = TempDir::new().unwrap();
    let value = plan("single save");
    write_sections(root.path(), &[("plan", Some(value.clone()))]).unwrap();
    let path = root.path().join(GOVERNANCE_PATH);
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    write_sections(root.path(), &[("plan", Some(value))]).unwrap();
    assert_eq!(modified, fs::metadata(&path).unwrap().modified().unwrap());
    assert!(
        write_sections(
            root.path(),
            &[
                ("plan", Some(plan("must roll back"))),
                ("validation", Some(json!({"bad":"receipt"})))
            ]
        )
        .is_err()
    );
    assert_eq!(before, fs::read(&path).unwrap());
}

#[test]
fn pending_spec_review_survives_compact_boundary_and_context_is_proof_material() {
    let root = repo();
    let mut value = plan("prior plan");
    value["spec_reviews"] = json!([{
        "path":".decapod/managed/specs/SECURITY.md", "spec_material_hash":"sha256:abc",
        "reviewed_code_fingerprint":"abc", "disposition":"requires_decision",
        "reason":"An explicit security decision remains open"
    }]);
    let legacy = root.path().join(".decapod/governance/plan.json");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, serde_json::to_vec(&value).unwrap()).unwrap();
    git(root.path(), &["add", "."]).unwrap();
    commit(root.path(), "legacy review obligation");
    let document = begin_pr(root.path(), "review-carry", "master").unwrap();
    assert!(
        document
            .baseline
            .unresolved
            .contains_key("spec-review:.decapod/managed/specs/SECURITY.md")
    );
    let before = validation_context_hash(root.path()).unwrap();
    resolve_obligation(
        root.path(),
        "spec-review:.decapod/managed/specs/SECURITY.md",
        "Approved bounded decision",
        vec!["decision:review-123".into()],
    )
    .unwrap();
    assert_ne!(before, validation_context_hash(root.path()).unwrap());
}

#[test]
fn malformed_normalized_values_return_errors_instead_of_panicking() {
    for sections in [
        json!({"trajectory":42}),
        json!({"validation":false}),
        json!({"trajectory":{"schema_version":"unsupported"}}),
        json!({"trajectory":{"schema_version":"1.1.0", "custody":42}}),
    ] {
        let value = json!({"schema_version":SCHEMA_VERSION, "sections":sections});
        assert!(parse(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

fn branch_context() -> BranchWorkContext {
    BranchWorkContext {
        title: "Fresh branch work".into(),
        intent: "Implement only the newly claimed task".into(),
        todo_ids: vec!["code_current".into()],
        scope: vec!["src/current.rs".into(), "tests/current.rs".into()],
    }
}

/// A fully archived journal with both completed evidence and open obligations.
fn accepted_branch_repo() -> TempDir {
    use crate::core::trajectory::{self, TrajectoryCheck, TrajectoryCheckStatus, TrajectoryInit};
    use crate::core::validate::ValidationReceipt;

    let root = repo();
    begin_pr(root.path(), "accepted-change", "master").unwrap();
    let mut accepted_plan = plan("Accepted work");
    accepted_plan["state"] = json!("DONE");
    accepted_plan["todo_ids"] = json!(["code_accepted"]);
    accepted_plan["unknowns"] = json!(["An accepted-plan question remains open"]);
    accepted_plan["spec_reviews"] = json!([{
        "path": ".decapod/managed/specs/SECURITY.md",
        "spec_material_hash": "sha256:accepted-spec",
        "reviewed_code_fingerprint": "accepted-code",
        "disposition": "requires_decision",
        "reason": "A human policy decision remains open"
    }]);
    write_section(root.path(), "plan", &accepted_plan).unwrap();
    let mut run = trajectory::fresh_trajectory(
        root.path(),
        TrajectoryInit {
            run_id: "accepted-run".into(),
            task_id: Some("code_accepted".into()),
            intent_id: Some("intent:accepted".into()),
            original_intent: "Complete the accepted task".into(),
            derived_intent: "Complete the accepted task".into(),
            active_boundaries: vec!["accepted scope".into()],
            repo_scope: vec!["src/accepted.rs".into()],
            destination: Some("pull request".into()),
            current_phase: Some("complete".into()),
            next_transitions: vec![],
            blockers: vec!["A carried operational blocker".into()],
        },
    )
    .unwrap();
    run.checks.push(TrajectoryCheck {
        name: "accepted test".into(),
        status: TrajectoryCheckStatus::Passed,
    });
    run.evidence.push("proof:accepted-result".into());
    run.completion_claim = Some("Accepted implementation is complete".into());
    run.unresolved_assumptions
        .push("A carried assumption".into());
    let run = run.with_recomputed_hash().unwrap();
    write_section(
        root.path(),
        "trajectory",
        &serde_json::to_value(&run).unwrap(),
    )
    .unwrap();
    record_claim(
        root.path(),
        "accepted-supported",
        "Accepted behavior worked",
        "A contrary result",
        "supported",
        vec!["proof:accepted-result".into()],
    )
    .unwrap();
    record_claim(
        root.path(),
        "accepted-open",
        "The open obligation still needs proof",
        "A contrary result",
        "blocked",
        vec![],
    )
    .unwrap();
    let receipt = ValidationReceipt {
        schema_version: "1.0.0".into(),
        kind: "validation_receipt".into(),
        decapod_release: crate::core::entrypoint_integrity::RELEASE_VERSION.into(),
        git_revision: git(root.path(), &["rev-parse", "HEAD"]).unwrap(),
        repo_signal_fingerprint: "sha256:accepted-repository".into(),
        trajectory_run_id: Some(run.run_id),
        trajectory_artifact_hash: Some(run.artifact_hash),
        validation_epoch: crate::core::validation_epoch::active_validation_epoch(root.path())
            .unwrap(),
        status: "ok".into(),
        pass_count: 1,
        fail_count: 0,
        warn_count: 0,
        elapsed_ms: 1,
        drift_findings: vec![],
        temporary_artifacts_cleaned: 0,
        failures: None,
        warnings: None,
        gate_timings: None,
        parallelism: None,
        ci_prediction: None,
        receipt_hash: String::new(),
    }
    .with_recomputed_hash()
    .unwrap();
    write_section(
        root.path(),
        "validation",
        &serde_json::to_value(receipt).unwrap(),
    )
    .unwrap();
    let mut document = load(root.path()).unwrap().unwrap();
    document
        .baseline
        .policy
        .insert("review_policy".into(), json!({"required": true}));
    document
        .baseline
        .unresolved
        .insert("baseline:existing".into(), json!("Keep this obligation"));
    save(root.path(), &document).unwrap();
    checkpoint(
        root.path(),
        "accepted-checkpoint",
        "Accepted evidence",
        vec![],
    )
    .unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "accepted governance journal");
    root
}

#[test]
fn workbranch_template_has_no_accepted_identity_or_proof() {
    let template = parse(include_bytes!("../../../assets/templates/governance.json")).unwrap();
    assert!(template.change.is_none());
    assert!(template.baseline.accepted_commit.is_none());
    assert!(template.claims.is_empty());
    assert!(template.checkpoints.is_empty());
    assert!(template.epochs.is_empty());
    assert!(template.intents.is_empty());
    assert_eq!(template.sections.len(), 1);
    let draft = get_section(&template, "plan").unwrap().unwrap();
    assert_eq!(draft["state"], "DRAFT");
    assert_eq!(draft["title"], "");
    assert_eq!(draft["intent"], "");
    assert_eq!(draft["todo_ids"], json!([]));
}

#[test]
fn workbranch_compacts_accepted_proof_and_seeds_only_current_task_context() {
    let root = accepted_branch_repo();
    let accepted_head = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    let accepted_bytes = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    git(root.path(), &["checkout", "-b", "fresh-work"]).unwrap();
    let context = branch_context();
    let next = initialize_workbranch(root.path(), "master", Some(&context))
        .unwrap()
        .unwrap();
    let change = next.change.as_ref().unwrap();
    assert_ne!(change.id, "accepted-change");
    assert_eq!(change.base_commit, accepted_head);
    assert_eq!(change.target_base, "master");
    assert_eq!(
        next.baseline.accepted_commit.as_deref(),
        Some(accepted_head.as_str())
    );
    assert_eq!(
        next.baseline.policy["review_policy"],
        json!({"required": true})
    );
    assert_eq!(
        next.baseline.unresolved["baseline:existing"],
        "Keep this obligation"
    );
    assert!(next.baseline.unresolved.contains_key("claim:accepted-open"));
    assert!(
        !next
            .baseline
            .unresolved
            .contains_key("claim:accepted-supported")
    );
    assert!(
        next.baseline
            .unresolved
            .contains_key("spec-review:.decapod/managed/specs/SECURITY.md")
    );
    for obligation in [
        "An accepted-plan question remains open",
        "A carried operational blocker",
        "A carried assumption",
    ] {
        assert!(
            next.baseline
                .unresolved
                .values()
                .any(|value| value == &json!(obligation))
        );
    }
    assert!(next.claims.is_empty());
    assert!(next.checkpoints.is_empty());
    assert!(next.resolutions.is_empty());
    assert!(next.epochs.is_empty());
    assert!(get_section(&next, "validation").unwrap().is_none());
    assert!(!next.intents.contains_key("intent:accepted"));
    let draft = get_section(&next, "plan").unwrap().unwrap();
    assert_eq!(draft["state"], "DRAFT");
    assert_eq!(draft["title"], context.title);
    assert_eq!(draft["intent"], context.intent);
    assert_eq!(draft["todo_ids"], json!(context.todo_ids));
    let run = crate::core::trajectory::load_trajectory_cookie(root.path())
        .unwrap()
        .unwrap();
    assert_ne!(run.run_id, "accepted-run");
    assert_eq!(run.intent_id, Some(format!("intent:{}", change.id)));
    assert_eq!(run.task_id.as_deref(), Some("code_current"));
    assert_eq!(run.original_intent, context.intent);
    assert_eq!(run.derived_intent, context.intent);
    assert_eq!(run.repo_scope, context.scope);
    assert!(run.checks.is_empty());
    assert!(run.evidence.is_empty());
    assert!(run.completion_claim.is_none());
    assert_eq!(
        run.proof_status,
        crate::core::trajectory::TrajectoryProofStatus::NoChecksRun
    );
    assert_eq!(
        run.verdicts.completion_proof,
        crate::core::trajectory::TrajectoryVerdict::Unsupported
    );
    assert_eq!(
        parse(
            git(root.path(), &["show", &format!("master:{GOVERNANCE_PATH}")])
                .unwrap()
                .as_bytes()
        )
        .unwrap()
        .change
        .unwrap()
        .id,
        "accepted-change"
    );
    assert_ne!(
        accepted_bytes,
        fs::read(root.path().join(GOVERNANCE_PATH)).unwrap()
    );
}

#[test]
fn workbranch_repeated_initialization_preserves_uncommitted_current_evidence() {
    let root = accepted_branch_repo();
    git(root.path(), &["checkout", "-b", "repeat-work"]).unwrap();
    let first = initialize_workbranch(root.path(), "master", Some(&branch_context()))
        .unwrap()
        .unwrap();
    record_claim(
        root.path(),
        "current",
        "Work in progress",
        "Counterexample",
        "open",
        vec![],
    )
    .unwrap();
    let path = root.path().join(GOVERNANCE_PATH);
    let before = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let mut unrelated_context = branch_context();
    unrelated_context.title = "Do not replace the current plan".into();
    for context in [None, Some(&unrelated_context), Some(&unrelated_context)] {
        let repeated = initialize_workbranch(root.path(), "master", context)
            .unwrap()
            .unwrap();
        assert_eq!(
            repeated.change.unwrap().id,
            first.change.as_ref().unwrap().id
        );
        assert!(repeated.claims.contains_key("current"));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    }
}

#[test]
fn workbranch_concurrent_initialization_creates_one_durable_identity() {
    let root = accepted_branch_repo();
    git(root.path(), &["checkout", "-b", "concurrent-work"]).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                initialize_workbranch(&root, "master", Some(&branch_context()))
                    .unwrap()
                    .unwrap()
            })
        })
        .collect();
    let snapshots: Vec<_> = threads
        .into_iter()
        .map(|thread| serde_json::to_value(thread.join().unwrap()).unwrap())
        .collect();
    assert!(snapshots.iter().all(|document| document == &snapshots[0]));
    assert_eq!(
        serde_json::to_value(load(root.path()).unwrap().unwrap()).unwrap(),
        snapshots[0]
    );
}

#[test]
fn workbranch_identity_and_evidence_survive_branch_rename_and_rebase() {
    let root = accepted_branch_repo();
    git(root.path(), &["checkout", "-b", "before-rename"]).unwrap();
    initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap();
    record_claim(
        root.path(),
        "current",
        "Current proof",
        "Counterexample",
        "supported",
        vec!["proof:current".into()],
    )
    .unwrap();
    checkpoint(root.path(), "current-checkpoint", "Current work", vec![]).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "current branch work");
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    git(root.path(), &["branch", "-m", "after-rename"]).unwrap();
    initialize_workbranch(root.path(), "master", None).unwrap();
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    git(root.path(), &["checkout", "master"]).unwrap();
    fs::write(
        root.path().join("upstream.rs"),
        "// independent upstream change\n",
    )
    .unwrap();
    git(root.path(), &["add", "upstream.rs"]).unwrap();
    commit(root.path(), "advance upstream");
    git(root.path(), &["checkout", "after-rename"]).unwrap();
    git(
        root.path(),
        &[
            "-c",
            "user.name=Governance Tests",
            "-c",
            "user.email=test@example.invalid",
            "rebase",
            "master",
        ],
    )
    .unwrap();
    initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap();
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
}

#[test]
fn workbranch_protected_refs_do_not_initialize_or_rewrite_governance() {
    let root = accepted_branch_repo();
    let path = root.path().join(GOVERNANCE_PATH);
    let before = fs::read(&path).unwrap();
    assert!(
        initialize_workbranch(root.path(), "master", Some(&branch_context()))
            .unwrap()
            .is_none()
    );
    git(root.path(), &["checkout", "-b", "main"]).unwrap();
    assert!(
        initialize_workbranch(root.path(), "master", Some(&branch_context()))
            .unwrap()
            .is_none()
    );
    git(root.path(), &["checkout", "-b", "release"]).unwrap();
    for target in [
        "release",
        "refs/heads/release",
        "origin/release",
        "refs/remotes/origin/release",
    ] {
        assert!(
            initialize_workbranch(root.path(), target, Some(&branch_context()))
                .unwrap()
                .is_none()
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn workbranch_dirty_inherited_journal_fails_closed_even_when_staged() {
    for staged in [false, true] {
        let root = accepted_branch_repo();
        git(root.path(), &["checkout", "-b", "dirty-inherited"]).unwrap();
        write_section(root.path(), "plan", &plan("Unarchived user edits")).unwrap();
        if staged {
            git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
        }
        let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
        let error =
            initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
        assert!(error.to_string().contains("UNARCHIVED_WORK"), "{error}");
        assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    }
}

#[test]
fn workbranch_malformed_inherited_journal_is_not_repaired_by_reset() {
    for malformed in [
        b"not-json".as_slice(),
        br#"{"schema_version":"99.0.0"}"#.as_slice(),
    ] {
        let root = accepted_branch_repo();
        git(root.path(), &["checkout", "-b", "malformed-inherited"]).unwrap();
        fs::write(root.path().join(GOVERNANCE_PATH), malformed).unwrap();
        assert!(initialize_workbranch(root.path(), "master", Some(&branch_context())).is_err());
        assert_eq!(
            fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(),
            malformed
        );
    }
}

#[test]
fn workbranch_deleted_inherited_journal_fails_closed_without_recreating_it() {
    let root = accepted_branch_repo();
    let base_before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    git(root.path(), &["checkout", "-b", "deleted-inherited"]).unwrap();
    let path = root.path().join(GOVERNANCE_PATH);
    fs::remove_file(&path).unwrap();
    let status_before = git(root.path(), &["status", "--porcelain=v1"]).unwrap();
    let error = initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
    assert!(
        error.to_string().contains("GOVERNANCE_UNARCHIVED_WORK"),
        "{error}"
    );
    assert!(
        !path.exists(),
        "a deleted inherited journal must not be silently recreated"
    );
    assert_eq!(
        git(root.path(), &["status", "--porcelain=v1"]).unwrap(),
        status_before
    );
    let base_after = Command::new("git")
        .current_dir(root.path())
        .args(["show", &format!("master:{GOVERNANCE_PATH}")])
        .output()
        .unwrap();
    assert!(base_after.status.success());
    assert_eq!(base_after.stdout, base_before);
}

#[test]
fn workbranch_default_replacement_cannot_discard_inherited_obligations() {
    let root = accepted_branch_repo();
    let path = root.path().join(GOVERNANCE_PATH);
    let base_before = fs::read(&path).unwrap();
    let accepted = parse(&base_before).unwrap();
    assert!(
        accepted
            .baseline
            .unresolved
            .contains_key("baseline:existing")
    );
    assert!(accepted.claims.contains_key("accepted-open"));
    git(
        root.path(),
        &["checkout", "-b", "empty-inherited-replacement"],
    )
    .unwrap();
    let replacement = br#"{"schema_version":"1.0.0"}"#;
    assert!(parse(replacement).unwrap().change.is_none());
    fs::write(&path, replacement).unwrap();
    let status_before = git(root.path(), &["status", "--porcelain=v1"]).unwrap();
    let error = initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
    assert!(
        error.to_string().contains("GOVERNANCE_UNARCHIVED_WORK"),
        "{error}"
    );
    assert_eq!(fs::read(&path).unwrap(), replacement);
    assert_eq!(
        git(root.path(), &["status", "--porcelain=v1"]).unwrap(),
        status_before
    );
    let base_after = Command::new("git")
        .current_dir(root.path())
        .args(["show", &format!("master:{GOVERNANCE_PATH}")])
        .output()
        .unwrap();
    assert!(base_after.status.success());
    assert_eq!(base_after.stdout, base_before);
}

#[test]
fn workbranch_partial_or_full_legacy_deletion_cannot_discard_accepted_obligations() {
    for remove_claims in [false, true] {
        let root = repo();
        let legacy_plan = ".decapod/governance/plan.json";
        let legacy_claims = ".decapod/governance/claims.json";
        let mut accepted_plan = plan("Accepted legacy work");
        accepted_plan["unknowns"] = json!(["Do not lose this unresolved legacy question"]);
        let accepted_claims: Value =
            serde_json::from_str(include_str!("../../../assets/templates/claims.json")).unwrap();
        let plan_bytes = serde_json::to_vec_pretty(&accepted_plan).unwrap();
        let claims_bytes = serde_json::to_vec_pretty(&accepted_claims).unwrap();
        fs::create_dir_all(root.path().join(".decapod/governance")).unwrap();
        fs::write(root.path().join(legacy_plan), &plan_bytes).unwrap();
        fs::write(root.path().join(legacy_claims), &claims_bytes).unwrap();
        let accepted = load(root.path()).unwrap().unwrap();
        assert_eq!(accepted.legacy_digests.len(), 2);
        assert_eq!(accepted.legacy_claims.as_ref(), Some(&accepted_claims));
        git(root.path(), &["add", legacy_plan, legacy_claims]).unwrap();
        commit(root.path(), "accepted legacy obligations");
        git(root.path(), &["checkout", "-b", "deleted-legacy-context"]).unwrap();
        fs::remove_file(root.path().join(legacy_plan)).unwrap();
        if remove_claims {
            fs::remove_file(root.path().join(legacy_claims)).unwrap();
        }
        let status_before = git(root.path(), &["status", "--porcelain=v1"]).unwrap();
        let error =
            initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
        assert!(
            error.to_string().contains("GOVERNANCE_UNARCHIVED_WORK"),
            "{error}"
        );
        assert!(!root.path().join(GOVERNANCE_PATH).exists());
        assert!(!root.path().join(legacy_plan).exists());
        if remove_claims {
            assert!(!root.path().join(legacy_claims).exists());
        } else {
            assert_eq!(
                fs::read(root.path().join(legacy_claims)).unwrap(),
                claims_bytes
            );
        }
        assert_eq!(
            git(root.path(), &["status", "--porcelain=v1"]).unwrap(),
            status_before
        );
        for (relative, expected) in [(legacy_plan, &plan_bytes), (legacy_claims, &claims_bytes)] {
            let archived = Command::new("git")
                .current_dir(root.path())
                .args(["show", &format!("master:{relative}")])
                .output()
                .unwrap();
            assert!(archived.status.success());
            assert_eq!(&archived.stdout, expected);
        }
    }
}

#[test]
fn workbranch_uses_preferred_remote_base_when_local_master_is_stale() {
    let root = accepted_branch_repo();
    let accepted = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    let stale = git(root.path(), &["rev-parse", "HEAD^"]).unwrap();
    git(
        root.path(),
        &["update-ref", "refs/remotes/origin/master", &accepted],
    )
    .unwrap();
    git(
        root.path(),
        &["checkout", "-b", "remote-based-work", "origin/master"],
    )
    .unwrap();
    git(root.path(), &["branch", "-f", "master", &stale]).unwrap();
    let base = crate::core::workspace::base_ref_for_branch(root.path(), "master").unwrap();
    assert_eq!(base, "origin/master");
    let next = initialize_workbranch(root.path(), &base, Some(&branch_context()))
        .unwrap()
        .unwrap();
    assert_ne!(next.change.as_ref().unwrap().id, "accepted-change");
    assert_eq!(next.change.as_ref().unwrap().base_commit, accepted);
    assert_eq!(next.change.unwrap().target_base, "origin/master");
    assert!(next.claims.is_empty());
}

#[test]
fn workbranch_captures_its_fork_commit_when_target_has_advanced() {
    let root = accepted_branch_repo();
    let fork = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    git(root.path(), &["branch", "fresh-before-upstream"]).unwrap();
    fs::write(root.path().join("upstream.rs"), "// upstream\n").unwrap();
    git(root.path(), &["add", "upstream.rs"]).unwrap();
    commit(root.path(), "advance target after branch creation");
    git(root.path(), &["checkout", "fresh-before-upstream"]).unwrap();
    let next = initialize_workbranch(root.path(), "master", Some(&branch_context()))
        .unwrap()
        .unwrap();
    assert_eq!(next.change.unwrap().base_commit, fork);
    assert_eq!(
        next.baseline.accepted_commit.as_deref(),
        Some(fork.as_str())
    );
}

#[test]
fn workbranch_old_merged_branch_is_not_mistaken_for_a_fresh_boundary() {
    let root = repo();
    git(root.path(), &["checkout", "-b", "old-merged-work"]).unwrap();
    begin_pr(root.path(), "old-merged-change", "master").unwrap();
    write_section(root.path(), "plan", &plan("Keep the old branch journal")).unwrap();
    git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
    commit(root.path(), "old branch implementation");
    git(root.path(), &["checkout", "master"]).unwrap();
    git(root.path(), &["merge", "--ff-only", "old-merged-work"]).unwrap();
    git(root.path(), &["checkout", "old-merged-work"]).unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    let error = initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
    assert!(error.to_string().contains("RESET_REQUIRED"), "{error}");
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
}

#[test]
fn workbranch_requires_current_task_context_before_discarding_accepted_state() {
    let root = accepted_branch_repo();
    git(root.path(), &["checkout", "-b", "missing-task"]).unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    let mut missing_title = branch_context();
    missing_title.title = " ".into();
    let mut missing_intent = branch_context();
    missing_intent.intent.clear();
    let mut missing_todo = branch_context();
    missing_todo.todo_ids.clear();
    for context in [
        None,
        Some(&missing_title),
        Some(&missing_intent),
        Some(&missing_todo),
    ] {
        let error = initialize_workbranch(root.path(), "master", context).unwrap_err();
        assert!(
            error.to_string().contains("WORK_CONTEXT_REQUIRED"),
            "{error}"
        );
        assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    }
}

#[test]
fn workbranch_two_accepted_cycles_get_distinct_unproven_current_journals() {
    let root = accepted_branch_repo();
    let mut previous_id = "accepted-change".to_string();
    for cycle in 1..=2 {
        let branch = format!("cycle-{cycle}");
        git(root.path(), &["checkout", "-b", &branch]).unwrap();
        let mut context = branch_context();
        context.title = format!("Task for cycle {cycle}");
        context.todo_ids = vec![format!("code_cycle_{cycle}")];
        let next = initialize_workbranch(root.path(), "master", Some(&context))
            .unwrap()
            .unwrap();
        let id = next.change.unwrap().id;
        assert_ne!(id, previous_id);
        assert!(next.claims.is_empty());
        assert!(next.checkpoints.is_empty());
        assert!(next.baseline.unresolved.contains_key("claim:accepted-open"));
        assert_eq!(
            read_section(root.path(), "plan").unwrap().unwrap()["todo_ids"],
            json!(context.todo_ids)
        );
        assert_eq!(
            read_section(root.path(), "trajectory").unwrap().unwrap()["proof_status"],
            "no_checks_run"
        );
        record_claim(
            root.path(),
            "cycle-proof",
            "This cycle works",
            "Counterexample",
            "supported",
            vec![format!("proof:cycle-{cycle}")],
        )
        .unwrap();
        checkpoint(
            root.path(),
            &format!("cycle-{cycle}"),
            "Current cycle evidence",
            vec![],
        )
        .unwrap();
        git(root.path(), &["add", GOVERNANCE_PATH]).unwrap();
        commit(root.path(), &format!("complete cycle {cycle}"));
        git(root.path(), &["checkout", "master"]).unwrap();
        git(root.path(), &["merge", "--ff-only", &branch]).unwrap();
        previous_id = id;
    }
}

#[test]
fn workbranch_explicit_creation_proof_works_without_branch_reflog() {
    let root = accepted_branch_repo();
    let created_base = git(root.path(), &["rev-parse", "HEAD"]).unwrap();
    git(root.path(), &["config", "core.logAllRefUpdates", "false"]).unwrap();
    git(root.path(), &["checkout", "-b", "without-reflog"]).unwrap();
    assert!(
        git(
            root.path(),
            &["reflog", "show", "refs/heads/without-reflog"]
        )
        .unwrap()
        .is_empty()
    );
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    assert!(initialize_workbranch(root.path(), "master", Some(&branch_context())).is_err());
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    let next = initialize_workbranch_at(
        root.path(),
        "master",
        Some(&branch_context()),
        Some(&created_base),
    )
    .unwrap()
    .unwrap();
    assert_ne!(next.change.as_ref().unwrap().id, "accepted-change");
    assert_eq!(next.change.unwrap().base_commit, created_base);
}

#[test]
fn workbranch_active_target_change_is_rejected_without_mutation() {
    let root = accepted_branch_repo();
    git(root.path(), &["branch", "release"]).unwrap();
    git(root.path(), &["checkout", "-b", "current-target-work"]).unwrap();
    let initialized = initialize_workbranch(root.path(), "master", Some(&branch_context()))
        .unwrap()
        .unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    let error = initialize_workbranch(root.path(), "release", Some(&branch_context())).unwrap_err();
    assert!(error.to_string().contains("TARGET_MISMATCH"), "{error}");
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    let repeated = begin_pr(
        root.path(),
        &initialized.change.unwrap().id,
        "refs/heads/master",
    )
    .unwrap();
    assert_eq!(repeated.change.unwrap().target_base, "master");
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
}

#[test]
fn workbranch_active_identity_rejects_a_target_behind_its_recorded_base() {
    let root = accepted_branch_repo();
    let stale_base = git(root.path(), &["rev-parse", "HEAD^"]).unwrap();
    git(root.path(), &["checkout", "-b", "current-with-stale-base"]).unwrap();
    initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap();
    record_claim(
        root.path(),
        "current",
        "Keep current work",
        "Counterexample",
        "open",
        vec![],
    )
    .unwrap();
    let before = fs::read(root.path().join(GOVERNANCE_PATH)).unwrap();
    git(root.path(), &["branch", "-f", "master", &stale_base]).unwrap();
    let error = initialize_workbranch(root.path(), "master", Some(&branch_context())).unwrap_err();
    assert!(
        error.to_string().contains("GOVERNANCE_PR_BASE_MISMATCH"),
        "{error}"
    );
    assert_eq!(fs::read(root.path().join(GOVERNANCE_PATH)).unwrap(), before);
    assert_eq!(
        git(root.path(), &["rev-parse", "master"]).unwrap(),
        stale_base
    );
}

#[test]
fn workbranch_bootstrap_without_current_task_does_not_create_a_journal() {
    let root = repo();
    git(root.path(), &["checkout", "-b", "bootstrap-without-task"]).unwrap();
    let unchanged = initialize_workbranch(root.path(), "master", None)
        .unwrap()
        .unwrap();
    assert!(unchanged.change.is_none());
    assert!(!root.path().join(GOVERNANCE_PATH).exists());
    let initialized = initialize_workbranch(root.path(), "master", Some(&branch_context()))
        .unwrap()
        .unwrap();
    assert!(initialized.change.is_some());
    assert!(initialized.claims.is_empty());
    assert_eq!(
        read_section(root.path(), "trajectory").unwrap().unwrap()["proof_status"],
        "no_checks_run"
    );
}

#[test]
fn workbranch_mount_guard_rejects_only_exact_governance_overmounts() {
    let root = Path::new("/workspace/project");
    for (point, blocked) in [
        ("/workspace/project/.decapod", true),
        ("/workspace/project/.decapod/governance.json", true),
        ("/workspace/project/.decapod/data", false),
        ("/workspace/project/.decapod/data/workunits", false),
        ("/workspace/project", false),
        ("/workspace", false),
        ("/workspace/project/.decapod-other", false),
        ("/workspace/other/.decapod", false),
    ] {
        let mounts = format!("35 25 0:32 / {point} rw,relatime - overlay overlay rw\n");
        assert_eq!(
            governance_overmounted(root, &mounts),
            blocked,
            "mountpoint {point}"
        );
    }
    assert!(!governance_overmounted(root, "\nmalformed\n"));
    assert!(governance_overmounted(
        root,
        concat!(
            "35 25 0:32 / /workspace/project rw - overlay overlay rw\n",
            "36 35 0:33 / /workspace/project/.decapod/data rw - tmpfs tmpfs rw\n",
            "37 35 0:34 / /workspace/project/.decapod/governance.json rw - ext4 /dev/root rw\n"
        )
    ));
}

#[test]
fn workbranch_mount_guard_decodes_escaped_spaces_and_tabs() {
    let root = Path::new("/workspace/project with space\tand-tab");
    let encoded = "/workspace/project\\040with\\040space\\011and-tab";
    for (suffix, blocked) in [
        ("/.decapod", true),
        ("/.decapod/governance.json", true),
        ("/.decapod/data", false),
        ("", false),
    ] {
        let mounts = format!("35 25 0:32 / {encoded}{suffix} rw - overlay overlay rw\n");
        assert_eq!(
            governance_overmounted(root, &mounts),
            blocked,
            "mount suffix {suffix}"
        );
    }
}

#[cfg(unix)]
#[test]
fn workbranch_host_governance_aliases_fail_closed_before_initialization_or_save() {
    use std::os::unix::fs::symlink;

    for alias in [
        "decapod-directory-symlink",
        "governance-file-symlink",
        "governance-hardlink",
    ] {
        let host = accepted_branch_repo();
        let host_path = host.path().join(GOVERNANCE_PATH);
        let host_before = fs::read(&host_path).unwrap();
        let workspace = host.path().join(".decapod/workspaces/aliased-governance");
        fs::create_dir_all(workspace.parent().unwrap()).unwrap();
        git(
            host.path(),
            &[
                "worktree",
                "add",
                "-b",
                "aliased-work",
                workspace.to_str().unwrap(),
                "master",
            ],
        )
        .unwrap();
        let local_path = workspace.join(GOVERNANCE_PATH);
        match alias {
            "decapod-directory-symlink" => {
                fs::remove_dir_all(workspace.join(".decapod")).unwrap();
                symlink(host.path().join(".decapod"), workspace.join(".decapod")).unwrap();
            }
            "governance-file-symlink" => {
                fs::remove_file(&local_path).unwrap();
                symlink(&host_path, &local_path).unwrap();
            }
            "governance-hardlink" => {
                fs::remove_file(&local_path).unwrap();
                fs::hard_link(&host_path, &local_path).unwrap();
            }
            _ => unreachable!(),
        }
        let error =
            initialize_workbranch(&workspace, "master", Some(&branch_context())).unwrap_err();
        assert!(
            error.to_string().contains("GOVERNANCE_WORKSPACE_ALIAS"),
            "{alias}: {error}"
        );
        assert_eq!(
            fs::read(&host_path).unwrap(),
            host_before,
            "{alias} changed accepted host evidence"
        );
        assert_eq!(fs::read(&local_path).unwrap(), host_before);
        let error =
            write_section(&workspace, "plan", &plan("Must not overwrite host work")).unwrap_err();
        assert!(
            error.to_string().contains("GOVERNANCE_WORKSPACE_ALIAS"),
            "save {alias}: {error}"
        );
        assert_eq!(fs::read(&host_path).unwrap(), host_before);
        assert_eq!(fs::read(&local_path).unwrap(), host_before);
        assert_eq!(
            git(host.path(), &["branch", "--show-current"]).unwrap(),
            "master"
        );
    }
}

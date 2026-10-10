use super::*;
use tempfile::TempDir;

fn repo() -> TempDir {
    let root = TempDir::new().unwrap();
    git(root.path(), &["init", "-b", "master"]).unwrap();
    fs::write(root.path().join("source.rs"), "fn main() {}\n").unwrap();
    fs::write(root.path().join(".gitignore"), ".decapod/data/\n").unwrap();
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

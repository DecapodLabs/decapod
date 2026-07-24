// Moved from src/decapod/core/completion_evidence.rs
use super::*;
use crate::core::capsule_policy::CapsulePolicyBinding;
use crate::core::workunit::{WorkUnitManifest, WorkUnitProofResult};
use std::process::Command;
use tempfile::tempdir;

fn fixture() -> CompletionEvidenceRecord {
    CompletionEvidenceRecord {
        schema_version: COMPLETION_EVIDENCE_SCHEMA_VERSION.to_string(),
        task_id: "task-1".to_string(),
        workunit_hash: "sha256:workunit".to_string(),
        intent_ref: "INTENT.md".to_string(),
        spec_refs: vec!["b".to_string(), "a".to_string(), "a".to_string()],
        state_refs: vec!["state.json".to_string()],
        proof_plan: vec!["b".to_string(), "a".to_string()],
        proof_results: vec![CompletionProofEvidence {
            gate: "a".to_string(),
            status: "pass".to_string(),
            artifact_ref: None,
            artifact: None,
        }],
        capsule: CapsuleEvidence {
            path: ".decapod/generated/context/task-1.json".to_string(),
            capsule_hash: "sha256:capsule".to_string(),
            policy_hash: "sha256:policy".to_string(),
            policy_version: "1".to_string(),
            repo_revision: "revision".to_string(),
        },
        repository: RepositoryEvidence {
            head_revision: "head".to_string(),
            base_revision: Some("base".to_string()),
            changed_paths: vec!["b".to_string(), "a".to_string()],
            working_tree: Vec::new(),
        },
        validation_epoch: ValidationEpochEvidence {
            epoch_id: "epoch".to_string(),
            epoch_hash: "sha256:epoch".to_string(),
        },
        unresolved_claims: vec![UnresolvedClaim {
            kind: "unknown".to_string(),
            text: "later".to_string(),
        }],
        plan_hash: None,
        evidence_hash: String::new(),
    }
}

#[test]
fn canonical_hash_is_stable_and_excludes_stored_hash() {
    let record = fixture();
    let hashed = record.with_recomputed_hash().expect("hash record");
    assert_eq!(
        record.computed_hash_hex().unwrap(),
        hashed.computed_hash_hex().unwrap()
    );
    assert_eq!(hashed.evidence_hash, hashed.computed_hash_hex().unwrap());
}

#[test]
fn canonical_hash_ignores_order_of_repeated_fields() {
    let record = fixture();
    let mut reordered = record.clone();
    reordered.spec_refs.reverse();
    reordered.proof_plan.reverse();
    reordered.repository.changed_paths.reverse();
    assert_eq!(
        record.computed_hash_hex().unwrap(),
        reordered.computed_hash_hex().unwrap()
    );
}

#[test]
fn proof_artifact_changes_are_reported_as_altered() {
    let directory = tempdir().expect("temp directory");
    let artifact_path = directory.path().join("proof.txt");
    fs::write(&artifact_path, "original").expect("write artifact");
    let digest = digest_reference(directory.path(), "proof.txt").expect("digest artifact");
    let result = CompletionProofEvidence {
        gate: "proof".to_string(),
        status: "pass".to_string(),
        artifact_ref: Some("proof.txt".to_string()),
        artifact: Some(digest),
    };
    fs::write(&artifact_path, "altered").expect("alter artifact");
    let mut checks = Vec::new();
    assert_eq!(
        verify_proof_artifacts(directory.path(), &[result], &mut checks),
        "altered"
    );
    assert_eq!(checks[0].status, "fail");
}

#[test]
fn missing_proof_artifact_is_reported_as_incomplete() {
    let directory = tempdir().expect("temp directory");
    let result = CompletionProofEvidence {
        gate: "proof".to_string(),
        status: "pass".to_string(),
        artifact_ref: Some("missing.txt".to_string()),
        artifact: None,
    };
    let mut checks = Vec::new();
    assert_eq!(
        verify_proof_artifacts(directory.path(), &[result], &mut checks),
        "incomplete"
    );
    assert_eq!(checks[0].status, "fail");
}

#[test]
fn artifact_reference_cannot_escape_repository_root() {
    let directory = tempdir().expect("temp directory");
    assert!(safe_reference_path(directory.path(), "../outside.txt").is_err());
}

#[test]
fn portable_source_repository_id_does_not_carry_remote_credentials() {
    assert_eq!(
        sanitized_source_repository_id("https://token:secret@github.com/org/repo.git"),
        Some("https://github.com/org/repo.git".to_string())
    );
    assert_eq!(
        sanitized_source_repository_id("git@github.com:org/repo.git"),
        Some("ssh://github.com/org/repo.git".to_string())
    );
}

#[test]
fn portable_envelope_is_tamper_evident_and_fails_closed_without_local_binding() {
    let record = fixture().with_recomputed_hash().expect("record hash");
    let envelope = PortableCompletionEvidence {
        schema_version: COMPLETION_EVIDENCE_SCHEMA_VERSION.to_string(),
        source_repository: SourceRepositoryBinding {
            repository_id: Some("https://example.invalid/source".to_string()),
            head_revision: record.repository.head_revision.clone(),
            base_revision: record.repository.base_revision.clone(),
        },
        record,
        capsule_artifact: FileDigest {
            path: ".decapod/generated/context/task-1.json".to_string(),
            sha256: "sha256:capsule".to_string(),
            size: 1,
        },
        proof_artifacts: Vec::new(),
        artifact_contents: Vec::new(),
        envelope_hash: String::new(),
    }
    .with_recomputed_hash()
    .expect("envelope hash");
    validate_portable_evidence(&envelope).expect("valid envelope");

    let directory = tempdir().expect("temp directory");
    let report = verify_portable_evidence(
        directory.path(),
        "task-1",
        &envelope,
        &directory.path().join("import.json"),
    )
    .expect("portable report");
    assert_eq!(report.structural_status, "structurally_valid");
    assert_eq!(report.local_decision, "rejected");

    let mut tampered = envelope;
    tampered.source_repository.repository_id = Some("tampered".to_string());
    assert!(validate_portable_evidence(&tampered).is_err());
}

#[test]
fn local_record_replays_and_detects_artifact_change() {
    let directory = tempdir().expect("temp directory");
    let root = directory.path();
    git(root, &["init", "--initial-branch", "master"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    fs::write(root.join("README.md"), "fixture\n").expect("write fixture");
    git(root, &["add", "README.md"]);
    git(root, &["commit", "-m", "fixture"]);

    let capsule_path = root.join(".decapod/generated/context/task-1.json");
    fs::create_dir_all(capsule_path.parent().unwrap()).expect("capsule directory");
    let capsule = DeterministicContextCapsule {
        schema_version: "1.1.0".to_string(),
        topic: "fixture".to_string(),
        scope: "core".to_string(),
        task_id: Some("task-1".to_string()),
        workunit_id: None,
        sources: Vec::new(),
        snippets: Vec::new(),
        capabilities: Vec::new(),
        policy: CapsulePolicyBinding {
            risk_tier: "low".to_string(),
            policy_hash: "sha256:policy".to_string(),
            policy_version: "1".to_string(),
            policy_path: "policy.json".to_string(),
            repo_revision: "HEAD".to_string(),
        },
        capsule_hash: String::new(),
        repo_signal_fingerprint: String::new(),
        config_input_hash: String::new(),
        spec_input_hash: String::new(),
    }
    .with_recomputed_hash()
    .expect("capsule hash");
    fs::write(
        &capsule_path,
        serde_json::to_vec_pretty(&capsule).expect("capsule JSON"),
    )
    .expect("write capsule");

    let proof_path = root.join("proof.txt");
    fs::write(&proof_path, "proof\n").expect("write proof");
    let epoch = active_validation_epoch(root).expect("validation epoch");
    let manifest = WorkUnitManifest {
        task_id: "task-1".to_string(),
        intent_ref: "INTENT.md".to_string(),
        spec_refs: vec!["ARCHITECTURE.md".to_string()],
        state_refs: vec![".decapod/generated/context/task-1.json".to_string()],
        proof_plan: vec!["proof".to_string()],
        proof_results: vec![WorkUnitProofResult {
            gate: "proof".to_string(),
            status: "pass".to_string(),
            artifact_ref: Some("proof.txt".to_string()),
            evaluator_epoch: None,
            validation_epoch: Some(epoch.clone()),
        }],
        validation_epoch: Some(epoch),
        status: WorkUnitStatus::Verified,
    };
    workunit::write_workunit(root, &manifest).expect("write workunit");

    let record = build_record(root, "task-1").expect("build record");
    let record_path = write_record(root, &record).expect("write record");
    let report = verify_record(root, "task-1", &record_path).expect("verify record");
    assert_eq!(report.status, "current");

    let portable_path = root.parent().unwrap().join("completion-evidence.json");
    let envelope = export_record(root, "task-1", &record_path, &portable_path)
        .expect("export portable evidence");
    assert_eq!(envelope.record.evidence_hash, record.evidence_hash);
    assert_eq!(envelope.artifact_contents.len(), 2);
    let imported = read_portable_evidence(&portable_path).expect("read portable evidence");
    assert_eq!(imported.envelope_hash, envelope.envelope_hash);
    let (_, report) = import_record(root, "task-1", &portable_path).expect("import evidence");
    assert_eq!(report.local_decision, "rejected");
    assert!(report.decision_path.ends_with(".decision.json"));
    assert_eq!(report.custody_paths.len(), 2);
    assert!(root.join(&report.decision_path).is_file());

    fs::write(&proof_path, "altered\n").expect("alter proof");
    let report = verify_record(root, "task-1", &record_path).expect("verify altered record");
    assert_eq!(report.status, "altered");
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

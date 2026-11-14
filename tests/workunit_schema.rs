use decapod::core::workunit::{WorkUnitManifest, WorkUnitProofResult, WorkUnitStatus};

#[test]
fn workunit_canonical_serialization_is_deterministic() {
    let manifest = WorkUnitManifest {
        task_id: "test_01".to_string(),
        intent_ref: "intent://alpha".to_string(),
        spec_refs: vec![
            "spec://b".to_string(),
            "spec://a".to_string(),
            "spec://a".to_string(),
        ],
        state_refs: vec![
            "state://2".to_string(),
            "state://1".to_string(),
            "state://1".to_string(),
        ],
        proof_plan: vec![
            "validate_passes".to_string(),
            "state_commit".to_string(),
            "validate_passes".to_string(),
        ],
        proof_results: vec![
            WorkUnitProofResult {
                gate: "state_commit".to_string(),
                status: "pass".to_string(),
                artifact_ref: Some("sha256:bbb".to_string()),
                evaluator_epoch: None,
                validation_epoch: None,
            },
            WorkUnitProofResult {
                gate: "validate_passes".to_string(),
                status: "pass".to_string(),
                artifact_ref: Some("sha256:aaa".to_string()),
                evaluator_epoch: None,
                validation_epoch: None,
            },
        ],
        validation_epoch: None,
        validation_record: None,
        status: WorkUnitStatus::Claimed,
    };

    let bytes1 = manifest.canonical_json_bytes().expect("serialize #1");
    let bytes2 = manifest.canonical_json_bytes().expect("serialize #2");
    assert_eq!(bytes1, bytes2, "canonical bytes must be stable");

    let hash1 = manifest.canonical_hash_hex().expect("hash #1");
    let hash2 = manifest.canonical_hash_hex().expect("hash #2");
    assert_eq!(hash1, hash2, "canonical hash must be stable");
}

#[test]
fn workunit_canonicalization_sorts_and_dedups_contract_arrays() {
    let manifest = WorkUnitManifest {
        task_id: "test_02".to_string(),
        intent_ref: "intent://beta".to_string(),
        spec_refs: vec!["spec://b".to_string(), "spec://a".to_string()],
        state_refs: vec!["state://2".to_string(), "state://1".to_string()],
        proof_plan: vec!["z".to_string(), "a".to_string(), "z".to_string()],
        proof_results: vec![WorkUnitProofResult {
            gate: "b".to_string(),
            status: "pass".to_string(),
            artifact_ref: None,
            evaluator_epoch: None,
            validation_epoch: None,
        }],
        validation_epoch: None,
        validation_record: None,
        status: WorkUnitStatus::Draft,
    };

    let c = manifest.canonicalized();
    assert_eq!(c.spec_refs, vec!["spec://a", "spec://b"]);
    assert_eq!(c.state_refs, vec!["state://1", "state://2"]);
    assert_eq!(c.proof_plan, vec!["a", "z"]);
}

#[test]
fn legacy_manifests_preserve_canonical_bytes_without_producer_provenance() {
    let legacy = serde_json::json!({
        "task_id": "legacy", "intent_ref": "intent:old",
        "spec_refs": [], "state_refs": [], "proof_plan": ["validate_passes"],
        "proof_results": [], "status": "CLAIMED"
    });
    let manifest: WorkUnitManifest = serde_json::from_value(legacy.clone()).unwrap();
    assert!(manifest.validation_record.is_none());
    assert_eq!(serde_json::to_value(&manifest).unwrap(), legacy);
}

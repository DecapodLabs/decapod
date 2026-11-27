use super::*;
use std::cell::RefCell;

struct MockTransport {
    writes: RefCell<Vec<(String, JsonValue)>>,
    reads: RefCell<Vec<(String, JsonValue)>>,
    next_write: RefCell<Vec<Result<JsonValue, CloudControlError>>>,
    next_read: RefCell<Vec<Result<JsonValue, CloudControlError>>>,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            writes: RefCell::new(Vec::new()),
            reads: RefCell::new(Vec::new()),
            next_write: RefCell::new(Vec::new()),
            next_read: RefCell::new(Vec::new()),
        }
    }

    fn push_write(&self, r: Result<JsonValue, CloudControlError>) {
        self.next_write.borrow_mut().push(r);
    }

    fn push_read(&self, r: Result<JsonValue, CloudControlError>) {
        self.next_read.borrow_mut().push(r);
    }
}

impl ProofControlTransport for MockTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.writes
            .borrow_mut()
            .push((op_name.to_string(), payload.clone()));
        self.next_write.borrow_mut().remove(0)
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.reads
            .borrow_mut()
            .push((op_name.to_string(), payload.clone()));
        self.next_read.borrow_mut().remove(0)
    }
}

#[test]
fn live_custody_requires_the_exact_current_workspace_fence() {
    let transport = MockTransport::new();
    let req = CustodyGetRequest {
        project_id: "example/project".into(),
        task_id: "todo_01abc".into(),
        workspace_id: "ws_01abc".into(),
        custody_id: "custody_01abc".into(),
        task_revision: 4,
        fencing_generation: 6,
    };
    let receipt = json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "status": "custody_current",
        "project_id": req.project_id,
        "task_id": req.task_id,
        "workspace_id": req.workspace_id,
        "custody_id": req.custody_id,
        "task_revision": req.task_revision,
        "fencing_generation": req.fencing_generation,
        "authority_generation": 2,
        "expires_at": crate::core::time::format_rfc3339_nanos(
            time::OffsetDateTime::now_utc() + time::Duration::hours(1)
        ).expect("expiry"),
    });
    transport.push_read(Ok(receipt.clone()));
    assert_eq!(custody_get(&transport, &req).unwrap().fencing_generation, 6);
    let reads = transport.reads.borrow();
    assert_eq!(reads[0].0, PROOF_ACCEPTANCE_READ_OP);
    assert_eq!(reads[0].1["action"], "get_custody");
    drop(reads);

    for (field, bad_value) in [
        ("status", json!("custody_bound")),
        ("project_id", json!("example/other")),
        ("workspace_id", json!("ws_other")),
        ("custody_id", json!("custody_other")),
        ("task_revision", json!(5)),
        ("fencing_generation", json!(7)),
        ("authority_generation", json!(0)),
        ("expires_at", json!("2020-01-01T00:00:00Z")),
    ] {
        let mut mismatched = receipt.clone();
        mismatched[field] = bad_value;
        transport.push_read(Ok(mismatched));
        assert!(
            custody_get(&transport, &req)
                .unwrap_err()
                .to_string()
                .contains("CLOUD_CUSTODY_RECEIPT_INVALID"),
            "{field}"
        );
    }
    transport.push_read(Err(classify_control_status(
        None,
        "transport_timeout",
        "read timed out",
        true,
    )));
    assert!(
        custody_get(&transport, &req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_CONTROL_UNAVAILABLE")
    );
    assert!(transport.writes.borrow().is_empty());
}

fn sample_capture_request(op_id: &str) -> ProofCaptureRequest {
    ProofCaptureRequest {
        contract: PROOF_CAPTURE_CONTRACT_ID.to_string(),
        contract_digest: PROOF_CAPTURE_CONTRACT_DIGEST.to_string(),
        operation_id: op_id.to_string(),
        subject: ProofSubject {
            repository_id: "DecapodLabs/decapod".to_string(),
            task_id: "todo_01abc".to_string(),
            workspace_id: "ws_01abc".to_string(),
            custody_id: "custody_01abc".to_string(),
            task_revision: 2,
            fencing_generation: 1,
            candidate: ProofGitIdentity {
                commit: "a".repeat(40),
                tree: "b".repeat(40),
            },
            accepted_target: ProofGitIdentity {
                commit: "c".repeat(40),
                tree: "d".repeat(40),
            },
            integration: None,
            policy_digest: format!("sha256:{}", "1".repeat(64)),
            requirements_digest: format!("sha256:{}", "2".repeat(64)),
            material_digest: format!("sha256:{}", "3".repeat(64)),
            prerequisites_digest: format!("sha256:{}", "4".repeat(64)),
        },
        evidence: vec![
            ProofEvidenceReference {
                kind: "suite".to_string(),
                digest: format!("sha256:{}", "e".repeat(64)),
                bytes: 256,
                provenance_digest: format!("sha256:{}", "f".repeat(64)),
            },
            ProofEvidenceReference {
                kind: "material".to_string(),
                digest: format!("sha256:{}", "a".repeat(64)),
                bytes: 128,
                provenance_digest: format!("sha256:{}", "9".repeat(64)),
            },
        ],
    }
}

#[test]
fn proof_capture_fingerprint_ignores_evidence_slice_order_and_rejects_duplicates() {
    let op_id = new_uuid_v4().expect("uuid");
    let mut req_a = sample_capture_request(&op_id);
    let mut req_b = req_a.clone();
    req_b.evidence.reverse();

    let fp_a = proof_capture_fingerprint(&req_a).expect("fingerprint a");
    let fp_b = proof_capture_fingerprint(&req_b).expect("fingerprint b");
    assert_eq!(fp_a, fp_b);
    assert!(fp_a.starts_with("sha256:"));

    req_a.evidence.push(req_a.evidence[0].clone());
    assert!(proof_capture_fingerprint(&req_a).is_err());
}

#[test]
fn distinct_403_413_429_and_outcome_unknown_reconciliation() {
    let err_403 = classify_control_status(Some(403), "authorization_denied", "no seat", false)
        .to_decapod_error(Some("op-1"));
    assert!(err_403.to_string().contains("CLOUD_ENTITLEMENT_DENIED"));
    assert!(err_403.to_string().contains("HTTP 403"));

    let err_413 = classify_control_status(Some(413), "proof_acceptance_limit", "too big", false)
        .to_decapod_error(Some("op-2"));
    assert!(err_413.to_string().contains("CLOUD_PAYLOAD_BOUND_EXCEEDED"));
    assert!(err_413.to_string().contains("HTTP 413"));

    let err_429 = classify_control_status(Some(429), "quota_exceeded", "slow down", false)
        .to_decapod_error(Some("op-3"));
    assert!(err_429.to_string().contains("CLOUD_QUOTA_EXCEEDED"));
    assert!(err_429.to_string().contains("HTTP 429"));

    // OutcomeUnknown triggers read-only get_operation reconciliation, not replay.
    let op_id = new_uuid_v4().expect("uuid");
    let req = sample_capture_request(&op_id);
    let transport = MockTransport::new();
    transport.push_write(Err(classify_control_status(
        None,
        "outcome_unknown",
        "connection dropped after commit",
        true,
    )));
    transport.push_read(Ok(json!({
        "found": true,
        "receipt": {
            "state": "captured",
            "attestation_id": op_id,
        }
    })));
    let reconciled =
        submit_capture(&transport, "DecapodLabs/decapod", 1, &req).expect("reconciled");
    assert_eq!(reconciled["state"], "captured");
    assert_eq!(transport.writes.borrow().len(), 1);
    assert_eq!(transport.reads.borrow().len(), 1);
    assert_eq!(transport.reads.borrow()[0].0, PROOF_ACCEPTANCE_READ_OP);
    assert_eq!(transport.reads.borrow()[0].1["action"], "get_operation");
}

#[test]
fn proof_transitions_reject_mismatched_subject_stale_fingerprint_and_self_escalation() {
    let att_id = new_uuid_v4().expect("att uuid");
    let req_cap = sample_capture_request(&att_id);
    let fp = proof_capture_fingerprint(&req_cap).expect("fp");
    let ev_digest = format!("sha256:{}", "7".repeat(64));

    let verify_req = VerifyProofTransitionRequest {
        project_id: "DecapodLabs/decapod".into(),
        operation_id: new_uuid_v4().expect("verify op"),
        attestation_id: att_id.clone(),
        expected_fingerprint: fp.clone(),
        expected_candidate_commit: "a".repeat(40),
        expected_candidate_tree: "b".repeat(40),
        verified_evidence_digest: ev_digest.clone(),
        verifier_scope: "independent-ci-v1".into(),
    };
    let valid_verify = json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "independently_verified",
        "operation_id": verify_req.operation_id,
        "attestation_id": verify_req.attestation_id,
        "fingerprint": fp,
        "candidate": { "commit": "a".repeat(40), "tree": "b".repeat(40) },
        "verified_evidence_digest": ev_digest,
        "references_verified": true,
        "logical_custody_verified": false,
        "acceptance_supported": false,
        "publication_eligible": false,
        "forge_merge_authority": false,
    });
    let transport = MockTransport::new();
    transport.push_write(Ok(valid_verify.clone()));
    assert_eq!(
        submit_verify(&transport, &verify_req).unwrap()["state"],
        "independently_verified"
    );

    // Negative controls on verify: wrong commit, wrong fingerprint, wrong state, escalated flag.
    for mut bad in [
        {
            let mut r = valid_verify.clone();
            r["candidate"]["commit"] = json!("9".repeat(40));
            r
        },
        {
            let mut r = valid_verify.clone();
            r["fingerprint"] = json!(format!("sha256:{}", "0".repeat(64)));
            r
        },
        {
            let mut r = valid_verify.clone();
            r["state"] = json!("published");
            r
        },
        {
            let mut r = valid_verify.clone();
            r["publication_eligible"] = json!(true);
            r
        },
        {
            let mut r = valid_verify.clone();
            r["forge_merge_authority"] = json!(true);
            r
        },
    ] {
        let _ = &mut bad;
        transport.push_write(Ok(bad));
        assert!(
            submit_verify(&transport, &verify_req)
                .unwrap_err()
                .to_string()
                .contains("CLOUD_PROOF_RECEIPT_INVALID")
        );
    }

    // Reconcile unknown outcome returning another operation's receipt MUST fail closed.
    transport.push_write(Err(classify_control_status(
        None,
        "outcome_unknown",
        "socket closed",
        true,
    )));
    let mut wrong_op = valid_verify.clone();
    wrong_op["operation_id"] = json!(new_uuid_v4().expect("other op"));
    transport.push_read(Ok(json!({ "found": true, "receipt": wrong_op })));
    assert!(
        submit_verify(&transport, &verify_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    // Admit vs publish positive + negative controls.
    let admit_req = AdmitOrPublishProofRequest {
        project_id: "DecapodLabs/decapod".into(),
        operation_id: new_uuid_v4().expect("admit op"),
        attestation_id: att_id.clone(),
        expected_fingerprint: fp.clone(),
        expected_task_revision: 2,
        expected_fencing_generation: 1,
        expected_candidate_commit: "a".repeat(40),
        expected_accepted_target_commit: "c".repeat(40),
        expected_policy_digest: format!("sha256:{}", "1".repeat(64)),
    };
    let valid_admit = json!({
        "version": 1,
        "profile": "candidate-acceptance-v1",
        "state": "currently_admissible",
        "operation_id": admit_req.operation_id,
        "attestation_id": att_id,
        "fingerprint": fp,
        "task_revision": 2,
        "fencing_generation": 1,
        "candidate": { "commit": "a".repeat(40), "tree": "b".repeat(40) },
        "accepted_target": { "commit": "c".repeat(40), "tree": "d".repeat(40) },
        "policy_digest": format!("sha256:{}", "1".repeat(64)),
        "references_verified": true,
        "logical_custody_verified": true,
        "acceptance_supported": true,
        "publication_eligible": false,
        "forge_merge_authority": false,
    });
    transport.push_write(Ok(valid_admit.clone()));
    assert_eq!(
        submit_admit_or_publish(&transport, "admit", &admit_req).unwrap()["state"],
        "currently_admissible"
    );

    let mut wrong_target = valid_admit.clone();
    wrong_target["accepted_target"]["commit"] = json!("e".repeat(40));
    transport.push_write(Ok(wrong_target));
    assert!(
        submit_admit_or_publish(&transport, "admit", &admit_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );

    let mut valid_publish = valid_admit;
    valid_publish["state"] = json!("published");
    valid_publish["publication_eligible"] = json!(true);
    transport.push_write(Ok(valid_publish.clone()));
    assert_eq!(
        submit_admit_or_publish(&transport, "publish", &admit_req).unwrap()["publication_eligible"],
        true
    );

    let mut falsified_forge = valid_publish;
    falsified_forge["forge_merge_authority"] = json!(true);
    transport.push_write(Ok(falsified_forge));
    assert!(
        submit_admit_or_publish(&transport, "publish", &admit_req)
            .unwrap_err()
            .to_string()
            .contains("CLOUD_PROOF_RECEIPT_INVALID")
    );
}

// Moved from src/decapod/lib.rs
use super::{
    IdentityEvidenceClass, IdentityVerificationPolicy, IdentityVerificationResult,
    build_identity_assertions, identity_attestation_digest, verify_identity_assertion,
};

#[test]
fn records_agent_and_provider_as_separate_unverified_claims() {
    let assertions =
        build_identity_assertions("agent/codex", Some("example-provider"), "task-123", 42);

    assert_eq!(assertions.len(), 2);
    assert_eq!(assertions[0].claim_kind, "agent_id");
    assert_eq!(assertions[0].subject_type, "agent");
    assert_eq!(assertions[0].asserted_value, "agent/codex");
    assert_eq!(assertions[1].claim_kind, "agent_provider");
    assert_eq!(assertions[1].subject_type, "model_provider");
    assert_eq!(assertions[1].asserted_value, "example-provider");

    for assertion in assertions {
        assert_eq!(
            assertion.evidence_class,
            IdentityEvidenceClass::SelfDeclared
        );
        assert_eq!(
            assertion.verification_result,
            IdentityVerificationResult::Unverified
        );
        assert_eq!(assertion.scope, "task-123");
        assert_eq!(assertion.issued_at_epoch_secs, 42);
        assert!(assertion.authority.is_none());
        assert!(assertion.verifier.is_none());
    }
}

#[test]
fn omits_an_empty_provider_claim() {
    let assertions = build_identity_assertions("agent/codex", Some("  "), "task-123", 42);

    assert_eq!(assertions.len(), 1);
    assert_eq!(assertions[0].claim_kind, "agent_id");
}

#[test]
fn recomputes_results_instead_of_trusting_serialized_verification() {
    let mut assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    assertion.verification_result = IdentityVerificationResult::Verified;

    let policy = IdentityVerificationPolicy::local_handshake("task-123", 42);
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Unverified
    );
}

#[test]
fn stronger_policy_does_not_promote_self_declared_identity() {
    let assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    let policy = IdentityVerificationPolicy {
        expected_scope: "task-123".to_string(),
        now_epoch_secs: 42,
        accepted_evidence_classes: vec![IdentityEvidenceClass::RemotelyAuthenticated],
        trusted_authorities: Vec::new(),
        trusted_verifiers: Vec::new(),
        require_authority: true,
        require_verifier: true,
        expected_binding_hash: None,
        replayed_nonces: Vec::new(),
        trusted_attestation_keys: Vec::new(),
    };

    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Unverified
    );
}

#[test]
fn lifecycle_and_scope_fail_closed() {
    let mut assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    let policy = IdentityVerificationPolicy::local_handshake("task-123", 42);

    assertion.expires_at_epoch_secs = Some(42);
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Expired
    );

    assertion.expires_at_epoch_secs = None;
    assertion.revocation = Some("revoked-by-policy".to_string());
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Revoked
    );

    assertion.revocation = None;
    assertion.scope = "other-task".to_string();
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Rejected
    );
}

#[test]
fn policy_is_claim_specific() {
    let mut assertions =
        build_identity_assertions("agent/codex", Some("example-provider"), "task-123", 42);
    assertions[0].evidence_class = IdentityEvidenceClass::LocallyObserved;
    assertions[0].verification_method = "local process observation".to_string();
    let policy = IdentityVerificationPolicy {
        expected_scope: "task-123".to_string(),
        now_epoch_secs: 42,
        accepted_evidence_classes: vec![IdentityEvidenceClass::LocallyObserved],
        trusted_authorities: Vec::new(),
        trusted_verifiers: Vec::new(),
        require_authority: false,
        require_verifier: false,
        expected_binding_hash: None,
        replayed_nonces: Vec::new(),
        trusted_attestation_keys: Vec::new(),
    };

    assert_eq!(
        verify_identity_assertion(&assertions[0], &policy),
        IdentityVerificationResult::Verified
    );
    assert_eq!(
        verify_identity_assertion(&assertions[1], &policy),
        IdentityVerificationResult::Unverified
    );
}

#[test]
fn configured_policy_requires_matching_trust_authority_and_verifier() {
    let mut assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    assertion.evidence_class = IdentityEvidenceClass::RemotelyAuthenticated;
    assertion.authority = Some("provider-authority".to_string());
    assertion.verifier = Some("local-verifier-v1".to_string());
    assertion.nonce = Some("nonce-1".to_string());
    assertion.binding_hash = Some("sha256:binding".to_string());
    assertion.verification_method = "signed assertion".to_string();

    let mut policy = IdentityVerificationPolicy::configured(
        "task-123",
        42,
        vec![IdentityEvidenceClass::RemotelyAuthenticated],
        vec!["provider-authority".to_string()],
        vec!["local-verifier-v1".to_string()],
    );
    policy = policy.with_attestation_key(
        "provider-authority",
        "local-verifier-v1",
        "test-key",
        Some("sha256:binding"),
        Vec::new(),
    );
    assertion.signature = Some(identity_attestation_digest(&assertion, "test-key"));
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Verified
    );

    assertion.authority = Some("attacker-controlled-authority".to_string());
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Rejected
    );

    assertion.authority = Some("provider-authority".to_string());
    assertion.verifier = Some("unknown-verifier".to_string());
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Rejected
    );
}

#[test]
fn configured_policy_does_not_promote_missing_trust_configuration() {
    let mut assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    assertion.evidence_class = IdentityEvidenceClass::HarnessAttested;
    assertion.authority = Some("provider-authority".to_string());
    assertion.verifier = Some("local-verifier-v1".to_string());
    assertion.verification_method = "attested harness record".to_string();

    let policy = IdentityVerificationPolicy::configured(
        "task-123",
        42,
        vec![IdentityEvidenceClass::HarnessAttested],
        Vec::new(),
        Vec::new(),
    );
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Rejected
    );
}

#[test]
fn attestation_replay_and_forgery_fail_closed() {
    let mut assertion = build_identity_assertions("agent/codex", None, "task-123", 42).remove(0);
    assertion.evidence_class = IdentityEvidenceClass::HarnessAttested;
    assertion.authority = Some("harness-authority".to_string());
    assertion.verifier = Some("repo-verifier".to_string());
    assertion.nonce = Some("nonce-2".to_string());
    assertion.binding_hash = Some("sha256:task-binding".to_string());
    assertion.verification_method = "keyed attestation".to_string();
    assertion.signature = Some(identity_attestation_digest(&assertion, "harness-key"));
    let policy = IdentityVerificationPolicy::configured(
        "task-123",
        42,
        vec![IdentityEvidenceClass::HarnessAttested],
        vec!["harness-authority".to_string()],
        vec!["repo-verifier".to_string()],
    )
    .with_attestation_key(
        "harness-authority",
        "repo-verifier",
        "harness-key",
        Some("sha256:task-binding"),
        vec!["already-used".to_string()],
    );
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Verified
    );
    let mut replay_policy = policy.clone();
    replay_policy.replayed_nonces.push("nonce-2".to_string());
    assert_eq!(
        verify_identity_assertion(&assertion, &replay_policy),
        IdentityVerificationResult::Rejected
    );
    assertion.asserted_value = "forged-agent".to_string();
    assert_eq!(
        verify_identity_assertion(&assertion, &policy),
        IdentityVerificationResult::Rejected
    );
}

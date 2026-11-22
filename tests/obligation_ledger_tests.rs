//! Comprehensive unit & integration tests for Shard 5 governance & ledger deliverables:
//! #1083, #1084, #1093, #1095, #1097, #1365, #1468, #1569, #1691, #1726,
//! #1729, #1761, #1762, #1768, #1772, #1774, #1796, #1907.

use decapod::core::artifact_clarity::{
    ClarityProfileConfig, ClarityProfileKind, ClaritySeverity, canonical_decapod_terminology,
    evaluate_artifacts, validate_governance_json_clarity, validate_text_clarity,
};
use decapod::core::assets::canonical_template;
use decapod::core::formal_obligations::{
    CapacityAccountingState, CapacityTransition, FORMAL_OBLIGATION_SCHEMA, FormalCheckerKind,
    FormalCheckerTranscript, FormalClaimScope, FormalLinkingArgument, FormalObligationConfig,
    FormalObligationRecord, FormalVerificationStatus, PinnedFormalChecker,
    compute_runner_attestation, sha256_hex, verify_formal_obligation,
};
use decapod::core::governance_document::historical_exception::{
    HISTORICAL_EXCEPTION_SCHEMA, HISTORICAL_EXCEPTION_SIGNERS_SCHEMA, HistoricalExceptionContext,
    HistoricalExceptionDecision, HistoricalExceptionSigners, canonical_signing_payload,
    verify_historical_exceptions,
};
use decapod::core::governance_document::semantic_integrity::{
    detect_governance_theater_diff, validate_semantic_governance_json,
};
use decapod::core::obligation_ledger::{
    ActiveCustodySnapshot, AgentOutcomeClaim, ApprovalLedger, AsyncEffectState,
    CanonicalApprovalRecord, CanonicalInvocation, CompletionReportInput, DelayedAsyncEffect,
    DerivedObligationStatus, DimensionStatus, ExecutionTelemetryRecord, GraderOutcome,
    GraderSelectionInput, InvalidationReason, MainMergeAuthorizationInput, MappedRootObligation,
    ObligationCategory, ObligationInput, ObligationSurface, ProviderWaitState,
    RelevanceGateBenchmarkCase, RootRequirement, RootSupersessionRecord, SourceAnchorStatus,
    StagnationDisposition, StagnationObservation, StagnationTracker, TaskFirstReviewBenchmark,
    build_completion_report_4d, derive_obligation_ledger, evaluate_constitution_promotion,
    evaluate_main_merge_authorization, evaluate_root_conservation, evaluate_trusted_grader,
    verify_outcome_claim_anchor,
};
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[allow(clippy::too_many_arguments)]
fn make_signed_exception(
    keypair: &Ed25519KeyPair,
    key_id: &str,
    decision_id: &str,
    repo_id: &str,
    base: &str,
    cand: &str,
    affected_commits: Vec<String>,
    affected_trees: Vec<String>,
    obligations: Vec<String>,
    issued_at: i64,
    expires_at: i64,
) -> HistoricalExceptionDecision {
    let mut decision = HistoricalExceptionDecision {
        schema: HISTORICAL_EXCEPTION_SCHEMA.to_string(),
        decision_id: decision_id.to_string(),
        repository_id: repo_id.to_string(),
        base_commit: base.to_string(),
        candidate_commit: cand.to_string(),
        affected_commits,
        affected_trees,
        obligations,
        authorizer_key_id: key_id.to_string(),
        authorizer_authority: "maintainer-offline-key".to_string(),
        reason: "Reviewed immutable historical gap preserved with fresh proof.".to_string(),
        evidence_refs: vec!["audit/historical-gap-1569.json".to_string()],
        issued_at,
        expires_at,
        signature: String::new(),
    };
    let payload = canonical_signing_payload(&decision).unwrap();
    decision.signature = to_hex(keypair.sign(&payload).as_ref());
    decision
}

// ============================================================================
// #1691: Escalation admissibility in canonical entrypoints & constitution
// ============================================================================

#[test]
fn issue_1691_canonical_entrypoints_and_constitution_enforce_escalation_admissibility() {
    for surface in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let body = canonical_template(surface).expect("canonical entrypoint template");
        assert!(
            !body.contains("MUST refine intent with the user before inference-heavy work"),
            "{surface} still contains pre-escalation-admissibility Golden Rule 1"
        );
        assert!(
            !body.contains("MUST stop and ask the human when Decapod emits a **Decision Gate**"),
            "{surface} still contains unconditional Decision Gate ask wording"
        );
        assert!(
            !body.contains("Stop if requirements are ambiguous or conflicting."),
            "{surface} still contains unconditional stop-on-ambiguity wording"
        );
        assert!(
            body.contains("escalation admission") || body.contains("bounded reflection"),
            "{surface} must route ordinary uncertainty through bounded reflection/escalation admission"
        );
    }

    // Verify embedded constitution compiled into candidate binary has zero stale routing-pass prompts.
    for node_id in ["core/DECAPOD", "core/SPECS", "interfaces/PROCEDURAL_NORMS"] {
        let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
            .args(["constitution", "get", node_id])
            .output()
            .expect("query candidate decapod binary");
        assert!(
            output.status.success(),
            "constitution get {node_id} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            !text.contains(
                "After two unresolved routing passes, stop and ask a focused human question"
            ),
            "embedded constitution node {node_id} still has pre-escalation-admissibility wording"
        );
    }
}

// ============================================================================
// #1468: Opt-in main_merge = true (default fail-closed & target-move check)
// ============================================================================

#[test]
fn issue_1468_main_merge_config_default_fail_closed_and_authorization() {
    // Verify embedded config-schema docs document governance.main_merge opt-in.
    let docs_out = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["docs", "show", "config-schema", "--source", "embedded"])
        .output()
        .expect("query embedded config-schema docs");
    assert!(docs_out.status.success());
    let docs_txt = String::from_utf8_lossy(&docs_out.stdout);
    assert!(
        docs_txt.contains("main_merge"),
        "embedded config-schema docs must document [governance].main_merge"
    );

    // Default false fails closed.
    let denied_default = evaluate_main_merge_authorization(&MainMergeAuthorizationInput {
        accepted_base_main_merge: false,
        candidate_main_merge: false,
        validated_candidate_commit: "cand-1".into(),
        current_candidate_commit: "cand-1".into(),
        validated_target_commit: "base-1".into(),
        current_target_commit: "base-1".into(),
        full_validation_passed: true,
    });
    assert!(!denied_default.permitted);
    assert_eq!(
        denied_default.rejection_code.as_deref(),
        Some("MAIN_MERGE_DISABLED_BY_DEFAULT")
    );

    // Candidate self-authorization rejected.
    let self_auth = evaluate_main_merge_authorization(&MainMergeAuthorizationInput {
        accepted_base_main_merge: false,
        candidate_main_merge: true,
        validated_candidate_commit: "cand-1".into(),
        current_candidate_commit: "cand-1".into(),
        validated_target_commit: "base-1".into(),
        current_target_commit: "base-1".into(),
        full_validation_passed: true,
    });
    assert!(!self_auth.permitted);
    assert_eq!(
        self_auth.rejection_code.as_deref(),
        Some("POLICY_SELF_AUTHORIZATION_REJECTED")
    );

    // Accepted policy opted in + target moved -> fails closed!
    let target_moved = evaluate_main_merge_authorization(&MainMergeAuthorizationInput {
        accepted_base_main_merge: true,
        candidate_main_merge: true,
        validated_candidate_commit: "cand-1".into(),
        current_candidate_commit: "cand-1".into(),
        validated_target_commit: "base-1".into(),
        current_target_commit: "base-2".into(),
        full_validation_passed: true,
    });
    assert!(!target_moved.permitted);
    assert!(target_moved.target_moved);
    assert_eq!(
        target_moved.rejection_code.as_deref(),
        Some("TARGET_MOVED_FAIL_CLOSED")
    );

    // Accepted policy opted in + fresh exact candidate/target proof -> permitted!
    let allowed = evaluate_main_merge_authorization(&MainMergeAuthorizationInput {
        accepted_base_main_merge: true,
        candidate_main_merge: true,
        validated_candidate_commit: "cand-1".into(),
        current_candidate_commit: "cand-1".into(),
        validated_target_commit: "base-1".into(),
        current_target_commit: "base-1".into(),
        full_validation_passed: true,
    });
    assert!(allowed.permitted);
    assert!(allowed.accepted_policy_opted_in);
    assert!(allowed.proof_fresh);
}

// ============================================================================
// #1569: Offline verifiable historical exception authority & history_proven=false
// ============================================================================

#[test]
fn issue_1569_offline_historical_exception_authority_and_negative_controls() {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
    let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
    let pub_hex = to_hex(keypair.public_key().as_ref());

    let mut keys = BTreeMap::new();
    keys.insert("maint-key-1".to_string(), pub_hex);
    let signers = HistoricalExceptionSigners {
        schema: HISTORICAL_EXCEPTION_SIGNERS_SCHEMA.to_string(),
        policy_permits_exceptional_admission: true,
        keys,
        revoked_key_ids: BTreeSet::new(),
        revoked_decision_ids: BTreeSet::new(),
    };

    let ctx = HistoricalExceptionContext {
        repository_id: "DecapodLabs/decapod".to_string(),
        base_commit: "base-aaa".to_string(),
        candidate_commit: "cand-bbb".to_string(),
        affected_commits: vec!["hist-ccc".to_string()],
        affected_trees: vec!["tree-ddd".to_string()],
        required_obligations: vec!["checkpoint_history".to_string()],
        now_epoch: 1_700_000_100,
        consumed_decision_ids: BTreeSet::new(),
        signers_from_candidate_worktree: false,
    };

    let valid_decision = make_signed_exception(
        &keypair,
        "maint-key-1",
        "dec-1569-001",
        "DecapodLabs/decapod",
        "base-aaa",
        "cand-bbb",
        vec!["hist-ccc".to_string()],
        vec!["tree-ddd".to_string()],
        vec!["checkpoint_history".to_string()],
        1_700_000_000,
        1_700_050_000,
    );

    // 1. Positive control: valid offline signed exception ALWAYS preserves history_proven = false!
    let ok =
        verify_historical_exceptions(Some(&signers), std::slice::from_ref(&valid_decision), &ctx);
    assert!(
        !ok.history_proven,
        "exceptions NEVER make unproven history proven"
    );
    assert!(ok.exception_authorized);
    assert!(ok.policy_permits_exceptional_admission);
    assert_eq!(ok.identity_assurance, "offline_ed25519_accepted_trust_root");
    assert_eq!(ok.authorized_exceptions.len(), 1);

    // 2. Negative: candidate-worktree signers rejected.
    let mut cand_ctx = ctx.clone();
    cand_ctx.signers_from_candidate_worktree = true;
    let cand_rej = verify_historical_exceptions(
        Some(&signers),
        std::slice::from_ref(&valid_decision),
        &cand_ctx,
    );
    assert!(!cand_rej.exception_authorized);
    assert!(!cand_rej.history_proven);

    // 3. Negative: replay of consumed decision_id rejected.
    let mut replay_ctx = ctx.clone();
    replay_ctx
        .consumed_decision_ids
        .insert("dec-1569-001".to_string());
    let replay_rej = verify_historical_exceptions(
        Some(&signers),
        std::slice::from_ref(&valid_decision),
        &replay_ctx,
    );
    assert!(!replay_rej.exception_authorized);
    assert!(
        replay_rej
            .rejection_reasons
            .iter()
            .any(|r| r.contains("REPLAY_REJECTED"))
    );

    // 4. Negative: revoked key rejected.
    let mut revoked_signers = signers.clone();
    revoked_signers
        .revoked_key_ids
        .insert("maint-key-1".to_string());
    let rev_rej = verify_historical_exceptions(
        Some(&revoked_signers),
        std::slice::from_ref(&valid_decision),
        &ctx,
    );
    assert!(!rev_rej.exception_authorized);
    assert!(
        rev_rej
            .rejection_reasons
            .iter()
            .any(|r| r.contains("AUTHORIZER_KEY_REVOKED"))
    );

    // 5. Negative: expired decision rejected.
    let mut stale_ctx = ctx.clone();
    stale_ctx.now_epoch = 1_700_099_999;
    let exp_rej = verify_historical_exceptions(
        Some(&signers),
        std::slice::from_ref(&valid_decision),
        &stale_ctx,
    );
    assert!(!exp_rej.exception_authorized);
    assert!(
        exp_rej
            .rejection_reasons
            .iter()
            .any(|r| r.contains("STALE_OR_EXPIRED"))
    );

    // 6. Negative: tampered candidate commit / tree rejected.
    let mut moved_ctx = ctx.clone();
    moved_ctx.affected_trees = vec!["tree-mutated".to_string()];
    let tree_rej = verify_historical_exceptions(
        Some(&signers),
        std::slice::from_ref(&valid_decision),
        &moved_ctx,
    );
    assert!(!tree_rej.exception_authorized);
    assert!(
        tree_rej
            .rejection_reasons
            .iter()
            .any(|r| r.contains("AFFECTED_TREE_MISMATCH"))
    );

    // 7. Negative: conflicting decisions fail closed.
    let mut conflicting = valid_decision.clone();
    conflicting.decision_id = "dec-1569-002".to_string();
    conflicting.reason = "Conflicting rationale".to_string();
    let payload2 = canonical_signing_payload(&conflicting).unwrap();
    conflicting.signature = to_hex(keypair.sign(&payload2).as_ref());
    let conflict_rej =
        verify_historical_exceptions(Some(&signers), &[valid_decision, conflicting], &ctx);
    assert!(!conflict_rej.exception_authorized);
    assert!(
        conflict_rej
            .rejection_reasons
            .iter()
            .any(|r| r.contains("CONFLICTING_AUTHORITY"))
    );
}

// ============================================================================
// #1093: Semantic claims & plan integrity + governance theater diff detector
// ============================================================================

#[test]
fn issue_1093_semantic_governance_integrity_and_theater_diff_detection() {
    let base_raw = r#"{"sections":{"plan":{"title":"Add obligation ledger","updated_at":"2026-11-22T01:00:00Z"}}}"#;
    let theater_raw = "{\n  \"sections\": {\n    \"plan\": {\n      \"title\": \"Add obligation ledger\",\n      \"updated_at\": \"2026-11-22T02:30:00Z\"\n    }\n  }\n}";
    assert!(detect_governance_theater_diff(base_raw, theater_raw));

    let real_change_raw = r#"{"sections":{"plan":{"title":"Add obligation ledger and clarity validator","updated_at":"2026-11-22T02:30:00Z"}}}"#;
    assert!(!detect_governance_theater_diff(base_raw, real_change_raw));

    let bad_plan = json!({
        "schema_version": "1.1.0",
        "title": "TBD",
        "intent": "todo",
        "state": "APPROVED",
        "todo_ids": ["stale_todo_999"],
        "proof_hooks": ["true"],
        "unknowns": [],
        "human_questions": [],
        "stop_conditions": [],
        "unresolved_contradictions": [],
        "deferred_questions": [],
        "constraints": { "forbidden_paths": [], "file_touch_budget": null },
        "phases": [],
        "updated_at": "2026-11-22T00:00:00Z"
    });

    let bad_claims = json!({
        "$schema": "https://decapod.dev/schemas/research-claims-ledger-1.0.0.schema.json",
        "schema_version": "1.0.0",
        "kind": "research_claims_ledger",
        "ledger_id": "test",
        "title": "Test",
        "purpose": "TBD",
        "authority": {
            "owner": "decapod",
            "source_issue": "https://github.com/DecapodLabs/decapod/issues/682",
            "status": "active",
            "created_at": "2026-11-22T00:00:00Z",
            "updated_at": "2026-11-22T00:00:00Z",
            "change_policy": "wip"
        },
        "scope": {
            "product": "decapod",
            "repository": "DecapodLabs/decapod",
            "in_scope": ["gov"],
            "out_of_scope": []
        },
        "methodology": {
            "falsifiability_standard": "tests",
            "evidence_statuses": [],
            "measurement_rules": []
        },
        "claims": [],
        "governance": {
            "required_claim_fields": ["id"],
            "review_policy": "pr",
            "new_capability_rule": "proof",
            "change_control": {
                "requires_issue": true,
                "requires_validation": true,
                "requires_human_review": true
            }
        }
    });

    let report = validate_semantic_governance_json(
        Some(&bad_plan),
        Some(&bad_claims),
        Some("code_active_123"),
        Some("https://github.com/DecapodLabs/decapod/issues/1093"),
        Some((base_raw, theater_raw)),
    )
    .unwrap();
    assert!(!report.valid);
    assert!(report.theater_diff_detected);
    let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
    assert!(codes.contains("GOVERNANCE_THEATER_DIFF"));
    assert!(codes.contains("PLAN_TITLE_NON_SUBSTANTIVE"));
    assert!(codes.contains("PLAN_INTENT_NON_SUBSTANTIVE"));
    assert!(codes.contains("PLAN_PROOF_HOOK_THEATER"));
    assert!(codes.contains("PLAN_TODO_BINDING_MISMATCH"));
    assert!(codes.contains("CLAIMS_SOURCE_ISSUE_MISMATCH"));
    assert!(codes.contains("CLAIMS_PURPOSE_NON_SUBSTANTIVE"));
    assert!(codes.contains("CLAIMS_CHANGE_POLICY_NON_SUBSTANTIVE"));
}

// ============================================================================
// #1084 & #1083: Derived Obligation Ledger & Lifecycle Stagnation Detection
// ============================================================================

#[test]
fn issue_1084_derived_per_todo_and_pr_obligation_ledger_with_async_effects() {
    let inputs = vec![
        ObligationInput {
            obligation_id: "ob-conservation".into(),
            requirement_id: "REQ-1".into(),
            task_id: "code_1084".into(),
            pr_number: Some(42),
            category: ObligationCategory::Conservation,
            surface: ObligationSurface::Governance,
            owner: "intent_reconcile".into(),
            accepted_revision: "rev-1".into(),
            observed_revision: Some("rev-1".into()),
            rule_id: "rule.conservation".into(),
            phase: "validation".into(),
            evidence_ref: Some("receipt://rev-1".into()),
            evidence_passed: true,
            model_declared_done: true,
            cached_status_only: false,
            requires_human_authority: false,
            invalidations: vec![],
            async_effect: None,
            next_admissible_action: "none".into(),
        },
        ObligationInput {
            obligation_id: "ob-consistency-stale".into(),
            requirement_id: "REQ-2".into(),
            task_id: "code_1084".into(),
            pr_number: Some(42),
            category: ObligationCategory::Consistency,
            surface: ObligationSurface::LivingSpec,
            owner: "project_specs".into(),
            accepted_revision: "rev-2".into(),
            observed_revision: Some("rev-1".into()),
            rule_id: "rule.specs".into(),
            phase: "validation".into(),
            evidence_ref: Some("receipt://rev-1".into()),
            evidence_passed: true,
            model_declared_done: true,
            cached_status_only: false,
            requires_human_authority: false,
            invalidations: vec![InvalidationReason::BaseMoved],
            async_effect: None,
            next_admissible_action: "Run `decapod rpc --op specs.refresh`".into(),
        },
        ObligationInput {
            obligation_id: "ob-proof-async".into(),
            requirement_id: "REQ-3".into(),
            task_id: "code_1084".into(),
            pr_number: Some(42),
            category: ObligationCategory::Proof,
            surface: ObligationSurface::ExternalEffect,
            owner: "ci_provider".into(),
            accepted_revision: "rev-2".into(),
            observed_revision: Some("rev-2".into()),
            rule_id: "rule.ci".into(),
            phase: "publication".into(),
            evidence_ref: None,
            evidence_passed: false,
            model_declared_done: true,
            cached_status_only: false,
            requires_human_authority: false,
            invalidations: vec![],
            async_effect: Some(DelayedAsyncEffect {
                effect_id: "eff-99".into(),
                obligation_id: "ob-proof-async".into(),
                target_system: "github-actions".into(),
                state: AsyncEffectState::Indeterminate,
                deadline_epoch: Some(1_700_010_000),
                evidence_ref: None,
            }),
            next_admissible_action: "Reconcile delayed CI outcome before claiming done".into(),
        },
        // Unrelated global backlog item must NOT block PR #42.
        ObligationInput {
            obligation_id: "ob-unrelated".into(),
            requirement_id: "REQ-999".into(),
            task_id: "code_other".into(),
            pr_number: Some(99),
            category: ObligationCategory::Proof,
            surface: ObligationSurface::Execution,
            owner: "other".into(),
            accepted_revision: "rev-x".into(),
            observed_revision: None,
            rule_id: "rule.other".into(),
            phase: "execution".into(),
            evidence_ref: None,
            evidence_passed: false,
            model_declared_done: false,
            cached_status_only: false,
            requires_human_authority: false,
            invalidations: vec![],
            async_effect: None,
            next_admissible_action: "other".into(),
        },
    ];

    let ledger = derive_obligation_ledger(None, Some(42), &inputs);
    assert!(!ledger.grants_authority);
    assert!(!ledger.ready);
    assert_eq!(ledger.ignored_unrelated_backlog, 1);
    assert_eq!(ledger.conservation_unsatisfied, 0);
    assert_eq!(ledger.consistency_unsatisfied, 1);
    assert_eq!(ledger.proof_unsatisfied, 1);
    assert_eq!(ledger.pending_or_indeterminate_effects, 1);
    assert_eq!(
        ledger
            .items
            .iter()
            .find(|i| i.obligation_id == "ob-consistency-stale")
            .unwrap()
            .status,
        DerivedObligationStatus::Stale
    );
    assert_eq!(
        ledger
            .items
            .iter()
            .find(|i| i.obligation_id == "ob-proof-async")
            .unwrap()
            .status,
        DerivedObligationStatus::IndeterminateAsync
    );
}

#[test]
fn issue_1083_stagnation_detection_resists_paraphrase_survives_compaction_and_distinguishes_provider_wait()
 {
    let mut tracker = StagnationTracker::new("code_1083", 2);
    let mut unresolved = BTreeMap::new();
    unresolved.insert("ob-1".into(), "Run `decapod validate`".into());

    let obs1 = StagnationObservation {
        task_id: "code_1083".into(),
        head_commit: "aaa".into(),
        ledger_fingerprint: "fp-1".into(),
        satisfied_obligations: BTreeSet::new(),
        unresolved_obligations: unresolved.clone(),
        evidence_digests: BTreeSet::new(),
        agent_proposal_prose: "First attempt: checking PR diff".into(),
        now_epoch: 1000,
        provider_wait: None,
    };
    assert!(matches!(
        tracker.observe(&obs1).unwrap(),
        StagnationDisposition::Progressing { .. }
    ));

    // Provider wait with unexpired deadline must NOT count as stagnation!
    let mut wait_obs = obs1.clone();
    wait_obs.provider_wait = Some(ProviderWaitState {
        provider: "github-actions".into(),
        check_id: "ci/core".into(),
        deadline_epoch: 1500,
        recheck_after_secs: 60,
    });
    assert!(matches!(
        tracker.observe(&wait_obs).unwrap(),
        StagnationDisposition::WaitingOnProvider { .. }
    ));

    // Paraphrased proposal with identical governed state -> Stagnant (round 1).
    let mut obs2 = obs1.clone();
    obs2.agent_proposal_prose = "Completely rewritten eloquent explanation of the same diff".into();
    assert!(matches!(
        tracker.observe(&obs2).unwrap(),
        StagnationDisposition::Stagnant {
            repeated_rounds: 1,
            remaining_budget: 1,
            ..
        }
    ));

    // Serialize & deserialize tracker to prove compaction/restart survival.
    let compacted = serde_json::to_string(&tracker).unwrap();
    let mut restored: StagnationTracker = serde_json::from_str(&compacted).unwrap();

    // Another unchanged observation exhausts the finite budget!
    let mut obs3 = obs1.clone();
    obs3.agent_proposal_prose = "Third paraphrase without changing governed state".into();
    assert!(matches!(
        restored.observe(&obs3).unwrap(),
        StagnationDisposition::TerminalExhausted { budget: 2, .. }
    ));

    // Cross-task isolation must fail closed.
    let mut foreign_obs = obs1.clone();
    foreign_obs.task_id = "code_foreign".into();
    assert!(restored.observe(&foreign_obs).is_err());

    // Material governed progress resets stagnation counter.
    let mut progress_obs = obs1.clone();
    progress_obs.head_commit = "bbb".into();
    progress_obs.ledger_fingerprint = "fp-2".into();
    progress_obs.satisfied_obligations.insert("ob-1".into());
    progress_obs.unresolved_obligations.clear();
    assert!(matches!(
        restored.observe(&progress_obs).unwrap(),
        StagnationDisposition::Progressing {
            satisfied_count: 1,
            unresolved_count: 0
        }
    ));
    assert_eq!(restored.stagnant_rounds, 0);
}

// ============================================================================
// #1097 & #1365: Mechanical Clarity Validator & Controlled-English Profile
// ============================================================================

#[test]
fn issue_1097_and_1365_clarity_validator_profiles_terminology_and_cli() {
    assert!(!canonical_decapod_terminology().is_empty());

    let bad_doc = "This revolutionary ai assistant might perform an analysis and then look into the sandbox checkout where the file was written by an executor and a worker while an agent runs an excessively long governance sentence that keeps going well beyond twenty words without stopping for clarity.\n";
    let strict = ClarityProfileConfig::ste_strict();
    let lite = ClarityProfileConfig::ste_lite();

    let r1 = evaluate_artifacts(&[("docs/bad.md", bad_doc)], &strict);
    let r2 = evaluate_artifacts(&[("docs/bad.md", bad_doc)], &strict);
    assert_eq!(r1, r2, "clarity findings must be 100% deterministic");
    assert!(!r1.formal_asd_ste100_compliance);
    assert!(!r1.passed);
    let rules: BTreeSet<&str> = r1.findings.iter().map(|f| f.rule_id.as_str()).collect();
    assert!(rules.contains("CLARITY_SENTENCE_LENGTH"));
    assert!(rules.contains("CLARITY_NOMINALIZATION"));
    assert!(rules.contains("CLARITY_HEDGE_LANGUAGE"));
    assert!(rules.contains("CLARITY_MARKETING_LANGUAGE"));
    assert!(rules.contains("CLARITY_PHRASAL_VERB"));
    assert!(rules.contains("CLARITY_PASSIVE_VOICE"));
    assert!(rules.contains("CLARITY_TERMINOLOGY_DRIFT"));
    assert!(rules.contains("CLARITY_MULTI_ACTION_SENTENCE"));
    assert!(rules.contains("CLARITY_DUPLICATE_CONCEPT"));

    // ste-lite emits warnings rather than errors.
    let lite_report = evaluate_artifacts(&[("docs/bad.md", bad_doc)], &lite);
    assert!(lite_report.passed);
    assert!(lite_report.warning_count > 0);

    // Clean controlled English passes ste-strict with zero findings.
    let clean_doc = "The agent runs `decapod validate` in the isolated worktree.\nDecapod verifies each obligation and records proof.\n";
    let clean_report = evaluate_artifacts(&[("docs/clean.md", clean_doc)], &strict);
    assert!(
        clean_report.passed,
        "unexpected findings: {:?}",
        clean_report.findings
    );
    assert_eq!(clean_report.findings.len(), 0);

    // Human prompts inside governance JSON remain unconstrained (#1365).
    let gov_json = json!({
        "original_prompt": "Please might you look into this revolutionary ai assistant thing?",
        "plan": {
            "intent": "The agent runs validation in the worktree."
        }
    });
    let json_findings =
        validate_governance_json_clarity(".decapod/governance.json", &gov_json, &strict);
    assert!(
        json_findings.is_empty(),
        "human prompts must stay unconstrained: {json_findings:?}"
    );

    // Custom profile works via ClarityProfileKind::parse.
    assert_eq!(
        ClarityProfileKind::parse("ste-strict").unwrap(),
        ClarityProfileKind::SteStrict
    );
    assert_eq!(
        ClarityProfileKind::parse("custom").unwrap(),
        ClarityProfileKind::Custom
    );
    assert!(ClarityProfileKind::parse("unknown-profile").is_err());

    // End-to-end CLI invocation: `decapod validate --clarity --profile ste-strict --format json`.
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".decapod/managed/specs")).unwrap();
    fs::write(
        tmp.path().join(".decapod/managed/specs/INTENT.md"),
        "The revolutionary ai assistant might look into the task.\n",
    )
    .unwrap();
    let bad_cli = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(tmp.path())
        .args([
            "validate",
            "--clarity",
            "--profile",
            "ste-strict",
            "--format",
            "json",
        ])
        .output()
        .expect("run decapod validate --clarity");
    assert!(
        !bad_cli.status.success(),
        "ste-strict CLI must fail deterministically on ambiguous governance prose"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&bad_cli.stdout).unwrap();
    assert_eq!(parsed["formal_asd_ste100_compliance"], false);
    assert_eq!(parsed["passed"], false);

    // Clean spec passes CLI.
    fs::write(
        tmp.path().join(".decapod/managed/specs/INTENT.md"),
        clean_doc,
    )
    .unwrap();
    let ok_cli = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(tmp.path())
        .args([
            "validate",
            "--clarity",
            "--profile",
            "ste-strict",
            "--format",
            "json",
        ])
        .output()
        .expect("run decapod validate --clarity on clean spec");
    assert!(
        ok_cli.status.success(),
        "clean spec failed clarity CLI: {}",
        String::from_utf8_lossy(&ok_cli.stderr)
    );
    let _ = validate_text_clarity(
        "inline.md",
        clean_doc,
        &ClarityProfileConfig::from_kind(ClarityProfileKind::Custom),
    );
    assert!(ClaritySeverity::Error > ClaritySeverity::Warning);
}

// ============================================================================
// #1729, #1726, #1761, #1762, #1768
// ============================================================================

#[test]
fn issue_1729_and_1726_outcome_claim_anchors_and_4d_completion_report() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("src/auth.rs"),
        "pub fn enforce_tenant_isolation() -> bool { true }\n",
    )
    .unwrap();

    let mut roots = BTreeSet::new();
    roots.insert("REQ-1".to_string());
    roots.insert("REQ-2".to_string());

    let claim = AgentOutcomeClaim {
        claim_id: "claim-1".into(),
        declared_change_id: "chg-1".into(),
        source_path: "src/auth.rs".into(),
        symbol_or_callsite: "enforce_tenant_isolation".into(),
        claimed_head_revision: "head-1".into(),
        original_request_revision: "orig-1".into(),
        interpretation_revision: "interp-1".into(),
        linked_root_obligations: vec!["REQ-1".into()],
        claimed_effect: "Enforces tenant isolation".into(),
        trajectory_loop_refs: vec!["loop-1".into()],
        is_proof: false,
    };

    let matched = verify_outcome_claim_anchor(tmp.path(), &claim, "head-1", &roots);
    assert_eq!(matched.status, SourceAnchorStatus::Matched);
    assert_eq!(matched.missing_root_obligations, vec!["REQ-2".to_string()]);

    let head_stale = verify_outcome_claim_anchor(tmp.path(), &claim, "head-2", &roots);
    assert_eq!(head_stale.status, SourceAnchorStatus::HeadChangedStale);

    let mut absent_claim = claim.clone();
    absent_claim.symbol_or_callsite = "moved_or_missing_fn".into();
    let absent = verify_outcome_claim_anchor(tmp.path(), &absent_claim, "head-1", &roots);
    assert_eq!(absent.status, SourceAnchorStatus::SymbolAbsentAtSite);

    // 4-dimension completion report rejects stale executable & zero assertions independently.
    let stale_report = build_completion_report_4d(&CompletionReportInput {
        candidate_head: "head-1".into(),
        built_binary_source_digest: "digest-old".into(),
        current_source_digest: "digest-new".into(),
        executed_assertion_count: 0,
        path_filter_skipped_required_checks: true,
        behavioral_tests_passed: true,
        anchor_verifications: vec![matched.clone()],
        unauthorized_path_transitions: vec!["stale_lease_write".into()],
        observer_missing_events: vec![],
        pr_admission_gate_passed: true,
    });
    assert!(!stale_report.overall_admissible);
    assert_eq!(stale_report.outcome_proof.status, DimensionStatus::Failed);
    assert_eq!(stale_report.path_integrity.status, DimensionStatus::Failed);
    assert_eq!(stale_report.pr_admission.status, DimensionStatus::Failed);

    // Clean 4-dimension pass.
    let ok_report = build_completion_report_4d(&CompletionReportInput {
        candidate_head: "head-1".into(),
        built_binary_source_digest: "digest-new".into(),
        current_source_digest: "digest-new".into(),
        executed_assertion_count: 14,
        path_filter_skipped_required_checks: false,
        behavioral_tests_passed: true,
        anchor_verifications: vec![matched],
        unauthorized_path_transitions: vec![],
        observer_missing_events: vec![],
        pr_admission_gate_passed: true,
    });
    assert!(ok_report.overall_admissible);
    assert_eq!(ok_report.outcome_proof.status, DimensionStatus::Passed);
    assert_eq!(ok_report.path_integrity.status, DimensionStatus::Passed);
    assert_eq!(
        ok_report.observer_completeness.status,
        DimensionStatus::Passed
    );
    assert_eq!(ok_report.pr_admission.status, DimensionStatus::Passed);
}

#[test]
fn issue_1761_canonicalized_approval_single_use_and_conflict_handling() {
    let inv = CanonicalInvocation {
        program: "decapod".into(),
        args: vec!["validate".into(), "--target".into(), "prod".into()],
        subject_commit: "cand-1".into(),
        target_base: "master".into(),
    };
    let digest = inv.canonical_digest();

    let mut ledger = ApprovalLedger::default();
    ledger.approvals.push(CanonicalApprovalRecord {
        approval_id: "appr-1".into(),
        principal_id: "alice".into(),
        canonical_invocation_digest: digest.clone(),
        subject_commit: "cand-1".into(),
        target_base: "master".into(),
        issued_at: 100,
        expires_at: 500,
        single_use: true,
        decision_allow: true,
    });

    // Independent read parity mismatch fails closed.
    let parity_fail = ledger.authorize_and_consume(&inv, 200, "wrong-digest");
    assert!(!parity_fail.admitted);
    assert_eq!(
        parity_fail.rejection_code.as_deref(),
        Some("INDEPENDENT_READ_PARITY_MISMATCH")
    );

    // Argument drift fails closed.
    let mut drifted = inv.clone();
    drifted.args = vec!["validate".into(), "--target".into(), "staging".into()];
    let drifted_digest = drifted.canonical_digest();
    let drift_fail = ledger.authorize_and_consume(&drifted, 200, &drifted_digest);
    assert!(!drift_fail.admitted);
    assert_eq!(
        drift_fail.rejection_code.as_deref(),
        Some("CANONICAL_ARGUMENT_MISMATCH")
    );

    // First exact execution succeeds and consumes single-use approval.
    let first = ledger.authorize_and_consume(&inv, 200, &digest);
    assert!(first.admitted);
    assert_eq!(first.consumed_approval_id.as_deref(), Some("appr-1"));

    // Replay of consumed single-use approval fails closed.
    let replay = ledger.authorize_and_consume(&inv, 201, &digest);
    assert!(!replay.admitted);
    assert_eq!(
        replay.rejection_code.as_deref(),
        Some("SINGLE_USE_REPLAY_REJECTED")
    );

    // Two-principal conflict fails closed.
    let mut conflict_ledger = ApprovalLedger::default();
    conflict_ledger.approvals.push(CanonicalApprovalRecord {
        approval_id: "appr-allow".into(),
        principal_id: "alice".into(),
        canonical_invocation_digest: digest.clone(),
        subject_commit: "cand-1".into(),
        target_base: "master".into(),
        issued_at: 100,
        expires_at: 500,
        single_use: true,
        decision_allow: true,
    });
    conflict_ledger.approvals.push(CanonicalApprovalRecord {
        approval_id: "appr-deny".into(),
        principal_id: "bob".into(),
        canonical_invocation_digest: digest.clone(),
        subject_commit: "cand-1".into(),
        target_base: "master".into(),
        issued_at: 110,
        expires_at: 500,
        single_use: true,
        decision_allow: false,
    });
    let conflict = conflict_ledger.authorize_and_consume(&inv, 200, &digest);
    assert!(!conflict.admitted);
    assert_eq!(
        conflict.rejection_code.as_deref(),
        Some("TWO_PRINCIPAL_CONFLICT")
    );

    // Revoked principal fails closed.
    let mut revoked_ledger = ApprovalLedger::default();
    revoked_ledger.revoked_principals.insert("alice".into());
    revoked_ledger.approvals.push(CanonicalApprovalRecord {
        approval_id: "appr-rev".into(),
        principal_id: "alice".into(),
        canonical_invocation_digest: digest.clone(),
        subject_commit: "cand-1".into(),
        target_base: "master".into(),
        issued_at: 100,
        expires_at: 500,
        single_use: true,
        decision_allow: true,
    });
    let revoked = revoked_ledger.authorize_and_consume(&inv, 200, &digest);
    assert!(!revoked.admitted);
    assert_eq!(revoked.rejection_code.as_deref(), Some("REVOKED_PRINCIPAL"));
}

#[test]
fn issue_1762_and_1768_trusted_grader_and_root_conservation() {
    // #1762: Empty selection rejected.
    let empty = evaluate_trusted_grader(&GraderSelectionInput {
        subject_commit: "c1".into(),
        base_commit: "b1".into(),
        test_selection: "".into(),
        accepted_test_count: 0,
        actual_executable: "cargo".into(),
        observed_executable_sha256: "sha-ok".into(),
        trusted_executable_sha256: "sha-ok".into(),
        environment_fingerprint: "env-1".into(),
        producer_source: "accepted-base".into(),
        checked_obligations: vec!["REQ-1".into()],
        accepted_negative_control_executed: false,
        heldout_counterexample_id: None,
        heldout_mutation_detected: false,
        candidate_authored_tests_only: false,
        checks_passed: true,
    });
    assert_eq!(empty.outcome, GraderOutcome::Failed);
    assert!(
        empty
            .rejection_codes
            .contains(&"EMPTY_TEST_SELECTION".to_string())
    );

    // #1762: Candidate self-tests only (API v2 false-green) -> NotProven!
    let false_green = evaluate_trusted_grader(&GraderSelectionInput {
        subject_commit: "c1".into(),
        base_commit: "b1".into(),
        test_selection: "api_v2_tests".into(),
        accepted_test_count: 5,
        actual_executable: "cargo".into(),
        observed_executable_sha256: "sha-ok".into(),
        trusted_executable_sha256: "sha-ok".into(),
        environment_fingerprint: "env-1".into(),
        producer_source: "candidate-only".into(),
        checked_obligations: vec!["REQ-API-V1".into()],
        accepted_negative_control_executed: false,
        heldout_counterexample_id: None,
        heldout_mutation_detected: false,
        candidate_authored_tests_only: true,
        checks_passed: true,
    });
    assert_eq!(false_green.outcome, GraderOutcome::NotProven);

    // #1762: Substituted runner -> EvaluatorTampered!
    let tampered = evaluate_trusted_grader(&GraderSelectionInput {
        subject_commit: "c1".into(),
        base_commit: "b1".into(),
        test_selection: "tenant_isolation".into(),
        accepted_test_count: 3,
        actual_executable: "fake-runner".into(),
        observed_executable_sha256: "sha-evil".into(),
        trusted_executable_sha256: "sha-ok".into(),
        environment_fingerprint: "env-1".into(),
        producer_source: "accepted-base".into(),
        checked_obligations: vec!["REQ-TENANT".into()],
        accepted_negative_control_executed: true,
        heldout_counterexample_id: Some("cx-tenant-01".into()),
        heldout_mutation_detected: true,
        candidate_authored_tests_only: false,
        checks_passed: true,
    });
    assert_eq!(tampered.outcome, GraderOutcome::EvaluatorTampered);

    // #1768: Root requirement conservation, authorized supersession, freshness loss, nonblocking backlog.
    let roots = vec![
        RootRequirement {
            req_id: "R1".into(),
            source_revision: "rev-1".into(),
            description: "Auth check".into(),
        },
        RootRequirement {
            req_id: "R2".into(),
            source_revision: "rev-1".into(),
            description: "Audit log".into(),
        },
        RootRequirement {
            req_id: "R3".into(),
            source_revision: "rev-1".into(),
            description: "Legacy export".into(),
        },
    ];
    let mappings = vec![
        MappedRootObligation {
            req_id: "R1".into(),
            owner_task_id: "t1".into(),
            lease_id: "lease-1".into(),
            agent_id: "agent-a".into(),
            mapped_head_commit: "head-1".into(),
            mapped_base_commit: "base-1".into(),
            proof_executed: true,
        },
        MappedRootObligation {
            req_id: "R2".into(),
            owner_task_id: "t2".into(),
            lease_id: "lease-2".into(),
            agent_id: "agent-b".into(),
            mapped_head_commit: "head-1".into(),
            mapped_base_commit: "base-1".into(),
            proof_executed: true,
        },
    ];
    let supersessions = vec![RootSupersessionRecord {
        superseded_req_id: "R3".into(),
        replacement_req_id: None,
        authorized_by: "maintainer".into(),
        authority_ref: "issue#1768-comment-9".into(),
        reason: "Explicitly deprecated by maintainer".into(),
        preserved_original_description: "Legacy export".into(),
    }];
    let mut leases = BTreeMap::new();
    leases.insert("t1".into(), ("lease-1".into(), "agent-a".into()));
    leases.insert("t2".into(), ("lease-2".into(), "agent-b".into()));
    let custody = ActiveCustodySnapshot {
        task_leases: leases.clone(),
        current_head_commit: "head-1".into(),
        current_base_commit: "base-1".into(),
        unrelated_backlog_task_ids: vec!["backlog-99".into(), "backlog-100".into()],
    };

    let conserved = evaluate_root_conservation(&roots, &mappings, &supersessions, &custody);
    assert!(conserved.conserved);
    assert!(conserved.all_proven);
    assert_eq!(conserved.superseded_history.len(), 1);
    assert_eq!(conserved.ignored_unrelated_backlog_count, 2);

    // Agent swap & base move invalidate freshness!
    let mut swapped_leases = leases;
    swapped_leases.insert("t2".into(), ("lease-2".into(), "agent-other".into()));
    let stale_custody = ActiveCustodySnapshot {
        task_leases: swapped_leases,
        current_head_commit: "head-1".into(),
        current_base_commit: "base-2".into(),
        unrelated_backlog_task_ids: vec![],
    };
    let lost = evaluate_root_conservation(&roots, &mappings, &supersessions, &stale_custody);
    assert!(!lost.conserved);
    assert!(!lost.freshness_lost.is_empty());
}

// ============================================================================
// #1907: Opt-in Formal Obligations & Finite-Capacity Conservation State Machine
// ============================================================================

#[test]
fn issue_1907_formal_obligations_and_capacity_conservation_state_machine() {
    let model_src = "import Decapod.Conservation\ndef conservation (c a r s av cap : Nat) := c + a + r + s + av = cap\n";
    let clean_proof = "import Decapod.Conservation\ntheorem step_preserves : True := trivial\n";
    let hole_proof = "import Decapod.Conservation\ntheorem step_preserves : True := by sorry\n";

    let pinned_sha = sha256_hex(b"/usr/local/bin/lean-4.12.0");
    let record = FormalObligationRecord {
        schema: FORMAL_OBLIGATION_SCHEMA.into(),
        obligation_id: "formal-cap-001".into(),
        property_name: "capacity_conservation".into(),
        scope: FormalClaimScope::ImplementationLinked,
        checker_kind: FormalCheckerKind::Lean,
        checker_version: "4.12.0".into(),
        checker_executable_sha256: pinned_sha.clone(),
        checker_flags: vec!["--json".into()],
        model_source_path: "formal/conservation.lean".into(),
        model_source_sha256: sha256_hex(model_src.as_bytes()),
        proof_source_path: "formal/conservation_proof.lean".into(),
        proof_source_sha256: sha256_hex(clean_proof.as_bytes()),
        declared_assumptions: vec!["u64 arithmetic does not overflow issued_capacity".into()],
        declared_imports: vec!["import Decapod.Conservation".into()],
        repository_id: "DecapodLabs/decapod".into(),
        task_id: "code_1907".into(),
        source_commit: "abc123".into(),
        policy_digest: "pol-1".into(),
        linking_argument: Some(FormalLinkingArgument {
            linked_rust_symbols: vec!["CapacityAccountingState::apply".into()],
            conformance_test_refs: vec!["tests/obligation_ledger_tests.rs".into()],
            modeled_bounds: vec!["finite u64 capacity".into()],
            unmodeled_host_effects: vec!["disk I/O fsync failures".into()],
            conformance_tests_passed: true,
        }),
    };

    let make_transcript = |proof: &str, trusted: bool, exec_sha: &str| {
        let stdout = "lean 4.12.0: theorem step_preserves verified".to_string();
        let stderr = String::new();
        let att = compute_runner_attestation(
            &record.obligation_id,
            exec_sha,
            &sha256_hex(model_src.as_bytes()),
            &sha256_hex(proof.as_bytes()),
            0,
            &stdout,
            &stderr,
        );
        FormalCheckerTranscript {
            checker_kind: FormalCheckerKind::Lean,
            checker_version: "4.12.0".into(),
            observed_executable_sha256: exec_sha.to_string(),
            exit_code: 0,
            stdout,
            stderr,
            proof_source_contents: proof.to_string(),
            model_source_contents: model_src.to_string(),
            runtime_ms: 42,
            storage_bytes: 1024,
            runner_attestation_sha256: att,
            produced_by_trusted_runner: trusted,
        }
    };

    // Disabled config -> zero interference!
    let disabled = verify_formal_obligation(
        &FormalObligationConfig::default(),
        &record,
        &make_transcript(clean_proof, true, &pinned_sha),
    );
    assert_eq!(disabled.status, FormalVerificationStatus::Disabled);

    let enabled_cfg = FormalObligationConfig {
        enabled: true,
        pinned_checkers: vec![PinnedFormalChecker {
            kind: FormalCheckerKind::Lean,
            version: "4.12.0".into(),
            executable_path: "/usr/local/bin/lean".into(),
            executable_sha256: pinned_sha.clone(),
            allowed_flags: vec!["--json".into()],
        }],
    };

    // Valid positive control.
    let ok = verify_formal_obligation(
        &enabled_cfg,
        &record,
        &make_transcript(clean_proof, true, &pinned_sha),
    );
    assert_eq!(
        ok.status,
        FormalVerificationStatus::VerifiedImplementationLinked
    );
    assert!(ok.accepted_for_declared_assumptions_only);
    assert!(!ok.blanket_production_correctness_claimed);

    // Negative: admitted hole (`sorry`) rejected even if record hash matches!
    let mut hole_record = record.clone();
    hole_record.proof_source_sha256 = sha256_hex(hole_proof.as_bytes());
    let hole_res = verify_formal_obligation(
        &enabled_cfg,
        &hole_record,
        &make_transcript(hole_proof, true, &pinned_sha),
    );
    assert_eq!(hole_res.status, FormalVerificationStatus::Rejected);
    assert!(
        hole_res
            .rejection_codes
            .contains(&"ADMITTED_HOLE_REJECTED".to_string())
    );

    // Negative: agent-authored fabricated transcript rejected!
    let fake = verify_formal_obligation(
        &enabled_cfg,
        &record,
        &make_transcript(clean_proof, false, &pinned_sha),
    );
    assert_eq!(fake.status, FormalVerificationStatus::Rejected);
    assert!(
        fake.rejection_codes
            .contains(&"FABRICATED_TRANSCRIPT".to_string())
    );

    // Negative: untrusted executable substitution rejected!
    let evil_bin = verify_formal_obligation(
        &enabled_cfg,
        &record,
        &make_transcript(clean_proof, true, "deadbeef"),
    );
    assert_eq!(evil_bin.status, FormalVerificationStatus::Rejected);
    assert!(
        evil_bin
            .rejection_codes
            .contains(&"UNTRUSTED_EXECUTABLE_SUBSTITUTION".to_string())
    );

    // Finite-capacity conservation state machine invariant tests.
    let mut cap = CapacityAccountingState::new(100);
    assert!(cap.conservation_holds());
    cap.apply(CapacityTransition::Allocate { amount: 40 })
        .unwrap();
    cap.apply(CapacityTransition::Arm { amount: 30 }).unwrap();
    cap.apply(CapacityTransition::Settle { amount: 10 })
        .unwrap();
    cap.apply(CapacityTransition::MarkIndeterminate { amount: 15 })
        .unwrap();
    let avail_before_timeout = cap.available;
    // Timeout alone must NOT replenish available capacity!
    cap.apply(CapacityTransition::TimeoutWithoutAuthoritativeReconciliation)
        .unwrap();
    assert_eq!(cap.available, avail_before_timeout);
    assert_eq!(cap.retained_indeterminate, 15);
    // Crash/restart moves remaining armed (5) to retained_indeterminate (20), no double-release!
    cap.apply(CapacityTransition::RecoverAfterRestart).unwrap();
    assert_eq!(cap.armed, 0);
    assert_eq!(cap.retained_indeterminate, 20);
    // Authoritative reconciliation settles 10 and releases 10.
    cap.apply(CapacityTransition::ReconcileIndeterminate {
        amount: 10,
        settled: true,
    })
    .unwrap();
    cap.apply(CapacityTransition::ReconcileIndeterminate {
        amount: 10,
        settled: false,
    })
    .unwrap();
    assert!(cap.conservation_holds());
    assert_eq!(cap.committed_or_spent, 20);
    assert_eq!(cap.available, 70);
    assert_eq!(cap.reserved, 10);
}

// ============================================================================
// #1095, #1772, #1774, #1796: Deterministic Offline Telemetry & Benchmarks
// ============================================================================

#[test]
fn issue_1095_1772_1774_1796_offline_telemetry_and_benchmark_harnesses() {
    // #1095 telemetry JSONL + OTel span.
    let tel = ExecutionTelemetryRecord {
        run_id: "run-1095-01".into(),
        mode: "cold".into(),
        condition: "decapod".into(),
        active_human_minutes: 3.5,
        model_calls: 12,
        estimated_cost_usd: 0.42,
        harmful_context_events: 0,
        verification_passed: true,
        live_real_agent_run: false,
    };
    assert!(tel.to_jsonl_line().contains("\"active_human_minutes\":3.5"));
    assert_eq!(
        tel.to_otel_span_json()["attributes"]["decapod.mode"],
        "cold"
    );

    // #1772 relevance gate benchmark record.
    let rel = RelevanceGateBenchmarkCase {
        case_id: "rep-bias-01".into(),
        repeated_irrelevant_term: "kubernetes".into(),
        repetitions: 25,
        baseline_no_file_injected_irrelevant: false,
        static_file_injected_irrelevant: true,
        decapod_gate_rejected_irrelevant: true,
        mandatory_policy_preserved: true,
        requires_live_paired_agent_eval: true,
    };
    assert!(rel.decapod_gate_rejected_irrelevant);
    assert!(rel.requires_live_paired_agent_eval);

    // #1774 constitution promotion check rejects research/* binding self-promotion & fake enforcement.
    let bad_promo =
        evaluate_constitution_promotion("research/NEW_IDEA", "binding", true, None, false);
    assert!(!bad_promo.admitted);
    let bad_gate = evaluate_constitution_promotion(
        "core/CUSTOM",
        "advisory",
        false,
        Some("gate.fake.missing"),
        false,
    );
    assert!(!bad_gate.admitted);
    let ok_advisory =
        evaluate_constitution_promotion("research/NEW_IDEA", "advisory", false, None, false);
    assert!(ok_advisory.admitted);
    assert!(!ok_advisory.maintainer_review_required);

    // #1796 task-first review offline benchmark honestly flags pending live paired eval.
    let bench = TaskFirstReviewBenchmark::evaluate_offline("bench-1796-01", 4, 2, 4, 0, 1.2);
    assert!(bench.live_paired_eval_pending);
    assert_eq!(
        bench.preliminary_recommendation,
        "candidate_optional_step_pending_live_paired_eval"
    );
}

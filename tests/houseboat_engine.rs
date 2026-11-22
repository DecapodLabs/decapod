//! Comprehensive integration test suite for the Houseboat Core Engine (#852-#859),
//! Storage Compaction & Recovery (#1630), Knowledge Write-Time Attestation (#657),
//! Offline Init Reconciliation Matrix (#1024), Cross-Crash Unknown-Outcome
//! Reconciliation (#1367), Darkmatter Read-Only Contract (#1711), and
//! 3-Worktree Combined-Candidate Negative Control (#1763).

use decapod::core::db;
use decapod::core::houseboat::{
    BudgetPolicy, CombinedReconciliationProof, CrashRecoveryVerdict, DarkmatterReadonlySnapshot,
    DegradedMode, DependencyCriticality, DependencyKind, DependencyStatus, DeprecatedApiUsage,
    DriftPolicyConfig, DurableJournal, EscalationLevel, FleetCoordGraph, FleetEdge, FleetEdgeKind,
    HabitatManifest, InterruptedExecutionRecord, JournalDomain, KnowledgeAttestation,
    KnowledgeClass, KnowledgeSourceKind, LeakedResourceKind, MotionAdjudicator,
    MotionViolationKind, ObservedWorkspaceResource, OperationalState, OutcomeSettlement,
    PartialWriteMarker, PartialWriteRecoveryAction, ReservationState, ResourceBudgetLedger,
    ResourceUnit, SeaworthinessRemediationLoop, SeaworthinessSeverity, ToolchainCheckInput,
    WorkspaceLifecycleMachine, WorkspaceLifecycleState, WorkunitMotionSnapshot, WorkunitNode,
    WritePhase, classify_dependencies, detect_leaked_resources, evaluate_exit_degraded,
    evaluate_seaworthiness, reconcile_after_crash, reconcile_machine_journals,
    recover_partial_write, run_local_storage_compaction_benchmark, validate_darkmatter_snapshot,
};
use decapod::core::mooring::{
    ArtifactProvenance, CollaborationContract, ContractState, MooringDenialCode,
    MooringEnforcementRequest, MooringOperation, MooringRegistry, MooringSubjectKind,
};
use decapod::core::storage_compaction;
use decapod::core::todo::{self, ClaimMode, TodoCommand};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_decapod(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(args)
        .env("XDG_CONFIG_HOME", dir.join(".xdg-config"))
        .env("XDG_DATA_HOME", dir.join(".xdg-data"))
        .env("DECAPOD_AGENT_ID", "houseboat_test")
        .env("DECAPOD_CLAIM_AUTORUN", "0")
        .env_remove("DECAPOD_SESSION_PASSWORD")
        .output()
        .expect("run decapod binary")
}

// ============================================================================
// #852: Typed Fleet Graph Edges & Multi-Workunit Merge/Reconciliation Gates
// ============================================================================

#[test]
fn houseboat_852_typed_fleet_edges_cycle_rejection_and_merge_gate() {
    let mut graph = FleetCoordGraph::new();
    let target = "sha256:accepted_base_001";

    for (id, sha, paths, verified) in [
        ("wu-a", "sha-a", ["src/api.rs"], true),
        ("wu-b", "sha-b", ["src/api.rs"], true),
        ("wu-c", "sha-c", ["src/worker.rs"], true),
        ("wu-old", "sha-old", ["src/legacy.rs"], true),
    ] {
        graph
            .add_node(WorkunitNode {
                workunit_id: id.to_string(),
                task_id: format!("task-{id}"),
                branch: format!("agent/test/{id}"),
                head_sha: sha.to_string(),
                base_sha: target.to_string(),
                touched_paths: paths.into_iter().map(String::from).collect(),
                trajectory_verified: verified,
                proof_refs: vec![format!("proof:{id}")],
                capacity_units: 1,
            })
            .unwrap();
    }

    // Typed non-prerequisite edges.
    graph
        .add_edge(FleetEdge {
            from_workunit: "wu-a".to_string(),
            to_workunit: "wu-b".to_string(),
            kind: FleetEdgeKind::ConflictsWith,
            reason: "shared src/api.rs schema mutation".to_string(),
            required_proof_ref: Some("proof:combined-ab".to_string()),
        })
        .unwrap();
    graph
        .add_edge(FleetEdge {
            from_workunit: "wu-c".to_string(),
            to_workunit: "wu-old".to_string(),
            kind: FleetEdgeKind::Supersedes,
            reason: "replaces legacy worker path".to_string(),
            required_proof_ref: None,
        })
        .unwrap();
    graph
        .add_edge(FleetEdge {
            from_workunit: "wu-c".to_string(),
            to_workunit: "wu-a".to_string(),
            kind: FleetEdgeKind::DerivesFrom,
            reason: "consumes api trait from wu-a".to_string(),
            required_proof_ref: None,
        })
        .unwrap();
    graph
        .add_edge(FleetEdge {
            from_workunit: "wu-a".to_string(),
            to_workunit: "wu-c".to_string(),
            kind: FleetEdgeKind::IntegrationRequired,
            reason: "combined integration check".to_string(),
            required_proof_ref: None,
        })
        .unwrap();

    // Cyclic DependsOn/DerivesFrom edge fails closed.
    let cycle_err = graph
        .add_edge(FleetEdge {
            from_workunit: "wu-a".to_string(),
            to_workunit: "wu-c".to_string(),
            kind: FleetEdgeKind::DependsOn,
            reason: "illegal cycle".to_string(),
            required_proof_ref: None,
        })
        .unwrap_err();
    assert!(cycle_err.contains("FLEET_DEPENDENCY_CYCLE"));

    // Superseded workunit is rejected for admission.
    let superseded = graph.evaluate_admission("wu-old").unwrap();
    assert!(!superseded.admitted);
    assert!(
        superseded
            .blockers
            .iter()
            .any(|b| b.contains("WORKUNIT_SUPERSEDED"))
    );

    // Overlapping/conflicting merge without combined proof fails closed.
    let blocked = graph.evaluate_combined_merge_gate(&["wu-a", "wu-b"], target, None);
    assert!(!blocked.allowed);
    assert_eq!(blocked.code, "COMBINED_CANDIDATE_RECONCILIATION_REQUIRED");
    assert!(blocked.overlapping_paths.contains_key("src/api.rs"));

    // Providing an exact verified CombinedReconciliationProof admits the merge.
    let proof = CombinedReconciliationProof {
        proof_id: "proof:combined-ab".to_string(),
        accepted_target_sha: target.to_string(),
        combined_tree_sha: "tree:ab".to_string(),
        participating_heads: BTreeMap::from([
            ("wu-a".to_string(), "sha-a".to_string()),
            ("wu-b".to_string(), "sha-b".to_string()),
        ]),
        passed: true,
    };
    let admitted = graph.evaluate_combined_merge_gate(&["wu-a", "wu-b"], target, Some(&proof));
    assert!(admitted.allowed, "blockers: {:?}", admitted.blockers);

    // Stale target SHA invalidates combined proof.
    let stale =
        graph.evaluate_combined_merge_gate(&["wu-a", "wu-b"], "sha256:moved_target", Some(&proof));
    assert!(!stale.allowed);
    assert!(
        stale
            .blockers
            .iter()
            .any(|b| b.contains("COMBINED_TARGET_DRIFT"))
    );
}

// ============================================================================
// #853: Workspace Lifecycle State Machine, Habitat Manifest & Leak Detector
// ============================================================================

#[test]
fn houseboat_853_workspace_lifecycle_habitat_and_leak_detection() {
    let habitat = HabitatManifest::build(
        "ghcr.io/decapodlabs/decapod:v0.162.0-debian",
        "sha256:1234567890abcdef",
        BTreeMap::from([("rustc".to_string(), "1.96.1".to_string())]),
        BTreeSet::from(["DECAPOD_AGENT_ID".to_string()]),
        vec!["/workspace:/workspace:rw".to_string()],
    );
    assert!(habitat.verify_reproducibility());

    let mut ws = WorkspaceLifecycleMachine::new("ws-1", "wu-1", habitat);
    ws.transition(WorkspaceLifecycleState::Active, "agent", "ready", 100)
        .unwrap();
    // Invalid jump straight from Active -> Terminated fails closed.
    let invalid = ws.transition(WorkspaceLifecycleState::Terminated, "agent", "skip", 105);
    assert!(invalid.is_err());

    ws.dirty_worktree = true;
    ws.transition(WorkspaceLifecycleState::Abandoned, "agent", "timeout", 110)
        .unwrap();
    ws.transition(WorkspaceLifecycleState::Teardown, "agent", "cleanup", 120)
        .unwrap();
    // Non-force termination preserves dirty worktree.
    let dirty_block = ws.transition(WorkspaceLifecycleState::Terminated, "agent", "rm", 125);
    assert!(
        dirty_block
            .unwrap_err()
            .contains("DIRTY_WORKTREE_PRESERVED")
    );

    // Recover -> clean -> teardown -> terminated succeeds.
    ws.transition(
        WorkspaceLifecycleState::Recovered,
        "operator",
        "inspect",
        130,
    )
    .unwrap();
    ws.dirty_worktree = false;
    ws.transition(WorkspaceLifecycleState::Teardown, "operator", "clean", 140)
        .unwrap();
    ws.transition(WorkspaceLifecycleState::Terminated, "operator", "done", 150)
        .unwrap();
    assert_eq!(ws.state, WorkspaceLifecycleState::Terminated);

    // Leaked resource detector across containers, mounts, temp credentials, dirty worktrees.
    let active = BTreeSet::from(["wu-live".to_string()]);
    let report = detect_leaked_resources(
        &active,
        &[
            ObservedWorkspaceResource {
                resource_id: "ctr-live".to_string(),
                kind: LeakedResourceKind::Container,
                owner_workunit: Some("wu-live".to_string()),
                created_at_epoch: 100,
                ttl_secs: 300,
                dirty: false,
            },
            ObservedWorkspaceResource {
                resource_id: "ctr-orphan".to_string(),
                kind: LeakedResourceKind::Container,
                owner_workunit: Some("wu-dead".to_string()),
                created_at_epoch: 100,
                ttl_secs: 300,
                dirty: false,
            },
            ObservedWorkspaceResource {
                resource_id: "mnt-stale".to_string(),
                kind: LeakedResourceKind::Mount,
                owner_workunit: None,
                created_at_epoch: 50,
                ttl_secs: 60,
                dirty: false,
            },
            ObservedWorkspaceResource {
                resource_id: "cred-expired".to_string(),
                kind: LeakedResourceKind::TempCredential,
                owner_workunit: Some("wu-live".to_string()),
                created_at_epoch: 50,
                ttl_secs: 60,
                dirty: false,
            },
            ObservedWorkspaceResource {
                resource_id: "wt-dirty".to_string(),
                kind: LeakedResourceKind::OrphanWorktree,
                owner_workunit: Some("wu-dead".to_string()),
                created_at_epoch: 50,
                ttl_secs: 60,
                dirty: true,
            },
        ],
        200,
    );
    assert_eq!(report.scanned_count, 5);
    assert_eq!(report.leaked_count, 4);
    let dirty_finding = report
        .findings
        .iter()
        .find(|f| f.resource_id == "wt-dirty")
        .unwrap();
    assert!(dirty_finding.requires_human_review);
}

// ============================================================================
// #854: Configurable Drift Policy, Abandoned API/Toolchain & Bounded Remediation
// ============================================================================

#[test]
fn houseboat_854_configurable_drift_policy_and_bounded_remediation() {
    // Non-blocking configurable projection severity + threshold.
    let policy = DriftPolicyConfig {
        projection_drift_severity: SeaworthinessSeverity::Warning,
        unsupported_toolchain_severity: SeaworthinessSeverity::Blocking,
        deprecated_api_severity: SeaworthinessSeverity::Warning,
        max_warning_threshold: 2,
        max_remediation_attempts: 2,
        allow_uncertain_auto_apply: false,
    };

    // Warning-only drift within threshold remains seaworthy.
    let warn_only = evaluate_seaworthiness(
        &policy,
        &["specs/ARCHITECTURE.md".to_string()],
        &[ToolchainCheckInput {
            tool: "rustc".to_string(),
            observed_version: "1.96.1".to_string(),
            min_supported_version: "1.90.0".to_string(),
            sunset_epoch: None,
        }],
        &[DeprecatedApiUsage {
            symbol: "legacy_connect".to_string(),
            file_path: "src/client.rs".to_string(),
            replacement: Some("connect_v2".to_string()),
            sunset_epoch: Some(2_000_000_000),
            confidence_pct: 80,
        }],
        1_800_000_000,
    );
    assert!(warn_only.seaworthy);
    assert_eq!(warn_only.warning_count, 2);

    // Unsupported toolchain + past-sunset API blocks seaworthiness.
    let blocked = evaluate_seaworthiness(
        &policy,
        &[],
        &[ToolchainCheckInput {
            tool: "rustc".to_string(),
            observed_version: "1.75.0".to_string(),
            min_supported_version: "1.90.0".to_string(),
            sunset_epoch: None,
        }],
        &[DeprecatedApiUsage {
            symbol: "abandoned_v1".to_string(),
            file_path: "src/old.rs".to_string(),
            replacement: None,
            sunset_epoch: Some(1_700_000_000),
            confidence_pct: 75,
        }],
        1_800_000_000,
    );
    assert!(!blocked.seaworthy);
    assert_eq!(blocked.blocking_count, 2);

    // Bounded remediation loop preserves uncertainty and caps attempts.
    let mut loop_mgr = SeaworthinessRemediationLoop::new(policy);
    let a1 = loop_mgr.propose_next(&blocked.findings[1]).unwrap();
    assert_eq!(a1.attempt, 1);
    assert!(!a1.auto_apply_permitted);
    assert!(a1.uncertainty_preserved.is_some());
    let _a2 = loop_mgr.propose_next(&blocked.findings[0]).unwrap();
    let exhausted = loop_mgr.propose_next(&blocked.findings[0]).unwrap_err();
    assert!(exhausted.contains("REMEDIATION_BUDGET_EXHAUSTED"));
}

// ============================================================================
// #855: Typed Expiring Mooring / Collaboration Contracts (`mooring.rs`)
// ============================================================================

#[test]
fn houseboat_855_mooring_contracts_fail_closed_and_prevent_wrapper_bypass() {
    let prov = ArtifactProvenance {
        artifact_id: "ext:patch-42".to_string(),
        source_uri: "https://partner.example/patch-42.diff".to_string(),
        content_sha256: "sha256:1111111111111111111111111111111111111111111111111111111111111111"
            .to_string(),
        wrapper_sha256: "sha256:2222222222222222222222222222222222222222222222222222222222222222"
            .to_string(),
        verification_obligations: BTreeSet::from([
            "sig_check".to_string(),
            "sha256_match".to_string(),
        ]),
        verified_obligations: BTreeSet::from(["sig_check".to_string(), "sha256_match".to_string()]),
        approved_by: Some("maintainer:alice".to_string()),
    };
    let contract = CollaborationContract::new(
        "mooring:partner:42",
        MooringSubjectKind::ExternalArtifact,
        "ext:patch-42",
        [
            MooringOperation::Read,
            MooringOperation::ImportArtifact,
            MooringOperation::Publish,
        ],
        [MooringOperation::Admin],
        vec!["src/adapters".to_string()],
        ["import:verified".to_string()],
        1000,
        2000,
        "maintainer:alice",
        Some(prov.clone()),
    );

    let mut reg = MooringRegistry::new();
    reg.register(contract).unwrap();

    // Valid bounded publish passes.
    let ok = reg
        .check_publish_gate(
            "mooring:partner:42",
            Some("src/adapters/partner.rs".to_string()),
            1500,
            Some(prov.content_sha256.clone()),
            Some(prov.wrapper_sha256.clone()),
        )
        .unwrap();
    assert!(ok.allowed);

    // Wrapper metadata change without re-verification fails closed.
    let tampered = reg
        .check_publish_gate(
            "mooring:partner:42",
            Some("src/adapters/partner.rs".to_string()),
            1500,
            Some(prov.content_sha256.clone()),
            Some(
                "sha256:9999999999999999999999999999999999999999999999999999999999999999"
                    .to_string(),
            ),
        )
        .unwrap();
    assert!(!tampered.allowed);
    assert_eq!(
        tampered.denial_code,
        Some(MooringDenialCode::ProvenanceTampered)
    );

    // Path traversal / out of boundary fails closed.
    let escape = reg
        .enforce_at_boundary(
            "mooring:partner:42",
            &MooringEnforcementRequest {
                operation: MooringOperation::Read,
                target_path: Some("src/adapters/../../.github/workflows/ci.yml".to_string()),
                required_capability: None,
                now_epoch: 1500,
                observed_content_sha256: None,
                observed_wrapper_sha256: None,
            },
        )
        .unwrap();
    assert!(!escape.allowed);
    assert_eq!(
        escape.denial_code,
        Some(MooringDenialCode::OutOfDataBoundary)
    );

    // Expired contract fails closed.
    let expired = reg
        .check_publish_gate(
            "mooring:partner:42",
            Some("src/adapters/x.rs".to_string()),
            2000,
            None,
            None,
        )
        .unwrap();
    assert!(!expired.allowed);
    assert_eq!(
        expired.denial_code,
        Some(MooringDenialCode::ContractExpired)
    );

    // Idempotent revocation records evidence and blocks further operations.
    let c = reg.get_mut("mooring:partner:42").unwrap();
    let rev1 = c.revoke("maintainer:alice", "partner rotation", 1600);
    let rev2 = c.revoke("maintainer:alice", "partner rotation", 1601);
    assert_eq!(rev1.evidence_digest, rev2.evidence_digest);
    assert_eq!(c.state, ContractState::Revoked);
    assert!(c.disengage("maintainer:alice", "late", 1700).is_err());
}

// ============================================================================
// #856: Unified Durable Journal, Idempotent Replay & Conflict-Aware Sync
// ============================================================================

#[test]
fn houseboat_856_durable_journal_replay_and_conflict_aware_sync() {
    let mut j_local = DurableJournal::new("machine-a");
    j_local.append(
        "e1",
        JournalDomain::Todo,
        "t1",
        "claimed",
        b"claim-a",
        100,
        true,
    );
    j_local.append(
        "e2",
        JournalDomain::Lease,
        "l1",
        "extended",
        b"lease-a",
        101,
        true,
    );
    j_local.append(
        "e3",
        JournalDomain::Capsule,
        "c1",
        "materialized",
        b"cap-a",
        102,
        true,
    );
    j_local.append(
        "e4",
        JournalDomain::Proof,
        "p1",
        "verified",
        b"proof-a",
        103,
        true,
    );
    // Uncommitted tail must NOT invent success.
    j_local.append(
        "e5-partial",
        JournalDomain::Workunit,
        "w1",
        "publishing",
        b"part",
        104,
        false,
    );
    // Duplicate entry_id is idempotent.
    j_local.append(
        "e4",
        JournalDomain::Proof,
        "p1",
        "verified",
        b"proof-a",
        103,
        true,
    );

    let replay = j_local.replay().unwrap();
    assert_eq!(replay.committed_count, 4);
    assert_eq!(
        replay.incomplete_transitions,
        vec!["e5-partial".to_string()]
    );
    assert!(
        !replay
            .entity_states
            .contains_key(&(JournalDomain::Workunit, "w1".to_string()))
    );

    let cp = j_local.checkpoint("cp-1").unwrap();
    assert_eq!(cp.last_committed_seq, 4);

    // Contradictory remote journal surfaces explicit conflict (NO silent last-writer-wins!).
    let mut j_remote = DurableJournal::new("machine-b");
    j_remote.append(
        "e1-b",
        JournalDomain::Todo,
        "t1",
        "abandoned",
        b"abandon-b",
        200,
        true,
    );
    let conflict_sync = reconcile_machine_journals(&j_local, &j_remote);
    assert!(!conflict_sync.clean);
    assert_eq!(conflict_sync.conflicts.len(), 1);
    assert!(conflict_sync.conflicts[0].resolution_required);

    // Disjoint remote journal merges cleanly and preserves offline evidence hashes.
    let mut j_disjoint = DurableJournal::new("machine-c");
    let proof_c = j_disjoint.append(
        "e9-c",
        JournalDomain::Proof,
        "p9",
        "verified",
        b"proof-c",
        150,
        true,
    );
    let clean_sync = reconcile_machine_journals(&j_local, &j_disjoint);
    assert!(clean_sync.clean);
    assert!(
        clean_sync
            .preserved_evidence_hashes
            .contains(&proof_c.payload_sha256)
    );
}

// ============================================================================
// #857: MotionAdjudicator — Premature/Out-of-Scope Work & Recovery Proposals
// ============================================================================

#[test]
fn houseboat_857_motion_adjudicator_detects_violations_and_proposes_bounded_recovery() {
    let bad = WorkunitMotionSnapshot {
        workunit_id: "wu-77".to_string(),
        plan_converged: false,
        prerequisite_proofs_passed: false,
        declared_scope_prefixes: vec!["src/decapod/core/houseboat.rs".to_string()],
        touched_paths: vec![
            "src/decapod/core/houseboat.rs".to_string(),
            ".github/workflows/ci.yml".to_string(),
        ],
        consecutive_failures: 3,
        failure_signatures: vec!["E0425".to_string(), "E0425".to_string()],
        authority_state: "BLOCKED".to_string(),
        projected_motion_state: "active".to_string(),
    };
    let adj = MotionAdjudicator::adjudicate(&bad);
    assert!(!adj.permitted);
    assert!(!adj.dual_path_consistent);
    assert_eq!(adj.violations.len(), 4);
    assert!(
        adj.violations
            .iter()
            .any(|(k, _)| *k == MotionViolationKind::PrematureImplementation)
    );
    assert!(
        adj.violations
            .iter()
            .any(|(k, _)| *k == MotionViolationKind::OutOfScopeMutation)
    );
    assert!(
        adj.violations
            .iter()
            .any(|(k, _)| *k == MotionViolationKind::StalledRetryLoop)
    );
    assert!(
        adj.violations
            .iter()
            .any(|(k, _)| *k == MotionViolationKind::AuthorityProjectionMismatch)
    );
    assert!(
        adj.recovery_proposals
            .iter()
            .all(|p| p.preserves_proof_gates)
    );

    let good = WorkunitMotionSnapshot {
        workunit_id: "wu-78".to_string(),
        plan_converged: true,
        prerequisite_proofs_passed: true,
        declared_scope_prefixes: vec!["src/decapod/core/".to_string()],
        touched_paths: vec!["src/decapod/core/houseboat.rs".to_string()],
        consecutive_failures: 0,
        failure_signatures: vec![],
        authority_state: "VERIFIED".to_string(),
        projected_motion_state: "completed".to_string(),
    };
    let ok = MotionAdjudicator::adjudicate(&good);
    assert!(ok.permitted);
    assert!(ok.dual_path_consistent);
}

// ============================================================================
// #858: Resilience, Degraded Modes, Partial-Write Recovery & Exit Contract
// ============================================================================

#[test]
fn houseboat_858_dependency_classification_partial_write_and_exit_degraded() {
    // Optional dependency loss enters visible degraded mode with exit contract.
    let degraded = classify_dependencies(&[
        DependencyStatus {
            name: "local-sqlite".to_string(),
            kind: DependencyKind::LocalStore,
            criticality: DependencyCriticality::Critical,
            available: true,
            detail: "ok".to_string(),
        },
        DependencyStatus {
            name: "propodus-cloud".to_string(),
            kind: DependencyKind::CloudBackend,
            criticality: DependencyCriticality::Optional,
            available: false,
            detail: "offline".to_string(),
        },
        DependencyStatus {
            name: "podman".to_string(),
            kind: DependencyKind::ContainerRuntime,
            criticality: DependencyCriticality::Optional,
            available: false,
            detail: "stopped".to_string(),
        },
    ]);
    assert_eq!(degraded.state, OperationalState::Degraded);
    assert!(
        degraded
            .active_degraded_modes
            .contains(&DegradedMode::CloudUnavailable)
    );
    assert!(
        degraded
            .active_degraded_modes
            .contains(&DegradedMode::ContainerUnavailable)
    );

    let exit_contract = degraded.exit_contract.as_ref().unwrap();
    // Exiting degraded mode without evidence or repo health check fails closed.
    let missing = evaluate_exit_degraded(exit_contract, &BTreeSet::new(), false).unwrap_err();
    assert!(missing.iter().any(|m| m == "REPO_HEALTH_CHECK_REQUIRED"));
    assert!(evaluate_exit_degraded(exit_contract, &exit_contract.required_evidence, true).is_ok());

    // Critical dependency loss stops in recoverable state (never masks failed proof gates).
    let stopped = classify_dependencies(&[DependencyStatus {
        name: "proof-gate".to_string(),
        kind: DependencyKind::ProofGate,
        criticality: DependencyCriticality::Critical,
        available: false,
        detail: "required gate failed".to_string(),
    }]);
    assert_eq!(stopped.state, OperationalState::StoppedRecoverable);

    // Resumable partial-write recovery.
    let marker = PartialWriteMarker {
        operation_id: "op-1".to_string(),
        target_path: ".decapod/governance.json".to_string(),
        staging_path: ".decapod/governance.json.tmp".to_string(),
        expected_sha256: "sha256:good".to_string(),
        phase: WritePhase::Synced,
    };
    assert_eq!(
        recover_partial_write(&marker, Some("sha256:good"), None),
        PartialWriteRecoveryAction::AlreadyCommittedIdempotent
    );
    assert_eq!(
        recover_partial_write(&marker, None, Some("sha256:good")),
        PartialWriteRecoveryAction::PromoteStagingToTarget
    );
    assert_eq!(
        recover_partial_write(&marker, None, Some("sha256:truncated")),
        PartialWriteRecoveryAction::RollbackIncompleteStaging
    );
}

// ============================================================================
// #859: Shared Principal Resource Budgets & Uncertain-Effect Holds
// ============================================================================

#[test]
fn houseboat_859_shared_budgets_6plus6_3x4_uncertain_holds_and_controls() {
    let mut ledger = ResourceBudgetLedger::new();
    ledger.register_policy(BudgetPolicy {
        principal_id: "repo:main".to_string(),
        unit: ResourceUnit::ComputeSeconds,
        principal_ceiling: 10,
        per_child_limit: Some(6),
        warning_threshold_pct: 80,
        hard_stop: true,
    });
    // Separate non-overlapping principal budget (control).
    ledger.register_policy(BudgetPolicy {
        principal_id: "repo:other".to_string(),
        unit: ResourceUnit::ComputeSeconds,
        principal_ceiling: 10,
        per_child_limit: Some(5),
        warning_threshold_pct: 80,
        hard_stop: true,
    });

    // Negative 1: 6 + 6 vs 10 -> exactly one durable winner.
    let r1 = ledger
        .reserve(
            "res-6a",
            "repo:main",
            "wu-1",
            "t-1",
            "exec",
            ResourceUnit::ComputeSeconds,
            6,
            100,
            200,
        )
        .unwrap();
    let r2_err = ledger
        .reserve(
            "res-6b",
            "repo:main",
            "wu-2",
            "t-2",
            "exec",
            ResourceUnit::ComputeSeconds,
            6,
            100,
            200,
        )
        .unwrap_err();
    assert!(r2_err.contains("BUDGET_PRINCIPAL_CEILING_EXCEEDED"));

    // Lost reply -> UncertainHold MUST NOT be auto-refunded even after expires_at_epoch!
    ledger
        .mark_effect_uncertain("res-6a", r1.fence_token, "network reply lost")
        .unwrap();
    let swept = ledger.sweep_expired_unused(
        999,
        &BTreeSet::from(["res-6a".to_string()]),
        "expiry-sweeper",
    );
    assert!(
        swept.is_empty(),
        "uncertain hold must never be auto-refunded on expiry"
    );
    assert_eq!(
        ledger.get_reservation("res-6a").unwrap().state,
        ReservationState::UncertainHold
    );

    // Stale fence token cannot release or settle!
    let stale_err = ledger
        .reconcile_outcome(
            "res-6a",
            r1.fence_token + 99,
            "stale-agent",
            OutcomeSettlement::ConfirmedNoEffect,
        )
        .unwrap_err();
    assert!(stale_err.contains("STALE_FENCE_TOKEN"));

    // Authoritative ConfirmedNoEffect releases cleanly; double-release is idempotent.
    ledger
        .reconcile_outcome(
            "res-6a",
            r1.fence_token,
            "authoritative-owner",
            OutcomeSettlement::ConfirmedNoEffect,
        )
        .unwrap();
    ledger
        .reconcile_outcome(
            "res-6a",
            r1.fence_token,
            "authoritative-owner",
            OutcomeSettlement::ConfirmedNoEffect,
        )
        .unwrap();

    // Negative 2: 3 children x 4 under 5-per-child & 10 shared -> 3rd fails!
    ledger.register_policy(BudgetPolicy {
        principal_id: "repo:three-kids".to_string(),
        unit: ResourceUnit::WorkunitSlots,
        principal_ceiling: 10,
        per_child_limit: Some(5),
        warning_threshold_pct: 70,
        hard_stop: true,
    });
    ledger
        .reserve(
            "k1",
            "repo:three-kids",
            "c1",
            "t1",
            "run",
            ResourceUnit::WorkunitSlots,
            4,
            100,
            300,
        )
        .unwrap();
    ledger
        .reserve(
            "k2",
            "repo:three-kids",
            "c2",
            "t2",
            "run",
            ResourceUnit::WorkunitSlots,
            4,
            100,
            300,
        )
        .unwrap();
    let k3_err = ledger
        .reserve(
            "k3",
            "repo:three-kids",
            "c3",
            "t3",
            "run",
            ResourceUnit::WorkunitSlots,
            4,
            100,
            300,
        )
        .unwrap_err();
    assert!(k3_err.contains("BUDGET_PRINCIPAL_CEILING_EXCEEDED"));

    // Control: 5 + 5 exact fit against 10 succeeds on independent budget.
    ledger
        .reserve(
            "fit-1",
            "repo:other",
            "o1",
            "t1",
            "run",
            ResourceUnit::ComputeSeconds,
            5,
            100,
            300,
        )
        .unwrap();
    ledger
        .reserve(
            "fit-2",
            "repo:other",
            "o2",
            "t2",
            "run",
            ResourceUnit::ComputeSeconds,
            5,
            100,
            300,
        )
        .unwrap();
    let fc = ledger
        .forecast("repo:other", ResourceUnit::ComputeSeconds)
        .unwrap();
    assert_eq!(fc.available_units, 0);
    assert_eq!(fc.escalation_level, EscalationLevel::Blocked);
}

// ============================================================================
// #1630: Storage Compaction Benchmark & Interrupted Migration/GC/Downgrade Recovery
// ============================================================================

#[test]
fn houseboat_1630_storage_compaction_benchmark_and_recovery_invariants() {
    let tmp = tempdir().unwrap();
    let report = run_local_storage_compaction_benchmark(tmp.path(), 8).unwrap();
    assert_eq!(report.workload_tasks, 8);
    assert_eq!(report.workload_operations, 24);
    assert_eq!(report.backend_scope, "local_sqlite_benchmark");
    assert_eq!(report.hosted_backend_status, "unavailable_local_only");
    assert!(report.before_compaction.main_db_bytes > 0);
    assert!(report.after_explicit_reclaim.main_db_bytes > 0);
    assert!(!report.unknown_fields.is_empty());

    // Backup/restore identity preservation + downgrade refusal.
    let db_path = tmp.path().join(".decapod/data/decapod.db");
    let backup_path = tmp.path().join(".decapod/data/decapod.backup.db");
    fs::copy(&db_path, &backup_path).unwrap();

    let conn = db::db_connect(&backup_path.to_string_lossy()).unwrap();
    storage_compaction::stamp_storage_format_metadata(&conn).unwrap();
    // Simulate older reader and writer binaries opening a v2 store -> fail closed!
    for read_only in [true, false] {
        let downgrade_err = storage_compaction::check_storage_format_compatibility_for_version(
            &conn, "0.100.0", read_only,
        )
        .unwrap_err();
        assert!(
            downgrade_err
                .to_string()
                .contains("STORAGE_FORMAT_INCOMPATIBLE")
        );
    }

    // Interrupted migration simulation: uncommitted transaction rollback preserves rows & idempotent retry succeeds.
    let task_count_before: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    conn.execute("BEGIN IMMEDIATE", []).unwrap();
    conn.execute("ROLLBACK", []).unwrap();
    let retry_gc = storage_compaction::gc_unreferenced_components(&conn, false).unwrap();
    let task_count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(task_count_before, task_count_after);
    assert_eq!(retry_gc.deleted_components, 0);
}

// ============================================================================
// #657 & #1367: Knowledge Write-Time Attestation & Cross-Crash Reconciliation
// ============================================================================

#[test]
fn houseboat_657_and_1367_knowledge_attestation_and_crash_reconciliation() {
    // #657: secret material rejected at write-time.
    let secret_err = KnowledgeAttestation::create_attested(
        "k-secret",
        KnowledgeClass::ProjectContext,
        KnowledgeSourceKind::HumanInput,
        "issue:#657",
        "Use key ghp_123456789012345678901234567890123456 for prod",
        "agent",
        100,
        false,
        true,
    )
    .unwrap_err();
    assert!(secret_err.contains("KNOWLEDGE_SECRET_REJECTED"));

    let mut episodic = KnowledgeAttestation::create_attested(
        "k-ep-1",
        KnowledgeClass::EpisodicLesson,
        KnowledgeSourceKind::IssueOrPr,
        "pr:#1864",
        "Never skip cargo fmt before validating a PR.",
        "agent",
        100,
        true,
        false,
    )
    .unwrap();
    assert!(episodic.secret_scan_passed);
    assert!(
        episodic
            .promote_to_doctrine("agent", vec!["proof:1".to_string()])
            .is_err()
    );
    episodic.verified_by_human = true;
    episodic
        .promote_to_doctrine("maintainer:alex", vec!["proof:pr-1864".to_string()])
        .unwrap();
    assert!(episodic.promoted_to_doctrine);

    // #1367: cross-crash reconciliation refuses stale repo SHA, changed instruction, or unknown outcome.
    let rec = InterruptedExecutionRecord {
        workunit_id: "wu-crash".to_string(),
        task_id: "t-crash".to_string(),
        instruction_revision: "sha256:instr-v1".to_string(),
        repo_head_sha: "commit-aaa".to_string(),
        checkpoint_id: "cp-7".to_string(),
        pending_operation_id: Some("op-push-1".to_string()),
        unresolved_contradictions: vec![],
        model_hop_history: vec!["model-a".to_string(), "model-b".to_string()],
    };
    assert_eq!(
        reconcile_after_crash(&rec, "sha256:instr-v2", "commit-aaa", Some(true)).verdict,
        CrashRecoveryVerdict::RefusedInstructionChanged
    );
    assert_eq!(
        reconcile_after_crash(&rec, "sha256:instr-v1", "commit-bbb", Some(true)).verdict,
        CrashRecoveryVerdict::RefusedStaleRepoState
    );
    assert_eq!(
        reconcile_after_crash(&rec, "sha256:instr-v1", "commit-aaa", None).verdict,
        CrashRecoveryVerdict::RefusedUnknownOutcomePending
    );
    let safe = reconcile_after_crash(&rec, "sha256:instr-v1", "commit-aaa", Some(true));
    assert!(safe.can_resume);
    assert_eq!(safe.verdict, CrashRecoveryVerdict::SafeToResume);
}

// ============================================================================
// #1024: Offline Multi-Provider / Non-Rust / Monorepo / Version-Skew Init Matrix
// ============================================================================

#[test]
fn houseboat_1024_offline_init_reconciliation_matrix() {
    // 1. Non-Rust Python + Node monorepo with multi-provider CI (GitHub Actions + Buildkite + GitLab CI).
    let mono = tempdir().unwrap();
    fs::write(
        mono.path().join("pyproject.toml"),
        "[project]\nname = \"py-svc\"\n",
    )
    .unwrap();
    fs::write(
        mono.path().join("package.json"),
        "{\"name\":\"web-app\",\"workspaces\":[\"packages/*\"]}\n",
    )
    .unwrap();
    fs::create_dir_all(mono.path().join("packages/api")).unwrap();
    fs::write(
        mono.path().join("packages/api/go.mod"),
        "module example.com/api\n\ngo 1.23\n",
    )
    .unwrap();
    fs::create_dir_all(mono.path().join(".github/workflows")).unwrap();
    fs::write(
        mono.path().join(".github/workflows/ci.yml"),
        "name: CI\njobs: {}\n",
    )
    .unwrap();
    fs::create_dir_all(mono.path().join(".buildkite")).unwrap();
    fs::write(mono.path().join(".buildkite/pipeline.yml"), "steps: []\n").unwrap();
    fs::write(mono.path().join(".gitlab-ci.yml"), "stages: [test]\n").unwrap();

    let init_out = run_decapod(
        mono.path(),
        &[
            "init",
            "--proof",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
    );
    assert!(
        init_out.status.success(),
        "monorepo init failed: {}",
        String::from_utf8_lossy(&init_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&init_out.stdout);
    assert!(stdout.contains("merge-ready: false"), "{stdout}");

    // 2. Version-skew config preservation on re-init.
    let cfg_path = mono.path().join(".decapod/config.toml");
    let mut cfg: toml::Value = toml::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    cfg.as_table_mut().unwrap().insert(
        "custom_future_field".to_string(),
        toml::Value::String("preserve-me".to_string()),
    );
    fs::write(&cfg_path, toml::to_string_pretty(&cfg).unwrap()).unwrap();

    let reinit = run_decapod(
        mono.path(),
        &[
            "init",
            "--proof",
            "--force",
            "--no-ci",
            "--no-specs",
            "--no-container-workspaces",
        ],
    );
    assert!(
        reinit.status.success(),
        "version-skew reinit failed: {}",
        String::from_utf8_lossy(&reinit.stderr)
    );
    let after: toml::Value = toml::from_str(&fs::read_to_string(&cfg_path).unwrap()).unwrap();
    assert_eq!(
        after
            .get("custom_future_field")
            .and_then(toml::Value::as_str),
        Some("preserve-me"),
        "unknown authored/future config fields must be preserved across version-skew re-init"
    );
}

// ============================================================================
// #1711: Darkmatter Read-Only Snapshot Fixture & CLI Verification
// ============================================================================

#[test]
fn houseboat_1711_darkmatter_readonly_snapshot_fixture_and_cli() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/darkmatter_readonly_snapshot.v1.json");
    let raw = fs::read_to_string(&fixture_path).expect("read darkmatter snapshot fixture");
    let mut snapshot: DarkmatterReadonlySnapshot =
        serde_json::from_str(&raw).expect("parse darkmatter snapshot");
    validate_darkmatter_snapshot(&snapshot).expect("canonical fixture must validate");

    // Negative controls: mutation flag or path escape fails closed.
    snapshot.mutation_endpoints_exposed = true;
    snapshot.relevant_files.push("../../etc/passwd".to_string());
    let errs = validate_darkmatter_snapshot(&snapshot).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.contains("DARKMATTER_BOUNDARY_VIOLATION"))
    );
    assert!(errs.iter().any(|e| e.contains("PATH_ESCAPE")));

    // CLI verification against canonical fixture.
    let tmp = tempdir().unwrap();
    let cli = run_decapod(
        tmp.path(),
        &[
            "houseboat",
            "verify-darkmatter-snapshot",
            "--file",
            fixture_path.to_str().unwrap(),
        ],
    );
    assert!(
        cli.status.success(),
        "cli verify-darkmatter-snapshot failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let parsed: serde_json::Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["read_only"], true);
}

// ============================================================================
// #1763: 3-Worktree Combined-Candidate Negative Control (Alpha + Beta + Gamma)
// ============================================================================

#[test]
fn houseboat_1763_three_worktree_combined_candidate_negative_control() {
    // Real git repo with 3 isolated worktrees (`wt-alpha`, `wt-beta`, `wt-gamma`).
    // Shared semantic invariant: ALPHA + BETA + GAMMA <= 2.
    // Each worktree sets its own delta to 1:
    // - Individually: 1 <= 2 (PASS)
    // - Any pair: 2 <= 2 (PASS)
    // - All three combined: 3 > 2 (FAIL until reconciled!)
    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).unwrap();

    let git = |cwd: &Path, args: &[&str]| {
        let out = Command::new("git")
            .current_dir(cwd)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Houseboat Test")
            .env("GIT_AUTHOR_EMAIL", "houseboat@example.invalid")
            .env("GIT_COMMITTER_NAME", "Houseboat Test")
            .env("GIT_COMMITTER_EMAIL", "houseboat@example.invalid")
            .output()
            .expect("git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };

    git(&repo, &["init", "-b", "main"]);
    fs::write(
        repo.join("shared_budget.txt"),
        "ALPHA=0\n\n\nBETA=0\n\n\nGAMMA=0\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline shared budget"]);
    let base_sha = git(&repo, &["rev-parse", "HEAD"]);

    // Register 3 overlapping exclusive claims in canonical todo store.
    let store = repo.join(".decapod/data");
    fs::create_dir_all(&store).unwrap();
    todo::initialize_todo_db(&store).unwrap();
    let mut task_ids = Vec::new();
    for agent in ["alpha", "beta", "gamma"] {
        let added = todo::add_task(
            &store,
            &TodoCommand::Add {
                title: format!("Worktree slice {agent}"),
                description: String::new(),
                tags: String::new(),
                owner: String::new(),
                due: None,
                r#ref: String::new(),
                scope: r#"paths:["shared_budget.txt"]"#.to_string(),
                dir: Some(repo.to_string_lossy().to_string()),
                priority: "medium".to_string(),
                depends_on: String::new(),
                blocks: String::new(),
                parent: None,
                one_shot: 0,
            },
        )
        .unwrap();
        let tid = added["id"].as_str().unwrap().to_string();
        let claim = todo::claim_task(&store, &tid, agent, ClaimMode::Exclusive).unwrap();
        assert_eq!(claim["status"], "ok");
        task_ids.push(tid);
    }

    // Create 3 isolated worktrees and apply disjoint-line edits that cleanly merge textually.
    let mut heads = BTreeMap::new();
    for (idx, (agent, find, replace)) in [
        ("alpha", "ALPHA=0", "ALPHA=1"),
        ("beta", "BETA=0", "BETA=1"),
        ("gamma", "GAMMA=0", "GAMMA=1"),
    ]
    .into_iter()
    .enumerate()
    {
        let wt = tmp.path().join(format!("wt-{agent}"));
        let branch = format!("agent/{agent}/slice-{idx}");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-b",
                &branch,
                wt.to_str().unwrap(),
                "main",
            ],
        );
        let content = fs::read_to_string(wt.join("shared_budget.txt")).unwrap();
        fs::write(wt.join("shared_budget.txt"), content.replace(find, replace)).unwrap();
        git(&wt, &["add", "shared_budget.txt"]);
        git(&wt, &["commit", "-m", &format!("{agent} delta=1")]);
        heads.insert(agent.to_string(), git(&wt, &["rev-parse", "HEAD"]));
    }

    // Merge all 3 branches into an integration worktree cleanly (zero git textual conflicts).
    let wt_combined = tmp.path().join("wt-combined");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "integration/3way",
            wt_combined.to_str().unwrap(),
            "main",
        ],
    );
    git(&wt_combined, &["merge", "--no-edit", "agent/alpha/slice-0"]);
    git(&wt_combined, &["merge", "--no-edit", "agent/beta/slice-1"]);
    git(&wt_combined, &["merge", "--no-edit", "agent/gamma/slice-2"]);

    let combined_text = fs::read_to_string(wt_combined.join("shared_budget.txt")).unwrap();
    let sum: u32 = combined_text
        .lines()
        .filter_map(|l| l.split_once('=').and_then(|(_, v)| v.parse::<u32>().ok()))
        .sum();
    assert_eq!(
        sum, 3,
        "clean 3-way textual merge violates ALPHA+BETA+GAMMA <= 2 invariant"
    );

    // FleetCoordGraph blocks 3-worktree combined admission when combined proof is missing or failed.
    let mut graph = FleetCoordGraph::new();
    for (idx, agent) in ["alpha", "beta", "gamma"].into_iter().enumerate() {
        graph
            .add_node(WorkunitNode {
                workunit_id: format!("wu-{agent}"),
                task_id: task_ids[idx].clone(),
                branch: format!("agent/{agent}/slice-{idx}"),
                head_sha: heads[agent].clone(),
                base_sha: base_sha.clone(),
                touched_paths: BTreeSet::from(["shared_budget.txt".to_string()]),
                trajectory_verified: true,
                proof_refs: vec![format!("proof:{agent}")],
                capacity_units: 1,
            })
            .unwrap();
    }

    let failed_3way = CombinedReconciliationProof {
        proof_id: "proof:3way-combined-fail".to_string(),
        accepted_target_sha: base_sha.clone(),
        combined_tree_sha: git(&wt_combined, &["rev-parse", "HEAD^{tree}"]),
        participating_heads: BTreeMap::from([
            ("wu-alpha".to_string(), heads["alpha"].clone()),
            ("wu-beta".to_string(), heads["beta"].clone()),
            ("wu-gamma".to_string(), heads["gamma"].clone()),
        ]),
        passed: sum <= 2,
    };
    let verdict = graph.evaluate_combined_merge_gate(
        &["wu-alpha", "wu-beta", "wu-gamma"],
        &base_sha,
        Some(&failed_3way),
    );
    assert!(!verdict.allowed);
    assert_eq!(verdict.overlapping_paths["shared_budget.txt"].len(), 3);
    assert!(
        verdict
            .blockers
            .iter()
            .any(|b| b.contains("COMBINED_RECONCILIATION_FAILED"))
    );
}

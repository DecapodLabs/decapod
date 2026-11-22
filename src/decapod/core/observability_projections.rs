//! Strictly read-only, bounded, versioned observability projections and
//! optional authenticated loopback HTTP server (#860, #874, #1712-#1714,
//! #1719-#1720, #1723, #1727-#1728, #1730-#1732).
//!
//! Boundary invariant: Decapod owns authoritative state, safe read contracts,
//! snapshot consistency, redaction, and cryptographic verification. Zero UI
//! logic or display-specific state machines live here. Every projection marks
//! `read_only = true` and `authoritative = false`, never mutates repository or
//! SQLite state, and preserves unknown/stale/not_proven uncertainty explicitly.

pub mod pr_manifest;
pub mod provenance;
pub mod server;
pub mod snapshot;
pub mod specs;
pub mod timeline;
pub mod validation_plan;
pub mod workspace;

pub use pr_manifest::{
    PrCorrespondenceReport, PrCorrespondenceState, PrDriftFlag, PrManifestProjection,
    PrManifestQuery, ProspectivePrManifest, PublishedPrInput, PublishedPrRecord,
    project_pr_manifest,
};
pub use provenance::{
    ConsultationResponseMetadata, ContextProvenanceProjection, ContextProvenanceQuery,
    IntentProvenanceSummary, ProvenancePagination, RedactedConsultationRecord,
    project_context_provenance, redact_provenance_text,
};
pub use server::{
    ALLOWLISTED_OBSERVE_RPC_OPS, ObserveServeOptions, ReadProjectionAccessPolicy,
    dispatch_observe_http_request, dispatch_observe_rpc_op, serve_observe_http,
    serve_observe_on_listener,
};
pub use snapshot::{
    AnchorFreshness, DEFAULT_MAX_ANCHOR_BYTES, DEFAULT_MAX_INVENTORY_FILES, HARD_MAX_ANCHOR_BYTES,
    HARD_MAX_INVENTORY_FILES, LiveWorkspaceInventory, OBSERVE_SCHEMA_VERSION, ObservationSnapshot,
    PinnedBaseline, RepositoryIdentity, SnapshotConsistency, SourceAnchor, SqliteRevisionAnchor,
    TaskRecordView, WorkspaceFileEntry, WorktreeIdentity, capture_observation_with,
    compute_repo_id, enumerate_live_workspace_inventory, is_generated_artifact_path,
    is_secret_sensitive_path, read_source_anchor, read_tasks_readonly, resolve_data_root_readonly,
    validate_repo_bounded_path, verify_anchor_freshness,
};
pub use specs::{
    ClaimProjection, GovernanceStateProjection, IntentLineageProjection, MissingObligation,
    PaginationMetadata, PlanSummaryProjection, ReceiptIntegrityProjection, SpecFileProjection,
    SpecSecurityFlags, SpecsGovernanceProjection, SpecsManifestProjection, SpecsProjectionQuery,
    TraversalEdge, inspect_spec_text_security, project_specs_and_governance,
};
pub use timeline::{
    ActivityTimelineProjection, ObjectiveEvidenceEdge, ProofApplicability, TimelineCursor,
    TimelineEntry, TimelineIntegrityFinding, TimelineProjectionQuery, TrustClass,
    project_activity_timeline,
};
pub use validation_plan::{
    ApplicableRuleProjection, CandidateIdentity, ConstraintConsistencyReport,
    ConstraintContradiction, ConstraintRelation, ConstraintRelationKind, EvidenceFreshness,
    JointCheckProjection, JointCheckSpec, OverallReadinessVerdict, PlannedCheckProjection,
    RequirementCoverageEntry, ValidationPlanProjection, ValidationPlanQuery,
    evaluate_constraint_relations, project_validation_plan,
};
pub use workspace::{
    ClaimedEffectRecord, FleetObservabilitySummary, StatusAuthorityClass, WorkspaceEvolutionRecord,
    WorkspaceObservationProjection, WorkspaceProjectionQuery, WorkunitActivityCategory,
    WorkunitClassificationInput, WorkunitStatusProjection, classify_workunit_activity,
    project_workspace_observation, render_workspace_human,
};

/// Concise human-readable renderers driven by the exact same canonical structs
/// as JSON/RPC output (#860).
pub fn render_specs_human(proj: &SpecsGovernanceProjection) -> String {
    format!(
        "Decapod Specs & Governance ({})\n  snapshot_id: {} (consistent={})\n  specs: {}/{} present | manifest_valid={} | governance_valid={}\n  original_intent: {}\n  refined_intent: {}\n  claims: {} | missing_obligations: {} | receipt_valid={} stale_vs_head={}\n",
        proj.schema_version,
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.specs_tree.iter().filter(|s| s.present).count(),
        proj.specs_pagination.total,
        proj.manifest.valid,
        proj.governance.valid,
        proj.governance
            .intent_lineage
            .original_intent
            .as_deref()
            .unwrap_or("<absent>"),
        proj.governance
            .intent_lineage
            .refined_intent
            .as_deref()
            .unwrap_or("<absent>"),
        proj.governance.claims.len(),
        proj.missing_obligations.len(),
        proj.governance.validation_receipt.integrity_valid,
        proj.governance.validation_receipt.stale_vs_head,
    )
}

pub fn render_timeline_human(proj: &ActivityTimelineProjection) -> String {
    format!(
        "Decapod Activity Timeline ({})\n  snapshot_id: {} (consistent={})\n  history_intact: {} | witnessed_path_present: {} | witnessed={} declared_only={}\n  entries: {} | integrity_findings: {} | cursor: after={} next={} has_more={}\n",
        proj.schema_version,
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.history_intact,
        proj.witnessed_path_present,
        proj.witnessed_count,
        proj.declared_only_count,
        proj.entries.len(),
        proj.integrity_findings.len(),
        proj.cursor.after_seq,
        proj.cursor.next_seq,
        proj.cursor.has_more,
    )
}

pub fn render_validation_plan_human(proj: &ValidationPlanProjection) -> String {
    format!(
        "Decapod Validation Plan ({})\n  snapshot_id: {} (consistent={})\n  overall_readiness: {:?} | semantic_assurance: {} | dirty={}\n  rules: {} | requirements: {} | local_checks: {} | joint_checks: {} | relation_consistency: {}\n  blockers: {}\n",
        proj.schema_version,
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.overall_readiness,
        proj.semantic_assurance_status,
        proj.candidate_identity.working_tree_dirty,
        proj.applicable_rules.len(),
        proj.requirement_coverage.len(),
        proj.local_checks.len(),
        proj.joint_integration_checks.len(),
        proj.relation_consistency.status,
        if proj.blockers.is_empty() {
            "none".to_string()
        } else {
            proj.blockers.join(", ")
        },
    )
}

pub fn render_pr_manifest_human(proj: &PrManifestProjection) -> String {
    format!(
        "Decapod PR Manifest ({})\n  snapshot_id: {} (consistent={})\n  correspondence: {:?} | plan_absent={} | plan_is_not_published={}\n  drift_flags: {}\n",
        proj.schema_version,
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.correspondence.state,
        proj.correspondence.plan_absent,
        proj.correspondence.plan_is_not_published,
        if proj.correspondence.drift_flags.is_empty() {
            "none".to_string()
        } else {
            proj.correspondence
                .drift_flags
                .iter()
                .map(|f| format!("{f:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        },
    )
}

pub fn render_context_provenance_human(proj: &ContextProvenanceProjection) -> String {
    format!(
        "Decapod Context Provenance ({})\n  snapshot_id: {} (consistent={})\n  intent_id: {} | origin_trust: {} | consultations: {}/{}\n",
        proj.schema_version,
        proj.snapshot.snapshot_id,
        proj.snapshot.consistency.consistent,
        proj.intent_context
            .intent_id
            .as_deref()
            .unwrap_or("<absent>"),
        proj.intent_context.origin_trust,
        proj.pagination.returned,
        proj.pagination.total,
    )
}

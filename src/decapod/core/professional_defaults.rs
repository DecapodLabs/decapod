//! Outcome-implied professional responsibility derivation, behavior-relevant
//! evidence admission, and authorized delivery endpoint resolution.
//!
//! Covers:
//! - `#1981`: Infer ordinary professional responsibilities from a short outcome request
//! - `#1982`: Require outcome-relevant tests and evidence for inferred professional duties
//! - `#1983`: Carry short outcome requests through authorized commit, push, and merge
//! - `#1367`: Durable continuation recovery of derived duties and intended endpoint

use crate::core::obligation_ledger::{
    MainMergeAuthorizationDecision, MainMergeAuthorizationInput, evaluate_main_merge_authorization,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROFESSIONAL_DEFAULTS_SCHEMA: &str = "decapod.professional-defaults.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProfessionalFunction {
    ProductResearch,
    DesignUx,
    EngineeringQa,
    SecurityPrivacyDataOps,
    DocsSupport,
    CommercialOrg,
}

impl ProfessionalFunction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProductResearch => "product_research",
            Self::DesignUx => "design_ux",
            Self::EngineeringQa => "engineering_qa",
            Self::SecurityPrivacyDataOps => "security_privacy_data_ops",
            Self::DocsSupport => "docs_support",
            Self::CommercialOrg => "commercial_org",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DutyOrigin {
    ExplicitHumanRequest,
    InferredProfessionalMeans,
    AcceptedBaselineRule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DutyEvidenceKind {
    BehaviorRegressionTest,
    FailingBaselineControl,
    InteractiveObservation,
    DocsAlignmentCheck,
    SourceBackedBusinessArtifact,
    ProportionateValidation,
    DeliveryReadback,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DutyDisposition {
    Pending,
    Planned,
    CheckExecuted,
    Satisfied,
    Blocked,
    NonApplicableWithRationale,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskOutcomeProfile {
    InteractiveUx,
    CustomerCommercial,
    EngineeringInterface,
    ProportionateProse,
    NonCodeBusiness,
    AnalysisOrDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedProfessionalDuty {
    pub duty_id: String,
    pub function: ProfessionalFunction,
    pub origin: DutyOrigin,
    pub originating_outcome: String,
    pub source_revision: String,
    pub policy_revision: String,
    pub applicability_rationale: String,
    pub acceptance_condition: String,
    pub evidence_kind: DutyEvidenceKind,
    pub owner: String,
    pub required: bool,
    pub disposition: DutyDisposition,
    pub represented_in_plan: bool,
    pub check_executed: bool,
    pub check_passed: bool,
    pub requested_behavior_covered: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_ref: Option<String>,
    #[serde(default)]
    pub observed_execution: bool,
    #[serde(default)]
    pub negative_control_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collected_test_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclusion_rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryEndpointKind {
    AnalysisOnly,
    DraftOnly,
    CommitOnly,
    PushOnly,
    PullRequest,
    DirectMerge,
    PullRequestThenMerge,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryPhaseStatus {
    Planned,
    Validated,
    Pushed,
    PrOpened,
    QueuedForMerge,
    MergeRequested,
    UnknownPendingReadback,
    Merged,
    StoppedMissingAuthority,
    Blocked,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryResolutionInput {
    pub governing_request: String,
    pub accepted_main_merge: bool,
    pub candidate_main_merge: bool,
    pub requires_pr_workflow: bool,
    pub standing_pr_merge_delegated: bool,
    pub requires_review: bool,
    pub requires_ci_checks: bool,
    pub policy_revision: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedDeliveryContract {
    pub endpoint: DeliveryEndpointKind,
    pub standing_delegation_source: String,
    pub accepted_main_merge: bool,
    pub candidate_attempted_elevation: bool,
    pub requires_pr_workflow: bool,
    pub requires_review: bool,
    pub requires_ci_checks: bool,
    pub explicit_narrowing: Option<String>,
    pub remaining_steps: Vec<String>,
    pub policy_revision: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DutyCompactSummary {
    pub total_duties: usize,
    pub required_duties: usize,
    pub satisfied_duties: usize,
    pub functions_covered: Vec<String>,
    pub duty_ids: Vec<String>,
    pub delivery_endpoint: DeliveryEndpointKind,
    pub coverage_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutcomeDerivationInput {
    pub original_request: Option<String>,
    pub task_title: Option<String>,
    pub task_description: Option<String>,
    pub clarifications: Vec<String>,
    pub project_intent: Option<String>,
    pub affected_paths: Vec<String>,
    pub source_revision: String,
    pub owner: String,
    pub delivery_policy: DeliveryResolutionInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DerivedProfessionalContract {
    pub schema: String,
    pub full_governing_request: String,
    pub profile: TaskOutcomeProfile,
    pub duties: Vec<DerivedProfessionalDuty>,
    pub delivery_endpoint: ResolvedDeliveryContract,
    pub relevant_areas: Vec<String>,
    pub proof_required: Vec<String>,
    pub constraints: Vec<String>,
    pub allowed_scope: Vec<String>,
    pub summary: DutyCompactSummary,
}

fn sha256_str(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Combine original request, task title, task description, and accepted clarifications
/// into one lossless governing request without truncating to a short task title.
pub fn compose_governing_request(
    original_request: Option<&str>,
    task_title: Option<&str>,
    task_description: Option<&str>,
    clarifications: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in [original_request, task_title, task_description]
        .into_iter()
        .flatten()
    {
        let trimmed = item.trim();
        if !trimmed.is_empty() && !parts.iter().any(|existing| existing == trimmed) {
            parts.push(trimmed.to_string());
        }
    }
    for clar in clarifications {
        let trimmed = clar.trim();
        if !trimmed.is_empty() && !parts.iter().any(|existing| existing == trimmed) {
            parts.push(format!("Clarification: {trimmed}"));
        }
    }
    if parts.is_empty() {
        "Unspecified outcome request".to_string()
    } else {
        parts.join(" | ")
    }
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

/// Classify the task profile from domain context and discovered surfaces without
/// gating ordinary engineering work on literal prompt keywords like "fix"/"test".
pub fn classify_task_profile(
    request_lower: &str,
    affected_paths: &[String],
    project_intent: Option<&str>,
) -> TaskOutcomeProfile {
    if has_any(
        request_lower,
        &[
            "analysis only",
            "analysis-only",
            "read-only",
            "readonly",
            "report only",
            "draft only",
            "draft-only",
            "cancel",
        ],
    ) {
        return TaskOutcomeProfile::AnalysisOrDraft;
    }

    let paths_joined = affected_paths.join(" ").to_lowercase();
    let only_prose_paths = !affected_paths.is_empty()
        && affected_paths.iter().all(|p| {
            let l = p.to_lowercase();
            l.ends_with(".md") || l.ends_with(".txt") || l.contains("readme")
        });

    if (only_prose_paths
        || has_any(
            request_lower,
            &["typo", "spelling", "grammar", "punctuation", "wording"],
        ))
        && !has_any(
            request_lower,
            &[
                "pricing", "billing", "policy", "contract", "ux", "tui", "cli",
            ],
        )
    {
        return TaskOutcomeProfile::ProportionateProse;
    }

    if has_any(
        request_lower,
        &[
            "pricing memo",
            "pricing summary",
            "investor faq",
            "investor update",
            "board deck",
            "compliance brief",
            "marketing brief",
            "launch plan",
            "customer memo",
            "non-code",
            "policy memo",
            "sales deck",
        ],
    ) && affected_paths.iter().all(|p| {
        let l = p.to_lowercase();
        !l.ends_with(".rs") && !l.ends_with(".ts") && !l.ends_with(".go")
    }) {
        return TaskOutcomeProfile::NonCodeBusiness;
    }

    if has_any(
        request_lower,
        &[
            "paid",
            "signup",
            "sign-up",
            "billing",
            "entitlement",
            "subscription",
            "checkout",
            "team onboarding",
            "tenant",
        ],
    ) {
        return TaskOutcomeProfile::CustomerCommercial;
    }

    let project_lower = project_intent.unwrap_or_default().to_lowercase();
    if has_any(
        request_lower,
        &[
            "tui",
            "ux",
            "ui",
            "layout",
            "panel",
            "composer",
            "keyboard",
            "discoverab",
            "navigation",
            "usability",
            "accessibility",
            "interaction",
        ],
    ) || has_any(&paths_joined, &["tui", "ui/", "ux", "frontend"])
        || has_any(&project_lower, &["tui", "terminal interface"])
    {
        return TaskOutcomeProfile::InteractiveUx;
    }

    TaskOutcomeProfile::EngineeringInterface
}

/// Resolve the intended delivery endpoint from accepted standing policy and explicit
/// user narrowing without requiring "commit", "push", or "merge" verbs.
pub fn resolve_delivery_endpoint(input: &DeliveryResolutionInput) -> ResolvedDeliveryContract {
    let req_lower = input.governing_request.to_lowercase();
    let candidate_attempted_elevation = !input.accepted_main_merge && input.candidate_main_merge;

    if req_lower.contains("cancel") && !req_lower.contains("cancellation test") {
        return ResolvedDeliveryContract {
            endpoint: DeliveryEndpointKind::Cancelled,
            standing_delegation_source: "explicit_user_cancellation".to_string(),
            accepted_main_merge: input.accepted_main_merge,
            candidate_attempted_elevation,
            requires_pr_workflow: input.requires_pr_workflow,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: Some("explicit_cancellation".to_string()),
            remaining_steps: vec!["stop_and_record_cancellation".to_string()],
            policy_revision: input.policy_revision.clone(),
            rationale: "Explicit cancellation stops execution before mutation or delivery."
                .to_string(),
        };
    }

    if has_any(
        &req_lower,
        &[
            "analysis only",
            "analysis-only",
            "report only",
            "investigate only",
        ],
    ) {
        return ResolvedDeliveryContract {
            endpoint: DeliveryEndpointKind::AnalysisOnly,
            standing_delegation_source: "explicit_user_narrowing".to_string(),
            accepted_main_merge: input.accepted_main_merge,
            candidate_attempted_elevation,
            requires_pr_workflow: input.requires_pr_workflow,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: Some("analysis_only".to_string()),
            remaining_steps: vec!["deliver_source_grounded_analysis".to_string()],
            policy_revision: input.policy_revision.clone(),
            rationale: "Explicit analysis-only instruction overrides default change delivery."
                .to_string(),
        };
    }

    if has_any(&req_lower, &["draft only", "draft-only", "draft pr only"]) {
        return ResolvedDeliveryContract {
            endpoint: DeliveryEndpointKind::DraftOnly,
            standing_delegation_source: "explicit_user_narrowing".to_string(),
            accepted_main_merge: input.accepted_main_merge,
            candidate_attempted_elevation,
            requires_pr_workflow: true,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: Some("draft_only".to_string()),
            remaining_steps: vec![
                "verify_behavior_and_docs".to_string(),
                "commit_and_push_branch".to_string(),
                "open_or_update_draft_pr".to_string(),
            ],
            policy_revision: input.policy_revision.clone(),
            rationale: "Explicit draft-only instruction halts after draft publication.".to_string(),
        };
    }

    if candidate_attempted_elevation {
        return ResolvedDeliveryContract {
            endpoint: if input.requires_pr_workflow {
                DeliveryEndpointKind::PullRequest
            } else {
                DeliveryEndpointKind::PushOnly
            },
            standing_delegation_source: "accepted_base_policy_enforced".to_string(),
            accepted_main_merge: false,
            candidate_attempted_elevation: true,
            requires_pr_workflow: input.requires_pr_workflow,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: None,
            remaining_steps: vec![
                "verify_behavior_and_docs".to_string(),
                "commit_and_push_branch".to_string(),
                "open_pr_and_report_missing_merge_authority".to_string(),
            ],
            policy_revision: input.policy_revision.clone(),
            rationale: "Candidate branch attempted governance.main_merge self-authorization; merge remains blocked under accepted policy."
                .to_string(),
        };
    }

    if input.requires_pr_workflow {
        if input.standing_pr_merge_delegated {
            return ResolvedDeliveryContract {
                endpoint: DeliveryEndpointKind::PullRequestThenMerge,
                standing_delegation_source: "accepted_repo_pr_merge_delegation".to_string(),
                accepted_main_merge: input.accepted_main_merge,
                candidate_attempted_elevation: false,
                requires_pr_workflow: true,
                requires_review: input.requires_review,
                requires_ci_checks: input.requires_ci_checks,
                explicit_narrowing: None,
                remaining_steps: vec![
                    "verify_behavior_and_docs".to_string(),
                    "decapod_validate".to_string(),
                    "commit_and_push_branch".to_string(),
                    "open_or_reuse_pr".to_string(),
                    "observe_required_checks_and_reviews".to_string(),
                    "merge_pr_and_read_back_target".to_string(),
                ],
                policy_revision: input.policy_revision.clone(),
                rationale: "Repository requires PR workflow and standing delegation authorizes PR completion through merge once checks and reviews pass."
                    .to_string(),
            };
        }
        return ResolvedDeliveryContract {
            endpoint: DeliveryEndpointKind::PullRequest,
            standing_delegation_source: "accepted_repo_pr_workflow".to_string(),
            accepted_main_merge: input.accepted_main_merge,
            candidate_attempted_elevation: false,
            requires_pr_workflow: true,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: None,
            remaining_steps: vec![
                "verify_behavior_and_docs".to_string(),
                "decapod_validate".to_string(),
                "commit_and_push_branch".to_string(),
                "open_or_reuse_pr".to_string(),
                "await_human_pr_merge_authority".to_string(),
            ],
            policy_revision: input.policy_revision.clone(),
            rationale: "Accepted policy requires PR workflow without autonomous merge delegation; advance through validated PR without extra prompts."
                .to_string(),
        };
    }

    if input.accepted_main_merge {
        return ResolvedDeliveryContract {
            endpoint: DeliveryEndpointKind::DirectMerge,
            standing_delegation_source: "accepted_governance_main_merge_opt_in".to_string(),
            accepted_main_merge: true,
            candidate_attempted_elevation: false,
            requires_pr_workflow: false,
            requires_review: input.requires_review,
            requires_ci_checks: input.requires_ci_checks,
            explicit_narrowing: None,
            remaining_steps: vec![
                "verify_behavior_and_docs".to_string(),
                "decapod_validate".to_string(),
                "commit_candidate".to_string(),
                "direct_merge_to_accepted_base".to_string(),
                "read_back_accepted_target".to_string(),
            ],
            policy_revision: input.policy_revision.clone(),
            rationale: "Accepted base enables governance.main_merge = true; direct merge proceeds after exact-candidate validation and freshness check."
                .to_string(),
        };
    }

    ResolvedDeliveryContract {
        endpoint: DeliveryEndpointKind::PushOnly,
        standing_delegation_source: "accepted_default_fail_closed".to_string(),
        accepted_main_merge: false,
        candidate_attempted_elevation: false,
        requires_pr_workflow: false,
        requires_review: input.requires_review,
        requires_ci_checks: input.requires_ci_checks,
        explicit_narrowing: None,
        remaining_steps: vec![
            "verify_behavior_and_docs".to_string(),
            "decapod_validate".to_string(),
            "commit_and_push_branch".to_string(),
            "report_remaining_merge_authority".to_string(),
        ],
        policy_revision: input.policy_revision.clone(),
        rationale:
            "Accepted default main_merge = false withholds autonomous merge after branch push."
                .to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_duty(
    duty_id: &str,
    function: ProfessionalFunction,
    origin: DutyOrigin,
    outcome: &str,
    source_revision: &str,
    policy_revision: &str,
    applicability_rationale: &str,
    acceptance_condition: &str,
    evidence_kind: DutyEvidenceKind,
    owner: &str,
) -> DerivedProfessionalDuty {
    DerivedProfessionalDuty {
        duty_id: duty_id.to_string(),
        function,
        origin,
        originating_outcome: outcome.to_string(),
        source_revision: source_revision.to_string(),
        policy_revision: policy_revision.to_string(),
        applicability_rationale: applicability_rationale.to_string(),
        acceptance_condition: acceptance_condition.to_string(),
        evidence_kind,
        owner: owner.to_string(),
        required: true,
        disposition: DutyDisposition::Planned,
        represented_in_plan: true,
        check_executed: false,
        check_passed: false,
        requested_behavior_covered: false,
        evidence_ref: None,
        observed_execution: false,
        negative_control_verified: false,
        assertion_count: None,
        collected_test_count: None,
        subject_commit: None,
        target_commit: None,
        exclusion_rationale: None,
    }
}

/// Summarize duties compactly while preserving stable IDs and functions across
/// compact views and continuation boundaries.
pub fn summarize_duties(
    duties: &[DerivedProfessionalDuty],
    delivery_endpoint: DeliveryEndpointKind,
) -> DutyCompactSummary {
    let mut functions: Vec<String> = duties
        .iter()
        .map(|d| d.function.as_str().to_string())
        .collect();
    functions.sort();
    functions.dedup();
    let duty_ids: Vec<String> = duties.iter().map(|d| d.duty_id.clone()).collect();
    let required_duties = duties.iter().filter(|d| d.required).count();
    let satisfied_duties = duties
        .iter()
        .filter(|d| {
            matches!(
                d.disposition,
                DutyDisposition::Satisfied | DutyDisposition::NonApplicableWithRationale
            )
        })
        .count();
    let fp_payload = serde_json::to_string(duties).unwrap_or_default();
    DutyCompactSummary {
        total_duties: duties.len(),
        required_duties,
        satisfied_duties,
        functions_covered: functions,
        duty_ids,
        delivery_endpoint,
        coverage_digest: sha256_str(&fp_payload),
    }
}

/// Derive the full outcome-implied professional responsibility contract (#1981/#1982/#1983).
pub fn derive_professional_contract(input: &OutcomeDerivationInput) -> DerivedProfessionalContract {
    let full_request = compose_governing_request(
        input.original_request.as_deref(),
        input.task_title.as_deref(),
        input.task_description.as_deref(),
        &input.clarifications,
    );
    let req_lower = full_request.to_lowercase();
    let profile = classify_task_profile(
        &req_lower,
        &input.affected_paths,
        input.project_intent.as_deref(),
    );

    let mut delivery_input = input.delivery_policy.clone();
    delivery_input.governing_request = full_request.clone();
    let delivery_endpoint = resolve_delivery_endpoint(&delivery_input);
    let policy_rev = &input.delivery_policy.policy_revision;
    let src_rev = &input.source_revision;
    let owner = &input.owner;

    let mut duties = Vec::new();
    let mut relevant_areas = Vec::new();
    let mut proof_required = vec!["decapod validate passes".to_string()];
    let mut constraints = vec!["Strict adherence to AGENTS.md".to_string()];
    let mut allowed_scope = Vec::new();

    match profile {
        TaskOutcomeProfile::ProportionateProse => {
            duties.push(make_duty(
                "duty:docs:proportionate_clarity",
                ProfessionalFunction::DocsSupport,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Prose-only edit requires accurate wording without unrelated doc churn.",
                "Affected documentation reads accurately and renders cleanly.",
                DutyEvidenceKind::ProportionateValidation,
                owner,
            ));
            duties.push(make_duty(
                "duty:qa:proportionate_suite_check",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Existing adequate validation satisfies prose edits without test proliferation.",
                "Existing doc/pre-build validation checks pass.",
                DutyEvidenceKind::ProportionateValidation,
                owner,
            ));
            relevant_areas.push("docs/".to_string());
            allowed_scope.push("docs/".to_string());
        }
        TaskOutcomeProfile::NonCodeBusiness => {
            duties.push(make_duty(
                "duty:product:problem_and_assumptions",
                ProfessionalFunction::ProductResearch,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Business deliverable requires explicit user problem, constraints, and grounded assumptions.",
                "Assumptions and constraints cite repository/business sources honestly.",
                DutyEvidenceKind::SourceBackedBusinessArtifact,
                owner,
            ));
            duties.push(make_duty(
                "duty:commercial:source_backed_claims",
                ProfessionalFunction::CommercialOrg,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Commercial/policy claims require factual consistency with product capabilities and pricing.",
                "Claims and deliverables match accepted repository/commercial facts.",
                DutyEvidenceKind::SourceBackedBusinessArtifact,
                owner,
            ));
            duties.push(make_duty(
                "duty:docs:deliverable_clarity",
                ProfessionalFunction::DocsSupport,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Stakeholder-facing deliverable must be diagnosable and clearly structured.",
                "Deliverable cites verifiable sources and preserves open risks.",
                DutyEvidenceKind::DocsAlignmentCheck,
                owner,
            ));
            relevant_areas.push("docs/".to_string());
            allowed_scope.push("docs/".to_string());
        }
        TaskOutcomeProfile::InteractiveUx => {
            duties.push(make_duty(
                "duty:product:ux_problem_acceptance",
                ProfessionalFunction::ProductResearch,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Short UX outcome implies investigating user friction and defining observable acceptance.",
                "Recorded problem statement, bounded improvements, and observable UX criteria.",
                DutyEvidenceKind::InteractiveObservation,
                owner,
            ));
            duties.push(make_duty(
                "duty:ux:spatial_discoverability_context",
                ProfessionalFunction::DesignUx,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Interactive UX work requires coherent navigation, feature discoverability, accessibility, and preserved work context.",
                "Navigation is spatially coherent, controls are discoverable, and instruction context stays visible while composing.",
                DutyEvidenceKind::InteractiveObservation,
                owner,
            ));
            duties.push(make_duty(
                "duty:eng:ux_implementation_and_error_paths",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "UX improvements must fit existing architecture and handle edge/error states cleanly.",
                "Implementation preserves existing functionality and handles terminal/viewport boundaries.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:qa:behavior_negative_control_tests",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Pre-existing green tests do not prove UX behavior; add checks that fail on the defect and pass after fix.",
                "Behavior-specific regression checks fail on broken baseline / negative control and pass on candidate.",
                DutyEvidenceKind::FailingBaselineControl,
                owner,
            ));
            duties.push(make_duty(
                "duty:docs:affected_ux_guidance",
                ProfessionalFunction::DocsSupport,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Updated shortcuts, controls, or workflows require accurate user/operator documentation.",
                "Affected docs match implemented controls and keybindings.",
                DutyEvidenceKind::DocsAlignmentCheck,
                owner,
            ));
            duties.push(make_duty(
                "duty:ops:ux_runtime_reliability",
                ProfessionalFunction::SecurityPrivacyDataOps,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Interactive surface changes must preserve terminal/session safety and state persistence.",
                "No session leakage, crash, or lost input buffer across resize/abort.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            relevant_areas.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
            proof_required.push(
                "Behavior-specific UX regression/negative-control check and affected docs alignment"
                    .to_string(),
            );
            constraints.push(
                "Replaying pre-existing green tests alone cannot satisfy changed UX behavior (#1982)."
                    .to_string(),
            );
            allowed_scope.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
        }
        TaskOutcomeProfile::CustomerCommercial => {
            duties.push(make_duty(
                "duty:product:customer_flow_constraints",
                ProfessionalFunction::ProductResearch,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Customer signup/billing request requires explicit user flow, tier constraints, and edge-case acceptance.",
                "Customer flow milestones and failure states have observable acceptance criteria.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:ux:accessible_onboarding_flow",
                ProfessionalFunction::DesignUx,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Usable team signup requires clear form validation, accessibility, and actionable recovery.",
                "Interactive flow handles invalid/denied inputs cleanly with accessible affordances.",
                DutyEvidenceKind::InteractiveObservation,
                owner,
            ));
            duties.push(make_duty(
                "duty:commercial:pricing_entitlement_consistency",
                ProfessionalFunction::CommercialOrg,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Paid team signup must align UI/API pricing, seat limits, and entitlement enforcement.",
                "Seat, billing, and entitlement states match accepted pricing rules with zero unpaid bypass.",
                DutyEvidenceKind::FailingBaselineControl,
                owner,
            ));
            duties.push(make_duty(
                "duty:secops:auth_rls_and_rollout_safety",
                ProfessionalFunction::SecurityPrivacyDataOps,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Paid team signup crosses auth, tenant isolation, and operational telemetry boundaries.",
                "Auth/tenant boundaries fail closed and observability emits non-secret diagnostics.",
                DutyEvidenceKind::FailingBaselineControl,
                owner,
            ));
            duties.push(make_duty(
                "duty:qa:end_to_end_signup_regression",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Behavior changes require positive and negative signup/entitlement integration coverage.",
                "Integration tests verify valid flow and fail-closed rejection cases.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:docs:signup_supportability_guidance",
                ProfessionalFunction::DocsSupport,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Customer/operator onboarding and error states require accurate docs and support guidance.",
                "Onboarding/billing docs match actual API and UI errors.",
                DutyEvidenceKind::DocsAlignmentCheck,
                owner,
            ));
            relevant_areas.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
            proof_required.push(
                "Entitlement consistency, auth boundary negative control, and customer-flow regression checks"
                    .to_string(),
            );
            allowed_scope.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
        }
        TaskOutcomeProfile::EngineeringInterface => {
            duties.push(make_duty(
                "duty:product:outcome_acceptance_criteria",
                ProfessionalFunction::ProductResearch,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Outcome request requires explicit acceptance boundaries grounded in repository state.",
                "Acceptance criteria bind requested outcome to target interfaces.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:eng:architecture_and_error_paths",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Implementation must preserve interface compatibility and handle failure paths.",
                "Changed interfaces and error paths maintain compatibility and invariants.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:qa:behavior_relevant_tests",
                ProfessionalFunction::EngineeringQa,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Ordinary engineering work includes outcome-relevant tests without requiring 'test'/'fix' keywords.",
                "Relevant regression/negative-control tests execute and cover changed behavior.",
                DutyEvidenceKind::FailingBaselineControl,
                owner,
            ));
            duties.push(make_duty(
                "duty:secops:trust_and_recovery_boundaries",
                ProfessionalFunction::SecurityPrivacyDataOps,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Changes touching state, auth, custody, or config must preserve fail-closed recovery and security boundaries.",
                "Trust/custody invariants remain verified.",
                DutyEvidenceKind::BehaviorRegressionTest,
                owner,
            ));
            duties.push(make_duty(
                "duty:docs:affected_interface_docs",
                ProfessionalFunction::DocsSupport,
                DutyOrigin::InferredProfessionalMeans,
                &full_request,
                src_rev,
                policy_rev,
                "Affected CLI/RPC/config surfaces require synchronized docs and living specs.",
                "Affected documentation and living specs match implemented interfaces.",
                DutyEvidenceKind::DocsAlignmentCheck,
                owner,
            ));
            relevant_areas.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
            proof_required.push(
                "Outcome-relevant regression/negative-control tests and affected docs sync"
                    .to_string(),
            );
            constraints
                .push("Do not introduce regressions across error paths or interfaces.".to_string());
            allowed_scope.extend([
                "src/".to_string(),
                "tests/".to_string(),
                "docs/".to_string(),
            ]);
        }
        TaskOutcomeProfile::AnalysisOrDraft => {
            duties.push(make_duty(
                "duty:product:grounded_findings",
                ProfessionalFunction::ProductResearch,
                DutyOrigin::ExplicitHumanRequest,
                &full_request,
                src_rev,
                policy_rev,
                "Explicit analysis/draft scope requires source-grounded findings without unrequested mutation.",
                "Findings cite exact observed files, symbols, and command outputs.",
                DutyEvidenceKind::SourceBackedBusinessArtifact,
                owner,
            ));
            relevant_areas.push("src/".to_string());
            allowed_scope.push("src/".to_string());
        }
    }

    if req_lower.contains("fix") || req_lower.contains("bug") {
        let repro = "Reproduction test case".to_string();
        if !proof_required.contains(&repro) {
            proof_required.push(repro);
        }
        let no_reg = "Do not introduce regressions".to_string();
        if !constraints.contains(&no_reg) {
            constraints.push(no_reg);
        }
    }

    if !matches!(
        delivery_endpoint.endpoint,
        DeliveryEndpointKind::AnalysisOnly | DeliveryEndpointKind::Cancelled
    ) {
        duties.push(make_duty(
            "duty:delivery:authorized_endpoint_readback",
            ProfessionalFunction::SecurityPrivacyDataOps,
            DutyOrigin::AcceptedBaselineRule,
            &full_request,
            src_rev,
            policy_rev,
            &delivery_endpoint.rationale,
            "Reach the resolved delivery endpoint under standing authority and read back remote/target state.",
            DutyEvidenceKind::DeliveryReadback,
            owner,
        ));
        proof_required.push(format!(
            "Delivery endpoint ({:?}): {}",
            delivery_endpoint.endpoint,
            delivery_endpoint.remaining_steps.join(" -> ")
        ));
    }

    let summary = summarize_duties(&duties, delivery_endpoint.endpoint);
    DerivedProfessionalContract {
        schema: PROFESSIONAL_DEFAULTS_SCHEMA.to_string(),
        full_governing_request: full_request,
        profile,
        duties,
        delivery_endpoint,
        relevant_areas,
        proof_required,
        constraints,
        allowed_scope,
        summary,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DutyRefinementUpdate {
    pub duty_id: String,
    pub disposition: DutyDisposition,
    pub requested_required_flag: Option<bool>,
    pub check_executed: bool,
    pub check_passed: bool,
    pub requested_behavior_covered: bool,
    pub evidence_ref: Option<String>,
    pub observed_execution: bool,
    pub negative_control_verified: bool,
    pub assertion_count: Option<usize>,
    pub collected_test_count: Option<usize>,
    pub subject_commit: Option<String>,
    pub target_commit: Option<String>,
    pub exclusion_rationale: Option<String>,
    pub rationale_is_source_backed: bool,
}

/// Apply a governed refinement/evidence update to a derived duty (#1981/#1982).
/// Rejects unsupported omission, required->optional relabeling, and empty-rationale N/A.
pub fn refine_duty_disposition(
    duties: &mut [DerivedProfessionalDuty],
    update: &DutyRefinementUpdate,
) -> Result<(), String> {
    let duty = duties
        .iter_mut()
        .find(|d| d.duty_id == update.duty_id)
        .ok_or_else(|| format!("DUTY_NOT_FOUND: unknown derived duty '{}'", update.duty_id))?;

    if duty.required && update.requested_required_flag == Some(false) {
        return Err(format!(
            "REQUIRED_TO_OPTIONAL_FORBIDDEN: cannot relabel required duty '{}' as optional.",
            duty.duty_id
        ));
    }

    if update.disposition == DutyDisposition::NonApplicableWithRationale {
        let valid_reason = update
            .exclusion_rationale
            .as_deref()
            .is_some_and(|r| r.trim().len() >= 12)
            && update.rationale_is_source_backed;
        if !valid_reason {
            return Err(format!(
                "UNSUPPORTED_DUTY_EXCLUSION: marking duty '{}' non-applicable requires a source-backed applicability rationale.",
                duty.duty_id
            ));
        }
        if duty.origin == DutyOrigin::ExplicitHumanRequest {
            return Err(format!(
                "EXPLICIT_REQUEST_CANNOT_BE_DROPPED: explicit human requirement '{}' cannot be marked non-applicable without human cancellation.",
                duty.duty_id
            ));
        }
    }

    duty.disposition = update.disposition;
    duty.check_executed = update.check_executed;
    duty.check_passed = update.check_passed;
    duty.requested_behavior_covered = update.requested_behavior_covered;
    duty.evidence_ref = update.evidence_ref.clone();
    duty.observed_execution = update.observed_execution;
    duty.negative_control_verified = update.negative_control_verified;
    duty.assertion_count = update.assertion_count;
    duty.collected_test_count = update.collected_test_count;
    duty.subject_commit = update.subject_commit.clone();
    duty.target_commit = update.target_commit.clone();
    duty.exclusion_rationale = update.exclusion_rationale.clone();
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DutyCoverageGap {
    pub duty_id: String,
    pub function: ProfessionalFunction,
    pub code: String,
    pub reason: String,
    pub next_admissible_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DutyCoverageReport {
    pub ready: bool,
    pub represented_in_plan_count: usize,
    pub check_executed_count: usize,
    pub check_passed_count: usize,
    pub requested_behavior_covered_count: usize,
    pub unmet_duties: Vec<DutyCoverageGap>,
}

/// Evaluate whether derived professional duties are actually covered by fresh,
/// outcome-relevant, non-vacuous evidence bound to current candidate/target (#1982).
pub fn evaluate_duty_coverage(
    duties: &[DerivedProfessionalDuty],
    current_candidate_commit: &str,
    current_target_commit: &str,
    current_policy_revision: &str,
) -> DutyCoverageReport {
    let mut unmet = Vec::new();
    let mut represented = 0usize;
    let mut executed = 0usize;
    let mut passed = 0usize;
    let mut covered = 0usize;

    for duty in duties {
        if duty.represented_in_plan {
            represented += 1;
        }
        if duty.check_executed {
            executed += 1;
        }
        if duty.check_passed {
            passed += 1;
        }
        if duty.requested_behavior_covered {
            covered += 1;
        }

        if !duty.required {
            continue;
        }

        if duty.disposition == DutyDisposition::NonApplicableWithRationale {
            if duty
                .exclusion_rationale
                .as_deref()
                .is_none_or(|r| r.trim().is_empty())
            {
                unmet.push(DutyCoverageGap {
                    duty_id: duty.duty_id.clone(),
                    function: duty.function,
                    code: "UNSUPPORTED_NON_APPLICABLE".to_string(),
                    reason: "Non-applicable label lacks source-backed rationale.".to_string(),
                    next_admissible_action: format!(
                        "Provide source-backed rationale or execute evidence for '{}'.",
                        duty.duty_id
                    ),
                });
            }
            continue;
        }

        if !duty.represented_in_plan {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "DUTY_MISSING_FROM_PLAN".to_string(),
                reason: "Required duty is absent from governed plan representation.".to_string(),
                next_admissible_action: format!(
                    "Add duty '{}' to the governed plan.",
                    duty.duty_id
                ),
            });
            continue;
        }

        if !duty.check_executed || !duty.check_passed || duty.evidence_ref.is_none() {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "EVIDENCE_MISSING_OR_FAILED".to_string(),
                reason: "Required duty check has not executed and passed with a bound artifact."
                    .to_string(),
                next_admissible_action: format!(
                    "Execute acceptance check '{}' and attach evidence_ref.",
                    duty.acceptance_condition
                ),
            });
            continue;
        }

        if duty.collected_test_count == Some(0) || duty.assertion_count == Some(0) {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "VACUOUS_OR_EMPTY_CHECK".to_string(),
                reason: "Zero collected tests or zero assertions cannot satisfy a required duty."
                    .to_string(),
                next_admissible_action: format!(
                    "Replace vacuous check on '{}' with real behavioral assertions.",
                    duty.duty_id
                ),
            });
            continue;
        }

        if !duty.requested_behavior_covered {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "REQUESTED_BEHAVIOR_UNCOVERED".to_string(),
                reason: "Passing checks do not cover the requested outcome behavior (e.g. old suite stayed green while requested change stayed unverified)."
                    .to_string(),
                next_admissible_action: format!(
                    "Add behavior-specific check covering '{}' and re-verify.",
                    duty.acceptance_condition
                ),
            });
            continue;
        }

        if matches!(duty.evidence_kind, DutyEvidenceKind::FailingBaselineControl)
            && !duty.negative_control_verified
        {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "MISSING_NEGATIVE_CONTROL".to_string(),
                reason: "Behavior/security duty requires failing baseline or negative-control verification."
                    .to_string(),
                next_admissible_action: format!(
                    "Run negative-control / failing-baseline verification for '{}'.",
                    duty.duty_id
                ),
            });
            continue;
        }

        if !matches!(
            duty.evidence_kind,
            DutyEvidenceKind::SourceBackedBusinessArtifact
                | DutyEvidenceKind::InteractiveObservation
        ) && !duty.observed_execution
        {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "UNOBSERVED_AGENT_SUMMARY_ONLY".to_string(),
                reason: "Agent-declared summary without mechanically observed execution cannot satisfy verification."
                    .to_string(),
                next_admissible_action: format!(
                    "Run actual verification command for '{}' and record observed output.",
                    duty.duty_id
                ),
            });
            continue;
        }

        let stale_commit = duty
            .subject_commit
            .as_deref()
            .is_some_and(|c| c != current_candidate_commit)
            || duty
                .target_commit
                .as_deref()
                .is_some_and(|t| t != current_target_commit);
        let stale_policy = duty.policy_revision != current_policy_revision;
        if stale_commit || stale_policy {
            unmet.push(DutyCoverageGap {
                duty_id: duty.duty_id.clone(),
                function: duty.function,
                code: "STALE_DUTY_EVIDENCE".to_string(),
                reason: "Evidence is bound to an older candidate commit, target commit, or policy revision."
                    .to_string(),
                next_admissible_action: format!(
                    "Re-run '{}' against candidate {} and target {}.",
                    duty.duty_id, current_candidate_commit, current_target_commit
                ),
            });
        }
    }

    DutyCoverageReport {
        ready: unmet.is_empty(),
        represented_in_plan_count: represented,
        check_executed_count: executed,
        check_passed_count: passed,
        requested_behavior_covered_count: covered,
        unmet_duties: unmet,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryProgressionInput {
    pub contract: ResolvedDeliveryContract,
    pub phase_status: DeliveryPhaseStatus,
    pub duty_coverage_ready: bool,
    pub main_merge_input: MainMergeAuthorizationInput,
    pub pr_number: Option<u64>,
    pub pr_checks_green: bool,
    pub pr_reviews_satisfied: bool,
    pub operation_id: Option<String>,
    pub forge_merge_observed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeliveryProgressionDecision {
    pub terminal_reached: bool,
    pub permitted_to_advance: bool,
    pub requires_human_followup_prompt: bool,
    pub status: DeliveryPhaseStatus,
    pub next_action: String,
    pub blocking_code: Option<String>,
    pub must_query_remote_first: bool,
    pub reuse_operation_id: Option<String>,
    pub reuse_pr_number: Option<u64>,
    pub direct_merge_decision: Option<MainMergeAuthorizationDecision>,
}

/// Evaluate the next authorized delivery transition (#1983), continuing automatically
/// through commit/push/PR/merge without asking follow-up prompts whenever standing
/// policy and fresh evidence permit, and recovering unknown remote states idempotently.
pub fn advance_delivery_progression(
    input: &DeliveryProgressionInput,
) -> DeliveryProgressionDecision {
    if input.contract.endpoint == DeliveryEndpointKind::Cancelled
        || input.phase_status == DeliveryPhaseStatus::Cancelled
    {
        return DeliveryProgressionDecision {
            terminal_reached: true,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::Cancelled,
            next_action: "none".to_string(),
            blocking_code: Some("EXPLICIT_CANCELLATION".to_string()),
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        };
    }

    if input.phase_status == DeliveryPhaseStatus::UnknownPendingReadback {
        if let Some(true) = input.forge_merge_observed {
            return DeliveryProgressionDecision {
                terminal_reached: true,
                permitted_to_advance: false,
                requires_human_followup_prompt: false,
                status: DeliveryPhaseStatus::Merged,
                next_action: "record_observed_merge_once".to_string(),
                blocking_code: None,
                must_query_remote_first: false,
                reuse_operation_id: input.operation_id.clone(),
                reuse_pr_number: input.pr_number,
                direct_merge_decision: None,
            };
        }
        return DeliveryProgressionDecision {
            terminal_reached: false,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::UnknownPendingReadback,
            next_action: "query_operation_journal_and_forge_state_before_retry".to_string(),
            blocking_code: Some("REMOTE_OUTCOME_UNKNOWN".to_string()),
            must_query_remote_first: true,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        };
    }

    if input.phase_status == DeliveryPhaseStatus::Merged {
        return DeliveryProgressionDecision {
            terminal_reached: true,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::Merged,
            next_action: "none".to_string(),
            blocking_code: None,
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        };
    }

    if !input.duty_coverage_ready {
        return DeliveryProgressionDecision {
            terminal_reached: false,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::Blocked,
            next_action: "repair_or_verify_unmet_professional_duties".to_string(),
            blocking_code: Some("DUTY_COVERAGE_INCOMPLETE".to_string()),
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        };
    }

    if !input.main_merge_input.full_validation_passed
        || input.main_merge_input.validated_candidate_commit
            != input.main_merge_input.current_candidate_commit
        || input.main_merge_input.validated_target_commit
            != input.main_merge_input.current_target_commit
    {
        return DeliveryProgressionDecision {
            terminal_reached: false,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::Blocked,
            next_action: "run_decapod_validate_for_exact_candidate_and_target".to_string(),
            blocking_code: Some("FRESH_EXACT_CANDIDATE_VALIDATION_REQUIRED".to_string()),
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        };
    }

    match input.contract.endpoint {
        DeliveryEndpointKind::AnalysisOnly => DeliveryProgressionDecision {
            terminal_reached: true,
            permitted_to_advance: false,
            requires_human_followup_prompt: false,
            status: DeliveryPhaseStatus::Validated,
            next_action: "none".to_string(),
            blocking_code: None,
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        },
        DeliveryEndpointKind::DraftOnly => DeliveryProgressionDecision {
            terminal_reached: input.pr_number.is_some(),
            permitted_to_advance: input.pr_number.is_none(),
            requires_human_followup_prompt: false,
            status: if input.pr_number.is_some() {
                DeliveryPhaseStatus::PrOpened
            } else {
                DeliveryPhaseStatus::Validated
            },
            next_action: if input.pr_number.is_some() {
                "none".to_string()
            } else {
                "push_branch_and_open_draft_pr".to_string()
            },
            blocking_code: None,
            must_query_remote_first: false,
            reuse_operation_id: input.operation_id.clone(),
            reuse_pr_number: input.pr_number,
            direct_merge_decision: None,
        },
        DeliveryEndpointKind::CommitOnly | DeliveryEndpointKind::PushOnly => {
            let blocked_on_merge = input.contract.candidate_attempted_elevation
                || matches!(
                    input.phase_status,
                    DeliveryPhaseStatus::Pushed | DeliveryPhaseStatus::Validated
                );
            DeliveryProgressionDecision {
                terminal_reached: false,
                permitted_to_advance: !matches!(input.phase_status, DeliveryPhaseStatus::Pushed),
                requires_human_followup_prompt: false,
                status: if matches!(input.phase_status, DeliveryPhaseStatus::Pushed) {
                    DeliveryPhaseStatus::StoppedMissingAuthority
                } else {
                    input.phase_status
                },
                next_action: if matches!(input.phase_status, DeliveryPhaseStatus::Pushed) {
                    "await_accepted_merge_authority".to_string()
                } else {
                    "commit_and_push_validated_branch".to_string()
                },
                blocking_code: if blocked_on_merge
                    && matches!(input.phase_status, DeliveryPhaseStatus::Pushed)
                {
                    Some(if input.contract.candidate_attempted_elevation {
                        "POLICY_SELF_AUTHORIZATION_REJECTED".to_string()
                    } else {
                        "MAIN_MERGE_AUTHORITY_MISSING".to_string()
                    })
                } else {
                    None
                },
                must_query_remote_first: false,
                reuse_operation_id: input.operation_id.clone(),
                reuse_pr_number: input.pr_number,
                direct_merge_decision: None,
            }
        }
        DeliveryEndpointKind::PullRequest => {
            if input.pr_number.is_none() {
                return DeliveryProgressionDecision {
                    terminal_reached: false,
                    permitted_to_advance: true,
                    requires_human_followup_prompt: false,
                    status: DeliveryPhaseStatus::Validated,
                    next_action: "publish_branch_and_open_or_reuse_pr".to_string(),
                    blocking_code: None,
                    must_query_remote_first: false,
                    reuse_operation_id: input.operation_id.clone(),
                    reuse_pr_number: None,
                    direct_merge_decision: None,
                };
            }
            DeliveryProgressionDecision {
                terminal_reached: false,
                permitted_to_advance: false,
                requires_human_followup_prompt: false,
                status: DeliveryPhaseStatus::StoppedMissingAuthority,
                next_action: "await_human_pr_merge_approval".to_string(),
                blocking_code: Some("MERGE_DELEGATION_NOT_GRANTED".to_string()),
                must_query_remote_first: false,
                reuse_operation_id: input.operation_id.clone(),
                reuse_pr_number: input.pr_number,
                direct_merge_decision: None,
            }
        }
        DeliveryEndpointKind::PullRequestThenMerge => {
            if input.pr_number.is_none() {
                return DeliveryProgressionDecision {
                    terminal_reached: false,
                    permitted_to_advance: true,
                    requires_human_followup_prompt: false,
                    status: DeliveryPhaseStatus::Validated,
                    next_action: "publish_branch_and_open_or_reuse_pr".to_string(),
                    blocking_code: None,
                    must_query_remote_first: false,
                    reuse_operation_id: input.operation_id.clone(),
                    reuse_pr_number: None,
                    direct_merge_decision: None,
                };
            }
            if !input.pr_checks_green || !input.pr_reviews_satisfied {
                return DeliveryProgressionDecision {
                    terminal_reached: false,
                    permitted_to_advance: false,
                    requires_human_followup_prompt: false,
                    status: DeliveryPhaseStatus::PrOpened,
                    next_action: if !input.pr_checks_green {
                        "wait_for_or_repair_pr_checks".to_string()
                    } else {
                        "wait_for_required_pr_review".to_string()
                    },
                    blocking_code: Some(if !input.pr_checks_green {
                        "PR_CHECKS_PENDING_OR_FAILED".to_string()
                    } else {
                        "PR_REVIEW_REQUIRED".to_string()
                    }),
                    must_query_remote_first: false,
                    reuse_operation_id: input.operation_id.clone(),
                    reuse_pr_number: input.pr_number,
                    direct_merge_decision: None,
                };
            }
            if input.phase_status == DeliveryPhaseStatus::QueuedForMerge
                || input.phase_status == DeliveryPhaseStatus::MergeRequested
            {
                return DeliveryProgressionDecision {
                    terminal_reached: false,
                    permitted_to_advance: false,
                    requires_human_followup_prompt: false,
                    status: input.phase_status,
                    next_action: "observe_pr_merge_completion_on_target".to_string(),
                    blocking_code: None,
                    must_query_remote_first: true,
                    reuse_operation_id: input.operation_id.clone(),
                    reuse_pr_number: input.pr_number,
                    direct_merge_decision: None,
                };
            }
            DeliveryProgressionDecision {
                terminal_reached: false,
                permitted_to_advance: true,
                requires_human_followup_prompt: false,
                status: DeliveryPhaseStatus::PrOpened,
                next_action: "execute_authorized_pr_merge_and_read_back_target".to_string(),
                blocking_code: None,
                must_query_remote_first: false,
                reuse_operation_id: input.operation_id.clone(),
                reuse_pr_number: input.pr_number,
                direct_merge_decision: None,
            }
        }
        DeliveryEndpointKind::DirectMerge => {
            let merge_decision = evaluate_main_merge_authorization(&input.main_merge_input);
            if !merge_decision.permitted {
                return DeliveryProgressionDecision {
                    terminal_reached: false,
                    permitted_to_advance: false,
                    requires_human_followup_prompt: false,
                    status: DeliveryPhaseStatus::StoppedMissingAuthority,
                    next_action: "reconcile_target_or_obtain_authority".to_string(),
                    blocking_code: merge_decision.rejection_code.clone(),
                    must_query_remote_first: false,
                    reuse_operation_id: input.operation_id.clone(),
                    reuse_pr_number: input.pr_number,
                    direct_merge_decision: Some(merge_decision),
                };
            }
            DeliveryProgressionDecision {
                terminal_reached: false,
                permitted_to_advance: true,
                requires_human_followup_prompt: false,
                status: DeliveryPhaseStatus::Validated,
                next_action: "execute_authorized_direct_merge_and_read_back_target".to_string(),
                blocking_code: None,
                must_query_remote_first: false,
                reuse_operation_id: input.operation_id.clone(),
                reuse_pr_number: input.pr_number,
                direct_merge_decision: Some(merge_decision),
            }
        }
        DeliveryEndpointKind::Cancelled => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_policy(
        accepted_main_merge: bool,
        candidate_main_merge: bool,
        requires_pr_workflow: bool,
        standing_pr_merge_delegated: bool,
    ) -> DeliveryResolutionInput {
        DeliveryResolutionInput {
            governing_request: String::new(),
            accepted_main_merge,
            candidate_main_merge,
            requires_pr_workflow,
            standing_pr_merge_delegated,
            requires_review: requires_pr_workflow,
            requires_ci_checks: requires_pr_workflow,
            policy_revision: "sha256:policy-v1".to_string(),
        }
    }

    #[test]
    fn unit_1981_keywordless_and_terse_request_parity() {
        let terse = derive_professional_contract(&OutcomeDerivationInput {
            original_request: Some("roll the dice on the agy tui ux".to_string()),
            task_title: None,
            task_description: None,
            clarifications: vec![],
            project_intent: Some("Decapod governance runtime".to_string()),
            affected_paths: vec!["src/tui/app.rs".to_string()],
            source_revision: "rev-1".to_string(),
            owner: "agent-a".to_string(),
            delivery_policy: base_policy(true, false, false, false),
        });
        let prose = derive_professional_contract(&OutcomeDerivationInput {
            original_request: Some(
                "Audit the current AGY terminal flow from a new user perspective and leave it clearer"
                    .to_string(),
            ),
            task_title: None,
            task_description: None,
            clarifications: vec![],
            project_intent: Some("Decapod governance runtime".to_string()),
            affected_paths: vec!["src/tui/app.rs".to_string()],
            source_revision: "rev-1".to_string(),
            owner: "agent-a".to_string(),
            delivery_policy: base_policy(true, false, false, false),
        });
        assert!(terse.duties.len() > 5);
        assert_eq!(terse.profile, TaskOutcomeProfile::InteractiveUx);
        assert_eq!(prose.profile, TaskOutcomeProfile::InteractiveUx);
        let f1: Vec<_> = terse.duties.iter().map(|d| d.function).collect();
        let f2: Vec<_> = prose.duties.iter().map(|d| d.function).collect();
        assert_eq!(f1, f2);
        assert_eq!(
            terse.delivery_endpoint.endpoint,
            DeliveryEndpointKind::DirectMerge
        );
    }

    #[test]
    fn unit_1982_rejects_unobserved_summary_missing_negative_control_and_policy_drift() {
        let mut contract = derive_professional_contract(&OutcomeDerivationInput {
            original_request: Some("make paid team signup usable".to_string()),
            task_title: None,
            task_description: None,
            clarifications: vec![],
            project_intent: Some("Commercial web portal".to_string()),
            affected_paths: vec!["src/signup.rs".to_string()],
            source_revision: "cand-1".to_string(),
            owner: "agent-a".to_string(),
            delivery_policy: base_policy(false, false, true, true),
        });

        let qa_id = contract
            .duties
            .iter()
            .find(|d| d.evidence_kind == DutyEvidenceKind::FailingBaselineControl)
            .unwrap()
            .duty_id
            .clone();

        for d in &contract.duties.clone() {
            refine_duty_disposition(
                &mut contract.duties,
                &DutyRefinementUpdate {
                    duty_id: d.duty_id.clone(),
                    disposition: DutyDisposition::Satisfied,
                    requested_required_flag: None,
                    check_executed: true,
                    check_passed: true,
                    requested_behavior_covered: true,
                    evidence_ref: Some("ci/report.json".to_string()),
                    observed_execution: d.duty_id != qa_id,
                    negative_control_verified: d.duty_id != qa_id,
                    assertion_count: Some(4),
                    collected_test_count: Some(2),
                    subject_commit: Some("cand-1".to_string()),
                    target_commit: Some("targ-1".to_string()),
                    exclusion_rationale: None,
                    rationale_is_source_backed: false,
                },
            )
            .unwrap();
        }

        let missing_neg =
            evaluate_duty_coverage(&contract.duties, "cand-1", "targ-1", "sha256:policy-v1");
        assert!(!missing_neg.ready);
        assert!(
            missing_neg
                .unmet_duties
                .iter()
                .any(|g| g.code == "MISSING_NEGATIVE_CONTROL")
        );

        refine_duty_disposition(
            &mut contract.duties,
            &DutyRefinementUpdate {
                duty_id: qa_id.clone(),
                disposition: DutyDisposition::Satisfied,
                requested_required_flag: None,
                check_executed: true,
                check_passed: true,
                requested_behavior_covered: true,
                evidence_ref: Some("ci/report.json".to_string()),
                observed_execution: false,
                negative_control_verified: true,
                assertion_count: Some(4),
                collected_test_count: Some(2),
                subject_commit: Some("cand-1".to_string()),
                target_commit: Some("targ-1".to_string()),
                exclusion_rationale: None,
                rationale_is_source_backed: false,
            },
        )
        .unwrap();
        let unobs =
            evaluate_duty_coverage(&contract.duties, "cand-1", "targ-1", "sha256:policy-v1");
        assert!(
            unobs
                .unmet_duties
                .iter()
                .any(|g| g.code == "UNOBSERVED_AGENT_SUMMARY_ONLY")
        );

        // Fix qa_id cleanly, then drift policy revision and target commit -> STALE_DUTY_EVIDENCE
        refine_duty_disposition(
            &mut contract.duties,
            &DutyRefinementUpdate {
                duty_id: qa_id,
                disposition: DutyDisposition::Satisfied,
                requested_required_flag: None,
                check_executed: true,
                check_passed: true,
                requested_behavior_covered: true,
                evidence_ref: Some("ci/report.json".to_string()),
                observed_execution: true,
                negative_control_verified: true,
                assertion_count: Some(4),
                collected_test_count: Some(2),
                subject_commit: Some("cand-1".to_string()),
                target_commit: Some("targ-1".to_string()),
                exclusion_rationale: None,
                rationale_is_source_backed: false,
            },
        )
        .unwrap();
        assert!(
            evaluate_duty_coverage(&contract.duties, "cand-1", "targ-1", "sha256:policy-v1").ready
        );
        assert!(
            !evaluate_duty_coverage(&contract.duties, "cand-1", "targ-2", "sha256:policy-v1").ready
        );
        assert!(
            !evaluate_duty_coverage(&contract.duties, "cand-1", "targ-1", "sha256:policy-v2").ready
        );
    }

    #[test]
    fn unit_1983_endpoint_narrowing_direct_merge_and_unknown_readback() {
        let mut pol = base_policy(false, false, true, false);
        pol.governing_request = "analysis only: check why login breaks".to_string();
        assert_eq!(
            resolve_delivery_endpoint(&pol).endpoint,
            DeliveryEndpointKind::AnalysisOnly
        );

        pol.governing_request = "draft only: prototype layout".to_string();
        assert_eq!(
            resolve_delivery_endpoint(&pol).endpoint,
            DeliveryEndpointKind::DraftOnly
        );

        pol.governing_request = "cancel the work now".to_string();
        assert_eq!(
            resolve_delivery_endpoint(&pol).endpoint,
            DeliveryEndpointKind::Cancelled
        );

        let direct = resolve_delivery_endpoint(&DeliveryResolutionInput {
            governing_request: "roll the dice on the agy tui ux".to_string(),
            ..base_policy(true, false, false, false)
        });
        assert_eq!(direct.endpoint, DeliveryEndpointKind::DirectMerge);

        let merge_decision = advance_delivery_progression(&DeliveryProgressionInput {
            contract: direct.clone(),
            phase_status: DeliveryPhaseStatus::Validated,
            duty_coverage_ready: true,
            main_merge_input: MainMergeAuthorizationInput {
                accepted_base_main_merge: true,
                candidate_main_merge: true,
                validated_candidate_commit: "c1".to_string(),
                current_candidate_commit: "c1".to_string(),
                validated_target_commit: "t1".to_string(),
                current_target_commit: "t1".to_string(),
                full_validation_passed: true,
            },
            pr_number: None,
            pr_checks_green: true,
            pr_reviews_satisfied: true,
            operation_id: Some("op-direct-1".to_string()),
            forge_merge_observed: None,
        });
        assert!(merge_decision.permitted_to_advance);
        assert!(!merge_decision.requires_human_followup_prompt);

        // Unknown outcome during PR merge forces readback reusing same operation_id + pr_number.
        let pr_del = resolve_delivery_endpoint(&DeliveryResolutionInput {
            governing_request: "roll the dice".to_string(),
            ..base_policy(false, false, true, true)
        });
        let unknown = advance_delivery_progression(&DeliveryProgressionInput {
            contract: pr_del,
            phase_status: DeliveryPhaseStatus::UnknownPendingReadback,
            duty_coverage_ready: true,
            main_merge_input: MainMergeAuthorizationInput {
                accepted_base_main_merge: false,
                candidate_main_merge: false,
                validated_candidate_commit: "c1".to_string(),
                current_candidate_commit: "c1".to_string(),
                validated_target_commit: "t1".to_string(),
                current_target_commit: "t1".to_string(),
                full_validation_passed: true,
            },
            pr_number: Some(99),
            pr_checks_green: true,
            pr_reviews_satisfied: true,
            operation_id: Some("op-99".to_string()),
            forge_merge_observed: None,
        });
        assert!(unknown.must_query_remote_first);
        assert_eq!(unknown.reuse_operation_id.as_deref(), Some("op-99"));
        assert_eq!(unknown.reuse_pr_number, Some(99));
    }
}

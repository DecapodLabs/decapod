//! Validation Rules System
//!
//! This module implements a unified, rule-driven validation system that extends
//! the existing Constitution and OVERRIDE.md authority model. Rules translate
//! applicable principles into mechanically checkable requirements.

use crate::core::error;
use crate::core::validation_epoch::ValidationEpochMetadata;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// A unique identifier for a validation rule.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
pub struct RuleId(pub String);

impl RuleId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The authority that governs a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAuthority {
    /// Rule comes from Decapod's embedded constitution.
    Constitution,
    /// Rule comes from project-specific OVERRIDE.md.
    ProjectOverride,
    /// Rule comes from task-specific constraints (governed plan).
    TaskConstraints,
    /// Rule is a kernel invariant (non-waivable).
    KernelInvariant,
}

/// Phase at which a rule is evaluated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationPhase {
    /// Before any work begins (workspace creation, plan approval).
    PreExecution,
    /// During work execution (gate checks).
    DuringExecution,
    /// At completion validation (decapod validate).
    Validation,
    /// At publication readiness (workspace publish).
    Publication,
}

/// Severity of a rule violation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSeverity {
    /// Advisory - does not block.
    Advisory,
    /// Warning - blocks unless explicitly acknowledged.
    Warning,
    /// Error - blocks completion/publication.
    Error,
    /// Critical - cannot be overridden.
    Critical,
}

/// Verdict of a rule evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleVerdict {
    /// Rule passed - condition satisfied.
    Pass,
    /// Rule failed - condition violated.
    Fail,
    /// Rule could not be evaluated - missing evidence/evaluator.
    Unverifiable,
    /// Evidence was bound to a different governing identity.
    Stale,
}

/// Applicability conditions for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuleApplicability {
    /// File patterns that must be present in the change.
    pub required_changed_paths: Vec<String>,
    /// File patterns that must NOT be present in the change.
    pub forbidden_changed_paths: Vec<String>,
    /// Task types this rule applies to.
    pub task_types: Vec<String>,
    /// Git ref patterns (e.g., "refs/heads/main").
    pub ref_patterns: Vec<String>,
    /// Whether rule applies only in CI.
    pub ci_only: bool,
    /// Whether rule applies only outside CI.
    pub local_only: bool,
}

/// Required inputs and evidence for rule evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuleRequirements {
    /// Required proof artifacts.
    pub required_evidence: Vec<String>,
    /// Maximum age of evidence in seconds (0 = no limit).
    pub evidence_max_age_seconds: u64,
    /// Required evaluator capabilities.
    pub required_evaluators: Vec<String>,
    /// Additional context needed.
    pub required_context: Vec<String>,
}

/// Remediation contract for a rule violation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleRemediation {
    /// Stable error code.
    pub error_code: String,
    /// Human-readable description of the violation.
    pub violation_description: String,
    /// Expected condition.
    pub expected_condition: String,
    /// Observed condition.
    pub observed_condition: String,
    /// Affected paths or governance entities.
    pub affected_paths: Vec<String>,
    /// Required remediation steps.
    pub required_steps: Vec<String>,
    /// Whether an agent can autonomously remediate.
    pub autonomous_remediation_permitted: bool,
    /// Whether human authorization is required.
    pub human_authorization_required: bool,
    /// Evidence needed for revalidation.
    pub evidence_for_revalidation: Vec<String>,
    /// Related rule IDs that may also be affected.
    pub related_rules: Vec<RuleId>,
}

/// Override eligibility for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverrideEligibility {
    /// Rule can be fully overridden (severity, applicability, etc.).
    FullyOverridable,
    /// Rule can be tightened but not weakened.
    TightenOnly,
    /// Rule parameters can be configured but not disabled.
    ConfigurableOnly,
    /// Rule cannot be overridden (kernel invariant).
    NonOverridable,
}

/// Source provenance for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleProvenance {
    /// Source file (e.g., "embedded/constitution.json", ".decapod/OVERRIDE.md").
    pub source: String,
    /// Hash of the source content.
    pub source_hash: String,
    /// Hash of the rule body.
    pub body_hash: String,
    /// When the rule was registered.
    pub registered_at: String,
    /// Decapod version that registered the rule.
    pub decapod_version: String,
}

/// A validation rule definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    /// Stable unique identifier.
    pub id: RuleId,
    /// Human-readable description.
    pub description: String,
    /// Governing authority.
    pub authority: RuleAuthority,
    /// Applicability conditions.
    pub applicability: RuleApplicability,
    /// Evaluation phase.
    pub evaluation_phase: EvaluationPhase,
    /// Deterministic evaluator identity.
    pub evaluator: String,
    /// Required inputs and evidence.
    pub requirements: RuleRequirements,
    /// Enforcement severity.
    pub severity: RuleSeverity,
    /// Remediation contract.
    pub remediation: RuleRemediation,
    /// Override eligibility.
    pub override_eligibility: OverrideEligibility,
    /// Source provenance.
    pub provenance: RuleProvenance,
}

impl ValidationRule {
    /// Check if the rule is applicable to the given context.
    pub fn is_applicable(
        &self,
        changed_paths: &BTreeSet<String>,
        task_type: Option<&str>,
        git_ref: Option<&str>,
        is_ci: bool,
    ) -> bool {
        // Check required changed paths
        if !self.applicability.required_changed_paths.is_empty() {
            let has_required = self
                .applicability
                .required_changed_paths
                .iter()
                .any(|pattern| changed_paths.iter().any(|path| glob_match(pattern, path)));
            if !has_required {
                return false;
            }
        }

        // Forbidden paths are scope conditions. They do not exempt the rule.
        // Evaluation fails the change when a forbidden path is present.

        // Check task types
        if let Some(task_type) = task_type
            && !self.applicability.task_types.is_empty()
            && !self
                .applicability
                .task_types
                .contains(&task_type.to_string())
        {
            return false;
        }

        // Check ref patterns
        if let Some(git_ref) = git_ref
            && !self.applicability.ref_patterns.is_empty()
        {
            let matches = self
                .applicability
                .ref_patterns
                .iter()
                .any(|pattern| glob_match(pattern, git_ref));
            if !matches {
                return false;
            }
        }

        // Check CI/local only
        if self.applicability.ci_only && !is_ci {
            return false;
        }
        if self.applicability.local_only && is_ci {
            return false;
        }

        true
    }

    /// True when a changed path matches a forbidden-path condition.
    pub fn forbidden_scope_hit(&self, changed_paths: &BTreeSet<String>) -> bool {
        self.applicability
            .forbidden_changed_paths
            .iter()
            .any(|pattern| {
                changed_paths
                    .iter()
                    .any(|path| glob_match(pattern, path) || path_prefix(pattern, path))
            })
    }
}

fn path_prefix(prefix: &str, path: &str) -> bool {
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

/// Simple glob matching.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    // Handle **/pattern pattern (e.g., **/*.rs, **/secrets/**)
    if let Some(rest) = pattern.strip_prefix("**/") {
        if rest == "*" {
            return true;
        }
        if rest.contains('*') {
            // Handle *.rs pattern
            let parts: Vec<&str> = rest.split('*').collect();
            if parts.len() >= 2 {
                let suffix = parts[parts.len() - 1]; // last part after last *
                if !suffix.is_empty() {
                    return text.ends_with(suffix);
                }
                // If suffix is empty, check if prefix matches
                let prefix = parts[0];
                return text.contains(prefix);
            }
        }
        return text.ends_with(rest);
    }
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return (suffix.is_empty() || text.ends_with(suffix))
                && (prefix.is_empty() || text.starts_with(prefix));
        }
    }
    if pattern.contains('*') && !pattern.contains("**") {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];
            return text.starts_with(prefix) && text.ends_with(suffix);
        }
    }
    pattern == text
}

/// A resolved rule with its effective configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedRule {
    /// The base rule definition.
    pub rule: ValidationRule,
    /// Override values applied (if any).
    pub override_values: Option<RuleOverrideValues>,
    /// Final effective severity.
    pub effective_severity: RuleSeverity,
    /// Final effective applicability.
    pub effective_applicability: RuleApplicability,
}

/// Override values that can be applied to a rule.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleOverrideValues {
    /// Override severity.
    pub severity: Option<RuleSeverity>,
    /// Override applicability.
    pub applicability: Option<RuleApplicability>,
    /// Override requirements.
    pub requirements: Option<RuleRequirements>,
    /// Override remediation.
    pub remediation: Option<RuleRemediation>,
}

/// Parse override content from TOML format.
fn parse_override_content(content: &str) -> Result<RuleOverrideValues, error::DecapodError> {
    let mut values = RuleOverrideValues::default();

    // Simple TOML parsing for validation rule overrides
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "severity" => {
                    values.severity = Some(match value {
                        "advisory" => RuleSeverity::Advisory,
                        "warning" => RuleSeverity::Warning,
                        "error" => RuleSeverity::Error,
                        "critical" => RuleSeverity::Critical,
                        _ => {
                            return Err(error::DecapodError::ValidationError(format!(
                                "Invalid severity: {}",
                                value
                            )));
                        }
                    });
                }
                "evidence_max_age_seconds" => {
                    if let Ok(seconds) = value.parse::<u64>() {
                        values.requirements = Some(RuleRequirements {
                            evidence_max_age_seconds: seconds,
                            ..Default::default()
                        });
                    }
                }
                _ => {
                    // Ignore unknown keys for forward compatibility
                }
            }
        }
    }

    Ok(values)
}

/// Result of a rule evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEvaluationResult {
    /// Rule identifier.
    pub rule_id: RuleId,
    /// Verdict.
    pub verdict: RuleVerdict,
    /// Effective severity.
    pub severity: RuleSeverity,
    /// Evaluation timestamp.
    pub evaluated_at: String,
    /// Evidence references used.
    pub evidence_refs: Vec<String>,
    /// Detailed message.
    pub message: String,
    /// Remediation contract (if failed/unverifiable).
    pub remediation: Option<RuleRemediation>,
    /// Policy fingerprint this evaluation was bound to.
    pub policy_fingerprint: String,
    /// Validation epoch.
    pub validation_epoch: ValidationEpochMetadata,
}

/// Rule evaluation context.
#[derive(Debug, Clone)]
pub struct RuleEvaluationContext {
    /// Changed paths in the candidate.
    pub changed_paths: BTreeSet<String>,
    /// Task type.
    pub task_type: Option<String>,
    /// Git ref.
    pub git_ref: Option<String>,
    /// Whether running in CI.
    pub is_ci: bool,
    /// Available evidence artifacts.
    pub evidence_artifacts: BTreeMap<String, String>, // path -> hash
    /// Current validation epoch.
    pub validation_epoch: ValidationEpochMetadata,
    /// Policy fingerprint.
    pub policy_fingerprint: String,
    /// Repository whose governed files the evaluators must read.
    pub repo_root: Option<std::path::PathBuf>,
    /// Include terminal publication rules, after required proof execution.
    pub include_publication: bool,
    /// Governing identity evidence must be bound to, when freshness is required.
    pub governing_identity: Option<String>,
    /// Evidence name to the state identity it was bound to.
    pub evidence_identity: BTreeMap<String, String>,
    /// Opaque current-invocation proof. Caller-supplied hashes cannot construct it.
    pub terminal_evidence: Option<TerminalEvidence>,
}

/// Full proof is issued only by the live runtime, after complete-suite verification.
/// Private fields prevent evidence names, marker strings or persisted receipts from
/// impersonating an executed current invocation.
#[derive(Debug, Clone)]
pub struct TerminalEvidence {
    proof: crate::core::validation_history::CompletedReadinessEvidence,
    workunit: Option<crate::core::workunit::WorkUnitManifest>,
}

/// Rule registry for managing validation rules.
pub struct RuleRegistry {
    /// Built-in rules from Decapod constitution.
    builtin_rules: BTreeMap<RuleId, ValidationRule>,
    /// Project override rules from OVERRIDE.md.
    project_rules: BTreeMap<RuleId, ValidationRule>,
    /// Task-specific rules from governed plan.
    task_rules: BTreeMap<RuleId, ValidationRule>,
    /// Resolved effective rules (cached).
    resolved_cache: Option<BTreeMap<RuleId, ResolvedRule>>,
}

impl RuleRegistry {
    /// Create a new rule registry.
    pub fn new() -> Self {
        let mut registry = Self {
            builtin_rules: BTreeMap::new(),
            project_rules: BTreeMap::new(),
            task_rules: BTreeMap::new(),
            resolved_cache: None,
        };
        registry.register_builtin_rules();
        registry
    }

    /// Register Decapod's built-in validation rules.
    fn register_builtin_rules(&mut self) {
        // Rule: Authorized file and directory scope
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.scope.authorized_paths"),
            description:
                "All changed files must be within the authorized scope defined in the governed plan"
                    .to_string(),
            authority: RuleAuthority::Constitution,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec![
                    "feature".to_string(),
                    "bugfix".to_string(),
                    "refactor".to_string(),
                    "docs".to_string(),
                    "test".to_string(),
                ],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::Validation,
            evaluator: "decapod.scope.validator".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec!["governed_plan".to_string()],
                required_evaluators: vec!["scope_validator".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Error,
            remediation: RuleRemediation {
                error_code: "SCOPE_VIOLATION".to_string(),
                violation_description: "Changed files exceed authorized scope".to_string(),
                expected_condition: "All changed files must be within declared scope constraints"
                    .to_string(),
                observed_condition: "Files outside authorized scope were modified".to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Revert changes to files outside authorized scope".to_string(),
                    "If scope expansion is needed, request human approval and update governed plan"
                        .to_string(),
                ],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec!["governed_plan".to_string()],
                related_rules: vec![RuleId::new("decapod.scope.file_touch_budget")],
            },
            override_eligibility: OverrideEligibility::TightenOnly,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: Protected paths and prohibited changes
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.scope.protected_paths"),
            description:
                "Protected paths declared in config.toml and governed plan must not be modified"
                    .to_string(),
            authority: RuleAuthority::Constitution,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec![
                    "feature".to_string(),
                    "bugfix".to_string(),
                    "refactor".to_string(),
                    "docs".to_string(),
                    "test".to_string(),
                ],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::Validation,
            evaluator: "decapod.gatekeeper".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec!["config.toml".to_string(), "governed_plan".to_string()],
                required_evaluators: vec!["gatekeeper".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Critical,
            remediation: RuleRemediation {
                error_code: "PROTECTED_PATH_VIOLATION".to_string(),
                violation_description: "Protected path was modified".to_string(),
                expected_condition: "Protected paths remain unchanged".to_string(),
                observed_condition: "One or more protected paths were modified".to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Revert all changes to protected paths immediately".to_string(),
                    "Protected path modifications require explicit human authorization".to_string(),
                ],
                autonomous_remediation_permitted: false,
                human_authorization_required: true,
                evidence_for_revalidation: vec![
                    "config.toml".to_string(),
                    "governed_plan".to_string(),
                ],
                related_rules: vec![],
            },
            override_eligibility: OverrideEligibility::NonOverridable,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: Valid task ownership and workspace custody
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.custody.task_ownership"),
            description:
                "All work must be performed within a claimed task workspace with valid ownership"
                    .to_string(),
            authority: RuleAuthority::KernelInvariant,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec![
                    "feature".to_string(),
                    "bugfix".to_string(),
                    "refactor".to_string(),
                    "docs".to_string(),
                    "test".to_string(),
                ],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::PreExecution,
            evaluator: "decapod.workspace".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec!["workspace_status".to_string(), "todo_claim".to_string()],
                required_evaluators: vec!["workspace_validator".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Critical,
            remediation: RuleRemediation {
                error_code: "INVALID_WORKSPACE_CUSTODY".to_string(),
                violation_description: "Work performed without valid task ownership".to_string(),
                expected_condition: "All changes made within a claimed task workspace".to_string(),
                observed_condition: "Changes made outside valid workspace or without task claim"
                    .to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Claim a task using `decapod todo claim`".to_string(),
                    "Create workspace using `decapod workspace ensure`".to_string(),
                    "Perform all work within the isolated workspace".to_string(),
                ],
                autonomous_remediation_permitted: false,
                human_authorization_required: false,
                evidence_for_revalidation: vec![
                    "workspace_status".to_string(),
                    "todo_claim".to_string(),
                ],
                related_rules: vec![RuleId::new("decapod.custody.workspace_isolation")],
            },
            override_eligibility: OverrideEligibility::NonOverridable,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: Required validation evidence and evidence freshness
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.evidence.required_and_fresh"),
            description: "Required evidence artifacts must exist and be fresh (not stale)"
                .to_string(),
            authority: RuleAuthority::Constitution,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec![
                    "feature".to_string(),
                    "bugfix".to_string(),
                    "refactor".to_string(),
                    "docs".to_string(),
                    "test".to_string(),
                ],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::Publication,
            evaluator: "decapod.evidence.validator".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec![
                    "validation_proof".to_string(),
                    "trajectory_artifact".to_string(),
                ],
                required_evaluators: vec!["evidence_validator".to_string()],
                evidence_max_age_seconds: 86400, // 24 hours default
                ..Default::default()
            },
            severity: RuleSeverity::Error,
            remediation: RuleRemediation {
                error_code: "STALE_OR_MISSING_EVIDENCE".to_string(),
                violation_description: "Required evidence is missing or stale".to_string(),
                expected_condition: "All required evidence artifacts exist and are fresh"
                    .to_string(),
                observed_condition:
                    "One or more required evidence artifacts are missing or exceed max age"
                        .to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Run `decapod validate` to generate fresh evidence".to_string(),
                    "Ensure all required proof checks pass".to_string(),
                    "Check evidence freshness against validation epoch".to_string(),
                ],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec![
                    "validation_proof".to_string(),
                    "trajectory_artifact".to_string(),
                ],
                related_rules: vec![RuleId::new("decapod.evidence.proof_backed_completion")],
            },
            override_eligibility: OverrideEligibility::ConfigurableOnly,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: Proof-backed completion prerequisites
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.evidence.proof_backed_completion"),
            description: "Task completion requires passing proof-plan gates with valid evidence"
                .to_string(),
            authority: RuleAuthority::KernelInvariant,
            applicability: RuleApplicability::default(),
            evaluation_phase: EvaluationPhase::Publication,
            evaluator: "decapod.workunit".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec![
                    "workunit_manifest".to_string(),
                    "proof_plan_gates".to_string(),
                ],
                required_evaluators: vec!["workunit_validator".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Critical,
            remediation: RuleRemediation {
                error_code: "UNVERIFIED_COMPLETION".to_string(),
                violation_description: "Task completion claimed without passing proof gates"
                    .to_string(),
                expected_condition: "All proof-plan gates passed with valid evidence".to_string(),
                observed_condition: "One or more proof gates failed or lack evidence".to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Run all required proof checks for the task".to_string(),
                    "Ensure each proof gate has valid evidence artifacts".to_string(),
                    "Revalidate with `decapod validate` after fixing failures".to_string(),
                ],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec![
                    "workunit_manifest".to_string(),
                    "proof_plan_gates".to_string(),
                ],
                related_rules: vec![RuleId::new("decapod.evidence.required_and_fresh")],
            },
            override_eligibility: OverrideEligibility::NonOverridable,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: File touch budget enforcement
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.scope.file_touch_budget"),
            description: "Number of files changed must not exceed the declared file touch budget"
                .to_string(),
            authority: RuleAuthority::Constitution,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec![
                    "feature".to_string(),
                    "bugfix".to_string(),
                    "refactor".to_string(),
                    "docs".to_string(),
                    "test".to_string(),
                ],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::Validation,
            evaluator: "decapod.scope.validator".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec!["governed_plan".to_string()],
                required_evaluators: vec!["scope_validator".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Error,
            remediation: RuleRemediation {
                error_code: "FILE_TOUCH_BUDGET_EXCEEDED".to_string(),
                violation_description: "File touch budget exceeded".to_string(),
                expected_condition: "Changed file count within declared budget".to_string(),
                observed_condition: "Changed file count exceeds declared budget".to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Reduce scope of changes to fit within budget".to_string(),
                    "If more changes needed, request human approval to increase budget".to_string(),
                ],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec!["governed_plan".to_string()],
                related_rules: vec![RuleId::new("decapod.scope.authorized_paths")],
            },
            override_eligibility: OverrideEligibility::TightenOnly,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        // Rule: Workspace isolation enforcement
        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.custody.workspace_isolation"),
            description: "Changes must be isolated to the task workspace; root repository must not be modified".to_string(),
            authority: RuleAuthority::KernelInvariant,
            applicability: RuleApplicability {
                required_changed_paths: vec!["*".to_string()],
                task_types: vec!["feature".to_string(), "bugfix".to_string(), "refactor".to_string(), "docs".to_string(), "test".to_string()],
                ..Default::default()
            },
            evaluation_phase: EvaluationPhase::PreExecution,
            evaluator: "decapod.workspace".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec!["workspace_status".to_string()],
                required_evaluators: vec!["workspace_validator".to_string()],
                ..Default::default()
            },
            severity: RuleSeverity::Critical,
            remediation: RuleRemediation {
                error_code: "ROOT_REPOSITORY_MODIFIED".to_string(),
                violation_description: "Root repository was modified directly".to_string(),
                expected_condition: "All changes in isolated workspace only".to_string(),
                observed_condition: "Files in root repository were modified".to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Revert all changes in root repository".to_string(),
                    "Recreate workspace with `decapod workspace ensure`".to_string(),
                    "Perform work only in the isolated workspace".to_string(),
                ],
                autonomous_remediation_permitted: false,
                human_authorization_required: false,
                evidence_for_revalidation: vec!["workspace_status".to_string()],
                related_rules: vec![RuleId::new("decapod.custody.task_ownership")],
            },
            override_eligibility: OverrideEligibility::NonOverridable,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });

        self.register_rule(ValidationRule {
            id: RuleId::new("decapod.publication.fresh_proof"),
            description: "Publication requires fresh proof bound to the current governing identity"
                .to_string(),
            authority: RuleAuthority::KernelInvariant,
            applicability: RuleApplicability::default(),
            evaluation_phase: EvaluationPhase::Publication,
            evaluator: "decapod.publication".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec![
                    "validation_proof".to_string(),
                    "candidate_head".to_string(),
                ],
                ..Default::default()
            },
            severity: RuleSeverity::Critical,
            remediation: RuleRemediation {
                error_code: "PUBLICATION_NOT_PROVEN".to_string(),
                violation_description: "Publication was requested without fresh proof".to_string(),
                expected_condition:
                    "A fresh receipt is bound to the current head, rules, and verifier".to_string(),
                observed_condition: "Publication evidence is missing, stale, or self-reported"
                    .to_string(),
                affected_paths: vec![],
                required_steps: vec![
                    "Revalidate against the current canonical state".to_string(),
                    "Do not treat a status flag as publication eligibility".to_string(),
                ],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec!["validation_proof".to_string()],
                related_rules: vec![RuleId::new("decapod.evidence.proof_backed_completion")],
            },
            override_eligibility: OverrideEligibility::NonOverridable,
            provenance: RuleProvenance {
                source: "embedded/constitution.json".to_string(),
                source_hash: String::new(),
                body_hash: String::new(),
                registered_at: crate::core::time::now_epoch_z(),
                decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
            },
        });
    }

    /// Register a built-in rule.
    fn register_rule(&mut self, rule: ValidationRule) {
        self.builtin_rules.insert(rule.id.clone(), rule);
    }

    /// Load project-specific rules from OVERRIDE.md.
    pub fn load_project_rules(&mut self, repo_root: &Path) -> Result<(), error::DecapodError> {
        let override_path = repo_root.join(".decapod").join("OVERRIDE.md");
        if !override_path.exists() {
            return Ok(());
        }

        let override_content =
            std::fs::read_to_string(&override_path).map_err(error::DecapodError::IoError)?;

        // Parse validation-rules section from OVERRIDE.md
        if let Some(rules_section) = self.extract_validation_rules_section(&override_content) {
            self.parse_project_rules(&rules_section, &override_content)?;
        }

        Ok(())
    }

    /// Extract the validation-rules section from OVERRIDE.md.
    fn extract_validation_rules_section(&self, content: &str) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();
        let mut in_section = false;
        let mut section_lines = Vec::new();

        for line in lines {
            if line.trim() == "## VALIDATION_RULES Overrides"
                || line.trim() == "## Validation Rules Overrides"
            {
                in_section = true;
                continue;
            }
            if in_section && line.starts_with("## ") {
                break;
            }
            if in_section {
                section_lines.push(line);
            }
        }

        if section_lines.is_empty() {
            None
        } else {
            Some(section_lines.join("\n"))
        }
    }

    /// Parse project rules from the validation-rules section.
    fn parse_project_rules(
        &mut self,
        section: &str,
        full_content: &str,
    ) -> Result<(), error::DecapodError> {
        // For now, we'll parse a simple TOML-like format within fenced blocks
        // Each rule override is in a fenced block under a heading like ### decapod.scope.authorized_paths
        let lines: Vec<&str> = section.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i].trim();
            if line.starts_with("### ") {
                let rule_id_str = line.strip_prefix("### ").unwrap().trim();
                if let Some(rule_id) = RuleId::new(rule_id_str).0.strip_prefix("decapod.") {
                    // Look for the fenced block
                    if i + 1 < lines.len() && lines[i + 1].trim().starts_with("````") {
                        let mut j = i + 2;
                        let mut rule_content = Vec::new();
                        while j < lines.len() && !lines[j].trim().starts_with("````") {
                            rule_content.push(lines[j]);
                            j += 1;
                        }
                        if j < lines.len() {
                            let content = rule_content.join("\n");
                            self.apply_rule_override(rule_id, &content, full_content)?;
                            i = j;
                        }
                    }
                }
            }
            i += 1;
        }

        Ok(())
    }

    /// Apply a rule override from OVERRIDE.md.
    fn apply_rule_override(
        &mut self,
        rule_id_suffix: &str,
        override_content: &str,
        full_content: &str,
    ) -> Result<(), error::DecapodError> {
        // Find the base rule (builtin or project)
        let full_id = format!("decapod.{}", rule_id_suffix);
        let rule_id = RuleId::new(&full_id);

        // Get the base rule to override
        let base_rule = self
            .builtin_rules
            .get(&rule_id)
            .or_else(|| self.project_rules.get(&rule_id))
            .cloned();

        let Some(mut base_rule) = base_rule else {
            // Unknown rule ID - this is an error
            return Err(error::DecapodError::ValidationError(format!(
                "OVERRIDE_UNKNOWN_RULE: '{}' is not a recognized validation rule ID",
                full_id
            )));
        };

        // Parse override content (TOML format within fenced block)
        let override_values = parse_override_content(override_content)?;

        // Apply severity override if present
        if let Some(severity) = override_values.severity {
            // Check override eligibility for severity changes
            match base_rule.override_eligibility {
                OverrideEligibility::NonOverridable => {
                    return Err(error::DecapodError::ValidationError(format!(
                        "OVERRIDE_NOT_PERMITTED: Rule '{}' severity cannot be overridden (kernel invariant)",
                        full_id
                    )));
                }
                OverrideEligibility::TightenOnly => {
                    // Only allow increasing severity
                    if severity > base_rule.severity {
                        base_rule.severity = severity;
                    }
                }
                _ => {
                    base_rule.severity = severity;
                }
            }
        }

        // Apply applicability override if present
        if let Some(applicability) = override_values.applicability {
            match base_rule.override_eligibility {
                OverrideEligibility::NonOverridable => {
                    // Non-overridable rules cannot have applicability changed
                }
                OverrideEligibility::TightenOnly => {
                    // Only allow adding constraints
                    base_rule
                        .applicability
                        .required_changed_paths
                        .extend(applicability.required_changed_paths);
                    base_rule
                        .applicability
                        .forbidden_changed_paths
                        .extend(applicability.forbidden_changed_paths);
                    if !applicability.task_types.is_empty() {
                        base_rule.applicability.task_types = applicability.task_types;
                    }
                }
                _ => {
                    base_rule.applicability = applicability;
                }
            }
        }

        // Apply requirements override if present
        if let Some(requirements) = override_values.requirements {
            match base_rule.override_eligibility {
                OverrideEligibility::NonOverridable => {}
                OverrideEligibility::TightenOnly => {
                    base_rule
                        .requirements
                        .required_evidence
                        .extend(requirements.required_evidence);
                    base_rule
                        .requirements
                        .required_evaluators
                        .extend(requirements.required_evaluators);
                    if requirements.evidence_max_age_seconds
                        > base_rule.requirements.evidence_max_age_seconds
                    {
                        base_rule.requirements.evidence_max_age_seconds =
                            requirements.evidence_max_age_seconds;
                    }
                }
                _ => {
                    base_rule.requirements = requirements;
                }
            }
        }

        // Apply remediation override if present
        if let Some(remediation) = override_values.remediation {
            match base_rule.override_eligibility {
                OverrideEligibility::NonOverridable => {}
                OverrideEligibility::TightenOnly => {
                    // Only allow tightening remediation (more specific steps)
                    base_rule
                        .remediation
                        .required_steps
                        .extend(remediation.required_steps);
                }
                _ => {
                    base_rule.remediation = remediation;
                }
            };
        }

        // Update provenance
        let source_hash = format!("{:x}", Sha256::digest(full_content.as_bytes()));
        let body_hash = format!("{:x}", Sha256::digest(override_content.as_bytes()));
        base_rule.provenance = RuleProvenance {
            source: ".decapod/OVERRIDE.md".to_string(),
            source_hash,
            body_hash,
            registered_at: crate::core::time::now_epoch_z(),
            decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION.to_string(),
        };
        base_rule.authority = RuleAuthority::ProjectOverride;

        self.project_rules.insert(rule_id, base_rule);
        self.resolved_cache = None; // Invalidate cache

        Ok(())
    }

    /// Load task-specific rules from governed plan.
    pub fn load_task_rules(&mut self, project_root: &Path) -> Result<(), error::DecapodError> {
        let governance_path = project_root.join(".decapod").join("governance.json");
        if !governance_path.exists() {
            return Ok(());
        }

        let content =
            std::fs::read_to_string(&governance_path).map_err(error::DecapodError::IoError)?;
        let governance: serde_json::Value = serde_json::from_str(&content).map_err(|e| {
            error::DecapodError::ValidationError(format!("Invalid governance.json: {}", e))
        })?;

        // Extract plan constraints as rules
        if let Some(plan) = governance.pointer("/sections/plan")
            && let Some(constraints) = plan.pointer("/constraints")
        {
            if let Some(forbidden_paths) = constraints
                .pointer("/forbidden_paths")
                .and_then(|v| v.as_array())
            {
                let paths: Vec<String> = forbidden_paths
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect();

                if !paths.is_empty() {
                    let rule = ValidationRule {
                        id: RuleId::new("decapod.task.forbidden_paths"),
                        description: "Task-specific forbidden paths from governed plan".to_string(),
                        authority: RuleAuthority::TaskConstraints,
                        applicability: RuleApplicability {
                            required_changed_paths: vec!["*".to_string()],
                            forbidden_changed_paths: paths,
                            ..Default::default()
                        },
                        evaluation_phase: EvaluationPhase::DuringExecution,
                        evaluator: "decapod.scope.validator".to_string(),
                        requirements: RuleRequirements {
                            required_evidence: vec!["governed_plan".to_string()],
                            required_evaluators: vec!["scope_validator".to_string()],
                            ..Default::default()
                        },
                        severity: RuleSeverity::Error,
                        remediation: RuleRemediation {
                            error_code: "TASK_FORBIDDEN_PATH_VIOLATION".to_string(),
                            violation_description: "Task-specific forbidden path was modified"
                                .to_string(),
                            expected_condition: "Task forbidden paths remain unchanged".to_string(),
                            observed_condition: "One or more task-forbidden paths were modified"
                                .to_string(),
                            affected_paths: vec![],
                            required_steps: vec![
                                "Revert changes to task-forbidden paths".to_string(),
                                "Consult governed plan for authorized scope".to_string(),
                            ],
                            autonomous_remediation_permitted: true,
                            human_authorization_required: false,
                            evidence_for_revalidation: vec!["governed_plan".to_string()],
                            related_rules: vec![RuleId::new("decapod.scope.protected_paths")],
                        },
                        override_eligibility: OverrideEligibility::NonOverridable,
                        provenance: RuleProvenance {
                            source: ".decapod/governance.json".to_string(),
                            source_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
                            body_hash: String::new(),
                            registered_at: crate::core::time::now_epoch_z(),
                            decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION
                                .to_string(),
                        },
                    };
                    self.task_rules.insert(rule.id.clone(), rule);
                    self.resolved_cache = None;
                }
            }

            if let Some(_file_touch_budget) = constraints
                .pointer("/file_touch_budget")
                .and_then(|v| v.as_u64())
            {
                // The budget rule already exists; we could tighten it
                if let Some(existing) = self
                    .builtin_rules
                    .get(&RuleId::new("decapod.scope.file_touch_budget"))
                {
                    // Create a tightened version
                    let mut tightened = existing.clone();
                    tightened.applicability.required_changed_paths = vec!["*".to_string()];
                    // Note: actual budget enforcement is in the evaluator
                    tightened.authority = RuleAuthority::TaskConstraints;
                    tightened.provenance = RuleProvenance {
                        source: ".decapod/governance.json".to_string(),
                        source_hash: format!("{:x}", Sha256::digest(content.as_bytes())),
                        body_hash: String::new(),
                        registered_at: crate::core::time::now_epoch_z(),
                        decapod_version: crate::core::entrypoint_integrity::RELEASE_VERSION
                            .to_string(),
                    };
                    self.task_rules.insert(tightened.id.clone(), tightened);
                    self.resolved_cache = None;
                }
            }
        }

        Ok(())
    }

    /// Resolve the effective rule set (applying overrides with precedence).
    #[allow(
        clippy::unnecessary_unwrap,
        reason = "Cache checked with is_some() or populated by build_and_cache_rules"
    )]
    pub fn resolve_effective_rules(&mut self) -> &BTreeMap<RuleId, ResolvedRule> {
        // Fast path: return cached if available
        if self.resolved_cache.is_some() {
            return self.resolved_cache.as_ref().unwrap();
        }

        // Slow path: build and cache
        self.build_and_cache_rules();
        self.resolved_cache.as_ref().unwrap()
    }

    fn build_and_cache_rules(&mut self) {
        let mut resolved = BTreeMap::new();

        // Start with built-in rules
        for (id, rule) in &self.builtin_rules {
            resolved.insert(
                id.clone(),
                ResolvedRule {
                    rule: rule.clone(),
                    override_values: None,
                    effective_severity: rule.severity.clone(),
                    effective_applicability: rule.applicability.clone(),
                },
            );
        }

        // Apply project overrides (can tighten or configure)
        for (id, project_rule) in &self.project_rules {
            if let Some(existing) = resolved.get_mut(id) {
                // Check override eligibility
                match existing.rule.override_eligibility {
                    OverrideEligibility::NonOverridable => {
                        // Skip - cannot override
                        continue;
                    }
                    OverrideEligibility::TightenOnly => {
                        // Only allow increasing severity and adding constraints.
                        // The builtin rule body stays in place so a project copy
                        // cannot replace the evaluator or authority.
                        if project_rule.severity > existing.rule.severity {
                            existing.effective_severity = project_rule.severity.clone();
                        }
                        existing
                            .effective_applicability
                            .required_changed_paths
                            .extend(project_rule.applicability.required_changed_paths.clone());
                        existing
                            .effective_applicability
                            .forbidden_changed_paths
                            .extend(project_rule.applicability.forbidden_changed_paths.clone());
                        extend_requirements(
                            &mut existing.rule.requirements,
                            &project_rule.requirements,
                        );
                        existing.override_values = Some(RuleOverrideValues {
                            severity: Some(project_rule.severity.clone()),
                            applicability: Some(project_rule.applicability.clone()),
                            ..Default::default()
                        });
                    }
                    OverrideEligibility::ConfigurableOnly => {
                        existing.effective_severity = project_rule.severity.clone();
                        existing.rule.severity = project_rule.severity.clone();
                        existing.rule.requirements = project_rule.requirements.clone();
                        existing.override_values = Some(RuleOverrideValues {
                            severity: Some(project_rule.severity.clone()),
                            requirements: Some(project_rule.requirements.clone()),
                            ..Default::default()
                        });
                    }
                    OverrideEligibility::FullyOverridable => {
                        existing.effective_severity = project_rule.severity.clone();
                        existing.effective_applicability = project_rule.applicability.clone();
                        existing.override_values = Some(RuleOverrideValues {
                            severity: Some(project_rule.severity.clone()),
                            applicability: Some(project_rule.applicability.clone()),
                            requirements: Some(project_rule.requirements.clone()),
                            remediation: Some(project_rule.remediation.clone()),
                        });
                        existing.rule = project_rule.clone();
                    }
                }
            } else {
                // New rule from project override (additive)
                resolved.insert(
                    id.clone(),
                    ResolvedRule {
                        rule: project_rule.clone(),
                        override_values: None,
                        effective_severity: project_rule.severity.clone(),
                        effective_applicability: project_rule.applicability.clone(),
                    },
                );
            }
        }

        // Apply task constraints (highest precedence for applicable rules)
        for (id, task_rule) in &self.task_rules {
            if let Some(existing) = resolved.get_mut(id) {
                // Task constraints cannot replace a kernel rule body.
                if existing.rule.override_eligibility == OverrideEligibility::NonOverridable {
                    continue;
                }
                if task_rule.severity > existing.effective_severity {
                    existing.effective_severity = task_rule.severity.clone();
                }
                existing
                    .effective_applicability
                    .required_changed_paths
                    .extend(task_rule.applicability.required_changed_paths.clone());
                existing
                    .effective_applicability
                    .forbidden_changed_paths
                    .extend(task_rule.applicability.forbidden_changed_paths.clone());
                extend_requirements(&mut existing.rule.requirements, &task_rule.requirements);
                existing.override_values = Some(RuleOverrideValues {
                    severity: Some(task_rule.severity.clone()),
                    applicability: Some(task_rule.applicability.clone()),
                    ..Default::default()
                });
            } else {
                // New task-specific rule
                resolved.insert(
                    id.clone(),
                    ResolvedRule {
                        rule: task_rule.clone(),
                        override_values: None,
                        effective_severity: task_rule.severity.clone(),
                        effective_applicability: task_rule.applicability.clone(),
                    },
                );
            }
        }

        self.resolved_cache = Some(resolved);
    }

    /// Get all applicable rules for a given context.
    pub fn get_applicable_rules(&mut self, context: &RuleEvaluationContext) -> Vec<ResolvedRule> {
        let resolved = self.resolve_effective_rules();
        resolved
            .values()
            .filter(|r| {
                if r.rule.evaluation_phase == EvaluationPhase::Publication
                    && !context.include_publication
                {
                    return false;
                }
                // Ordinary tasks need full validation, but do not implicitly
                // declare the separate optional workunit completion protocol.
                if r.rule.id.as_str() == "decapod.evidence.proof_backed_completion"
                    && r.rule.authority == RuleAuthority::KernelInvariant
                    && context
                        .terminal_evidence
                        .as_ref()
                        .is_some_and(|proof| proof.workunit.is_none())
                {
                    return false;
                }
                r.rule.is_applicable(
                    &context.changed_paths,
                    context.task_type.as_deref(),
                    context.git_ref.as_deref(),
                    context.is_ci,
                )
            })
            .cloned()
            .collect()
    }

    /// Get a specific rule by ID.
    pub fn get_rule(&mut self, id: &RuleId) -> Option<ResolvedRule> {
        self.resolve_effective_rules().get(id).cloned()
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn extend_requirements(target: &mut RuleRequirements, extra: &RuleRequirements) {
    target
        .required_evidence
        .extend(extra.required_evidence.clone());
    target.required_evidence.sort();
    target.required_evidence.dedup();
    target
        .required_evaluators
        .extend(extra.required_evaluators.clone());
    target.required_evaluators.sort();
    target.required_evaluators.dedup();
    if extra.evidence_max_age_seconds > target.evidence_max_age_seconds {
        target.evidence_max_age_seconds = extra.evidence_max_age_seconds;
    }
}

/// Evaluate a rule against the current context.
pub fn evaluate_rule(rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleEvaluationResult {
    // For now, we'll implement a basic evaluator that checks applicability
    // and delegates to the actual evaluator implementations
    let evaluator_name = &rule.rule.evaluator;

    // Check if required evidence is available
    let mut missing_evidence = Vec::new();
    for evidence in &rule.rule.requirements.required_evidence {
        if !context.evidence_artifacts.contains_key(evidence) {
            missing_evidence.push(evidence.clone());
        }
    }

    let mut stale_evidence = Vec::new();
    if let Some(governing) = context.governing_identity.as_deref() {
        for evidence in &rule.rule.requirements.required_evidence {
            match context.evidence_identity.get(evidence).map(String::as_str) {
                Some(bound) if bound != governing => stale_evidence.push(evidence.clone()),
                Some(_) => {}
                None if context.evidence_artifacts.contains_key(evidence) => {
                    missing_evidence.push(format!("{evidence} governing identity"));
                }
                None => {}
            }
        }
    }

    let verdict = if rule.rule.forbidden_scope_hit(&context.changed_paths)
        && matches!(
            rule.effective_severity,
            RuleSeverity::Error | RuleSeverity::Critical
        ) {
        RuleVerdict::Fail
    } else if !stale_evidence.is_empty() {
        RuleVerdict::Stale
    } else if !missing_evidence.is_empty() {
        RuleVerdict::Unverifiable
    } else {
        match evaluator_name.as_str() {
            "decapod.scope.validator" => evaluate_scope_rule(rule, context),
            "decapod.gatekeeper" => evaluate_gatekeeper_rule(rule, context),
            "decapod.workspace" => evaluate_workspace_rule(rule, context),
            "decapod.evidence.validator" => evaluate_evidence_rule(rule, context),
            "decapod.workunit" => evaluate_workunit_rule(rule, context),
            "decapod.publication" => evaluate_publication_rule(context),
            _ => RuleVerdict::Unverifiable,
        }
    };

    let severity = rule.effective_severity.clone();
    let remediation = if matches!(
        verdict,
        RuleVerdict::Fail | RuleVerdict::Unverifiable | RuleVerdict::Stale
    ) {
        Some(rule.rule.remediation.clone())
    } else {
        None
    };

    let verdict_for_message = verdict.clone();

    RuleEvaluationResult {
        rule_id: rule.rule.id.clone(),
        verdict,
        severity,
        evaluated_at: crate::core::time::now_epoch_z(),
        evidence_refs: context.evidence_artifacts.keys().cloned().collect(),
        message: match verdict_for_message {
            RuleVerdict::Pass => "Rule passed".to_string(),
            RuleVerdict::Fail => format!(
                "Rule violated: {}",
                rule.rule.remediation.violation_description
            ),
            RuleVerdict::Stale => format!(
                "Rule stale: evidence bound to a different governing identity ({})",
                stale_evidence.join(", ")
            ),
            RuleVerdict::Unverifiable => {
                if missing_evidence.is_empty() {
                    "Rule unverifiable: evidence does not establish the claim".to_string()
                } else {
                    format!(
                        "Rule unverifiable: missing evidence {}",
                        missing_evidence.join(", ")
                    )
                }
            }
        },
        remediation,
        policy_fingerprint: context.policy_fingerprint.clone(),
        validation_epoch: context.validation_epoch.clone(),
    }
}

/// Evaluate scope rule using existing plan governance.
fn evaluate_scope_rule(rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    let Some(repo_root) = context.repo_root.as_deref() else {
        return RuleVerdict::Unverifiable;
    };
    let governance_path = repo_root.join(".decapod").join("governance.json");

    let file_touch_budget = if governance_path.exists() {
        let content = match std::fs::read_to_string(&governance_path) {
            Ok(c) => c,
            Err(_) => return RuleVerdict::Unverifiable,
        };
        let governance: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return RuleVerdict::Unverifiable,
        };
        governance
            .pointer("/sections/plan/constraints/file_touch_budget")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
    } else {
        None
    };

    let constraints = crate::plan_governance::ScopeConstraints {
        forbidden_paths: rule.effective_applicability.forbidden_changed_paths.clone(),
        file_touch_budget,
    };

    match crate::plan_governance::enforce_scope_paths(&constraints, &context.changed_paths) {
        Ok(()) => RuleVerdict::Pass,
        Err(_) => RuleVerdict::Fail,
    }
}

/// Evaluate gatekeeper rule using path, diff-ceiling, and governed-plan protected-path checks.
///
/// Full-file secret and dangerous-pattern scanning runs on staged files in
/// `validate_gatekeeper_gate`; `decapod.scope.protected_paths` specifically
/// enforces protected/blocked/forbidden path boundaries and diff size over the
/// branch's `changed_paths` without failing on pre-existing patterns in touched files.
fn evaluate_gatekeeper_rule(_rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    // Check if we have the required evidence for gatekeeper
    if !context.evidence_artifacts.contains_key("config.toml") {
        return RuleVerdict::Unverifiable;
    }

    // Gatekeeper only applies in governed agent workspaces with active task claims
    if !context.evidence_artifacts.contains_key("workspace_status") {
        return RuleVerdict::Unverifiable;
    }

    let Some(repo_root) = context.repo_root.as_deref() else {
        return RuleVerdict::Unverifiable;
    };
    match crate::core::workspace::get_workspace_status(repo_root) {
        Ok(status) => {
            // Gatekeeper only applies in governed agent workspaces with active task claims
            if !status.can_work || !status.git.in_worktree || status.git.is_protected {
                return RuleVerdict::Unverifiable;
            }
        }
        Err(_) => return RuleVerdict::Unverifiable,
    }

    // Calculate diff size using git diff
    let diff_bytes = calculate_diff_size(repo_root).unwrap_or(0);
    evaluate_gatekeeper_paths_and_diff(repo_root, &context.changed_paths, diff_bytes)
}

fn evaluate_gatekeeper_paths_and_diff(
    repo_root: &std::path::Path,
    changed_paths: &BTreeSet<String>,
    diff_bytes: u64,
) -> RuleVerdict {
    match crate::plan_governance::load_plan(repo_root) {
        Ok(Some(plan)) if !plan.constraints.forbidden_paths.is_empty() => {
            let constraints = crate::plan_governance::ScopeConstraints {
                forbidden_paths: plan.constraints.forbidden_paths,
                file_touch_budget: None,
            };
            if crate::plan_governance::enforce_scope_paths(&constraints, changed_paths).is_err() {
                return RuleVerdict::Fail;
            }
        }
        Ok(_) => {}
        Err(_) => return RuleVerdict::Unverifiable,
    }

    let paths: Vec<std::path::PathBuf> =
        changed_paths.iter().map(std::path::PathBuf::from).collect();

    let mut config = crate::core::gatekeeper::GatekeeperConfig::from_repo_config(repo_root);
    config.scan_secrets = false;
    config.scan_dangerous_patterns = false;

    match crate::core::gatekeeper::run_gatekeeper(repo_root, &paths, diff_bytes, &config) {
        Ok(result) => {
            let has_path_or_diff_violation =
                result
                    .violations
                    .iter()
                    .any(|violation| match violation.kind {
                        crate::core::gatekeeper::ViolationKind::DiffTooLarge => true,
                        crate::core::gatekeeper::ViolationKind::ProtectedPath
                        | crate::core::gatekeeper::ViolationKind::PathBlocked => {
                            changed_paths.contains(violation.path.to_string_lossy().as_ref())
                        }
                        crate::core::gatekeeper::ViolationKind::SecretDetected
                        | crate::core::gatekeeper::ViolationKind::DangerousPattern => false,
                    });
            if has_path_or_diff_violation {
                RuleVerdict::Fail
            } else {
                RuleVerdict::Pass
            }
        }
        Err(_) => RuleVerdict::Unverifiable,
    }
}

/// Calculate the total diff size in bytes for the current changes.
fn calculate_diff_size(repo_root: &std::path::Path) -> Option<u64> {
    use std::process::Command;

    // Get the base branch
    let base_branch = crate::core::workspace::resolve_base_branch(repo_root, None);
    let base = crate::core::workspace::base_ref_for_branch(repo_root, &base_branch)?;

    // Get diff stats
    let output = Command::new("git")
        .args(["diff", "--numstat", &base, "HEAD", "--"])
        .current_dir(repo_root)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut total_size = 0u64;

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            if let Ok(added) = parts[0].parse::<u64>() {
                total_size += added;
            }
            if let Ok(deleted) = parts[1].parse::<u64>() {
                total_size += deleted;
            }
        }
    }

    Some(total_size)
}

/// Evaluate workspace rule using actual workspace checks.
fn evaluate_workspace_rule(_rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    // Check workspace isolation and task ownership
    if !context.evidence_artifacts.contains_key("workspace_status") {
        return RuleVerdict::Unverifiable;
    }

    let Some(repo_root) = context.repo_root.as_deref() else {
        return RuleVerdict::Unverifiable;
    };
    match crate::core::workspace::get_workspace_status(repo_root) {
        Ok(status) => {
            // Custody rules only apply in governed agent workspaces with active task claims
            if status.can_work && status.git.in_worktree && !status.git.is_protected {
                RuleVerdict::Pass
            } else {
                // Not in a governed agent workspace - rule not applicable
                RuleVerdict::Unverifiable
            }
        }
        Err(_) => RuleVerdict::Unverifiable,
    }
}

/// Evaluate evidence rule using actual evidence validation.
fn evaluate_evidence_rule(rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    if rule.rule.evaluation_phase == EvaluationPhase::Publication
        && context
            .terminal_evidence
            .as_ref()
            .is_none_or(|proof| proof.proof.verify().is_err())
    {
        return RuleVerdict::Unverifiable;
    }
    // Check required evidence exists
    let mut missing = Vec::new();
    for evidence in &rule.rule.requirements.required_evidence {
        if !context.evidence_artifacts.contains_key(evidence) {
            missing.push(evidence.clone());
        }
    }

    if !missing.is_empty() {
        return RuleVerdict::Unverifiable;
    }

    // Check evidence freshness if max_age is set
    // Runtime-owned proof was produced by this invocation. Source/governance
    // bindings, rather than the mtime of an older authored trajectory, establish
    // its freshness. Legacy file evidence retains its bounded-age check.
    if context.terminal_evidence.is_none() && rule.rule.requirements.evidence_max_age_seconds > 0 {
        let Some(repo_root) = context.repo_root.as_deref() else {
            return RuleVerdict::Unverifiable;
        };
        let max_age =
            std::time::Duration::from_secs(rule.rule.requirements.evidence_max_age_seconds);
        let now = std::time::SystemTime::now();

        for evidence in &rule.rule.requirements.required_evidence {
            let path = match evidence.as_str() {
                "validation_receipt" | "trajectory_artifact" | "governed_plan" => {
                    repo_root.join(".decapod").join("governance.json")
                }
                "workunit_manifest" => {
                    // Check the most recent workunit manifest
                    let workunits_dir = repo_root.join(".decapod").join("data").join("workunits");
                    if workunits_dir.exists() {
                        match std::fs::read_dir(&workunits_dir) {
                            Ok(entries) => {
                                let mut latest = None;
                                for entry in entries.flatten() {
                                    if let Ok(metadata) = entry.metadata()
                                        && let Ok(modified) = metadata.modified()
                                        && latest.is_none_or(|m| modified > m)
                                    {
                                        latest = Some(modified);
                                    }
                                }
                                if let Some(modified) = latest
                                    && now.duration_since(modified).unwrap_or(max_age) > max_age
                                {
                                    return RuleVerdict::Unverifiable;
                                }
                                continue;
                            }
                            Err(_) => continue,
                        }
                    } else {
                        continue;
                    }
                }
                "workspace_status" | "todo_claim" => continue, // These are runtime state, not files
                _ => continue,
            };

            if path.exists()
                && let Ok(metadata) = std::fs::metadata(&path)
                && let Ok(modified) = metadata.modified()
                && now.duration_since(modified).unwrap_or(max_age) > max_age
            {
                return RuleVerdict::Unverifiable;
            }
        }
    }

    if rule
        .rule
        .requirements
        .required_evidence
        .iter()
        .any(|evidence| {
            context
                .evidence_artifacts
                .get(evidence)
                .is_none_or(|value| !evidence_digest_ok(value))
        })
    {
        return RuleVerdict::Unverifiable;
    }

    RuleVerdict::Pass
}

fn evidence_digest_ok(value: &str) -> bool {
    let body = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
    body.len() >= 8 && body.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Evaluate workunit rule using actual workunit validation.
fn evaluate_workunit_rule(_rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    let Some(terminal) = &context.terminal_evidence else {
        return RuleVerdict::Unverifiable;
    };
    let Some(manifest) = &terminal.workunit else {
        return RuleVerdict::Unverifiable;
    };
    let Some(root) = context.repo_root.as_deref() else {
        return RuleVerdict::Unverifiable;
    };
    // This is the current complete runtime observation, before its canonical
    // append. It must not depend on an agent's pass row or an earlier VERIFIED
    // projection: first proof would otherwise become circular. Durable subject
    // binding is checked by load_terminal_workunit and again by the producer.
    if !matches!(
        manifest.status,
        crate::core::workunit::WorkUnitStatus::Claimed
            | crate::core::workunit::WorkUnitStatus::Verified
    ) || terminal.proof.verify().is_err()
        || crate::core::validation_history::ensure_full_validation_plan(&manifest.proof_plan)
            .is_err()
        || crate::core::workunit::ensure_living_specs_bound_for_proof(root, manifest).is_err()
        || crate::core::workunit::verify_capsule_policy_lineage_for_task(root, manifest).is_err()
    {
        return RuleVerdict::Unverifiable;
    }
    RuleVerdict::Pass
}

fn evaluate_publication_rule(context: &RuleEvaluationContext) -> RuleVerdict {
    match &context.terminal_evidence {
        Some(terminal) if terminal.proof.verify().is_ok() => RuleVerdict::Pass,
        _ => RuleVerdict::Unverifiable,
    }
}

#[cfg(test)]
pub(crate) fn load_terminal_workunit(
    root: &Path,
    task_id: &str,
) -> Result<Option<crate::core::workunit::WorkUnitManifest>, error::DecapodError> {
    load_terminal_workunit_with_budget(root, task_id, &|| Ok(()))
}

fn load_terminal_workunit_with_budget(
    root: &Path,
    task_id: &str,
    check_budget: &dyn Fn() -> Result<(), error::DecapodError>,
) -> Result<Option<crate::core::workunit::WorkUnitManifest>, error::DecapodError> {
    check_budget()?;
    // The durable journal distinguishes an undeclared protocol from a deleted
    // or invalid projection. Lookup errors never become an exemption.
    let status = crate::core::workunit::workunit_journal_status(root, task_id)?;
    check_budget()?;
    if status.projection_state == "absent" {
        for path in [
            crate::core::workunit::workunit_path(root, task_id)?,
            root.join(".decapod/governance/workunits")
                .join(format!("{task_id}.json")),
        ] {
            match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error::DecapodError::IoError(error)),
                Ok(_) => {
                    return Err(error::DecapodError::ValidationError(
                        "TERMINAL_WORKUNIT_UNPROVEN: unknown manifest projection".into(),
                    ));
                }
            }
        }
        return Ok(None);
    }
    if !status.resumable {
        return Err(error::DecapodError::ValidationError(format!(
            "TERMINAL_WORKUNIT_UNPROVEN: {} ({})",
            task_id,
            status
                .resume_blocker
                .as_deref()
                .unwrap_or(&status.projection_state)
        )));
    }
    let manifest = crate::core::workunit::load_workunit(root, task_id)?;
    check_budget()?;
    let current = crate::core::workunit::workunit_journal_status(root, task_id)?;
    check_budget()?;
    if !current.resumable
        || current.generation != status.generation
        || current.checkpoint_hash != status.checkpoint_hash
    {
        return Err(error::DecapodError::ValidationError(
            "TERMINAL_WORKUNIT_STALE: journal changed while collecting proof".into(),
        ));
    }
    Ok(Some(manifest))
}

/// Normalize a task category or task ID prefix to the canonical validation rule
/// `task_type` vocabulary (`"feature"`, `"bugfix"`, `"refactor"`, `"docs"`, `"test"`).
///
/// Unrecognized or administrative categories (such as `"code"`, `"cicd"`, `"secu"`,
/// or `"task"`) return `None` so rules restricting `task_types` remain applicable
/// rather than silently exempted.
pub(crate) fn normalize_rule_task_type(
    category: Option<&str>,
    task_id: Option<&str>,
) -> Option<String> {
    let map_token = |raw: &str| match raw.trim().to_ascii_lowercase().as_str() {
        "feature" | "features" | "feat" => Some("feature".to_string()),
        "bugfix" | "bug" | "bugs" => Some("bugfix".to_string()),
        "refactor" | "refa" => Some("refactor".to_string()),
        "docs" | "doc" | "documentation" => Some("docs".to_string()),
        "test" | "tests" | "testing" => Some("test".to_string()),
        _ => None,
    };
    category.and_then(map_token).or_else(|| {
        task_id
            .and_then(|id| id.trim().split_once('_').map(|(prefix, _)| prefix))
            .and_then(map_token)
    })
}

/// Normalize a branch or full git reference into a canonical `refs/...` string
/// matching `RuleApplicability::ref_patterns` and `git symbolic-ref HEAD`.
pub(crate) fn canonical_git_ref(work_branch: &str) -> Option<String> {
    let trimmed = work_branch.trim();
    if trimmed.is_empty() || trimmed == "HEAD" {
        None
    } else if trimmed.starts_with("refs/") {
        Some(trimmed.to_string())
    } else {
        Some(format!("refs/heads/{trimmed}"))
    }
}

/// The terminal production boundary runs before any readiness receipt/history
/// is issued. It receives the runtime-owned result rather than importing a pass
/// from a path, a legacy prerequisite receipt, or caller-authored digest strings.
pub(crate) fn evaluate_terminal_rules(
    root: &Path,
    proof: &crate::core::validation_history::CompletedReadinessEvidence,
    material: &crate::core::readiness_evidence::MaterialIdentity,
    governance_digest: &str,
    trajectory_digest: &str,
    budget: &crate::core::validation_runtime::RuntimeBudget<'_>,
) -> Result<RulesValidationReport, error::DecapodError> {
    let check_budget = || {
        budget
            .remaining()
            .map(|_| ())
            .map_err(error::DecapodError::IoError)
    };
    check_budget()?;
    proof.verify_current(material, governance_digest, trajectory_digest)?;
    // Re-read the canonical bytes at this terminal boundary, including the
    // compact stored trajectory. Expanded read projections are not its digest.
    let input = crate::core::validation_epoch::input::BoundedEpochInput::new(root)?;
    use std::io::Read;
    let mut bytes = Vec::new();
    input
        .open_regular(&root.join(".decapod/governance.json"))?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(error::DecapodError::ValidationError(
            "terminal governance exceeds byte bound".into(),
        ));
    }
    let document: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    let current_governance = format!("sha256:{:x}", Sha256::digest(&bytes));
    let current_trajectory =
        crate::core::readiness_evidence::digest(&document["sections"]["trajectory"])
            .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
    proof.verify_current(material, &current_governance, &current_trajectory)?;
    check_budget()?;
    let governance = crate::core::governance_document::read_section(root, "trajectory")?;
    let branch_task_ids =
        crate::core::workspace::extract_task_ids_from_branch(&material.work_branch);
    let task_id = governance
        .as_ref()
        .and_then(|value| value.get("task_id"))
        .and_then(|value| value.as_str())
        .or_else(|| branch_task_ids.first().map(String::as_str));
    let task_category = task_id.and_then(|id| {
        let main_root = crate::core::workspace::get_main_repo_root(root).ok()?;
        let store_root = main_root.join(".decapod").join("data");
        if store_root.join("decapod.db").is_file() && !store_root.join("todo.db").exists() {
            crate::core::todo::get_task(&store_root, id)
                .ok()
                .flatten()
                .map(|task| task.category)
        } else {
            None
        }
    });
    let workunit = task_id
        .map(|id| load_terminal_workunit_with_budget(root, id, &check_budget))
        .transpose()?
        .flatten();
    let mut artifacts: BTreeMap<String, String> = BTreeMap::from([
        (
            "validation_proof".into(),
            crate::core::readiness_evidence::digest(proof)
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
        ),
        ("trajectory_artifact".into(), trajectory_digest.into()),
        ("candidate_head".into(), material.head_revision.clone()),
    ]);
    if let Some(manifest) = &workunit {
        artifacts.insert(
            "workunit_manifest".into(),
            manifest
                .canonical_hash_hex()
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
        );
        artifacts.insert(
            "proof_plan_gates".into(),
            crate::core::readiness_evidence::digest(&manifest.proof_results)
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
        );
    }
    let identities = artifacts
        .keys()
        .map(|key| (key.clone(), governance_digest.into()))
        .collect();
    check_budget()?;
    let context = RuleEvaluationContext {
        changed_paths: proof.changed_paths().clone(),
        task_type: normalize_rule_task_type(task_category.as_deref(), task_id),
        git_ref: canonical_git_ref(&material.work_branch),
        is_ci: std::env::var("GITHUB_ACTIONS").is_ok() || std::env::var("BUILDKITE").is_ok(),
        evidence_artifacts: artifacts,
        validation_epoch: crate::core::validation_epoch::active_validation_epoch_bounded(root)?,
        policy_fingerprint: governance_digest.into(),
        repo_root: Some(root.into()),
        include_publication: true,
        governing_identity: Some(governance_digest.into()),
        evidence_identity: identities,
        terminal_evidence: Some(TerminalEvidence {
            proof: proof.clone(),
            workunit,
        }),
    };
    let mut registry = RuleRegistry::new();
    registry.load_project_rules(root)?;
    registry.load_task_rules(root)?;
    check_budget()?;
    let report = RulesValidationReport::from_results(
        registry
            .get_applicable_rules(&context)
            .iter()
            .filter(|rule| rule.rule.evaluation_phase == EvaluationPhase::Publication)
            .map(|rule| evaluate_rule(rule, &context))
            .collect(),
    );
    check_budget()?;
    Ok(report)
}

/// Evaluate all applicable rules.
pub fn evaluate_all_rules(
    registry: &mut RuleRegistry,
    context: &RuleEvaluationContext,
) -> Vec<RuleEvaluationResult> {
    let applicable_rules = registry.get_applicable_rules(context);
    applicable_rules
        .iter()
        .map(|rule| evaluate_rule(rule, context))
        .collect()
}

/// Aggregate rule results into a validation report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesValidationReport {
    pub results: Vec<RuleEvaluationResult>,
    pub passed: usize,
    pub failed: usize,
    pub unverifiable: usize,
    #[serde(default)]
    pub stale: usize,
    pub overall_verdict: RuleVerdict,
}

impl RulesValidationReport {
    pub fn from_results(results: Vec<RuleEvaluationResult>) -> Self {
        let passed = results
            .iter()
            .filter(|r| r.verdict == RuleVerdict::Pass)
            .count();
        let failed = results
            .iter()
            .filter(|r| r.verdict == RuleVerdict::Fail)
            .count();
        let unverifiable = results
            .iter()
            .filter(|r| r.verdict == RuleVerdict::Unverifiable)
            .count();
        let stale = results
            .iter()
            .filter(|r| r.verdict == RuleVerdict::Stale)
            .count();

        let overall_verdict = if failed > 0 {
            RuleVerdict::Fail
        } else if stale > 0 {
            RuleVerdict::Stale
        } else if unverifiable > 0 {
            RuleVerdict::Unverifiable
        } else {
            RuleVerdict::Pass
        };

        Self {
            results,
            passed,
            failed,
            unverifiable,
            stale,
            overall_verdict,
        }
    }

    pub fn has_blocking_failures(&self) -> bool {
        self.results.iter().any(|r| {
            matches!(r.verdict, RuleVerdict::Fail | RuleVerdict::Stale)
                && matches!(r.severity, RuleSeverity::Error | RuleSeverity::Critical)
        })
    }

    pub fn has_unverifiable_mandatory(&self) -> bool {
        self.results.iter().any(|r| {
            r.verdict == RuleVerdict::Unverifiable
                && matches!(r.severity, RuleSeverity::Error | RuleSeverity::Critical)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_validation_epoch() -> ValidationEpochMetadata {
        ValidationEpochMetadata {
            schema_version: "1.0.0".to_string(),
            epoch_id: "test-epoch".to_string(),
            evaluator_identity: "test-evaluator".to_string(),
            evaluator_set_hash: "test-evaluator-hash".to_string(),
            constitution_version: "test-version".to_string(),
            constitution_hash: "test-constitution-hash".to_string(),
            validation_profile: "test-profile".to_string(),
            validation_profile_hash: "test-profile-hash".to_string(),
            proof_rubric: "test-rubric".to_string(),
            proof_rubric_hash: "test-rubric-hash".to_string(),
            generated_specs_manifest_hash: "test-specs-hash".to_string(),
            generated_specs_fingerprint: "test-specs-fingerprint".to_string(),
            material_hashes: BTreeMap::new(),
        }
    }

    fn terminal_test_context(
        root: Option<&Path>,
        workunit: Option<crate::core::workunit::WorkUnitManifest>,
    ) -> RuleEvaluationContext {
        let governing = root
            .map(|root| {
                format!(
                    "sha256:{:x}",
                    Sha256::digest(std::fs::read(root.join(".decapod/governance.json")).unwrap())
                )
            })
            .unwrap_or_else(|| format!("sha256:{}", "a".repeat(64)));
        let mut artifacts: BTreeMap<String, String> = BTreeMap::new();
        if let Some(manifest) = &workunit {
            artifacts.insert(
                "workunit_manifest".into(),
                manifest.canonical_hash_hex().unwrap(),
            );
            artifacts.insert(
                "proof_plan_gates".into(),
                crate::core::readiness_evidence::digest(&manifest.proof_results).unwrap(),
            );
        }
        let identities = artifacts
            .keys()
            .map(|name: &String| (name.clone(), governing.clone()))
            .collect();
        RuleEvaluationContext {
            changed_paths: BTreeSet::from(["README.md".into()]),
            task_type: Some("docs".into()),
            git_ref: None,
            is_ci: false,
            evidence_artifacts: artifacts,
            validation_epoch: root
                .map(|root| crate::core::validation_epoch::active_validation_epoch(root).unwrap())
                .unwrap_or_else(test_validation_epoch),
            policy_fingerprint: governing.clone(),
            repo_root: root.map(Path::to_path_buf),
            include_publication: true,
            governing_identity: Some(governing),
            evidence_identity: identities,
            terminal_evidence: Some(TerminalEvidence {
                proof: crate::core::validation_history::test_fixture::completed(),
                workunit,
            }),
        }
    }

    #[test]
    fn explicit_project_workunit_obligation_is_never_exempted_as_optional() {
        let mut registry = RuleRegistry::new();
        let mut required = registry
            .get_rule(&RuleId::new("decapod.evidence.proof_backed_completion"))
            .unwrap()
            .rule;
        required.id = RuleId::new("decapod.project.required_workunit");
        required.authority = RuleAuthority::ProjectOverride;
        let id = required.id.clone();
        registry.project_rules.insert(id.clone(), required);
        registry.resolved_cache = None;
        let context = terminal_test_context(None, None);
        let applicable = registry.get_applicable_rules(&context);
        assert!(
            !applicable
                .iter()
                .any(|rule| rule.rule.id.as_str() == "decapod.evidence.proof_backed_completion")
        );
        let required = applicable
            .iter()
            .find(|rule| rule.rule.id == id)
            .expect("explicit mandatory project rule remains applicable");
        assert_eq!(
            evaluate_rule(required, &context).verdict,
            RuleVerdict::Unverifiable
        );
    }

    #[test]
    fn current_declared_workunit_reaches_terminal_rule_with_real_capsule_lineage() {
        use crate::core::{capsule_policy, context_capsule, workunit};
        let root = workunit::journal_tests::accepted_execution_fixture();
        let root = root.path();
        let task = "task_test";
        let spec = ".decapod/managed/specs/INTENT.md";
        std::fs::create_dir_all(root.join(spec).parent().unwrap()).unwrap();
        std::fs::write(
            root.join(spec),
            "# Workunit fixture\n\nBind declared source proof to reviewed task intent.\n",
        )
        .unwrap();
        let policy =
            capsule_policy::resolve_capsule_policy(root, "interfaces", None, 1, true).unwrap();
        let capsule = context_capsule::query_embedded_capsule_governed(
            root,
            "source proof",
            "interfaces",
            Some(task),
            Some(task),
            policy.effective_limit,
            policy.binding,
        )
        .unwrap();
        let capsule_path = context_capsule::write_context_capsule(root, &capsule).unwrap();
        let capsule_ref = capsule_path.strip_prefix(root).unwrap().to_string_lossy();
        workunit::add_state_ref(root, task, &capsule_ref).unwrap();
        workunit::add_spec_ref(root, task, spec).unwrap();
        workunit::set_proof_plan(root, task, &["validate_passes".into()]).unwrap();
        for args in [
            vec!["add", "."],
            vec![
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                "bind actual capsule and spec",
            ],
        ] {
            let out = std::process::Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        workunit::checkpoint_workunit(root, task, "terminal-current", Some(4)).unwrap();
        let artifact = ".decapod/data/terminal-source.txt";
        std::fs::write(root.join(artifact), "unit-fixture source proof bytes").unwrap();
        workunit::record_proof_result(root, task, "source-check", "pass", Some(artifact.into()))
            .unwrap();
        for state in [
            workunit::WorkUnitStatus::Executing,
            workunit::WorkUnitStatus::Claimed,
        ] {
            workunit::transition_status(root, task, state).unwrap();
        }
        let manifest = load_terminal_workunit(root, task).unwrap().unwrap();
        assert!(manifest.validation_epoch.is_none());
        let mut context = terminal_test_context(Some(root), Some(manifest));
        let rule = RuleRegistry::new()
            .get_rule(&RuleId::new("decapod.evidence.proof_backed_completion"))
            .unwrap();
        assert_eq!(evaluate_rule(&rule, &context).verdict, RuleVerdict::Pass);
        context
            .terminal_evidence
            .as_mut()
            .unwrap()
            .workunit
            .as_mut()
            .unwrap()
            .validation_epoch = Some(test_validation_epoch());
        // A stale optional legacy projection is not evidence for or against
        // this fresh complete observation. Canonical append binds its subject.
        assert_eq!(evaluate_rule(&rule, &context).verdict, RuleVerdict::Pass);
        context
            .terminal_evidence
            .as_mut()
            .unwrap()
            .workunit
            .as_mut()
            .unwrap()
            .validation_epoch = None;
        std::fs::write(&capsule_path, "corrupt capsule").unwrap();
        assert_eq!(
            evaluate_rule(&rule, &context).verdict,
            RuleVerdict::Unverifiable
        );
        assert!(
            load_terminal_workunit(root, task).is_err(),
            "changed source/capsule cannot reuse current binding"
        );
        std::fs::remove_file(workunit::workunit_path(root, task).unwrap()).unwrap();
        assert!(load_terminal_workunit(root, task).is_err());
    }

    #[test]
    fn output_rules_do_not_gate_first_execution_and_markers_cannot_prove_completion() {
        let mut registry = RuleRegistry::new();
        let hash = format!("sha256:{}", "a".repeat(64));
        let mut context = RuleEvaluationContext {
            changed_paths: BTreeSet::from(["README.md".into()]),
            task_type: Some("docs".into()),
            git_ref: None,
            is_ci: false,
            evidence_artifacts: BTreeMap::from([
                ("validation_proof".into(), hash.clone()),
                ("validation_receipt".into(), hash.clone()),
                ("trajectory_artifact".into(), hash.clone()),
                ("candidate_head".into(), "b".repeat(40)),
                ("workunit_manifest".into(), hash.clone()),
                ("proof_plan_gates".into(), format!("proven:{hash}")),
            ]),
            validation_epoch: test_validation_epoch(),
            policy_fingerprint: hash.clone(),
            repo_root: None,
            include_publication: false,
            governing_identity: Some(hash.clone()),
            evidence_identity: BTreeMap::new(),
            terminal_evidence: None,
        };
        assert!(
            registry
                .get_applicable_rules(&context)
                .iter()
                .all(|rule| rule.rule.evaluation_phase != EvaluationPhase::Publication)
        );
        context.include_publication = true;
        context.evidence_identity = context
            .evidence_artifacts
            .keys()
            .map(|name| (name.clone(), hash.clone()))
            .collect();
        let terminal: Vec<_> = registry
            .get_applicable_rules(&context)
            .iter()
            .filter(|rule| rule.rule.evaluation_phase == EvaluationPhase::Publication)
            .map(|rule| evaluate_rule(rule, &context))
            .collect();
        assert_eq!(terminal.len(), 3);
        assert!(
            terminal
                .iter()
                .all(|result| result.verdict != RuleVerdict::Pass)
        );
    }

    #[test]
    fn declared_workunit_never_becomes_absent_when_its_projection_is_removed() {
        let root = tempfile::tempdir().unwrap();
        crate::core::todo::initialize_todo_db(&root.path().join(".decapod/data")).unwrap();
        let id = "docs_declared";
        assert!(load_terminal_workunit(root.path(), id).unwrap().is_none());
        crate::core::workunit::init_workunit(root.path(), id, "intent:explicit").unwrap();
        assert!(
            load_terminal_workunit(root.path(), id).is_err(),
            "unproven explicit gates remain required"
        );
        let path = crate::core::workunit::workunit_path(root.path(), id).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(
            load_terminal_workunit(root.path(), id).is_err(),
            "durable declaration survives projection deletion"
        );
        std::fs::write(&path, "invalid manifest").unwrap();
        assert!(load_terminal_workunit(root.path(), id).is_err());
        // Failed durable reads are Unknown, never an absent optional contract.
        let database = crate::core::events::canonical_db_path(&root.path().join(".decapod/data"));
        std::fs::write(database, b"not a sqlite database").unwrap();
        assert!(load_terminal_workunit(root.path(), id).is_err());
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*", "anything"));
        assert!(glob_match("src/**", "src/main.rs"));
        assert!(glob_match("src/**", "src/lib.rs"));
        assert!(glob_match("**/*.rs", "src/main.rs"));
        assert!(glob_match("**/*.rs", "main.rs"));
        assert!(!glob_match("src/**", "lib.rs"));
        assert!(glob_match("exact", "exact"));
        assert!(!glob_match("exact", "different"));
    }

    #[test]
    fn test_rule_id() {
        let id = RuleId::new("decapod.test.rule");
        assert_eq!(id.as_str(), "decapod.test.rule");
        assert_eq!(id.to_string(), "decapod.test.rule");
    }

    #[test]
    fn test_rule_applicability() {
        let applicability = RuleApplicability {
            required_changed_paths: vec!["src/**".to_string()],
            forbidden_changed_paths: vec!["**/secrets/**".to_string()],
            task_types: vec!["feature".to_string()],
            ref_patterns: vec!["refs/heads/*".to_string()],
            ci_only: false,
            local_only: false,
        };

        let mut changed_paths = BTreeSet::new();
        changed_paths.insert("src/main.rs".to_string());

        // Create a test rule to use is_applicable
        let mut test_rule = ValidationRule {
            id: RuleId::new("test.rule"),
            description: "Test rule".to_string(),
            authority: RuleAuthority::Constitution,
            applicability: applicability.clone(),
            evaluation_phase: EvaluationPhase::Validation,
            evaluator: "test".to_string(),
            requirements: RuleRequirements::default(),
            severity: RuleSeverity::Error,
            remediation: RuleRemediation {
                error_code: "TEST".to_string(),
                violation_description: "test".to_string(),
                expected_condition: "test".to_string(),
                observed_condition: "test".to_string(),
                affected_paths: vec![],
                required_steps: vec![],
                autonomous_remediation_permitted: true,
                human_authorization_required: false,
                evidence_for_revalidation: vec![],
                related_rules: vec![],
            },
            override_eligibility: OverrideEligibility::FullyOverridable,
            provenance: RuleProvenance {
                source: "test".to_string(),
                source_hash: "hash1".to_string(),
                body_hash: "hash2".to_string(),
                registered_at: "2024-01-01T00:00:00Z".to_string(),
                decapod_version: "0.1.0".to_string(),
            },
        };

        // Should apply - has required path
        assert!(test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));

        // Should not apply - missing required path
        changed_paths.clear();
        changed_paths.insert("docs/readme.md".to_string());
        assert!(!test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));

        // Forbidden paths stay applicable. They are violations, not exemptions.
        changed_paths.clear();
        changed_paths.insert("src/main.rs".to_string());
        changed_paths.insert("config/secrets/key.txt".to_string());
        assert!(test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));
        assert!(test_rule.forbidden_scope_hit(&changed_paths));

        // Should not apply - wrong task type
        changed_paths.clear();
        changed_paths.insert("src/main.rs".to_string());
        assert!(!test_rule.is_applicable(
            &changed_paths,
            Some("bugfix"),
            Some("refs/heads/main"),
            false
        ));

        // Should not apply - CI only but running locally
        test_rule.applicability.ci_only = true;
        assert!(!test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));
        assert!(test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            true
        ));

        // Should not apply - local only but running in CI
        test_rule.applicability.ci_only = false;
        test_rule.applicability.local_only = true;
        assert!(test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));
        assert!(!test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            true
        ));
    }

    #[test]
    fn test_rule_registry_builtin_rules() {
        let mut registry = RuleRegistry::new();
        let resolved = registry.resolve_effective_rules();

        // Check that built-in rules are registered
        assert!(resolved.contains_key(&RuleId::new("decapod.scope.authorized_paths")));
        assert!(resolved.contains_key(&RuleId::new("decapod.scope.protected_paths")));
        assert!(resolved.contains_key(&RuleId::new("decapod.custody.task_ownership")));
        assert!(resolved.contains_key(&RuleId::new("decapod.evidence.required_and_fresh")));
        assert!(resolved.contains_key(&RuleId::new("decapod.evidence.proof_backed_completion")));
        assert!(resolved.contains_key(&RuleId::new("decapod.scope.file_touch_budget")));
        assert!(resolved.contains_key(&RuleId::new("decapod.custody.workspace_isolation")));

        // Check rule properties
        let scope_rule = resolved
            .get(&RuleId::new("decapod.scope.authorized_paths"))
            .unwrap();
        assert_eq!(scope_rule.rule.authority, RuleAuthority::Constitution);
        assert_eq!(scope_rule.rule.severity, RuleSeverity::Error);
        assert_eq!(scope_rule.rule.evaluator, "decapod.scope.validator");
        assert_eq!(
            scope_rule.rule.override_eligibility,
            OverrideEligibility::TightenOnly
        );

        let critical_rule = resolved
            .get(&RuleId::new("decapod.custody.task_ownership"))
            .unwrap();
        assert_eq!(critical_rule.rule.authority, RuleAuthority::KernelInvariant);
        assert_eq!(critical_rule.rule.severity, RuleSeverity::Critical);
        assert_eq!(
            critical_rule.rule.override_eligibility,
            OverrideEligibility::NonOverridable
        );
    }

    #[test]
    fn test_rules_validation_report() {
        let results = vec![
            RuleEvaluationResult {
                rule_id: RuleId::new("rule1"),
                verdict: RuleVerdict::Pass,
                severity: RuleSeverity::Error,
                evaluated_at: "2024-01-01T00:00:00Z".to_string(),
                evidence_refs: vec![],
                message: "passed".to_string(),
                remediation: None,
                policy_fingerprint: "fp1".to_string(),
                validation_epoch: test_validation_epoch(),
            },
            RuleEvaluationResult {
                rule_id: RuleId::new("rule2"),
                verdict: RuleVerdict::Fail,
                severity: RuleSeverity::Error,
                evaluated_at: "2024-01-01T00:00:00Z".to_string(),
                evidence_refs: vec![],
                message: "failed".to_string(),
                remediation: None,
                policy_fingerprint: "fp1".to_string(),
                validation_epoch: test_validation_epoch(),
            },
            RuleEvaluationResult {
                rule_id: RuleId::new("rule3"),
                verdict: RuleVerdict::Unverifiable,
                severity: RuleSeverity::Warning,
                evaluated_at: "2024-01-01T00:00:00Z".to_string(),
                evidence_refs: vec![],
                message: "unverifiable".to_string(),
                remediation: None,
                policy_fingerprint: "fp1".to_string(),
                validation_epoch: test_validation_epoch(),
            },
        ];

        let report = RulesValidationReport::from_results(results);
        assert_eq!(report.passed, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.unverifiable, 1);
        assert_eq!(report.overall_verdict, RuleVerdict::Fail);
        assert!(report.has_blocking_failures());
        assert!(!report.has_unverifiable_mandatory());

        // Test with unverifiable mandatory
        let results2 = vec![RuleEvaluationResult {
            rule_id: RuleId::new("rule1"),
            verdict: RuleVerdict::Unverifiable,
            severity: RuleSeverity::Error,
            evaluated_at: "2024-01-01T00:00:00Z".to_string(),
            evidence_refs: vec![],
            message: "unverifiable".to_string(),
            remediation: None,
            policy_fingerprint: "fp1".to_string(),
            validation_epoch: test_validation_epoch(),
        }];
        let report2 = RulesValidationReport::from_results(results2);
        assert!(report2.has_unverifiable_mandatory());
    }

    #[test]
    fn test_override_eligibility() {
        let mut registry = RuleRegistry::new();

        // Test that NonOverridable rules cannot be overridden
        let critical_rule = registry
            .resolve_effective_rules()
            .get(&RuleId::new("decapod.custody.task_ownership"))
            .unwrap()
            .clone();
        assert_eq!(
            critical_rule.rule.override_eligibility,
            OverrideEligibility::NonOverridable
        );

        // Test that TightenOnly rules can be tightened
        let scope_rule = registry
            .resolve_effective_rules()
            .get(&RuleId::new("decapod.scope.authorized_paths"))
            .unwrap()
            .clone();
        assert_eq!(
            scope_rule.rule.override_eligibility,
            OverrideEligibility::TightenOnly
        );
    }

    #[test]
    fn test_rule_provenance() {
        let provenance = RuleProvenance {
            source: "test".to_string(),
            source_hash: "hash1".to_string(),
            body_hash: "hash2".to_string(),
            registered_at: "2024-01-01T00:00:00Z".to_string(),
            decapod_version: "0.1.0".to_string(),
        };

        assert_eq!(provenance.source, "test");
        assert_eq!(provenance.source_hash, "hash1");
    }

    #[test]
    fn test_remediation_contract() {
        let remediation = RuleRemediation {
            error_code: "TEST_ERROR".to_string(),
            violation_description: "Test violation".to_string(),
            expected_condition: "Expected".to_string(),
            observed_condition: "Observed".to_string(),
            affected_paths: vec!["src/test.rs".to_string()],
            required_steps: vec!["Fix it".to_string()],
            autonomous_remediation_permitted: true,
            human_authorization_required: false,
            evidence_for_revalidation: vec!["evidence1".to_string()],
            related_rules: vec![RuleId::new("rule1")],
        };

        assert_eq!(remediation.error_code, "TEST_ERROR");
        assert!(remediation.autonomous_remediation_permitted);
        assert!(!remediation.human_authorization_required);
    }

    #[test]
    fn test_normalize_rule_task_type_and_canonical_git_ref() {
        assert_eq!(
            normalize_rule_task_type(Some("features"), None).as_deref(),
            Some("feature")
        );
        assert_eq!(
            normalize_rule_task_type(None, Some("feat_01m79knt8h670tjh")).as_deref(),
            Some("feature")
        );
        assert_eq!(
            normalize_rule_task_type(Some("bugs"), None).as_deref(),
            Some("bugfix")
        );
        assert_eq!(
            normalize_rule_task_type(Some("code"), Some("bugs_01m79knt8h670tjh")).as_deref(),
            Some("bugfix")
        );
        assert_eq!(
            normalize_rule_task_type(None, Some("refa_01m79knt8h670tjh")).as_deref(),
            Some("refactor")
        );
        assert_eq!(
            normalize_rule_task_type(Some("docs"), Some("task_01m79knt8h670tjh")).as_deref(),
            Some("docs")
        );
        assert_eq!(
            normalize_rule_task_type(Some("tests"), None).as_deref(),
            Some("test")
        );
        assert_eq!(
            normalize_rule_task_type(Some("cicd"), Some("cicd_01")),
            None
        );
        assert_eq!(normalize_rule_task_type(None, Some("task_01")), None);

        assert_eq!(canonical_git_ref(""), None);
        assert_eq!(canonical_git_ref("HEAD"), None);
        assert_eq!(
            canonical_git_ref("agent/tester/docs_01-guide").as_deref(),
            Some("refs/heads/agent/tester/docs_01-guide")
        );
        assert_eq!(
            canonical_git_ref("refs/heads/master").as_deref(),
            Some("refs/heads/master")
        );
        assert_eq!(
            canonical_git_ref("refs/tags/v0.143.0").as_deref(),
            Some("refs/tags/v0.143.0")
        );
    }

    #[test]
    fn terminal_publication_applicability_filters_task_type_and_git_ref() {
        let mut registry = RuleRegistry::new();
        let base_rule = registry
            .get_rule(&RuleId::new("decapod.evidence.required_and_fresh"))
            .unwrap()
            .rule;

        let mut docs_agent = base_rule.clone();
        docs_agent.id = RuleId::new("decapod.project.docs_agent_branch");
        docs_agent.authority = RuleAuthority::ProjectOverride;
        docs_agent.applicability.task_types = vec!["docs".to_string()];
        docs_agent.applicability.ref_patterns = vec!["refs/heads/agent/*".to_string()];

        let mut feature_only = docs_agent.clone();
        feature_only.id = RuleId::new("decapod.project.feature_only");
        feature_only.applicability.task_types = vec!["feature".to_string()];

        let mut release_only = docs_agent.clone();
        release_only.id = RuleId::new("decapod.project.release_branch_only");
        release_only.applicability.ref_patterns = vec!["refs/heads/release/*".to_string()];

        for rule in [docs_agent, feature_only, release_only] {
            registry.project_rules.insert(rule.id.clone(), rule);
        }
        registry.resolved_cache = None;

        let mut context = terminal_test_context(None, None);
        context.changed_paths = BTreeSet::from(["README.md".to_string()]);
        context.task_type = normalize_rule_task_type(None, Some("docs_01m79knt8h670tjh"));
        context.git_ref = canonical_git_ref("agent/tester/docs_01m79knt8h670tjh-guide");

        let applicable_ids: BTreeSet<String> = registry
            .get_applicable_rules(&context)
            .into_iter()
            .filter(|r| r.rule.evaluation_phase == EvaluationPhase::Publication)
            .map(|r| r.rule.id.as_str().to_string())
            .collect();

        assert!(applicable_ids.contains("decapod.evidence.required_and_fresh"));
        assert!(applicable_ids.contains("decapod.project.docs_agent_branch"));
        assert!(!applicable_ids.contains("decapod.project.feature_only"));
        assert!(!applicable_ids.contains("decapod.project.release_branch_only"));
    }

    #[test]
    fn declared_unverified_workunit_blocks_terminal_proof_backed_completion() {
        use crate::core::workunit;
        let root = workunit::journal_tests::accepted_execution_fixture();
        let root = root.path();
        let task = "task_test";
        workunit::set_proof_plan(root, task, &["docs-proof".into()]).unwrap();
        workunit::checkpoint_workunit(root, task, "terminal-unverified", None).unwrap();

        let manifest = load_terminal_workunit(root, task).unwrap().unwrap();
        assert_eq!(manifest.status, workunit::WorkUnitStatus::Draft);
        let context = terminal_test_context(Some(root), Some(manifest));
        let mut registry = RuleRegistry::new();
        let applicable = registry.get_applicable_rules(&context);
        let completion_rule = applicable
            .iter()
            .find(|r| r.rule.id.as_str() == "decapod.evidence.proof_backed_completion")
            .expect("declared workunit must enable proof_backed_completion at terminal phase");
        let evaluation = evaluate_rule(completion_rule, &context);
        assert_eq!(evaluation.verdict, RuleVerdict::Unverifiable);
        assert_eq!(
            evaluation
                .remediation
                .as_ref()
                .map(|r| r.error_code.as_str()),
            Some("UNVERIFIED_COMPLETION")
        );
    }

    #[test]
    fn protected_paths_rule_ignores_preexisting_file_patterns_and_blocks_protected_or_blocked_paths()
     {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".decapod")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::create_dir_all(root.join("migrations")).unwrap();

        std::fs::write(
            root.join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n\n[init]\nspecs = true\nci = true\ndiagram_style = \"mermaid\"\nentrypoints = [\"AGENTS.md\"]\n\n[repo]\nproduct_name = \"test\"\nproduct_summary = \"test\"\narchitecture_direction = \"cli\"\nproduct_type = \"service_or_library\"\ndone_criteria = \"tests pass\"\nprimary_languages = [\"rust\"]\ndetected_surfaces = [\"cargo\"]\n\n[governance]\nprotected_paths = [\"config/production.toml\"]\n",
        )
        .unwrap();
        crate::cli::DecapodProjectConfig::load(root).unwrap();
        std::fs::write(root.join("config/production.toml"), "mode = \"prod\"\n").unwrap();
        std::fs::write(root.join(".env.production"), "APP_MODE=prod\n").unwrap();
        std::fs::write(root.join("migrations/001.sql"), "SELECT 1;\n").unwrap();

        // Construct pre-existing patterns dynamically so this source file stays free of literals.
        let preexisting_shell = format!("// shell fixture {}{{EXPANSION}}\n", '$');
        let preexisting_header = format!("// header {} {}\n", "Bea", "rer example_token_value");
        std::fs::write(
            root.join("src/legacy.rs"),
            format!("{preexisting_shell}{preexisting_header}pub fn ok() -> usize {{ 1 }}\n"),
        )
        .unwrap();
        std::fs::write(
            root.join("src/loader.rs"),
            "pub const PROD_CFG: &str = include_str!(\"../config/production.toml\");\n",
        )
        .unwrap();

        let full_scan = crate::core::gatekeeper::run_gatekeeper(
            root,
            &[std::path::PathBuf::from("src/legacy.rs")],
            0,
            &crate::core::gatekeeper::GatekeeperConfig::from_repo_config(root),
        )
        .unwrap();
        assert!(
            !full_scan.passed,
            "fixture must trigger full-file gatekeeper secret/dangerous-pattern checks"
        );

        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from(["src/legacy.rs".to_string()]),
                128
            ),
            RuleVerdict::Pass
        );
        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from(["src/loader.rs".to_string()]),
                64
            ),
            RuleVerdict::Pass,
            "unmodified include_str dependency matching protected_paths must not false-fail"
        );
        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from(["config/production.toml".to_string()]),
                64
            ),
            RuleVerdict::Fail
        );
        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from([".env.production".to_string()]),
                64
            ),
            RuleVerdict::Fail
        );
        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from(["src/legacy.rs".to_string()]),
                20 * 1024 * 1024
            ),
            RuleVerdict::Fail
        );

        let mut plan = crate::plan_governance::init_plan(
            root,
            crate::plan_governance::InitPlanInput {
                title: "Protected path scope test".to_string(),
                intent: "Verify plan forbidden_paths enforcement".to_string(),
                todo_ids: Vec::new(),
                proof_hooks: Vec::new(),
                unknowns: Vec::new(),
                human_questions: Vec::new(),
                stop_conditions: Vec::new(),
                unresolved_contradictions: Vec::new(),
                deferred_questions: Vec::new(),
                constraints: Default::default(),
                phases: Vec::new(),
            },
        )
        .unwrap();
        plan.constraints.forbidden_paths = vec!["migrations".to_string()];
        crate::plan_governance::save_plan(root, &plan).unwrap();

        assert_eq!(
            evaluate_gatekeeper_paths_and_diff(
                root,
                &BTreeSet::from(["migrations/001.sql".to_string()]),
                32
            ),
            RuleVerdict::Fail
        );
    }
}

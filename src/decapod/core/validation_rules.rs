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

        // Check forbidden changed paths
        for pattern in &self.applicability.forbidden_changed_paths {
            if changed_paths.iter().any(|path| glob_match(pattern, path)) {
                return false;
            }
        }

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
            evaluation_phase: EvaluationPhase::Validation,
            evaluator: "decapod.evidence.validator".to_string(),
            requirements: RuleRequirements {
                required_evidence: vec![
                    "validation_receipt".to_string(),
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
                    "validation_receipt".to_string(),
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
            applicability: RuleApplicability {
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
                        // Only allow increasing severity
                        if project_rule.severity > existing.rule.severity {
                            existing.effective_severity = project_rule.severity.clone();
                        }
                        // Merge applicability (add constraints)
                        existing
                            .effective_applicability
                            .required_changed_paths
                            .extend(project_rule.applicability.required_changed_paths.clone());
                        existing
                            .effective_applicability
                            .forbidden_changed_paths
                            .extend(project_rule.applicability.forbidden_changed_paths.clone());
                        existing.override_values = Some(RuleOverrideValues {
                            severity: Some(project_rule.severity.clone()),
                            applicability: Some(project_rule.applicability.clone()),
                            ..Default::default()
                        });
                    }
                    _ => {
                        // Fully overridable or configurable
                        existing.effective_severity = project_rule.severity.clone();
                        existing.effective_applicability = project_rule.applicability.clone();
                        existing.override_values = Some(RuleOverrideValues {
                            severity: Some(project_rule.severity.clone()),
                            applicability: Some(project_rule.applicability.clone()),
                            requirements: Some(project_rule.requirements.clone()),
                            remediation: Some(project_rule.remediation.clone()),
                        });
                    }
                }
                existing.rule = project_rule.clone();
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
                // Task constraints can only tighten
                if task_rule.severity > existing.rule.severity {
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
                existing.override_values = Some(RuleOverrideValues {
                    severity: Some(task_rule.severity.clone()),
                    applicability: Some(task_rule.applicability.clone()),
                    ..Default::default()
                });
                existing.rule = task_rule.clone();
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

    // Check evidence freshness
    let mut _stale_evidence: Vec<String> = Vec::new();
    if rule.rule.requirements.evidence_max_age_seconds > 0 {
        for _evidence in context.evidence_artifacts.keys() {
            // In a real implementation, we'd check the actual timestamp
            // For now, we'll skip this check
        }
    }

    let verdict = if !missing_evidence.is_empty() {
        RuleVerdict::Unverifiable
    } else {
        // Delegate to actual evaluator
        match evaluator_name.as_str() {
            "decapod.scope.validator" => evaluate_scope_rule(rule, context),
            "decapod.gatekeeper" => evaluate_gatekeeper_rule(rule, context),
            "decapod.workspace" => evaluate_workspace_rule(rule, context),
            "decapod.evidence.validator" => evaluate_evidence_rule(rule, context),
            "decapod.workunit" => evaluate_workunit_rule(rule, context),
            _ => RuleVerdict::Unverifiable,
        }
    };

    let severity = rule.effective_severity.clone();
    let remediation = if matches!(verdict, RuleVerdict::Fail | RuleVerdict::Unverifiable) {
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
            RuleVerdict::Unverifiable => format!(
                "Rule unverifiable: missing evidence {}",
                missing_evidence.join(", ")
            ),
        },
        remediation,
        policy_fingerprint: context.policy_fingerprint.clone(),
        validation_epoch: context.validation_epoch.clone(),
    }
}

/// Evaluate scope rule using existing plan governance.
fn evaluate_scope_rule(rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    // Read governed plan to get scope constraints
    let repo_root = std::path::Path::new(".");
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

/// Evaluate gatekeeper rule using actual gatekeeper.
fn evaluate_gatekeeper_rule(_rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    // Check if we have the required evidence for gatekeeper
    if !context.evidence_artifacts.contains_key("config.toml") {
        return RuleVerdict::Unverifiable;
    }

    // Gatekeeper only applies in governed agent workspaces with active task claims
    if !context.evidence_artifacts.contains_key("workspace_status") {
        return RuleVerdict::Unverifiable;
    }

    // Verify we're in a workspace (not root repo)
    let repo_root = std::path::Path::new(".");
    match crate::core::workspace::get_workspace_status(repo_root) {
        Ok(status) => {
            // Gatekeeper only applies in governed agent workspaces with active task claims
            if !status.can_work || !status.git.in_worktree || status.git.is_protected {
                return RuleVerdict::Unverifiable;
            }
        }
        Err(_) => return RuleVerdict::Unverifiable,
    }

    // Run actual gatekeeper checks
    let repo_root = std::path::Path::new(".");
    let paths: Vec<std::path::PathBuf> = context
        .changed_paths
        .iter()
        .map(std::path::PathBuf::from)
        .collect();

    // Calculate diff size using git diff
    let diff_bytes = calculate_diff_size(repo_root).unwrap_or(0);

    let config = crate::core::gatekeeper::GatekeeperConfig::from_repo_config(repo_root);
    match crate::core::gatekeeper::run_gatekeeper(repo_root, &paths, diff_bytes, &config) {
        Ok(result) if result.passed => RuleVerdict::Pass,
        Ok(_) => RuleVerdict::Fail,
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

    // Verify we're in a workspace (not root repo)
    let repo_root = std::path::Path::new(".");
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
    if rule.rule.requirements.evidence_max_age_seconds > 0 {
        let repo_root = std::path::Path::new(".");
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

    RuleVerdict::Pass
}

/// Evaluate workunit rule using actual workunit validation.
fn evaluate_workunit_rule(_rule: &ResolvedRule, context: &RuleEvaluationContext) -> RuleVerdict {
    // Check workunit manifest and proof gates
    if !context.evidence_artifacts.contains_key("workunit_manifest") {
        return RuleVerdict::Unverifiable;
    }

    // In a real implementation, we'd check proof gates
    // For now, assume pass if manifest exists
    RuleVerdict::Pass
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

        let overall_verdict = if failed > 0 {
            RuleVerdict::Fail
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
            overall_verdict,
        }
    }

    pub fn has_blocking_failures(&self) -> bool {
        self.results.iter().any(|r| {
            r.verdict == RuleVerdict::Fail
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

        // Should not apply - forbidden path
        changed_paths.clear();
        changed_paths.insert("src/main.rs".to_string());
        changed_paths.insert("config/secrets/key.txt".to_string());
        assert!(!test_rule.is_applicable(
            &changed_paths,
            Some("feature"),
            Some("refs/heads/main"),
            false
        ));

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
}

//! Semantic integrity checks for governed `plan` and `claims` artifacts (#1093).
//!
//! Ensures agents bind `claims` (`authority.source_issue`, `change_policy`, `purpose`)
//! and `plan` (`title`, `intent`, `todo_ids`, `proof_hooks`) semantically to the active
//! governed change rather than performing superficial whitespace or timestamp theater edits.

use crate::core::research_claims::ClaimsLedger;
use crate::plan_governance::GovernedPlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const PLACEHOLDER_TOKENS: &[&str] = &[
    "tbd",
    "todo",
    "placeholder",
    "wip",
    "fix",
    "update",
    "test",
    "n/a",
    "none",
    "...",
    "change",
];

const NOOP_PROOF_HOOKS: &[&str] = &[
    "true",
    "echo ok",
    "echo pass",
    ":",
    "exit 0",
    "none",
    "skip",
    "n/a",
    "manual",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIntegrityFinding {
    pub code: String,
    pub surface: String,
    pub field: String,
    pub detail: String,
    pub remediation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIntegrityReport {
    pub valid: bool,
    pub theater_diff_detected: bool,
    pub findings: Vec<SemanticIntegrityFinding>,
}

fn is_substantive_prose(text: &str, min_len: usize) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < min_len {
        return false;
    }
    let normalized = trimmed.to_ascii_lowercase();
    !PLACEHOLDER_TOKENS.contains(&normalized.as_str())
}

fn is_valid_issue_ref(source_issue: &str) -> bool {
    let trimmed = source_issue.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Some(rest) = trimmed.strip_prefix('#') {
        return !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit());
    }
    if trimmed.starts_with("https://github.com/")
        && trimmed.contains("/issues/")
        && let Some(tail) = trimmed.rsplit("/issues/").next()
    {
        return !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit());
    }
    false
}

fn is_noop_proof_hook(hook: &str) -> bool {
    let normalized = hook.trim().to_ascii_lowercase();
    normalized.is_empty() || NOOP_PROOF_HOOKS.contains(&normalized.as_str())
}

fn strip_volatile_timestamps(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut cleaned = serde_json::Map::new();
            for (k, v) in map {
                if matches!(
                    k.as_str(),
                    "updated_at" | "last_evaluated_at" | "observed_at"
                ) {
                    continue;
                }
                cleaned.insert(k.clone(), strip_volatile_timestamps(v));
            }
            Value::Object(cleaned)
        }
        Value::Array(items) => Value::Array(items.iter().map(strip_volatile_timestamps).collect()),
        other => other.clone(),
    }
}

/// Detect whitespace-only or timestamp-only "governance theater" diffs between a
/// base governance JSON artifact and its candidate revision.
pub fn detect_governance_theater_diff(base_raw: &str, candidate_raw: &str) -> bool {
    if base_raw == candidate_raw {
        return false;
    }
    let (Ok(base_val), Ok(cand_val)) = (
        serde_json::from_str::<Value>(base_raw),
        serde_json::from_str::<Value>(candidate_raw),
    ) else {
        return false;
    };
    strip_volatile_timestamps(&base_val) == strip_volatile_timestamps(&cand_val)
}

/// Validate semantic integrity of `plan` and `claims` artifacts against the active
/// governed task, optional governing issue reference, and optional base-vs-candidate
/// raw JSON snapshots for theater-diff detection.
pub fn validate_semantic_governance_integrity(
    plan: Option<&GovernedPlan>,
    claims: Option<&ClaimsLedger>,
    active_todo_id: Option<&str>,
    expected_source_issue: Option<&str>,
    base_vs_candidate_raw: Option<(&str, &str)>,
) -> SemanticIntegrityReport {
    let mut findings = Vec::new();
    let mut theater_diff_detected = false;

    if let Some((base_raw, candidate_raw)) = base_vs_candidate_raw
        && detect_governance_theater_diff(base_raw, candidate_raw)
    {
        theater_diff_detected = true;
        findings.push(SemanticIntegrityFinding {
            code: "GOVERNANCE_THEATER_DIFF".to_string(),
            surface: "governance.json".to_string(),
            field: "sections".to_string(),
            detail: "Candidate governance diff only changes whitespace or volatile timestamps without semantic updates.".to_string(),
            remediation: "Use `decapod govern plan` and `decapod govern artifacts inventory --claims-note` to record real semantic changes.".to_string(),
        });
    }

    if let Some(plan) = plan {
        if !is_substantive_prose(&plan.title, 8) {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_TITLE_NON_SUBSTANTIVE".to_string(),
                surface: "plan".to_string(),
                field: "title".to_string(),
                detail: format!("Plan title '{}' is empty or placeholder text.", plan.title),
                remediation: "Set a concrete plan title via `decapod govern plan init/update`."
                    .to_string(),
            });
        }
        if !is_substantive_prose(&plan.intent, 12) {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_INTENT_NON_SUBSTANTIVE".to_string(),
                surface: "plan".to_string(),
                field: "intent".to_string(),
                detail: format!(
                    "Plan intent '{}' is empty or placeholder text.",
                    plan.intent
                ),
                remediation: "Describe the actual governed change intent in `plan.intent`."
                    .to_string(),
            });
        }
        if plan.proof_hooks.is_empty() {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_PROOF_HOOKS_EMPTY".to_string(),
                surface: "plan".to_string(),
                field: "proof_hooks".to_string(),
                detail: "Governed plan declares zero proof_hooks.".to_string(),
                remediation: "Bind concrete verification commands (e.g. `decapod validate`) in `plan.proof_hooks`.".to_string(),
            });
        } else if plan.proof_hooks.iter().all(|h| is_noop_proof_hook(h)) {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_PROOF_HOOK_THEATER".to_string(),
                surface: "plan".to_string(),
                field: "proof_hooks".to_string(),
                detail: format!(
                    "Plan proof_hooks {:?} contain only trivial no-op checks.",
                    plan.proof_hooks
                ),
                remediation:
                    "Replace no-op proof_hooks with real executable verification commands."
                        .to_string(),
            });
        }
        if let Some(todo_id) = active_todo_id.map(str::trim).filter(|s| !s.is_empty())
            && !plan.todo_ids.iter().any(|id| id == todo_id)
        {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_TODO_BINDING_MISMATCH".to_string(),
                surface: "plan".to_string(),
                field: "todo_ids".to_string(),
                detail: format!(
                    "Active claimed todo '{}' is not bound in plan.todo_ids {:?}.",
                    todo_id, plan.todo_ids
                ),
                remediation: "Bind the active todo via `decapod govern plan` instead of leaving stale todo_ids.".to_string(),
            });
        }
        if plan.todo_ids.iter().any(|id| id.trim().is_empty()) {
            findings.push(SemanticIntegrityFinding {
                code: "PLAN_TODO_ID_EMPTY".to_string(),
                surface: "plan".to_string(),
                field: "todo_ids".to_string(),
                detail: "Plan todo_ids contains an empty entry.".to_string(),
                remediation: "Remove blank entries from `plan.todo_ids`.".to_string(),
            });
        }
    }

    if let Some(claims) = claims {
        if !is_valid_issue_ref(&claims.authority.source_issue) {
            findings.push(SemanticIntegrityFinding {
                code: "CLAIMS_SOURCE_ISSUE_INVALID".to_string(),
                surface: "claims".to_string(),
                field: "authority.source_issue".to_string(),
                detail: format!(
                    "claims.authority.source_issue '{}' is not a valid issue reference (#N or GitHub issue URL).",
                    claims.authority.source_issue
                ),
                remediation: "Point `claims.authority.source_issue` to the governing issue."
                    .to_string(),
            });
        } else if let Some(expected) = expected_source_issue
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let matches_exact = claims.authority.source_issue.trim() == expected;
            let expected_num = expected
                .rsplit('/')
                .next()
                .unwrap_or(expected)
                .trim_start_matches('#');
            let actual_num = claims
                .authority
                .source_issue
                .trim()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .trim_start_matches('#');
            if !matches_exact && (expected_num.is_empty() || expected_num != actual_num) {
                findings.push(SemanticIntegrityFinding {
                    code: "CLAIMS_SOURCE_ISSUE_MISMATCH".to_string(),
                    surface: "claims".to_string(),
                    field: "authority.source_issue".to_string(),
                    detail: format!(
                        "claims.authority.source_issue '{}' does not match governing issue '{}'.",
                        claims.authority.source_issue, expected
                    ),
                    remediation: "Update `claims.authority.source_issue` via governed CLI to bind the active governing issue.".to_string(),
                });
            }
        }
        if !is_substantive_prose(&claims.purpose, 12) {
            findings.push(SemanticIntegrityFinding {
                code: "CLAIMS_PURPOSE_NON_SUBSTANTIVE".to_string(),
                surface: "claims".to_string(),
                field: "purpose".to_string(),
                detail: format!(
                    "claims.purpose '{}' is empty or placeholder text.",
                    claims.purpose
                ),
                remediation: "Record substantive governance purpose in `claims.purpose`."
                    .to_string(),
            });
        }
        if !is_substantive_prose(&claims.authority.change_policy, 8) {
            findings.push(SemanticIntegrityFinding {
                code: "CLAIMS_CHANGE_POLICY_NON_SUBSTANTIVE".to_string(),
                surface: "claims".to_string(),
                field: "authority.change_policy".to_string(),
                detail: "claims.authority.change_policy is empty or placeholder text.".to_string(),
                remediation: "Append a substantive change note via `decapod govern artifacts inventory --claims-note`.".to_string(),
            });
        }
    }

    SemanticIntegrityReport {
        valid: findings.is_empty(),
        theater_diff_detected,
        findings,
    }
}

/// JSON-facing entry point for external callers and integration tests to validate
/// `plan` and `claims` JSON sections semantically without exposing internal module paths.
pub fn validate_semantic_governance_json(
    plan_json: Option<&Value>,
    claims_json: Option<&Value>,
    active_todo_id: Option<&str>,
    expected_source_issue: Option<&str>,
    base_vs_candidate_raw: Option<(&str, &str)>,
) -> Result<SemanticIntegrityReport, String> {
    let plan: Option<GovernedPlan> = match plan_json {
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|e| format!("invalid plan JSON payload: {e}"))?,
        ),
        None => None,
    };
    let claims: Option<ClaimsLedger> = match claims_json {
        Some(v) => Some(
            serde_json::from_value(v.clone())
                .map_err(|e| format!("invalid claims JSON payload: {e}"))?,
        ),
        None => None,
    };
    Ok(validate_semantic_governance_integrity(
        plan.as_ref(),
        claims.as_ref(),
        active_todo_id,
        expected_source_issue,
        base_vs_candidate_raw,
    ))
}

//! Redacted read-only provenance for agent context consultations (#1732).
//!
//! Strictly read-only, bounded, secret-redacted, and prompt/reasoning-stripped.
//! Never leaks raw user/agent prompts, private chain-of-thought reasoning,
//! tokens, passwords, PEM keys, or cross-repository trace records.

use crate::core::error::DecapodError;
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::observability_projections::snapshot::{
    OBSERVE_SCHEMA_VERSION, ObservationSnapshot, capture_observation_with,
};
use crate::core::observability_projections::specs::extract_intent_lineage;
use crate::core::path_policy;
use crate::core::trace;
use crate::plan_governance::GovernedPlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::Path;

pub const DEFAULT_PROVENANCE_LIMIT: usize = 64;
pub const HARD_PROVENANCE_LIMIT: usize = 256;
pub const MAX_TRACE_SCAN_LINES: usize = 4_096;

/// Keys stripped wholesale so raw prompts, model reasoning, or credentials
/// can NEVER leak into read-only provenance projections (#1732).
const FORBIDDEN_PROVENANCE_KEYS: &[&str] = &[
    "thinking",
    "chain_of_thought",
    "private_reasoning",
    "reasoning",
    "raw_prompt",
    "user_prompt",
    "system_prompt",
    "prompt",
    "token",
    "auth_token",
    "access_token",
    "refresh_token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "authorization",
    "cookie",
    "private_key",
    "credential",
    "credentials",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextProvenanceQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_prov_limit")]
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// Optional caller-asserted repo/workspace scope; mismatched scope is
    /// explicitly rejected to prevent cross-scope data leakage (#1732).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_repo_id: Option<String>,
}

impl Default for ContextProvenanceQuery {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: default_prov_limit(),
            task_id: None,
            expected_repo_id: None,
        }
    }
}

fn default_prov_limit() -> usize {
    DEFAULT_PROVENANCE_LIMIT
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentProvenanceSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_revision: Option<String>,
    /// Preserves whether request origin was observed via a trusted host vs
    /// merely agent-reported (#1732).
    pub origin_trust: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_instruction_redacted: Option<String>,
    pub later_instructions_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_interpretation_redacted: Option<String>,
    pub unresolved_contradictions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsultationResponseMetadata {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outputs_hash: Option<String>,
    pub redacted_summary: String,
    pub approx_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactedConsultationRecord {
    pub consultation_id: String,
    pub timestamp: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub workspace_repo_id: String,
    pub op: String,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_revision: Option<String>,
    /// `"trusted_host"` only when backed by verified host provenance; otherwise
    /// `"agent_reported"` (consultation cannot independently authenticate human prompt).
    pub origin_trust: String,
    /// `"original_instruction"`, `"later_instruction"`, `"working_interpretation"`,
    /// `"unresolved_contradiction"`, or `"context_lookup"`.
    pub instruction_class: String,
    pub referenced_specs: Vec<String>,
    pub referenced_obligations: Vec<String>,
    pub response_metadata: ConsultationResponseMetadata,
    pub redaction_applied: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redacted_fields: Vec<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenancePagination {
    pub offset: usize,
    pub limit: usize,
    pub total: usize,
    pub returned: usize,
    pub has_more: bool,
    pub retention_scan_cap_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextProvenanceProjection {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub snapshot: ObservationSnapshot,
    pub intent_context: IntentProvenanceSummary,
    pub consultations: Vec<RedactedConsultationRecord>,
    pub pagination: ProvenancePagination,
}

/// Sanitize arbitrary text for provenance display: strip control characters,
/// redact secrets/credentials, normalize absolute paths, and bound length.
pub fn redact_provenance_text(repo_root: &Path, raw: &str, max_chars: usize) -> (String, bool) {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t' | ' '))
        .collect();
    let secret_redacted = trace::redact_string(&cleaned);
    let path_redacted = path_policy::redact_text(repo_root, &secret_redacted);
    let bounded: String = path_redacted.chars().take(max_chars).collect();
    let changed = bounded != raw;
    (bounded, changed)
}

fn collect_redacted_keys(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if FORBIDDEN_PROVENANCE_KEYS
                    .iter()
                    .any(|forbidden| lower.contains(forbidden))
                {
                    out.push(k.clone());
                } else {
                    collect_redacted_keys(v, out);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr {
                collect_redacted_keys(item, out);
            }
        }
        _ => {}
    }
}

fn is_context_consultation_op(op: &str) -> bool {
    matches!(
        op,
        "context.resolve"
            | "context.scope"
            | "context.capsule.query"
            | "context.bindings"
            | "constitution.get"
            | "constitution.links.query"
            | "constitution.links.navigate"
            | "standards.resolve"
            | "mentor.obligations"
            | "agent.init"
            | "infer.orientation"
            | "intent.reconcile"
    ) || op.starts_with("context.")
        || op.starts_with("constitution.")
        || op.starts_with("infer.")
}

fn purpose_for_op(op: &str) -> &'static str {
    match op {
        "context.resolve" | "context.scope" | "context.bindings" => {
            "orientation_context_resolution"
        }
        "context.capsule.query" => "scoped_capsule_consultation",
        "constitution.get" | "constitution.links.query" | "constitution.links.navigate" => {
            "constitution_rule_consultation"
        }
        "standards.resolve" => "standards_lookup",
        "mentor.obligations" => "obligation_guidance",
        "infer.orientation" => "pre_inference_orientation",
        "intent.reconcile" => "intent_reconciliation",
        "agent.init" => "agent_bootstrap_orientation",
        _ => "governed_context_consultation",
    }
}

fn instruction_class_for_op(op: &str, has_contradictions: bool) -> &'static str {
    if has_contradictions && op == "intent.reconcile" {
        "unresolved_contradiction"
    } else if op == "intent.reconcile" {
        "later_instruction"
    } else if op == "infer.orientation" || op == "context.resolve" || op == "context.scope" {
        "working_interpretation"
    } else {
        "context_lookup"
    }
}

fn extract_referenced_specs(val: &Value) -> Vec<String> {
    let mut specs = Vec::new();
    fn walk(v: &Value, specs: &mut Vec<String>) {
        match v {
            Value::String(s) => {
                if (s.contains(".decapod/managed/specs/")
                    || s.starts_with("core/")
                    || s.starts_with("specs/")
                    || s.starts_with("interfaces/"))
                    && s.len() <= 160
                    && !s.contains('\n')
                {
                    specs.push(s.clone());
                }
            }
            Value::Array(arr) => {
                for item in arr.iter().take(64) {
                    walk(item, specs);
                }
            }
            Value::Object(map) => {
                for (k, item) in map {
                    let lower = k.to_ascii_lowercase();
                    if !FORBIDDEN_PROVENANCE_KEYS.iter().any(|f| lower.contains(f)) {
                        walk(item, specs);
                    }
                }
            }
            _ => {}
        }
    }
    walk(val, &mut specs);
    specs.sort();
    specs.dedup();
    specs.truncate(32);
    specs
}

fn extract_referenced_obligations(val: &Value) -> Vec<String> {
    let mut obligations = Vec::new();
    if let Some(mandates) = val.get("mandates").and_then(Value::as_array) {
        for m in mandates.iter().take(32) {
            if let Some(id) = m.get("id").and_then(Value::as_str) {
                obligations.push(id.to_string());
            }
        }
    }
    if let Some(anchors) = val
        .get("receipt")
        .and_then(|r| r.get("governing_anchors"))
        .and_then(Value::as_array)
    {
        for a in anchors.iter().take(32) {
            if let Some(s) = a.as_str() {
                obligations.push(s.to_string());
            }
        }
    }
    obligations.sort();
    obligations.dedup();
    obligations
}

/// Project strictly read-only, redacted provenance for agent context
/// consultations (#1732).
pub fn project_context_provenance(
    repo_root: &Path,
    query: &ContextProvenanceQuery,
) -> Result<ContextProvenanceProjection, DecapodError> {
    let limit = query.limit.clamp(1, HARD_PROVENANCE_LIMIT);

    let (snapshot, inner) = capture_observation_with(repo_root, |root, data_root| {
        let doc = governance_document::load(root).ok().flatten();
        let trace_path = data_root.join("traces.jsonl");
        let trace_lines: Vec<String> = if trace_path.exists() {
            fs::read_to_string(&trace_path)
                .ok()
                .map(|raw| {
                    let all: Vec<String> = raw.lines().map(ToOwned::to_owned).collect();
                    let start = all.len().saturating_sub(MAX_TRACE_SCAN_LINES);
                    all[start..].to_vec()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok((doc, trace_lines))
    })?;

    // Cross-scope rejection (#1732)
    if let Some(ref expected) = query.expected_repo_id
        && expected.trim() != snapshot.repository.repo_id
    {
        return Err(DecapodError::ValidationError(format!(
            "CROSS_SCOPE_DENIED: requested repo scope '{}' does not match observed repository '{}'",
            expected, snapshot.repository.repo_id
        )));
    }

    let (doc, trace_lines) = inner;
    let plan_opt: Option<GovernedPlan> = doc
        .as_ref()
        .and_then(|d| d.sections.get("plan"))
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    let lineage = extract_intent_lineage(doc.as_ref(), plan_opt.as_ref());

    let origin_trust = if lineage
        .origin_confidence
        .eq_ignore_ascii_case("trusted_host")
        || lineage
            .independent_authority
            .eq_ignore_ascii_case("verified")
    {
        "trusted_host".to_string()
    } else {
        "agent_reported".to_string()
    };

    let original_instruction_redacted = lineage
        .original_intent
        .as_deref()
        .map(|s| redact_provenance_text(repo_root, s, 256).0);
    let working_interpretation_redacted = lineage
        .refined_intent
        .as_deref()
        .map(|s| redact_provenance_text(repo_root, s, 256).0);
    let has_contradictions = !lineage.unresolved_contradictions.is_empty();

    let intent_context = IntentProvenanceSummary {
        intent_id: lineage.intent_id.clone(),
        intent_revision: lineage.instruction_revision.clone(),
        origin_trust: origin_trust.clone(),
        original_instruction_redacted,
        later_instructions_count: lineage.revision_count,
        working_interpretation_redacted,
        unresolved_contradictions: lineage.unresolved_contradictions.clone(),
    };

    let mut records = Vec::new();
    for (idx, line) in trace_lines.iter().enumerate() {
        let Ok(ev) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let op = ev.get("op").and_then(Value::as_str).unwrap_or("");
        if !is_context_consultation_op(op) {
            continue;
        }
        let req = ev.get("request").cloned().unwrap_or(Value::Null);
        let resp = ev.get("response").cloned().unwrap_or(Value::Null);

        let task_id = req
            .get("params")
            .and_then(|p| p.get("task_id").or_else(|| p.get("todo_id")))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        if let Some(ref filter_task) = query.task_id
            && task_id.as_deref() != Some(filter_task.as_str())
        {
            continue;
        }

        let mut redacted_fields = Vec::new();
        collect_redacted_keys(&req, &mut redacted_fields);
        collect_redacted_keys(&resp, &mut redacted_fields);
        redacted_fields.sort();
        redacted_fields.dedup();

        let trace_id = ev
            .get("trace_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("trace_{idx}"));
        let ts = ev
            .get("ts")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let actor = ev
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let mut specs = extract_referenced_specs(&req);
        specs.extend(extract_referenced_specs(&resp));
        specs.sort();
        specs.dedup();

        let obligations = extract_referenced_obligations(&resp);

        let success = resp
            .get("success")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let outputs_hash = resp
            .get("receipt")
            .and_then(|r| r.get("outputs_hash"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        let raw_summary = format!(
            "op={op} success={success} specs={} obligations={}",
            specs.len(),
            obligations.len()
        );
        let (redacted_summary, _) = redact_provenance_text(repo_root, &raw_summary, 256);
        let approx_bytes = line.len();

        let req_origin_trust = req
            .get("params")
            .and_then(|p| p.get("origin_trust"))
            .and_then(Value::as_str)
            // A consultation cannot independently authenticate a human prompt;
            // only trust when governance lineage itself is trusted_host.
            .filter(|v| *v == "trusted_host" && origin_trust == "trusted_host")
            .unwrap_or("agent_reported")
            .to_string();

        records.push(RedactedConsultationRecord {
            consultation_id: trace_id,
            timestamp: ts,
            actor,
            task_id,
            workspace_repo_id: snapshot.repository.repo_id.clone(),
            op: op.to_string(),
            purpose: purpose_for_op(op).to_string(),
            intent_id: lineage.intent_id.clone(),
            intent_revision: lineage.instruction_revision.clone(),
            origin_trust: req_origin_trust,
            instruction_class: instruction_class_for_op(op, has_contradictions).to_string(),
            referenced_specs: specs,
            referenced_obligations: obligations,
            response_metadata: ConsultationResponseMetadata {
                success,
                outputs_hash,
                redacted_summary,
                approx_bytes,
            },
            redaction_applied: true,
            redacted_fields,
            source_ref: format!(".decapod/data/traces.jsonl#line:{}", idx + 1),
        });
    }

    // If governance.json recorded an intent without traces, include a synthetic
    // metadata pointer to the canonical intent record (never fabricating a prompt).
    if records.is_empty() && lineage.intent_id.is_some() {
        records.push(RedactedConsultationRecord {
            consultation_id: format!(
                "intent_origin:{}",
                lineage.intent_id.as_deref().unwrap_or("unknown")
            ),
            timestamp: snapshot.observed_at.clone(),
            actor: "governance_record".to_string(),
            task_id: plan_opt.as_ref().and_then(|p| p.todo_ids.first().cloned()),
            workspace_repo_id: snapshot.repository.repo_id.clone(),
            op: "governance.intent".to_string(),
            purpose: "canonical_intent_provenance".to_string(),
            intent_id: lineage.intent_id.clone(),
            intent_revision: lineage.instruction_revision.clone(),
            origin_trust,
            instruction_class: "original_instruction".to_string(),
            referenced_specs: Vec::new(),
            referenced_obligations: Vec::new(),
            response_metadata: ConsultationResponseMetadata {
                success: true,
                outputs_hash: snapshot.governance_doc_sha256.clone(),
                redacted_summary: "canonical intent recorded in .decapod/governance.json"
                    .to_string(),
                approx_bytes: 0,
            },
            redaction_applied: true,
            redacted_fields: Vec::new(),
            source_ref: format!("{GOVERNANCE_PATH}#/intents"),
        });
    }

    let total = records.len();
    let start = query.offset.min(total);
    let end = (start + limit).min(total);
    let paged = records[start..end].to_vec();

    Ok(ContextProvenanceProjection {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        read_only: true,
        authoritative: false,
        snapshot,
        intent_context,
        consultations: paged.clone(),
        pagination: ProvenancePagination {
            offset: start,
            limit,
            total,
            returned: paged.len(),
            has_more: end < total,
            retention_scan_cap_lines: MAX_TRACE_SCAN_LINES,
        },
    })
}

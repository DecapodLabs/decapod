//! Strict, repository-native research claims ledger validation.
//!
//! The ledger is deliberately typed and closed over unknown fields. The JSON
//! Schema is the machine-readable public contract; these Rust types provide
//! the executable semantic checks used by validation.

use crate::core::error::DecapodError;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub const CLAIMS_PATH: &str = crate::core::governance_document::GOVERNANCE_PATH;
pub const CLAIMS_SCHEMA_VERSION: &str = "1.0.0";
pub const CLAIMS_KIND: &str = "research_claims_ledger";
pub const CLAIMS_SCHEMA_URI: &str =
    "https://decapod.dev/schemas/research-claims-ledger-1.0.0.schema.json";
pub const CLAIMS_COMPACT_COMMAND: &str = "decapod govern artifacts inventory --compact";
const CLAIMS_SCHEMA_DOCUMENT: &str = include_str!("../../../assets/schemas/claims.schema.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimsLedger {
    #[serde(rename = "$schema")]
    pub schema_uri: String,
    pub schema_version: String,
    pub kind: String,
    pub ledger_id: String,
    pub title: String,
    pub purpose: String,
    pub authority: Authority,
    pub scope: Scope,
    pub methodology: Methodology,
    pub claims: Vec<Claim>,
    pub governance: Governance,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authority {
    pub owner: String,
    pub source_issue: String,
    pub status: AuthorityStatus,
    pub created_at: String,
    pub updated_at: String,
    pub change_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub product: String,
    pub repository: String,
    pub in_scope: Vec<String>,
    pub out_of_scope: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Methodology {
    pub falsifiability_standard: String,
    pub evidence_statuses: Vec<EvidenceStatus>,
    pub measurement_rules: Vec<MeasurementRule>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    NotStarted,
    Instrumented,
    Partial,
    Supported,
    Falsified,
    Superseded,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementRule {
    pub id: String,
    pub name: String,
    pub requirement: String,
    pub unit: String,
    pub evidence_source: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub id: String,
    pub sequence: u32,
    pub title: String,
    pub statement: String,
    pub why_it_matters: String,
    pub baseline: ExperimentCondition,
    pub decapod_condition: ExperimentCondition,
    pub failure_modes: Vec<FailureMode>,
    pub measurements: Vec<Measurement>,
    pub proof_gate: ProofGate,
    pub open_questions: Vec<OpenQuestion>,
    pub evidence: Evidence,
    pub implementation_links: Vec<ImplementationLink>,
    pub non_guarantees: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentCondition {
    pub description: String,
    pub population: String,
    pub unit_of_analysis: String,
    pub protocol: Vec<String>,
    pub controls: Vec<String>,
    pub confounders: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureMode {
    pub id: String,
    pub description: String,
    pub observable_signal: String,
    pub severity: Severity,
    pub falsifier: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub id: String,
    pub name: String,
    pub definition: String,
    pub unit: MeasurementUnit,
    pub scale: MeasurementScale,
    pub aggregation: Aggregation,
    pub collection_method: String,
    pub comparator: String,
    pub success_direction: SuccessDirection,
    pub threshold: Option<Threshold>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementUnit {
    Count,
    Ratio,
    Seconds,
    Tokens,
    Boolean,
    Categorical,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementScale {
    Ratio,
    Interval,
    Ordinal,
    Nominal,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggregation {
    Mean,
    Median,
    Rate,
    Count,
    Distribution,
    Qualitative,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuccessDirection {
    Lower,
    Higher,
    Match,
    Absence,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Threshold {
    pub operator: ThresholdOperator,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdOperator {
    Lt,
    Lte,
    Eq,
    Gte,
    Gt,
    Neq,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProofGate {
    pub id: String,
    pub required_artifacts: Vec<ArtifactRequirement>,
    pub required_commands: Vec<CommandRequirement>,
    pub acceptance_criteria: Vec<String>,
    pub failure_behavior: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRequirement {
    pub path: String,
    pub kind: ArtifactKind,
    pub required: bool,
    pub integrity: IntegrityMethod,
    pub relationship: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Trajectory,
    Receipt,
    Capsule,
    Todo,
    Test,
    Document,
    Schema,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntegrityMethod {
    Hash,
    Schema,
    GitTracked,
    ManualReview,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandRequirement {
    pub id: String,
    pub command: String,
    pub expected_exit_code: i32,
    pub bounded_seconds: u64,
    pub evidence_artifact: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenQuestion {
    pub id: String,
    pub question: String,
    pub decision_impact: String,
    pub owner: String,
    pub status: QuestionStatus,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    Open,
    Resolved,
    Deferred,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub status: EvidenceStatus,
    pub summary: String,
    pub sources: Vec<EvidenceSource>,
    pub observations: Vec<Observation>,
    pub last_evaluated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceSource {
    pub id: String,
    pub kind: EvidenceSourceKind,
    #[serde(rename = "ref")]
    pub reference: String,
    pub description: String,
    pub trust: EvidenceTrust,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSourceKind {
    Issue,
    Code,
    Test,
    Artifact,
    Benchmark,
    Document,
    Observation,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceTrust {
    RepoNative,
    HumanAuthored,
    External,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub observed_at: String,
    pub result: String,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImplementationLink {
    pub surface: String,
    pub path: String,
    pub symbol: String,
    pub relationship: ImplementationRelationship,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationRelationship {
    Implements,
    Measures,
    Proves,
    Documents,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Governance {
    pub required_claim_fields: Vec<String>,
    pub review_policy: String,
    pub new_capability_rule: String,
    pub change_control: ChangeControl,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeControl {
    pub requires_issue: bool,
    pub requires_validation: bool,
    pub requires_human_review: bool,
}

/// Legacy schema remains readable for explicit, lossless migration.
pub fn validate_legacy_value(value: &Value) -> Result<(), DecapodError> {
    let ledger: ClaimsLedger = serde_json::from_value(value.clone())
        .map_err(|e| DecapodError::ValidationError(format!("Invalid legacy claims: {e}")))?;
    validate_ledger(&ledger)
}

pub fn load_and_validate(repo_root: &Path) -> Result<Option<Value>, DecapodError> {
    crate::core::governance_document::read_section(repo_root, "claims")
}

pub fn ledger_size_bytes(repo_root: &Path) -> Result<Option<u64>, DecapodError> {
    load_and_validate(repo_root)?
        .map(|value| {
            serde_json::to_vec(&value)
                .map(|bytes| bytes.len() as u64)
                .map_err(|e| DecapodError::ValidationError(e.to_string()))
        })
        .transpose()
}

/// New repositories start with no historical claim catalog. No claim is invented.
pub fn ensure_template(repo_root: &Path, dry_run: bool) -> Result<bool, DecapodError> {
    if dry_run || crate::core::governance_document::load(repo_root)?.is_some() {
        return Ok(false);
    }
    crate::core::governance_document::migrate(repo_root)?;
    Ok(true)
}

pub fn compact(repo_root: &Path) -> Result<bool, DecapodError> {
    let existed = repo_root.join(CLAIMS_PATH).exists();
    crate::core::governance_document::migrate(repo_root)?;
    Ok(!existed)
}

/// Compatibility note surface records PR work, never appends history to policy.
pub fn append_change_note(repo_root: &Path, note: &str) -> Result<bool, DecapodError> {
    if note.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "Claims note must not be empty".into(),
        ));
    }
    let id = format!(
        "note:{}",
        &crate::core::governance_document::digest(&serde_json::json!(note))?[7..23]
    );
    let document = crate::core::governance_document::load(repo_root)?.ok_or_else(|| {
        DecapodError::ValidationError("Begin a PR before recording a claims note".into())
    })?;
    if document.checkpoints.iter().any(|entry| entry.id == id) {
        return Ok(false);
    }
    crate::core::governance_document::checkpoint(repo_root, &id, note, Vec::new())?;
    Ok(true)
}

pub fn validate_schema_document() -> Result<(), DecapodError> {
    let schema: Value = serde_json::from_str(CLAIMS_SCHEMA_DOCUMENT).map_err(|error| {
        DecapodError::ValidationError(format!(
            "embedded research claims schema is invalid JSON: {error}"
        ))
    })?;
    let object = schema.as_object().ok_or_else(|| {
        DecapodError::ValidationError(
            "embedded research claims schema must be a JSON object".to_string(),
        )
    })?;
    for required in ["$schema", "$id", "type", "properties", "required", "$defs"] {
        if !object.contains_key(required) {
            return Err(DecapodError::ValidationError(format!(
                "embedded research claims schema is missing top-level field {required}"
            )));
        }
    }
    if object.get("type").and_then(Value::as_str) != Some("object") {
        return Err(DecapodError::ValidationError(
            "embedded research claims schema top-level type must be object".to_string(),
        ));
    }
    validate_closed_schema_objects(&schema, "schema")?;
    Ok(())
}

fn validate_closed_schema_objects(value: &Value, location: &str) -> Result<(), DecapodError> {
    if let Some(object) = value.as_object() {
        if object.get("type").and_then(Value::as_str) == Some("object")
            && object.get("additionalProperties") != Some(&Value::Bool(false))
        {
            return Err(DecapodError::ValidationError(format!(
                "research claims schema object {location} must set additionalProperties=false"
            )));
        }
        for (key, child) in object {
            validate_closed_schema_objects(child, &format!("{location}.{key}"))?;
        }
    }
    if let Some(array) = value.as_array() {
        for (index, child) in array.iter().enumerate() {
            validate_closed_schema_objects(child, &format!("{location}[{index}]"))?;
        }
    }
    Ok(())
}

fn validate_ledger(ledger: &ClaimsLedger) -> Result<(), DecapodError> {
    validate_schema_document()?;
    if ledger.schema_uri != CLAIMS_SCHEMA_URI
        || ledger.schema_version != CLAIMS_SCHEMA_VERSION
        || ledger.kind != CLAIMS_KIND
    {
        return Err(DecapodError::ValidationError(
            "research claims ledger schema identity is invalid".to_string(),
        ));
    }
    for (field, value) in [
        ("ledger_id", &ledger.ledger_id),
        ("title", &ledger.title),
        ("purpose", &ledger.purpose),
        ("authority.owner", &ledger.authority.owner),
        ("authority.source_issue", &ledger.authority.source_issue),
        ("scope.product", &ledger.scope.product),
        ("scope.repository", &ledger.scope.repository),
        (
            "methodology.falsifiability_standard",
            &ledger.methodology.falsifiability_standard,
        ),
        ("governance.review_policy", &ledger.governance.review_policy),
        (
            "governance.new_capability_rule",
            &ledger.governance.new_capability_rule,
        ),
    ] {
        if value.trim().is_empty() {
            return Err(DecapodError::ValidationError(format!(
                "research claims ledger field {field} must not be empty"
            )));
        }
    }
    if ledger.claims.is_empty() {
        return Err(DecapodError::ValidationError(
            "research claims ledger must contain at least one claim".to_string(),
        ));
    }
    let mut claim_ids = HashSet::new();
    let mut sequences: Vec<u32> = Vec::with_capacity(ledger.claims.len());
    for claim in &ledger.claims {
        if !claim_ids.insert(&claim.id) {
            return Err(DecapodError::ValidationError(format!(
                "research claims ledger contains duplicate claim id {}",
                claim.id
            )));
        }
        if claim.id.trim().is_empty()
            || claim.title.trim().is_empty()
            || claim.statement.trim().is_empty()
            || claim.why_it_matters.trim().is_empty()
        {
            return Err(DecapodError::ValidationError(format!(
                "claim {} has an empty identity or statement field",
                claim.id
            )));
        }
        if claim.failure_modes.is_empty()
            || claim.measurements.is_empty()
            || claim.proof_gate.required_artifacts.is_empty()
            || claim.proof_gate.required_commands.is_empty()
            || claim.proof_gate.acceptance_criteria.is_empty()
            || claim.evidence.sources.is_empty()
        {
            return Err(DecapodError::ValidationError(format!(
                "claim {} is missing failure, measurement, proof, acceptance, or evidence detail",
                claim.id
            )));
        }
        sequences.push(claim.sequence);
        validate_nonempty_list(&claim.id, "baseline.protocol", &claim.baseline.protocol)?;
        validate_nonempty_list(
            &claim.id,
            "decapod_condition.protocol",
            &claim.decapod_condition.protocol,
        )?;
        validate_nonempty_list(&claim.id, "non_guarantees", &claim.non_guarantees)?;
    }
    sequences.sort_unstable();
    for (index, sequence) in sequences.iter().enumerate() {
        let expected = (index + 1) as u32;
        if *sequence != expected {
            return Err(DecapodError::ValidationError(format!(
                "research claim sequences must be contiguous starting at 1; expected {expected}, found {sequence}"
            )));
        }
    }
    Ok(())
}

fn validate_nonempty_list(
    claim_id: &str,
    field: &str,
    values: &[String],
) -> Result<(), DecapodError> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(DecapodError::ValidationError(format!(
            "claim {claim_id} field {field} must contain non-empty values"
        )));
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/unit/core/research_claims_tests.rs"]
mod tests;

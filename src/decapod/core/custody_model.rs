//! Typed custody model for durable agent execution state within the Decapod runtime.
//!
//! Intent authority lives in [`IntentRecord`]. Trajectories are explicitly
//! evidence: they can explain what happened, but they cannot approve scope or
//! complete an intent. The host runtime owns persistence and decides when to
//! call this model at execution boundaries.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub const CUSTODY_SCHEMA_VERSION: &str = "1.1.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IntentStatus {
    Raw,
    Refined,
    Approved,
    ScopeApproved,
    Mutating,
    Proving,
    Validating,
    Completed,
    Failed,
    Abandoned,
    Contaminated,
    Stale,
}

impl IntentStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Abandoned | Self::Contaminated | Self::Stale
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum IntentEventKind {
    Created,
    Refined,
    Approved,
    ScopeApproved,
    MutationStarted,
    ProofRecorded,
    ValidationRecorded,
    Completed,
    Failed,
    Abandoned,
    Contaminated,
    Stale,
    TrajectoryStepRecorded,
    KnowledgeCandidateProposed,
    InstructionRecorded,
}

/// Caller-reported provenance only. There is deliberately no caller-settable
/// authenticated/host-verified flag. A source string or digest grants nothing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReportedInstructionSource {
    pub source_ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal_claim: Option<String>,
}

impl ReportedInstructionSource {
    pub fn confidence(&self) -> &'static str {
        "agent_reported"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum RecordedInstructionKind {
    Clarification,
    Contradiction,
    Supersession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstructionRevisionInput {
    pub revision_id: String,
    pub expected_revision: String,
    pub instruction: String,
    pub source_claim: ReportedInstructionSource,
    pub kind: RecordedInstructionKind,
    #[serde(default)]
    pub supersedes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_intent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InstructionRevision {
    pub revision_id: String,
    pub sequence: u64,
    pub predecessor_revision: String,
    pub instruction: String,
    pub source_claim: ReportedInstructionSource,
    pub kind: RecordedInstructionKind,
    pub supersedes: Vec<String>,
    pub preceding_refinement: Option<String>,
    pub derived_intent: Option<String>,
    /// Historical provider observation, not a human grant or a current network read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_observation: Option<crate::core::trajectory::instruction_source::SourceObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstructionLineage {
    pub intent_id: String,
    pub revision: String,
    pub original_intent: String,
    pub origin: Option<ReportedInstructionSource>,
    pub origin_confidence: String,
    pub refined_intent: Option<String>,
    pub revisions: Vec<InstructionRevision>,
    pub revision_source_confidence: String,
    pub independent_authority: crate::core::validation_rules::RuleVerdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentRecord {
    pub id: String,
    /// The original recorded request. Its source claim does not authenticate a human.
    pub raw_intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction_origin: Option<ReportedInstructionSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instruction_revisions: Vec<InstructionRevision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refined_intent: Option<String>,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub boundaries: Vec<String>,
    #[serde(default)]
    pub out_of_scope: Vec<String>,
    #[serde(default)]
    pub proof_requirements: Vec<String>,
    pub status: IntentStatus,
    pub created_sequence: u64,
    pub updated_sequence: u64,
}

impl IntentRecord {
    pub fn instruction_lineage(&self) -> Result<InstructionLineage, CustodyError> {
        self.validate_instruction_history()?;
        Ok(InstructionLineage {
            intent_id: self.id.clone(),
            revision: self.instruction_revision()?,
            original_intent: self.raw_intent.clone(),
            origin: self.instruction_origin.clone(),
            origin_confidence: if self.instruction_origin.is_some() {
                "agent_reported"
            } else {
                "unknown"
            }
            .into(),
            refined_intent: self.refined_intent.clone(),
            revisions: self.instruction_revisions.clone(),
            revision_source_confidence: if self
                .instruction_revisions
                .iter()
                .any(|r| r.source_observation.is_some())
            {
                "per_revision_observed_or_reported"
            } else {
                "agent_reported"
            }
            .into(),
            independent_authority: crate::core::validation_rules::RuleVerdict::Unverifiable,
        })
    }
    pub fn instruction_revision(&self) -> Result<String, CustodyError> {
        crate::core::governance_document::digest(&serde_json::json!([
            self.id,
            self.raw_intent,
            self.instruction_origin,
            if self.instruction_revisions.is_empty() {
                None
            } else {
                Some(&self.instruction_revisions)
            }
        ]))
        .map_err(|_| CustodyError::InstructionConflict("INSTRUCTION_ENCODING_INVALID".into()))
    }

    pub fn validate_instruction_history(&self) -> Result<(), CustodyError> {
        let invalid = || {
            CustodyError::InstructionConflict("INSTRUCTION_HISTORY_CONFLICT: invalid revision sequence, predecessor or interpretation history".into())
        };
        if self.instruction_revisions.len() > 128 {
            return Err(invalid());
        }
        let mut prefix = self.clone();
        prefix.instruction_revisions.clear();
        let mut ids = std::collections::BTreeSet::new();
        let mut refinement = self
            .instruction_revisions
            .first()
            .and_then(|r| r.preceding_refinement.clone());
        for revision in &self.instruction_revisions {
            if revision.sequence != prefix.instruction_revisions.len() as u64 + 1
                || revision.predecessor_revision != prefix.instruction_revision()?
                || !ids.insert(&revision.revision_id)
                || revision.preceding_refinement != refinement
                || revision.instruction.len() > 8192
                || revision.instruction.trim().is_empty()
            {
                return Err(invalid());
            }
            if let Some(derived) = &revision.derived_intent {
                refinement = Some(derived.clone());
            }
            prefix.instruction_revisions.push(revision.clone());
        }
        if !self.instruction_revisions.is_empty() && refinement != self.refined_intent {
            return Err(invalid());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentRefinement {
    pub refined_intent: String,
    #[serde(default)]
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub assumptions: Vec<String>,
    #[serde(default)]
    pub boundaries: Vec<String>,
    #[serde(default)]
    pub out_of_scope: Vec<String>,
    #[serde(default)]
    pub proof_requirements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentEvent {
    pub sequence: u64,
    pub event_id: String,
    pub intent_id: String,
    pub kind: IntentEventKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryStepInput {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub observations: Vec<String>,
    #[serde(default)]
    pub proof_refs: Vec<String>,
    #[serde(default)]
    pub validation_findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrajectoryStep {
    pub sequence: u64,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub observations: Vec<String>,
    #[serde(default)]
    pub proof_refs: Vec<String>,
    #[serde(default)]
    pub validation_findings: Vec<String>,
    /// Optional link back to the append-only custody event.
    pub custody_event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Trajectory {
    pub id: String,
    pub intent_id: String,
    /// This record is evidence only and is never an authority source.
    pub evidence_only: bool,
    #[serde(default)]
    pub steps: Vec<TrajectoryStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentSummary {
    pub intent_id: String,
    pub status: IntentStatus,
    pub raw_intent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refined_intent: Option<String>,
    pub event_count: usize,
    pub trajectory_step_count: usize,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub knowledge_candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectKnowledgeCandidate {
    pub id: String,
    pub intent_id: String,
    pub lesson: String,
    pub authority: String,
    pub status: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustodyState {
    pub schema_version: String,
    #[serde(default)]
    pub intents: BTreeMap<String, IntentRecord>,
    #[serde(default)]
    pub events: Vec<IntentEvent>,
    #[serde(default)]
    pub trajectories: BTreeMap<String, Trajectory>,
    #[serde(default)]
    pub summaries: BTreeMap<String, IntentSummary>,
    #[serde(default)]
    pub knowledge_candidates: Vec<ProjectKnowledgeCandidate>,
    #[serde(default)]
    pub next_sequence: u64,
}

impl CustodyState {
    pub fn is_empty(&self) -> bool {
        self.intents.is_empty()
            && self.events.is_empty()
            && self.trajectories.is_empty()
            && self.summaries.is_empty()
            && self.knowledge_candidates.is_empty()
    }

    fn normalized(mut self) -> Self {
        if self.schema_version.is_empty() {
            self.schema_version = CUSTODY_SCHEMA_VERSION.to_string();
        }
        self
    }
}

pub trait CustodyStore {
    fn state(&self) -> &CustodyState;
    fn state_mut(&mut self) -> &mut CustodyState;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryCustodyStore {
    state: CustodyState,
}

impl InMemoryCustodyStore {
    pub fn new(state: CustodyState) -> Self {
        Self {
            state: state.normalized(),
        }
    }

    pub fn into_state(self) -> CustodyState {
        self.state
    }
}

impl CustodyStore for InMemoryCustodyStore {
    fn state(&self) -> &CustodyState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut CustodyState {
        &mut self.state
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CustodyError {
    #[error("{0}")]
    InstructionConflict(String),
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("intent '{0}' already exists")]
    AlreadyExists(String),
    #[error("intent '{0}' not found")]
    IntentNotFound(String),
    #[error("trajectory '{0}' not found")]
    TrajectoryNotFound(String),
    #[error("intent '{intent_id}' cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        intent_id: String,
        from: IntentStatus,
        to: IntentStatus,
    },
    #[error("trajectory step must belong to intent '{0}'")]
    IntentMismatch(String),
}

pub struct Custody<S> {
    store: S,
}

impl Custody<InMemoryCustodyStore> {
    pub fn in_memory() -> Self {
        Self {
            store: InMemoryCustodyStore::new(CustodyState {
                schema_version: CUSTODY_SCHEMA_VERSION.to_string(),
                ..Default::default()
            }),
        }
    }

    pub fn from_state(state: CustodyState) -> Self {
        Self {
            store: InMemoryCustodyStore::new(state),
        }
    }

    pub fn into_state(self) -> CustodyState {
        self.store.into_state()
    }
}

impl<S: CustodyStore> Custody<S> {
    pub fn state(&self) -> &CustodyState {
        self.store.state()
    }

    pub fn create_intent(&mut self, id: &str, raw_intent: String) -> Result<(), CustodyError> {
        require_nonempty("intent id", id)?;
        if raw_intent.is_empty() {
            return Err(CustodyError::EmptyField {
                field: "raw intent",
            });
        }
        if self.state().intents.contains_key(id) {
            return Err(CustodyError::AlreadyExists(id.to_string()));
        }
        let sequence = self.next_sequence();
        self.store.state_mut().intents.insert(
            id.to_string(),
            IntentRecord {
                id: id.to_string(),
                raw_intent,
                instruction_origin: Some(ReportedInstructionSource {
                    source_ref: "caller:unverified".into(),
                    principal_claim: None,
                }),
                instruction_revisions: Vec::new(),
                refined_intent: None,
                acceptance_criteria: Vec::new(),
                constraints: Vec::new(),
                assumptions: Vec::new(),
                boundaries: Vec::new(),
                out_of_scope: Vec::new(),
                proof_requirements: Vec::new(),
                status: IntentStatus::Raw,
                created_sequence: sequence,
                updated_sequence: sequence,
            },
        );
        self.event(id, IntentEventKind::Created, String::new())?;
        Ok(())
    }

    pub fn refine(&mut self, id: &str, refinement: IntentRefinement) -> Result<(), CustodyError> {
        require_nonempty("refined intent", &refinement.refined_intent)?;
        self.transition(
            id,
            IntentStatus::Refined,
            IntentEventKind::Refined,
            |record| {
                record.refined_intent = Some(refinement.refined_intent.clone());
                record.acceptance_criteria = refinement.acceptance_criteria.clone();
                record.constraints = refinement.constraints.clone();
                record.assumptions = refinement.assumptions.clone();
                record.boundaries = refinement.boundaries.clone();
                record.out_of_scope = refinement.out_of_scope.clone();
                record.proof_requirements = refinement.proof_requirements.clone();
            },
        )
    }

    /// Only the host's canonical task-scope reconciler calls this. It repairs
    /// a proven legacy projection, retains history and revokes derived approval.
    pub(crate) fn reconcile_scope_projection(
        &mut self,
        id: &str,
        expected: &[String],
        canonical: &[String],
        detail: String,
    ) -> Result<(), CustodyError> {
        let record = self
            .state()
            .intents
            .get(id)
            .ok_or_else(|| CustodyError::IntentNotFound(id.into()))?;
        if record.status.is_terminal() || record.constraints != expected {
            return Err(CustodyError::InstructionConflict(
                "SCOPE_RECONCILIATION_AMBIGUOUS: preserve terminal or independently changed constraints".into(),
            ));
        }
        let sequence = self.next_sequence();
        let record = self
            .store
            .state_mut()
            .intents
            .get_mut(id)
            .expect("checked above");
        record.constraints = canonical.to_vec();
        record.status = if record.refined_intent.is_some() {
            IntentStatus::Refined
        } else {
            IntentStatus::Raw
        };
        record.updated_sequence = sequence;
        self.event_with_sequence(
            sequence,
            id,
            IntentEventKind::Refined,
            detail,
            self.event_id(sequence),
        );
        Ok(())
    }

    pub fn append_instruction(
        &mut self,
        id: &str,
        input: InstructionRevisionInput,
    ) -> Result<InstructionRevision, CustodyError> {
        self.append_instruction_from_source(id, input, None)
    }

    pub(crate) fn append_observed_instruction(
        &mut self,
        id: &str,
        input: InstructionRevisionInput,
        observed: crate::core::trajectory::instruction_source::ObservedInstruction,
    ) -> Result<InstructionRevision, CustodyError> {
        let (instruction, source, observation) = observed.into_parts();
        let input = InstructionRevisionInput {
            instruction,
            source_claim: source,
            ..input
        };
        self.append_instruction_from_source(id, input, Some(observation))
    }

    fn append_instruction_from_source(
        &mut self,
        id: &str,
        mut input: InstructionRevisionInput,
        observation: Option<crate::core::trajectory::instruction_source::SourceObservation>,
    ) -> Result<InstructionRevision, CustodyError> {
        let conflict = |message: &str| CustodyError::InstructionConflict(message.into());
        let identifier = |value: &str| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'-' | b'_' | b'.'))
        };
        if !identifier(&input.revision_id)
            || input.instruction.trim().is_empty()
            || input.instruction.len() > 8192
            || input
                .derived_intent
                .as_ref()
                .is_some_and(|s| s.trim().is_empty() || s.len() > 8192)
            || input.source_claim.source_ref.is_empty()
            || input.source_claim.source_ref.len() > 1024
            || input
                .source_claim
                .source_ref
                .chars()
                .any(char::is_whitespace)
            || input.source_claim.source_ref.chars().any(char::is_control)
            || input.source_claim.source_ref.contains('?')
            || crate::core::trace::redact_string(&input.source_claim.source_ref)
                != input.source_claim.source_ref
            || input
                .source_claim
                .source_ref
                .split("://")
                .nth(1)
                .is_some_and(|s| s.split('/').next().unwrap_or_default().contains('@'))
            || input
                .source_claim
                .principal_claim
                .as_deref()
                .is_some_and(|s| !identifier(s))
            || input.supersedes.len() > 128
            || input.supersedes.iter().any(|s| !identifier(s))
        {
            return Err(conflict(
                "INSTRUCTION_INPUT_INVALID: use bounded redacted content, a credential-free source reference and reported principal label",
            ));
        }
        input.instruction = crate::core::trace::redact_string(&input.instruction);
        if let Some(derived) = &mut input.derived_intent {
            *derived = crate::core::trace::redact_string(derived);
        }
        let record = self
            .state()
            .intents
            .get(id)
            .ok_or_else(|| CustodyError::IntentNotFound(id.into()))?;
        record.validate_instruction_history()?;
        if let Some(previous) = record
            .instruction_revisions
            .iter()
            .find(|r| r.revision_id == input.revision_id)
        {
            if previous.predecessor_revision == input.expected_revision
                && previous.instruction == input.instruction
                && previous.source_claim == input.source_claim
                && previous.kind == input.kind
                && previous.supersedes == input.supersedes
                && previous.derived_intent == input.derived_intent
                && previous.source_observation == observation
            {
                return Ok(previous.clone());
            }
            return Err(conflict(
                "INSTRUCTION_REVISION_CONFLICT: existing revision ID has different content; history was not changed",
            ));
        }
        if record.instruction_revision()? != input.expected_revision {
            return Err(conflict(
                "INSTRUCTION_REVISION_STALE: inspect the current recorded revision and retry without overwriting concurrent instructions",
            ));
        }
        if record.instruction_revisions.len() >= 128 {
            return Err(conflict(
                "INSTRUCTION_REVISION_LIMIT: preserve this lineage and establish a separately accepted successor",
            ));
        }
        if input.supersedes.iter().any(|target| {
            target != "origin"
                && !record
                    .instruction_revisions
                    .iter()
                    .any(|r| &r.revision_id == target)
        }) || (input.kind == RecordedInstructionKind::Supersession
            && input.supersedes.is_empty())
        {
            return Err(conflict(
                "INSTRUCTION_SUPERSESSION_INVALID: name retained predecessor revisions; a claim does not authenticate authority",
            ));
        }
        let revision = InstructionRevision {
            revision_id: input.revision_id,
            sequence: record.instruction_revisions.len() as u64 + 1,
            predecessor_revision: input.expected_revision,
            instruction: input.instruction,
            source_claim: input.source_claim,
            kind: input.kind,
            supersedes: input.supersedes,
            preceding_refinement: record.refined_intent.clone(),
            derived_intent: input.derived_intent,
            source_observation: observation,
        };
        let sequence = self.next_sequence();
        let record = self
            .store
            .state_mut()
            .intents
            .get_mut(id)
            .expect("checked intent");
        record.instruction_revisions.push(revision.clone());
        if let Some(derived) = &revision.derived_intent {
            record.refined_intent = Some(derived.clone());
        }
        record.status = if record.refined_intent.is_some() {
            IntentStatus::Refined
        } else {
            IntentStatus::Raw
        };
        record.updated_sequence = sequence;
        let current = record.instruction_revision()?;
        self.store.state_mut().schema_version = CUSTODY_SCHEMA_VERSION.into();
        self.store.state_mut().summaries.remove(id);
        self.event_with_sequence(sequence, id, IntentEventKind::InstructionRecorded,
            serde_json::json!({"revision_id":revision.revision_id,"instruction_revision":current,"source_confidence": if revision.source_observation.is_some() { "provider_observed_record" } else { "agent_reported" }}).to_string(), self.event_id(sequence));
        Ok(revision)
    }

    pub fn approve(&mut self, id: &str) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::Approved,
            IntentEventKind::Approved,
            |_| {},
        )
    }

    pub fn approve_scope(&mut self, id: &str) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::ScopeApproved,
            IntentEventKind::ScopeApproved,
            |_| {},
        )
    }

    pub fn start_mutation(&mut self, id: &str) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::Mutating,
            IntentEventKind::MutationStarted,
            |_| {},
        )
    }

    pub fn record_proof(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::Proving,
            IntentEventKind::ProofRecorded,
            |_| {},
        )?;
        self.replace_last_event_detail(detail);
        Ok(())
    }

    pub fn record_validation(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::Validating,
            IntentEventKind::ValidationRecorded,
            |_| {},
        )?;
        self.replace_last_event_detail(detail);
        Ok(())
    }

    pub fn complete(&mut self, id: &str) -> Result<(), CustodyError> {
        self.transition(
            id,
            IntentStatus::Completed,
            IntentEventKind::Completed,
            |_| {},
        )
    }

    pub fn fail(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.terminal_transition(id, IntentStatus::Failed, IntentEventKind::Failed, detail)
    }

    pub fn abandon(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.terminal_transition(
            id,
            IntentStatus::Abandoned,
            IntentEventKind::Abandoned,
            detail,
        )
    }

    pub fn contaminate(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.terminal_transition(
            id,
            IntentStatus::Contaminated,
            IntentEventKind::Contaminated,
            detail,
        )
    }

    pub fn mark_stale(&mut self, id: &str, detail: String) -> Result<(), CustodyError> {
        self.terminal_transition(id, IntentStatus::Stale, IntentEventKind::Stale, detail)
    }

    pub fn append_trajectory_step(
        &mut self,
        trajectory_id: &str,
        intent_id: &str,
        input: TrajectoryStepInput,
    ) -> Result<String, CustodyError> {
        require_nonempty("trajectory id", trajectory_id)?;
        require_nonempty("action", &input.action)?;
        if !self.state().intents.contains_key(intent_id) {
            return Err(CustodyError::IntentNotFound(intent_id.to_string()));
        }
        if let Some(existing) = self.state().trajectories.get(trajectory_id)
            && existing.intent_id != intent_id
        {
            return Err(CustodyError::IntentMismatch(intent_id.to_string()));
        }
        let sequence = self.next_sequence();
        let event_id = self.event_id(sequence);
        self.event_with_sequence(
            sequence,
            intent_id,
            IntentEventKind::TrajectoryStepRecorded,
            format!("trajectory:{trajectory_id}"),
            event_id.clone(),
        );
        let trajectory = self
            .store
            .state_mut()
            .trajectories
            .entry(trajectory_id.to_string())
            .or_insert_with(|| Trajectory {
                id: trajectory_id.to_string(),
                intent_id: intent_id.to_string(),
                evidence_only: true,
                steps: Vec::new(),
            });
        trajectory.steps.push(TrajectoryStep {
            sequence,
            action: input.action,
            tool: input.tool,
            command: input.command,
            scope: input.scope,
            observations: input.observations,
            proof_refs: input.proof_refs,
            validation_findings: input.validation_findings,
            custody_event_id: event_id.clone(),
        });
        Ok(event_id)
    }

    pub fn summarize(&mut self, id: &str) -> Result<IntentSummary, CustodyError> {
        let record = self
            .state()
            .intents
            .get(id)
            .ok_or_else(|| CustodyError::IntentNotFound(id.to_string()))?;
        let trajectory_step_count = self
            .state()
            .trajectories
            .values()
            .filter(|trajectory| trajectory.intent_id == id)
            .map(|trajectory| trajectory.steps.len())
            .sum();
        let summary = IntentSummary {
            intent_id: id.to_string(),
            status: record.status.clone(),
            raw_intent: record.raw_intent.clone(),
            refined_intent: record.refined_intent.clone(),
            event_count: self
                .state()
                .events
                .iter()
                .filter(|event| event.intent_id == id)
                .count(),
            trajectory_step_count,
            blockers: self
                .state()
                .events
                .iter()
                .filter(|event| {
                    event.intent_id == id
                        && matches!(
                            event.kind,
                            IntentEventKind::Failed
                                | IntentEventKind::Contaminated
                                | IntentEventKind::Stale
                        )
                })
                .map(|event| event.detail.clone())
                .filter(|detail| !detail.is_empty())
                .collect(),
            knowledge_candidate_ids: self
                .state()
                .knowledge_candidates
                .iter()
                .filter(|candidate| candidate.intent_id == id)
                .map(|candidate| candidate.id.clone())
                .collect(),
        };
        self.store
            .state_mut()
            .summaries
            .insert(id.to_string(), summary.clone());
        Ok(summary)
    }

    pub fn propose_knowledge_candidate(
        &mut self,
        intent_id: &str,
        lesson: String,
        evidence_refs: Vec<String>,
    ) -> Result<ProjectKnowledgeCandidate, CustodyError> {
        require_nonempty("lesson", &lesson)?;
        if evidence_refs.is_empty() {
            return Err(CustodyError::EmptyField {
                field: "knowledge evidence refs",
            });
        }
        if !self.state().intents.contains_key(intent_id) {
            return Err(CustodyError::IntentNotFound(intent_id.to_string()));
        }
        let sequence = self.next_sequence();
        let candidate = ProjectKnowledgeCandidate {
            id: format!("knowledge:{intent_id}:{sequence}"),
            intent_id: intent_id.to_string(),
            lesson,
            authority: "proposed".to_string(),
            status: "candidate".to_string(),
            evidence_refs,
        };
        self.store
            .state_mut()
            .knowledge_candidates
            .push(candidate.clone());
        self.event(
            intent_id,
            IntentEventKind::KnowledgeCandidateProposed,
            candidate.id.clone(),
        )?;
        Ok(candidate)
    }

    fn transition<F: FnOnce(&mut IntentRecord)>(
        &mut self,
        id: &str,
        to: IntentStatus,
        kind: IntentEventKind,
        update: F,
    ) -> Result<(), CustodyError> {
        let from = self
            .state()
            .intents
            .get(id)
            .ok_or_else(|| CustodyError::IntentNotFound(id.to_string()))?
            .status
            .clone();
        if !valid_transition(&from, &to) {
            return Err(CustodyError::InvalidTransition {
                intent_id: id.to_string(),
                from,
                to,
            });
        }
        let sequence = self.next_sequence();
        let record = self
            .store
            .state_mut()
            .intents
            .get_mut(id)
            .expect("checked above");
        update(record);
        record.status = to;
        record.updated_sequence = sequence;
        self.event_with_sequence(sequence, id, kind, String::new(), self.event_id(sequence));
        Ok(())
    }

    fn terminal_transition(
        &mut self,
        id: &str,
        to: IntentStatus,
        kind: IntentEventKind,
        detail: String,
    ) -> Result<(), CustodyError> {
        self.transition(id, to, kind, |_| {})?;
        self.replace_last_event_detail(detail);
        Ok(())
    }

    fn event(
        &mut self,
        intent_id: &str,
        kind: IntentEventKind,
        detail: String,
    ) -> Result<(), CustodyError> {
        if !self.state().intents.contains_key(intent_id) {
            return Err(CustodyError::IntentNotFound(intent_id.to_string()));
        }
        let sequence = self.next_sequence();
        let event_id = self.event_id(sequence);
        self.event_with_sequence(sequence, intent_id, kind, detail, event_id);
        Ok(())
    }

    fn event_with_sequence(
        &mut self,
        sequence: u64,
        intent_id: &str,
        kind: IntentEventKind,
        detail: String,
        event_id: String,
    ) {
        self.store.state_mut().events.push(IntentEvent {
            sequence,
            event_id,
            intent_id: intent_id.to_string(),
            kind,
            detail,
        });
    }

    fn replace_last_event_detail(&mut self, detail: String) {
        if let Some(event) = self.store.state_mut().events.last_mut() {
            event.detail = detail;
        }
    }

    fn next_sequence(&mut self) -> u64 {
        let state = self.store.state_mut();
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.next_sequence
    }

    fn event_id(&self, sequence: u64) -> String {
        format!("event:{sequence}")
    }
}

fn require_nonempty(field: &'static str, value: &str) -> Result<(), CustodyError> {
    if value.trim().is_empty() {
        Err(CustodyError::EmptyField { field })
    } else {
        Ok(())
    }
}

fn valid_transition(from: &IntentStatus, to: &IntentStatus) -> bool {
    if matches!(
        to,
        IntentStatus::Failed
            | IntentStatus::Abandoned
            | IntentStatus::Contaminated
            | IntentStatus::Stale
    ) {
        return !from.is_terminal();
    }
    matches!(
        (from, to),
        (IntentStatus::Raw, IntentStatus::Refined)
            | (IntentStatus::Refined, IntentStatus::Approved)
            | (IntentStatus::Approved, IntentStatus::ScopeApproved)
            | (IntentStatus::ScopeApproved, IntentStatus::Mutating)
            | (IntentStatus::Mutating, IntentStatus::Proving)
            | (IntentStatus::Proving, IntentStatus::Validating)
            | (IntentStatus::Validating, IntentStatus::Completed)
    )
}
#[cfg(test)]
#[path = "../../../tests/unit/core/custody_model_tests.rs"]
mod tests;

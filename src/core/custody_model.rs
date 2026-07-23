//! Typed custody model for durable agent execution state within the Decapod runtime.
//!
//! Intent authority lives in [`IntentRecord`]. Trajectories are explicitly
//! evidence: they can explain what happened, but they cannot approve scope or
//! complete an intent. The host runtime owns persistence and decides when to
//! call this model at execution boundaries.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

pub const CUSTODY_SCHEMA_VERSION: &str = "1.0.0";

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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntentRecord {
    pub id: String,
    /// The human request, preserved byte-for-byte by the custody layer.
    pub raw_intent: String,
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
mod tests {
    use super::*;

    fn refinement() -> IntentRefinement {
        IntentRefinement {
            refined_intent: "ship the bounded change".to_string(),
            acceptance_criteria: vec!["tests pass".to_string()],
            constraints: vec!["no orchestration".to_string()],
            assumptions: vec!["current runtime owns persistence".to_string()],
            boundaries: vec!["src/**".to_string()],
            out_of_scope: vec!["database storage".to_string()],
            proof_requirements: vec!["unit tests".to_string()],
        }
    }

    #[test]
    fn lifecycle_preserves_raw_intent_and_orders_events() {
        let raw = "  keep this request exactly  ".to_string();
        let mut custody = Custody::in_memory();
        custody.create_intent("intent-1", raw.clone()).unwrap();
        custody.refine("intent-1", refinement()).unwrap();
        custody.approve("intent-1").unwrap();
        custody.approve_scope("intent-1").unwrap();
        custody.start_mutation("intent-1").unwrap();
        custody
            .record_proof("intent-1", "tests".to_string())
            .unwrap();
        custody
            .record_validation("intent-1", "validate".to_string())
            .unwrap();
        custody.complete("intent-1").unwrap();

        let record = &custody.state().intents["intent-1"];
        assert_eq!(record.raw_intent, raw);
        assert_eq!(record.status, IntentStatus::Completed);
        assert!(
            custody
                .state()
                .events
                .windows(2)
                .all(|events| events[0].sequence < events[1].sequence)
        );
    }

    #[test]
    fn invalid_transition_is_typed_and_trajectory_is_evidence() {
        let mut custody = Custody::in_memory();
        custody
            .create_intent("intent-2", "request".to_string())
            .unwrap();
        assert!(matches!(
            custody.approve("intent-2"),
            Err(CustodyError::InvalidTransition { .. })
        ));
        custody.refine("intent-2", refinement()).unwrap();
        let event_id = custody
            .append_trajectory_step(
                "run-2",
                "intent-2",
                TrajectoryStepInput {
                    action: "cargo test".to_string(),
                    command: Some("cargo test".to_string()),
                    scope: vec!["src/**".to_string()],
                    proof_refs: vec!["test-output".to_string()],
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            custody.state().intents["intent-2"].status,
            IntentStatus::Refined
        );
        assert_eq!(
            custody.state().trajectories["run-2"].steps[0].custody_event_id,
            event_id
        );
    }

    #[test]
    fn contaminated_summary_and_knowledge_candidate_remain_explicit() {
        let mut custody = Custody::in_memory();
        custody
            .create_intent("intent-3", "request".to_string())
            .unwrap();
        custody.refine("intent-3", refinement()).unwrap();
        custody
            .contaminate("intent-3", "untrusted input observed".to_string())
            .unwrap();
        let candidate = custody
            .propose_knowledge_candidate(
                "intent-3",
                "keep external artifacts quarantined".to_string(),
                vec!["event:3".to_string()],
            )
            .unwrap();
        let summary = custody.summarize("intent-3").unwrap();
        assert_eq!(summary.status, IntentStatus::Contaminated);
        assert_eq!(summary.knowledge_candidate_ids, vec![candidate.id]);
        assert_eq!(candidate.authority, "proposed");
    }
}

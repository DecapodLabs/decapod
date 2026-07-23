//! Host integration for the intent custody model.
//!
//! The trajectory subsystem owns the durable file and invokes these helpers
//! at its existing init/record boundaries. The model lives in the root crate
//! alongside the rest of Decapod's core modules.

#[path = "custody_model.rs"]
mod custody_model;

pub use custody_model::{
    CUSTODY_SCHEMA_VERSION, Custody, CustodyError, CustodyState, InMemoryCustodyStore, IntentEvent,
    IntentEventKind, IntentRecord, IntentRefinement, IntentStatus, IntentSummary,
    ProjectKnowledgeCandidate, Trajectory, TrajectoryStep, TrajectoryStepInput,
};

pub fn bootstrap_intent(
    intent_id: &str,
    raw_intent: String,
    refined_intent: String,
    boundaries: Vec<String>,
    repo_scope: Vec<String>,
) -> Result<CustodyState, CustodyError> {
    let mut custody = Custody::in_memory();
    custody.create_intent(intent_id, raw_intent)?;
    custody.refine(
        intent_id,
        IntentRefinement {
            refined_intent,
            boundaries,
            constraints: repo_scope,
            ..Default::default()
        },
    )?;
    Ok(custody.into_state())
}

pub fn append_trajectory_step(
    state: CustodyState,
    trajectory_id: &str,
    intent_id: &str,
    input: TrajectoryStepInput,
) -> Result<(CustodyState, String), CustodyError> {
    let mut custody = Custody::from_state(state);
    let event_id = custody.append_trajectory_step(trajectory_id, intent_id, input)?;
    Ok((custody.into_state(), event_id))
}

//! Model-independent post-proof context and custody used by Laya.
//! No model installation, inference, calibration or proof authority lives here.
use super::{
    bounded_process::Cancellation,
    readiness_evidence::{self as evidence, MaterialIdentity},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    time::Instant,
};
const MAX_INPUT: usize = 32 * 1024;
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Disabled,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RuntimePin {
    pub runtime: PathBuf,
    pub runtime_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Installation {
    pub directory: PathBuf,
    pub manifest_digest: String,
    #[serde(default)]
    pub runtime: PathBuf,
    #[serde(default)]
    pub runtime_sha256: String,
    /// Separate CPU-only executable when GPU libraries prevent worker startup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_runtime: Option<RuntimePin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub material: MaterialIdentity,
    pub governance_digest: String,
    pub proof_plan_digest: String,
    pub evidence_digest: String,
    pub configuration_digest: String,
    pub trajectory_run_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Disabled,
    Accept,
    Reject,
    Abstain,
}

/// Shared final consumption rule. Only the live validator can establish that
/// proof is current and authenticated; a replay's boolean grants no authority.
pub fn permits_decision(proof_ready: bool, mode: Mode, decision: Decision) -> bool {
    proof_ready
        && (decision == Decision::Accept
            || (mode == Mode::Disabled && decision == Decision::Disabled))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

pub fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn digest<T: Serialize>(value: &T) -> io::Result<String> {
    evidence::digest(value).map_err(io::Error::other)
}

/// Actual validated observations, never a model-authored summary or a green flag.
pub struct ValidationSummary<'a> {
    pub scope: &'a str,
    pub evidence: &'a evidence::SuiteEvidence,
    pub dispositions: &'a [super::readiness_profile::StageDisposition],
    pub blockers: &'a [super::validation_runtime::Blocker],
}

/// Exact governed input types shared by live assessment and replay evaluation.
/// Deserializing these records is not authentication of their origin or proof
/// of execution. Evaluators must establish that custody independently.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionContext {
    plan: crate::plan_governance::GovernedPlan,
    trajectory: super::trajectory::TrajectoryArtifact,
}

/// Private replay input for evaluating the actual production representation.
/// The expected plan must be derived independently of returned observations.
/// These unsigned records do not establish authentic execution, independently
/// adjudicated labels, current task ownership or permission to publish.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluationSnapshot {
    pub binding: Binding,
    pub context: ProjectionContext,
    pub expected_plan: evidence::ExpectedPlan,
    pub evidence: evidence::SuiteEvidence,
    pub scope: String,
    pub dispositions: Vec<super::readiness_profile::StageDisposition>,
    pub blockers: Vec<super::validation_runtime::Blocker>,
}
impl EvaluationSnapshot {
    /// Match the production call order: complete deterministic evidence first,
    /// then exactly the shared bounded projection. Callers must not dispatch
    /// Laya when this fails. This is a replay check, never live readiness.
    pub fn project(&self) -> io::Result<String> {
        let checked = evidence::verify(
            &self.expected_plan,
            &self.binding.material,
            &self.evidence.facts,
            &self.evidence,
        );
        if checked.verdict != evidence::Verdict::Ready || !self.blockers.is_empty() {
            return Err(invalid(format!(
                "deterministic_validation_not_ready: {:?}",
                checked.failures
            )));
        }
        project(
            &self.binding,
            &ValidationSummary {
                scope: &self.scope,
                evidence: &self.evidence,
                dispositions: &self.dispositions,
                blockers: &self.blockers,
            },
            &self.context,
        )
    }
}
fn bounded_projection(value: &serde_json::Value) -> io::Result<String> {
    // The Laya SDK enables serde_json/preserve_order in its separate worker.
    // Model input must remain byte-identical to the controller's projection
    // regardless of Cargo feature unification. Preserve arrays and text.
    let mut value = value.clone();
    value.sort_all_objects();
    let state = serde_json::to_string(&value).map_err(io::Error::other)?;
    if state.len() > MAX_INPUT {
        return Err(invalid("projection_overflow_no_truncation"));
    }
    Ok(state)
}
/// Keep explicit acceptance obligations rather than inferring them from a
/// successful check list. Administrative plan metadata is not model input.
fn plan_obligations(plan: &crate::plan_governance::GovernedPlan) -> serde_json::Value {
    let gates = |gates: &[crate::plan_governance::Gate]| {
        gates
            .iter()
            .map(|gate| {
                serde_json::json!({
                    "description":gate.description,
                    "required_artifacts":gate.required_artifacts,
                    "validation_checks":gate.validation_checks,
                    "satisfied":gate.satisfied
                })
            })
            .collect::<Vec<_>>()
    };
    serde_json::json!({
        "proof_hooks":plan.proof_hooks,
        "unknowns":plan.unknowns,
        "human_questions":plan.human_questions,
        "stop_conditions":plan.stop_conditions,
        "unresolved_contradictions":plan.unresolved_contradictions,
        "deferred_questions":plan.deferred_questions,
        "phases":plan.phases.iter().map(|phase| serde_json::json!({
            "id":phase.id,"name":phase.name,"description":phase.description,
            "entered":phase.entered,"completed":phase.completed,
            "entry_gates":gates(&phase.entry_gates),
            "exit_gates":gates(&phase.exit_gates)
        })).collect::<Vec<_>>()
    })
}
pub(crate) fn projection(
    root: &Path,
    binding: &Binding,
    validation: &ValidationSummary<'_>,
) -> io::Result<String> {
    let plan = crate::plan_governance::load_plan(root)
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing_intent"))?;
    let trajectory = super::trajectory::load_trajectory_cookie(root)
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing_trajectory"))?;
    project(binding, validation, &ProjectionContext { plan, trajectory })
}

/// Build the production validation-results.v3 representation, without an
/// evaluation-specific approximation. The validation caller must first verify
/// its complete proof plan and must recheck custody after inference. A replay
/// cannot authorize publication, even when this projection and Laya accept.
pub fn project(
    binding: &Binding,
    validation: &ValidationSummary<'_>,
    context: &ProjectionContext,
) -> io::Result<String> {
    let results = validation_projection(binding, validation)?;
    let ProjectionContext { plan, trajectory } = context;
    if plan.intent.trim().is_empty() {
        return Err(invalid("missing_intent"));
    }
    if trajectory.run_id != binding.trajectory_run_id {
        return Err(invalid("stale_trajectory"));
    }
    bounded_projection(&serde_json::json!({
        "projection":"validation-results.v3",
        "publication_actor":"agent through existing validation gates",
        "intent":plan.intent,"validation":results,
        "plan_obligations":plan_obligations(plan),
        "unresolved_assumptions":trajectory.unresolved_assumptions,
        "shortcut_risk_signals":trajectory.shortcut_risk_signals,
        "trajectory_blockers":trajectory.blockers,
        "coverage":"all check outcomes, stage dispositions, declared plan obligations and adverse facts; repeated facts are indexed once. Plan declarations and gate satisfaction flags are not independent execution proof. Exact custody hashes bind the complete evidence outside this prompt. No raw logs, administrative plan metadata or source diff; unrepresented warnings and untested intent remain unknown"
    }))
}

/// Keep observed outcomes, including every failed/pending/skipped substep. The
/// cryptographic custody and immutable identities remain bound by Request and
/// Receipt, rather than consuming the model's small reasoning context repeatedly.
fn observation_projection(observation: &evidence::Observation) -> serde_json::Value {
    use evidence::Observation;
    use serde_json::json;
    match observation {
        Observation::Executed {
            conclusion,
            exit_code,
            ..
        } => json!({
            "kind":"executed", "conclusion":conclusion, "exit_code":exit_code
        }),
        Observation::CiAttested { attestation } => json!({
            "kind":"ci_attested", "run_conclusion":attestation.run_conclusion,
            "jobs":attestation.jobs.iter().map(|job| json!({
                "step":job.step_key, "conclusion":job.conclusion
            })).collect::<Vec<_>>()
        }),
        Observation::GithubActions { attestation } => json!({
            "kind":"github_actions", "run_conclusion":attestation.run_conclusion,
            "job_conclusion":attestation.job_conclusion, "steps":attestation.steps
        }),
        Observation::NotApplicable { .. } => json!({"kind":"not_applicable"}),
        Observation::Reused { .. } => json!({"kind":"reused"}),
    }
}

/// Lossless interning of semantic observations and disposition definitions. No
/// conclusion, reason, exclusion, simulation, risk signal or unknown is clipped.
fn intern(values: &mut Vec<serde_json::Value>, value: serde_json::Value) -> usize {
    if let Some(index) = values.iter().position(|known| known == &value) {
        index
    } else {
        let index = values.len();
        values.push(value);
        index
    }
}

fn validation_projection(
    binding: &Binding,
    validation: &ValidationSummary<'_>,
) -> io::Result<serde_json::Value> {
    if digest(validation.evidence)? != binding.evidence_digest
        || validation.evidence.expected_plan_digest != binding.proof_plan_digest
        || validation.evidence.after != binding.material
        || validation.evidence.before != binding.material
    {
        return Err(invalid("validation_projection_binding_mismatch"));
    }
    use serde_json::json;
    let mut observations = Vec::new();
    let mut check_templates = Vec::new();
    let mut check_members: Vec<Vec<serde_json::Value>> = Vec::new();
    for (index, row) in validation.evidence.results.iter().enumerate() {
        let observation = intern(&mut observations, observation_projection(&row.observation));
        let group = intern(
            &mut check_templates,
            json!({
                "provider":row.key.provider, "workflow":row.key.workflow,
                "profile":row.key.profile, "executor":row.executor,
                "observation":observation
            }),
        );
        if group == check_members.len() {
            check_members.push(Vec::new());
        }
        check_members[group].push(json!({
            "index":index, "job":row.key.job, "matrix":row.key.matrix
        }));
    }
    for (template, members) in check_templates.iter_mut().zip(check_members) {
        template["checks"] = json!(members);
    }
    let mut disposition_templates = Vec::new();
    let mut disposition_members: Vec<Vec<serde_json::Value>> = Vec::new();
    for row in validation.dispositions {
        let group = intern(
            &mut disposition_templates,
            json!({
                "stage":row.stage, "disposition":row.disposition, "policy":row.policy,
                "reason":row.reason, "evidence_scope":row.evidence_scope
            }),
        );
        if group == disposition_members.len() {
            disposition_members.push(Vec::new());
        }
        let reference = row
            .key
            .as_ref()
            .map(|key| {
                validation
                    .evidence
                    .results
                    .iter()
                    .position(|result| &result.key == key)
                    .map(|index| json!({"check_index":index}))
                    .unwrap_or_else(|| json!({"unobserved_check":key}))
            })
            .unwrap_or(serde_json::Value::Null);
        disposition_members[group].push(reference);
    }
    for (template, members) in disposition_templates.iter_mut().zip(disposition_members) {
        template["checks"] = json!(members);
    }
    Ok(serde_json::json!({
        "scope":validation.scope,
        "check_count":validation.evidence.results.len(),
        "check_groups":check_templates,"observations":observations,
        "stage_dispositions":disposition_templates,
        "validation_blockers":validation.blockers,
        "changed_paths":validation.evidence.facts.changed_paths
    }))
}
fn pinned_runtime(
    path: &Path,
    expected: &str,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    if !super::model_assets::valid_digest(expected)
        || super::laya_runtime::runtime_digest(path, deadline, cancel)? != expected
    {
        return Err(invalid("runtime_digest_mismatch"));
    }
    Ok(())
}

pub(crate) fn verify_runtime_pins(
    installation: &Installation,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    pinned_runtime(
        &installation.runtime,
        &installation.runtime_sha256,
        deadline,
        cancel,
    )?;
    if let Some(cpu) = &installation.cpu_runtime {
        pinned_runtime(&cpu.runtime, &cpu.runtime_sha256, deadline, cancel)?;
    }
    Ok(())
}

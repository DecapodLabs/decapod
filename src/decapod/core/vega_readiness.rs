//! vega is an untrusted post-proof readiness assessor, never a proof authority.
use super::readiness_evidence::MaterialIdentity;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SCHEMA: &str = "decapod.vega-readiness.v1";
pub const WORKER_SCHEMA: &str = "decapod.vega-worker.v2";

/// Portable repository preference. The worker must disclose the actual device;
/// it cannot silently replace this request or choose a different checkpoint.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendPreference {
    #[default]
    Auto,
    Metal,
    Cuda,
    Cpu,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Metal,
    Cuda,
    Cpu,
}
impl Backend {
    pub fn device(self) -> &'static str {
        match self {
            Self::Metal => "metal",
            Self::Cuda => "cuda",
            Self::Cpu => "cpu",
        }
    }
    pub fn runtime(self) -> &'static str {
        match self {
            Self::Metal => "candle-metal",
            Self::Cuda => "candle-cuda",
            Self::Cpu => "candle-cpu",
        }
    }
}
fn default_cpu_fallback() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}
fn is_auto(value: &BackendPreference) -> bool {
    *value == BackendPreference::Auto
}
impl BackendPreference {
    pub fn resolve(self) -> Backend {
        match self {
            Self::Auto if cfg!(target_os = "macos") => Backend::Metal,
            Self::Auto | Self::Cuda => Backend::Cuda,
            Self::Metal => Backend::Metal,
            Self::Cpu => Backend::Cpu,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorKind {
    HardwareUnavailable,
    InferenceFailed,
}

pub const BACKEND_CHECK_SCHEMA: &str = "decapod.vega-backend-check.v1";
/// Model-free startup only. This carries no model input or readiness answer.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendCheck {
    pub schema: String,
    pub backend: Backend,
    pub available: bool,
    pub error_kind: Option<WorkerErrorKind>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptPhase {
    Capability,
    #[default]
    Inference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendAttempt {
    pub backend: Backend,
    #[serde(default)]
    pub phase: AttemptPhase,
    #[serde(default)]
    pub runtime_sha256: String,
    pub request_id: String,
    pub error_kind: Option<WorkerErrorKind>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Disabled,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub mode: Mode,
    #[serde(skip_serializing_if = "is_auto")]
    pub backend: BackendPreference,
    #[serde(default = "default_cpu_fallback", skip_serializing_if = "is_true")]
    pub cpu_fallback: bool,
    /// Shared immutable model identity; local executable paths stay out of Git.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model_manifest_digest: String,
    pub installation: Option<Installation>,
    /// A key actually present in the pinned conformal table, not a guarantee
    /// of code-readiness coverage under distribution shift.
    pub alpha: String,
    pub risk: String,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            mode: Mode::Disabled,
            backend: BackendPreference::Auto,
            cpu_fallback: true,
            model_manifest_digest: String::new(),
            installation: None,
            alpha: String::new(),
            risk: String::new(),
        }
    }
}
impl Configuration {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    /// Fixed label order [false, true], matching upstream [no, yes].
    pub probabilities: [f64; 2],
    pub prediction_set: Option<Vec<bool>>,
    pub abstain: bool,
    pub unbound: bool,
    pub settle_steps: usize,
    pub energy: f64,
    pub kinetic: f64,
    pub nearest: f64,
    pub confidence: f64,
}
impl Outcome {
    pub fn decision(&self) -> Result<Decision, &'static str> {
        let [no, yes] = self.probabilities;
        if !no.is_finite()
            || !yes.is_finite()
            || !(0.0..=1.0).contains(&no)
            || !(0.0..=1.0).contains(&yes)
            || (no + yes - 1.0).abs() > 1e-8
            || !self.confidence.is_finite()
            || (self.confidence - no.max(yes)).abs() > 1e-8
            || !self.energy.is_finite()
            || !self.kinetic.is_finite()
            || self.kinetic < 0.0
            || !self.nearest.is_finite()
            || self.nearest < 0.0
            || !(1..=12).contains(&self.settle_steps)
        {
            return Err("invalid_vega_numerical_output");
        }
        if self.abstain || self.unbound {
            return Ok(Decision::Abstain);
        }
        match self.prediction_set.as_deref() {
            Some([true]) if yes > no => Ok(Decision::Accept),
            Some([false]) if no >= yes => Ok(Decision::Reject),
            Some([true]) | Some([false]) => Err("prediction_set_disagrees_with_top_label"),
            Some([false, true]) | Some([true, false]) | None => Ok(Decision::Abstain),
            _ => Err("invalid_prediction_set"),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub schema: String,
    pub request_id: String,
    pub input_digest: String,
    pub installation: PathBuf,
    pub manifest_digest: String,
    pub state: String,
    pub alpha: String,
    pub risk: String,
    pub backend: Backend,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub schema: String,
    pub request_id: String,
    pub input_digest: String,
    pub manifest_digest: String,
    pub backend: String,
    pub device: String,
    pub engine_device: String,
    pub precision: Precision,
    pub input_tokens: usize,
    pub outcome: Option<Outcome>,
    pub error: Option<String>,
    pub error_kind: Option<WorkerErrorKind>,
}
impl Response {
    /// Interpret the worker through the same binding, backend and numerical
    /// checks in production and offline evaluation. This does not verify the
    /// caller's deterministic proof, installation or current Git material.
    pub fn validated_outcome(&self, request: &Request) -> io::Result<&Outcome> {
        self.validate_binding(request)?;
        if let Some(error) = &self.error {
            return Err(invalid(format!(
                "vega_native_abstained:{}",
                error.chars().take(512).collect::<String>()
            )));
        }
        if self.error_kind.is_some() {
            return Err(invalid("vega_worker_error_without_message"));
        }
        if self.input_tokens == 0 || self.input_tokens > super::vega_prompt::MAX_TOKENS {
            return Err(invalid("vega_response_token_count_invalid"));
        }
        let outcome = self
            .outcome
            .as_ref()
            .ok_or_else(|| invalid("vega_outcome_missing"))?;
        outcome.decision().map_err(invalid)?;
        Ok(outcome)
    }
    fn validate_binding(&self, request: &Request) -> io::Result<()> {
        if self.schema != WORKER_SCHEMA
            || self.request_id != request.request_id
            || self.input_digest != request.input_digest
            || self.manifest_digest != request.manifest_digest
            || self.backend != request.backend.runtime()
            || self.device != request.backend.device()
            || self.engine_device != "cpu"
            || self.precision != Precision::supported()
        {
            return Err(invalid("vega_response_binding_or_backend_invalid"));
        }
        Ok(())
    }
    /// A typed hardware failure is the only replayable result. It must contain
    /// no partial model answer or tokenization evidence. A normal abstention,
    /// malformed IPC, crash, timeout, negative verdict or arithmetic failure
    /// never authorizes another attempt to seek a positive answer.
    /// This classifier grants no validation or publication authority. Native
    /// clients also use it to verify fallback after a real unsupported backend.
    pub fn permits_cpu_retry(&self, requested: Backend, enabled: bool) -> bool {
        enabled
            && requested != Backend::Cpu
            && self.error_kind == Some(WorkerErrorKind::HardwareUnavailable)
            && self
                .error
                .as_ref()
                .is_some_and(|error| !error.is_empty() && error.len() <= 512)
            && self.outcome.is_none()
            && self.input_tokens == 0
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Precision {
    pub backbone: String,
    pub engine: String,
    pub calibration: String,
}
impl Precision {
    pub fn supported() -> Self {
        Self {
            backbone: "f32".into(),
            engine: "f32".into(),
            calibration: "f64".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: String,
    pub decapod_version: String,
    pub request_id: String,
    pub binding: Binding,
    pub rubric: String,
    pub input_digest: String,
    pub input_bytes: usize,
    pub mode: Mode,
    pub decision: Decision,
    pub reason: String,
    pub outcome: Option<Outcome>,
    pub alpha: String,
    pub risk: String,
    pub manifest: Option<serde_json::Value>,
    pub runtime_sha256: Option<String>,
    pub backend: Option<String>,
    pub device: Option<String>,
    pub engine_device: Option<String>,
    pub precision: Option<Precision>,
    pub input_tokens: Option<usize>,
    #[serde(default)]
    pub backend_attempts: Vec<BackendAttempt>,
    pub calibration_scope: String,
}
impl Receipt {
    pub fn permits_readiness(&self) -> bool {
        self.decision == Decision::Accept
            || (self.mode == Mode::Disabled && self.decision == Decision::Disabled)
    }
    pub fn stale(&mut self) {
        self.decision = Decision::Abstain;
        self.reason = "state_or_model_changed_during_inference".into();
    }
}

pub(crate) fn permits_publication(proof_ready: bool, receipt: &Receipt) -> bool {
    proof_ready && receipt.permits_readiness()
}

use super::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use super::readiness_evidence as evidence;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
const MAX_INPUT: usize = 32 * 1024;
fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
pub fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn digest<T: Serialize>(value: &T) -> io::Result<String> {
    evidence::digest(value).map_err(io::Error::other)
}

pub(crate) fn configuration(root: &Path) -> io::Result<Configuration> {
    let mut bytes = Vec::new();
    fs::File::open(root.join(".decapod/config.toml"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    configuration_from_bytes(&bytes)
}

pub(crate) fn configuration_from_bytes(bytes: &[u8]) -> io::Result<Configuration> {
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("vega_configuration_overflow"));
    }
    let value: toml::Value = toml::from_str(std::str::from_utf8(bytes).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    let mut cfg: Configuration = value
        .get("vega")
        .cloned()
        .map(|v| v.try_into().map_err(io::Error::other))
        .transpose()?
        .unwrap_or_default();
    if cfg.alpha.is_empty() {
        cfg.alpha = "0.1".into();
    }
    if cfg.risk.is_empty() {
        cfg.risk = "0.1".into();
    }
    if !matches!(cfg.alpha.as_str(), "0.1" | "0.05")
        || !matches!(cfg.risk.as_str(), "0.05" | "0.1" | "0.2")
    {
        return Err(invalid("unsupported_vega_alpha_or_risk"));
    }
    if !cfg.model_manifest_digest.is_empty()
        && !super::vega_setup::valid_digest(&cfg.model_manifest_digest)
    {
        return Err(invalid("invalid_vega_model_manifest_digest"));
    }
    Ok(cfg)
}

/// Required accepted policy is authority, not a setting a candidate may weaken
/// or replace with its own executable. New optional use can add a gate, but a
/// required policy change needs an independently accepted policy transition.
pub(crate) fn verify_policy_acceptance(
    candidate: &Configuration,
    accepted: &Configuration,
) -> io::Result<()> {
    if accepted.mode == Mode::Required && candidate != accepted {
        return Err(invalid(
            "VEGA_ACCEPTED_POLICY_CHANGED: candidate cannot disable or repin the accepted required vega policy; obtain independent acceptance for the policy transition",
        ));
    }
    Ok(())
}
fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
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
    /// Vega when this fails. This is a replay check, never live readiness.
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
    let state = serde_json::to_string(value).map_err(io::Error::other)?;
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
fn projection(
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
/// cannot authorize publication, even when this projection and Vega accept.
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
    if !super::vega_setup::valid_digest(expected)
        || super::vega_runtime::runtime_digest(path, deadline, cancel)? != expected
    {
        return Err(invalid("runtime_digest_mismatch"));
    }
    Ok(())
}

fn verify_runtime_pins(
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
fn runtime_for(installation: &Installation, backend: Backend) -> (&Path, &str) {
    if backend == Backend::Cpu
        && let Some(cpu) = &installation.cpu_runtime
    {
        return (&cpu.runtime, &cpu.runtime_sha256);
    }
    (&installation.runtime, &installation.runtime_sha256)
}

#[derive(Debug)]
enum GpuStartup {
    Available,
    Unavailable {
        reason: String,
        separate_cpu_required: bool,
    },
}
fn backend_check_response(bytes: &[u8], backend: Backend) -> io::Result<GpuStartup> {
    let check: BackendCheck = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    if check.schema != BACKEND_CHECK_SCHEMA || check.backend != backend || backend == Backend::Cpu {
        return Err(invalid("vega_backend_check_binding_invalid"));
    }
    match (check.available, check.error_kind, check.error) {
        (true, None, None) => Ok(GpuStartup::Available),
        (false, Some(WorkerErrorKind::HardwareUnavailable), Some(reason))
            if !reason.is_empty() && reason.len() <= 512 =>
        {
            Ok(GpuStartup::Unavailable {
                reason,
                separate_cpu_required: false,
            })
        }
        _ => Err(invalid("vega_backend_check_failed_or_malformed")),
    }
}

/// No model paths, input state or readiness request is supplied to this process.
/// Only typed hardware unavailability or the loader's empty exit127 can select
/// another executable. Crashes, timeouts, malformed reports and compute errors
/// remain failures. Inference outcomes never enter this startup classifier.
fn check_gpu_startup(
    installation: &Installation,
    backend: Backend,
    directory: &Path,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<GpuStartup> {
    let capture = Command::new(&installation.runtime)
        .env_clear()
        .args(["--backend-check", backend.device()])
        .bounded_capture(
            ProcessLimits {
                timeout: deadline
                    .checked_duration_since(Instant::now())
                    .ok_or_else(|| invalid("vega_timeout"))?
                    .min(Duration::from_secs(10)),
                max_output_bytes: 16 * 1024,
            },
            cancel,
        );
    let capture = match capture {
        // A verified ELF executable with an absent interpreter reports ENOENT.
        // Every pin is checked again before the separately registered CPU starts.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(GpuStartup::Unavailable {
                reason: "gpu_worker_loader_unavailable".into(),
                separate_cpu_required: true,
            });
        }
        other => other?,
    };
    private_write(
        &directory.join("vega-capability.stdout"),
        &capture.output.stdout,
    )?;
    private_write(
        &directory.join("vega-capability.stderr"),
        &capture.output.stderr,
    )?;
    if capture.termination != Termination::Exited {
        return Err(invalid("vega_backend_check_cancelled_or_timeout"));
    }
    if !capture.output.status.success() {
        if capture.output.status.code() == Some(127) && capture.output.stdout.is_empty() {
            return Ok(GpuStartup::Unavailable {
                reason: "gpu_worker_loader_exit127".into(),
                separate_cpu_required: true,
            });
        }
        return Err(invalid("vega_backend_check_process_failed"));
    }
    backend_check_response(&capture.output.stdout, backend)
}

pub(crate) fn capture_installation(config: &Configuration) -> io::Result<Option<Installation>> {
    if config.mode == Mode::Disabled {
        return Ok(None);
    }
    let installation = match &config.installation {
        Some(installation) => installation.clone(),
        None => super::vega_runtime::resolve(&config.model_manifest_digest)?,
    };
    if !config.model_manifest_digest.is_empty()
        && config.model_manifest_digest != installation.manifest_digest
    {
        return Err(invalid("vega_shared_local_manifest_mismatch"));
    }
    Ok(Some(installation))
}

pub(crate) fn verify_installation_snapshot(
    config: &Configuration,
    captured: &io::Result<Option<Installation>>,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    let installation = captured
        .as_ref()
        .map_err(|error| invalid(error.to_string()))?
        .as_ref()
        .ok_or_else(|| invalid("vega_installation_not_captured"))?;
    if capture_installation(config)?.as_ref() != Some(installation) {
        return Err(invalid("vega_local_binding_changed_during_validation"));
    }
    super::vega_setup::verify_installation_bounded(
        &installation.directory,
        &installation.manifest_digest,
        deadline,
        cancel,
    )?;
    verify_runtime_pins(installation, deadline, cancel)
}

/// Shared policy and per-maintainer custody captured at validation entry.
pub(crate) struct AssessmentConfig<'a> {
    pub policy: &'a Configuration,
    pub captured_installation: &'a io::Result<Option<Installation>>,
}

/// Sole caller: validation_runtime after `readiness_profile::finish` succeeds.
/// Caller must independently re-run proof and local custody freshness afterwards.
pub(crate) fn assess(
    root: &Path,
    directory: &Path,
    selected: AssessmentConfig<'_>,
    validation: &ValidationSummary<'_>,
    mut binding: Binding,
    timeout: Duration,
    cancel: &Cancellation,
) -> io::Result<Receipt> {
    let AssessmentConfig {
        policy: config,
        captured_installation,
    } = selected;
    binding.configuration_digest = digest(&config)?;
    let mut receipt = Receipt {
        schema: SCHEMA.into(),
        decapod_version: env!("CARGO_PKG_VERSION").into(),
        request_id: digest(&binding)?,
        binding,
        rubric: super::vega_prompt::RUBRIC.into(),
        input_digest: hash(b""),
        input_bytes: 0,
        mode: config.mode,
        decision: Decision::Abstain,
        reason: "not_evaluated".into(),
        outcome: None,
        alpha: config.alpha.clone(),
        risk: config.risk.clone(),
        manifest: None,
        runtime_sha256: None,
        backend: None,
        device: None,
        engine_device: None,
        precision: None,
        input_tokens: None,
        backend_attempts: Vec::new(),
        calibration_scope:
            "upstream-general-decisions; code-readiness coverage/risk not established".into(),
    };
    if config.mode == Mode::Disabled {
        receipt.decision = Decision::Disabled;
        receipt.reason = "disabled".into();
        return Ok(receipt);
    }
    let deadline = Instant::now() + timeout.min(Duration::from_secs(120));
    let evaluated = (|| -> io::Result<()> {
        let install = captured_installation
            .as_ref()
            .map_err(|error| invalid(error.to_string()))?
            .as_ref()
            .ok_or_else(|| invalid("vega_installation_not_captured"))?;
        if capture_installation(config)?.as_ref() != Some(install) {
            return Err(invalid("vega_local_binding_changed_during_validation"));
        }
        let manifest = super::vega_setup::verify_installation_bounded(
            &install.directory,
            &install.manifest_digest,
            deadline,
            cancel,
        )?;
        receipt.manifest = Some(serde_json::to_value(&manifest).map_err(io::Error::other)?);
        verify_runtime_pins(install, deadline, cancel)?;
        receipt.runtime_sha256 = Some(install.runtime_sha256.clone());
        let mut backend = config.backend.resolve();
        if backend != Backend::Cpu {
            let startup = check_gpu_startup(install, backend, directory, deadline, cancel)?;
            let (error_kind, error) = match &startup {
                GpuStartup::Available => (None, None),
                GpuStartup::Unavailable { reason, .. } => (
                    Some(WorkerErrorKind::HardwareUnavailable),
                    Some(reason.clone()),
                ),
            };
            receipt.backend_attempts.push(BackendAttempt {
                backend,
                phase: AttemptPhase::Capability,
                runtime_sha256: install.runtime_sha256.clone(),
                request_id: digest(&(
                    &receipt.binding,
                    "model-free-startup",
                    backend,
                    &install.runtime_sha256,
                ))?,
                error_kind,
                error,
            });
            if let GpuStartup::Unavailable {
                separate_cpu_required,
                ..
            } = startup
            {
                if !config.cpu_fallback {
                    return Err(invalid("vega_gpu_unavailable_cpu_fallback_disabled"));
                }
                if separate_cpu_required && install.cpu_runtime.is_none() {
                    return Err(invalid(
                        "vega_cpu_runtime_registration_required: setup vega --runtime <gpu-worker> --cpu-runtime <cpu-only-worker>",
                    ));
                }
                verify_installation_snapshot(config, captured_installation, deadline, cancel)?;
                backend = Backend::Cpu;
            }
        }
        deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| invalid("vega_timeout"))?;
        let state = projection(root, &receipt.binding, validation)?;
        receipt.input_digest = hash(state.as_bytes());
        receipt.input_bytes = state.len();
        receipt.request_id = digest(&(
            &receipt.binding,
            &receipt.rubric,
            &receipt.input_digest,
            &receipt.decapod_version,
            &install.manifest_digest,
            install,
        ))?;
        let mut request = Request {
            schema: WORKER_SCHEMA.into(),
            request_id: receipt.request_id.clone(),
            input_digest: receipt.input_digest.clone(),
            installation: install.directory.clone(),
            manifest_digest: install.manifest_digest.clone(),
            state,
            alpha: config.alpha.clone(),
            risk: config.risk.clone(),
            backend,
        };
        let mut response = None;
        for attempt in 0..2 {
            // Every retry is a fresh process with a separately bound immutable input.
            // All attempts share the original overall deadline and frozen pins.
            let (runtime, runtime_sha256) = runtime_for(install, request.backend);
            receipt.runtime_sha256 = Some(runtime_sha256.to_owned());
            request.request_id = digest(&(
                &receipt.request_id,
                attempt,
                request.backend,
                runtime_sha256,
            ))?;
            let path = directory.join(format!("vega-input-{attempt}.json"));
            private_write(
                &path,
                &serde_json::to_vec(&request).map_err(io::Error::other)?,
            )?;
            let capture = Command::new(runtime)
                .env_clear()
                .arg("--request")
                .arg(&path)
                .bounded_capture(
                    ProcessLimits {
                        timeout: deadline
                            .checked_duration_since(Instant::now())
                            .ok_or_else(|| invalid("vega_timeout"))?,
                        max_output_bytes: 16 * 1024,
                    },
                    cancel,
                )?;
            if capture.termination != Termination::Exited || !capture.output.status.success() {
                return Err(invalid("vega_worker_failed_or_timeout"));
            }
            let current: Response =
                serde_json::from_slice(&capture.output.stdout).map_err(io::Error::other)?;
            current.validate_binding(&request)?;
            receipt.backend_attempts.push(BackendAttempt {
                backend: request.backend,
                phase: AttemptPhase::Inference,
                runtime_sha256: runtime_sha256.to_owned(),
                request_id: request.request_id.clone(),
                error_kind: current.error_kind,
                error: current
                    .error
                    .as_ref()
                    .map(|error| error.chars().take(512).collect()),
            });
            if attempt == 0 && current.permits_cpu_retry(request.backend, config.cpu_fallback) {
                // Re-verify before launching the independently owned CPU worker.
                super::vega_setup::verify_installation_bounded(
                    &install.directory,
                    &install.manifest_digest,
                    deadline,
                    cancel,
                )?;
                verify_runtime_pins(install, deadline, cancel)?;
                if capture_installation(config)?.as_ref() != Some(install) {
                    return Err(invalid("vega_local_binding_changed_before_fallback"));
                }
                request.backend = Backend::Cpu;
                continue;
            }
            response = Some(current);
            break;
        }
        let response = response.ok_or_else(|| invalid("vega_worker_response_missing"))?;
        let outcome = response.validated_outcome(&request)?.clone();
        let decision = outcome.decision().map_err(invalid)?;
        // Recheck both immutable installation and executable after inference.
        super::vega_setup::verify_installation_bounded(
            &install.directory,
            &install.manifest_digest,
            deadline,
            cancel,
        )?;
        verify_runtime_pins(install, deadline, cancel)?;
        if capture_installation(config)?.as_ref() != Some(install) {
            return Err(invalid("vega_local_binding_changed_during_inference"));
        }
        receipt.decision = decision;
        receipt.reason = match decision {
            Decision::Accept => "positive_singleton",
            Decision::Reject => "negative_singleton",
            _ => "uncertain_abstaining_or_unbound",
        }
        .into();
        receipt.outcome = Some(outcome);
        receipt.backend = Some(response.backend);
        receipt.device = Some(response.device);
        receipt.engine_device = Some(response.engine_device);
        receipt.precision = Some(response.precision);
        receipt.input_tokens = Some(response.input_tokens);
        Ok(())
    })();
    if let Err(error) = evaluated {
        receipt.decision = Decision::Abstain;
        receipt.reason = error.to_string();
    }
    Ok(receipt)
}

#[cfg(test)]
mod projection_tests {
    use super::*;
    fn obligation_fixture() -> crate::plan_governance::GovernedPlan {
        serde_json::from_value(serde_json::json!({
            "schema_version":"1.1.0","title":"bounded readiness",
            "intent":"meet the declared requirements","state":"APPROVED",
            "updated_at":"123Z",
            "proof_hooks":["measure peak memory on the supported workload"],
            "unknowns":["supported workload not established"],
            "human_questions":["which latency bound applies?"],
            "stop_conditions":["stop if the workload cannot be reproduced"],
            "unresolved_contradictions":["two reports disagree about the memory limit"],
            "deferred_questions":["green tests do not establish the memory requirement"],
            "phases":[{
                "id":"measurement","name":"measure","description":"verify resource bounds",
                "entered":true,"completed":false,
                "entry_gates":[{"description":"pin workload","required_artifacts":["workload.json"],
                    "validation_checks":["integrity"],"satisfied":true,"satisfied_at":"122Z"}],
                "exit_gates":[{"description":"memory under limit","required_artifacts":["memory.json"],
                    "validation_checks":["peak_memory"],"satisfied":false}]
            }]
        })).unwrap()
    }
    #[test]
    fn plan_projection_retains_requirements_beyond_successful_checks() {
        let plan = obligation_fixture();
        let value = plan_obligations(&plan);
        for key in [
            "proof_hooks",
            "unknowns",
            "human_questions",
            "stop_conditions",
            "unresolved_contradictions",
            "deferred_questions",
        ] {
            assert_eq!(value[key], serde_json::to_value(&plan).unwrap()[key]);
        }
        let phase = &value["phases"][0];
        assert_eq!(phase["completed"], false);
        assert_eq!(phase["entry_gates"][0]["satisfied"], true);
        assert_eq!(phase["exit_gates"][0]["satisfied"], false);
        assert_eq!(
            phase["exit_gates"][0]["validation_checks"][0],
            "peak_memory"
        );
        assert_eq!(
            phase["exit_gates"][0]["required_artifacts"][0],
            "memory.json"
        );
        assert!(value.get("state").is_none());
        assert!(value.get("updated_at").is_none());
        assert!(phase["entry_gates"][0].get("satisfied_at").is_none());
    }
    #[test]
    fn oversized_deferred_obligation_cannot_be_silently_dropped() {
        let mut plan = obligation_fixture();
        plan.deferred_questions = vec!["unresolved requirement ".repeat(MAX_INPUT)];
        assert!(
            bounded_projection(&serde_json::json!({
                "intent":plan.intent,"plan_obligations":plan_obligations(&plan)
            }))
            .is_err()
        );
    }
    fn fixture() -> (Binding, evidence::SuiteEvidence) {
        let material = evidence::MaterialIdentity {
            repository: "example/repo".into(),
            work_branch: "agent/test/task".into(),
            head_revision: "a".repeat(40),
            base_branch: "master".into(),
            accepted_base_revision: "b".repeat(40),
            history_digest: hash(b"history"),
            tree_digest: hash(b"tree"),
            configuration_digest: hash(b"config"),
            workflow_digest: hash(b"workflow"),
            toolchain_digest: hash(b"toolchain"),
            candidate_digest: hash(b"candidate"),
        };
        let key = evidence::CheckKey {
            provider: evidence::Provider::Native,
            workflow: "local".into(),
            job: "tests".into(),
            matrix: Default::default(),
            profile: "required".into(),
        };
        let suite = evidence::SuiteEvidence {
            schema_version: evidence::SCHEMA_VERSION.into(),
            expected_plan_digest: hash(b"plan"),
            before: material.clone(),
            after: material.clone(),
            facts: evidence::ApplicabilityFacts {
                event: "pull_request".into(),
                target_branch: "master".into(),
                changed_paths: ["src/lib.rs".into()].into(),
            },
            results: vec![evidence::CheckResult {
                key,
                definition_digest: hash(b"def"),
                invocation_digest: hash(b"invoke"),
                toolchain_digest: hash(b"tools"),
                executor: evidence::Provider::Native,
                identity: material.clone(),
                observation: evidence::Observation::Executed {
                    execution_id: "run-1".into(),
                    conclusion: evidence::Conclusion::Failure,
                    exit_code: Some(1),
                    output_digest: hash(b"warning and failure"),
                    artifact_digest: None,
                },
            }],
        };
        let binding = Binding {
            material,
            governance_digest: hash(b"governance"),
            proof_plan_digest: hash(b"plan"),
            evidence_digest: digest(&suite).unwrap(),
            configuration_digest: hash(b"config"),
            trajectory_run_id: "run-1".into(),
        };
        (binding, suite)
    }
    #[test]
    fn summary_preserves_actual_failure_exclusion_and_blocker() {
        let (binding, suite) = fixture();
        let dispositions = vec![super::super::readiness_profile::StageDisposition {
            stage: "macos".into(),
            key: None,
            disposition: super::super::readiness_profile::StageDispositionKind::NonApplicable,
            policy: "predicate".into(),
            reason: "not selected".into(),
            evidence_scope: "no execution claim".into(),
        }];
        let blockers = vec![super::super::validation_runtime::Blocker::new(
            "FAILED",
            "test failed",
        )];
        let value = validation_projection(
            &binding,
            &ValidationSummary {
                scope: "test",
                evidence: &suite,
                dispositions: &dispositions,
                blockers: &blockers,
            },
        )
        .unwrap();
        assert_eq!(value["observations"][0]["exit_code"], 1);
        assert_eq!(value["observations"][0]["conclusion"], "failure");
        assert_eq!(value["check_count"], suite.results.len());
        assert_eq!(value["check_groups"][0]["checks"][0]["job"], "tests");
        assert_eq!(value["stage_dispositions"][0]["reason"], "not selected");
        assert_eq!(value["validation_blockers"][0]["detail"], "test failed");
        assert_eq!(value["changed_paths"][0], "src/lib.rs");
        assert!(value.get("implementation_diff").is_none());
    }

    #[test]
    fn repeated_success_is_interned_without_losing_check_or_disposition_identity() {
        let (mut binding, mut suite) = fixture();
        let original = suite.results[0].clone();
        suite.results = (0..96)
            .map(|index| {
                let mut row = original.clone();
                row.key.job = format!("required-check-{index}");
                row.observation = evidence::Observation::Executed {
                    execution_id: format!("run-{index}"),
                    conclusion: evidence::Conclusion::Success,
                    exit_code: Some(0),
                    output_digest: hash(format!("output-{index}").as_bytes()),
                    artifact_digest: None,
                };
                row
            })
            .collect();
        let dispositions = suite
            .results
            .iter()
            .map(|row| super::super::readiness_profile::StageDisposition {
                stage: "required".into(),
                key: Some(row.key.clone()),
                disposition: super::super::readiness_profile::StageDispositionKind::Real,
                policy: "required-policy".into(),
                reason: "must actually execute".into(),
                evidence_scope: "verified execution".into(),
            })
            .collect::<Vec<_>>();
        binding.evidence_digest = digest(&suite).unwrap();
        let value = validation_projection(
            &binding,
            &ValidationSummary {
                scope: "test",
                evidence: &suite,
                dispositions: &dispositions,
                blockers: &[],
            },
        )
        .unwrap();
        assert_eq!(value["check_count"], 96);
        assert_eq!(value["observations"].as_array().unwrap().len(), 1);
        assert_eq!(value["check_groups"].as_array().unwrap().len(), 1);
        let members = value["check_groups"][0]["checks"].as_array().unwrap();
        let references = value["stage_dispositions"][0]["checks"].as_array().unwrap();
        assert_eq!(members.len(), suite.results.len());
        assert_eq!(references.len(), suite.results.len());
        for (index, row) in suite.results.iter().enumerate() {
            assert_eq!(members[index]["index"], index);
            assert_eq!(members[index]["job"], row.key.job);
            assert_eq!(references[index]["check_index"], index);
        }
        assert!(
            serde_json::to_vec(&value).unwrap().len()
                < serde_json::to_vec(&suite).unwrap().len() / 4
        );
        assert!(bounded_projection(&value).is_ok());
    }

    #[test]
    fn compact_observations_preserve_every_non_success_and_missing_check() {
        let (mut binding, mut suite) = fixture();
        let original = suite.results[0].clone();
        let conclusions = [
            evidence::Conclusion::Pending,
            evidence::Conclusion::Running,
            evidence::Conclusion::Failure,
            evidence::Conclusion::Skipped,
            evidence::Conclusion::Cancelled,
            evidence::Conclusion::TimedOut,
        ];
        suite.results = conclusions
            .iter()
            .enumerate()
            .map(|(index, conclusion)| {
                let mut row = original.clone();
                row.key.job = format!("adverse-{index}");
                row.observation = evidence::Observation::Executed {
                    execution_id: format!("run-{index}"),
                    conclusion: *conclusion,
                    exit_code: None,
                    output_digest: hash(b"adverse"),
                    artifact_digest: None,
                };
                row
            })
            .collect();
        let mut absent_key = original.key;
        absent_key.job = "missing-native-check".into();
        let dispositions = [super::super::readiness_profile::StageDisposition {
            stage: "native".into(),
            key: Some(absent_key),
            disposition: super::super::readiness_profile::StageDispositionKind::NonApplicable,
            policy: "unproven-predicate".into(),
            reason: "no Apple execution evidence".into(),
            evidence_scope: "unknown".into(),
        }];
        binding.evidence_digest = digest(&suite).unwrap();
        let value = validation_projection(
            &binding,
            &ValidationSummary {
                scope: "test",
                evidence: &suite,
                dispositions: &dispositions,
                blockers: &[],
            },
        )
        .unwrap();
        for (index, conclusion) in conclusions.iter().enumerate() {
            assert_eq!(
                value["observations"][index]["conclusion"],
                serde_json::to_value(conclusion).unwrap()
            );
            assert!(value["observations"][index]["exit_code"].is_null());
        }
        assert_eq!(
            value["stage_dispositions"][0]["reason"],
            "no Apple execution evidence"
        );
        assert_eq!(value["stage_dispositions"][0]["evidence_scope"], "unknown");
        assert_eq!(
            value["stage_dispositions"][0]["checks"][0]["unobserved_check"]["job"],
            "missing-native-check"
        );
    }
    #[test]
    fn stale_candidate_or_modified_results_refuse_projection() {
        let (mut binding, mut suite) = fixture();
        binding.material.head_revision = "c".repeat(40);
        let summary = ValidationSummary {
            scope: "test",
            evidence: &suite,
            dispositions: &[],
            blockers: &[],
        };
        assert!(validation_projection(&binding, &summary).is_err());
        binding.material = suite.after.clone();
        binding.proof_plan_digest = hash(b"different plan");
        let summary = ValidationSummary {
            scope: "test",
            evidence: &suite,
            dispositions: &[],
            blockers: &[],
        };
        assert!(validation_projection(&binding, &summary).is_err());
        binding.proof_plan_digest = suite.expected_plan_digest.clone();
        suite.results.clear();
        let summary = ValidationSummary {
            scope: "test",
            evidence: &suite,
            dispositions: &[],
            blockers: &[],
        };
        assert!(validation_projection(&binding, &summary).is_err());
    }
    #[test]
    fn projection_bound_refuses_instead_of_truncating_adverse_facts() {
        let value = serde_json::json!({"warning":"x".repeat(MAX_INPUT)});
        assert!(bounded_projection(&value).is_err());
        let small = serde_json::json!({"warning":"retained"});
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&bounded_projection(&small).unwrap())
                .unwrap(),
            small
        );
    }

    #[test]
    fn publication_requires_proof_even_when_vega_accepts_and_stale_abstains() {
        let (binding, suite) = fixture();
        let summary = ValidationSummary {
            scope: "test",
            evidence: &suite,
            dispositions: &[],
            blockers: &[],
        };
        let mut receipt = assess(
            Path::new("/unused"),
            Path::new("/unused"),
            AssessmentConfig {
                policy: &Configuration::default(),
                captured_installation: &Ok(None),
            },
            &summary,
            binding,
            Duration::from_secs(1),
            &Cancellation::default(),
        )
        .unwrap();
        assert!(permits_publication(true, &receipt));
        assert!(!permits_publication(false, &receipt));
        receipt.mode = Mode::Required;
        receipt.decision = Decision::Accept;
        assert!(permits_publication(true, &receipt));
        assert!(!permits_publication(false, &receipt));
        for decision in [Decision::Reject, Decision::Abstain, Decision::Disabled] {
            receipt.decision = decision;
            assert!(!permits_publication(true, &receipt));
        }
        receipt.decision = Decision::Accept;
        receipt.stale();
        assert!(!permits_publication(true, &receipt));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_free_backend_checks_reject_answers_and_ambiguous_failures() {
        let check = || BackendCheck {
            schema: BACKEND_CHECK_SCHEMA.into(),
            backend: Backend::Cuda,
            available: false,
            error_kind: Some(WorkerErrorKind::HardwareUnavailable),
            error: Some("GPU unavailable".into()),
        };
        let bytes = |value: &BackendCheck| serde_json::to_vec(value).unwrap();
        assert!(matches!(
            backend_check_response(&bytes(&check()), Backend::Cuda).unwrap(),
            GpuStartup::Unavailable {
                separate_cpu_required: false,
                ..
            }
        ));
        let mut ready = check();
        ready.available = true;
        ready.error_kind = None;
        ready.error = None;
        assert!(matches!(
            backend_check_response(&bytes(&ready), Backend::Cuda).unwrap(),
            GpuStartup::Available
        ));
        for invalid in [
            BackendCheck {
                error_kind: Some(WorkerErrorKind::InferenceFailed),
                ..check()
            },
            BackendCheck {
                available: true,
                ..check()
            },
            BackendCheck {
                error_kind: None,
                ..check()
            },
            BackendCheck {
                error: None,
                ..check()
            },
            BackendCheck {
                error: Some(String::new()),
                ..check()
            },
            BackendCheck {
                error: Some("x".repeat(513)),
                ..check()
            },
            BackendCheck {
                backend: Backend::Cpu,
                ..check()
            },
            BackendCheck {
                schema: WORKER_SCHEMA.into(),
                ..check()
            },
        ] {
            assert!(backend_check_response(&bytes(&invalid), Backend::Cuda).is_err());
        }
        assert!(backend_check_response(&bytes(&check()), Backend::Metal).is_err());
        let mut answer = serde_json::to_value(check()).unwrap();
        answer["outcome"] = serde_json::to_value(outcome()).unwrap();
        assert!(
            backend_check_response(&serde_json::to_vec(&answer).unwrap(), Backend::Cuda).is_err()
        );
        for malformed in [b"".as_slice(), b"null", b"{", b"{}"] {
            assert!(backend_check_response(malformed, Backend::Cuda).is_err());
        }
    }

    #[test]
    fn frozen_cpu_runtime_changes_fail_before_model_access() {
        let mut config = Configuration {
            mode: Mode::Required,
            installation: Some(Installation {
                directory: "/unavailable-model".into(),
                manifest_digest: hash(b"model"),
                runtime: "/gpu-worker".into(),
                runtime_sha256: hash(b"gpu"),
                cpu_runtime: Some(RuntimePin {
                    runtime: "/cpu-worker".into(),
                    runtime_sha256: hash(b"cpu"),
                }),
            }),
            ..Default::default()
        };
        let captured = capture_installation(&config);
        config
            .installation
            .as_mut()
            .unwrap()
            .cpu_runtime
            .as_mut()
            .unwrap()
            .runtime_sha256 = hash(b"changed");
        let error = verify_installation_snapshot(
            &config,
            &captured,
            Instant::now() + Duration::from_secs(1),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("local_binding_changed"));
    }

    #[test]
    fn portable_defaults_preserve_legacy_serialization_and_explicit_preferences() {
        let defaults: Configuration = toml::from_str("mode='required'").unwrap();
        assert_eq!(defaults.backend, BackendPreference::Auto);
        assert!(defaults.cpu_fallback);
        assert_eq!(
            toml::to_string(&defaults).unwrap().trim(),
            "mode = \"required\"\nalpha = \"\"\nrisk = \"\""
        );
        let explicit: Configuration =
            toml::from_str("mode='required'\nbackend='cpu'\ncpu_fallback=false").unwrap();
        assert_eq!(explicit.backend.resolve(), Backend::Cpu);
        let roundtrip: Configuration =
            toml::from_str(&toml::to_string(&explicit).unwrap()).unwrap();
        assert_eq!(explicit, roundtrip);
        for invalid in ["backend='gpu'", "cpu_fallback='true'", "backend=3"] {
            assert!(toml::from_str::<Configuration>(invalid).is_err());
        }
        assert_eq!(
            BackendPreference::Auto.resolve(),
            if cfg!(target_os = "macos") {
                Backend::Metal
            } else {
                Backend::Cuda
            }
        );
    }
    #[test]
    fn cpu_retry_requires_only_a_typed_hardware_failure_without_an_answer() {
        let response = || Response {
            schema: WORKER_SCHEMA.into(),
            request_id: "request".into(),
            input_digest: hash(b"input"),
            manifest_digest: hash(b"model"),
            backend: Backend::Metal.runtime().into(),
            device: "metal".into(),
            engine_device: "cpu".into(),
            precision: Precision::supported(),
            input_tokens: 0,
            outcome: None,
            error: Some("device unavailable".into()),
            error_kind: Some(WorkerErrorKind::HardwareUnavailable),
        };
        assert!(response().permits_cpu_retry(Backend::Metal, true));
        assert!(response().permits_cpu_retry(Backend::Cuda, true));
        assert!(!response().permits_cpu_retry(Backend::Cpu, true));
        assert!(!response().permits_cpu_retry(Backend::Metal, false));
        let mut current = response();
        current.error_kind = Some(WorkerErrorKind::InferenceFailed);
        assert!(!current.permits_cpu_retry(Backend::Metal, true));
        current = response();
        current.error_kind = None;
        assert!(!current.permits_cpu_retry(Backend::Metal, true));
        for answer in [
            outcome(),
            {
                let mut answer = outcome();
                answer.abstain = true;
                answer
            },
            {
                let mut answer = outcome();
                answer.probabilities = [0.9, 0.1];
                answer.confidence = 0.9;
                answer.prediction_set = Some(vec![false]);
                answer
            },
        ] {
            current = response();
            current.outcome = Some(answer);
            assert!(!current.permits_cpu_retry(Backend::Metal, true));
        }
        current = response();
        current.input_tokens = 1;
        assert!(!current.permits_cpu_retry(Backend::Metal, true));
        for error in [None, Some(String::new()), Some("x".repeat(513))] {
            current = response();
            current.error = error;
            assert!(!current.permits_cpu_retry(Backend::Metal, true));
        }
    }
    fn outcome() -> Outcome {
        Outcome {
            probabilities: [0.1, 0.9],
            prediction_set: Some(vec![true]),
            abstain: false,
            unbound: false,
            settle_steps: 12,
            energy: 0.0,
            kinetic: 0.1,
            nearest: 0.1,
            confidence: 0.9,
        }
    }
    #[test]
    fn only_positive_unambiguous_nonabstaining_result_accepts() {
        let mut o = outcome();
        assert_eq!(o.decision().unwrap(), Decision::Accept);
        o.prediction_set = Some(vec![false, true]);
        assert_eq!(o.decision().unwrap(), Decision::Abstain);
        o.prediction_set = None;
        assert_eq!(o.decision().unwrap(), Decision::Abstain);
        o = outcome();
        o.abstain = true;
        assert_eq!(o.decision().unwrap(), Decision::Abstain);
        o = outcome();
        o.unbound = true;
        assert_eq!(o.decision().unwrap(), Decision::Abstain);
    }
    #[test]
    fn shared_response_checks_reject_wrong_custody_lengths_errors_and_missing_answers() {
        let request = Request {
            schema: WORKER_SCHEMA.into(),
            request_id: "request".into(),
            input_digest: hash(b"state"),
            installation: "/unused".into(),
            manifest_digest: hash(b"model"),
            state: "state".into(),
            alpha: "0.1".into(),
            risk: "0.1".into(),
            backend: Backend::Cpu,
        };
        let response = || Response {
            schema: WORKER_SCHEMA.into(),
            request_id: request.request_id.clone(),
            input_digest: request.input_digest.clone(),
            manifest_digest: request.manifest_digest.clone(),
            backend: "candle-cpu".into(),
            device: "cpu".into(),
            engine_device: "cpu".into(),
            precision: Precision::supported(),
            input_tokens: 120,
            outcome: Some(outcome()),
            error: None,
            error_kind: None,
        };
        assert_eq!(
            response()
                .validated_outcome(&request)
                .unwrap()
                .decision()
                .unwrap(),
            Decision::Accept
        );
        for field in [
            "schema",
            "request",
            "input",
            "manifest",
            "backend",
            "device",
            "engine",
            "precision",
            "zero-tokens",
            "overflow",
            "error",
            "error-kind",
            "missing",
            "malformed",
        ] {
            let mut current = response();
            match field {
                "schema" => current.schema = "old".into(),
                "request" => current.request_id = "other".into(),
                "input" => current.input_digest = hash(b"other"),
                "manifest" => current.manifest_digest = hash(b"other"),
                "backend" => current.backend = "candle-metal".into(),
                "device" => current.device = "metal".into(),
                "engine" => current.engine_device = "metal".into(),
                "precision" => current.precision.backbone = "f16".into(),
                "zero-tokens" => current.input_tokens = 0,
                "overflow" => current.input_tokens = super::super::vega_prompt::MAX_TOKENS + 1,
                "error" => current.error = Some("unavailable".into()),
                "error-kind" => current.error_kind = Some(WorkerErrorKind::InferenceFailed),
                "missing" => current.outcome = None,
                "malformed" => current.outcome.as_mut().unwrap().probabilities = [0.9, 0.9],
                _ => unreachable!(),
            }
            assert!(current.validated_outcome(&request).is_err(), "{field}");
        }
        let mut abstention = response();
        abstention.outcome.as_mut().unwrap().abstain = true;
        assert_eq!(
            abstention
                .validated_outcome(&request)
                .unwrap()
                .decision()
                .unwrap(),
            Decision::Abstain
        );
    }
    #[test]
    fn negative_singleton_rejects_not_approves() {
        let mut o = outcome();
        o.probabilities = [0.9, 0.1];
        o.prediction_set = Some(vec![false]);
        assert_eq!(o.decision().unwrap(), Decision::Reject);
    }
    #[test]
    fn malformed_output_never_accepts() {
        let mut o = outcome();
        o.probabilities[0] = f64::NAN;
        assert!(o.decision().is_err());
        o = outcome();
        o.prediction_set = Some(vec![true, true]);
        assert!(o.decision().is_err());
        o = outcome();
        o.prediction_set = Some(vec![false]);
        assert!(o.decision().is_err());
        o = outcome();
        o.confidence = 1.0;
        assert!(o.decision().is_err());
    }

    #[test]
    fn accepted_required_policy_cannot_be_disabled_or_repinned_by_candidate() {
        let mut required = configuration_from_bytes(b"[vega]\nmode = 'required'\n").unwrap();
        required.installation = Some(Installation {
            directory: PathBuf::from("/verified/model"),
            manifest_digest: hash(b"manifest"),
            runtime: PathBuf::from("/verified/worker"),
            runtime_sha256: hash(b"runtime"),
            cpu_runtime: None,
        });
        assert!(verify_policy_acceptance(&required, &required).is_ok());
        let disabled = configuration_from_bytes(b"").unwrap();
        assert!(verify_policy_acceptance(&disabled, &required).is_err());
        for field in [
            "manifest",
            "runtime",
            "directory",
            "runtime_path",
            "alpha",
            "risk",
            "model_manifest",
            "backend",
            "cpu_fallback",
        ] {
            let mut changed = required.clone();
            let installation = changed.installation.as_mut().unwrap();
            match field {
                "manifest" => installation.manifest_digest = hash(b"replacement"),
                "runtime" => installation.runtime_sha256 = hash(b"replacement"),
                "directory" => installation.directory = PathBuf::from("/replacement/model"),
                "runtime_path" => installation.runtime = PathBuf::from("/replacement/worker"),
                "alpha" => changed.alpha = "0.05".into(),
                "risk" => changed.risk = "0.2".into(),
                "model_manifest" => changed.model_manifest_digest = hash(b"new model"),
                "backend" => changed.backend = BackendPreference::Cpu,
                "cpu_fallback" => changed.cpu_fallback = false,
                _ => unreachable!(),
            }
            assert!(
                verify_policy_acceptance(&changed, &required).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn disabled_accepted_policy_preserves_optional_compatibility() {
        let implicit = configuration_from_bytes(b"").unwrap();
        let explicit = configuration_from_bytes(b"[vega]\nmode = 'disabled'\n").unwrap();
        assert_eq!(implicit, explicit);
        assert!(verify_policy_acceptance(&implicit, &explicit).is_ok());
        let required = configuration_from_bytes(b"[vega]\nmode = 'required'\n").unwrap();
        assert!(verify_policy_acceptance(&required, &implicit).is_ok());
    }

    #[test]
    fn local_binding_snapshot_rejects_changes_before_model_access() {
        let mut config = Configuration {
            mode: Mode::Required,
            installation: Some(Installation {
                directory: PathBuf::from("/unavailable-model"),
                manifest_digest: hash(b"model"),
                runtime: PathBuf::from("/unavailable-worker"),
                runtime_sha256: hash(b"worker"),
                cpu_runtime: None,
            }),
            ..Default::default()
        };
        let captured = capture_installation(&config);
        config.installation.as_mut().unwrap().runtime = PathBuf::from("/different-worker");
        let error = verify_installation_snapshot(
            &config,
            &captured,
            Instant::now() + Duration::from_secs(1),
            &Cancellation::default(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("binding_changed"));
        config.mode = Mode::Disabled;
        assert!(capture_installation(&config).unwrap().is_none());
        config.mode = Mode::Required;
        config.model_manifest_digest = hash(b"different model");
        assert!(capture_installation(&config).is_err());
    }
}

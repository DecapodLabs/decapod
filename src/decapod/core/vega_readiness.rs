//! vega is an untrusted post-proof readiness assessor, never a proof authority.
use super::readiness_evidence::MaterialIdentity;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const SCHEMA: &str = "decapod.vega-readiness.v1";
pub const WORKER_SCHEMA: &str = "decapod.vega-worker.v1";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Disabled,
    Required,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub mode: Mode,
    pub installation: Option<Installation>,
    /// A key actually present in the pinned conformal table, not a guarantee
    /// of code-readiness coverage under distribution shift.
    pub alpha: String,
    pub risk: String,
}
impl Configuration {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
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
pub(crate) struct ValidationSummary<'a> {
    pub scope: &'a str,
    pub evidence: &'a evidence::SuiteEvidence,
    pub dispositions: &'a [super::readiness_profile::StageDisposition],
    pub blockers: &'a [super::validation_runtime::Blocker],
}
fn bounded_projection(value: &serde_json::Value) -> io::Result<String> {
    let state = serde_json::to_string(value).map_err(io::Error::other)?;
    if state.len() > MAX_INPUT {
        return Err(invalid("projection_overflow_no_truncation"));
    }
    Ok(state)
}
fn projection(
    root: &Path,
    binding: &Binding,
    validation: &ValidationSummary<'_>,
) -> io::Result<String> {
    let results = validation_projection(binding, validation)?;
    let plan = crate::plan_governance::load_plan(root)
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing_intent"))?;
    if plan.intent.trim().is_empty() {
        return Err(invalid("missing_intent"));
    }
    let trajectory = super::trajectory::load_trajectory_cookie(root)
        .map_err(io::Error::other)?
        .ok_or_else(|| invalid("missing_trajectory"))?;
    if trajectory.run_id != binding.trajectory_run_id {
        return Err(invalid("stale_trajectory"));
    }
    bounded_projection(&serde_json::json!({
        "projection":"validation-results.v2",
        "publication_actor":"agent through existing validation gates",
        "intent":plan.intent,"validation":results,
        "unresolved_assumptions":trajectory.unresolved_assumptions,
        "shortcut_risk_signals":trajectory.shortcut_risk_signals,
        "trajectory_blockers":trajectory.blockers,
        "coverage":"all check outcomes, stage dispositions and adverse facts; repeated facts are indexed once. Exact custody hashes bind the complete evidence outside this prompt. No raw logs, full plan or source diff; unrepresented warnings and untested intent remain unknown"
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
    if !path.is_absolute()
        || path.canonicalize()? != path
        || !expected.starts_with("sha256:")
        || expected.len() != 71
    {
        return Err(invalid("invalid_runtime_pin"));
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.len() > 512 * 1024 * 1024 {
        return Err(invalid("runtime_not_bounded_regular_file"));
    }
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0; 65536];
    let mut count = 0u64;
    let mut header = [0u8; 8];
    file.read_exact(&mut header)?;
    if header != [0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01] {
        return Err(invalid("runtime_not_native_arm64_macho"));
    }
    use std::io::Seek;
    file.rewind()?;
    loop {
        if Instant::now() >= deadline || cancel.is_cancelled() {
            return Err(invalid("runtime_verification_cancelled_or_timeout"));
        }
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > 512 * 1024 * 1024 {
            return Err(invalid("runtime_size_changed"));
        }
        hasher.update(&buf[..n]);
    }
    if format!("sha256:{:x}", hasher.finalize()) != expected {
        return Err(invalid("runtime_digest_mismatch"));
    }
    Ok(())
}

/// Sole caller: validation_runtime after `readiness_profile::finish` succeeds.
/// Caller must independently re-run exact-state proof freshness after this returns.
pub(crate) fn assess(
    root: &Path,
    directory: &Path,
    config: &Configuration,
    validation: &ValidationSummary<'_>,
    mut binding: Binding,
    timeout: Duration,
    cancel: &Cancellation,
) -> io::Result<Receipt> {
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
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(invalid("native_apple_metal_required"));
        }
        let install = config
            .installation
            .as_ref()
            .ok_or_else(|| invalid("vega_installation_not_configured"))?;
        let manifest = super::vega_setup::verify_installation_bounded(
            &install.directory,
            &install.manifest_digest,
            deadline,
            cancel,
        )?;
        receipt.manifest = Some(serde_json::to_value(&manifest).map_err(io::Error::other)?);
        pinned_runtime(&install.runtime, &install.runtime_sha256, deadline, cancel)?;
        receipt.runtime_sha256 = Some(install.runtime_sha256.clone());
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
        ))?;
        let request = Request {
            schema: WORKER_SCHEMA.into(),
            request_id: receipt.request_id.clone(),
            input_digest: receipt.input_digest.clone(),
            installation: install.directory.clone(),
            manifest_digest: install.manifest_digest.clone(),
            state,
            alpha: config.alpha.clone(),
            risk: config.risk.clone(),
        };
        let path = directory.join("vega-input.json");
        private_write(
            &path,
            &serde_json::to_vec(&request).map_err(io::Error::other)?,
        )?;
        let capture = Command::new(&install.runtime)
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
        let response: Response =
            serde_json::from_slice(&capture.output.stdout).map_err(io::Error::other)?;
        if response.schema != WORKER_SCHEMA
            || response.request_id != receipt.request_id
            || response.input_digest != receipt.input_digest
            || response.manifest_digest != install.manifest_digest
            || response.backend != "candle-metal"
            || response.device != "metal"
            || response.engine_device != "cpu"
            || response.precision != Precision::supported()
        {
            return Err(invalid("vega_response_binding_or_backend_invalid"));
        }
        if let Some(error) = response.error {
            return Err(invalid(format!(
                "vega_native_abstained:{}",
                error.chars().take(512).collect::<String>()
            )));
        }
        if response.input_tokens == 0 || response.input_tokens > super::vega_prompt::MAX_TOKENS {
            return Err(invalid("vega_response_token_count_invalid"));
        }
        let outcome = response
            .outcome
            .ok_or_else(|| invalid("vega_outcome_missing"))?;
        let decision = outcome.decision().map_err(invalid)?;
        // Recheck both immutable installation and executable after inference.
        super::vega_setup::verify_installation_bounded(
            &install.directory,
            &install.manifest_digest,
            deadline,
            cancel,
        )?;
        pinned_runtime(&install.runtime, &install.runtime_sha256, deadline, cancel)?;
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
            &Configuration::default(),
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

//! Passive legacy configuration and receipt records, retained for lossless audit decoding.
//! These records never install, execute or authorize a Vega evaluator. Required
//! legacy policy is rejected by Laya's explicit migration boundary.
use super::readiness_assessment::{Binding, Decision, Installation, Mode, permits_decision};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorKind {
    HardwareUnavailable,
    InferenceFailed,
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

fn default_cpu_fallback() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn is_auto(value: &BackendPreference) -> bool {
    *value == BackendPreference::Auto
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Precision {
    pub backbone: String,
    pub engine: String,
    pub calibration: String,
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
        permits_decision(true, self.mode, self.decision)
    }
    pub fn stale(&mut self) {
        self.decision = Decision::Abstain;
        self.reason = "state_or_model_changed_during_inference".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn legacy_configuration_keeps_every_recorded_setting_without_execution() {
        let value = json!({"mode":"required","backend":"cpu","cpu_fallback":false,
            "model_manifest_digest":"sha256:historical","installation":{"directory":"/unavailable/model","manifest_digest":"sha256:historical","runtime":"/unavailable/runtime","runtime_sha256":"sha256:recorded","cpu_runtime":{"runtime":"/unavailable/cpu","runtime_sha256":"sha256:cpu"}},"alpha":"0.05","risk":"0.1"});
        let record: Configuration = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(record).unwrap(), value);
    }
    #[test]
    fn historical_receipt_roundtrip_preserves_bindings_outcomes_and_attribution() {
        let value = json!({"schema":"decapod.vega-readiness.v1","decapod_version":"historical","request_id":"original-request",
            "binding":{"material":{"repository":"fixture/audit","work_branch":"agent/historical/task","head_revision":"a","base_branch":"master","accepted_base_revision":"b","history_digest":"h","tree_digest":"t","configuration_digest":"c","workflow_digest":"w","toolchain_digest":"tool","candidate_digest":"candidate"},"governance_digest":"governance","proof_plan_digest":"proof","evidence_digest":"evidence","configuration_digest":"config","trajectory_run_id":"original-run"},
            "rubric":"historical-rubric","input_digest":"original-input","input_bytes":512,"mode":"required","decision":"accept","reason":"recorded reason",
            "outcome":{"probabilities":[0.1,0.9],"prediction_set":[true],"abstain":false,"unbound":false,"settle_steps":7,"energy":1.0,"kinetic":2.0,"nearest":3.0,"confidence":0.9},
            "alpha":"0.05","risk":"0.1","manifest":{"immutable":"body"},"runtime_sha256":"original-runtime","backend":"cpu","device":"cpu","engine_device":"cpu","precision":{"backbone":"f32","engine":"f32","calibration":"f64"},"input_tokens":20,"backend_attempts":[{"backend":"cpu","phase":"inference","runtime_sha256":"original-runtime","request_id":"original-request","error_kind":null,"error":null}],"calibration_scope":"historical only"});
        let record: Receipt = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(record).unwrap(), value);
    }
}

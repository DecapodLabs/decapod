//! Active Laya post-proof judgment. Deterministic validation owns readiness;
//! the worker supplies one bound model observation, never proof authority.
use super::{
    bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
    laya_assets,
    laya_protocol::{
        self as wire, Backend, BackendPreference, Judgment, RUBRIC, Request, Response,
        WORKER_SCHEMA,
    },
    laya_setup, readiness_evidence as evidence, vega_readiness as shared, vega_runtime,
};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use shared as readiness;
#[cfg(test)]
#[path = "../../../tests/support/laya_fitting_cases.rs"]
mod fitting_cases;
#[cfg(test)]
#[path = "../../../tests/support/readiness_snapshot.rs"]
mod test_fixture;
#[cfg(test)]
#[path = "../../../tests/unit/core/laya_readiness_tests.rs"]
mod tests;
pub use shared::{Binding, Decision, Installation, Mode, ValidationSummary};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub mode: Mode,
    pub backend: BackendPreference,
    #[serde(default = "cpu_fallback")]
    pub cpu_fallback: bool,
    pub model_manifest_digest: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installation: Option<Installation>,
}
fn cpu_fallback() -> bool {
    true
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            mode: Mode::Disabled,
            backend: BackendPreference::Auto,
            cpu_fallback: true,
            model_manifest_digest: String::new(),
            installation: None,
        }
    }
}
impl Configuration {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

fn invalid(reason: impl Into<String>) -> io::Error {
    io::Error::other(reason.into())
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
        return Err(invalid("laya_configuration_overflow"));
    }
    let value: toml::Value = toml::from_str(std::str::from_utf8(bytes).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    let config: Configuration = value
        .get("laya")
        .cloned()
        .map(|v| v.try_into().map_err(io::Error::other))
        .transpose()?
        .unwrap_or_default();
    if config.mode == Mode::Required
        && config.model_manifest_digest
            != laya_assets::manifest_digest(&laya_assets::baseline()).map_err(io::Error::other)?
    {
        return Err(invalid(
            "laya_reviewed_manifest_required: run decapod setup laya and commit its shared manifest digest; each maintainer registers a private runtime",
        ));
    }
    if value
        .get("vega")
        .and_then(|v| v.get("mode"))
        .and_then(|v| v.as_str())
        == Some("required")
    {
        return Err(invalid(
            "LAYA_EXPLICIT_MIGRATION_REQUIRED: legacy required Vega policy is not silently disabled or interpreted as Laya; review and accept the explicit team policy migration",
        ));
    }
    Ok(config)
}
pub(crate) fn verify_policy_acceptance(
    candidate: &Configuration,
    accepted: &Configuration,
) -> io::Result<()> {
    if accepted.mode == Mode::Required && candidate != accepted {
        return Err(invalid(
            "LAYA_ACCEPTED_POLICY_CHANGED: required accepted policy cannot be disabled or repinned by the candidate",
        ));
    }
    Ok(())
}
pub(crate) fn capture_installation(config: &Configuration) -> io::Result<Option<Installation>> {
    if config.mode == Mode::Disabled {
        return Ok(None);
    }
    let installation = match &config.installation {
        Some(installation) => installation.clone(),
        None => {
            vega_runtime::resolve_at(&laya_setup::default_cache()?, &config.model_manifest_digest)?
        }
    };
    if installation.manifest_digest != config.model_manifest_digest {
        return Err(invalid("laya_shared_local_manifest_mismatch"));
    }
    Ok(Some(installation))
}
pub(crate) fn verify_installation_snapshot(
    config: &Configuration,
    captured: &io::Result<Option<Installation>>,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<()> {
    let install = captured
        .as_ref()
        .map_err(|e| invalid(e.to_string()))?
        .as_ref()
        .ok_or_else(|| invalid("laya_installation_not_captured"))?;
    if capture_installation(config)?.as_ref() != Some(install) {
        return Err(invalid("laya_local_binding_changed_during_validation"));
    }
    laya_setup::verify_installation(
        &install.directory,
        &install.manifest_digest,
        deadline,
        cancel,
    )?;
    shared::verify_runtime_pins(install, deadline, cancel)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: String,
    pub decapod_version: String,
    pub binding: Binding,
    pub request_id: String,
    pub rubric: String,
    pub input_digest: String,
    pub input_bytes: usize,
    pub mode: Mode,
    pub decision: Decision,
    pub reason: String,
    pub judgment: Option<Judgment>,
    pub manifest: Option<wire::Manifest>,
    pub attempts: Vec<Response>,
    pub runtime_sha256: Option<String>,
    pub capability: Option<wire::BackendCheck>,
    pub cpu_fallback_reason: Option<String>,
    pub accuracy_scope: String,
}
impl Receipt {
    pub fn permits_readiness(&self) -> bool {
        shared::permits_decision(true, self.mode, self.decision)
    }
    pub fn stale(&mut self) {
        self.decision = Decision::Abstain;
        self.reason = "state_or_model_changed_during_inference".into();
    }
}
pub(crate) fn permits_publication(proof_ready: bool, receipt: &Receipt) -> bool {
    shared::permits_decision(proof_ready, receipt.mode, receipt.decision)
}

pub(crate) struct AssessmentConfig<'a> {
    pub policy: &'a Configuration,
    pub captured_installation: &'a io::Result<Option<Installation>>,
}
fn private_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = super::fs_permissions::open_private_file(
        path,
        OpenOptions::new().write(true).create_new(true),
    )?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn gpu_startup(
    install: &Installation,
    backend: Backend,
    directory: &Path,
    deadline: Instant,
    cancel: &Cancellation,
) -> io::Result<Option<wire::BackendCheck>> {
    let name = match backend {
        Backend::Cpu => return Err(invalid("laya_gpu_probe_requires_gpu")),
        Backend::Metal => "metal",
        Backend::Cuda => "cuda",
    };
    let capture = Command::new(&install.runtime)
        .env_clear()
        .args(["--backend-check", name])
        .bounded_capture(
            ProcessLimits {
                timeout: deadline
                    .checked_duration_since(Instant::now())
                    .ok_or_else(|| invalid("laya_timeout"))?
                    .min(Duration::from_secs(10)),
                max_output_bytes: 16384,
            },
            cancel,
        );
    let capture = match capture {
        // This is model-free. A previously verified native ELF can fail to
        // start when its interpreter is missing; only a distinct pinned CPU
        // companion may run after this loader condition.
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        other => other?,
    };
    private_write(
        &directory.join("laya-capability.stdout"),
        &capture.output.stdout,
    )?;
    private_write(
        &directory.join("laya-capability.stderr"),
        &capture.output.stderr,
    )?;
    if capture.termination != Termination::Exited {
        return Err(invalid("laya_backend_check_cancelled_or_timeout"));
    }
    if !capture.output.status.success() {
        if capture.output.status.code() == Some(127) && capture.output.stdout.is_empty() {
            return Ok(None);
        }
        return Err(invalid("laya_backend_check_process_failed"));
    }
    let check: wire::BackendCheck =
        serde_json::from_slice(&capture.output.stdout).map_err(io::Error::other)?;
    check.available_for(backend).map_err(invalid)?;
    Ok(Some(check))
}

/// Sole live caller is validation_runtime, after complete current proof passes.
/// Both state/proof and local installation are independently rechecked after it.
pub(crate) fn assess(
    root: &Path,
    directory: &Path,
    selected: AssessmentConfig<'_>,
    validation: &ValidationSummary<'_>,
    mut binding: Binding,
    timeout: Duration,
    cancel: &Cancellation,
) -> io::Result<Receipt> {
    let config = selected.policy;
    binding.configuration_digest = digest(config)?;
    let mut receipt = Receipt {schema:"decapod.laya-readiness.v1".into(),decapod_version:env!("CARGO_PKG_VERSION").into(),request_id:digest(&binding)?,binding,rubric:RUBRIC.into(),input_digest:laya_assets::hash(b""),input_bytes:0,mode:config.mode,decision:Decision::Abstain,reason:"not_evaluated".into(),judgment:None,manifest:None,attempts:Vec::new(),runtime_sha256:None,capability:None,cpu_fallback_reason:None,accuracy_scope:"Upstream general-decision profile; readiness-domain reliability is unestablished. Functional examples and published numerical parity are not population accuracy evidence.".into()};
    if config.mode == Mode::Disabled {
        receipt.decision = Decision::Disabled;
        receipt.reason = "disabled".into();
        return Ok(receipt);
    }
    let deadline = Instant::now() + timeout.min(Duration::from_secs(120));
    let evaluated = (|| -> io::Result<()> {
        verify_installation_snapshot(config, selected.captured_installation, deadline, cancel)?;
        let install = selected
            .captured_installation
            .as_ref()
            .map_err(|e| invalid(e.to_string()))?
            .as_ref()
            .ok_or_else(|| invalid("laya_installation_not_captured"))?;
        receipt.manifest = Some(laya_assets::baseline());
        let mut backend = config.backend.resolve();
        if backend != Backend::Cpu {
            let check = gpu_startup(install, backend, directory, deadline, cancel)?;
            let unavailable = match &check {
                Some(check) => !check.available_for(backend).map_err(invalid)?,
                None => true,
            };
            if unavailable {
                if !config.cpu_fallback {
                    return Err(invalid("laya_gpu_unavailable_cpu_fallback_disabled"));
                }
                if check.is_none() && install.cpu_runtime.is_none() {
                    return Err(invalid(
                        "laya_cpu_companion_required: register setup laya --runtime <gpu-worker> --cpu-runtime <cpu-only-worker> for GPU loader unavailability",
                    ));
                }
                receipt.cpu_fallback_reason = Some(
                    check
                        .as_ref()
                        .and_then(|c| c.error.clone())
                        .unwrap_or_else(|| {
                            "gpu_loader_unavailable_using_separately_pinned_cpu_companion".into()
                        }),
                );
                verify_installation_snapshot(
                    config,
                    selected.captured_installation,
                    deadline,
                    cancel,
                )?;
                backend = Backend::Cpu;
            }
            receipt.capability = check;
        }
        let state = shared::projection(root, &receipt.binding, validation)?;
        receipt.input_digest = laya_assets::hash(state.as_bytes());
        receipt.input_bytes = state.len();
        let mut request = Request {
            schema: WORKER_SCHEMA.into(),
            rubric: RUBRIC.into(),
            request_id: String::new(),
            input_digest: receipt.input_digest.clone(),
            manifest_digest: install.manifest_digest.clone(),
            installation: install.directory.clone(),
            backend,
            state,
        };
        for attempt in 0..2 {
            let pin = if request.backend == Backend::Cpu {
                install
                    .cpu_runtime
                    .as_ref()
                    .map(|p| (&p.runtime, &p.runtime_sha256))
                    .unwrap_or((&install.runtime, &install.runtime_sha256))
            } else {
                (&install.runtime, &install.runtime_sha256)
            };
            receipt.runtime_sha256 = Some(pin.1.clone());
            request.request_id = digest(&(
                &receipt.binding,
                RUBRIC,
                &receipt.input_digest,
                install,
                request.backend,
                pin.1,
                attempt,
            ))?;
            receipt.request_id = request.request_id.clone();
            let path = directory.join(format!("laya-input-{attempt}.json"));
            private_write(
                &path,
                &serde_json::to_vec(&request).map_err(io::Error::other)?,
            )?;
            let capture = Command::new(pin.0)
                .env_clear()
                .arg("--request")
                .arg(path.canonicalize()?)
                .bounded_capture(
                    ProcessLimits {
                        timeout: deadline
                            .checked_duration_since(Instant::now())
                            .ok_or_else(|| invalid("laya_timeout"))?,
                        max_output_bytes: 16384,
                    },
                    cancel,
                )?;
            private_write(
                &directory.join(format!("laya-worker-{attempt}.stderr")),
                &capture.output.stderr,
            )?;
            if capture.termination != Termination::Exited || !capture.output.status.success() {
                return Err(invalid(
                    "laya_worker_failed_or_timeout: inspect the private worker log; no readiness approval",
                ));
            }
            let response: Response =
                serde_json::from_slice(&capture.output.stdout).map_err(io::Error::other)?;
            response.validate_binding(&request).map_err(invalid)?;
            let retry = attempt == 0 && response.permits_cpu_retry(&request, config.cpu_fallback);
            receipt.attempts.push(response.clone());
            if retry {
                receipt.cpu_fallback_reason = response.error.clone();
                verify_installation_snapshot(
                    config,
                    selected.captured_installation,
                    deadline,
                    cancel,
                )?;
                request.backend = Backend::Cpu;
                continue;
            }
            let decision = response
                .decision(&request, laya_assets::baseline().max_tokens)
                .map_err(invalid)?;
            receipt.judgment = response.judgment;
            receipt.decision = match decision {
                wire::Decision::Accept => Decision::Accept,
                wire::Decision::Reject => Decision::Reject,
                _ => Decision::Abstain,
            };
            receipt.reason = match receipt.decision {
                Decision::Accept => "model_affirmative_act",
                Decision::Reject => "model_negative_act",
                _ => "model_escalated_or_uncertain",
            }
            .into();
            verify_installation_snapshot(config, selected.captured_installation, deadline, cancel)?;
            return Ok(());
        }
        Err(invalid("laya_worker_response_missing"))
    })();
    if let Err(error) = evaluated {
        receipt.decision = Decision::Abstain;
        receipt.reason = error.to_string();
    }
    Ok(receipt)
}

//! Opt-in rendering of compact host observations. Labels are producer assertions,
//! never authenticated execution proof or publication authority.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};
use std::path::Path;

pub const METRICS: &[&str] = &[
    "wall_ms",
    "setup_ms",
    "validation_ms",
    "active_human_minutes",
    "human_interruptions",
    "approval_decisions",
    "incorrectly_suppressed_escalations",
    "tool_calls",
    "test_actions",
    "retries",
    "model_calls",
    "input_tokens",
    "cached_input_tokens",
    "output_tokens",
    "total_cost_usd",
    "context_bytes",
    "retained_bytes",
    "files_read",
    "harmful_context_rate",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    RealHost,
    IndependentCheck,
    NativeVerifier,
    HumanReview,
    OperatorReport,
    DeterministicFixture,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub class: EvidenceClass,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    NotObserved,
    Unsupported,
    NotReviewed,
    NotApplicable,
    InvalidClock,
    PreInferenceFailure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub value: Option<f64>,
    pub evidence: Option<Evidence>,
    pub missing_reason: Option<MissingReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub value: Option<bool>,
    pub evidence: Option<Evidence>,
    pub missing_reason: Option<MissingReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    Baseline,
    Governed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalStatus {
    Completed,
    Failed,
    Cancelled,
    Partial,
    LaunchFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub run_id: String,
    pub pair_id: String,
    pub task_id: String,
    pub stratum: String,
    pub arm: Arm,
    pub host: String,
    pub host_version: String,
    pub host_sha256: Option<String>,
    pub model: Option<String>,
    pub kernel_sha256: String,
    pub source_commit: String,
    pub accepted_base: String,
    pub task_source_sha256: String,
    pub environment_sha256: Option<String>,
    pub candidate_commit: Option<String>,
    pub candidate_source_sha256: Option<String>,
    pub candidate_dirty: Option<bool>,
    pub policy_sha256: Option<String>,
    pub constitution_sha256: Option<String>,
    pub context_sha256: Option<String>,
    pub identity_missing: BTreeMap<String, MissingReason>,
    pub protocol_sha256: String,
    pub evidence_class: EvidenceClass,
    pub status: TerminalStatus,
    pub functional_correctness: Verdict,
    pub governed_completion: Verdict,
    pub context_delivery: Verdict,
    pub first_missing_boundary: Option<String>,
    pub selected_context_ids: Vec<String>,
    pub metrics: BTreeMap<String, Metric>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub schema_version: String,
    pub runs: Vec<Run>,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub arm: Arm,
    pub runs: usize,
    pub completed: usize,
    pub unsuccessful_or_partial: usize,
    pub reported_independent_correct: usize,
    pub reported_verified_useful_work: usize,
    pub correctness_unobserved: usize,
    pub reviewed_active_human_minutes: Option<f64>,
    pub reported_verified_useful_work_per_reviewed_human_minute: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: &'static str,
    pub publication_ready: bool,
    pub merge_authorized: bool,
    pub evidence_limit: &'static str,
    pub incomplete_pairs: Vec<String>,
    pub unverified_pair_identities: Vec<String>,
    pub summaries: Vec<Summary>,
    pub runs: Vec<Run>,
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid compact outcome record; inspect schema, bounds, identities and evidence classes",
    )
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('/')
        && !value.contains("..")
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:/-".contains(&c))
        && crate::core::trace::redact_string(value) == value
        && crate::core::db_maintenance::redact_secrets(value) == value
}
fn evidence(value: &Evidence) -> bool {
    digest(&value.sha256)
}
fn verdict(value: &Verdict) -> bool {
    match value.value {
        Some(_) => value.missing_reason.is_none() && value.evidence.as_ref().is_some_and(evidence),
        None => value.missing_reason.is_some() && value.evidence.is_none(),
    }
}

pub fn compare(input: Input) -> io::Result<Report> {
    if input.schema_version != "decapod.outcome-observations.v1"
        || input.runs.is_empty()
        || input.runs.len() > 128
    {
        return Err(invalid());
    }
    let mut ids = BTreeSet::new();
    let mut pairs: BTreeMap<&str, (&Run, BTreeSet<Arm>)> = BTreeMap::new();
    for run in &input.runs {
        if [
            &run.run_id,
            &run.pair_id,
            &run.task_id,
            &run.stratum,
            &run.host,
            &run.host_version,
        ]
        .into_iter()
        .any(|value| !identifier(value))
            || !ids.insert(&run.run_id)
            || run.model.as_ref().is_some_and(|value| !identifier(value))
            || [
                &run.kernel_sha256,
                &run.task_source_sha256,
                &run.protocol_sha256,
            ]
            .into_iter()
            .any(|value| !digest(value))
            || [&run.source_commit, &run.accepted_base]
                .into_iter()
                .any(|value| {
                    !matches!(value.len(), 40 | 64) || !value.bytes().all(|c| c.is_ascii_hexdigit())
                })
            || run
                .first_missing_boundary
                .as_ref()
                .is_some_and(|value| !identifier(value))
            || run.selected_context_ids.len() > 12
            || run
                .selected_context_ids
                .iter()
                .any(|value| !identifier(value))
            || !verdict(&run.functional_correctness)
            || !verdict(&run.governed_completion)
            || !verdict(&run.context_delivery)
            || run
                .metrics
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != METRICS.iter().copied().collect()
        {
            return Err(invalid());
        }
        for (name, metric) in &run.metrics {
            match metric.value {
                Some(value)
                    if value.is_finite()
                        && (0.0..=9_007_199_254_740_991.0).contains(&value)
                        && metric.missing_reason.is_none()
                        && metric.evidence.as_ref().is_some_and(evidence) =>
                {
                    if !matches!(
                        name.as_str(),
                        "active_human_minutes"
                            | "total_cost_usd"
                            | "wall_ms"
                            | "setup_ms"
                            | "validation_ms"
                            | "harmful_context_rate"
                    ) && value.fract() != 0.0
                    {
                        return Err(invalid());
                    }
                    if name == "active_human_minutes"
                        && metric.evidence.as_ref().unwrap().class != EvidenceClass::HumanReview
                    {
                        return Err(invalid());
                    }
                    if name == "harmful_context_rate"
                        && (value > 1.0
                            || run.context_delivery.value != Some(true)
                            || !matches!(
                                metric.evidence.as_ref().unwrap().class,
                                EvidenceClass::IndependentCheck | EvidenceClass::HumanReview
                            ))
                    {
                        return Err(invalid());
                    }
                }
                None if metric.missing_reason.is_some() && metric.evidence.is_none() => {}
                _ => return Err(invalid()),
            }
        }
        let optional_digests = [
            ("host_sha256", &run.host_sha256),
            ("environment_sha256", &run.environment_sha256),
            ("candidate_source_sha256", &run.candidate_source_sha256),
            ("policy_sha256", &run.policy_sha256),
            ("constitution_sha256", &run.constitution_sha256),
            ("context_sha256", &run.context_sha256),
        ];
        if optional_digests
            .iter()
            .any(|(_, value)| value.as_ref().is_some_and(|v| !digest(v)))
            || run.candidate_commit.as_ref().is_some_and(|value| {
                value.len() != 40 || !value.bytes().all(|c| c.is_ascii_hexdigit())
            })
        {
            return Err(invalid());
        }
        let mut missing: BTreeSet<&str> = optional_digests
            .iter()
            .filter_map(|(key, value)| value.is_none().then_some(*key))
            .collect();
        if run.model.is_none() {
            missing.insert("model");
        }
        if run.candidate_commit.is_none() {
            missing.insert("candidate_commit");
        }
        if run.candidate_dirty.is_none() {
            missing.insert("candidate_dirty");
        }
        if missing != run.identity_missing.keys().map(String::as_str).collect() {
            return Err(invalid());
        }
        let (first, arms) = pairs
            .entry(&run.pair_id)
            .or_insert_with(|| (run, BTreeSet::new()));
        if !arms.insert(run.arm.clone())
            || first.task_id != run.task_id
            || first.stratum != run.stratum
            || first.host != run.host
            || first.host_version != run.host_version
            || first.host_sha256 != run.host_sha256
            || first.model != run.model
            || first.kernel_sha256 != run.kernel_sha256
            || first.task_source_sha256 != run.task_source_sha256
            || first.environment_sha256 != run.environment_sha256
            || first.protocol_sha256 != run.protocol_sha256
            || first.evidence_class != run.evidence_class
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "duplicate arm or mismatched task/host/model/source/environment/protocol pair",
            ));
        }
    }
    let incomplete_pairs = pairs
        .iter()
        .filter(|(_, (_, arms))| arms.len() != 2)
        .map(|(id, _)| (*id).to_string())
        .collect();
    let unverified_pair_identities = pairs
        .iter()
        .filter(|(_, (run, _))| {
            run.host_sha256.is_none() || run.environment_sha256.is_none() || run.model.is_none()
        })
        .map(|(id, _)| (*id).to_string())
        .collect();
    let summaries = [Arm::Baseline, Arm::Governed]
        .into_iter()
        .map(|arm| {
            let runs: Vec<_> = input.runs.iter().filter(|run| run.arm == arm).collect();
            let independently_correct: Vec<_> = runs
                .iter()
                .filter(|run| {
                    run.status == TerminalStatus::Completed
                        && run.evidence_class == EvidenceClass::RealHost
                        && run.functional_correctness.value == Some(true)
                        && run
                            .functional_correctness
                            .evidence
                            .as_ref()
                            .is_some_and(|e| e.class == EvidenceClass::IndependentCheck)
                })
                .collect();
            let useful = independently_correct
                .iter()
                .filter(|run| {
                    run.arm == Arm::Baseline
                        || (run.governed_completion.value == Some(true)
                            && run
                                .governed_completion
                                .evidence
                                .as_ref()
                                .is_some_and(|e| e.class == EvidenceClass::NativeVerifier))
                })
                .count();
            let human = runs.iter().try_fold(0.0, |total, run| {
                run.metrics["active_human_minutes"]
                    .value
                    .map(|minutes| total + minutes)
            });
            Summary {
                arm,
                runs: runs.len(),
                completed: runs
                    .iter()
                    .filter(|run| run.status == TerminalStatus::Completed)
                    .count(),
                unsuccessful_or_partial: runs
                    .iter()
                    .filter(|run| run.status != TerminalStatus::Completed)
                    .count(),
                reported_independent_correct: independently_correct.len(),
                reported_verified_useful_work: useful,
                correctness_unobserved: runs
                    .iter()
                    .filter(|run| run.functional_correctness.value.is_none())
                    .count(),
                reviewed_active_human_minutes: human.filter(|_| !runs.is_empty()),
                reported_verified_useful_work_per_reviewed_human_minute: human
                    .filter(|minutes| *minutes > 0.0 && !runs.is_empty())
                    .map(|minutes| useful as f64 / minutes),
            }
        })
        .collect();
    Ok(Report {
        schema_version: "decapod.outcome-comparison.v1",
        publication_ready: false,
        merge_authorized: false,
        evidence_limit: "Producer labels and digest references are assertions. This renderer does not authenticate captures, execute checks, infer semantic question labels, verify native proof, or establish current admission. Deterministic fixtures are not real-model outcomes; missing human attention prevents an efficiency ratio.",
        incomplete_pairs,
        unverified_pair_identities,
        summaries,
        runs: input.runs,
    })
}

pub fn read_and_compare(path: &Path) -> io::Result<Report> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(invalid());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid());
        }
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid());
    }
    let input = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    compare(input)
}

#[cfg(test)]
#[path = "../../../../tests/unit/core/outcome_comparison_tests.rs"]
mod tests;

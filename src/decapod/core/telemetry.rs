//! Bounded, diagnostic-only, privacy-preserving time-series execution telemetry (#1095).
//!
//! Boundary invariants:
//! - Strictly diagnostic (`read_only = true`, `authoritative = false`, `can_grant_authority = false`).
//! - Never grants validation, task-completion, lease, or publication authority.
//! - Keeps Decapod kernel/product diagnostics separate from Propodus (#230) service SLOs
//!   and Dactyl (#144) transport budgets; zero paid telemetry dependencies.
//! - Reuses existing structured receipts (`BrokerEvent`, `TraceEvent`, timeline/governance
//!   receipts, and SQLite workload observations).
//! - Strict privacy redaction: prompts, secrets, raw evidence blobs, filesystem paths,
//!   URLs, emails, and unredacted customer/repo identifiers never enter telemetry output.
//! - Strict ring-buffer, stage-cardinality, bucket-cardinality, and retention bounds.

use crate::core::broker::BrokerEvent;
use crate::core::db;
use crate::core::events;
use crate::core::observability_projections::snapshot::load_raw_governance_json_readonly;
use crate::core::observability_projections::specs::inspect_validation_receipt_value;
use crate::core::schemas;
use crate::core::trace::{TraceEvent, redact_string};
use fancy_regex::Regex;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DIAGNOSTIC_TELEMETRY_SCHEMA_VERSION: &str = "decapod.diagnostic-telemetry.v1";

pub const DEFAULT_RING_BUFFER_CAPACITY: usize = 256;
pub const HARD_RING_BUFFER_CAPACITY: usize = 512;
pub const DEFAULT_STAGE_CARDINALITY: usize = 24;
pub const HARD_STAGE_CARDINALITY: usize = 32;
pub const DEFAULT_BUCKET_CARDINALITY: usize = 64;
pub const HARD_BUCKET_CARDINALITY: usize = 128;
pub const DEFAULT_RETENTION_SECS: u64 = 24 * 3600;
pub const HARD_RETENTION_SECS: u64 = 7 * 24 * 3600;
pub const DEFAULT_BUCKET_SECS: u64 = 60;
pub const MAX_DIAGNOSTIC_LABEL_BYTES: usize = 64;

pub const SCOPE_SEPARATION_NOTE: &str = "Product/kernel diagnostics only (#1095); separate from Propodus (#230) service SLOs and Dactyl (#144) transport budgets; cannot grant authority; zero paid telemetry dependency.";

static PATH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:/(?:Users|home|var|tmp|opt|etc|srv|private|workspace)/[^\s'",;)}\]]+|[A-Za-z]:\\[^\s'",;)}\]]+)"#)
        .expect("valid path regex")
});

static URL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:https?|ssh|git)://[^\s'",;)}\]]+|git@[A-Za-z0-9._-]+:[^\s'",;)}\]]+"#)
        .expect("valid url regex")
});

static EMAIL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"#).expect("valid email regex")
});

static IDENTIFIER_KV_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:customer|tenant|account|org|organization|repo|repository|project|user|owner)(?:_id|_name|_slug)?\s*[:=]\s*['"]?[A-Za-z0-9._:/-]+['"]?"#)
        .expect("valid identifier kv regex")
});

static ORG_REPO_SLUG_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b[A-Za-z0-9][A-Za-z0-9._-]{1,63}/[A-Za-z0-9][A-Za-z0-9._-]{1,63}\b"#)
        .expect("valid org/repo slug regex")
});

static PROMPT_OR_EVIDENCE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\b(?:prompt|system_prompt|user_prompt|raw_evidence|transcript|stdout|stderr|stacktrace|backtrace|diff\s+--git)\b[\s\S]*"#)
        .expect("valid prompt/evidence regex")
});

/// Nearest-rank latency quantiles in microseconds (#1095, #1938).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct LatencyQuantilesUs {
    pub sample_count: usize,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

/// Compute exact nearest-rank `p50`/`p95`/`p99`/`max` in microseconds.
/// Returns `None` when no observations exist rather than fabricating `0`.
pub fn compute_latency_quantiles_us(samples: &[u64]) -> Option<LatencyQuantilesUs> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len();
    let rank = |pct: usize| -> usize {
        // Nearest-rank index (0-based): ceil(pct/100 * n) - 1
        let pos = (n.saturating_mul(pct)).div_ceil(100);
        pos.saturating_sub(1).min(n.saturating_sub(1))
    };
    Some(LatencyQuantilesUs {
        sample_count: n,
        p50_us: sorted[rank(50)],
        p95_us: sorted[rank(95)],
        p99_us: sorted[rank(99)],
        max_us: sorted[n - 1],
    })
}

/// Proof freshness state reused from validation receipts and timeline projections.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProofFreshness {
    Fresh,
    Stale,
    Unproven,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProofFreshnessCounts {
    pub fresh: u64,
    pub stale: u64,
    pub unproven: u64,
    pub unknown: u64,
}

/// Outcome classification preserving denied and unknown outcomes explicitly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticOutcome {
    Succeeded,
    Denied,
    Failed,
    CancelledOrPartial,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DiagnosticOutcomeCounts {
    pub succeeded: u64,
    pub denied: u64,
    pub failed: u64,
    pub cancelled_or_partial: u64,
    pub unknown: u64,
}

/// Observed repository/datastore workload size dimensions. Missing fields stay `None`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkloadSizeMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tasks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_agents: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sqlite_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wal_bytes: Option<u64>,
}

fn generate_ephemeral_salt() -> [u8; 32] {
    let mut salt = [0u8; 32];
    if SystemRandom::new().fill(&mut salt).is_ok() {
        return salt;
    }
    let mut hasher = Sha256::new();
    hasher.update(b"decapod.telemetry.ephemeral_salt.fallback.v1:");
    hasher.update(std::process::id().to_le_bytes());
    let stack_addr = (&salt as *const [u8; 32]) as usize;
    hasher.update(stack_addr.to_le_bytes());
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    hasher.update(nanos.to_le_bytes());
    let digest = hasher.finalize();
    salt.copy_from_slice(&digest);
    salt
}

/// Non-reversible per-instance salted reference (`<kind>:<12-hex>`).
///
/// Hashing an identifier does NOT exempt its observation, retention, or correlation
/// from authorization requirements: this helper MUST only be invoked when explicitly
/// authorized via `TelemetryRetentionPolicy::allow_identifier_observation`.
pub fn anonymize_identifier_with_salt(kind: &str, raw: &str, salt: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"decapod.telemetry.anon.v2:");
    hasher.update(salt);
    hasher.update(b":");
    hasher.update(kind.as_bytes());
    hasher.update(b":");
    hasher.update(raw.trim().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    let short_kind: String = kind
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(12)
        .collect();
    let prefix = if short_kind.is_empty() {
        "id"
    } else {
        short_kind.as_str()
    };
    format!("{prefix}:{}", &digest[..12])
}

/// Redact secrets, prompts, raw evidence, paths, URLs, emails, and customer/repo identifiers
/// from a telemetry text field and bound its length.
pub fn sanitize_telemetry_text(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return "[PROMPT_OR_EVIDENCE_REDACTED]".to_string();
    }
    let mut out = redact_string(trimmed);
    out = PROMPT_OR_EVIDENCE_PATTERN
        .replace_all(&out, "[PROMPT_OR_EVIDENCE_REDACTED]")
        .into_owned();
    out = URL_PATTERN.replace_all(&out, "[URL_REDACTED]").into_owned();
    out = PATH_PATTERN
        .replace_all(&out, "[PATH_REDACTED]")
        .into_owned();
    out = EMAIL_PATTERN
        .replace_all(&out, "[EMAIL_REDACTED]")
        .into_owned();
    out = IDENTIFIER_KV_PATTERN
        .replace_all(&out, "[IDENTIFIER_REDACTED]")
        .into_owned();
    out = ORG_REPO_SLUG_PATTERN
        .replace_all(&out, "[REPO_SLUG_REDACTED]")
        .into_owned();
    if out.len() > MAX_DIAGNOSTIC_LABEL_BYTES {
        let mut end = MAX_DIAGNOSTIC_LABEL_BYTES.saturating_sub(3);
        while end > 0 && !out.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &out[..end])
    } else {
        out
    }
}

/// Strict bounded policy governing telemetry ring-buffer capacity, cardinality, and retention.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryRetentionPolicy {
    pub max_samples: usize,
    pub max_stage_cardinality: usize,
    pub max_bucket_cardinality: usize,
    pub retention_secs: u64,
    pub bucket_seconds: u64,
    /// Explicit fail-closed authorization gate controlling whether `repo_identifier`
    /// may be observed into a per-instance ephemerally salted `repo_digest`.
    /// Defaults to `false` (drop identifiers rather than hashing/retaining them).
    #[serde(default)]
    pub allow_identifier_observation: bool,
}

impl Default for TelemetryRetentionPolicy {
    fn default() -> Self {
        Self {
            max_samples: DEFAULT_RING_BUFFER_CAPACITY,
            max_stage_cardinality: DEFAULT_STAGE_CARDINALITY,
            max_bucket_cardinality: DEFAULT_BUCKET_CARDINALITY,
            retention_secs: DEFAULT_RETENTION_SECS,
            bucket_seconds: DEFAULT_BUCKET_SECS,
            allow_identifier_observation: false,
        }
    }
}

impl TelemetryRetentionPolicy {
    pub fn clamped(self) -> Self {
        Self {
            max_samples: self.max_samples.clamp(1, HARD_RING_BUFFER_CAPACITY),
            max_stage_cardinality: self.max_stage_cardinality.clamp(1, HARD_STAGE_CARDINALITY),
            max_bucket_cardinality: self
                .max_bucket_cardinality
                .clamp(1, HARD_BUCKET_CARDINALITY),
            retention_secs: self.retention_secs.clamp(1, HARD_RETENTION_SECS),
            bucket_seconds: self.bucket_seconds.max(1),
            allow_identifier_observation: self.allow_identifier_observation,
        }
    }
}

/// Optional queueing/execution/lock timing breakdown attached to structured receipts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReceiptTimingObservation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_wait_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_wait_us: Option<u64>,
}

/// Caller-supplied raw diagnostic input; sanitized immediately on ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RawDiagnosticSample {
    pub epoch_secs: u64,
    pub stage: String,
    pub operation: String,
    pub outcome: DiagnosticOutcome,
    pub proof_freshness: ProofFreshness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_wait_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_wait_us: Option<u64>,
    #[serde(default)]
    pub lock_contended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_identifier: Option<String>,
}

/// Sanitized, bounded record stored in the ring buffer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SanitizedTelemetrySample {
    pub epoch_secs: u64,
    pub stage: String,
    pub operation: String,
    pub outcome: DiagnosticOutcome,
    pub proof_freshness: ProofFreshness,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_wait_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_wait_us: Option<u64>,
    pub lock_contended: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_digest: Option<String>,
}

/// Time-series bucket over diagnostic samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticTimeBucket {
    pub bucket_start_epoch: u64,
    pub sample_count: usize,
    pub outcomes: DiagnosticOutcomeCounts,
    pub proof_freshness: ProofFreshnessCounts,
    pub lock_contention_events: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_wait: Option<LatencyQuantilesUs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<LatencyQuantilesUs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_wait: Option<LatencyQuantilesUs>,
}

/// Cardinality/eviction accounting proving strict bounds were enforced.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TelemetryBoundAccounting {
    pub retained_samples: usize,
    pub max_samples: usize,
    pub ring_evicted_samples: u64,
    pub retention_evicted_samples: u64,
    pub active_stage_cardinality: usize,
    pub max_stage_cardinality: usize,
    pub cardinality_overflow_samples: u64,
    pub bucket_evicted_count: u64,
    pub max_bucket_cardinality: usize,
    pub redactions_applied: u64,
}

/// Compact diagnostic summary embeddable in `ExecutionTelemetryProjection`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticTelemetryReport {
    pub schema_version: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub can_grant_authority: bool,
    pub diagnostic_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_digest: Option<String>,
    pub workload: WorkloadSizeMetrics,
    pub outcomes: DiagnosticOutcomeCounts,
    pub proof_freshness: ProofFreshnessCounts,
    pub lock_contention_events: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_wait: Option<LatencyQuantilesUs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<LatencyQuantilesUs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_wait: Option<LatencyQuantilesUs>,
    pub stage_counts: BTreeMap<String, u64>,
    pub series: Vec<DiagnosticTimeBucket>,
    pub bounds: TelemetryBoundAccounting,
    pub missing_measurements: Vec<String>,
    pub scope_separation_note: String,
}

/// Bounded in-memory ring-buffer collector for diagnostic execution telemetry.
#[derive(Debug, Clone)]
pub struct TelemetryRingBuffer {
    policy: TelemetryRetentionPolicy,
    ephemeral_salt: [u8; 32],
    samples: VecDeque<SanitizedTelemetrySample>,
    known_stages: BTreeSet<String>,
    ring_evicted_samples: u64,
    retention_evicted_samples: u64,
    cardinality_overflow_samples: u64,
    redactions_applied: u64,
}

impl TelemetryRingBuffer {
    pub fn new(policy: TelemetryRetentionPolicy) -> Self {
        let clamped = policy.clamped();
        Self {
            policy: clamped,
            ephemeral_salt: generate_ephemeral_salt(),
            samples: VecDeque::with_capacity(clamped.max_samples.min(64)),
            known_stages: BTreeSet::new(),
            ring_evicted_samples: 0,
            retention_evicted_samples: 0,
            cardinality_overflow_samples: 0,
            redactions_applied: 0,
        }
    }

    pub fn policy(&self) -> TelemetryRetentionPolicy {
        self.policy
    }

    /// Prune samples older than `retention_secs` relative to `now_epoch_secs`.
    pub fn prune_expired(&mut self, now_epoch_secs: u64) {
        let cutoff = now_epoch_secs.saturating_sub(self.policy.retention_secs);
        let mut retained = VecDeque::with_capacity(self.samples.len());
        let mut evicted = 0u64;
        for sample in self.samples.drain(..) {
            if sample.epoch_secs >= cutoff {
                retained.push_back(sample);
            } else {
                evicted += 1;
            }
        }
        self.samples = retained;
        self.retention_evicted_samples = self.retention_evicted_samples.saturating_add(evicted);
    }

    /// Record a raw diagnostic sample after redaction, cardinality capping, and FIFO eviction.
    pub fn record(&mut self, raw: RawDiagnosticSample, now_epoch_secs: u64) {
        self.prune_expired(now_epoch_secs);

        let sanitized_stage = sanitize_telemetry_text(&raw.stage);
        let sanitized_op = sanitize_telemetry_text(&raw.operation);
        if sanitized_stage != raw.stage.trim() || sanitized_op != raw.operation.trim() {
            self.redactions_applied = self.redactions_applied.saturating_add(1);
        }

        let bounded_stage = if self.known_stages.contains(&sanitized_stage) {
            sanitized_stage
        } else if self.known_stages.len() < self.policy.max_stage_cardinality {
            self.known_stages.insert(sanitized_stage.clone());
            sanitized_stage
        } else {
            self.cardinality_overflow_samples = self.cardinality_overflow_samples.saturating_add(1);
            "overflow:other".to_string()
        };

        let repo_digest = raw
            .repo_identifier
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .and_then(|id| {
                self.redactions_applied = self.redactions_applied.saturating_add(1);
                if self.policy.allow_identifier_observation {
                    Some(anonymize_identifier_with_salt(
                        "repo",
                        id,
                        &self.ephemeral_salt,
                    ))
                } else {
                    None
                }
            });

        if self.samples.len() >= self.policy.max_samples {
            self.samples.pop_front();
            self.ring_evicted_samples = self.ring_evicted_samples.saturating_add(1);
        }

        self.samples.push_back(SanitizedTelemetrySample {
            epoch_secs: raw.epoch_secs,
            stage: bounded_stage,
            operation: sanitized_op,
            outcome: raw.outcome,
            proof_freshness: raw.proof_freshness,
            queue_wait_us: raw.queue_wait_us,
            execution_us: raw.execution_us,
            lock_wait_us: raw.lock_wait_us,
            lock_contended: raw.lock_contended || raw.lock_wait_us.is_some_and(|w| w > 0),
            repo_digest,
        });
    }

    /// Reuse an existing `BrokerEvent` structured receipt without leaking paths/secrets.
    pub fn record_broker_event(
        &mut self,
        event: &BrokerEvent,
        timing: ReceiptTimingObservation,
        now_epoch_secs: u64,
    ) {
        let status_lc = event.status.to_ascii_lowercase();
        let outcome = if status_lc == "ok" || status_lc == "success" {
            DiagnosticOutcome::Succeeded
        } else if status_lc.contains("denied")
            || status_lc.contains("reject")
            || status_lc.contains("refused")
        {
            DiagnosticOutcome::Denied
        } else if status_lc.contains("fail") || status_lc.contains("err") {
            DiagnosticOutcome::Failed
        } else if status_lc.contains("cancel") || status_lc.contains("timeout") {
            DiagnosticOutcome::CancelledOrPartial
        } else {
            DiagnosticOutcome::Unknown
        };
        let epoch_secs = parse_ts_epoch(&event.ts).unwrap_or(now_epoch_secs);
        self.record(
            RawDiagnosticSample {
                epoch_secs,
                stage: "broker".to_string(),
                operation: event.op.clone(),
                outcome,
                proof_freshness: ProofFreshness::Unknown,
                queue_wait_us: timing.queue_wait_us,
                execution_us: timing.execution_us,
                lock_wait_us: timing.lock_wait_us,
                lock_contended: timing.lock_wait_us.is_some_and(|w| w > 0),
                repo_identifier: None,
            },
            now_epoch_secs,
        );
    }

    /// Reuse an existing `TraceEvent` structured receipt without leaking request/response bodies.
    pub fn record_trace_event(
        &mut self,
        trace: &TraceEvent,
        timing: ReceiptTimingObservation,
        proof_freshness: ProofFreshness,
        outcome: DiagnosticOutcome,
        now_epoch_secs: u64,
    ) {
        let epoch_secs = parse_ts_epoch(&trace.ts).unwrap_or(now_epoch_secs);
        self.record(
            RawDiagnosticSample {
                epoch_secs,
                stage: "trace".to_string(),
                operation: trace.op.clone(),
                outcome,
                proof_freshness,
                queue_wait_us: timing.queue_wait_us,
                execution_us: timing.execution_us,
                lock_wait_us: timing.lock_wait_us,
                lock_contended: timing.lock_wait_us.is_some_and(|w| w > 0),
                repo_identifier: None,
            },
            now_epoch_secs,
        );
    }

    pub fn samples(&self) -> &VecDeque<SanitizedTelemetrySample> {
        &self.samples
    }

    /// Materialize a bounded diagnostic time-series report. Never grants authority.
    pub fn report(
        &self,
        workload: WorkloadSizeMetrics,
        repo_identifier: Option<&str>,
    ) -> DiagnosticTelemetryReport {
        let mut outcomes = DiagnosticOutcomeCounts::default();
        let mut freshness = ProofFreshnessCounts::default();
        let mut lock_contention_events: u64 = 0;
        let mut queue_samples = Vec::new();
        let mut exec_samples = Vec::new();
        let mut lock_samples = Vec::new();
        let mut stage_counts: BTreeMap<String, u64> = BTreeMap::new();

        #[derive(Default)]
        struct BucketAcc {
            sample_count: usize,
            outcomes: DiagnosticOutcomeCounts,
            freshness: ProofFreshnessCounts,
            lock_contention_events: u64,
            queue: Vec<u64>,
            exec: Vec<u64>,
            lock: Vec<u64>,
        }
        let mut bucket_map: BTreeMap<u64, BucketAcc> = BTreeMap::new();

        for s in &self.samples {
            *stage_counts.entry(s.stage.clone()).or_insert(0) += 1;
            let b_start = (s.epoch_secs / self.policy.bucket_seconds) * self.policy.bucket_seconds;
            let b = bucket_map.entry(b_start).or_default();
            b.sample_count += 1;

            match s.outcome {
                DiagnosticOutcome::Succeeded => {
                    outcomes.succeeded += 1;
                    b.outcomes.succeeded += 1;
                }
                DiagnosticOutcome::Denied => {
                    outcomes.denied += 1;
                    b.outcomes.denied += 1;
                }
                DiagnosticOutcome::Failed => {
                    outcomes.failed += 1;
                    b.outcomes.failed += 1;
                }
                DiagnosticOutcome::CancelledOrPartial => {
                    outcomes.cancelled_or_partial += 1;
                    b.outcomes.cancelled_or_partial += 1;
                }
                DiagnosticOutcome::Unknown => {
                    outcomes.unknown += 1;
                    b.outcomes.unknown += 1;
                }
            }

            match s.proof_freshness {
                ProofFreshness::Fresh => {
                    freshness.fresh += 1;
                    b.freshness.fresh += 1;
                }
                ProofFreshness::Stale => {
                    freshness.stale += 1;
                    b.freshness.stale += 1;
                }
                ProofFreshness::Unproven => {
                    freshness.unproven += 1;
                    b.freshness.unproven += 1;
                }
                ProofFreshness::Unknown => {
                    freshness.unknown += 1;
                    b.freshness.unknown += 1;
                }
            }

            if s.lock_contended {
                lock_contention_events += 1;
                b.lock_contention_events += 1;
            }
            if let Some(q) = s.queue_wait_us {
                queue_samples.push(q);
                b.queue.push(q);
            }
            if let Some(e) = s.execution_us {
                exec_samples.push(e);
                b.exec.push(e);
            }
            if let Some(l) = s.lock_wait_us {
                lock_samples.push(l);
                b.lock.push(l);
            }
        }

        let total_buckets = bucket_map.len();
        let skip_buckets = total_buckets.saturating_sub(self.policy.max_bucket_cardinality);
        let series: Vec<DiagnosticTimeBucket> = bucket_map
            .into_iter()
            .skip(skip_buckets)
            .map(|(bucket_start_epoch, b)| DiagnosticTimeBucket {
                bucket_start_epoch,
                sample_count: b.sample_count,
                outcomes: b.outcomes,
                proof_freshness: b.freshness,
                lock_contention_events: b.lock_contention_events,
                queue_wait: compute_latency_quantiles_us(&b.queue),
                execution: compute_latency_quantiles_us(&b.exec),
                lock_wait: compute_latency_quantiles_us(&b.lock),
            })
            .collect();

        let queue_wait = compute_latency_quantiles_us(&queue_samples);
        let execution = compute_latency_quantiles_us(&exec_samples);
        let lock_wait = compute_latency_quantiles_us(&lock_samples);

        let mut missing_measurements = Vec::new();
        if queue_wait.is_none() {
            missing_measurements.push("queue_wait".to_string());
        }
        if execution.is_none() {
            missing_measurements.push("execution".to_string());
        }
        if lock_wait.is_none() {
            missing_measurements.push("lock_wait".to_string());
        }
        if workload.tasks.is_none() {
            missing_measurements.push("workload.tasks".to_string());
        }
        if workload.events.is_none() {
            missing_measurements.push("workload.events".to_string());
        }

        DiagnosticTelemetryReport {
            schema_version: DIAGNOSTIC_TELEMETRY_SCHEMA_VERSION.to_string(),
            read_only: true,
            authoritative: false,
            can_grant_authority: false,
            diagnostic_only: true,
            repo_digest: if self.policy.allow_identifier_observation {
                repo_identifier
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| anonymize_identifier_with_salt("repo", s, &self.ephemeral_salt))
            } else {
                None
            },
            workload,
            outcomes,
            proof_freshness: freshness,
            lock_contention_events,
            queue_wait,
            execution,
            lock_wait,
            stage_counts,
            series,
            bounds: TelemetryBoundAccounting {
                retained_samples: self.samples.len(),
                max_samples: self.policy.max_samples,
                ring_evicted_samples: self.ring_evicted_samples,
                retention_evicted_samples: self.retention_evicted_samples,
                active_stage_cardinality: self.known_stages.len(),
                max_stage_cardinality: self.policy.max_stage_cardinality,
                cardinality_overflow_samples: self.cardinality_overflow_samples,
                bucket_evicted_count: skip_buckets as u64,
                max_bucket_cardinality: self.policy.max_bucket_cardinality,
                redactions_applied: self.redactions_applied,
            },
            missing_measurements,
            scope_separation_note: SCOPE_SEPARATION_NOTE.to_string(),
        }
    }
}

pub fn parse_ts_epoch(ts: &str) -> Option<u64> {
    let trimmed = ts.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed.strip_suffix('Z').unwrap_or(trimmed);
    if let Ok(v) = stripped.parse::<u64>() {
        return Some(v);
    }
    let (date_part, time_part) = stripped.split_once('T')?;
    let mut d = date_part.split('-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let time_core = time_part.split('.').next()?.split('+').next()?;
    let mut t = time_core.split(':');
    let hour: u64 = t.next()?.parse().ok()?;
    let min: u64 = t.next()?.parse().ok()?;
    let sec: u64 = t.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u64;
    let m = u64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + u64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + (doe as i64) - 719_468;
    if days < 0 {
        return None;
    }
    Some((days as u64) * 86_400 + hour * 3_600 + min * 60 + sec)
}

/// Read-only helper that measures local SQLite workload dimensions and collects
/// structured governance/event receipts into a bounded `DiagnosticTelemetryReport`.
pub fn collect_workspace_diagnostic_telemetry(
    repo_root: &Path,
    now_epoch_secs: u64,
    policy: TelemetryRetentionPolicy,
) -> DiagnosticTelemetryReport {
    let mut buffer = TelemetryRingBuffer::new(policy);
    let data_dir = repo_root.join(".decapod/data");
    let db_path = data_dir.join(schemas::LOCAL_DB_NAME);

    let mut workload = WorkloadSizeMetrics {
        worktrees: fs::read_dir(repo_root.join(".decapod/workspaces"))
            .ok()
            .map(|iter| {
                iter.filter_map(Result::ok)
                    .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                    .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                    .count() as u64
            }),
        sqlite_bytes: fs::metadata(&db_path).ok().map(|m| m.len()),
        wal_bytes: fs::metadata(format!("{}-wal", db_path.display()))
            .ok()
            .map(|m| m.len()),
        ..Default::default()
    };

    if db_path.exists()
        && let Ok(conn) = db::db_connect_readonly_bounded(
            &db_path.to_string_lossy(),
            std::time::Duration::from_millis(150),
        )
    {
        if conn.has_table("tasks").unwrap_or(false) {
            workload.tasks = conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get::<_, u64>(0))
                .ok();
        }
        if conn.has_table("agents").unwrap_or(false) {
            workload.active_agents = conn
                .query_row("SELECT COUNT(*) FROM agents", [], |r| r.get::<_, u64>(0))
                .ok();
        }
        if conn.has_table(events::EVENTS_TABLE).unwrap_or(false) {
            workload.events = conn
                .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, u64>(0))
                .ok();
            let limit = buffer.policy().max_samples as i64;
            if let Ok(mut stmt) =
                conn.prepare("SELECT stream, ts, event_type FROM events ORDER BY seq DESC LIMIT ?1")
                && let Ok(rows) = stmt.query_map([limit], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                })
            {
                for row in rows.flatten() {
                    let (stream, ts, event_type) = row;
                    let ev_lc = event_type.to_ascii_lowercase();
                    let outcome = if ev_lc.contains("denied")
                        || ev_lc.contains("reject")
                        || ev_lc.contains("block")
                    {
                        DiagnosticOutcome::Denied
                    } else if ev_lc.contains("fail") || ev_lc.contains("err") {
                        DiagnosticOutcome::Failed
                    } else if ev_lc.contains("cancel") || ev_lc.contains("timeout") {
                        DiagnosticOutcome::CancelledOrPartial
                    } else if ev_lc.contains("done")
                        || ev_lc.contains("pass")
                        || ev_lc.contains("claim")
                        || ev_lc.contains("commit")
                    {
                        DiagnosticOutcome::Succeeded
                    } else {
                        DiagnosticOutcome::Unknown
                    };
                    buffer.record(
                        RawDiagnosticSample {
                            epoch_secs: parse_ts_epoch(&ts).unwrap_or(now_epoch_secs),
                            stage: stream,
                            operation: event_type,
                            outcome,
                            proof_freshness: ProofFreshness::Unknown,
                            queue_wait_us: None,
                            execution_us: None,
                            lock_wait_us: None,
                            lock_contended: false,
                            repo_identifier: None,
                        },
                        now_epoch_secs,
                    );
                }
            }
        }
    }

    if let Some(gov) = load_raw_governance_json_readonly(repo_root) {
        let receipt = inspect_validation_receipt_value(Some(&gov), None);
        let freshness = if !receipt.present {
            ProofFreshness::Unproven
        } else if receipt.stale_vs_head {
            ProofFreshness::Stale
        } else if receipt.integrity_valid {
            ProofFreshness::Fresh
        } else {
            ProofFreshness::Unknown
        };
        buffer.record(
            RawDiagnosticSample {
                epoch_secs: now_epoch_secs,
                stage: "validation_receipt".to_string(),
                operation: "receipt.freshness".to_string(),
                outcome: match freshness {
                    ProofFreshness::Fresh => DiagnosticOutcome::Succeeded,
                    ProofFreshness::Stale | ProofFreshness::Unproven => DiagnosticOutcome::Denied,
                    ProofFreshness::Unknown => DiagnosticOutcome::Unknown,
                },
                proof_freshness: freshness,
                queue_wait_us: None,
                execution_us: None,
                lock_wait_us: None,
                lock_contended: false,
                repo_identifier: None,
            },
            now_epoch_secs,
        );
    }

    let repo_id = if policy.allow_identifier_observation {
        repo_root
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
    } else {
        None
    };
    buffer.report(workload, repo_id.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantiles_and_queue_vs_execution_and_lock_waits_preserve_missing_as_none() {
        assert_eq!(compute_latency_quantiles_us(&[]), None);

        let mut buf = TelemetryRingBuffer::new(TelemetryRetentionPolicy::default());
        for i in 1..=100u64 {
            buf.record(
                RawDiagnosticSample {
                    epoch_secs: 1_700_000_000 + (i % 3) * 10,
                    stage: "todo".to_string(),
                    operation: "claim".to_string(),
                    outcome: DiagnosticOutcome::Succeeded,
                    proof_freshness: ProofFreshness::Fresh,
                    queue_wait_us: Some(i * 10),
                    execution_us: Some(i * 100),
                    lock_wait_us: if i % 10 == 0 { Some(i * 5) } else { None },
                    lock_contended: i % 10 == 0,
                    repo_identifier: None,
                },
                1_700_000_050,
            );
        }

        let report = buf.report(
            WorkloadSizeMetrics {
                tasks: Some(100),
                active_agents: Some(4),
                events: Some(300),
                worktrees: Some(2),
                sqlite_bytes: Some(65_536),
                wal_bytes: Some(4_096),
            },
            Some("acme/private-repo"),
        );

        let q = report.queue_wait.expect("queue_wait quantiles");
        assert_eq!(q.sample_count, 100);
        assert_eq!(q.p50_us, 500);
        assert_eq!(q.p95_us, 950);
        assert_eq!(q.p99_us, 990);
        assert_eq!(q.max_us, 1000);

        let e = report.execution.expect("execution quantiles");
        assert_eq!(e.p50_us, 5000);
        assert_eq!(e.p95_us, 9500);
        assert_eq!(e.p99_us, 9900);

        let l = report.lock_wait.expect("lock_wait quantiles");
        assert_eq!(l.sample_count, 10);
        assert_eq!(l.p50_us, 250);
        assert_eq!(l.max_us, 500);
        assert_eq!(report.lock_contention_events, 10);
        assert_eq!(report.proof_freshness.fresh, 100);
    }

    #[test]
    fn ring_buffer_enforces_capacity_cardinality_bucket_and_retention_caps() {
        let policy = TelemetryRetentionPolicy {
            max_samples: 4,
            max_stage_cardinality: 2,
            max_bucket_cardinality: 2,
            retention_secs: 300,
            bucket_seconds: 60,
            allow_identifier_observation: false,
        };
        let mut buf = TelemetryRingBuffer::new(policy);

        // Expired sample at t=1000 relative to now=1500 (cutoff=1200) -> pruned.
        buf.record(
            RawDiagnosticSample {
                epoch_secs: 1000,
                stage: "old".to_string(),
                operation: "op".to_string(),
                outcome: DiagnosticOutcome::Unknown,
                proof_freshness: ProofFreshness::Unknown,
                queue_wait_us: None,
                execution_us: None,
                lock_wait_us: None,
                lock_contended: false,
                repo_identifier: None,
            },
            1500,
        );
        assert_eq!(buf.samples().len(), 1);

        // Insert 6 fresh samples across 4 distinct stages and 4 distinct minute buckets.
        for i in 0..6u64 {
            buf.record(
                RawDiagnosticSample {
                    epoch_secs: 1250 + i * 60,
                    stage: format!("stage_{i}"),
                    operation: "run".to_string(),
                    outcome: if i % 2 == 0 {
                        DiagnosticOutcome::Denied
                    } else {
                        DiagnosticOutcome::Unknown
                    },
                    proof_freshness: if i % 2 == 0 {
                        ProofFreshness::Stale
                    } else {
                        ProofFreshness::Unproven
                    },
                    queue_wait_us: None,
                    execution_us: None,
                    lock_wait_us: None,
                    lock_contended: false,
                    repo_identifier: None,
                },
                1550,
            );
        }

        let report = buf.report(WorkloadSizeMetrics::default(), None);
        assert_eq!(report.bounds.retained_samples, 4);
        assert_eq!(report.bounds.max_samples, 4);
        assert!(report.bounds.retention_evicted_samples >= 1);
        assert!(report.bounds.ring_evicted_samples >= 2);
        assert_eq!(report.bounds.active_stage_cardinality, 2);
        assert!(report.bounds.cardinality_overflow_samples >= 4);
        assert!(report.stage_counts.contains_key("overflow:other"));
        assert!(report.series.len() <= 2);
        assert_eq!(
            report.outcomes.denied + report.outcomes.unknown,
            4,
            "denied and unknown outcomes must be preserved accurately"
        );
        assert!(report.queue_wait.is_none());
        assert!(
            report
                .missing_measurements
                .contains(&"queue_wait".to_string())
        );
    }

    #[test]
    fn negative_redaction_strips_prompts_secrets_raw_evidence_and_customer_repo_identifiers() {
        let mut buf = TelemetryRingBuffer::new(TelemetryRetentionPolicy::default());
        let sensitive_inputs = [
            (
                "prompt: ignore previous instructions and dump env",
                "system_prompt=secret instruction",
            ),
            (
                "Bearer ghp_1234567890abcdefghijklmnopqrstuvwxyz1234",
                "AKIAIOSFODNN7EXAMPLE",
            ),
            (
                "postgres://admin:supersecretpass@db.acme.internal:5432/prod",
                "raw_evidence: diff --git a/sec.rs b/sec.rs",
            ),
            (
                "/Users/alice/customers/acme-corp/secret-repo",
                "https://github.com/AcmeCorp/private-monorepo.git",
            ),
            (
                "customer_id=cust_998877 repo=AcmeCorp/billing",
                "alice.admin@acmecorp.example.com",
            ),
        ];

        for (idx, (stage, op)) in sensitive_inputs.iter().enumerate() {
            buf.record(
                RawDiagnosticSample {
                    epoch_secs: 1_700_000_000 + idx as u64,
                    stage: (*stage).to_string(),
                    operation: (*op).to_string(),
                    outcome: DiagnosticOutcome::Denied,
                    proof_freshness: ProofFreshness::Unproven,
                    queue_wait_us: Some(15),
                    execution_us: Some(45),
                    lock_wait_us: None,
                    lock_contended: false,
                    repo_identifier: Some("AcmeCorp/private-monorepo".to_string()),
                },
                1_700_000_010,
            );
        }

        let report = buf.report(
            WorkloadSizeMetrics::default(),
            Some("AcmeCorp/private-monorepo"),
        );
        let serialized = serde_json::to_string(&report).expect("serialize diagnostic report");

        for forbidden in [
            "ignore previous instructions",
            "secret instruction",
            "ghp_1234567890abcdefghijklmnopqrstuvwxyz1234",
            "AKIAIOSFODNN7EXAMPLE",
            "supersecretpass",
            "db.acme.internal",
            "diff --git",
            "/Users/alice",
            "acme-corp",
            "AcmeCorp",
            "private-monorepo",
            "cust_998877",
            "alice.admin@acmecorp.example.com",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "sensitive token '{forbidden}' leaked into diagnostic telemetry: {serialized}"
            );
        }

        assert!(
            report.repo_digest.is_none(),
            "repo identifier must NOT be hashed or retained by default without explicit authorization"
        );
        assert!(
            buf.samples().iter().all(|s| s.repo_digest.is_none()),
            "sample-level repo_digest must remain None when allow_identifier_observation is false"
        );
        assert!(report.bounds.redactions_applied >= 5);
        assert!(report.read_only);
        assert!(!report.authoritative);
        assert!(!report.can_grant_authority);
        assert!(report.diagnostic_only);
    }

    #[test]
    fn explicit_authorized_identifier_observation_uses_per_instance_ephemeral_salt() {
        let policy = TelemetryRetentionPolicy {
            allow_identifier_observation: true,
            ..Default::default()
        };
        let mut buf_a = TelemetryRingBuffer::new(policy);
        let mut buf_b = TelemetryRingBuffer::new(policy);

        for buf in [&mut buf_a, &mut buf_b] {
            buf.record(
                RawDiagnosticSample {
                    epoch_secs: 1_700_000_000,
                    stage: "todo".to_string(),
                    operation: "claim".to_string(),
                    outcome: DiagnosticOutcome::Succeeded,
                    proof_freshness: ProofFreshness::Fresh,
                    queue_wait_us: Some(10),
                    execution_us: Some(20),
                    lock_wait_us: None,
                    lock_contended: false,
                    repo_identifier: Some("AcmeCorp/private-monorepo".to_string()),
                },
                1_700_000_010,
            );
        }

        let rep_a = buf_a.report(
            WorkloadSizeMetrics::default(),
            Some("AcmeCorp/private-monorepo"),
        );
        let rep_b = buf_b.report(
            WorkloadSizeMetrics::default(),
            Some("AcmeCorp/private-monorepo"),
        );

        let dig_a = rep_a
            .repo_digest
            .as_deref()
            .expect("explicitly authorized instance A emits salted repo digest");
        let dig_b = rep_b
            .repo_digest
            .as_deref()
            .expect("explicitly authorized instance B emits salted repo digest");
        assert!(dig_a.starts_with("repo:"));
        assert!(dig_b.starts_with("repo:"));
        assert_ne!(
            dig_a, dig_b,
            "independent instances must use distinct ephemeral salts so digests cannot correlate across sessions"
        );
        assert_ne!(
            buf_a
                .samples()
                .front()
                .and_then(|s| s.repo_digest.as_deref()),
            buf_b
                .samples()
                .front()
                .and_then(|s| s.repo_digest.as_deref()),
            "sample-level salted digests must also remain non-correlatable across sessions"
        );
    }

    #[test]
    fn reuses_broker_and_trace_structured_receipts_without_leaking_sensitive_fields() {
        let mut buf = TelemetryRingBuffer::new(TelemetryRetentionPolicy::default());
        let broker_ev = BrokerEvent {
            schema_version: "1.0.0".to_string(),
            request_id: "req-1".to_string(),
            ts: "1700000100Z".to_string(),
            event_id: "ev-broker-1".to_string(),
            actor: "agent-a".to_string(),
            actor_id: "agent-a".to_string(),
            caller_agent_id: "agent-a".to_string(),
            session_id: None,
            correlation_id: None,
            causation_id: None,
            idempotency_key: None,
            intent_ref: Some("intent:1".to_string()),
            op: "todo.claim".to_string(),
            db_id: "/Users/alice/private-customer/decapod.db".to_string(),
            status: "success".to_string(),
            writer_pid: None,
            writer_start_id: None,
            writer_boot_id: None,
            writer_host_id: None,
            writer_pid_ns: None,
        };
        buf.record_broker_event(
            &broker_ev,
            ReceiptTimingObservation {
                queue_wait_us: Some(20),
                execution_us: Some(140),
                lock_wait_us: Some(30),
            },
            1_700_000_100,
        );

        let trace_ev = TraceEvent {
            trace_id: "tr-1".to_string(),
            ts: "1700000110Z".to_string(),
            actor: "agent-a".to_string(),
            op: "validate.pre_build".to_string(),
            request: serde_json::json!({"prompt": "secret user prompt"}),
            response: serde_json::json!({"token": "ghp_secret_value"}),
        };
        buf.record_trace_event(
            &trace_ev,
            ReceiptTimingObservation {
                queue_wait_us: Some(5),
                execution_us: Some(250),
                lock_wait_us: None,
            },
            ProofFreshness::Fresh,
            DiagnosticOutcome::Succeeded,
            1_700_000_110,
        );

        let report = buf.report(WorkloadSizeMetrics::default(), None);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("private-customer"));
        assert!(!serialized.contains("secret user prompt"));
        assert!(!serialized.contains("ghp_secret_value"));
        assert_eq!(report.outcomes.succeeded, 2);
        assert_eq!(report.lock_contention_events, 1);
    }
}

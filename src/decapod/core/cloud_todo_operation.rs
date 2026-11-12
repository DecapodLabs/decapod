//! Non-secret retry identity for cloud todo creation.
//!
//! This is an operation receipt cache, never a task store. It contains no
//! task text, credentials, or authorization grants. Every retry authenticates
//! again and reconciles through the same service-authorized storage boundary.
use crate::core::error::{CloudAuthStatus, DecapodError};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_RECORDS: usize = 256;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const CACHE_VERSION: u16 = 2;
const COMPLETED_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
pub const INTENT_PREFIX: &str = "decapod.todo.add.v1:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Prepared,
    BlockedBeforeSubmit,
    OutcomeUnknown,
    Succeeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Authentication,
    OnboardingPending,
    Authorization,
    Offline,
    Transport,
    Conflict,
    BackendValidation,
    Cancelled,
    Unavailable,
    Unknown,
}

impl FailureKind {
    pub fn retryable(self) -> bool {
        !matches!(
            self,
            Self::Authorization | Self::Conflict | Self::BackendValidation | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: String,
    pub request_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_fingerprint: Option<String>,
    pub task_id: String,
    pub status: Outcome,
    pub failure_kind: Option<FailureKind>,
    pub updated_at: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOperations {
    version: u16,
    records: Vec<OperationRecord>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredOperationsInput {
    Versioned(StoredOperations),
    Legacy(Vec<OperationRecord>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub schema_version: &'static str,
    pub operation_id: String,
    pub status: Outcome,
    pub attempt_status: Outcome,
    pub failure_kind: FailureKind,
    pub retryable: bool,
    pub next_action: String,
}

impl Diagnostic {
    pub fn new(record: &OperationRecord, status: Outcome, kind: FailureKind) -> Self {
        let recovery = match kind {
            FailureKind::Authentication | FailureKind::OnboardingPending => {
                "Complete cloud onboarding, then "
            }
            FailureKind::Authorization => {
                "Resolve repository authorization with the service, then "
            }
            FailureKind::Offline | FailureKind::Transport | FailureKind::Unavailable => {
                "Restore service connectivity, then "
            }
            FailureKind::Conflict => {
                "Review the operation conflict; do not change this operation's inputs. To reconcile, "
            }
            FailureKind::Cancelled => "If you choose to resume, ",
            FailureKind::BackendValidation => {
                "Review the rejected request. To reconcile the unchanged request, "
            }
            FailureKind::Unknown => "The commit outcome is not known. To reconcile safely, ",
        };
        Self {
            schema_version: "decapod.cloud.todo-operation.v1",
            operation_id: record.operation_id.clone(),
            status: if record.status == Outcome::OutcomeUnknown && status != Outcome::Succeeded {
                Outcome::OutcomeUnknown
            } else {
                status
            },
            attempt_status: status,
            failure_kind: kind,
            retryable: kind.retryable(),
            next_action: format!(
                "{recovery}repeat the original `decapod todo add` arguments with `--operation-id {}`. Do not issue a fresh add to retry this operation.",
                record.operation_id
            ),
        }
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cloud todo operation {}: {:?} ({:?}); {}",
            self.operation_id, self.status, self.failure_kind, self.next_action
        )
    }
}

/// An adapter can say a read-only reconciliation failed before dispatch.
/// Errors after dispatch remain uncertain unless an exact durable receipt is read.
#[derive(Debug, thiserror::Error)]
#[error("cloud todo operation {status:?} ({kind:?})")]
pub struct AdapterFailure {
    pub status: Outcome,
    pub kind: FailureKind,
}

pub fn digest(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

pub fn fingerprint(endpoint: &str, repository: &str, payload: &serde_json::Value) -> String {
    digest(
        serde_json::to_string(&serde_json::json!({
            "version": 1, "endpoint": endpoint, "repository": repository, "request": payload
        }))
        .expect("JSON value serializes")
        .as_bytes(),
    )
}

impl OperationRecord {
    pub fn new(fingerprint: String, supplied: Option<&str>) -> Result<Self, DecapodError> {
        let operation_id = supplied.map(str::to_owned).unwrap_or_else(|| {
            // A nonce is identity, not authority. Mix process-local uniqueness
            // into the ULID for platforms without its /dev/urandom source.
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seed = format!(
                "{}:{}:{:?}:{}",
                crate::core::ulid::new_ulid(),
                std::process::id(),
                std::time::SystemTime::now(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            format!("{}_{}", &digest(seed.as_bytes())[..26], fingerprint)
        });
        let Some((nonce, bound_fingerprint)) = operation_id.split_once('_') else {
            return Err(invalid_token());
        };
        if nonce.len() != 26
            || !nonce
                .bytes()
                .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
            || bound_fingerprint != fingerprint
            || bound_fingerprint.len() != 64
        {
            return Err(invalid_token());
        }
        let task_id = format!("todo_{nonce}");
        Ok(Self {
            operation_id,
            request_fingerprint: fingerprint,
            routing_fingerprint: None,
            task_id,
            status: Outcome::Prepared,
            failure_kind: None,
            updated_at: chrono::Utc::now().timestamp(),
        })
    }

    pub fn intent(&self) -> String {
        format!("{INTENT_PREFIX}{}", self.operation_id)
    }
}

fn invalid_token() -> DecapodError {
    DecapodError::ValidationError("Cloud todo operation token is invalid or belongs to different arguments, repository, or endpoint. Repeat the original arguments and original token; use a fresh add only for an intentionally new task.".into())
}

pub fn failure_kind(error: &DecapodError) -> FailureKind {
    match error {
        DecapodError::CloudAuth(diagnostic) => match diagnostic.status {
            CloudAuthStatus::OnboardingPending => FailureKind::OnboardingPending,
            CloudAuthStatus::Unauthorized
            | CloudAuthStatus::UnauthorizedIdentity
            | CloudAuthStatus::RepositoryDenied
            | CloudAuthStatus::ActionRequired => FailureKind::Authorization,
            CloudAuthStatus::Revoked | CloudAuthStatus::RestartRequired => {
                FailureKind::Authentication
            }
            CloudAuthStatus::Offline => FailureKind::Offline,
            CloudAuthStatus::ProviderUnavailable => FailureKind::Unavailable,
            _ => FailureKind::Authentication,
        },
        DecapodError::DactylError(error) => dactyl_failure_kind(error),
        DecapodError::CloudTodo(diagnostic) => diagnostic.failure_kind,
        DecapodError::Config(_) | DecapodError::ValidationError(_) => {
            FailureKind::BackendValidation
        }
        DecapodError::IoError(_) => FailureKind::Transport,
        _ => FailureKind::Unknown,
    }
}

pub fn anyhow_failure(error: &anyhow::Error) -> (Outcome, FailureKind) {
    if let Some(error) = error.downcast_ref::<AdapterFailure>() {
        return (error.status, error.kind);
    }
    let kind = error
        .downcast_ref::<DecapodError>()
        .map(failure_kind)
        .or_else(|| {
            error
                .downcast_ref::<dactyl_db::DactylError>()
                .map(dactyl_failure_kind)
        })
        .unwrap_or(FailureKind::Unknown);
    (Outcome::OutcomeUnknown, kind)
}

fn dactyl_failure_kind(error: &dactyl_db::DactylError) -> FailureKind {
    use dactyl_db::AdapterErrorKind as K;
    match error.adapter_kind() {
        Some(K::Authentication) => FailureKind::Authentication,
        Some(K::Authorization) => FailureKind::Authorization,
        Some(K::Transport | K::Timeout) => FailureKind::Transport,
        Some(K::Unavailable | K::Busy | K::Locked | K::RateLimited) => FailureKind::Unavailable,
        Some(
            K::Conflict
            | K::Constraint
            | K::VersionConflict
            | K::IdempotencyConflict
            | K::IdempotencyInProgress,
        ) => FailureKind::Conflict,
        Some(K::Cancellation) => FailureKind::Cancelled,
        Some(
            K::Query
            | K::Value
            | K::InvalidOperation
            | K::TransactionAborted
            | K::ReadOnly
            | K::Capability,
        )
        | None => FailureKind::BackendValidation,
        _ => FailureKind::Unknown,
    }
}

/// Bounded machine-local receipt metadata; no protected-repository fallback.
pub struct Journal {
    directory: PathBuf,
}
impl Journal {
    pub fn machine() -> Result<Self, DecapodError> {
        Ok(Self {
            directory: crate::machine_config_dir()?.join("todo-operations"),
        })
    }
    pub fn at(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn with_records<T>(
        &self,
        operation: impl FnOnce(&mut Vec<OperationRecord>) -> Result<T, DecapodError>,
    ) -> Result<T, DecapodError> {
        use crate::core::fs_permissions::{ensure_private_dir, open_private_file};
        ensure_private_dir(&self.directory).map_err(DecapodError::IoError)?;
        let lock = open_private_file(
            &self.directory.join("operations.lock"),
            OpenOptions::new().create(true).read(true).write(true),
        )
        .map_err(DecapodError::IoError)?;
        let started = Instant::now();
        loop {
            match FileExt::try_lock_exclusive(&lock) {
                Ok(()) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock && started.elapsed() < Duration::from_secs(2) => std::thread::sleep(Duration::from_millis(10)),
                Err(_) => return Err(DecapodError::ValidationError("Cloud todo operation metadata is busy or unavailable; retry without changing the operation token.".into())),
            }
        }
        let path = self.directory.join("operations.json");
        let mut records = read_records(&path)?;
        let value = operation(&mut records)?;
        // A versioned envelope makes older array-only clients fail before
        // dispatch instead of discarding an unknown project-binding field.
        let stored = StoredOperations {
            version: CACHE_VERSION,
            records,
        };
        let bytes = serde_json::to_vec_pretty(&stored).map_err(|_| {
            DecapodError::ValidationError("Cannot encode cloud todo operation metadata".into())
        })?;
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(DecapodError::ValidationError("Cloud todo metadata exceeds its encoded byte bound; existing receipts were preserved.".into()));
        }
        crate::core::atomic::write_atomic(&path, &bytes).map_err(DecapodError::IoError)?;
        Ok(value)
    }

    pub fn prepare(&self, record: &OperationRecord) -> Result<OperationRecord, DecapodError> {
        self.with_records(|records| {
            if let Some(existing) = records.iter().find(|existing| existing.operation_id == record.operation_id) {
                if existing.request_fingerprint != record.request_fingerprint || existing.task_id != record.task_id { return Err(invalid_token()); }
                return Ok(existing.clone());
            }
            let cutoff = chrono::Utc::now().timestamp() - COMPLETED_RETENTION_SECONDS;
            records.retain(|existing| existing.status != Outcome::Succeeded || existing.updated_at >= cutoff);
            if records.len() >= MAX_RECORDS
                && let Some(oldest) = records.iter().enumerate()
                    .filter(|(_, record)| record.status == Outcome::Succeeded)
                    .min_by_key(|(_, record)| record.updated_at).map(|(index, _)| index)
            {
                // The durable service event remains authoritative after local
                // successful receipt eviction. Unknown outcomes are never evicted.
                records.remove(oldest);
            }
            if records.len() >= MAX_RECORDS {
                return Err(DecapodError::ValidationError("Cloud todo operation metadata is full. Review `decapod todo operations` and reconcile outstanding operations; unresolved outcomes are never evicted.".into()));
            }
            records.push(record.clone()); Ok(record.clone())
        })
    }

    /// Bind the first possible dispatch to its resolved project. A preflight
    /// operation can survive sign-in and initial selection without changing its
    /// ID, but it cannot be replayed against a different project afterward.
    pub fn bind_route(
        &self,
        record: &OperationRecord,
        routing: &str,
        legacy_routing: &str,
    ) -> Result<OperationRecord, DecapodError> {
        if !hash_valid(routing) || !hash_valid(legacy_routing) {
            return Err(invalid_token());
        }
        self.with_records(|records| {
            let existing = records.iter_mut().find(|r| r.operation_id == record.operation_id)
                .ok_or_else(invalid_token)?;
            if existing.request_fingerprint != record.request_fingerprint || existing.task_id != record.task_id {
                return Err(invalid_token());
            }
            let conflict = match &existing.routing_fingerprint {
                Some(bound) => bound != routing,
                // Before explicit selection existed, writes were necessarily
                // scoped to the canonical Git repository. Never reinterpret
                // an older unknown/successful write as a newly selected one.
                None => matches!(existing.status, Outcome::OutcomeUnknown | Outcome::Succeeded) && routing != legacy_routing,
            };
            if conflict {
                return Err(DecapodError::ValidationError("Cloud todo operation is bound to another resolved project. Select its original project and repeat the original arguments and operation token; no mutation was dispatched.".into()));
            }
            existing.routing_fingerprint = Some(routing.into());
            Ok(existing.clone())
        })
    }

    pub fn finish(
        &self,
        record: &OperationRecord,
        status: Outcome,
        kind: Option<FailureKind>,
    ) -> Result<(), DecapodError> {
        self.with_records(|records| {
            let existing = records.iter_mut().find(|existing| existing.operation_id == record.operation_id)
                .ok_or_else(|| DecapodError::ValidationError("Cloud todo operation metadata is missing; retain the original operation token for reconciliation.".into()))?;
            if existing.request_fingerprint != record.request_fingerprint || existing.task_id != record.task_id || existing.routing_fingerprint != record.routing_fingerprint {
                return Err(invalid_token());
            }
            if existing.status != Outcome::Succeeded {
                if existing.status != Outcome::OutcomeUnknown || status == Outcome::Succeeded {
                    existing.status = status;
                }
                existing.failure_kind = kind; existing.updated_at = chrono::Utc::now().timestamp();
            }
            Ok(())
        })
    }

    pub fn list(&self) -> Result<Vec<OperationRecord>, DecapodError> {
        self.with_records(|records| Ok(records.clone()))
    }
}

fn read_records(path: &Path) -> Result<Vec<OperationRecord>, DecapodError> {
    use std::io::Read;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = crate::core::fs_permissions::open_private_file(path, OpenOptions::new().read(true))
        .map_err(DecapodError::IoError)?;
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(DecapodError::IoError)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(DecapodError::ValidationError(
            "Cloud todo metadata exceeds its bounded size".into(),
        ));
    }
    let stored: StoredOperationsInput = serde_json::from_slice(&bytes).map_err(|_| DecapodError::ValidationError("Invalid cloud todo operation metadata; preserve it for review rather than starting duplicate work.".into()))?;
    let records = match stored {
        StoredOperationsInput::Legacy(records) => records,
        StoredOperationsInput::Versioned(stored) if stored.version == CACHE_VERSION => stored.records,
        StoredOperationsInput::Versioned(_) => return Err(DecapodError::ValidationError("Unsupported cloud todo metadata version; upgrade the CLI before retrying and preserve the existing receipts.".into())),
    };
    if records.len() > MAX_RECORDS
        || records.iter().any(|r| {
            r.routing_fingerprint
                .as_deref()
                .is_some_and(|v| !hash_valid(v))
        })
    {
        return Err(DecapodError::ValidationError(
            "Cloud todo metadata exceeds its bounded record count".into(),
        ));
    }
    Ok(records)
}

fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_todo_operation_tests.rs"]
mod tests;

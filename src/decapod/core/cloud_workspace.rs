//! Hosted workspace custody, container host relay, and authoring admission for
//! Decapod Cloud repositories (`#1886`).
//!
//! When an accepted repository selects `[repo].backend = "cloud"`, workspace
//! provisioning, authoring checks, container launches, and validation never
//! initialize or fall back to `.decapod/data/decapod.db`. Non-secret custody
//! operation state lives in private per-machine custody files, while tokens and
//! DPoP private keys remain exclusively on the host session boundary.

#[cfg(feature = "supabase-cloud")]
use crate::core::backend::{BackendRoute, BackendSelection, StorageContext};
use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
use crate::core::cloud_proof::{
    CloudControlDenialKind, CloudControlError, CustodyBindReceipt, CustodyBindRequest,
    CustodyGetRequest, PROOF_ACCEPTANCE_CONTROL_OP, PROOF_ACCEPTANCE_READ_OP,
    ProofControlTransport, custody_bind, custody_get, new_uuid_v4,
    reconcile_operation_after_unknown,
};
use crate::core::container_runtime;
use crate::core::container_signer::{
    ContainerCloudRelayHandler, ContainerRelayOperation, ContainerRelayRequest,
    ContainerRelayResponse, request_relay_via_socket,
};
#[cfg(feature = "supabase-cloud")]
use crate::core::dactyl_todo::DactylTodoStore;
use crate::core::error::DecapodError;
use crate::core::fs_permissions;
use crate::core::repo_identity::{RepositoryIdentity, resolve_repository_identity};
use crate::core::storage::Task as StorageTask;
use crate::core::todo::Task as TodoTask;
use crate::core::workspace;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

pub const CLOUD_CONTAINER_RELAY_ENV: &str = "DECAPOD_CLOUD_CONTAINER_RELAY";
pub const CLOUD_PROJECT_ID_ENV: &str = "DECAPOD_CLOUD_PROJECT_ID";
pub const CLOUD_TASK_ID_ENV: &str = "DECAPOD_CLOUD_TASK_ID";
pub const CLOUD_TASK_REVISION_ENV: &str = "DECAPOD_CLOUD_TASK_REVISION";
pub const CLOUD_FENCING_GENERATION_ENV: &str = "DECAPOD_CLOUD_FENCING_GENERATION";
pub const CLOUD_WORKSPACE_ID_ENV: &str = "DECAPOD_CLOUD_WORKSPACE_ID";
pub const CLOUD_CUSTODY_ID_ENV: &str = "DECAPOD_CLOUD_CUSTODY_ID";
pub const CLOUD_INVOCATION_ID_ENV: &str = "DECAPOD_CLOUD_INVOCATION_ID";
pub const CLOUD_CUSTODY_DIR_ENV: &str = "DECAPOD_CLOUD_CUSTODY_DIR";

const CUSTODY_RECORD_VERSION: u16 = 1;
#[cfg(feature = "supabase-cloud")]
const DEFAULT_CUSTODY_TTL_SECONDS: i64 = 1800;

/// Authoritative non-secret hosted custody state persisted outside `.decapod/data/`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CloudCustodyRecord {
    pub version: u16,
    pub origin: String,
    pub project_id: String,
    pub task_id: String,
    pub task_title: String,
    pub native_task_version: i64,
    pub workspace_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub actor: String,
    pub custody_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_custody_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_native_task_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fencing_generation: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    #[serde(default)]
    pub container_expected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}

/// Private per-machine storage for non-secret hosted custody records.
/// Never writes inside `.decapod/data/` or touches `decapod.db`.
#[derive(Debug, Clone)]
pub struct CloudCustodyStore {
    directory: PathBuf,
}

impl CloudCustodyStore {
    pub fn machine() -> Result<Self, DecapodError> {
        let dir = if let Ok(explicit) = std::env::var(CLOUD_CUSTODY_DIR_ENV)
            && !explicit.trim().is_empty()
        {
            PathBuf::from(explicit)
        } else {
            crate::machine_config_dir()?.join("cloud-custody")
        };
        Ok(Self { directory: dir })
    }

    pub fn at(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    fn key_for(main_root: &Path, worktree_path: &Path) -> String {
        let canonical_main = main_root
            .canonicalize()
            .unwrap_or_else(|_| main_root.to_path_buf());
        let canonical_wt = worktree_path
            .canonicalize()
            .unwrap_or_else(|_| worktree_path.to_path_buf());
        let mut hasher = Sha256::new();
        hasher.update(canonical_main.as_os_str().as_encoded_bytes());
        hasher.update([0u8]);
        hasher.update(canonical_wt.as_os_str().as_encoded_bytes());
        format!("{:x}", hasher.finalize())[..32].to_string()
    }

    fn record_path(&self, main_root: &Path, worktree_path: &Path) -> PathBuf {
        self.directory
            .join(format!("{}.json", Self::key_for(main_root, worktree_path)))
    }

    pub fn load(
        &self,
        main_root: &Path,
        worktree_path: &Path,
    ) -> Result<Option<CloudCustodyRecord>, DecapodError> {
        let path = self.record_path(main_root, worktree_path);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(DecapodError::IoError)?;
        let record: CloudCustodyRecord = serde_json::from_slice(&bytes).map_err(|e| {
            DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_RECORD_INVALID: unreadable custody state {}: {e}",
                path.display()
            ))
        })?;
        if record.version != CUSTODY_RECORD_VERSION {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_RECORD_INVALID: unsupported custody record version {}",
                record.version
            )));
        }
        Ok(Some(record))
    }

    pub fn save(&self, main_root: &Path, record: &CloudCustodyRecord) -> Result<(), DecapodError> {
        fs_permissions::ensure_private_dir(&self.directory).map_err(DecapodError::IoError)?;
        let path = self.record_path(main_root, &record.worktree_path);
        let bytes = serde_json::to_vec_pretty(record).map_err(|e| {
            DecapodError::ValidationError(format!("failed serializing cloud custody record: {e}"))
        })?;
        fs_permissions::write_private(&path, bytes).map_err(DecapodError::IoError)
    }

    pub fn remove(&self, main_root: &Path, worktree_path: &Path) -> Result<(), DecapodError> {
        let path = self.record_path(main_root, worktree_path);
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(DecapodError::IoError(err)),
        }
    }
}

/// Returns true when the repository configuration (accepted base OR project config)
/// selects Decapod Cloud. Contradictory ambient `DATASTORE` never overrides this.
pub fn is_cloud_project(repo_root: &Path) -> bool {
    if crate::core::db_maintenance::resolve_accepted_project_config(repo_root)
        .ok()
        .is_some_and(|(_main, cfg, _)| cfg.repo.effective_backend().is_cloud())
    {
        return true;
    }
    crate::cli::DecapodProjectConfig::load(repo_root)
        .ok()
        .is_some_and(|cfg| cfg.repo.effective_backend().is_cloud())
}

pub fn workspace_id_for_checkout(main_root: &Path, worktree_path: &Path) -> String {
    format!(
        "ws_{}",
        &CloudCustodyStore::key_for(main_root, worktree_path)[..24]
    )
}

#[derive(Debug, Clone)]
pub struct CloudCustodyBindParams {
    pub origin: String,
    pub project_id: String,
    pub task_id: String,
    pub task_title: String,
    pub native_task_version: i64,
    pub workspace_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub actor: String,
    pub ttl_seconds: i64,
    pub invocation_id: Option<String>,
    pub container_expected: bool,
}

fn validate_reconciled_bind_identity(
    reconciled: &JsonValue,
    expected_project_id: &str,
    expected_task_id: &str,
    expected_workspace_id: &str,
    expected_custody_id: &str,
    expected_operation_id: &str,
) -> Result<(i64, i64), DecapodError> {
    if let Some(op_id) = reconciled.get("operation_id").and_then(JsonValue::as_str)
        && op_id != expected_operation_id
    {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_RECEIPT_INVALID: reconciled operation_id '{op_id}' does not match pending '{expected_operation_id}'"
        )));
    }
    for (field, expected) in [
        ("project_id", expected_project_id),
        ("task_id", expected_task_id),
        ("workspace_id", expected_workspace_id),
        ("custody_id", expected_custody_id),
    ] {
        if let Some(observed) = reconciled.get(field).and_then(JsonValue::as_str)
            && observed != expected
        {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_RECEIPT_INVALID: reconciled {field} '{observed}' does not match expected '{expected}'"
            )));
        }
    }
    let task_revision = reconciled
        .get("task_revision")
        .and_then(JsonValue::as_i64)
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            DecapodError::ValidationError(
                "CLOUD_CUSTODY_RECEIPT_INVALID: reconciled custody operation missing positive task_revision"
                    .into(),
            )
        })?;
    let fencing_generation = reconciled
        .get("fencing_generation")
        .and_then(JsonValue::as_i64)
        .filter(|v| *v > 0)
        .ok_or_else(|| {
            DecapodError::ValidationError(
                "CLOUD_CUSTODY_RECEIPT_INVALID: reconciled custody operation missing positive fencing_generation"
                    .into(),
            )
        })?;
    Ok((task_revision, fencing_generation))
}

/// Bind hosted workspace custody or reconcile an interrupted bind using the
/// persisted non-secret `operation_id` and a live `get_custody` confirmation.
pub fn custody_bind_or_reconcile<T: ProofControlTransport>(
    transport: &T,
    store: &CloudCustodyStore,
    main_root: &Path,
    params: &CloudCustodyBindParams,
) -> Result<(CloudCustodyRecord, CustodyBindReceipt), DecapodError> {
    let canonical_wt = params
        .worktree_path
        .canonicalize()
        .unwrap_or_else(|_| params.worktree_path.clone());
    if let Some(mut existing) = store.load(main_root, &canonical_wt)? {
        if existing.project_id != params.project_id
            || existing.task_id != params.task_id
            || existing.workspace_id != params.workspace_id
            || existing.actor != params.actor
        {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_CHECKOUT_MISMATCH: workspace custody is bound to project='{}' task='{}' workspace='{}' actor='{}', refusing rebind for project='{}' task='{}' workspace='{}' actor='{}'",
                existing.project_id,
                existing.task_id,
                existing.workspace_id,
                existing.actor,
                params.project_id,
                params.task_id,
                params.workspace_id,
                params.actor
            )));
        }

        // Case 1: Interrupted prior submission with a saved pending operation ID.
        if let Some(pending_op_id) = existing.pending_operation_id.clone() {
            let pending_custody_id = existing
                .pending_custody_id
                .clone()
                .unwrap_or_else(|| existing.custody_id.clone());
            let pending_version = existing
                .pending_native_task_version
                .unwrap_or(existing.native_task_version);
            if pending_version != params.native_task_version {
                return Err(DecapodError::ValidationError(format!(
                    "CLOUD_CUSTODY_DRIFT: task '{}' native version drifted from {} to {} before pending custody operation '{}' reconciled",
                    params.task_id, pending_version, params.native_task_version, pending_op_id
                )));
            }
            let reconciled = reconcile_operation_after_unknown(
                transport,
                &params.project_id,
                &pending_op_id,
                None,
            )?;
            let (task_revision, fencing_generation) = validate_reconciled_bind_identity(
                &reconciled,
                &params.project_id,
                &params.task_id,
                &params.workspace_id,
                &pending_custody_id,
                &pending_op_id,
            )?;
            let live = custody_get(
                transport,
                &CustodyGetRequest {
                    project_id: params.project_id.clone(),
                    task_id: params.task_id.clone(),
                    workspace_id: params.workspace_id.clone(),
                    custody_id: pending_custody_id.clone(),
                    task_revision,
                    fencing_generation,
                },
            )?;
            existing.custody_id = pending_custody_id;
            existing.native_task_version = params.native_task_version;
            existing.pending_operation_id = None;
            existing.pending_custody_id = None;
            existing.pending_native_task_version = None;
            existing.task_revision = Some(live.task_revision);
            existing.fencing_generation = Some(live.fencing_generation);
            existing.expires_at = Some(live.expires_at.clone());
            if params.invocation_id.is_some() {
                existing.invocation_id = params.invocation_id.clone();
            }
            existing.container_expected = existing.container_expected || params.container_expected;
            store.save(main_root, &existing)?;
            return Ok((existing, live));
        }

        // Case 2: Already bound checkout; require exact current fence and version.
        if let (Some(task_rev), Some(fencing_gen)) =
            (existing.task_revision, existing.fencing_generation)
        {
            if existing.native_task_version != params.native_task_version
                || task_rev != params.native_task_version
            {
                return Err(DecapodError::ValidationError(format!(
                    "CLOUD_CUSTODY_DRIFT: hosted task '{}' version {} drifted from bound custody revision {}",
                    params.task_id, params.native_task_version, task_rev
                )));
            }
            let live = custody_get(
                transport,
                &CustodyGetRequest {
                    project_id: existing.project_id.clone(),
                    task_id: existing.task_id.clone(),
                    workspace_id: existing.workspace_id.clone(),
                    custody_id: existing.custody_id.clone(),
                    task_revision: task_rev,
                    fencing_generation: fencing_gen,
                },
            )?;
            existing.expires_at = Some(live.expires_at.clone());
            if params.invocation_id.is_some() {
                existing.invocation_id = params.invocation_id.clone();
            }
            existing.container_expected = existing.container_expected || params.container_expected;
            store.save(main_root, &existing)?;
            return Ok((existing, live));
        }
    }

    // Case 3: Brand new bind. Persist the non-secret operation ID before submit.
    let operation_id = new_uuid_v4()?;
    let custody_id = format!("custody_{}", crate::core::ulid::new_ulid().to_lowercase());
    let mut record = CloudCustodyRecord {
        version: CUSTODY_RECORD_VERSION,
        origin: params.origin.clone(),
        project_id: params.project_id.clone(),
        task_id: params.task_id.clone(),
        task_title: params.task_title.clone(),
        native_task_version: params.native_task_version,
        workspace_id: params.workspace_id.clone(),
        worktree_path: canonical_wt,
        branch: params.branch.clone(),
        actor: params.actor.clone(),
        custody_id: custody_id.clone(),
        pending_operation_id: Some(operation_id.clone()),
        pending_custody_id: Some(custody_id.clone()),
        pending_native_task_version: Some(params.native_task_version),
        task_revision: None,
        fencing_generation: None,
        expires_at: None,
        invocation_id: params.invocation_id.clone(),
        container_expected: params.container_expected,
        container_runtime: None,
        container_name: None,
        container_id: None,
    };
    store.save(main_root, &record)?;

    let bind_req = CustodyBindRequest {
        project_id: params.project_id.clone(),
        operation_id: operation_id.clone(),
        task_id: params.task_id.clone(),
        workspace_id: params.workspace_id.clone(),
        custody_id: custody_id.clone(),
        native_task_version: params.native_task_version,
        ttl_seconds: params.ttl_seconds,
    };
    let receipt = match custody_bind(transport, &bind_req) {
        Ok(receipt) => receipt,
        Err(err) if err.to_string().contains("CLOUD_OUTCOME_UNKNOWN") => {
            // Preserve `pending_operation_id` if reconciliation also cannot confirm
            // yet so a subsequent invocation recovers via `get_operation`.
            let reconciled = reconcile_operation_after_unknown(
                transport,
                &params.project_id,
                &operation_id,
                None,
            )?;
            let (task_revision, fencing_generation) = validate_reconciled_bind_identity(
                &reconciled,
                &params.project_id,
                &params.task_id,
                &params.workspace_id,
                &custody_id,
                &operation_id,
            )?;
            custody_get(
                transport,
                &CustodyGetRequest {
                    project_id: params.project_id.clone(),
                    task_id: params.task_id.clone(),
                    workspace_id: params.workspace_id.clone(),
                    custody_id: custody_id.clone(),
                    task_revision,
                    fencing_generation,
                },
            )?
        }
        Err(err) => {
            let _ = store.remove(main_root, &record.worktree_path);
            return Err(err);
        }
    };

    let live = custody_get(
        transport,
        &CustodyGetRequest {
            project_id: params.project_id.clone(),
            task_id: params.task_id.clone(),
            workspace_id: params.workspace_id.clone(),
            custody_id,
            task_revision: receipt.task_revision,
            fencing_generation: receipt.fencing_generation,
        },
    )?;

    record.pending_operation_id = None;
    record.pending_custody_id = None;
    record.pending_native_task_version = None;
    record.task_revision = Some(live.task_revision);
    record.fencing_generation = Some(live.fencing_generation);
    record.expires_at = Some(live.expires_at.clone());
    store.save(main_root, &record)?;
    Ok((record, live))
}

/// Returns true if execution is running inside a Decapod Cloud container using
/// the invocation-owned host relay socket.
pub fn is_container_relay_active() -> bool {
    std::env::var(CLOUD_CONTAINER_RELAY_ENV)
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE"))
        && std::env::var_os("DECAPOD_CONTAINER_SIGNER_SOCKET").is_some()
}

/// Non-secret ProofControlTransport that forwards typed control operations over
/// the host relay Unix domain socket (`DECAPOD_CONTAINER_SIGNER_SOCKET`).
#[derive(Debug, Clone)]
pub struct RelayProofControlTransport {
    socket_path: PathBuf,
    invocation_id: String,
    workspace_id: String,
    project_id: String,
    task_id: String,
    custody_id: String,
}

impl RelayProofControlTransport {
    pub fn new(
        socket_path: PathBuf,
        invocation_id: String,
        workspace_id: String,
        project_id: String,
        task_id: String,
        custody_id: String,
    ) -> Self {
        Self {
            socket_path,
            invocation_id,
            workspace_id,
            project_id,
            task_id,
            custody_id,
        }
    }

    pub fn from_env() -> Result<Self, DecapodError> {
        let required = |name: &str| -> Result<String, DecapodError> {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .ok_or_else(|| {
                    DecapodError::ValidationError(format!(
                        "CLOUD_CONTAINER_HANDOFF_MISSING: required container relay handle {name} is unset"
                    ))
                })
        };
        Ok(Self {
            socket_path: PathBuf::from(required("DECAPOD_CONTAINER_SIGNER_SOCKET")?),
            invocation_id: required(CLOUD_INVOCATION_ID_ENV)?,
            workspace_id: required(CLOUD_WORKSPACE_ID_ENV)?,
            project_id: required(CLOUD_PROJECT_ID_ENV)?,
            task_id: required(CLOUD_TASK_ID_ENV)?,
            custody_id: required(CLOUD_CUSTODY_ID_ENV)?,
        })
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    fn call(
        &self,
        operation: ContainerRelayOperation,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        let req = ContainerRelayRequest {
            kind: "cloud_relay".to_string(),
            invocation_id: self.invocation_id.clone(),
            workspace_id: self.workspace_id.clone(),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            custody_id: self.custody_id.clone(),
            operation,
            rpc_op: op_name.to_string(),
            payload: payload.clone(),
        };
        let resp =
            request_relay_via_socket(&self.socket_path, &req).map_err(|err| CloudControlError {
                kind: CloudControlDenialKind::Unavailable,
                code: "container_relay_transport_failed".to_string(),
                detail: err.to_string(),
                http_status: None,
            })?;
        if resp.ok {
            return Ok(resp.result.unwrap_or_else(|| json!({})));
        }
        let code = resp
            .code
            .unwrap_or_else(|| "container_relay_denied".to_string());
        let detail = resp
            .message
            .unwrap_or_else(|| "container relay rejected operation".to_string());
        let kind = match code.as_str() {
            "CLOUD_CONTAINER_ADMISSION_DENIED"
            | "CONTAINER_INVOCATION_MISMATCH"
            | "CLOUD_CUSTODY_PROJECT_MISMATCH"
            | "CLOUD_CUSTODY_CHECKOUT_MISMATCH" => CloudControlDenialKind::EntitlementForbidden,
            "outcome_unknown" | "CLOUD_OUTCOME_UNKNOWN" => CloudControlDenialKind::OutcomeUnknown,
            "CLOUD_CONTROL_UNAVAILABLE" | "container_relay_transport_failed" => {
                CloudControlDenialKind::Unavailable
            }
            _ => CloudControlDenialKind::InvalidRequest,
        };
        Err(CloudControlError {
            kind,
            code,
            detail,
            http_status: None,
        })
    }
}

impl ProofControlTransport for RelayProofControlTransport {
    fn write_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.call(ContainerRelayOperation::ProofControlWrite, op_name, payload)
    }

    fn read_control(
        &self,
        op_name: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, CloudControlError> {
        self.call(ContainerRelayOperation::ProofControlRead, op_name, payload)
    }
}

pub trait ContainerAdmissionVerifier: Send + Sync {
    fn verify_admission(
        &self,
        worktree_path: &Path,
        invocation_id: &str,
    ) -> Result<String, DecapodError>;
}

#[derive(Debug, Clone)]
pub struct RuntimeContainerAdmissionVerifier {
    pub main_root: PathBuf,
    pub runtime: String,
    pub container_name: String,
    pub expected_container_id: Option<String>,
}

impl ContainerAdmissionVerifier for RuntimeContainerAdmissionVerifier {
    fn verify_admission(
        &self,
        worktree_path: &Path,
        invocation_id: &str,
    ) -> Result<String, DecapodError> {
        verify_git_worktree_registered(&self.main_root, worktree_path)?;
        container_runtime::verify_running_container_for_invocation(
            &self.runtime,
            &self.container_name,
            worktree_path,
            invocation_id,
            self.expected_container_id.as_deref(),
        )
    }
}

pub trait CloudRelayExecutor: Send + Sync {
    fn execute(
        &self,
        operation: ContainerRelayOperation,
        rpc_op: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, DecapodError>;
}

#[derive(Debug, Clone)]
pub struct LiveHostCloudRelayExecutor {
    pub project_root: PathBuf,
    pub identity: RepositoryIdentity,
}

#[cfg(feature = "supabase-cloud")]
impl CloudRelayExecutor for LiveHostCloudRelayExecutor {
    fn execute(
        &self,
        operation: ContainerRelayOperation,
        rpc_op: &str,
        payload: &JsonValue,
    ) -> Result<JsonValue, DecapodError> {
        use dactyl_db::{OperationKind, ResourceBudget};

        let (connector, _routed, bearer) =
            crate::core::cloud_client::connect_control(&self.project_root, &self.identity)?;
        let kind =
            match operation {
                ContainerRelayOperation::DactylQuery
                | ContainerRelayOperation::ProofControlRead => OperationKind::Read,
                ContainerRelayOperation::DactylBatch
                | ContainerRelayOperation::ProofControlWrite => OperationKind::Write,
            };
        connector
            .control_request(
                rpc_op,
                payload,
                Some(&bearer),
                kind,
                ResourceBudget::default(),
            )
            .map_err(crate::core::cloud_client::transport_error)
    }
}

#[cfg(not(feature = "supabase-cloud"))]
impl CloudRelayExecutor for LiveHostCloudRelayExecutor {
    fn execute(
        &self,
        _operation: ContainerRelayOperation,
        _rpc_op: &str,
        _payload: &JsonValue,
    ) -> Result<JsonValue, DecapodError> {
        Err(DecapodError::NotImplemented(
            "CLOUD_CAPABILITY_DISABLED: hosted relay requires supabase-cloud feature".into(),
        ))
    }
}

/// Host-side invocation-bound relay handler that verifies container admission
/// and forwards only scoped typed Dactyl / Proof operations using host credentials.
pub struct HostCloudRelayHandler {
    pub project_id: String,
    pub workspace_id: String,
    pub worktree_path: PathBuf,
    pub task_id: String,
    pub custody_id: String,
    pub invocation_id: String,
    pub admission: Option<Arc<dyn ContainerAdmissionVerifier>>,
    pub executor: Arc<dyn CloudRelayExecutor>,
}

impl HostCloudRelayHandler {
    fn reject(code: &str, message: impl Into<String>) -> ContainerRelayResponse {
        ContainerRelayResponse {
            ok: false,
            code: Some(code.to_string()),
            message: Some(message.into()),
            result: None,
        }
    }

    fn validate_request_scope(
        &self,
        request: &ContainerRelayRequest,
    ) -> Result<(), ContainerRelayResponse> {
        if request.invocation_id != self.invocation_id {
            return Err(Self::reject(
                "CONTAINER_INVOCATION_MISMATCH",
                format!(
                    "container relay invocation '{}' does not match bound invocation '{}'",
                    request.invocation_id, self.invocation_id
                ),
            ));
        }
        if request.project_id != self.project_id {
            return Err(Self::reject(
                "CLOUD_CUSTODY_PROJECT_MISMATCH",
                format!(
                    "container relay project '{}' does not match bound project '{}'",
                    request.project_id, self.project_id
                ),
            ));
        }
        if request.workspace_id != self.workspace_id
            || request.task_id != self.task_id
            || request.custody_id != self.custody_id
        {
            return Err(Self::reject(
                "CLOUD_CUSTODY_CHECKOUT_MISMATCH",
                "container relay task/workspace/custody binding does not match admitted workspace"
                    .to_string(),
            ));
        }

        let allowed = match request.operation {
            ContainerRelayOperation::DactylQuery => request.rpc_op == "dactyl_query_v1",
            ContainerRelayOperation::DactylBatch => request.rpc_op == "dactyl_batch_v1",
            ContainerRelayOperation::ProofControlRead => {
                request.rpc_op == PROOF_ACCEPTANCE_READ_OP
                    || request.rpc_op == "propodus_repository_access_v1"
            }
            ContainerRelayOperation::ProofControlWrite => {
                request.rpc_op == PROOF_ACCEPTANCE_CONTROL_OP
            }
        };
        if !allowed {
            return Err(Self::reject(
                "CLOUD_CONTAINER_ADMISSION_DENIED",
                format!(
                    "container relay operation {:?} rpc '{}' is outside allowlist",
                    request.operation, request.rpc_op
                ),
            ));
        }

        if let Some(pid) = request
            .payload
            .get("project_id")
            .and_then(JsonValue::as_str)
            && pid != self.project_id
        {
            return Err(Self::reject(
                "CLOUD_CUSTODY_PROJECT_MISMATCH",
                format!(
                    "payload project_id '{pid}' differs from bound project '{}'",
                    self.project_id
                ),
            ));
        }
        if let Some(tid) = request.payload.get("task_id").and_then(JsonValue::as_str)
            && tid != self.task_id
        {
            return Err(Self::reject(
                "CLOUD_CUSTODY_CHECKOUT_MISMATCH",
                format!(
                    "payload task_id '{tid}' differs from bound task '{}'",
                    self.task_id
                ),
            ));
        }
        if let Some(wid) = request
            .payload
            .get("workspace_id")
            .and_then(JsonValue::as_str)
            && wid != self.workspace_id
        {
            return Err(Self::reject(
                "CLOUD_CUSTODY_CHECKOUT_MISMATCH",
                format!(
                    "payload workspace_id '{wid}' differs from bound workspace '{}'",
                    self.workspace_id
                ),
            ));
        }
        if let Some(cid) = request
            .payload
            .get("custody_id")
            .and_then(JsonValue::as_str)
            && cid != self.custody_id
        {
            return Err(Self::reject(
                "CLOUD_CUSTODY_CHECKOUT_MISMATCH",
                format!(
                    "payload custody_id '{cid}' differs from bound custody '{}'",
                    self.custody_id
                ),
            ));
        }
        Ok(())
    }
}

impl ContainerCloudRelayHandler for HostCloudRelayHandler {
    fn handle_relay(&self, request: &ContainerRelayRequest) -> ContainerRelayResponse {
        if let Err(denied) = self.validate_request_scope(request) {
            return denied;
        }
        if let Some(verifier) = &self.admission
            && let Err(err) = verifier.verify_admission(&self.worktree_path, &self.invocation_id)
        {
            return Self::reject("CLOUD_CONTAINER_ADMISSION_DENIED", err.to_string());
        }
        match self
            .executor
            .execute(request.operation.clone(), &request.rpc_op, &request.payload)
        {
            Ok(result) => ContainerRelayResponse {
                ok: true,
                code: None,
                message: None,
                result: Some(result),
            },
            Err(err) => {
                let code = if err.to_string().contains("CLOUD_OUTCOME_UNKNOWN") {
                    "CLOUD_OUTCOME_UNKNOWN"
                } else {
                    "CLOUD_CONTROL_UNAVAILABLE"
                };
                Self::reject(code, err.to_string())
            }
        }
    }
}

pub fn verify_git_worktree_registered(
    main_root: &Path,
    worktree_path: &Path,
) -> Result<(), DecapodError> {
    let canonical_target = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());
    let output = Command::new("git")
        .arg("-C")
        .arg(main_root)
        .args(["worktree", "list", "--porcelain"])
        .bounded_output(CONTROL_TIMEOUT)
        .map_err(DecapodError::IoError)?;
    let registered = output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .any(|path| Path::new(path).canonicalize().ok().as_ref() == Some(&canonical_target));
    if !registered {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_CHECKOUT_MISMATCH: checkout '{}' is not a registered Git worktree of '{}'",
            worktree_path.display(),
            main_root.display()
        )));
    }
    Ok(())
}

/// Non-secret environment bindings passed into a Cloud container launch.
/// Never carries bearer tokens, refresh tokens, device keys, or `DECAPOD_LOCAL_ONLY`.
pub fn cloud_container_env_handoff(
    record: &CloudCustodyRecord,
    relay_socket: Option<&Path>,
) -> Vec<(String, String)> {
    let mut vars = vec![
        ("DECAPOD_CONTAINER".to_string(), "1".to_string()),
        (CLOUD_CONTAINER_RELAY_ENV.to_string(), "1".to_string()),
        (CLOUD_PROJECT_ID_ENV.to_string(), record.project_id.clone()),
        (CLOUD_TASK_ID_ENV.to_string(), record.task_id.clone()),
        (
            CLOUD_WORKSPACE_ID_ENV.to_string(),
            record.workspace_id.clone(),
        ),
        (CLOUD_CUSTODY_ID_ENV.to_string(), record.custody_id.clone()),
        ("DATASTORE".to_string(), "supabase".to_string()),
    ];
    if let Some(rev) = record.task_revision {
        vars.push((CLOUD_TASK_REVISION_ENV.to_string(), rev.to_string()));
    }
    if let Some(fencing_gen) = record.fencing_generation {
        vars.push((
            CLOUD_FENCING_GENERATION_ENV.to_string(),
            fencing_gen.to_string(),
        ));
    }
    if let Some(inv) = record.invocation_id.as_deref() {
        vars.push((CLOUD_INVOCATION_ID_ENV.to_string(), inv.to_string()));
    }
    if let Some(sock) = relay_socket {
        vars.push((
            "DECAPOD_CONTAINER_SIGNER_SOCKET".to_string(),
            sock.to_string_lossy().into_owned(),
        ));
    }
    vars
}

/// Build the managed container shell command for an accepted-Cloud workspace.
/// Carries only the isolated checkout + non-secret handoff + host relay socket
/// with zero local `.decapod/data` mount or `decapod.db` backup snapshot.
pub fn cloud_container_workspace_launch_command(
    main_repo: &Path,
    worktree_path: &Path,
    runtime: &str,
    image: &str,
    invocation: &str,
    record: &CloudCustodyRecord,
    relay_socket: Option<&Path>,
) -> Result<String, DecapodError> {
    let workspace_str = worktree_path
        .to_str()
        .filter(|path| Path::new(path).is_absolute() && workspace_path_safe(path))
        .ok_or_else(|| {
            DecapodError::ValidationError(format!(
                "CONTAINER_STORE_PATH_UNSUPPORTED: cloud container workspace path '{}' must be safe UTF-8",
                worktree_path.display()
            ))
        })?;
    let runtime_safe = runtime
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'));
    if !runtime_safe || runtime.is_empty() {
        return Err(DecapodError::ValidationError(
            "CONTAINER_RUNTIME_UNSUPPORTED: container runtime name is invalid".into(),
        ));
    }
    let main_git = main_repo.join(".git");
    let main_git_arg = main_git
        .to_str()
        .filter(|path| workspace_path_safe(path))
        .map(|git_path| format!("-v {}", shell_quote(&format!("{git_path}:{git_path}"))))
        .unwrap_or_default();
    let relay_mount = relay_socket
        .and_then(|s| s.to_str())
        .filter(|s| workspace_path_safe(s))
        .map(|s| format!("-v {}", shell_quote(&format!("{s}:{s}"))))
        .unwrap_or_default();
    let mut env_flags = String::new();
    for (k, v) in cloud_container_env_handoff(record, relay_socket) {
        env_flags.push_str(&format!(" -e {}", shell_quote(&format!("{k}={v}"))));
    }
    let ws_mount = shell_quote(&format!("{workspace_str}:{workspace_str}"));
    let ws_label = shell_quote(&format!("org.decapod.workspace.path={workspace_str}"));
    let inv_label = shell_quote(&format!("org.decapod.invocation={invocation}"));
    let task_label = shell_quote(&format!("org.decapod.cloud.task_id={}", record.task_id));
    let workdir = shell_quote(workspace_str);
    let image_q = shell_quote(image);
    Ok(format!(
        "( cd {workdir} && {runtime} run --rm -it --label org.decapod.managed=workspace --label {ws_label} --label {inv_label} --label {task_label}{env_flags} -v {ws_mount} {main_git_arg} {relay_mount} -w {workdir} {image_q} bash )"
    ))
}

fn workspace_path_safe(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path).is_absolute()
        && !path.chars().any(|c| matches!(c, '\0' | '\r' | '\n'))
}

fn shell_quote(value: &str) -> String {
    let escaped = value.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

/// Convert a Dactyl-backed hosted task to the shape used to seed `.decapod/governance.json`.
pub fn storage_task_to_todo_task(task: &StorageTask) -> TodoTask {
    TodoTask {
        id: task.id.clone(),
        hash: task.hash.clone(),
        title: task.title.clone(),
        description: task.description.clone().unwrap_or_default(),
        tags: task.tags.join(","),
        owner: task.assignee.clone().unwrap_or_default(),
        due: None,
        r#ref: String::new(),
        status: task.status.clone(),
        created_at: crate::core::time::format_rfc3339_nanos(task.created_at).unwrap_or_default(),
        updated_at: crate::core::time::format_rfc3339_nanos(task.updated_at).unwrap_or_default(),
        completed_at: None,
        closed_at: None,
        dir_path: task.dir_path.clone(),
        scope: task.scope.clone(),
        parent_task_id: None,
        priority: task.priority.clone(),
        depends_on: String::new(),
        blocks: String::new(),
        category: task.category.clone(),
        component: String::new(),
        assigned_to: task.assignee.clone().unwrap_or_default(),
        assigned_at: None,
        owners: Vec::new(),
        comments: Vec::new(),
        one_shot: 0,
        revision: i64::from(task.version).max(1),
    }
}

#[cfg(feature = "supabase-cloud")]
fn run_with_hosted_todo_store<T, F, Fut>(
    project_root: &Path,
    identity: &RepositoryIdentity,
    op_label: &str,
    f: F,
) -> Result<T, DecapodError>
where
    F: FnOnce(DactylTodoStore) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let (_connector, routed, bearer) =
        crate::core::cloud_client::connect_control(project_root, identity)?;
    let origin = crate::core::cloud_client::selected_origin()?;
    let selection = BackendSelection::from_project(project_root)?;
    let route = match selection.route(Some(&origin))? {
        route @ BackendRoute::Cloud { .. } => route,
        BackendRoute::Local { .. } => BackendRoute::Cloud {
            repository: routed.clone(),
            uri: origin,
        },
    };
    let context = StorageContext::from_route(route, Some(&bearer))?
        .with_cloud_datastore(crate::core::backend::CloudDatastore::Managed)?;
    let store = DactylTodoStore::new(context, &routed.canonical_name);
    let rt = tokio::runtime::Runtime::new().map_err(|e| {
        DecapodError::ValidationError(format!("failed starting tokio runtime for cloud read: {e}"))
    })?;
    rt.block_on(f(store))
        .map_err(|e| DecapodError::ValidationError(format!("{op_label}: {e}")))
}

/// Fetch a single hosted task from Dactyl without creating or falling back to local SQLite.
#[cfg(feature = "supabase-cloud")]
pub fn fetch_hosted_task_blocking(
    project_root: &Path,
    identity: &RepositoryIdentity,
    task_id: &str,
) -> Result<Option<StorageTask>, DecapodError> {
    use crate::core::storage::TodoStore;
    let id = task_id.to_string();
    run_with_hosted_todo_store(
        project_root,
        identity,
        "CLOUD_TASK_READ_FAILED",
        move |store| async move { store.get_task(&id).await },
    )
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn fetch_hosted_task_blocking(
    _project_root: &Path,
    _identity: &RepositoryIdentity,
    _task_id: &str,
) -> Result<Option<StorageTask>, DecapodError> {
    Err(DecapodError::NotImplemented(
        "CLOUD_CAPABILITY_DISABLED: hosted task reads require supabase-cloud".into(),
    ))
}

#[cfg(feature = "supabase-cloud")]
pub fn list_hosted_open_tasks_blocking(
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<Vec<StorageTask>, DecapodError> {
    use crate::core::storage::TodoStore;
    run_with_hosted_todo_store(
        project_root,
        identity,
        "CLOUD_TASK_LIST_FAILED",
        |store| async move { store.list_tasks().await },
    )
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn list_hosted_open_tasks_blocking(
    _project_root: &Path,
    _identity: &RepositoryIdentity,
) -> Result<Vec<StorageTask>, DecapodError> {
    Err(DecapodError::NotImplemented(
        "CLOUD_CAPABILITY_DISABLED: hosted task reads require supabase-cloud".into(),
    ))
}

/// Bind or verify hosted custody for an authorized Cloud workspace checkout.
/// Returns the verified custody record + confirmed hosted tasks for governance seeding.
pub fn admit_cloud_checkout(
    main_root: &Path,
    worktree_path: &Path,
    branch: &str,
    actor: &str,
    invocation_id: Option<&str>,
    container_expected: bool,
) -> Result<(CloudCustodyRecord, Vec<TodoTask>), DecapodError> {
    if crate::core::todo::is_missing_or_unknown_actor(actor) {
        return Err(crate::core::todo::authoring_actor_required_error());
    }
    let identity = resolve_repository_identity(main_root)?;
    let origin = crate::core::cloud_client::selected_origin()?;
    let all_tasks = list_hosted_open_tasks_blocking(main_root, &identity)?;

    let branch_task_ids = workspace::extract_task_ids_from_branch(branch);
    let mut matching_tasks: Vec<StorageTask> = all_tasks
        .iter()
        .filter(|t| t.status == "in_progress" && t.assignee.as_deref() == Some(actor))
        .cloned()
        .collect();

    if !branch_task_ids.is_empty() {
        let mut branch_owned = Vec::new();
        for task in &all_tasks {
            let matches_branch = branch_task_ids
                .iter()
                .any(|tid| tid == &task.id || tid == &task.hash);
            if !matches_branch {
                continue;
            }
            if task.status == "in_progress" && task.assignee.as_deref() != Some(actor) {
                return Err(DecapodError::ValidationError(format!(
                    "WORKSPACE_TODO_CLAIM_CONFLICT: hosted task '{}' belongs to '{}' rather than '{}'",
                    task.id,
                    task.assignee.as_deref().unwrap_or("unassigned"),
                    actor
                )));
            }
            if task.status != "in_progress" {
                return Err(DecapodError::ValidationError(format!(
                    "TODO_CUSTODY_REQUIRED: claim hosted task '{}' via `decapod todo claim --id {}` before provisioning workspace",
                    task.id, task.id
                )));
            }
            branch_owned.push(task.clone());
        }
        if !branch_owned.is_empty() {
            matching_tasks = branch_owned;
        }
    }

    let primary_task = matching_tasks.first().cloned().ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "TODO_CUSTODY_REQUIRED: no actively claimed hosted task found for actor '{actor}' on repository '{}'. Claim a task via `decapod todo claim --id <task-id>` first.",
            identity.canonical_name
        ))
    })?;

    #[cfg(feature = "supabase-cloud")]
    let (transport, routed) =
        crate::core::cloud_proof::DactylProofControlTransport::from_context(main_root, &identity)?;
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (
            worktree_path,
            invocation_id,
            container_expected,
            origin,
            primary_task,
        );
        return Err(DecapodError::NotImplemented(
            "CLOUD_CAPABILITY_DISABLED: hosted custody requires supabase-cloud".into(),
        ));
    }

    #[cfg(feature = "supabase-cloud")]
    {
        if routed.canonical_name != identity.canonical_name {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_PROJECT_MISMATCH: repository identity '{}' resolved to incompatible cloud project '{}'",
                identity.canonical_name, routed.canonical_name
            )));
        }
        let store = CloudCustodyStore::machine()?;
        let workspace_id = workspace_id_for_checkout(main_root, worktree_path);
        let params = CloudCustodyBindParams {
            origin,
            project_id: routed.canonical_name,
            task_id: primary_task.id.clone(),
            task_title: primary_task.title.clone(),
            native_task_version: i64::from(primary_task.version).max(1),
            workspace_id,
            worktree_path: worktree_path.to_path_buf(),
            branch: branch.to_string(),
            actor: actor.to_string(),
            ttl_seconds: DEFAULT_CUSTODY_TTL_SECONDS,
            invocation_id: invocation_id.map(ToOwned::to_owned),
            container_expected,
        };
        let (record, _receipt) = custody_bind_or_reconcile(&transport, &store, main_root, &params)?;
        let todo_tasks: Vec<TodoTask> = matching_tasks
            .iter()
            .map(storage_task_to_todo_task)
            .collect();
        Ok((record, todo_tasks))
    }
}

/// Check live hosted custody for an isolated Cloud checkout without any SQLite fallback.
pub fn check_cloud_authoring(
    main_root: &Path,
    checkout: &Path,
    actor: &str,
    strict_require_bound: bool,
) -> Result<(), DecapodError> {
    if crate::core::todo::is_missing_or_unknown_actor(actor) {
        return Err(crate::core::todo::authoring_actor_required_error());
    }
    let Some(toplevel) = workspace::git_toplevel(checkout)? else {
        return Err(DecapodError::ValidationError(
            "AUTHORING_WORKTREE_REQUIRED: checkout is not a Git repository".into(),
        ));
    };
    if !workspace::is_canonical_decapod_workspace_path(&toplevel) {
        if strict_require_bound {
            return Err(DecapodError::ValidationError(
                "AUTHORING_WORKTREE_REQUIRED: protected root checkout is not an isolated Cloud authoring workspace".into(),
            ));
        }
        return Ok(());
    }

    verify_git_worktree_registered(main_root, &toplevel)?;
    let identity = resolve_repository_identity(checkout)?;
    let branch = workspace::get_current_branch(&toplevel)?;

    if is_container_relay_active() {
        let relay = RelayProofControlTransport::from_env()?;
        if relay.project_id != identity.canonical_name {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_PROJECT_MISMATCH: container handoff project '{}' does not match checkout project '{}'",
                relay.project_id, identity.canonical_name
            )));
        }
        let expected_rev = std::env::var(CLOUD_TASK_REVISION_ENV)
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1);
        let expected_gen = std::env::var(CLOUD_FENCING_GENERATION_ENV)
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(1);
        let _live = custody_get(
            &relay,
            &CustodyGetRequest {
                project_id: relay.project_id.clone(),
                task_id: relay.task_id.clone(),
                workspace_id: relay.workspace_id.clone(),
                custody_id: relay.custody_id.clone(),
                task_revision: expected_rev,
                fencing_generation: expected_gen,
            },
        )?;
        return Ok(());
    }

    let store = CloudCustodyStore::machine()?;
    let record = store
        .load(main_root, &toplevel)?
        .ok_or_else(|| {
            DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_UNBOUND: checkout '{}' has no bound hosted custody record; run `decapod workspace ensure`",
                toplevel.display()
            ))
        })?;
    if record.project_id != identity.canonical_name {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_PROJECT_MISMATCH: checkout custody belongs to '{}' rather than '{}'",
            record.project_id, identity.canonical_name
        )));
    }
    let canonical_wt = toplevel.canonicalize().unwrap_or(toplevel.clone());
    if record.worktree_path != canonical_wt {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_CHECKOUT_MISMATCH: custody is bound to '{}' rather than '{}'",
            record.worktree_path.display(),
            canonical_wt.display()
        )));
    }
    if record.branch != branch {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_CHECKOUT_MISMATCH: custody is bound to branch '{}' rather than '{}'",
            record.branch, branch
        )));
    }
    if record.actor != actor {
        return Err(DecapodError::ValidationError(format!(
            "CLOUD_CUSTODY_ACTOR_MISMATCH: custody is owned by '{}' rather than '{}'",
            record.actor, actor
        )));
    }
    if record.container_expected
        && !Path::new("/.dockerenv").exists()
        && !Path::new("/run/.containerenv").exists()
        && std::env::var("DECAPOD_CONTAINER").ok().as_deref() != Some("1")
    {
        if let (Some(rt), Some(cname), Some(inv)) = (
            record.container_runtime.as_deref(),
            record.container_name.as_deref(),
            record.invocation_id.as_deref(),
        ) {
            container_runtime::verify_running_container_for_invocation(
                rt,
                cname,
                &canonical_wt,
                inv,
                record.container_id.as_deref(),
            )?;
        } else if strict_require_bound {
            return Err(DecapodError::ValidationError(
                "CLOUD_CONTAINER_ADMISSION_DENIED: workspace requires isolated container custody; host-only execution without verified container admission is refused".into(),
            ));
        }
    }

    let (Some(task_rev), Some(fencing_gen)) = (record.task_revision, record.fencing_generation)
    else {
        return Err(DecapodError::ValidationError(
            "CLOUD_CUSTODY_UNCONFIRMED: custody bind outcome is still pending; rerun `decapod workspace ensure` to reconcile via `get_operation`".into(),
        ));
    };

    #[cfg(feature = "supabase-cloud")]
    {
        let (transport, routed) =
            crate::core::cloud_proof::DactylProofControlTransport::from_context(
                main_root, &identity,
            )?;
        if routed.canonical_name != record.project_id {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_PROJECT_MISMATCH: routed project '{}' disagrees with custody project '{}'",
                routed.canonical_name, record.project_id
            )));
        }
        let _live = custody_get(
            &transport,
            &CustodyGetRequest {
                project_id: record.project_id.clone(),
                task_id: record.task_id.clone(),
                workspace_id: record.workspace_id.clone(),
                custody_id: record.custody_id.clone(),
                task_revision: task_rev,
                fencing_generation: fencing_gen,
            },
        )?;
        let hosted = fetch_hosted_task_blocking(main_root, &identity, &record.task_id)?
            .ok_or_else(|| {
                DecapodError::ValidationError(format!(
                    "CLOUD_CUSTODY_TASK_MISSING: hosted task '{}' no longer exists",
                    record.task_id
                ))
            })?;
        if i64::from(hosted.version) != task_rev
            || hosted.status != "in_progress"
            || hosted.assignee.as_deref() != Some(actor)
        {
            return Err(DecapodError::ValidationError(format!(
                "CLOUD_CUSTODY_DRIFT: hosted task '{}' (version={}, status={}, assignee={:?}) no longer matches bound custody (revision={}, actor={})",
                record.task_id, hosted.version, hosted.status, hosted.assignee, task_rev, actor
            )));
        }
        Ok(())
    }
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (task_rev, fencing_gen);
        Err(DecapodError::NotImplemented(
            "CLOUD_CAPABILITY_DISABLED: hosted custody verification requires supabase-cloud".into(),
        ))
    }
}

/// Provide non-SQLite task metadata for seeding `.decapod/governance.json` on
/// an accepted-Cloud worktree without reading or creating `decapod.db`.
pub fn hosted_tasks_for_governance_seed(
    main_root: &Path,
    checkout: &Path,
    agent_id: &str,
) -> Result<Vec<TodoTask>, DecapodError> {
    let store = CloudCustodyStore::machine()?;
    let canonical_wt = checkout
        .canonicalize()
        .unwrap_or_else(|_| checkout.to_path_buf());
    if let Some(record) = store.load(main_root, &canonical_wt)? {
        return Ok(vec![TodoTask {
            id: record.task_id.clone(),
            hash: record.task_id.clone(),
            title: record.task_title.clone(),
            description: String::new(),
            tags: String::new(),
            owner: record.actor.clone(),
            due: None,
            r#ref: String::new(),
            status: "in_progress".to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            completed_at: None,
            closed_at: None,
            dir_path: canonical_wt.to_string_lossy().into_owned(),
            scope: "workspace".to_string(),
            parent_task_id: None,
            priority: "medium".to_string(),
            depends_on: String::new(),
            blocks: String::new(),
            category: String::new(),
            component: String::new(),
            assigned_to: record.actor,
            assigned_at: None,
            owners: Vec::new(),
            comments: Vec::new(),
            one_shot: 0,
            revision: record
                .task_revision
                .unwrap_or(record.native_task_version)
                .max(1),
        }]);
    }
    if crate::core::todo::is_missing_or_unknown_actor(agent_id) || is_container_relay_active() {
        return Ok(Vec::new());
    }
    let identity = resolve_repository_identity(main_root)?;
    match list_hosted_open_tasks_blocking(main_root, &identity) {
        Ok(tasks) => Ok(tasks
            .into_iter()
            .filter(|t| t.status == "in_progress" && t.assignee.as_deref() == Some(agent_id))
            .map(|t| storage_task_to_todo_task(&t))
            .collect()),
        Err(_) => Ok(Vec::new()),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_workspace_tests.rs"]
mod tests;

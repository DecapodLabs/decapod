//! Project-opted, capability-scoped local agent database maintenance (#1568).
//!
//! Exposes a small, versioned capability vocabulary over existing Dactyl and
//! `StorageLock` primitives when — and only when — the canonical main-repository
//! `.decapod/config.toml` explicitly opts in via `[governance.db_maintenance]`.
//!
//! Security and governance invariants:
//! - Default-deny when `[governance.db_maintenance]` is absent or `enabled = false`.
//! - Policy is resolved exclusively from the canonical main repository boundary,
//!   never from an agent's isolated worktree or an unmerged checkout branch, and
//!   its SHA-256 digest + revision are bound to every admission decision and
//!   append-only audit entry.
//! - Cloud backends (`backend = "cloud"`) always fail closed (`CLOUD_PRINCIPAL_UNVERIFIED`
//!   or `POLICY_OPT_IN_DISABLED`) until Propodus #88 supplies verified repository-privileged
//!   human delegation; client-provided environment variables or actor claims never
//!   grant cloud authority.
//! - Higher-risk repair (`db.recover`) belongs to the repository `owner` repair
//!   tier (#1590) and is never granted by `[governance.db_maintenance]` alone.
//!   Raw SQL, `sqlite3`, restore, vacuum, and schema migration are unmodeled.
//! - Privileged attempts emit redacted append-only NDJSON audit records under
//!   `<main_store>/maintenance/audit.ndjson` without falsifying validation or
//!   publication readiness (`implies_validation_ready = false`).

use crate::cli::{BackendType, DecapodProjectConfig};
use crate::core::error::DecapodError;
use crate::core::storage_lock::{StorageLock, StorageLockDiagnosis, StorageLockMode};
use dactyl_db::AccessMode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

/// Versioned schema identifier for capability discovery and responses.
pub const DB_MAINTENANCE_SCHEMA_VERSION: &str = "decapod.db-maintenance.v1";

/// Versioned schema identifier for append-only maintenance audit entries.
pub const DB_MAINTENANCE_AUDIT_SCHEMA_VERSION: &str = "decapod.db-maintenance-audit.v1";

/// Maximum bytes read from canonical `.decapod/config.toml` (`1 MiB`).
pub const MAX_CANONICAL_POLICY_BYTES: u64 = 1024 * 1024;

/// Maximum source SQLite size allowed for agent-invoked `db.backup` (`512 MiB`).
pub const MAX_AGENT_BACKUP_SOURCE_BYTES: u64 = 512 * 1024 * 1024;

/// Maximum retry attempts allowed for bounded `db.lock.retry`.
pub const MAX_RETRY_ATTEMPTS: u32 = 10;

/// Maximum wall-clock deadline allowed for bounded `db.lock.retry` (`10s`).
pub const MAX_RETRY_DEADLINE_MS: u64 = 10_000;

/// Maximum initial backoff allowed for bounded `db.lock.retry` (`1s`).
pub const MAX_RETRY_INITIAL_BACKOFF_MS: u64 = 1_000;

fn is_false(value: &bool) -> bool {
    !*value
}

///Individually modeled, versioned database maintenance capabilities.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    clap::ValueEnum,
)]
pub enum DbMaintenanceCapability {
    /// Bounded read-only storage inventory + lock snapshot.
    #[serde(rename = "db.inspect")]
    #[clap(name = "db.inspect", alias = "inspect")]
    Inspect,
    /// Read-only Dactyl SQLite integrity probe.
    #[serde(rename = "db.verify")]
    #[clap(name = "db.verify", alias = "verify")]
    Verify,
    /// Non-destructive advisory lock and sidecar contention diagnosis.
    #[serde(rename = "db.lock.diagnose")]
    #[clap(name = "db.lock.diagnose", alias = "lock.diagnose")]
    LockDiagnose,
    /// Bounded backoff/deadline retry of an authorized safe operation.
    #[serde(rename = "db.lock.retry")]
    #[clap(name = "db.lock.retry", alias = "lock.retry")]
    LockRetry,
    /// Safe relinquish of an explicitly registered idle current-process lease.
    #[serde(rename = "db.lock.release_own")]
    #[clap(name = "db.lock.release_own", alias = "lock.release_own")]
    LockReleaseOwn,
    /// Dactyl online SQLite backup to a bounded non-existing backup path.
    #[serde(rename = "db.backup")]
    #[clap(name = "db.backup", alias = "backup")]
    Backup,
    /// Logical dump/reload replacement; reserved for repository `owner` repair (#1590).
    #[serde(rename = "db.recover")]
    #[clap(name = "db.recover", alias = "recover")]
    Recover,
}

/// Repository authority tier aligned with Decapod #1590 (`read`/`write`/`owner`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceAuthorityTier {
    /// Safe diagnostic/contention/backup slice eligible via `[governance.db_maintenance]`.
    ProjectMaintenanceOptIn,
    /// Destructive/replacement repair requiring explicit repository `owner` authority (#1590).
    Owner,
}

impl DbMaintenanceCapability {
    pub const ALL: [Self; 7] = [
        Self::Inspect,
        Self::Verify,
        Self::LockDiagnose,
        Self::LockRetry,
        Self::LockReleaseOwn,
        Self::Backup,
        Self::Recover,
    ];

    pub const DEFAULT_SAFE_ALLOWLIST: [Self; 5] = [
        Self::Inspect,
        Self::Verify,
        Self::LockDiagnose,
        Self::LockRetry,
        Self::Backup,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "db.inspect",
            Self::Verify => "db.verify",
            Self::LockDiagnose => "db.lock.diagnose",
            Self::LockRetry => "db.lock.retry",
            Self::LockReleaseOwn => "db.lock.release_own",
            Self::Backup => "db.backup",
            Self::Recover => "db.recover",
        }
    }

    pub fn parse_str(raw: &str) -> Option<Self> {
        match raw.trim() {
            "db.inspect" => Some(Self::Inspect),
            "db.verify" => Some(Self::Verify),
            "db.lock.diagnose" => Some(Self::LockDiagnose),
            "db.lock.retry" => Some(Self::LockRetry),
            "db.lock.release_own" => Some(Self::LockReleaseOwn),
            "db.backup" => Some(Self::Backup),
            "db.recover" => Some(Self::Recover),
            _ => None,
        }
    }

    pub fn authority_tier(self) -> MaintenanceAuthorityTier {
        match self {
            Self::Inspect
            | Self::Verify
            | Self::LockDiagnose
            | Self::LockRetry
            | Self::LockReleaseOwn
            | Self::Backup => MaintenanceAuthorityTier::ProjectMaintenanceOptIn,
            Self::Recover => MaintenanceAuthorityTier::Owner,
        }
    }

    pub fn is_mutating(self) -> bool {
        matches!(self, Self::LockReleaseOwn | Self::Backup | Self::Recover)
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Inspect => {
                "Read-only Dactyl datastore inventory combined with non-destructive lock status"
            }
            Self::Verify => "Read-only Dactyl SQLite integrity check without automatic repair",
            Self::LockDiagnose => {
                "Distinguish unlocked, open shared connections, active exclusive maintenance, timeout, and current_process vs unknown owner"
            }
            Self::LockRetry => {
                "Bounded backoff and deadline retry of an authorized db.inspect, db.verify, or db.backup operation"
            }
            Self::LockReleaseOwn => {
                "Relinquish only an explicitly registered idle current-process lease without deleting sidecars or unlocking external processes"
            }
            Self::Backup => {
                "Create a Dactyl online SQLite backup under the repository .decapod/backups boundary"
            }
            Self::Recover => {
                "Logical dump/reload database replacement; requires repository owner repair authority (#1590) and is never granted by db_maintenance opt-in alone"
            }
        }
    }
}

/// Target capability eligible for bounded `db.lock.retry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RetryTargetCapability {
    Inspect,
    Verify,
    Backup,
}

impl RetryTargetCapability {
    pub fn as_capability(self) -> DbMaintenanceCapability {
        match self {
            Self::Inspect => DbMaintenanceCapability::Inspect,
            Self::Verify => DbMaintenanceCapability::Verify,
            Self::Backup => DbMaintenanceCapability::Backup,
        }
    }
}

/// Project-level configuration in `[governance.db_maintenance]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DbMaintenanceConfig {
    /// Explicit default-false project opt-in for local safe agent database maintenance.
    #[serde(default, skip_serializing_if = "is_false")]
    pub enabled: bool,
    /// Optional explicit subset of modeled capabilities; defaults to safe diagnostics + backup when `enabled = true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<DbMaintenanceCapability>>,
}

impl DbMaintenanceConfig {
    pub fn is_default(&self) -> bool {
        !self.enabled && self.allowed.is_none()
    }

    pub fn effective_safe_allowlist(&self) -> BTreeSet<DbMaintenanceCapability> {
        if !self.enabled {
            return BTreeSet::new();
        }
        let configured: Vec<DbMaintenanceCapability> = self
            .allowed
            .clone()
            .unwrap_or_else(|| DbMaintenanceCapability::DEFAULT_SAFE_ALLOWLIST.to_vec());
        configured
            .into_iter()
            .filter(|cap| cap.authority_tier() == MaintenanceAuthorityTier::ProjectMaintenanceOptIn)
            .collect()
    }
}

/// Three-valued capability discovery and admission disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilityDecision {
    Allowed,
    Denied,
    Unsupported,
}

/// Per-capability discovery entry returned by `data db capabilities`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityStatusEntry {
    pub capability: String,
    pub required_tier: MaintenanceAuthorityTier,
    pub mutating: bool,
    pub decision: CapabilityDecision,
    pub reason_code: String,
    pub summary: String,
}

/// Canonical policy snapshot bound from the accepted main repository boundary.
#[derive(Debug, Clone)]
pub struct CanonicalMaintenancePolicy {
    pub main_root: PathBuf,
    pub workspace_root: PathBuf,
    pub main_store_root: PathBuf,
    pub repo_identity: String,
    pub backend: BackendType,
    pub policy_sha256: String,
    pub canonical_revision: String,
    pub config: DbMaintenanceConfig,
    /// Populated when a non-protected branch on `main_root` attempted to diverge from protected base policy.
    pub unmerged_branch_violation: Option<String>,
}

/// Verified caller and isolated-worktree custody context.
#[derive(Debug, Clone)]
pub struct VerifiedMaintenanceActor {
    pub agent_id: String,
    pub session_token: String,
    pub task_id: Option<String>,
    pub worktree_id: String,
    pub branch: String,
}

/// Callback interface supplied by `lib.rs` to verify pre-existing agent session credentials
/// without auto-acquiring or overwriting session files.
pub type SessionVerifier<'a> =
    &'a dyn Fn(&Path, &Path, &str, &str) -> Result<String, (&'static str, String)>;

/// Append-only redacted audit record written for privileged maintenance attempts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DbMaintenanceAuditRecord {
    pub schema_version: String,
    pub audit_id: String,
    pub ts: String,
    pub operation_id: Option<String>,
    pub agent_id: String,
    pub task_id: Option<String>,
    pub worktree_id: String,
    pub repo_identity: String,
    pub database: String,
    pub backend: String,
    pub canonical_policy_sha256: Option<String>,
    pub canonical_revision: Option<String>,
    pub capability: String,
    pub preconditions: Vec<String>,
    pub decision: CapabilityDecision,
    pub reason_code: String,
    pub outcome: String,
    pub evidence_ref: Option<String>,
    pub implies_validation_ready: bool,
}

/// Parameters for bounded `db.lock.retry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockRetrySpec {
    pub target: RetryTargetCapability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    #[serde(default = "default_retry_max_attempts")]
    pub max_attempts: u32,
    #[serde(default = "default_retry_initial_backoff_ms")]
    pub initial_backoff_ms: u64,
    #[serde(default = "default_retry_deadline_ms")]
    pub deadline_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_policy_sha256: Option<String>,
}

pub fn default_retry_max_attempts() -> u32 {
    4
}

pub fn default_retry_initial_backoff_ms() -> u64 {
    50
}

pub fn default_retry_deadline_ms() -> u64 {
    2_000
}

/// Resolve canonical maintenance policy strictly from the main repository boundary.
pub fn resolve_canonical_policy(
    invocation_dir: &Path,
) -> Result<CanonicalMaintenancePolicy, (&'static str, String)> {
    let invocation_canon = invocation_dir.canonicalize().map_err(|e| {
        (
            "CANONICAL_ROOT_UNTRUSTED",
            format!("unable to canonicalize invocation path: {e}"),
        )
    })?;
    let workspace_root = find_project_root_upwards(&invocation_canon).ok_or_else(|| {
        (
            "CANONICAL_ROOT_UNTRUSTED",
            "no .decapod/config.toml found in current or parent directories".to_string(),
        )
    })?;
    let main_root_raw =
        crate::core::workspace::get_main_repo_root(&workspace_root).map_err(|e| {
            (
                "CANONICAL_ROOT_UNTRUSTED",
                format!("unable to resolve main repository root: {e}"),
            )
        })?;
    let main_root = main_root_raw.canonicalize().map_err(|e| {
        (
            "CANONICAL_ROOT_UNTRUSTED",
            format!("unable to canonicalize main repository root: {e}"),
        )
    })?;
    let main_root_recheck = crate::core::workspace::get_main_repo_root(&main_root)
        .and_then(|p| p.canonicalize().map_err(DecapodError::IoError))
        .map_err(|e| {
            (
                "CANONICAL_ROOT_UNTRUSTED",
                format!("main repository root stability check failed: {e}"),
            )
        })?;
    if main_root != main_root_recheck
        || crate::core::workspace::is_canonical_decapod_workspace_path(&main_root)
    {
        return Err((
            "CANONICAL_ROOT_UNTRUSTED",
            "resolved canonical root points inside an isolated workspace rather than the main repository"
                .to_string(),
        ));
    }

    let main_config_path = main_root.join(".decapod").join("config.toml");
    let main_bytes = read_regular_policy_bytes(&main_config_path)?;
    let main_parsed: DecapodProjectConfig =
        toml::from_str(std::str::from_utf8(&main_bytes).map_err(|_| {
            (
                "INVALID_CANONICAL_POLICY",
                "canonical .decapod/config.toml is not valid UTF-8".to_string(),
            )
        })?)
        .map_err(|e| {
            (
                "INVALID_CANONICAL_POLICY",
                format!("canonical .decapod/config.toml failed schema validation: {e}"),
            )
        })?;

    let configured_base = main_parsed
        .repo
        .base_branch
        .as_deref()
        .unwrap_or("master")
        .trim()
        .to_string();

    // Enforce protected-branch boundary on main_root: if main_root itself is checked
    // out to a non-protected branch (e.g. `agent/*` or a PR branch), refuse to trust
    // main_root's working tree and read committed `<base_branch>:.decapod/config.toml`.
    let (effective_bytes, effective_cfg, canonical_revision, unmerged_violation) =
        resolve_branch_safe_policy(&main_root, &configured_base, main_bytes, main_parsed)?;

    // Check backend consistency between workspace_root and canonical main_root so a
    // worktree config edit cannot switch `cloud` -> `local` or vice versa.
    if workspace_root != main_root {
        let ws_config_path = workspace_root.join(".decapod").join("config.toml");
        if ws_config_path.exists() {
            let ws_bytes = read_regular_policy_bytes(&ws_config_path)?;
            let ws_parsed: DecapodProjectConfig =
                toml::from_str(std::str::from_utf8(&ws_bytes).map_err(|_| {
                    (
                        "BACKEND_MISMATCH",
                        "worktree .decapod/config.toml is not valid UTF-8".to_string(),
                    )
                })?)
                .map_err(|e| {
                    (
                        "INVALID_WORKTREE_CONFIG",
                        format!("worktree .decapod/config.toml failed schema validation: {e}"),
                    )
                })?;
            if ws_parsed.repo.effective_backend() != effective_cfg.repo.effective_backend() {
                return Err((
                    "BACKEND_MISMATCH",
                    format!(
                        "worktree backend '{:?}' disagrees with canonical repository backend '{:?}'",
                        ws_parsed.repo.effective_backend(),
                        effective_cfg.repo.effective_backend()
                    ),
                ));
            }
        }
    }

    let policy_sha256 = format!("sha256:{:x}", Sha256::digest(&effective_bytes));
    let repo_identity = derive_repo_identity(&main_root, &effective_cfg);
    let main_store_root = main_root.join(".decapod").join("data");

    Ok(CanonicalMaintenancePolicy {
        main_root,
        workspace_root,
        main_store_root,
        repo_identity,
        backend: effective_cfg.repo.effective_backend(),
        policy_sha256,
        canonical_revision,
        config: effective_cfg.governance.db_maintenance,
        unmerged_branch_violation: unmerged_violation,
    })
}

fn resolve_branch_safe_policy(
    main_root: &Path,
    configured_base: &str,
    working_bytes: Vec<u8>,
    working_cfg: DecapodProjectConfig,
) -> Result<(Vec<u8>, DecapodProjectConfig, String, Option<String>), (&'static str, String)> {
    if !main_root.join(".git").exists() {
        return Err((
            "CANONICAL_GIT_BOUNDARY_REQUIRED",
            "agent database maintenance requires a Git-backed canonical repository boundary"
                .to_string(),
        ));
    }

    let head_branch = git_stdout(main_root, &["symbolic-ref", "--quiet", "--short", "HEAD"]);
    let head_oid = git_stdout(main_root, &["rev-parse", "--verify", "HEAD"]);
    let is_protected = head_branch
        .as_deref()
        .is_some_and(|branch| is_protected_branch_name(branch, configured_base));

    if is_protected {
        let dirty = git_stdout(
            main_root,
            &["status", "--porcelain", "--", ".decapod/config.toml"],
        )
        .is_some_and(|s| !s.trim().is_empty());
        let rev = match (head_oid, dirty) {
            (Some(oid), false) => format!("{oid}:clean"),
            (Some(oid), true) => format!("{oid}:canonical-worktree"),
            (None, _) => format!(
                "{}:unborn-bootstrap",
                head_branch.unwrap_or_else(|| configured_base.to_string())
            ),
        };
        return Ok((working_bytes, working_cfg, rev, None));
    }

    // `main_root` is on a detached HEAD or non-protected branch (e.g. `agent/*` or a PR branch).
    // Never trust working-tree edits on an unmerged branch; resolve from protected base ref.
    let candidate_refs = [
        format!("refs/remotes/origin/{configured_base}"),
        format!("refs/heads/{configured_base}"),
        "refs/remotes/origin/master".to_string(),
        "refs/remotes/origin/main".to_string(),
        "refs/heads/master".to_string(),
        "refs/heads/main".to_string(),
    ];
    for git_ref in candidate_refs {
        if let Some(base_oid) = git_stdout(main_root, &["rev-parse", "--verify", &git_ref])
            && let Some(blob) = git_bytes(
                main_root,
                &["show", &format!("{base_oid}:.decapod/config.toml")],
            )
        {
            if blob.len() as u64 > MAX_CANONICAL_POLICY_BYTES {
                return Err((
                    "INVALID_CANONICAL_POLICY",
                    "committed base .decapod/config.toml exceeds 1 MiB".to_string(),
                ));
            }
            let base_cfg: DecapodProjectConfig =
                toml::from_str(std::str::from_utf8(&blob).map_err(|_| {
                    (
                        "INVALID_CANONICAL_POLICY",
                        "committed base .decapod/config.toml is not valid UTF-8".to_string(),
                    )
                })?)
                .map_err(|e| {
                    (
                        "INVALID_CANONICAL_POLICY",
                        format!("committed base .decapod/config.toml invalid: {e}"),
                    )
                })?;
            let violation = if working_cfg.governance.db_maintenance
                != base_cfg.governance.db_maintenance
            {
                Some(format!(
                    "unmerged checkout '{}' on main root cannot self-elevate db_maintenance policy; enforced committed base '{}'",
                    head_branch.as_deref().unwrap_or("DETACHED"),
                    git_ref
                ))
            } else {
                None
            };
            return Ok((
                blob,
                base_cfg,
                format!("{base_oid}:accepted-ref"),
                violation,
            ));
        }
    }

    Err((
        "UNMERGED_BRANCH_POLICY_UNTRUSTED",
        format!(
            "main repository is on non-protected branch '{}' and no committed protected base policy could be verified",
            head_branch.as_deref().unwrap_or("DETACHED")
        ),
    ))
}

fn is_protected_branch_name(branch: &str, configured_base: &str) -> bool {
    let trimmed = branch.trim();
    if trimmed.starts_with("agent/") || trimmed.contains("todo-") {
        return false;
    }
    trimmed == "master" || trimmed == "main" || trimmed == configured_base
}

/// Strictly authenticate the calling agent session and verify isolated worktree custody.
pub fn verify_actor_and_custody(
    invocation_dir: &Path,
    policy: &CanonicalMaintenancePolicy,
    claimed_actor: Option<&str>,
    verify_session: SessionVerifier<'_>,
) -> Result<VerifiedMaintenanceActor, (&'static str, String)> {
    let raw_agent = std::env::var("DECAPOD_AGENT_ID").map_err(|_| {
        (
            "AGENT_IDENTITY_UNAUTHENTICATED",
            "DECAPOD_AGENT_ID must be explicitly set to an authenticated agent identity"
                .to_string(),
        )
    })?;
    if !is_valid_agent_identifier(&raw_agent) {
        return Err((
            "AGENT_IDENTITY_UNAUTHENTICATED",
            format!(
                "DECAPOD_AGENT_ID '{}' is reserved, empty, or malformed for governed agent maintenance",
                redact_secrets(&raw_agent)
            ),
        ));
    }
    if let Some(actor) = claimed_actor
        && actor.trim() != raw_agent
    {
        return Err((
            "AGENT_IDENTITY_FORGED",
            format!(
                "supplied actor '{}' does not match authenticated DECAPOD_AGENT_ID '{}'",
                redact_secrets(actor),
                raw_agent
            ),
        ));
    }

    let raw_password = std::env::var("DECAPOD_SESSION_PASSWORD").map_err(|_| {
        (
            "AGENT_IDENTITY_UNAUTHENTICATED",
            "DECAPOD_SESSION_PASSWORD is required for governed agent database maintenance"
                .to_string(),
        )
    })?;
    if raw_password.trim().is_empty() {
        return Err((
            "AGENT_IDENTITY_UNAUTHENTICATED",
            "DECAPOD_SESSION_PASSWORD must not be empty".to_string(),
        ));
    }

    let session_token = verify_session(
        &policy.workspace_root,
        &policy.main_root,
        &raw_agent,
        &raw_password,
    )?;

    // Verify isolated worktree boundary under `<main_root>/.decapod/workspaces/*`.
    let status = crate::core::workspace::get_workspace_status(invocation_dir).map_err(|e| {
        (
            "WORKTREE_CUSTODY_REQUIRED",
            format!("unable to inspect git worktree status: {e}"),
        )
    })?;
    if !status.git.in_worktree || status.git.is_main_repo || status.git.is_protected {
        return Err((
            "WORKTREE_CUSTODY_REQUIRED",
            "governed agent database maintenance must execute from the agent's isolated git worktree under '.decapod/workspaces/*'"
                .to_string(),
        ));
    }
    let wt_path = status
        .git
        .worktree_path
        .as_ref()
        .unwrap_or(&policy.workspace_root)
        .canonicalize()
        .map_err(|e| {
            (
                "WORKTREE_CUSTODY_REQUIRED",
                format!("unable to canonicalize worktree path: {e}"),
            )
        })?;
    let expected_workspaces_dir = policy.main_root.join(".decapod").join("workspaces");
    if !wt_path.starts_with(&expected_workspaces_dir)
        || !crate::core::workspace::is_canonical_decapod_workspace_path(&wt_path)
    {
        return Err((
            "WORKTREE_CUSTODY_REQUIRED",
            "worktree does not belong to this repository's canonical '.decapod/workspaces/*' directory"
                .to_string(),
        ));
    }

    let branch = status.git.current_branch.trim().to_string();
    let sanitized_agent = sanitize_agent_branch_segment(&raw_agent);
    if let Some(rest) = branch.strip_prefix("agent/") {
        let branch_owner = rest.split('/').next().unwrap_or("");
        if branch_owner != raw_agent && branch_owner != sanitized_agent {
            return Err((
                "WORKTREE_CUSTODY_MISMATCH",
                format!(
                    "worktree branch '{}' belongs to agent '{}', not '{}'",
                    branch, branch_owner, raw_agent
                ),
            ));
        }
    } else {
        return Err((
            "WORKTREE_CUSTODY_MISMATCH",
            format!(
                "worktree branch '{}' is not agent-scoped (`agent/<agent-id>/<task-id>-<topic>`)",
                branch
            ),
        ));
    }

    let worktree_slug = wt_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("worktree")
        .to_string();

    let task_id = extract_worktree_task_id(&wt_path, &branch);

    // Opportunistic non-blocking SQLite task custody cross-check when the canonical
    // database exists and is NOT currently under exclusive lock contention.
    if let Some(ref tid) = task_id {
        check_task_custody_if_unlocked(&policy.main_store_root, tid, &raw_agent)?;
    }

    Ok(VerifiedMaintenanceActor {
        agent_id: raw_agent,
        session_token,
        task_id,
        worktree_id: worktree_slug,
        branch,
    })
}

/// Evaluate capability admissibility for a single modeled capability against the canonical policy.
pub fn evaluate_capability_decision(
    policy: &CanonicalMaintenancePolicy,
    capability: DbMaintenanceCapability,
    expected_policy_sha256: Option<&str>,
) -> (CapabilityDecision, &'static str, String) {
    if let Some(expected) = expected_policy_sha256
        && normalize_sha256(expected) != normalize_sha256(&policy.policy_sha256)
    {
        return (
            CapabilityDecision::Denied,
            "POLICY_SNAPSHOT_STALE",
            format!(
                "expected canonical policy '{}' but observed '{}'",
                expected, policy.policy_sha256
            ),
        );
    }

    if let Some(ref violation) = policy.unmerged_branch_violation
        && !policy.config.enabled
    {
        return (
            CapabilityDecision::Denied,
            "UNMERGED_BRANCH_POLICY_UNTRUSTED",
            violation.clone(),
        );
    }

    if policy.backend.is_cloud() {
        if !policy.config.enabled {
            return (
                CapabilityDecision::Denied,
                "POLICY_OPT_IN_DISABLED",
                "canonical [governance.db_maintenance].enabled is false and backend is cloud"
                    .to_string(),
            );
        }
        return (
            CapabilityDecision::Denied,
            "CLOUD_PRINCIPAL_UNVERIFIED",
            "cloud database maintenance requires both canonical project opt-in and a server-verified repository-privileged human principal via Propodus #88; client config or environment never grants cloud authority"
                .to_string(),
        );
    }

    if capability.authority_tier() == MaintenanceAuthorityTier::Owner {
        return (
            CapabilityDecision::Denied,
            "OWNER_REPAIR_AUTHORITY_REQUIRED",
            format!(
                "'{}' performs destructive/replacement repair and requires explicit repository owner authority (#1590); [governance.db_maintenance] never grants it alone",
                capability.as_str()
            ),
        );
    }

    if !policy.config.enabled {
        return (
            CapabilityDecision::Denied,
            "POLICY_OPT_IN_DISABLED",
            "canonical .decapod/config.toml does not set [governance.db_maintenance] enabled = true"
                .to_string(),
        );
    }

    let allowlist = policy.config.effective_safe_allowlist();
    if !allowlist.contains(&capability) {
        return (
            CapabilityDecision::Denied,
            "CAPABILITY_NOT_ALLOWLISTED",
            format!(
                "capability '{}' is not included in canonical [governance.db_maintenance].allowed",
                capability.as_str()
            ),
        );
    }

    (
        CapabilityDecision::Allowed,
        "CANONICAL_LOCAL_OPT_IN_ALLOWED",
        format!(
            "capability '{}' authorized by canonical local policy {}",
            capability.as_str(),
            policy.policy_sha256
        ),
    )
}

/// Evaluate an arbitrary capability name (including unmodeled capabilities like `db.sql` or `db.vacuum`).
pub fn evaluate_named_capability(
    policy: &CanonicalMaintenancePolicy,
    raw_capability: &str,
    expected_policy_sha256: Option<&str>,
) -> CapabilityStatusEntry {
    if let Some(cap) = DbMaintenanceCapability::parse_str(raw_capability) {
        let (decision, code, _detail) =
            evaluate_capability_decision(policy, cap, expected_policy_sha256);
        CapabilityStatusEntry {
            capability: cap.as_str().to_string(),
            required_tier: cap.authority_tier(),
            mutating: cap.is_mutating(),
            decision,
            reason_code: code.to_string(),
            summary: cap.summary().to_string(),
        }
    } else {
        CapabilityStatusEntry {
            capability: raw_capability.trim().to_string(),
            required_tier: MaintenanceAuthorityTier::Owner,
            mutating: true,
            decision: CapabilityDecision::Unsupported,
            reason_code: "CAPABILITY_UNSUPPORTED".to_string(),
            summary: "Unmodeled database operation (raw SQL, sqlite3 shell, restore, vacuum, and ad-hoc schema migration are never exposed to agents)".to_string(),
        }
    }
}

/// Build the full capability discovery report for the calling agent.
pub fn build_capabilities_report(
    policy: &CanonicalMaintenancePolicy,
    actor: &VerifiedMaintenanceActor,
    probe_capabilities: &[String],
) -> serde_json::Value {
    let mut entries: Vec<CapabilityStatusEntry> = DbMaintenanceCapability::ALL
        .iter()
        .map(|cap| {
            let (decision, code, _) = evaluate_capability_decision(policy, *cap, None);
            CapabilityStatusEntry {
                capability: cap.as_str().to_string(),
                required_tier: cap.authority_tier(),
                mutating: cap.is_mutating(),
                decision,
                reason_code: code.to_string(),
                summary: cap.summary().to_string(),
            }
        })
        .collect();

    for extra in probe_capabilities {
        if DbMaintenanceCapability::parse_str(extra).is_none() {
            entries.push(evaluate_named_capability(policy, extra, None));
        }
    }

    serde_json::json!({
        "schema_version": DB_MAINTENANCE_SCHEMA_VERSION,
        "status": "ok",
        "backend": match policy.backend {
            BackendType::Local => "local",
            BackendType::Cloud => "cloud",
        },
        "repo_identity": policy.repo_identity,
        "database": crate::core::schemas::LOCAL_DB_NAME,
        "policy": {
            "enabled": policy.config.enabled,
            "canonical_policy_sha256": policy.policy_sha256,
            "canonical_revision": policy.canonical_revision,
            "unmerged_branch_violation": policy.unmerged_branch_violation,
        },
        "actor": {
            "agent_id": actor.agent_id,
            "task_id": actor.task_id,
            "worktree_id": actor.worktree_id,
        },
        "capabilities": entries,
        "unmodeled_forbidden": [
            "db.sql",
            "sqlite3",
            "db.restore",
            "db.vacuum",
            "db.schema_migrate"
        ],
        "implies_validation_ready": false,
    })
}

/// Centralized admission gate + audit execution wrapper for a governed capability.
#[allow(clippy::too_many_arguments)]
pub fn execute_governed_capability(
    invocation_dir: &Path,
    capability_name: &str,
    claimed_actor: Option<&str>,
    operation_id: Option<&str>,
    expected_policy_sha256: Option<&str>,
    verify_session: SessionVerifier<'_>,
    mut preconditions: Vec<String>,
    run_op: impl FnOnce(
        &CanonicalMaintenancePolicy,
        &VerifiedMaintenanceActor,
    ) -> Result<(serde_json::Value, Option<String>), (&'static str, String)>,
) -> Result<serde_json::Value, DecapodError> {
    if let Some(op_id) = operation_id {
        validate_operation_id(op_id)
            .map_err(|(code, msg)| DecapodError::ValidationError(format!("{code}: {msg}")))?;
        preconditions.push(format!("operation_id={op_id}"));
    }
    if let Some(expected) = expected_policy_sha256 {
        preconditions.push(format!("expected_policy_sha256={expected}"));
    }

    let policy = match resolve_canonical_policy(invocation_dir) {
        Ok(policy) => policy,
        Err((code, detail)) => {
            return Err(DecapodError::ValidationError(format!(
                "{code}: {}",
                redact_secrets(&detail)
            )));
        }
    };

    let actor =
        match verify_actor_and_custody(invocation_dir, &policy, claimed_actor, verify_session) {
            Ok(actor) => actor,
            Err((code, detail)) => {
                let redacted = redact_for_repo(&detail, &policy.main_root);
                let _ = append_audit_record(
                    &policy.main_store_root,
                    &DbMaintenanceAuditRecord {
                        schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                        audit_id: format!("maint_{}", crate::core::ulid::new_ulid()),
                        ts: crate::core::time::now_epoch_z(),
                        operation_id: operation_id.map(str::to_string),
                        agent_id: claimed_actor
                            .map(redact_secrets)
                            .or_else(|| {
                                std::env::var("DECAPOD_AGENT_ID")
                                    .ok()
                                    .map(|s| redact_secrets(&s))
                            })
                            .unwrap_or_else(|| "unauthenticated".to_string()),
                        task_id: None,
                        worktree_id: "unverified".to_string(),
                        repo_identity: policy.repo_identity.clone(),
                        database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                        backend: backend_label(policy.backend).to_string(),
                        canonical_policy_sha256: Some(policy.policy_sha256.clone()),
                        canonical_revision: Some(policy.canonical_revision.clone()),
                        capability: capability_name.to_string(),
                        preconditions,
                        decision: CapabilityDecision::Denied,
                        reason_code: code.to_string(),
                        outcome: "denied".to_string(),
                        evidence_ref: None,
                        implies_validation_ready: false,
                    },
                );
                return Err(DecapodError::ValidationError(format!("{code}: {redacted}")));
            }
        };

    let Some(capability) = DbMaintenanceCapability::parse_str(capability_name) else {
        let _ = append_audit_record(
            &policy.main_store_root,
            &DbMaintenanceAuditRecord {
                schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                audit_id: format!("maint_{}", crate::core::ulid::new_ulid()),
                ts: crate::core::time::now_epoch_z(),
                operation_id: operation_id.map(str::to_string),
                agent_id: actor.agent_id.clone(),
                task_id: actor.task_id.clone(),
                worktree_id: actor.worktree_id.clone(),
                repo_identity: policy.repo_identity.clone(),
                database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                backend: backend_label(policy.backend).to_string(),
                canonical_policy_sha256: Some(policy.policy_sha256.clone()),
                canonical_revision: Some(policy.canonical_revision.clone()),
                capability: capability_name.to_string(),
                preconditions,
                decision: CapabilityDecision::Unsupported,
                reason_code: "CAPABILITY_UNSUPPORTED".to_string(),
                outcome: "denied".to_string(),
                evidence_ref: None,
                implies_validation_ready: false,
            },
        );
        return Err(DecapodError::ValidationError(format!(
            "CAPABILITY_UNSUPPORTED: '{}' is not a modeled Decapod database maintenance capability",
            redact_secrets(capability_name)
        )));
    };

    let (decision, reason_code, detail) =
        evaluate_capability_decision(&policy, capability, expected_policy_sha256);
    if decision != CapabilityDecision::Allowed {
        let _ = append_audit_record(
            &policy.main_store_root,
            &DbMaintenanceAuditRecord {
                schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                audit_id: format!("maint_{}", crate::core::ulid::new_ulid()),
                ts: crate::core::time::now_epoch_z(),
                operation_id: operation_id.map(str::to_string),
                agent_id: actor.agent_id.clone(),
                task_id: actor.task_id.clone(),
                worktree_id: actor.worktree_id.clone(),
                repo_identity: policy.repo_identity.clone(),
                database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                backend: backend_label(policy.backend).to_string(),
                canonical_policy_sha256: Some(policy.policy_sha256.clone()),
                canonical_revision: Some(policy.canonical_revision.clone()),
                capability: capability.as_str().to_string(),
                preconditions,
                decision,
                reason_code: reason_code.to_string(),
                outcome: "denied".to_string(),
                evidence_ref: None,
                implies_validation_ready: false,
            },
        );
        return Err(DecapodError::ValidationError(format!(
            "{reason_code}: {}",
            redact_for_repo(&detail, &policy.main_root)
        )));
    }

    // Enforce replay protection on mutating operations when `operation_id` is supplied.
    if let Some(op_id) = operation_id
        && (capability.is_mutating() || capability == DbMaintenanceCapability::LockRetry)
        && has_completed_operation_id(&policy.main_store_root, op_id)?
    {
        let _ = append_audit_record(
            &policy.main_store_root,
            &DbMaintenanceAuditRecord {
                schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                audit_id: format!("maint_{}", crate::core::ulid::new_ulid()),
                ts: crate::core::time::now_epoch_z(),
                operation_id: Some(op_id.to_string()),
                agent_id: actor.agent_id.clone(),
                task_id: actor.task_id.clone(),
                worktree_id: actor.worktree_id.clone(),
                repo_identity: policy.repo_identity.clone(),
                database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                backend: backend_label(policy.backend).to_string(),
                canonical_policy_sha256: Some(policy.policy_sha256.clone()),
                canonical_revision: Some(policy.canonical_revision.clone()),
                capability: capability.as_str().to_string(),
                preconditions,
                decision: CapabilityDecision::Denied,
                reason_code: "OPERATION_REPLAY_REJECTED".to_string(),
                outcome: "denied".to_string(),
                evidence_ref: None,
                implies_validation_ready: false,
            },
        );
        return Err(DecapodError::ValidationError(format!(
            "OPERATION_REPLAY_REJECTED: mutating maintenance operation_id '{op_id}' was already executed and will not be replayed"
        )));
    }

    // Re-verify canonical policy SHA-256 right before executing to minimize TOCTOU window.
    let fresh_policy = resolve_canonical_policy(invocation_dir).map_err(|(code, msg)| {
        DecapodError::ValidationError(format!("{code}: {}", redact_secrets(&msg)))
    })?;
    if fresh_policy.policy_sha256 != policy.policy_sha256 {
        let _ = append_audit_record(
            &policy.main_store_root,
            &DbMaintenanceAuditRecord {
                schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                audit_id: format!("maint_{}", crate::core::ulid::new_ulid()),
                ts: crate::core::time::now_epoch_z(),
                operation_id: operation_id.map(str::to_string),
                agent_id: actor.agent_id.clone(),
                task_id: actor.task_id.clone(),
                worktree_id: actor.worktree_id.clone(),
                repo_identity: policy.repo_identity.clone(),
                database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                backend: backend_label(policy.backend).to_string(),
                canonical_policy_sha256: Some(fresh_policy.policy_sha256.clone()),
                canonical_revision: Some(fresh_policy.canonical_revision.clone()),
                capability: capability.as_str().to_string(),
                preconditions,
                decision: CapabilityDecision::Denied,
                reason_code: "POLICY_SNAPSHOT_STALE".to_string(),
                outcome: "denied".to_string(),
                evidence_ref: None,
                implies_validation_ready: false,
            },
        );
        return Err(DecapodError::ValidationError(
            "POLICY_SNAPSHOT_STALE: canonical .decapod/config.toml changed during admission"
                .to_string(),
        ));
    }

    let audit_id = format!("maint_{}", crate::core::ulid::new_ulid());
    match run_op(&fresh_policy, &actor) {
        Ok((mut payload, evidence_ref)) => {
            let redacted_ref = evidence_ref
                .as_deref()
                .map(|r| redact_for_repo(r, &fresh_policy.main_root));
            append_audit_record(
                &fresh_policy.main_store_root,
                &DbMaintenanceAuditRecord {
                    schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                    audit_id: audit_id.clone(),
                    ts: crate::core::time::now_epoch_z(),
                    operation_id: operation_id.map(str::to_string),
                    agent_id: actor.agent_id.clone(),
                    task_id: actor.task_id.clone(),
                    worktree_id: actor.worktree_id.clone(),
                    repo_identity: fresh_policy.repo_identity.clone(),
                    database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                    backend: backend_label(fresh_policy.backend).to_string(),
                    canonical_policy_sha256: Some(fresh_policy.policy_sha256.clone()),
                    canonical_revision: Some(fresh_policy.canonical_revision.clone()),
                    capability: capability.as_str().to_string(),
                    preconditions,
                    decision: CapabilityDecision::Allowed,
                    reason_code: reason_code.to_string(),
                    outcome: "succeeded".to_string(),
                    evidence_ref: redacted_ref.clone(),
                    implies_validation_ready: false,
                },
            )?;
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "schema_version".to_string(),
                    serde_json::json!(DB_MAINTENANCE_SCHEMA_VERSION),
                );
                obj.insert(
                    "capability".to_string(),
                    serde_json::json!(capability.as_str()),
                );
                obj.insert(
                    "canonical_policy_sha256".to_string(),
                    serde_json::json!(fresh_policy.policy_sha256),
                );
                obj.insert(
                    "canonical_revision".to_string(),
                    serde_json::json!(fresh_policy.canonical_revision),
                );
                obj.insert("audit_id".to_string(), serde_json::json!(audit_id));
                obj.insert(
                    "implies_validation_ready".to_string(),
                    serde_json::json!(false),
                );
            }
            Ok(payload)
        }
        Err((err_code, err_detail)) => {
            let outcome = if err_code == "RETRY_DEADLINE_EXCEEDED" {
                "deadline_exceeded"
            } else {
                "failed"
            };
            let redacted_detail = redact_for_repo(&err_detail, &fresh_policy.main_root);
            let _ = append_audit_record(
                &fresh_policy.main_store_root,
                &DbMaintenanceAuditRecord {
                    schema_version: DB_MAINTENANCE_AUDIT_SCHEMA_VERSION.to_string(),
                    audit_id,
                    ts: crate::core::time::now_epoch_z(),
                    operation_id: operation_id.map(str::to_string),
                    agent_id: actor.agent_id.clone(),
                    task_id: actor.task_id.clone(),
                    worktree_id: actor.worktree_id.clone(),
                    repo_identity: fresh_policy.repo_identity.clone(),
                    database: crate::core::schemas::LOCAL_DB_NAME.to_string(),
                    backend: backend_label(fresh_policy.backend).to_string(),
                    canonical_policy_sha256: Some(fresh_policy.policy_sha256.clone()),
                    canonical_revision: Some(fresh_policy.canonical_revision.clone()),
                    capability: capability.as_str().to_string(),
                    preconditions,
                    decision: CapabilityDecision::Allowed,
                    reason_code: err_code.to_string(),
                    outcome: outcome.to_string(),
                    evidence_ref: None,
                    implies_validation_ready: false,
                },
            );
            Err(DecapodError::ValidationError(format!(
                "{err_code}: {redacted_detail}"
            )))
        }
    }
}

/// Execute `db.inspect` (read-only Dactyl inventory + non-destructive lock snapshot).
pub fn run_maintain_inspect(
    policy: &CanonicalMaintenancePolicy,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    let lock = StorageLock::diagnose(&db_path, Duration::ZERO)
        .map_err(|e| ("LOCK_DIAGNOSE_FAILED", e.to_string()))?;
    let options = crate::core::storage_inventory::InventoryOptions::default();
    let inventory =
        crate::core::storage_inventory::inspect(&policy.main_store_root, "repository", &options)
            .map_err(|e| ("INSPECT_FAILED", e.to_string()))?;
    Ok((
        serde_json::json!({
            "status": inventory["status"],
            "diagnostic_status": if inventory["status"] == "ok" { "healthy" } else { "degraded" },
            "database": crate::core::schemas::LOCAL_DB_NAME,
            "lock": lock,
            "inventory": inventory,
            "automatic_repair": false,
        }),
        Some("<repo>/.decapod/data/decapod.db".to_string()),
    ))
}

/// Execute `db.verify` (read-only Dactyl integrity probe).
pub fn run_maintain_verify(
    policy: &CanonicalMaintenancePolicy,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    crate::core::dactyl::ensure_local_sqlite_runtime()
        .map_err(|e| ("SQLITE_RUNTIME_UNAVAILABLE", e.to_string()))?;
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    if !db_path.exists() {
        return Ok((
            serde_json::json!({
                "status": "missing",
                "diagnostic_status": "unavailable",
                "database": crate::core::schemas::LOCAL_DB_NAME,
                "integrity_check": null,
                "automatic_repair": false,
            }),
            Some("<repo>/.decapod/data/decapod.db".to_string()),
        ));
    }
    let bridge = crate::core::dactyl::DactylBridge::open_canonical(
        &policy.main_store_root,
        AccessMode::ReadOnly,
    )
    .map_err(|e| classify_dactyl_op_error("VERIFY_FAILED", &e))?;
    let report = bridge
        .verify_integrity()
        .map_err(|e| classify_dactyl_op_error("VERIFY_CORRUPT_OR_FAILED", &e))?;
    Ok((
        serde_json::json!({
            "status": "ok",
            "diagnostic_status": "healthy",
            "database": crate::core::schemas::LOCAL_DB_NAME,
            "integrity_check": "ok",
            "journal_mode": report.journal_mode,
            "user_version": report.user_version,
            "application_id": report.application_id,
            "automatic_repair": false,
        }),
        Some("<repo>/.decapod/data/decapod.db".to_string()),
    ))
}

/// Execute `db.lock.diagnose` (non-destructive sidecar lock & contention probe).
pub fn run_maintain_lock_diagnose(
    policy: &CanonicalMaintenancePolicy,
    probe_timeout_ms: u64,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    let diagnosis: StorageLockDiagnosis =
        StorageLock::diagnose(&db_path, Duration::from_millis(probe_timeout_ms))
            .map_err(|e| ("LOCK_DIAGNOSE_FAILED", e.to_string()))?;
    Ok((
        serde_json::json!({
            "status": "ok",
            "database": crate::core::schemas::LOCAL_DB_NAME,
            "diagnosis": diagnosis,
            "automatic_repair": false,
        }),
        Some("<repo>/.decapod/data/decapod.db.lock".to_string()),
    ))
}

/// Execute `db.lock.release_own` (safe idle current-process lease release).
pub fn run_maintain_lock_release_own(
    policy: &CanonicalMaintenancePolicy,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    let report = StorageLock::release_owned_if_idle(&db_path)
        .map_err(|e| ("LOCK_RELEASE_OWN_REFUSED", e.to_string()))?;
    Ok((
        serde_json::json!({
            "status": "ok",
            "database": crate::core::schemas::LOCAL_DB_NAME,
            "release": report,
            "automatic_repair": false,
        }),
        Some("<repo>/.decapod/data/decapod.db.lock".to_string()),
    ))
}

/// Validate and resolve a bounded, non-existing backup destination inside the repository's
/// `.decapod/backups` (or worktree `.decapod/backups`) boundary, then run Dactyl online backup.
pub fn run_maintain_backup(
    policy: &CanonicalMaintenancePolicy,
    raw_destination: &Path,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    crate::core::dactyl::ensure_local_sqlite_runtime()
        .map_err(|e| ("SQLITE_RUNTIME_UNAVAILABLE", e.to_string()))?;
    let resolved_dest = validate_agent_backup_destination(policy, raw_destination)?;
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    if !db_path.is_file() {
        return Err((
            "BACKUP_SOURCE_MISSING",
            "canonical datastore .decapod/data/decapod.db does not exist".to_string(),
        ));
    }
    let source_meta =
        fs::symlink_metadata(&db_path).map_err(|e| ("BACKUP_SOURCE_UNREADABLE", e.to_string()))?;
    if source_meta.len() > MAX_AGENT_BACKUP_SOURCE_BYTES {
        return Err((
            "BACKUP_RESOURCE_BUDGET_EXCEEDED",
            format!(
                "source database size {} bytes exceeds agent backup ceiling {} bytes",
                source_meta.len(),
                MAX_AGENT_BACKUP_SOURCE_BYTES
            ),
        ));
    }

    let bridge = crate::core::dactyl::DactylBridge::open_canonical(
        &policy.main_store_root,
        AccessMode::ReadOnly,
    )
    .map_err(|e| classify_dactyl_op_error("BACKUP_OPEN_FAILED", &e))?;
    let mut report = bridge
        .backup(&resolved_dest)
        .map_err(|e| classify_dactyl_op_error("BACKUP_FAILED", &e))?;
    let redacted_dest = redact_for_repo(&resolved_dest.to_string_lossy(), &policy.main_root);
    report.destination = redacted_dest.clone();

    Ok((
        serde_json::json!({
            "status": "ok",
            "diagnostic_status": "healthy",
            "operation": "backup",
            "database": crate::core::schemas::LOCAL_DB_NAME,
            "backup": report,
            "destination": redacted_dest,
            "grants_recovery_approval": false,
            "automatic_repair": false,
        }),
        Some(redacted_dest),
    ))
}

/// Execute bounded `db.lock.retry` over an authorized target capability (`inspect`, `verify`, `backup`),
/// re-verifying canonical policy before every attempt and failing closed on deadline exhaustion.
pub fn run_maintain_lock_retry(
    invocation_dir: &Path,
    spec: &LockRetrySpec,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    if spec.max_attempts == 0 || spec.max_attempts > MAX_RETRY_ATTEMPTS {
        return Err((
            "RETRY_BUDGET_INVALID",
            format!(
                "max_attempts {} must be within 1..={}",
                spec.max_attempts, MAX_RETRY_ATTEMPTS
            ),
        ));
    }
    if spec.initial_backoff_ms == 0 || spec.initial_backoff_ms > MAX_RETRY_INITIAL_BACKOFF_MS {
        return Err((
            "RETRY_BUDGET_INVALID",
            format!(
                "initial_backoff_ms {} must be within 1..={}",
                spec.initial_backoff_ms, MAX_RETRY_INITIAL_BACKOFF_MS
            ),
        ));
    }
    if spec.deadline_ms == 0 || spec.deadline_ms > MAX_RETRY_DEADLINE_MS {
        return Err((
            "RETRY_BUDGET_INVALID",
            format!(
                "deadline_ms {} must be within 1..={}",
                spec.deadline_ms, MAX_RETRY_DEADLINE_MS
            ),
        ));
    }

    let target_cap = spec.target.as_capability();
    if target_cap == DbMaintenanceCapability::Backup && spec.destination.is_none() {
        return Err((
            "BACKUP_DESTINATION_REQUIRED",
            "db.lock.retry with target=backup requires --destination".to_string(),
        ));
    }

    let started = Instant::now();
    let deadline = Duration::from_millis(spec.deadline_ms);
    let mut backoff_ms = spec.initial_backoff_ms;
    let mut attempts_run = 0u32;
    let mut last_contention = String::new();

    for attempt in 1..=spec.max_attempts {
        attempts_run = attempt;
        // Re-check canonical policy before EVERY attempt so reverting canonical opt-in
        // immediately stops in-flight retries and prevents privilege expansion.
        let fresh_policy = resolve_canonical_policy(invocation_dir)?;
        let (retry_dec, retry_code, retry_msg) = evaluate_capability_decision(
            &fresh_policy,
            DbMaintenanceCapability::LockRetry,
            spec.expected_policy_sha256.as_deref(),
        );
        if retry_dec != CapabilityDecision::Allowed {
            return Err((retry_code, retry_msg));
        }
        let (target_dec, target_code, target_msg) = evaluate_capability_decision(
            &fresh_policy,
            target_cap,
            spec.expected_policy_sha256.as_deref(),
        );
        if target_dec != CapabilityDecision::Allowed {
            return Err((target_code, target_msg));
        }

        let remaining_ms = deadline.saturating_sub(started.elapsed()).as_millis() as u64;
        if remaining_ms == 0 && attempt > 1 {
            return Err((
                "RETRY_DEADLINE_EXCEEDED",
                format!(
                    "contention persisted past {}ms deadline after {} attempt(s): {}",
                    spec.deadline_ms,
                    attempt - 1,
                    last_contention
                ),
            ));
        }

        let outcome = match spec.target {
            RetryTargetCapability::Inspect => run_maintain_inspect(&fresh_policy),
            RetryTargetCapability::Verify => {
                run_verify_with_bounded_lock(&fresh_policy, remaining_ms.max(25).min(350))
            }
            RetryTargetCapability::Backup => run_backup_with_bounded_lock(
                &fresh_policy,
                spec.destination.as_deref().unwrap(),
                remaining_ms.max(25).min(350),
            ),
        };

        match outcome {
            Ok((inner_value, evidence_ref)) => {
                return Ok((
                    serde_json::json!({
                        "status": "ok",
                        "target_capability": target_cap.as_str(),
                        "attempts": attempts_run,
                        "elapsed_ms": started.elapsed().as_millis() as u64,
                        "target_result": inner_value,
                        "automatic_repair": false,
                    }),
                    evidence_ref,
                ));
            }
            Err((code, detail)) if is_retryable_contention(code, &detail) => {
                last_contention = detail;
                if attempt == spec.max_attempts || started.elapsed() >= deadline {
                    return Err((
                        "RETRY_DEADLINE_EXCEEDED",
                        format!(
                            "contention exceeded bounded retry budget (attempts={}/{}, deadline_ms={}): {}",
                            attempts_run, spec.max_attempts, spec.deadline_ms, last_contention
                        ),
                    ));
                }
                let remaining = deadline.saturating_sub(started.elapsed());
                let sleep_for = Duration::from_millis(backoff_ms).min(remaining);
                if sleep_for.is_zero() {
                    return Err((
                        "RETRY_DEADLINE_EXCEEDED",
                        format!(
                            "contention exceeded {}ms deadline after {} attempt(s): {}",
                            spec.deadline_ms, attempts_run, last_contention
                        ),
                    ));
                }
                thread::sleep(sleep_for);
                backoff_ms = backoff_ms.saturating_mul(2).min(500);
            }
            Err(non_retryable) => return Err(non_retryable),
        }
    }

    Err((
        "RETRY_DEADLINE_EXCEEDED",
        format!(
            "contention exceeded bounded retry budget after {} attempt(s): {}",
            attempts_run, last_contention
        ),
    ))
}

fn run_verify_with_bounded_lock(
    policy: &CanonicalMaintenancePolicy,
    probe_budget_ms: u64,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    if !db_path.exists() {
        return run_maintain_verify(policy);
    }
    let diag = StorageLock::diagnose(&db_path, Duration::from_millis(probe_budget_ms))
        .map_err(|e| ("LOCK_DIAGNOSE_FAILED", e.to_string()))?;
    if diag.foreground_owner_active
        || diag.lock_state != crate::core::storage_lock::StorageLockContentionState::Unlocked
    {
        return Err((
            "STORAGE_LOCK_CONTENDED",
            format!(
                "datastore lock state is {:?} (owner={:?}, foreground_owner_active={})",
                diag.lock_state, diag.owner, diag.foreground_owner_active
            ),
        ));
    }
    run_maintain_verify(policy)
}

fn run_backup_with_bounded_lock(
    policy: &CanonicalMaintenancePolicy,
    destination: &Path,
    probe_budget_ms: u64,
) -> Result<(serde_json::Value, Option<String>), (&'static str, String)> {
    // Validate destination BEFORE checking lock contention so invalid/existing paths
    // fail immediately without retrying.
    let _ = validate_agent_backup_destination(policy, destination)?;
    let db_path = crate::core::events::canonical_db_path(&policy.main_store_root);
    if !db_path.exists() {
        return Err((
            "BACKUP_SOURCE_MISSING",
            "canonical datastore .decapod/data/decapod.db does not exist".to_string(),
        ));
    }
    let diag = StorageLock::diagnose(&db_path, Duration::from_millis(probe_budget_ms))
        .map_err(|e| ("LOCK_DIAGNOSE_FAILED", e.to_string()))?;
    if diag.foreground_owner_active
        || diag.lock_state != crate::core::storage_lock::StorageLockContentionState::Unlocked
    {
        return Err((
            "STORAGE_LOCK_CONTENDED",
            format!(
                "datastore lock state is {:?} (owner={:?}, foreground_owner_active={})",
                diag.lock_state, diag.owner, diag.foreground_owner_active
            ),
        ));
    }
    run_maintain_backup(policy, destination)
}

fn is_retryable_contention(code: &str, detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    code == "STORAGE_LOCK_CONTENDED"
        || lower.contains("storage_lock_timeout")
        || lower.contains("storage_owner_maintenance_blocked")
        || lower.contains("database is locked")
        || lower.contains("sqlite_busy")
}

fn classify_dactyl_op_error(
    default_code: &'static str,
    error: &DecapodError,
) -> (&'static str, String) {
    let text = error.to_string();
    let lower = text.to_ascii_lowercase();
    if lower.contains("storage_lock_timeout")
        || lower.contains("storage_owner_maintenance_blocked")
        || lower.contains("database is locked")
    {
        ("STORAGE_LOCK_CONTENDED", text)
    } else if lower.contains("not a database")
        || lower.contains("malformed")
        || lower.contains("corrupt")
    {
        ("DATABASE_CORRUPT", text)
    } else {
        (default_code, text)
    }
}

/// Validate that an agent-supplied backup destination is non-existing, free of traversal or
/// symlinks, and strictly inside `<main_root>/.decapod/backups`, `<main_root>/.decapod/data/backups`,
/// or `<workspace_root>/.decapod/backups`.
pub fn validate_agent_backup_destination(
    policy: &CanonicalMaintenancePolicy,
    raw_destination: &Path,
) -> Result<PathBuf, (&'static str, String)> {
    let raw_str = raw_destination.to_string_lossy();
    if raw_str.trim().is_empty() || raw_str.contains('\0') {
        return Err((
            "BACKUP_DESTINATION_UNSAFE",
            "backup destination path must not be empty or contain NUL bytes".to_string(),
        ));
    }
    for comp in raw_destination.components() {
        if matches!(comp, Component::ParentDir) {
            return Err((
                "BACKUP_DESTINATION_OUTSIDE_BOUNDARY",
                "backup destination must not contain '..' parent traversal components".to_string(),
            ));
        }
    }

    let candidate = if raw_destination.is_absolute() {
        raw_destination.to_path_buf()
    } else {
        policy.workspace_root.join(raw_destination)
    };

    let file_name = candidate
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| {
            (
                "BACKUP_DESTINATION_UNSAFE",
                "backup destination must name a file".to_string(),
            )
        })?;
    if file_name.starts_with('.')
        || file_name.ends_with(".lock")
        || file_name.ends_with("-wal")
        || file_name.ends_with("-shm")
        || !(file_name.ends_with(".db")
            || file_name.ends_with(".sqlite")
            || file_name.ends_with(".sqlite3")
            || file_name.ends_with(".bak"))
    {
        return Err((
            "BACKUP_DESTINATION_UNSAFE",
            "backup destination filename must be a non-hidden .db/.sqlite/.bak file and never a sidecar"
                .to_string(),
        ));
    }

    let allowed_roots = [
        policy.main_root.join(".decapod").join("backups"),
        policy
            .main_root
            .join(".decapod")
            .join("data")
            .join("backups"),
        policy.workspace_root.join(".decapod").join("backups"),
    ];

    let parent = candidate.parent().ok_or_else(|| {
        (
            "BACKUP_DESTINATION_UNSAFE",
            "backup destination must have a parent directory".to_string(),
        )
    })?;

    // Check if parent matches or is one level under one of the allowed `.decapod/backups` roots
    // before creating the directory if needed.
    let mut matched_allowed_root = None;
    for root in &allowed_roots {
        if parent == root || parent.starts_with(root) {
            matched_allowed_root = Some(root.clone());
            break;
        }
    }
    let Some(allowed_root) = matched_allowed_root else {
        return Err((
            "BACKUP_DESTINATION_OUTSIDE_BOUNDARY",
            "agent db.backup destination must reside under '.decapod/backups/' (never overwriting source files, active decapod.db, or paths outside the repository)"
                .to_string(),
        ));
    };

    reject_symlink_chain(&policy.main_root, parent)?;
    crate::core::fs_permissions::ensure_storage_dir(&allowed_root)
        .map_err(|e| ("BACKUP_DESTINATION_UNSAFE", e.to_string()))?;
    if parent != allowed_root {
        let rel = parent.strip_prefix(&allowed_root).map_err(|_| {
            (
                "BACKUP_DESTINATION_OUTSIDE_BOUNDARY",
                "backup destination escaped allowed backup directory".to_string(),
            )
        })?;
        if rel.components().count() > 2 {
            return Err((
                "BACKUP_DESTINATION_UNSAFE",
                "backup destination directory nesting exceeds bounded depth (max 2)".to_string(),
            ));
        }
        crate::core::fs_permissions::ensure_storage_dir(parent)
            .map_err(|e| ("BACKUP_DESTINATION_UNSAFE", e.to_string()))?;
    }

    let canonical_parent = parent
        .canonicalize()
        .map_err(|e| ("BACKUP_DESTINATION_UNSAFE", e.to_string()))?;
    let canonical_allowed = allowed_root
        .canonicalize()
        .map_err(|e| ("BACKUP_DESTINATION_UNSAFE", e.to_string()))?;
    if !canonical_parent.starts_with(&canonical_allowed) {
        return Err((
            "BACKUP_DESTINATION_OUTSIDE_BOUNDARY",
            "canonicalized backup parent escaped '.decapod/backups/' boundary".to_string(),
        ));
    }

    let resolved = canonical_parent.join(file_name);
    if fs::symlink_metadata(&resolved).is_ok() {
        return Err((
            "BACKUP_DESTINATION_EXISTS",
            "backup destination already exists; db.backup requires a fresh non-existing path"
                .to_string(),
        ));
    }

    Ok(resolved)
}

fn reject_symlink_chain(anchor: &Path, target: &Path) -> Result<(), (&'static str, String)> {
    let mut cur = PathBuf::new();
    for comp in target.components() {
        cur.push(comp.as_os_str());
        if cur.starts_with(anchor)
            && let Ok(meta) = fs::symlink_metadata(&cur)
            && meta.file_type().is_symlink()
        {
            return Err((
                "BACKUP_DESTINATION_UNSAFE",
                "symlink components are forbidden in agent backup destination paths".to_string(),
            ));
        }
    }
    Ok(())
}

/// Append a redacted maintenance audit record to `<main_store>/maintenance/audit.ndjson`.
pub fn append_audit_record(
    main_store_root: &Path,
    record: &DbMaintenanceAuditRecord,
) -> Result<(), DecapodError> {
    let maint_dir = main_store_root.join("maintenance");
    crate::core::fs_permissions::ensure_storage_dir(&maint_dir).map_err(DecapodError::IoError)?;
    let audit_path = maint_dir.join("audit.ndjson");
    let _guard = StorageLock::acquire(
        &audit_path,
        StorageLockMode::Exclusive,
        Duration::from_secs(2),
    )?;
    let mut file = crate::core::fs_permissions::open_storage_file(
        &audit_path,
        OpenOptions::new().create(true).append(true).read(true),
    )
    .map_err(DecapodError::IoError)?;
    let line = serde_json::to_string(record).map_err(|e| {
        DecapodError::ValidationError(format!("failed to encode maintenance audit record: {e}"))
    })?;
    writeln!(file, "{line}").map_err(DecapodError::IoError)?;
    file.sync_data().map_err(DecapodError::IoError)?;
    Ok(())
}

/// Read the most recent redacted maintenance audit records (`limit <= 500`).
pub fn read_audit_records(
    main_store_root: &Path,
    limit: usize,
) -> Result<Vec<DbMaintenanceAuditRecord>, DecapodError> {
    let capped = limit.clamp(1, 500);
    let audit_path = main_store_root.join("maintenance").join("audit.ndjson");
    if !audit_path.is_file() {
        return Ok(Vec::new());
    }
    let _guard =
        StorageLock::acquire(&audit_path, StorageLockMode::Shared, Duration::from_secs(2))?;
    let file =
        crate::core::fs_permissions::open_storage_file(&audit_path, OpenOptions::new().read(true))
            .map_err(DecapodError::IoError)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(DecapodError::IoError)?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: DbMaintenanceAuditRecord = serde_json::from_str(&line).map_err(|e| {
            DecapodError::ValidationError(format!("corrupt maintenance audit entry: {e}"))
        })?;
        records.push(parsed);
    }
    if records.len() > capped {
        records = records.split_off(records.len() - capped);
    }
    Ok(records)
}

fn has_completed_operation_id(
    main_store_root: &Path,
    operation_id: &str,
) -> Result<bool, DecapodError> {
    let audit_path = main_store_root.join("maintenance").join("audit.ndjson");
    if !audit_path.is_file() {
        return Ok(false);
    }
    let _guard =
        StorageLock::acquire(&audit_path, StorageLockMode::Shared, Duration::from_secs(2))?;
    let file =
        crate::core::fs_permissions::open_storage_file(&audit_path, OpenOptions::new().read(true))
            .map_err(DecapodError::IoError)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line.map_err(DecapodError::IoError)?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(parsed) = serde_json::from_str::<DbMaintenanceAuditRecord>(&line)
            && parsed.operation_id.as_deref() == Some(operation_id)
            && parsed.outcome == "succeeded"
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_operation_id(op_id: &str) -> Result<(), (&'static str, String)> {
    let trimmed = op_id.trim();
    if trimmed.is_empty()
        || trimmed != op_id
        || trimmed.len() > 128
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
    {
        return Err((
            "INVALID_OPERATION_ID",
            "operation_id must be 1..=128 ASCII alphanumeric/`-`/`_`/`:`/`.` characters"
                .to_string(),
        ));
    }
    Ok(())
}

fn read_regular_policy_bytes(path: &Path) -> Result<Vec<u8>, (&'static str, String)> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        (
            "CANONICAL_POLICY_MISSING",
            format!(
                "unable to stat '{}': {e}",
                redact_secrets(&path.to_string_lossy())
            ),
        )
    })?;
    if meta.file_type().is_symlink() || !meta.file_type().is_file() {
        return Err((
            "INVALID_CANONICAL_POLICY",
            "policy config.toml must be a regular file and never a symlink".to_string(),
        ));
    }
    if meta.len() > MAX_CANONICAL_POLICY_BYTES {
        return Err((
            "INVALID_CANONICAL_POLICY",
            format!(
                "policy config.toml size {} exceeds {} byte ceiling",
                meta.len(),
                MAX_CANONICAL_POLICY_BYTES
            ),
        ));
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let file: File = opts.open(path).map_err(|e| {
        (
            "INVALID_CANONICAL_POLICY",
            format!("unable to open policy config.toml safely: {e}"),
        )
    })?;
    let mut buf = Vec::new();
    file.take(MAX_CANONICAL_POLICY_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|e| {
            (
                "INVALID_CANONICAL_POLICY",
                format!("unable to read policy config.toml: {e}"),
            )
        })?;
    if buf.len() as u64 > MAX_CANONICAL_POLICY_BYTES {
        return Err((
            "INVALID_CANONICAL_POLICY",
            "policy config.toml exceeded 1 MiB while reading".to_string(),
        ));
    }
    Ok(buf)
}

fn find_project_root_upwards(start: &Path) -> Option<PathBuf> {
    let mut cur = start.to_path_buf();
    loop {
        if cur.join(".decapod").join("config.toml").exists() {
            return Some(cur);
        }
        if !cur.pop() {
            return None;
        }
    }
}

fn derive_repo_identity(main_root: &Path, cfg: &DecapodProjectConfig) -> String {
    if let Some(url) = git_stdout(main_root, &["config", "--get", "remote.origin.url"])
        && let Some(clean) = sanitize_remote_identity(&url)
    {
        return clean;
    }
    if let Some(ref name) = cfg.repo.product_name
        && !name.trim().is_empty()
    {
        return name.trim().to_string();
    }
    main_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repository")
        .to_string()
}

fn sanitize_remote_identity(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches(".git");
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        return Some(format!("github:{rest}"));
    }
    if let Some(idx) = trimmed.find("://") {
        let after_scheme = &trimmed[idx + 3..];
        // Strip any embedded userinfo (`user:token@host/org/repo`)!
        let host_and_path = after_scheme
            .rsplit_once('@')
            .map(|(_, hp)| hp)
            .unwrap_or(after_scheme);
        return Some(host_and_path.to_string());
    }
    None
}

fn is_valid_agent_identifier(agent_id: &str) -> bool {
    let trimmed = agent_id.trim();
    if trimmed.is_empty()
        || trimmed != agent_id
        || trimmed.len() > 128
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "unknown" | "operator" | "cli" | "human" | "root" | "admin"
        )
    {
        return false;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn sanitize_agent_branch_segment(agent: &str) -> String {
    agent
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

fn extract_worktree_task_id(worktree_path: &Path, branch: &str) -> Option<String> {
    let gov_path = worktree_path.join(".decapod").join("governance.json");
    if let Ok(raw) = fs::read_to_string(&gov_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw)
    {
        if let Some(tid) = val
            .pointer("/active/trajectory/task_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some(tid.to_string());
        }
        if let Some(tid) = val
            .pointer("/active/plan/todo_ids/0")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some(tid.to_string());
        }
    }
    // Extract `<cat>_<16chars>` from branch segment if present.
    let last = branch.rsplit('/').next().unwrap_or(branch);
    if last.len() >= 21 {
        let prefix = &last[..21];
        let bytes = prefix.as_bytes();
        if bytes[..4].iter().all(|b| b.is_ascii_lowercase())
            && bytes[4] == b'_'
            && bytes[5..21].iter().all(|b| b.is_ascii_alphanumeric())
        {
            return Some(prefix.to_string());
        }
    }
    None
}

fn check_task_custody_if_unlocked(
    main_store_root: &Path,
    task_id: &str,
    agent_id: &str,
) -> Result<(), (&'static str, String)> {
    let db_path = crate::core::events::canonical_db_path(main_store_root);
    if !db_path.is_file() {
        return Ok(());
    }
    let Ok(diag) = StorageLock::diagnose(&db_path, Duration::ZERO) else {
        return Ok(());
    };
    if diag.foreground_owner_active
        || diag.lock_state
            == crate::core::storage_lock::StorageLockContentionState::ExclusiveMaintenanceActive
    {
        // Do not block lock diagnosis or retry when exclusive contention is active.
        return Ok(());
    }
    let Ok(conn) = crate::core::db::db_connect_read_pooled(&db_path.to_string_lossy(), 1) else {
        // Corrupt or unavailable SQLite must not block read-only health/lock diagnosis.
        return Ok(());
    };
    if let Ok(assigned) = conn.query_row(
        "SELECT COALESCE(assigned_to, '') FROM tasks WHERE id = ?1",
        [task_id],
        |row| row.get::<_, String>(0),
    ) && !assigned.trim().is_empty()
        && assigned != agent_id
    {
        return Err((
            "TASK_CUSTODY_MISMATCH",
            format!("task '{task_id}' is currently assigned to '{assigned}', not '{agent_id}'"),
        ));
    }
    Ok(())
}

fn normalize_sha256(value: &str) -> &str {
    value.trim().strip_prefix("sha256:").unwrap_or(value.trim())
}

fn backend_label(backend: BackendType) -> &'static str {
    match backend {
        BackendType::Local => "local",
        BackendType::Cloud => "cloud",
    }
}

/// Redact secrets and normalize host filesystem paths relative to `<repo>` / `<home>`.
pub fn redact_for_repo(input: &str, main_root: &Path) -> String {
    let mut out = redact_secrets(input);
    let repo_str = main_root.to_string_lossy();
    if !repo_str.is_empty() && repo_str != "/" {
        out = out.replace(repo_str.as_ref(), "<repo>");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
        && home != "/"
    {
        out = out.replace(&home, "<home>");
    }
    out
}

pub fn redact_secrets(input: &str) -> String {
    let mut out = input.to_string();
    if let Ok(pw) = std::env::var("DECAPOD_SESSION_PASSWORD")
        && !pw.is_empty()
    {
        out = out.replace(&pw, "[REDACTED]");
    }
    for prefix in ["sbp_", "eyJhbGci", "local_01", "cloud_01"] {
        while let Some(idx) = out.find(prefix) {
            let end = out[idx..]
                .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ')' | ']'))
                .map(|len| idx + len)
                .unwrap_or(out.len());
            out.replace_range(idx..end, "[REDACTED]");
        }
    }
    out
}

fn git_stdout(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn git_bytes(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_denies_all_capabilities_and_omits_serialization() {
        let cfg = DbMaintenanceConfig::default();
        assert!(cfg.is_default());
        assert!(cfg.effective_safe_allowlist().is_empty());

        let policy = CanonicalMaintenancePolicy {
            main_root: PathBuf::from("/tmp/repo"),
            workspace_root: PathBuf::from("/tmp/repo/.decapod/workspaces/wt"),
            main_store_root: PathBuf::from("/tmp/repo/.decapod/data"),
            repo_identity: "repo".to_string(),
            backend: BackendType::Local,
            policy_sha256: "sha256:abc".to_string(),
            canonical_revision: "deadbeef:clean".to_string(),
            config: cfg,
            unmerged_branch_violation: None,
        };

        for cap in DbMaintenanceCapability::ALL {
            let (dec, code, _) = evaluate_capability_decision(&policy, cap, None);
            assert_eq!(dec, CapabilityDecision::Denied);
            if cap == DbMaintenanceCapability::Recover {
                assert_eq!(code, "OWNER_REPAIR_AUTHORITY_REQUIRED");
            } else {
                assert_eq!(code, "POLICY_OPT_IN_DISABLED");
            }
        }
    }

    #[test]
    fn enabled_default_allowlist_grants_safe_five_and_denies_recover_and_unmodeled() {
        let cfg = DbMaintenanceConfig {
            enabled: true,
            allowed: None,
        };
        let allow = cfg.effective_safe_allowlist();
        assert!(allow.contains(&DbMaintenanceCapability::Inspect));
        assert!(allow.contains(&DbMaintenanceCapability::Verify));
        assert!(allow.contains(&DbMaintenanceCapability::LockDiagnose));
        assert!(allow.contains(&DbMaintenanceCapability::LockRetry));
        assert!(allow.contains(&DbMaintenanceCapability::Backup));
        assert!(!allow.contains(&DbMaintenanceCapability::LockReleaseOwn));
        assert!(!allow.contains(&DbMaintenanceCapability::Recover));

        // Even if `db.recover` is explicitly listed in `allowed`, owner tier blocks it!
        let explicit_with_recover = DbMaintenanceConfig {
            enabled: true,
            allowed: Some(vec![
                DbMaintenanceCapability::Verify,
                DbMaintenanceCapability::LockReleaseOwn,
                DbMaintenanceCapability::Recover,
            ]),
        };
        let policy = CanonicalMaintenancePolicy {
            main_root: PathBuf::from("/tmp/repo"),
            workspace_root: PathBuf::from("/tmp/repo/.decapod/workspaces/wt"),
            main_store_root: PathBuf::from("/tmp/repo/.decapod/data"),
            repo_identity: "repo".to_string(),
            backend: BackendType::Local,
            policy_sha256: "sha256:def".to_string(),
            canonical_revision: "deadbeef:clean".to_string(),
            config: explicit_with_recover,
            unmerged_branch_violation: None,
        };
        assert_eq!(
            evaluate_capability_decision(&policy, DbMaintenanceCapability::Verify, None).0,
            CapabilityDecision::Allowed
        );
        assert_eq!(
            evaluate_capability_decision(&policy, DbMaintenanceCapability::LockReleaseOwn, None).0,
            CapabilityDecision::Allowed
        );
        let (rec_dec, rec_code, _) =
            evaluate_capability_decision(&policy, DbMaintenanceCapability::Recover, None);
        assert_eq!(rec_dec, CapabilityDecision::Denied);
        assert_eq!(rec_code, "OWNER_REPAIR_AUTHORITY_REQUIRED");

        let sql = evaluate_named_capability(&policy, "db.sql", None);
        assert_eq!(sql.decision, CapabilityDecision::Unsupported);
        assert_eq!(sql.reason_code, "CAPABILITY_UNSUPPORTED");
    }

    #[test]
    fn cloud_backend_always_denies_even_when_opt_in_enabled() {
        let policy = CanonicalMaintenancePolicy {
            main_root: PathBuf::from("/tmp/repo"),
            workspace_root: PathBuf::from("/tmp/repo/.decapod/workspaces/wt"),
            main_store_root: PathBuf::from("/tmp/repo/.decapod/data"),
            repo_identity: "repo".to_string(),
            backend: BackendType::Cloud,
            policy_sha256: "sha256:123".to_string(),
            canonical_revision: "deadbeef:clean".to_string(),
            config: DbMaintenanceConfig {
                enabled: true,
                allowed: None,
            },
            unmerged_branch_violation: None,
        };
        for cap in DbMaintenanceCapability::ALL {
            let (dec, code, _) = evaluate_capability_decision(&policy, cap, None);
            assert_eq!(dec, CapabilityDecision::Denied);
            assert_eq!(code, "CLOUD_PRINCIPAL_UNVERIFIED");
        }
    }

    #[test]
    fn stale_expected_policy_sha256_is_rejected() {
        let policy = CanonicalMaintenancePolicy {
            main_root: PathBuf::from("/tmp/repo"),
            workspace_root: PathBuf::from("/tmp/repo/.decapod/workspaces/wt"),
            main_store_root: PathBuf::from("/tmp/repo/.decapod/data"),
            repo_identity: "repo".to_string(),
            backend: BackendType::Local,
            policy_sha256: "sha256:actual111".to_string(),
            canonical_revision: "deadbeef:clean".to_string(),
            config: DbMaintenanceConfig {
                enabled: true,
                allowed: None,
            },
            unmerged_branch_violation: None,
        };
        let (dec, code, _) = evaluate_capability_decision(
            &policy,
            DbMaintenanceCapability::Verify,
            Some("sha256:stale222"),
        );
        assert_eq!(dec, CapabilityDecision::Denied);
        assert_eq!(code, "POLICY_SNAPSHOT_STALE");
    }

    #[test]
    fn redaction_strips_repo_paths_and_tokens() {
        let root = Path::new("/Users/example/secret-repo");
        let raw = "failed at /Users/example/secret-repo/.decapod/data/decapod.db with token sbp_secret12345 and local_01M7XYZ";
        let redacted = redact_for_repo(raw, root);
        assert!(!redacted.contains("/Users/example/secret-repo"));
        assert!(redacted.contains("<repo>/.decapod/data/decapod.db"));
        assert!(!redacted.contains("sbp_secret12345"));
        assert!(!redacted.contains("local_01M7XYZ"));
    }
}

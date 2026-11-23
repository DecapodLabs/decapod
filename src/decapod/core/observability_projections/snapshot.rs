//! Consistent repository and worktree observation snapshots across Git and
//! SQLite, plus bounded live workspace inventory and immutable source anchors
//! (#1714, #1720, #1727, #1728).

use crate::core::bounded_process::{BoundedCommand, CONTROL_TIMEOUT};
use crate::core::db::{self, Connection};
use crate::core::error::DecapodError;
use crate::core::events;
use crate::core::governance_document::{self, GOVERNANCE_PATH};
use crate::core::path_policy;
use crate::core::time;
use crate::core::trace;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const OBSERVE_SCHEMA_VERSION: &str = "decapod.observe.v1";
pub const DEFAULT_MAX_INVENTORY_FILES: usize = 512;
pub const HARD_MAX_INVENTORY_FILES: usize = 2_048;
pub const DEFAULT_MAX_ANCHOR_BYTES: usize = 64 * 1024;
pub const HARD_MAX_ANCHOR_BYTES: usize = 256 * 1024;
pub const MAX_HASHED_FILE_BYTES: u64 = 4 * 1024 * 1024;
const READER_LOCK_TIMEOUT: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryIdentity {
    pub repo_id: String,
    pub root_name: String,
    pub base_branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeIdentity {
    pub is_worktree: bool,
    pub worktree_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_tree: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PinnedBaseline {
    pub base_branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_main_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_main_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_base_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_tree: Option<String>,
    pub baseline_moved_vs_change_base: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SqliteRevisionAnchor {
    pub db_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_event_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_count: Option<u64>,
    pub query_only_enforced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SnapshotConsistency {
    pub consistent: bool,
    /// Explicitly declares non-atomic cross-store capture; never claims atomic
    /// reads across Git + working files + SQLite.
    pub atomicity: String,
    pub git_head_moved: bool,
    pub git_index_changed: bool,
    pub governance_doc_changed: bool,
    pub sqlite_revision_changed: bool,
    pub worktree_missing: bool,
    pub partial_read: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inconsistent_reasons: Vec<String>,
    pub retry_recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObservationSnapshot {
    pub schema_version: String,
    pub snapshot_id: String,
    pub observed_at: String,
    pub read_only: bool,
    pub authoritative: bool,
    pub repository: RepositoryIdentity,
    pub worktree: WorktreeIdentity,
    pub baseline: PinnedBaseline,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_index_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub governance_doc_sha256: Option<String>,
    pub sqlite_revision: SqliteRevisionAnchor,
    pub consistency: SnapshotConsistency,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CaptureMark {
    pub worktree_present: bool,
    pub head_commit: Option<String>,
    pub git_index_sha256: Option<String>,
    pub governance_doc_sha256: Option<String>,
    pub sqlite: SqliteRevisionAnchor,
}

/// Read-only task row projection from SQLite `tasks` table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRecordView {
    pub id: String,
    pub title: String,
    pub status: String,
    pub owner: String,
    pub assigned_to: String,
    pub priority: String,
    pub scope: String,
    pub dir_path: String,
    pub depends_on: Vec<String>,
    pub blocks: Vec<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<String>,
    pub lease_generation: i64,
    pub lease_lifecycle: String,
    pub intent_anchor: String,
    pub revision: i64,
}

/// One file in the bounded live workspace inventory (#1728).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceFileEntry {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub is_binary: bool,
    pub is_generated: bool,
    pub is_secret_sensitive: bool,
    pub symlink_blocked: bool,
    pub case_collision_warning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LiveWorkspaceInventory {
    pub snapshot_id: String,
    pub total_observed: usize,
    pub returned_count: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
    pub truncated: bool,
    pub staged_count: usize,
    pub unstaged_count: usize,
    pub untracked_count: usize,
    pub renamed_count: usize,
    pub deleted_count: usize,
    pub secret_blocked_count: usize,
    pub symlink_blocked_count: usize,
    pub entries: Vec<WorkspaceFileEntry>,
}

/// Immutable source anchor tied to exact file SHA-256 and snapshot ID (#1728).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnchor {
    pub schema_version: String,
    pub path: String,
    pub snapshot_id: String,
    pub file_sha256: String,
    pub start_line: usize,
    pub end_line: usize,
    pub total_lines: usize,
    pub slice_sha256: String,
    pub content: String,
    pub truncated: bool,
    pub redacted: bool,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnchorFreshness {
    pub path: String,
    pub expected_file_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_file_sha256: Option<String>,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

/// Bounded read-only loader for raw `.decapod/governance.json` JSON so
/// integrity checks can still expose tampered trajectory/receipt hashes when
/// strict schema validation rejects the file (#1713, #1720).
pub(crate) fn load_raw_governance_json_readonly(repo_root: &Path) -> Option<serde_json::Value> {
    let path = repo_root.join(GOVERNANCE_PATH);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() || meta.len() > 16 * 1024 * 1024 {
        return None;
    }
    let bytes = fs::read(&path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Resolve the `.decapod/data` root for read-only SQLite reads without creating
/// any directories or mutating state.
pub fn resolve_data_root_readonly(repo_root: &Path) -> PathBuf {
    let local_db = repo_root.join(".decapod/data/decapod.db");
    if local_db.exists() {
        return repo_root.join(".decapod/data");
    }
    if let Ok(main_root) = crate::core::workspace::get_main_repo_root(repo_root) {
        let main_data = main_root.join(".decapod/data");
        if main_data.join("decapod.db").exists() {
            return main_data;
        }
    }
    repo_root.join(".decapod/data")
}

pub(crate) fn run_git_readonly(repo_root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    if !repo_root.exists() {
        return None;
    }
    let output = Command::new("git")
        .current_dir(repo_root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("--no-optional-locks")
        .args(args)
        .bounded_output(CONTROL_TIMEOUT)
        .ok()?;
    if output.status.success() {
        Some(output.stdout)
    } else {
        None
    }
}

pub(crate) fn run_git_stdout_trimmed(repo_root: &Path, args: &[&str]) -> Option<String> {
    let bytes = run_git_readonly(repo_root, args)?;
    let text = String::from_utf8_lossy(&bytes).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn resolve_git_index_path(repo_root: &Path) -> Option<PathBuf> {
    let raw = run_git_stdout_trimmed(repo_root, &["rev-parse", "--git-path", "index"])?;
    let path = PathBuf::from(&raw);
    if path.is_absolute() {
        Some(path)
    } else {
        Some(repo_root.join(path))
    }
}

fn hash_file_bounded(path: &Path, max_bytes: u64) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.file_type().is_file() || meta.len() > max_bytes {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(sha256_hex(&bytes))
}

fn open_sqlite_readonly(data_root: &Path) -> Result<Option<Connection>, String> {
    let db_path = events::canonical_db_path(data_root);
    if !db_path.exists() {
        return Ok(None);
    }
    let db_str = db_path.to_string_lossy().to_string();
    match db::db_connect_readonly_bounded(&db_str, READER_LOCK_TIMEOUT) {
        Ok(conn) => {
            // Explicitly assert SQLite query_only on this connection.
            let _ = conn.execute_batch("PRAGMA query_only = ON;");
            Ok(Some(conn))
        }
        Err(err) => Err(err.to_string()),
    }
}

pub(crate) fn capture_sqlite_revision(data_root: &Path) -> SqliteRevisionAnchor {
    match open_sqlite_readonly(data_root) {
        Ok(None) => SqliteRevisionAnchor {
            db_present: false,
            data_version: None,
            max_event_seq: None,
            todo_count: None,
            query_only_enforced: true,
            read_error: None,
        },
        Ok(Some(conn)) => {
            let data_version = conn
                .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
                .ok();
            let max_event_seq = conn
                .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |row| {
                    row.get::<_, u64>(0)
                })
                .ok();
            let todo_count = conn
                .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get::<_, u64>(0))
                .ok();
            let query_only = conn
                .query_row("PRAGMA query_only", [], |row| row.get::<_, i64>(0))
                .map(|v| v != 0)
                .unwrap_or(true);
            SqliteRevisionAnchor {
                db_present: true,
                data_version,
                max_event_seq,
                todo_count,
                query_only_enforced: query_only,
                read_error: None,
            }
        }
        Err(err) => SqliteRevisionAnchor {
            db_present: true,
            data_version: None,
            max_event_seq: None,
            todo_count: None,
            query_only_enforced: true,
            read_error: Some(err),
        },
    }
}

pub(crate) fn capture_mark(repo_root: &Path, data_root: &Path) -> CaptureMark {
    let worktree_present = repo_root.exists() && repo_root.is_dir();
    let head_commit = if worktree_present {
        run_git_stdout_trimmed(repo_root, &["rev-parse", "HEAD"])
    } else {
        None
    };
    let git_index_sha256 = if worktree_present {
        resolve_git_index_path(repo_root).and_then(|idx| hash_file_bounded(&idx, 32 * 1024 * 1024))
    } else {
        None
    };
    let governance_doc_sha256 = if worktree_present {
        hash_file_bounded(&repo_root.join(GOVERNANCE_PATH), 16 * 1024 * 1024)
    } else {
        None
    };
    let sqlite = capture_sqlite_revision(data_root);
    CaptureMark {
        worktree_present,
        head_commit,
        git_index_sha256,
        governance_doc_sha256,
        sqlite,
    }
}

pub fn compute_repo_id(repo_root: &Path) -> String {
    let root_commit = run_git_stdout_trimmed(repo_root, &["rev-list", "--max-parents=0", "HEAD"])
        .and_then(|s| s.lines().next().map(str::to_owned));
    let remote = run_git_stdout_trimmed(repo_root, &["remote", "get-url", "origin"]).map(|u| {
        // Strip any credentials defensively.
        if let Some((scheme, rest)) = u.split_once("://") {
            let clean = rest.rsplit_once('@').map_or(rest, |(_, r)| r);
            format!("{scheme}://{clean}")
        } else {
            u
        }
    });
    let fallback_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());
    let material = format!(
        "{}|{}|{}",
        root_commit.as_deref().unwrap_or("unborn"),
        remote.as_deref().unwrap_or("local"),
        fallback_name
    );
    format!("repo_{}", &sha256_hex(material.as_bytes())[7..23])
}

pub(crate) fn finalize_snapshot(
    repo_root: &Path,
    pre: &CaptureMark,
    post: &CaptureMark,
) -> ObservationSnapshot {
    let config = crate::cli::DecapodProjectConfig::load(repo_root).ok();
    let base_branch = config
        .as_ref()
        .and_then(|c| c.repo.base_branch.clone())
        .filter(|b| !b.trim().is_empty())
        .or_else(|| {
            if run_git_stdout_trimmed(repo_root, &["rev-parse", "--verify", "master"]).is_some() {
                Some("master".to_string())
            } else {
                Some("main".to_string())
            }
        })
        .unwrap_or_else(|| "main".to_string());

    let product_name = config
        .as_ref()
        .and_then(|c| c.repo.product_name.clone())
        .filter(|s| !s.trim().is_empty());
    let root_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string());
    let repo_id = compute_repo_id(repo_root);

    let git_dir = run_git_stdout_trimmed(repo_root, &["rev-parse", "--git-dir"]);
    let common_dir = run_git_stdout_trimmed(repo_root, &["rev-parse", "--git-common-dir"]);
    let is_worktree = match (&git_dir, &common_dir) {
        (Some(gd), Some(cd)) => gd != cd,
        _ => false,
    };
    let branch = run_git_stdout_trimmed(repo_root, &["symbolic-ref", "--short", "HEAD"]);
    let head_tree = run_git_stdout_trimmed(repo_root, &["rev-parse", "HEAD^{tree}"]);

    let local_main_sha = run_git_stdout_trimmed(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{base_branch}"),
        ],
    );
    let remote_main_sha = run_git_stdout_trimmed(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/remotes/origin/{base_branch}"),
        ],
    );
    let pinned_ref = if remote_main_sha.is_some() {
        Some(format!("refs/remotes/origin/{base_branch}"))
    } else if local_main_sha.is_some() {
        Some(format!("refs/heads/{base_branch}"))
    } else {
        None
    };
    let pinned_commit = remote_main_sha.clone().or_else(|| local_main_sha.clone());
    let pinned_tree = pinned_commit.as_deref().and_then(|sha| {
        run_git_stdout_trimmed(repo_root, &["rev-parse", &format!("{sha}^{{tree}}")])
    });
    let merge_base_sha = pinned_commit
        .as_deref()
        .and_then(|target| run_git_stdout_trimmed(repo_root, &["merge-base", "HEAD", target]));

    let change_base_commit = governance_document::load(repo_root)
        .ok()
        .flatten()
        .and_then(|doc| doc.change.map(|c| c.base_commit));
    let baseline_moved_vs_change_base = match (&change_base_commit, &pinned_commit) {
        (Some(recorded), Some(current)) => recorded != current,
        _ => false,
    };

    let git_head_moved = pre.head_commit != post.head_commit;
    let git_index_changed = pre.git_index_sha256 != post.git_index_sha256;
    let governance_doc_changed = pre.governance_doc_sha256 != post.governance_doc_sha256;
    let sqlite_revision_changed = pre.sqlite.db_present != post.sqlite.db_present
        || pre.sqlite.max_event_seq != post.sqlite.max_event_seq
        || pre.sqlite.todo_count != post.sqlite.todo_count
        || pre.sqlite.data_version != post.sqlite.data_version;
    let worktree_missing = !pre.worktree_present || !post.worktree_present;
    let partial_read = pre.sqlite.read_error.is_some() || post.sqlite.read_error.is_some();

    let mut inconsistent_reasons = Vec::new();
    if worktree_missing {
        inconsistent_reasons.push("worktree_missing_or_deleted_during_observation".to_string());
    }
    if git_head_moved {
        inconsistent_reasons.push(format!(
            "git_head_moved_during_read: pre={:?} post={:?}",
            pre.head_commit, post.head_commit
        ));
    }
    if git_index_changed {
        inconsistent_reasons.push("git_index_changed_during_read".to_string());
    }
    if governance_doc_changed {
        inconsistent_reasons.push("governance_json_changed_during_read".to_string());
    }
    if sqlite_revision_changed {
        inconsistent_reasons.push("sqlite_revision_changed_during_read".to_string());
    }
    if partial_read {
        inconsistent_reasons.push(format!(
            "sqlite_partial_read: {}",
            post.sqlite
                .read_error
                .as_deref()
                .or(pre.sqlite.read_error.as_deref())
                .unwrap_or("unavailable")
        ));
    }

    let consistent = inconsistent_reasons.is_empty();
    let snapshot_material = format!(
        "{}|{}|{}|{}|{}|{}|{:?}|{:?}",
        OBSERVE_SCHEMA_VERSION,
        repo_id,
        pinned_commit.as_deref().unwrap_or("none"),
        post.head_commit.as_deref().unwrap_or("unborn"),
        post.git_index_sha256.as_deref().unwrap_or("no_index"),
        post.governance_doc_sha256.as_deref().unwrap_or("no_gov"),
        post.sqlite.max_event_seq,
        post.sqlite.todo_count,
    );
    let snapshot_id = sha256_hex(snapshot_material.as_bytes());

    ObservationSnapshot {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        snapshot_id,
        observed_at: time::now_epoch_z(),
        read_only: true,
        authoritative: false,
        repository: RepositoryIdentity {
            repo_id,
            root_name,
            base_branch: base_branch.clone(),
            product_name,
        },
        worktree: WorktreeIdentity {
            is_worktree,
            worktree_present: post.worktree_present,
            branch,
            head_commit: post.head_commit.clone(),
            head_tree,
        },
        baseline: PinnedBaseline {
            base_branch,
            local_main_sha,
            remote_main_sha,
            merge_base_sha,
            pinned_ref,
            pinned_commit,
            pinned_tree,
            baseline_moved_vs_change_base,
        },
        git_index_sha256: post.git_index_sha256.clone(),
        governance_doc_sha256: post.governance_doc_sha256.clone(),
        sqlite_revision: post.sqlite.clone(),
        consistency: SnapshotConsistency {
            consistent,
            atomicity: "best_effort_double_read".to_string(),
            git_head_moved,
            git_index_changed,
            governance_doc_changed,
            sqlite_revision_changed,
            worktree_missing,
            partial_read,
            inconsistent_reasons,
            retry_recommended: !consistent,
        },
    }
}

/// Capture an observation snapshot executing `mid_read` between the pre-read
/// and post-read marks so any concurrent Git/SQLite/governance mutation is
/// deterministically detected (#1727).
pub fn capture_observation_with<T, F>(
    repo_root: &Path,
    mid_read: F,
) -> Result<(ObservationSnapshot, T), DecapodError>
where
    F: FnOnce(&Path, &Path) -> Result<T, DecapodError>,
{
    let data_root = resolve_data_root_readonly(repo_root);
    let pre = capture_mark(repo_root, &data_root);
    let value = mid_read(repo_root, &data_root)?;
    let post = capture_mark(repo_root, &data_root);
    let snapshot = finalize_snapshot(repo_root, &pre, &post);
    Ok((snapshot, value))
}

/// Read tasks from `.decapod/data/decapod.db` strictly read-only without
/// blocking writers.
pub fn read_tasks_readonly(data_root: &Path, limit: usize) -> Vec<TaskRecordView> {
    let Ok(Some(conn)) = open_sqlite_readonly(data_root) else {
        return Vec::new();
    };
    let capped = limit.clamp(1, 1024);
    let mut stmt = match conn.prepare(
        "SELECT id, title, status, COALESCE(owner, ''), COALESCE(assigned_to, ''),
                COALESCE(priority, 'medium'), COALESCE(scope, ''), COALESCE(dir_path, ''),
                COALESCE(depends_on, ''), COALESCE(blocks, ''), COALESCE(updated_at, ''),
                completed_at, lease_expires_at, COALESCE(lease_generation, 0),
                COALESCE(lease_lifecycle, ''), COALESCE(intent_anchor, ''), COALESCE(revision, 0)
         FROM tasks
         ORDER BY id ASC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = match stmt.query_map([capped as i64], |row| {
        let deps_raw: String = row.get(8)?;
        let blocks_raw: String = row.get(9)?;
        Ok(TaskRecordView {
            id: row.get(0)?,
            title: row.get(1)?,
            status: row.get(2)?,
            owner: row.get(3)?,
            assigned_to: row.get(4)?,
            priority: row.get(5)?,
            scope: row.get(6)?,
            dir_path: row.get(7)?,
            depends_on: split_csv(&deps_raw),
            blocks: split_csv(&blocks_raw),
            updated_at: row.get(10)?,
            completed_at: row.get(11)?,
            lease_expires_at: row.get(12)?,
            lease_generation: row.get(13)?,
            lease_lifecycle: row.get(14)?,
            intent_anchor: row.get(15)?,
            revision: row.get(16)?,
        })
    }) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    rows.filter_map(Result::ok).collect()
}

fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Sensitive / secret file path classifier (#1712, #1720, #1728).
pub fn is_secret_sensitive_path(rel_path: &str) -> bool {
    let lower = rel_path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&lower)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    if lower == ".git"
        || lower.starts_with(".git/")
        || lower.starts_with(".decapod/managed/sessions/")
        || lower.starts_with(".decapod/data/broker-runtime/")
    {
        return true;
    }
    if file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name.ends_with(".env")
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name.ends_with(".p12")
        || file_name.ends_with(".pfx")
        || file_name.ends_with(".jks")
        || file_name == "id_rsa"
        || file_name == "id_ed25519"
        || file_name == "id_ecdsa"
        || file_name == ".npmrc"
        || file_name == ".netrc"
        || file_name == ".pypirc"
        || file_name.contains("credentials")
        || file_name.contains("secret")
        || file_name.contains("private_key")
        || file_name.contains("token.json")
    {
        return true;
    }
    false
}

/// Generated artifact classifier (#1728).
pub fn is_generated_artifact_path(rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    norm.ends_with(".lock")
        || norm == "Cargo.lock"
        || norm.starts_with(".decapod/managed/.manifest.json")
        || norm.starts_with(".decapod/managed/specs/.manifest.json")
        || norm.starts_with("target/")
        || norm.starts_with("dist/")
        || norm.contains("/generated/")
        || norm.ends_with(".min.js")
        || norm.ends_with(".min.css")
}

/// Validate a caller-supplied relative path against traversal, absolute paths,
/// NUL bytes, and symlink escapes at EVERY path component (#1712, #1720, #1728).
pub fn validate_repo_bounded_path(
    repo_root: &Path,
    raw_rel: &str,
) -> Result<(String, PathBuf), DecapodError> {
    if raw_rel.trim().is_empty() || raw_rel.contains('\0') {
        return Err(DecapodError::ValidationError(
            "PATH_BOUNDARY_VIOLATION: empty or NUL path rejected".to_string(),
        ));
    }
    let candidate = Path::new(raw_rel);
    if candidate.is_absolute() {
        return Err(DecapodError::ValidationError(
            "PATH_BOUNDARY_VIOLATION: absolute paths are not permitted".to_string(),
        ));
    }
    let mut normalized_parts = Vec::new();
    let mut full = repo_root.to_path_buf();
    for component in candidate.components() {
        match component {
            Component::Normal(seg) => {
                let s = seg.to_string_lossy().to_string();
                normalized_parts.push(s);
                full.push(seg);
                match fs::symlink_metadata(&full) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        return Err(DecapodError::ValidationError(format!(
                            "SYMLINK_BOUNDARY_VIOLATION: symlink component rejected at '{}'",
                            normalized_parts.join("/")
                        )));
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(DecapodError::IoError(e)),
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DecapodError::ValidationError(
                    "PATH_BOUNDARY_VIOLATION: path traversal outside repository root rejected"
                        .to_string(),
                ));
            }
        }
    }
    if normalized_parts.is_empty() {
        return Err(DecapodError::ValidationError(
            "PATH_BOUNDARY_VIOLATION: empty normalized path rejected".to_string(),
        ));
    }
    let normalized = normalized_parts.join("/");
    if let (Ok(canon_root), Ok(canon_target)) =
        (fs::canonicalize(repo_root), fs::canonicalize(&full))
        && !canon_target.starts_with(&canon_root)
    {
        return Err(DecapodError::ValidationError(
            "PATH_BOUNDARY_VIOLATION: resolved path escapes repository root".to_string(),
        ));
    }
    Ok((normalized, full))
}

fn open_regular_nofollow(path: &Path) -> Result<fs::File, DecapodError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| {
                if e.raw_os_error() == Some(libc::ELOOP) {
                    DecapodError::ValidationError(
                        "SYMLINK_BOUNDARY_VIOLATION: symlink target rejected".to_string(),
                    )
                } else {
                    DecapodError::IoError(e)
                }
            })?;
        let meta = file.metadata()?;
        if !meta.file_type().is_file() {
            return Err(DecapodError::ValidationError(
                "PATH_BOUNDARY_VIOLATION: expected a regular file".to_string(),
            ));
        }
        Ok(file)
    }
    #[cfg(not(unix))]
    {
        let meta = fs::symlink_metadata(path)?;
        if !meta.file_type().is_file() {
            return Err(DecapodError::ValidationError(
                "PATH_BOUNDARY_VIOLATION: expected a regular file".to_string(),
            ));
        }
        Ok(fs::File::open(path)?)
    }
}

fn read_bounded_bytes(path: &Path, max_bytes: usize) -> Result<(Vec<u8>, bool), DecapodError> {
    let mut file = open_regular_nofollow(path)?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(buf.len());
        if n > remaining {
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok((buf, truncated))
}

/// Read an immutable, bounded, repo-safe source anchor slice (#1728, #1712, #1720).
pub fn read_source_anchor(
    repo_root: &Path,
    snapshot: &ObservationSnapshot,
    raw_rel_path: &str,
    start_line: usize,
    end_line: usize,
    max_bytes: usize,
) -> Result<SourceAnchor, DecapodError> {
    let (rel_path, full_path) = validate_repo_bounded_path(repo_root, raw_rel_path)?;
    if is_secret_sensitive_path(&rel_path) {
        return Err(DecapodError::ValidationError(format!(
            "SECRET_PATH_DENIED: refusing to read sensitive file '{rel_path}'"
        )));
    }
    if !full_path.exists() {
        return Err(DecapodError::ValidationError(format!(
            "ANCHOR_NOT_FOUND: file '{rel_path}' does not exist"
        )));
    }
    let meta = fs::symlink_metadata(&full_path)?;
    if meta.len() > MAX_HASHED_FILE_BYTES {
        return Err(DecapodError::ValidationError(format!(
            "ANCHOR_FILE_TOO_LARGE: '{rel_path}' ({} bytes) exceeds {} byte cap",
            meta.len(),
            MAX_HASHED_FILE_BYTES
        )));
    }
    let bounded_cap = max_bytes.clamp(256, HARD_MAX_ANCHOR_BYTES);
    let (all_bytes, _) = read_bounded_bytes(&full_path, MAX_HASHED_FILE_BYTES as usize)?;
    if all_bytes.iter().take(8192).any(|b| *b == 0) {
        return Err(DecapodError::ValidationError(format!(
            "BINARY_FILE_DENIED: refusing text source anchor for binary file '{rel_path}'"
        )));
    }
    let file_sha256 = sha256_hex(&all_bytes);
    let text = String::from_utf8(all_bytes).map_err(|_| {
        DecapodError::ValidationError(format!(
            "NON_UTF8_FILE_DENIED: file '{rel_path}' is not valid UTF-8"
        ))
    })?;
    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();
    let start = start_line.max(1);
    let end = if end_line == 0 {
        total_lines.max(1)
    } else {
        end_line.max(start).min(total_lines.max(1))
    };

    let mut slice = String::new();
    let mut truncated = false;
    if total_lines > 0 && start <= total_lines {
        for (idx, line) in lines[(start - 1)..end].iter().enumerate() {
            let mut next = String::new();
            if idx > 0 {
                next.push('\n');
            }
            next.push_str(line);
            if slice.len() + next.len() > bounded_cap {
                let take = bounded_cap.saturating_sub(slice.len());
                let safe_prefix: String = next.chars().take(take).collect();
                slice.push_str(&safe_prefix);
                truncated = true;
                break;
            }
            slice.push_str(&next);
        }
    }

    let redacted_text = path_policy::redact_text(repo_root, &trace::redact_string(&slice));
    let redacted = redacted_text != slice;
    let slice_sha256 = sha256_hex(redacted_text.as_bytes());

    // Post-check file hash to catch concurrent editing while reading!
    let post_hash = hash_file_bounded(&full_path, MAX_HASHED_FILE_BYTES);
    let (stale, stale_reason) = if post_hash.as_deref() != Some(file_sha256.as_str()) {
        (
            true,
            Some("file_modified_concurrently_during_anchor_read".to_string()),
        )
    } else {
        (false, None)
    };

    Ok(SourceAnchor {
        schema_version: OBSERVE_SCHEMA_VERSION.to_string(),
        path: rel_path,
        snapshot_id: snapshot.snapshot_id.clone(),
        file_sha256,
        start_line: start,
        end_line: end,
        total_lines,
        slice_sha256,
        content: redacted_text,
        truncated,
        redacted,
        stale,
        stale_reason,
    })
}

/// Verify whether a previously issued immutable source anchor is still fresh
/// against the live file on disk (#1728).
pub fn verify_anchor_freshness(
    repo_root: &Path,
    anchor: &SourceAnchor,
) -> Result<AnchorFreshness, DecapodError> {
    let (_, full_path) = validate_repo_bounded_path(repo_root, &anchor.path)?;
    if !full_path.exists() {
        return Ok(AnchorFreshness {
            path: anchor.path.clone(),
            expected_file_sha256: anchor.file_sha256.clone(),
            current_file_sha256: None,
            stale: true,
            reason: Some("file_deleted_or_missing".to_string()),
        });
    }
    let current = hash_file_bounded(&full_path, MAX_HASHED_FILE_BYTES);
    let stale = current.as_deref() != Some(anchor.file_sha256.as_str());
    Ok(AnchorFreshness {
        path: anchor.path.clone(),
        expected_file_sha256: anchor.file_sha256.clone(),
        current_file_sha256: current,
        stale,
        reason: if stale {
            Some("file_hash_changed_since_anchor_capture".to_string())
        } else {
            None
        },
    })
}

fn git_blob_sha256(repo_root: &Path, spec: &str) -> Option<String> {
    let bytes = run_git_readonly(repo_root, &["cat-file", "blob", spec])?;
    if bytes.len() as u64 > MAX_HASHED_FILE_BYTES {
        return None;
    }
    Some(sha256_hex(&bytes))
}

/// Enumerate bounded tracked-changed, staged, unstaged, untracked, renamed,
/// and deleted files in the live workspace with exact SHA-256 hashes (#1728).
pub fn enumerate_live_workspace_inventory(
    repo_root: &Path,
    snapshot_id: &str,
    offset: usize,
    limit: usize,
) -> LiveWorkspaceInventory {
    let capped_limit = limit.clamp(1, HARD_MAX_INVENTORY_FILES);
    let raw = run_git_readonly(repo_root, &["status", "--porcelain=v1", "-z", "-uall"])
        .unwrap_or_default();

    // Parse NUL-terminated `git status --porcelain=v1 -z` entries.
    let mut parsed: Vec<(char, char, String, Option<String>)> = Vec::new();
    let tokens: Vec<&[u8]> = raw.split(|b| *b == 0).filter(|t| !t.is_empty()).collect();
    let mut idx = 0;
    while idx < tokens.len() {
        let tok = String::from_utf8_lossy(tokens[idx]).to_string();
        if tok.len() >= 4 {
            let bytes = tok.as_bytes();
            let x = bytes[0] as char;
            let y = bytes[1] as char;
            let path = tok[3..].to_string();
            let old_path = if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') {
                idx += 1;
                tokens
                    .get(idx)
                    .map(|t| String::from_utf8_lossy(t).to_string())
            } else {
                None
            };
            parsed.push((x, y, path, old_path));
        }
        idx += 1;
    }

    // Deterministic sort by canonical path.
    parsed.sort_by(|a, b| a.2.cmp(&b.2));

    // Detect case-insensitive collisions across observed paths.
    let mut lower_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, _, path, _) in &parsed {
        *lower_counts.entry(path.to_ascii_lowercase()).or_insert(0) += 1;
    }

    let total_observed = parsed.len();
    let mut staged_count = 0usize;
    let mut unstaged_count = 0usize;
    let mut untracked_count = 0usize;
    let mut renamed_count = 0usize;
    let mut deleted_count = 0usize;
    let mut secret_blocked_count = 0usize;
    let mut symlink_blocked_count = 0usize;

    let mut all_entries = Vec::with_capacity(total_observed.min(HARD_MAX_INVENTORY_FILES));
    let mut seen_paths = BTreeSet::new();

    for (x, y, path, old_path) in parsed {
        if !seen_paths.insert(path.clone()) {
            continue;
        }
        let is_untracked = x == '?' && y == '?';
        let is_renamed = matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C');
        let is_deleted = x == 'D' || y == 'D';
        let is_staged = !is_untracked && x != ' ';
        let is_unstaged = !is_untracked && y != ' ';

        if is_staged {
            staged_count += 1;
        }
        if is_unstaged {
            unstaged_count += 1;
        }
        if is_untracked {
            untracked_count += 1;
        }
        if is_renamed {
            renamed_count += 1;
        }
        if is_deleted {
            deleted_count += 1;
        }

        let category = if is_untracked {
            "untracked"
        } else if is_renamed {
            "renamed"
        } else if is_deleted {
            "deleted"
        } else if is_staged && is_unstaged {
            "staged_and_unstaged"
        } else if is_staged {
            "staged"
        } else {
            "unstaged"
        }
        .to_string();

        let is_secret_sensitive = is_secret_sensitive_path(&path);
        let is_generated = is_generated_artifact_path(&path);
        let case_collision_warning = lower_counts
            .get(&path.to_ascii_lowercase())
            .copied()
            .unwrap_or(0)
            > 1;

        let path_check = validate_repo_bounded_path(repo_root, &path);
        let (symlink_blocked, full_path) = match path_check {
            Ok((_, full)) => (false, Some(full)),
            Err(err) => (err.to_string().contains("SYMLINK"), None),
        };
        if is_secret_sensitive {
            secret_blocked_count += 1;
        }
        if symlink_blocked {
            symlink_blocked_count += 1;
        }

        let mut working_sha256 = None;
        let mut index_sha256 = None;
        let mut head_sha256 = None;
        let mut size_bytes = None;
        let mut is_binary = false;
        let mut omitted_reason = None;

        if symlink_blocked {
            omitted_reason = Some("symlink_blocked".to_string());
        } else if is_secret_sensitive {
            omitted_reason = Some("secret_sensitive_path".to_string());
        } else {
            if let Some(full) = full_path
                && full.exists()
                && let Ok(meta) = fs::symlink_metadata(&full)
                && meta.file_type().is_file()
            {
                size_bytes = Some(meta.len());
                if meta.len() > MAX_HASHED_FILE_BYTES {
                    omitted_reason = Some("exceeds_hash_byte_cap".to_string());
                } else if let Ok((bytes, _)) =
                    read_bounded_bytes(&full, MAX_HASHED_FILE_BYTES as usize)
                {
                    is_binary = bytes.iter().take(8192).any(|b| *b == 0);
                    working_sha256 = Some(sha256_hex(&bytes));
                }
            }
            if !is_untracked {
                index_sha256 = git_blob_sha256(repo_root, &format!(":{path}"));
                let head_target = old_path.as_deref().unwrap_or(path.as_str());
                head_sha256 = git_blob_sha256(repo_root, &format!("HEAD:{head_target}"));
            }
        }

        all_entries.push(WorkspaceFileEntry {
            path,
            old_path,
            index_status: x.to_string(),
            worktree_status: y.to_string(),
            category,
            working_sha256,
            index_sha256,
            head_sha256,
            size_bytes,
            is_binary,
            is_generated,
            is_secret_sensitive,
            symlink_blocked,
            case_collision_warning,
            omitted_reason,
        });
    }

    let start = offset.min(all_entries.len());
    let end = (start + capped_limit).min(all_entries.len());
    let entries = all_entries[start..end].to_vec();
    let has_more = end < all_entries.len();

    LiveWorkspaceInventory {
        snapshot_id: snapshot_id.to_string(),
        total_observed,
        returned_count: entries.len(),
        offset: start,
        limit: capped_limit,
        has_more,
        truncated: has_more,
        staged_count,
        unstaged_count,
        untracked_count,
        renamed_count,
        deleted_count,
        secret_blocked_count,
        symlink_blocked_count,
        entries,
    }
}

//! Reversible repository integration preview, bounded removal, and restoration (#1704).
//!
//! Design invariants:
//! - `preview` is strictly read-only: zero filesystem, Git, SQLite, session, or network mutations.
//! - `remove` requires explicit `--authorize`, refuses symlink replacement, active custody
//!   conflicts, and uncertain ownership (fail-closed, non-zero exit), and deletes only positively
//!   owned, unchanged generated integration files and owned `.gitignore` rules.
//! - Edited, authored, shared, and unknown resources, plus `.decapod/data/`,
//!   `.decapod/governance.json`, `*.bak`, and global config/credentials are preserved by default.
//! - An inspectable restoration manifest (`.decapod/integration-restore-manifest.json`) is
//!   written atomically before removing generated files so interrupted or repeated removal and
//!   restoration remain idempotent and lossless.
//! - Permanent project state deletion and protected-governance removal remain separate explicit
//!   authority boundaries that this module refuses.

use crate::core::capsule_policy::{GENERATED_POLICY_REL_PATH, default_policy_json_pretty};
use crate::core::error::DecapodError;
use crate::core::project_specs::{
    LOCAL_PROJECT_SPECS_DIR, LOCAL_PROJECT_SPECS_MANIFEST, read_specs_manifest,
};
use crate::core::scaffold::DECAPOD_GITIGNORE_RULES;
use crate::core::storage_lock::{StorageLock, StorageLockContentionState};
use crate::core::{assets, atomic, dactyl_db, entrypoint_integrity, fs_permissions, schemas, time};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

pub const INTEGRATION_OWNERSHIP_PATH: &str = ".decapod/managed/integration_ownership.json";
pub const INTEGRATION_RESTORE_MANIFEST_PATH: &str = ".decapod/integration-restore-manifest.json";
pub const OWNERSHIP_SCHEMA_VERSION: &str = "1.0.0";
pub const RESTORE_MANIFEST_SCHEMA_VERSION: &str = "1.0.0";
const SUPPORTED_RESTORE_SCHEMA_VERSIONS: &[&str] = &["0.9.0", "1.0.0"];
const MAX_SCAN_DEPTH: usize = 12;
const MAX_SCAN_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceClass {
    Generated,
    Edited,
    Authored,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipRecord {
    pub path: String,
    pub kind: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    pub generated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationOwnershipManifest {
    pub schema_version: String,
    pub decapod_release: String,
    pub recorded_at: String,
    #[serde(default)]
    pub created_gitignore: bool,
    #[serde(default)]
    pub gitignore_rules: Vec<String>,
    #[serde(default)]
    pub owned_files: BTreeMap<String, OwnershipRecord>,
    #[serde(default)]
    pub pre_existing_files: Vec<String>,
}

impl Default for IntegrationOwnershipManifest {
    fn default() -> Self {
        Self {
            schema_version: OWNERSHIP_SCHEMA_VERSION.to_string(),
            decapod_release: env!("CARGO_PKG_VERSION").to_string(),
            recorded_at: time::now_epoch_z(),
            created_gitignore: false,
            gitignore_rules: Vec::new(),
            owned_files: BTreeMap::new(),
            pre_existing_files: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoredFileEntry {
    pub path: String,
    pub kind: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    #[serde(default)]
    pub content_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoredGitignoreState {
    pub file_existed: bool,
    pub remove_created_file: bool,
    #[serde(default)]
    pub removed_rules: Vec<String>,
    #[serde(default)]
    pub original_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationRestoreManifest {
    pub schema_version: String,
    pub status: String,
    pub decapod_release: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub files: Vec<RestoredFileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitignore: Option<RestoredGitignoreState>,
    #[serde(default)]
    pub preserved_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifiedResource {
    pub path: String,
    pub kind: String,
    pub classification: ResourceClass,
    pub ownership_evidence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    pub diagnostic: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_entries: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preserved_entries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveResourceConflict {
    pub kind: String,
    pub path_or_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovalBlocker {
    pub code: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorityBoundaries {
    pub bounded_integration_removal: String,
    pub permanent_state_deletion: String,
    pub protected_governance_removal: String,
    pub retained_state_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationPreviewReport {
    pub schema_version: String,
    pub mode: String,
    pub repository_root: String,
    pub ownership_manifest_present: bool,
    pub restore_manifest_present: bool,
    pub can_apply: bool,
    pub generated: Vec<ClassifiedResource>,
    pub edited: Vec<ClassifiedResource>,
    pub authored: Vec<ClassifiedResource>,
    pub unknown: Vec<ClassifiedResource>,
    pub active: Vec<ActiveResourceConflict>,
    pub blockers: Vec<RemovalBlocker>,
    pub authority_boundaries: AuthorityBoundaries,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationApplyReport {
    pub schema_version: String,
    pub status: String,
    pub repository_root: String,
    pub authorized: bool,
    pub idempotent_replay: bool,
    pub restore_manifest_path: String,
    pub removed_paths: Vec<String>,
    pub gitignore_rules_removed: Vec<String>,
    pub preserved_paths: Vec<String>,
    pub blockers: Vec<RemovalBlocker>,
    pub authority_boundaries: AuthorityBoundaries,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationRestoreReport {
    pub schema_version: String,
    pub status: String,
    pub repository_root: String,
    pub manifest_path: String,
    pub idempotent_replay: bool,
    pub restored_paths: Vec<String>,
    pub gitignore_rules_restored: Vec<String>,
    pub skipped_matching_paths: Vec<String>,
    pub conflicts: Vec<RemovalBlocker>,
    pub authority_boundaries: AuthorityBoundaries,
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(hex: &str) -> Result<Vec<u8>, DecapodError> {
    if !hex.len().is_multiple_of(2) {
        return Err(DecapodError::ValidationError(
            "RESTORE_MANIFEST_INVALID: odd hex length in manifest payload".into(),
        ));
    }
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let hi = hex_nibble(pair[0])?;
        let lo = hex_nibble(pair[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> Result<u8, DecapodError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(DecapodError::ValidationError(
            "RESTORE_MANIFEST_INVALID: non-hex byte in manifest payload".into(),
        )),
    }
}

#[cfg(unix)]
fn file_mode(meta: &fs::Metadata) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(meta.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode(_meta: &fs::Metadata) -> Option<u32> {
    None
}

#[cfg(unix)]
fn apply_file_mode(path: &Path, mode: Option<u32>) -> Result<(), DecapodError> {
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(DecapodError::IoError)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn apply_file_mode(_path: &Path, _mode: Option<u32>) -> Result<(), DecapodError> {
    Ok(())
}

fn ensure_relative_safe(rel: &str) -> Result<(), DecapodError> {
    let path = Path::new(rel);
    if rel.is_empty()
        || rel.contains('\\')
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(DecapodError::ValidationError(format!(
            "INTEGRATION_PATH_UNSAFE: unsupported relative path '{rel}'"
        )));
    }
    Ok(())
}

/// Check symlink metadata for every component of `rel` under `root`.
/// Returns `Ok(Some(meta))` for the leaf if it exists, `Ok(None)` if absent,
/// or `Err` if any traversed component or leaf is a symlink.
fn inspect_no_symlink(root: &Path, rel: &str) -> Result<Option<fs::Metadata>, DecapodError> {
    ensure_relative_safe(rel)?;
    let mut cur = root.to_path_buf();
    let mut last = None;
    for comp in Path::new(rel).components() {
        cur.push(comp);
        match fs::symlink_metadata(&cur) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(DecapodError::ValidationError(format!(
                        "INTEGRATION_SYMLINK_REFUSED: symlink detected at '{}'",
                        cur.strip_prefix(root).unwrap_or(&cur).display()
                    )));
                }
                last = Some(meta);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(DecapodError::IoError(err)),
        }
    }
    Ok(last)
}

fn validate_target_root(raw_dir: Option<PathBuf>) -> Result<PathBuf, DecapodError> {
    let dir = match raw_dir {
        Some(d) => d,
        None => std::env::current_dir().map_err(DecapodError::IoError)?,
    };
    let meta = fs::symlink_metadata(&dir).map_err(DecapodError::IoError)?;
    if meta.file_type().is_symlink() {
        return Err(DecapodError::ValidationError(format!(
            "INTEGRATION_SYMLINK_REFUSED: target root '{}' is a symlink",
            dir.display()
        )));
    }
    if !meta.is_dir() {
        return Err(DecapodError::ValidationError(format!(
            "INTEGRATION_TARGET_INVALID: '{}' is not a directory",
            dir.display()
        )));
    }
    fs::canonicalize(&dir).map_err(DecapodError::IoError)
}

/// Record ownership of scaffolded integration artifacts during `decapod init`.
pub fn record_init_ownership(
    repo_root: &Path,
    generated_entrypoints: &[String],
    generated_ci: bool,
    created_gitignore: bool,
    pre_existing_files: &[String],
) -> Result<(), DecapodError> {
    let manifest_path = repo_root.join(INTEGRATION_OWNERSHIP_PATH);
    let mut manifest = if manifest_path.exists() {
        read_ownership_manifest(repo_root)
            .ok()
            .flatten()
            .unwrap_or_default()
    } else {
        IntegrationOwnershipManifest::default()
    };
    manifest.schema_version = OWNERSHIP_SCHEMA_VERSION.to_string();
    manifest.decapod_release = env!("CARGO_PKG_VERSION").to_string();
    manifest.recorded_at = time::now_epoch_z();
    if created_gitignore {
        manifest.created_gitignore = true;
    }
    manifest.gitignore_rules = DECAPOD_GITIGNORE_RULES
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    for existing in pre_existing_files {
        if !manifest.pre_existing_files.contains(existing) {
            manifest.pre_existing_files.push(existing.clone());
        }
    }

    let mut candidate_paths: Vec<(&str, String)> = Vec::new();
    for ep in generated_entrypoints {
        candidate_paths.push(("entrypoint", ep.clone()));
    }
    candidate_paths.push(("config", ".decapod/README.md".to_string()));
    candidate_paths.push(("config", ".decapod/OVERRIDE.md".to_string()));
    candidate_paths.push(("config", ".decapod/config.toml".to_string()));
    candidate_paths.push((
        "dockerfile",
        ".decapod/managed/Dockerfile.decapod".to_string(),
    ));
    candidate_paths.push(("policy", GENERATED_POLICY_REL_PATH.to_string()));
    candidate_paths.push((
        "metadata",
        ".decapod/managed/version_counter.json".to_string(),
    ));
    candidate_paths.push(("metadata", ".decapod/managed/override.checksum".to_string()));
    if generated_ci {
        candidate_paths.push((
            "workflow",
            ".github/workflows/decapod-validate.yml".to_string(),
        ));
    }

    for (kind, rel) in candidate_paths {
        if let Ok(Some(meta)) = inspect_no_symlink(repo_root, &rel)
            && meta.is_file()
            && !manifest.pre_existing_files.contains(&rel)
            && let Ok(bytes) = fs::read(repo_root.join(&rel))
        {
            if rel == ".decapod/OVERRIDE.md"
                && let Some(tmpl) = assets::get_template("OVERRIDE.md")
                && bytes != tmpl.as_bytes()
            {
                continue;
            }
            if rel == ".github/workflows/decapod-validate.yml"
                && let Some(tmpl) = assets::get_template("decapod-validate.yml")
                && bytes != tmpl.as_bytes()
            {
                continue;
            }
            manifest.owned_files.insert(
                rel.clone(),
                OwnershipRecord {
                    path: rel,
                    kind: kind.to_string(),
                    sha256: sha256_bytes(&bytes),
                    mode: file_mode(&meta),
                    generated_by: "decapod-init".to_string(),
                },
            );
        }
    }

    write_ownership_manifest(repo_root, &manifest)
}

/// Record or clear git-hook ownership provenance when `decapod setup hook` runs.
pub fn record_hook_ownership(
    repo_root: &Path,
    hook_name: &str,
    hook_path: &Path,
    installed: bool,
) -> Result<(), DecapodError> {
    if !repo_root.join(".decapod").is_dir() {
        return Ok(());
    }
    let rel = format!(".git/hooks/{hook_name}");
    let mut manifest = read_ownership_manifest(repo_root)
        .ok()
        .flatten()
        .unwrap_or_default();
    manifest.recorded_at = time::now_epoch_z();
    if installed {
        let meta = fs::symlink_metadata(hook_path).map_err(DecapodError::IoError)?;
        let bytes = fs::read(hook_path).map_err(DecapodError::IoError)?;
        manifest.owned_files.insert(
            rel.clone(),
            OwnershipRecord {
                path: rel,
                kind: "hook".to_string(),
                sha256: sha256_bytes(&bytes),
                mode: file_mode(&meta),
                generated_by: "decapod-setup-hook".to_string(),
            },
        );
    } else {
        manifest.owned_files.remove(&rel);
    }
    write_ownership_manifest(repo_root, &manifest)
}

fn write_ownership_manifest(
    repo_root: &Path,
    manifest: &IntegrationOwnershipManifest,
) -> Result<(), DecapodError> {
    let path = repo_root.join(INTEGRATION_OWNERSHIP_PATH);
    if let Some(parent) = path.parent() {
        fs_permissions::ensure_private_dir(parent).map_err(DecapodError::IoError)?;
    }
    let body = serde_json::to_string_pretty(manifest)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    atomic::write_atomic(&path, body.as_bytes()).map_err(DecapodError::IoError)?;
    Ok(())
}

fn read_ownership_manifest(
    repo_root: &Path,
) -> Result<Option<IntegrationOwnershipManifest>, DecapodError> {
    let Some(meta) = inspect_no_symlink(repo_root, INTEGRATION_OWNERSHIP_PATH)? else {
        return Ok(None);
    };
    if !meta.is_file() {
        return Err(DecapodError::ValidationError(
            "INTEGRATION_UNCERTAIN_OWNERSHIP: ownership manifest is not a regular file".into(),
        ));
    }
    let raw = fs::read_to_string(repo_root.join(INTEGRATION_OWNERSHIP_PATH))
        .map_err(DecapodError::IoError)?;
    let manifest: IntegrationOwnershipManifest = serde_json::from_str(&raw).map_err(|e| {
        DecapodError::ValidationError(format!(
            "INTEGRATION_UNCERTAIN_OWNERSHIP: corrupt ownership manifest: {e}"
        ))
    })?;
    if manifest.schema_version != OWNERSHIP_SCHEMA_VERSION {
        return Err(DecapodError::ValidationError(format!(
            "INTEGRATION_UNCERTAIN_OWNERSHIP: unsupported ownership manifest schema_version '{}'",
            manifest.schema_version
        )));
    }
    Ok(Some(manifest))
}

fn read_restore_manifest_at(
    repo_root: &Path,
    manifest_path: &Path,
) -> Result<Option<IntegrationRestoreManifest>, DecapodError> {
    if let Ok(rel) = manifest_path.strip_prefix(repo_root)
        && let Some(rel_str) = rel.to_str()
        && inspect_no_symlink(repo_root, rel_str)?.is_none()
    {
        return Ok(None);
    }
    let meta = match fs::symlink_metadata(manifest_path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(DecapodError::IoError(err)),
    };
    if meta.file_type().is_symlink() {
        return Err(DecapodError::ValidationError(format!(
            "INTEGRATION_SYMLINK_REFUSED: restore manifest '{}' is a symlink",
            manifest_path.display()
        )));
    }
    if !meta.is_file() {
        return Err(DecapodError::ValidationError(format!(
            "RESTORE_MANIFEST_INVALID: '{}' is not a regular file",
            manifest_path.display()
        )));
    }
    let raw = fs::read_to_string(manifest_path).map_err(DecapodError::IoError)?;
    let manifest: IntegrationRestoreManifest = serde_json::from_str(&raw).map_err(|e| {
        DecapodError::ValidationError(format!(
            "RESTORE_MANIFEST_INVALID: failed to parse '{}': {e}",
            manifest_path.display()
        ))
    })?;
    if !SUPPORTED_RESTORE_SCHEMA_VERSIONS.contains(&manifest.schema_version.as_str()) {
        return Err(DecapodError::ValidationError(format!(
            "RESTORE_MANIFEST_UNSUPPORTED_VERSION: schema_version '{}' is unsupported",
            manifest.schema_version
        )));
    }
    Ok(Some(manifest))
}

fn is_valid_rendered_entrypoint(surface: &str, content: &str) -> bool {
    let mut lines = content.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    let second = lines.next().unwrap_or("");
    let offset = first.len() + second.len();
    let payload = &content[offset..];

    let Some(release) = extract_comment_marker(first, "<!-- decapod-release:") else {
        return false;
    };
    let Some(declared_fp) = extract_comment_marker(second, "<!-- decapod-fingerprint:") else {
        return false;
    };
    if !is_valid_semver(&release) {
        return false;
    }

    let matches_canonical = entrypoint_integrity::render_entrypoint_for_release(surface, &release)
        .is_some_and(|rendered| rendered == content);
    if matches_canonical {
        return true;
    }

    if let Some(prev_body) = assets::previous_prompt_safety_template(surface) {
        let fp_input = format!(
            "decapod-entrypoint:{surface}\n<!-- decapod-release: {release} -->\n<!-- decapod-fingerprint: <fingerprint> -->\n{prev_body}"
        );
        let expected_fp = entrypoint_integrity::fingerprint(&fp_input);
        return payload == prev_body && declared_fp == expected_fp;
    }

    false
}

fn extract_comment_marker(line: &str, prefix: &str) -> Option<String> {
    let trimmed = line.trim_end_matches(['\r', '\n']).trim();
    let rest = trimmed.strip_prefix(prefix)?;
    let val = rest.strip_suffix("-->")?.trim();
    (!val.is_empty()).then(|| val.to_string())
}

fn is_valid_semver(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !(p.is_empty() || (p.len() > 1 && p.starts_with('0')))
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u64>().is_ok()
        })
}

fn has_decapod_entrypoint_markers(content: &str) -> bool {
    content.contains("<!-- decapod-release:") || content.contains("<!-- decapod-fingerprint:")
}

fn write_repo_file(repo_root: &Path, rel: &str, bytes: &[u8]) -> Result<(), DecapodError> {
    let abs = repo_root.join(rel);
    if rel.starts_with(".decapod/") {
        atomic::write_atomic(&abs, bytes).map_err(DecapodError::IoError)
    } else {
        if let Some(parent) = abs.parent() {
            fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
        }
        fs::write(&abs, bytes).map_err(DecapodError::IoError)
    }
}

fn collect_symlink_blockers_under(
    root: &Path,
    rel_dir: &str,
    depth: usize,
    seen: &mut usize,
    active: &mut Vec<ActiveResourceConflict>,
    blockers: &mut Vec<RemovalBlocker>,
) {
    if depth > MAX_SCAN_DEPTH || *seen >= MAX_SCAN_ENTRIES {
        return;
    }
    let dir_path = root.join(rel_dir);
    let Ok(meta) = fs::symlink_metadata(&dir_path) else {
        return;
    };
    if meta.file_type().is_symlink() {
        active.push(ActiveResourceConflict {
            kind: "symlink".into(),
            path_or_id: rel_dir.to_string(),
            detail: format!("Symlink detected at '{rel_dir}'"),
        });
        blockers.push(RemovalBlocker {
            code: "INTEGRATION_SYMLINK_REFUSED".into(),
            path: rel_dir.to_string(),
            detail: format!("Refusing symlink replacement at '{rel_dir}'"),
        });
        return;
    }
    if !meta.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(&dir_path) else {
        return;
    };
    for entry in entries.flatten() {
        *seen += 1;
        if *seen > MAX_SCAN_ENTRIES {
            break;
        }
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let child_rel = format!("{rel_dir}/{name_str}");
        match fs::symlink_metadata(entry.path()) {
            Ok(cmeta) if cmeta.file_type().is_symlink() => {
                active.push(ActiveResourceConflict {
                    kind: "symlink".into(),
                    path_or_id: child_rel.clone(),
                    detail: format!("Symlink detected at '{child_rel}'"),
                });
                blockers.push(RemovalBlocker {
                    code: "INTEGRATION_SYMLINK_REFUSED".into(),
                    path: child_rel,
                    detail: "Refusing symlink inside managed integration directory".into(),
                });
            }
            Ok(cmeta) if cmeta.is_dir() => {
                collect_symlink_blockers_under(root, &child_rel, depth + 1, seen, active, blockers);
            }
            _ => {}
        }
    }
}

fn check_git_hooks_dir_readonly(repo_root: &Path) -> Option<(PathBuf, bool)> {
    let dot_git = repo_root.join(".git");
    let Ok(meta) = fs::symlink_metadata(&dot_git) else {
        return None;
    };
    if meta.file_type().is_symlink() {
        return Some((dot_git, true));
    }
    if meta.is_dir() {
        let hooks = dot_git.join("hooks");
        if let Ok(hmeta) = fs::symlink_metadata(&hooks)
            && hmeta.file_type().is_symlink()
        {
            return Some((hooks, true));
        }
        return Some((hooks, false));
    }
    // Worktree `.git` file: resolve via read-only `git rev-parse --git-dir`
    let out = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "rev-parse", "--git-dir"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let git_dir_raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let git_dir = if Path::new(&git_dir_raw).is_absolute() {
        PathBuf::from(git_dir_raw)
    } else {
        repo_root.join(git_dir_raw)
    };
    Some((git_dir.join("hooks"), false))
}

fn inspect_active_custody(
    repo_root: &Path,
    active: &mut Vec<ActiveResourceConflict>,
    blockers: &mut Vec<RemovalBlocker>,
) {
    // 1. Check .decapod/workspaces/ directory for active or dirty worktrees
    let workspaces_dir = repo_root.join(".decapod/workspaces");
    if let Ok(meta) = fs::symlink_metadata(&workspaces_dir)
        && !meta.file_type().is_symlink()
        && meta.is_dir()
        && let Ok(entries) = fs::read_dir(&workspaces_dir)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".staging"
                && entry
                    .path()
                    .read_dir()
                    .is_ok_and(|mut r| r.next().is_none())
            {
                continue;
            }
            let rel = format!(".decapod/workspaces/{name}");
            active.push(ActiveResourceConflict {
                kind: "active_worktree".into(),
                path_or_id: rel.clone(),
                detail: format!("Managed workspace directory '{rel}' exists"),
            });
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                path: rel,
                detail: "Active or unpruned workspace directory must be retired before integration removal".into(),
            });
        }
    }

    // 2. Check `git worktree list --porcelain` (read-only) for worktrees under .decapod/workspaces
    if repo_root.join(".git").exists()
        && let Ok(out) = Command::new("git")
            .args([
                "-C",
                &repo_root.to_string_lossy(),
                "worktree",
                "list",
                "--porcelain",
            ])
            .output()
        && out.status.success()
    {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if let Some(wt_raw) = line.strip_prefix("worktree ") {
                let wt_path = Path::new(wt_raw.trim());
                if let Ok(canon_wt) = fs::canonicalize(wt_path)
                    && let Ok(rel) = canon_wt.strip_prefix(repo_root)
                {
                    let rel_str = rel.to_string_lossy().to_string();
                    if rel_str.starts_with(".decapod/workspaces/")
                        && !active.iter().any(|a| a.path_or_id == rel_str)
                    {
                        active.push(ActiveResourceConflict {
                            kind: "registered_worktree".into(),
                            path_or_id: rel_str.clone(),
                            detail: "Registered Git worktree under .decapod/workspaces".into(),
                        });
                        blockers.push(RemovalBlocker {
                            code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                            path: rel_str,
                            detail: "Active registered Git worktree blocks integration removal"
                                .into(),
                        });
                    }
                }
            }
        }
    }

    // 3. Check workspace-leases/*.lock non-destructively (no file creation)
    let leases_dir = repo_root.join(".decapod/data/workspace-leases");
    if leases_dir.is_dir()
        && let Ok(entries) = fs::read_dir(&leases_dir)
    {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("lock")
                && let Ok(f) = fs::OpenOptions::new().read(true).open(&p)
                && fs2::FileExt::try_lock_exclusive(&f).is_err()
            {
                let rel = format!(
                    ".decapod/data/workspace-leases/{}",
                    entry.file_name().to_string_lossy()
                );
                active.push(ActiveResourceConflict {
                    kind: "workspace_lease".into(),
                    path_or_id: rel.clone(),
                    detail: "Active exclusive workspace lease held".into(),
                });
                blockers.push(RemovalBlocker {
                    code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                    path: rel,
                    detail: "Active workspace operation lock held".into(),
                });
            }
        }
    }

    // 4. Check SQLite storage locks and active todo leases / authoring bindings (read-only)
    for db_name in [schemas::LOCAL_DB_NAME, "todo.db"] {
        let db_path = repo_root.join(".decapod/data").join(db_name);
        if !db_path.is_file() {
            continue;
        }
        if let Ok(diag) = StorageLock::diagnose(&db_path, Duration::from_millis(25))
            && (diag.lock_state != StorageLockContentionState::Unlocked
                || diag.foreground_owner_active)
        {
            let rel = format!(".decapod/data/{db_name}");
            active.push(ActiveResourceConflict {
                kind: "storage_lock".into(),
                path_or_id: rel.clone(),
                detail: format!(
                    "Active datastore lock or foreground owner ({:?})",
                    diag.lock_state
                ),
            });
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                path: rel,
                detail: "Active datastore owner or lock blocks integration removal".into(),
            });
            continue;
        }

        // Open strictly read-only without StorageLock::acquire (which creates `.lock`)
        if let Ok(conn) = dactyl_db::Connection::open_with_options(
            &db_path,
            ::dactyl_db::AccessMode::ReadOnly,
            Duration::from_millis(50),
        ) {
            if let Ok(mut stmt) = conn.prepare(
                "SELECT id, assigned_to FROM tasks WHERE status NOT IN ('done', 'archived', 'closed') AND assigned_to IS NOT NULL AND assigned_to != ''",
            ) && let Ok(rows) =
                stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            {
                for (id, owner) in rows.flatten() {
                    active.push(ActiveResourceConflict {
                        kind: "active_todo_lease".into(),
                        path_or_id: id.clone(),
                        detail: format!("Task '{id}' is actively claimed by '{owner}'"),
                    });
                    blockers.push(RemovalBlocker {
                        code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                        path: format!(".decapod/data/{db_name}"),
                        detail: format!("Active todo lease on '{id}' (owner '{owner}') must be released or completed first"),
                    });
                }
            }
            if let Ok(mut stmt) =
                conn.prepare("SELECT task_id, agent_id, branch FROM authoring_bindings")
                && let Ok(rows) = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
            {
                for (task_id, agent_id, branch) in rows.flatten() {
                    active.push(ActiveResourceConflict {
                        kind: "authoring_binding".into(),
                        path_or_id: task_id.clone(),
                        detail: format!("Active authoring binding for task '{task_id}' (agent '{agent_id}', branch '{branch}')"),
                    });
                    blockers.push(RemovalBlocker {
                        code: "INTEGRATION_ACTIVE_CUSTODY".into(),
                        path: format!(".decapod/data/{db_name}"),
                        detail: format!(
                            "Active authoring binding on '{branch}' must be retired first"
                        ),
                    });
                }
            }
        }
    }
}

fn is_git_tracked(repo_root: &Path, rel: &str) -> bool {
    Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "ls-files",
            "--error-unmatch",
            "--",
            rel,
        ])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn known_config_top_keys() -> BTreeSet<&'static str> {
    [
        "schema_version",
        "init",
        "repo",
        "governance",
        "laya",
        "proof",
        "custody",
        "tracker",
        "context",
    ]
    .into_iter()
    .collect()
}

fn build_authority_boundaries(
    repo_root: &Path,
    authored: &[ClassifiedResource],
) -> AuthorityBoundaries {
    let mut retained = Vec::new();
    for item in authored {
        retained.push(item.path.clone());
    }
    for rel in [
        ".decapod/data",
        ".decapod/governance.json",
        ".decapod/contracts",
    ] {
        let p = repo_root.join(rel);
        if p.exists() && !retained.iter().any(|r| r == rel) {
            retained.push(rel.to_string());
        }
    }
    retained.sort();
    retained.dedup();
    AuthorityBoundaries {
        bounded_integration_removal: "authorized_via_integration_remove_flag".into(),
        permanent_state_deletion: "separate_explicit_authority_required".into(),
        protected_governance_removal: "separate_explicit_authority_required".into(),
        retained_state_paths: retained,
    }
}

struct ClassifiedPlan {
    report: IntegrationPreviewReport,
    removable_files: Vec<(String, String)>, // (rel_path, kind)
    gitignore_state: Option<(bool, bool, Vec<String>, String)>, // (remove_file, existed, owned_rules, original_sha)
}

fn inspect_repository(repo_root: &Path) -> Result<ClassifiedPlan, DecapodError> {
    let mut generated = Vec::new();
    let mut edited = Vec::new();
    let mut authored = Vec::new();
    let mut unknown = Vec::new();
    let mut active = Vec::new();
    let mut blockers = Vec::new();
    let mut removable_files: Vec<(String, String)> = Vec::new();

    // Check symlink blockers across .decapod and .github early
    collect_symlink_blockers_under(repo_root, ".decapod", 0, &mut 0, &mut active, &mut blockers);
    collect_symlink_blockers_under(repo_root, ".github", 0, &mut 0, &mut active, &mut blockers);

    let decapod_dir_present = match fs::symlink_metadata(repo_root.join(".decapod")) {
        Ok(m) => m.is_dir() && !m.file_type().is_symlink(),
        Err(_) => false,
    };

    let ownership_manifest = match read_ownership_manifest(repo_root) {
        Ok(m) => m,
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_UNCERTAIN_OWNERSHIP".into(),
                path: INTEGRATION_OWNERSHIP_PATH.into(),
                detail: err.to_string(),
            });
            unknown.push(ClassifiedResource {
                path: INTEGRATION_OWNERSHIP_PATH.into(),
                kind: "metadata".into(),
                classification: ResourceClass::Unknown,
                ownership_evidence: "corrupt_or_unsupported".into(),
                sha256: None,
                expected_sha256: None,
                diagnostic: err.to_string(),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
            None
        }
    };

    let restore_manifest_present = match read_restore_manifest_at(
        repo_root,
        &repo_root.join(INTEGRATION_RESTORE_MANIFEST_PATH),
    ) {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "RESTORE_MANIFEST_INVALID".into(),
                path: INTEGRATION_RESTORE_MANIFEST_PATH.into(),
                detail: err.to_string(),
            });
            false
        }
    };

    let specs_manifest = match read_specs_manifest(repo_root) {
        Ok(m) => m,
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_UNCERTAIN_OWNERSHIP".into(),
                path: LOCAL_PROJECT_SPECS_MANIFEST.into(),
                detail: err.to_string(),
            });
            unknown.push(ClassifiedResource {
                path: LOCAL_PROJECT_SPECS_MANIFEST.into(),
                kind: "specs_manifest".into(),
                classification: ResourceClass::Unknown,
                ownership_evidence: "corrupt_manifest".into(),
                sha256: None,
                expected_sha256: None,
                diagnostic: err.to_string(),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
            None
        }
    };

    // Parse config.toml if present (read-only) to understand declared init options & custom keys
    let mut config_init_entrypoints: Option<Vec<String>> = None;
    let mut config_init_ci: Option<bool> = None;
    let mut config_is_valid_toml = true;
    let mut config_has_unknown_keys = false;
    match inspect_no_symlink(repo_root, ".decapod/config.toml") {
        Ok(Some(meta)) if meta.is_file() => {
            let bytes =
                fs::read(repo_root.join(".decapod/config.toml")).map_err(DecapodError::IoError)?;
            let actual_sha = sha256_bytes(&bytes);
            let raw_text = String::from_utf8_lossy(&bytes);
            match toml::from_str::<toml::Value>(&raw_text) {
                Ok(val) => {
                    let known = known_config_top_keys();
                    if let Some(tbl) = val.as_table() {
                        for k in tbl.keys() {
                            if !known.contains(k.as_str()) {
                                config_has_unknown_keys = true;
                            }
                        }
                        if let Some(init_tbl) = tbl.get("init").and_then(toml::Value::as_table) {
                            if let Some(eps) =
                                init_tbl.get("entrypoints").and_then(toml::Value::as_array)
                            {
                                config_init_entrypoints = Some(
                                    eps.iter()
                                        .filter_map(|v| v.as_str().map(str::to_string))
                                        .collect(),
                                );
                            }
                            config_init_ci = init_tbl.get("ci").and_then(toml::Value::as_bool);
                        }
                    }
                    let owned_rec = ownership_manifest
                        .as_ref()
                        .and_then(|m| m.owned_files.get(".decapod/config.toml"));
                    if let Some(rec) = owned_rec
                        && rec.sha256 == actual_sha
                        && !config_has_unknown_keys
                    {
                        generated.push(ClassifiedResource {
                            path: ".decapod/config.toml".into(),
                            kind: "config".into(),
                            classification: ResourceClass::Generated,
                            ownership_evidence: "ownership_manifest_digest_match".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: Some(rec.sha256.clone()),
                            diagnostic: "Unchanged Decapod-generated project config".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                        removable_files.push((".decapod/config.toml".into(), "config".into()));
                    } else {
                        edited.push(ClassifiedResource {
                            path: ".decapod/config.toml".into(),
                            kind: "config".into(),
                            classification: ResourceClass::Edited,
                            ownership_evidence: if owned_rec.is_some() {
                                "customized_or_partial_config".into()
                            } else {
                                "no_positive_unchanged_ownership".into()
                            },
                            sha256: Some(actual_sha),
                            expected_sha256: owned_rec.map(|r| r.sha256.clone()),
                            diagnostic: "Customized or shared project configuration preserved"
                                .into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                    }
                }
                Err(err) => {
                    config_is_valid_toml = false;
                    blockers.push(RemovalBlocker {
                        code: "INTEGRATION_UNCERTAIN_OWNERSHIP".into(),
                        path: ".decapod/config.toml".into(),
                        detail: format!(
                            "Malformed config.toml prevents ownership verification: {err}"
                        ),
                    });
                    unknown.push(ClassifiedResource {
                        path: ".decapod/config.toml".into(),
                        kind: "config".into(),
                        classification: ResourceClass::Unknown,
                        ownership_evidence: "invalid_toml".into(),
                        sha256: Some(actual_sha),
                        expected_sha256: None,
                        diagnostic: format!("Malformed TOML: {err}"),
                        owned_entries: Vec::new(),
                        preserved_entries: Vec::new(),
                    });
                }
            }
        }
        Ok(Some(_)) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_UNCERTAIN_OWNERSHIP".into(),
                path: ".decapod/config.toml".into(),
                detail: "Expected regular file at .decapod/config.toml".into(),
            });
        }
        Ok(None) => {}
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_SYMLINK_REFUSED".into(),
                path: ".decapod/config.toml".into(),
                detail: err.to_string(),
            });
        }
    }
    let _ = config_is_valid_toml;

    // 1. Inspect root agent entrypoints & .bak files
    for surface in entrypoint_integrity::ENTRYPOINT_FILES {
        match inspect_no_symlink(repo_root, surface) {
            Ok(Some(meta)) if meta.is_file() => {
                let bytes = fs::read(repo_root.join(surface)).map_err(DecapodError::IoError)?;
                let actual_sha = sha256_bytes(&bytes);
                let text = String::from_utf8_lossy(&bytes);
                let owned_rec = ownership_manifest
                    .as_ref()
                    .and_then(|m| m.owned_files.get(surface));
                let pre_existed = ownership_manifest
                    .as_ref()
                    .is_some_and(|m| m.pre_existing_files.iter().any(|p| p == surface));

                if let Some(rec) = owned_rec {
                    if rec.sha256 == actual_sha && is_valid_rendered_entrypoint(surface, &text) {
                        generated.push(ClassifiedResource {
                            path: surface.to_string(),
                            kind: "entrypoint".into(),
                            classification: ResourceClass::Generated,
                            ownership_evidence: "ownership_manifest_and_release_fingerprint".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: Some(rec.sha256.clone()),
                            diagnostic: "Unchanged Decapod-generated agent entrypoint".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                        removable_files.push((surface.to_string(), "entrypoint".into()));
                    } else {
                        edited.push(ClassifiedResource {
                            path: surface.to_string(),
                            kind: "entrypoint".into(),
                            classification: ResourceClass::Edited,
                            ownership_evidence: "modified_generated_entrypoint".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: Some(rec.sha256.clone()),
                            diagnostic: "Modified agent entrypoint preserved".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                    }
                } else if ownership_manifest.is_some() || pre_existed || !decapod_dir_present {
                    authored.push(ClassifiedResource {
                        path: surface.to_string(),
                        kind: "entrypoint".into(),
                        classification: ResourceClass::Authored,
                        ownership_evidence: "not_in_ownership_manifest_or_uninitialized".into(),
                        sha256: Some(actual_sha),
                        expected_sha256: None,
                        diagnostic: "User-authored file preserved (no Decapod ownership record)"
                            .into(),
                        owned_entries: Vec::new(),
                        preserved_entries: Vec::new(),
                    });
                } else {
                    // Legacy initialized repo without integration_ownership.json:
                    // require BOTH valid cryptographic release+fingerprint header AND canonical template body,
                    // plus consistency with config/specs manifest if present.
                    let allowed_by_cfg = config_init_entrypoints
                        .as_ref()
                        .is_none_or(|eps| eps.is_empty() || eps.iter().any(|e| e == surface));
                    if allowed_by_cfg && is_valid_rendered_entrypoint(surface, &text) {
                        generated.push(ClassifiedResource {
                            path: surface.to_string(),
                            kind: "entrypoint".into(),
                            classification: ResourceClass::Generated,
                            ownership_evidence: "release_fingerprint_header_verified".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: None,
                            diagnostic: "Unchanged Decapod-generated entrypoint (legacy init)"
                                .into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                        removable_files.push((surface.to_string(), "entrypoint".into()));
                    } else if has_decapod_entrypoint_markers(&text) {
                        edited.push(ClassifiedResource {
                            path: surface.to_string(),
                            kind: "entrypoint".into(),
                            classification: ResourceClass::Edited,
                            ownership_evidence: "decapod_markers_digest_mismatch".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: None,
                            diagnostic: "Edited or non-canonical entrypoint preserved".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                    } else {
                        authored.push(ClassifiedResource {
                            path: surface.to_string(),
                            kind: "entrypoint".into(),
                            classification: ResourceClass::Authored,
                            ownership_evidence: "no_decapod_ownership_markers".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: None,
                            diagnostic: "Authored repository file preserved (missing Decapod ownership headers)".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                    }
                }
            }
            Ok(Some(_)) => {
                blockers.push(RemovalBlocker {
                    code: "INTEGRATION_UNCERTAIN_OWNERSHIP".into(),
                    path: surface.to_string(),
                    detail: format!("Non-regular filesystem entry at '{surface}'"),
                });
            }
            Ok(None) => {}
            Err(err) => {
                active.push(ActiveResourceConflict {
                    kind: "symlink".into(),
                    path_or_id: surface.to_string(),
                    detail: err.to_string(),
                });
                blockers.push(RemovalBlocker {
                    code: "INTEGRATION_SYMLINK_REFUSED".into(),
                    path: surface.to_string(),
                    detail: err.to_string(),
                });
            }
        }

        let bak = format!("{surface}.bak");
        match inspect_no_symlink(repo_root, &bak) {
            Ok(Some(meta)) if meta.is_file() => {
                let bytes = fs::read(repo_root.join(&bak)).map_err(DecapodError::IoError)?;
                authored.push(ClassifiedResource {
                    path: bak,
                    kind: "entrypoint_backup".into(),
                    classification: ResourceClass::Authored,
                    ownership_evidence: "pre_existing_user_backup".into(),
                    sha256: Some(sha256_bytes(&bytes)),
                    expected_sha256: None,
                    diagnostic: "Pre-existing user entrypoint backup preserved".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
            }
            Err(err) => {
                blockers.push(RemovalBlocker {
                    code: "INTEGRATION_SYMLINK_REFUSED".into(),
                    path: bak,
                    detail: err.to_string(),
                });
            }
            _ => {}
        }
    }

    // 2. Inspect .gitignore partial/full ownership
    let mut gitignore_state: Option<(bool, bool, Vec<String>, String)> = None;
    match inspect_no_symlink(repo_root, ".gitignore") {
        Ok(Some(meta)) if meta.is_file() => {
            let bytes = fs::read(repo_root.join(".gitignore")).map_err(DecapodError::IoError)?;
            let actual_sha = sha256_bytes(&bytes);
            let text = String::from_utf8_lossy(&bytes);
            let mut owned_lines = Vec::new();
            let mut preserved_lines = Vec::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if DECAPOD_GITIGNORE_RULES.contains(&trimmed) {
                    owned_lines.push(trimmed.to_string());
                } else if !trimmed.is_empty() {
                    preserved_lines.push(line.to_string());
                }
            }
            if owned_lines.is_empty() {
                authored.push(ClassifiedResource {
                    path: ".gitignore".into(),
                    kind: "gitignore".into(),
                    classification: ResourceClass::Authored,
                    ownership_evidence: "no_decapod_gitignore_rules".into(),
                    sha256: Some(actual_sha),
                    expected_sha256: None,
                    diagnostic: "User .gitignore contains no Decapod rules".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: preserved_lines,
                });
            } else if preserved_lines.is_empty() && decapod_dir_present {
                let created_by_decapod = ownership_manifest
                    .as_ref()
                    .map(|m| m.created_gitignore)
                    .unwrap_or_else(|| !is_git_tracked(repo_root, ".gitignore"));
                if created_by_decapod {
                    generated.push(ClassifiedResource {
                        path: ".gitignore".into(),
                        kind: "gitignore".into(),
                        classification: ResourceClass::Generated,
                        ownership_evidence: "only_decapod_gitignore_rules".into(),
                        sha256: Some(actual_sha.clone()),
                        expected_sha256: None,
                        diagnostic: "Decapod-created .gitignore with only Decapod rules".into(),
                        owned_entries: owned_lines.clone(),
                        preserved_entries: Vec::new(),
                    });
                    gitignore_state = Some((true, true, owned_lines, actual_sha));
                } else {
                    edited.push(ClassifiedResource {
                        path: ".gitignore".into(),
                        kind: "gitignore".into(),
                        classification: ResourceClass::Edited,
                        ownership_evidence: "partial_tracked_gitignore".into(),
                        sha256: Some(actual_sha.clone()),
                        expected_sha256: None,
                        diagnostic:
                            "Tracked .gitignore; only Decapod ignore rules will be stripped".into(),
                        owned_entries: owned_lines.clone(),
                        preserved_entries: Vec::new(),
                    });
                    gitignore_state = Some((false, true, owned_lines, actual_sha));
                }
            } else if decapod_dir_present {
                edited.push(ClassifiedResource {
                    path: ".gitignore".into(),
                    kind: "gitignore".into(),
                    classification: ResourceClass::Edited,
                    ownership_evidence: "partial_gitignore_ownership".into(),
                    sha256: Some(actual_sha.clone()),
                    expected_sha256: None,
                    diagnostic: "Shared .gitignore; owned Decapod entries removed while preserving user entries".into(),
                    owned_entries: owned_lines.clone(),
                    preserved_entries: preserved_lines,
                });
                gitignore_state = Some((false, true, owned_lines, actual_sha));
            }
        }
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_SYMLINK_REFUSED".into(),
                path: ".gitignore".into(),
                detail: err.to_string(),
            });
        }
        _ => {}
    }

    // 3. Inspect .github/workflows/decapod-validate.yml
    let wf_rel = ".github/workflows/decapod-validate.yml";
    match inspect_no_symlink(repo_root, wf_rel) {
        Ok(Some(meta)) if meta.is_file() => {
            let bytes = fs::read(repo_root.join(wf_rel)).map_err(DecapodError::IoError)?;
            let actual_sha = sha256_bytes(&bytes);
            let tmpl = assets::get_template("decapod-validate.yml").unwrap_or_default();
            let owned_rec = ownership_manifest
                .as_ref()
                .and_then(|m| m.owned_files.get(wf_rel));
            if let Some(rec) = owned_rec {
                if rec.sha256 == actual_sha {
                    generated.push(ClassifiedResource {
                        path: wf_rel.into(),
                        kind: "workflow".into(),
                        classification: ResourceClass::Generated,
                        ownership_evidence: "ownership_manifest_digest_match".into(),
                        sha256: Some(actual_sha),
                        expected_sha256: Some(rec.sha256.clone()),
                        diagnostic: "Unchanged Decapod CI workflow".into(),
                        owned_entries: Vec::new(),
                        preserved_entries: Vec::new(),
                    });
                    removable_files.push((wf_rel.into(), "workflow".into()));
                } else {
                    edited.push(ClassifiedResource {
                        path: wf_rel.into(),
                        kind: "workflow".into(),
                        classification: ResourceClass::Edited,
                        ownership_evidence: "modified_generated_workflow".into(),
                        sha256: Some(actual_sha),
                        expected_sha256: Some(rec.sha256.clone()),
                        diagnostic: "Customized CI workflow preserved".into(),
                        owned_entries: Vec::new(),
                        preserved_entries: Vec::new(),
                    });
                }
            } else if ownership_manifest.is_none()
                && decapod_dir_present
                && config_init_ci.unwrap_or(false)
                && bytes == tmpl.as_bytes()
            {
                generated.push(ClassifiedResource {
                    path: wf_rel.into(),
                    kind: "workflow".into(),
                    classification: ResourceClass::Generated,
                    ownership_evidence: "legacy_ci_template_digest_match".into(),
                    sha256: Some(actual_sha),
                    expected_sha256: Some(sha256_bytes(tmpl.as_bytes())),
                    diagnostic: "Unchanged Decapod CI workflow (legacy init)".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
                removable_files.push((wf_rel.into(), "workflow".into()));
            } else {
                authored.push(ClassifiedResource {
                    path: wf_rel.into(),
                    kind: "workflow".into(),
                    classification: ResourceClass::Authored,
                    ownership_evidence: "no_positive_workflow_ownership".into(),
                    sha256: Some(actual_sha),
                    expected_sha256: None,
                    diagnostic: "Authored CI workflow preserved".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
            }
        }
        Err(err) => {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_SYMLINK_REFUSED".into(),
                path: wf_rel.into(),
                detail: err.to_string(),
            });
        }
        _ => {}
    }

    // 4. Inspect Git hooks (.git/hooks/commit-msg, .git/hooks/pre-commit)
    if let Some((hooks_dir, git_is_symlink)) = check_git_hooks_dir_readonly(repo_root) {
        if git_is_symlink {
            blockers.push(RemovalBlocker {
                code: "INTEGRATION_SYMLINK_REFUSED".into(),
                path: hooks_dir.to_string_lossy().to_string(),
                detail: "Symlink detected in .git or .git/hooks".into(),
            });
        } else {
            for hook_name in ["commit-msg", "pre-commit"] {
                let hook_path = hooks_dir.join(hook_name);
                let rel = format!(".git/hooks/{hook_name}");
                match fs::symlink_metadata(&hook_path) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        active.push(ActiveResourceConflict {
                            kind: "symlink".into(),
                            path_or_id: rel.clone(),
                            detail: format!("Git hook '{rel}' is a symlink"),
                        });
                        blockers.push(RemovalBlocker {
                            code: "INTEGRATION_SYMLINK_REFUSED".into(),
                            path: rel,
                            detail: "Refusing symlink git hook".into(),
                        });
                    }
                    Ok(meta) if meta.is_file() => {
                        let bytes = fs::read(&hook_path).map_err(DecapodError::IoError)?;
                        let actual_sha = sha256_bytes(&bytes);
                        let owned_rec = ownership_manifest
                            .as_ref()
                            .and_then(|m| m.owned_files.get(&rel));
                        if let Some(rec) = owned_rec {
                            if rec.sha256 == actual_sha {
                                generated.push(ClassifiedResource {
                                    path: rel.clone(),
                                    kind: "hook".into(),
                                    classification: ResourceClass::Generated,
                                    ownership_evidence: "hook_ownership_manifest_match".into(),
                                    sha256: Some(actual_sha),
                                    expected_sha256: Some(rec.sha256.clone()),
                                    diagnostic: format!(
                                        "Unchanged Decapod-installed {hook_name} hook"
                                    ),
                                    owned_entries: Vec::new(),
                                    preserved_entries: Vec::new(),
                                });
                                removable_files.push((rel, "hook".into()));
                            } else {
                                edited.push(ClassifiedResource {
                                    path: rel,
                                    kind: "hook".into(),
                                    classification: ResourceClass::Edited,
                                    ownership_evidence: "modified_decapod_hook".into(),
                                    sha256: Some(actual_sha),
                                    expected_sha256: Some(rec.sha256.clone()),
                                    diagnostic: format!("Edited {hook_name} hook preserved"),
                                    owned_entries: Vec::new(),
                                    preserved_entries: Vec::new(),
                                });
                            }
                        } else {
                            // Never delete a hook without explicit Decapod installation provenance,
                            // even if its bytes match Decapod's hook template.
                            authored.push(ClassifiedResource {
                                path: rel,
                                kind: "hook".into(),
                                classification: ResourceClass::Authored,
                                ownership_evidence: "no_hook_ownership_record".into(),
                                sha256: Some(actual_sha),
                                expected_sha256: None,
                                diagnostic: format!(
                                    "User or pre-existing {hook_name} hook preserved"
                                ),
                                owned_entries: Vec::new(),
                                preserved_entries: Vec::new(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // 5. Inspect .decapod/ generated files, specs, authored state, and unknown entries
    if decapod_dir_present {
        inspect_decapod_internal_files(
            repo_root,
            ownership_manifest.as_ref(),
            specs_manifest.as_ref(),
            &mut generated,
            &mut edited,
            &mut authored,
            &mut unknown,
            &mut removable_files,
        )?;
    }

    // 6. Inspect active custody conflicts
    inspect_active_custody(repo_root, &mut active, &mut blockers);

    let authority_boundaries = build_authority_boundaries(repo_root, &authored);
    let can_apply = blockers.is_empty();

    let report = IntegrationPreviewReport {
        schema_version: "1.0.0".into(),
        mode: "preview".into(),
        repository_root: repo_root.display().to_string(),
        ownership_manifest_present: ownership_manifest.is_some(),
        restore_manifest_present,
        can_apply,
        generated,
        edited,
        authored,
        unknown,
        active,
        blockers,
        authority_boundaries,
    };

    Ok(ClassifiedPlan {
        report,
        removable_files,
        gitignore_state,
    })
}

#[allow(clippy::too_many_arguments)]
fn inspect_decapod_internal_files(
    repo_root: &Path,
    ownership_manifest: Option<&IntegrationOwnershipManifest>,
    specs_manifest: Option<&crate::core::project_specs::ProjectSpecsManifest>,
    generated: &mut Vec<ClassifiedResource>,
    edited: &mut Vec<ClassifiedResource>,
    authored: &mut Vec<ClassifiedResource>,
    unknown: &mut Vec<ClassifiedResource>,
    removable_files: &mut Vec<(String, String)>,
) -> Result<(), DecapodError> {
    let mut handled_paths: BTreeSet<String> = BTreeSet::new();
    handled_paths.insert(".decapod/config.toml".into());
    handled_paths.insert(INTEGRATION_RESTORE_MANIFEST_PATH.into());

    // Helper for fixed template/managed files inside .decapod/
    let managed_candidates: Vec<(&str, &str, Option<String>)> = vec![
        (
            ".decapod/README.md",
            "config",
            assets::get_template("README.md"),
        ),
        (
            ".decapod/OVERRIDE.md",
            "override",
            assets::get_template("OVERRIDE.md"),
        ),
        (
            ".decapod/managed/Dockerfile.decapod",
            "dockerfile",
            Some(crate::plugins::container::generated_dockerfile_for_repo(
                repo_root,
            )),
        ),
        (
            GENERATED_POLICY_REL_PATH,
            "policy",
            default_policy_json_pretty().ok(),
        ),
    ];

    for (rel, kind, template_opt) in managed_candidates {
        handled_paths.insert(rel.to_string());
        let Ok(Some(meta)) = inspect_no_symlink(repo_root, rel) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let bytes = fs::read(repo_root.join(rel)).map_err(DecapodError::IoError)?;
        let actual_sha = sha256_bytes(&bytes);
        let owned_rec = ownership_manifest.and_then(|m| m.owned_files.get(rel));

        if rel == ".decapod/OVERRIDE.md" {
            let has_override_content = assets::resolved_override_evidence(repo_root)
                .map(|ev| !ev.is_empty())
                .unwrap_or(true);
            let matches_scaffold = template_opt
                .as_deref()
                .is_some_and(|t| bytes == t.as_bytes());
            if !has_override_content
                && matches_scaffold
                && owned_rec.is_none_or(|r| r.sha256 == actual_sha)
            {
                generated.push(ClassifiedResource {
                    path: rel.into(),
                    kind: kind.into(),
                    classification: ResourceClass::Generated,
                    ownership_evidence: "untouched_override_scaffold".into(),
                    sha256: Some(actual_sha),
                    expected_sha256: owned_rec.map(|r| r.sha256.clone()),
                    diagnostic: "Untouched OVERRIDE.md scaffold".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
                removable_files.push((rel.into(), kind.into()));
            } else {
                authored.push(ClassifiedResource {
                    path: rel.into(),
                    kind: kind.into(),
                    classification: ResourceClass::Authored,
                    ownership_evidence: "authored_override_directives".into(),
                    sha256: Some(actual_sha),
                    expected_sha256: owned_rec.map(|r| r.sha256.clone()),
                    diagnostic: "Authored OVERRIDE.md preserved".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
            }
            continue;
        }

        let matches_owned = owned_rec.is_some_and(|r| r.sha256 == actual_sha);
        let matches_legacy_template = ownership_manifest.is_none()
            && template_opt
                .as_deref()
                .is_some_and(|t| bytes == t.as_bytes());

        if matches_owned || matches_legacy_template {
            generated.push(ClassifiedResource {
                path: rel.into(),
                kind: kind.into(),
                classification: ResourceClass::Generated,
                ownership_evidence: if matches_owned {
                    "ownership_manifest_digest_match".into()
                } else {
                    "canonical_template_digest_match".into()
                },
                sha256: Some(actual_sha),
                expected_sha256: owned_rec.map(|r| r.sha256.clone()),
                diagnostic: format!("Unchanged Decapod-generated {kind}"),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
            removable_files.push((rel.into(), kind.into()));
        } else {
            edited.push(ClassifiedResource {
                path: rel.into(),
                kind: kind.into(),
                classification: ResourceClass::Edited,
                ownership_evidence: "modified_managed_file".into(),
                sha256: Some(actual_sha),
                expected_sha256: owned_rec.map(|r| r.sha256.clone()),
                diagnostic: format!("Customized {rel} preserved"),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
        }
    }

    // Metadata files inside .decapod/managed/
    for rel in [
        ".decapod/managed/version_counter.json",
        ".decapod/managed/override.checksum",
        INTEGRATION_OWNERSHIP_PATH,
    ] {
        handled_paths.insert(rel.to_string());
        if let Ok(Some(meta)) = inspect_no_symlink(repo_root, rel)
            && meta.is_file()
        {
            let bytes = fs::read(repo_root.join(rel)).map_err(DecapodError::IoError)?;
            let actual_sha = sha256_bytes(&bytes);
            generated.push(ClassifiedResource {
                path: rel.into(),
                kind: "metadata".into(),
                classification: ResourceClass::Generated,
                ownership_evidence: "decapod_managed_metadata".into(),
                sha256: Some(actual_sha),
                expected_sha256: None,
                diagnostic: "Decapod-managed integration metadata".into(),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
            removable_files.push((rel.into(), "metadata".into()));
        }
    }

    // Canonical specs manifest under .decapod/managed/.manifest.json (#1079)
    let canonical_manifest = repo_root.join(LOCAL_PROJECT_SPECS_MANIFEST);
    if canonical_manifest.is_file() {
        handled_paths.insert(LOCAL_PROJECT_SPECS_MANIFEST.into());
        if specs_manifest.is_some() {
            let bytes = fs::read(&canonical_manifest).map_err(DecapodError::IoError)?;
            generated.push(ClassifiedResource {
                path: LOCAL_PROJECT_SPECS_MANIFEST.into(),
                kind: "specs_manifest".into(),
                classification: ResourceClass::Generated,
                ownership_evidence: "specs_manifest_present".into(),
                sha256: Some(sha256_bytes(&bytes)),
                expected_sha256: None,
                diagnostic: "Decapod living-specs manifest".into(),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
            removable_files.push((LOCAL_PROJECT_SPECS_MANIFEST.into(), "specs_manifest".into()));
        }
    }

    // Living specs under .decapod/managed/specs/
    let specs_dir = repo_root.join(LOCAL_PROJECT_SPECS_DIR);
    if specs_dir.is_dir() {
        handled_paths.insert(LOCAL_PROJECT_SPECS_MANIFEST.into());
        if let Ok(entries) = fs::read_dir(&specs_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let rel = format!("{LOCAL_PROJECT_SPECS_DIR}/{name}");
                handled_paths.insert(rel.clone());
                if name == ".manifest.json" {
                    if specs_manifest.is_some() {
                        let bytes = fs::read(entry.path()).map_err(DecapodError::IoError)?;
                        generated.push(ClassifiedResource {
                            path: rel.clone(),
                            kind: "specs_manifest".into(),
                            classification: ResourceClass::Generated,
                            ownership_evidence: "specs_manifest_present".into(),
                            sha256: Some(sha256_bytes(&bytes)),
                            expected_sha256: None,
                            diagnostic: "Decapod living-specs manifest".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                        removable_files.push((rel, "specs_manifest".into()));
                    }
                    continue;
                }
                let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let bytes = fs::read(entry.path()).map_err(DecapodError::IoError)?;
                let actual_sha = sha256_bytes(&bytes);
                let text = String::from_utf8_lossy(&bytes);
                let text_hash = crate::core::project_specs::hash_text(&text);

                if let Some(manifest) = specs_manifest
                    && let Some(spec_entry) = manifest.files.iter().find(|f| f.path == rel)
                {
                    if text_hash == spec_entry.template_hash
                        && spec_entry.content_hash == spec_entry.template_hash
                    {
                        generated.push(ClassifiedResource {
                            path: rel.clone(),
                            kind: "spec".into(),
                            classification: ResourceClass::Generated,
                            ownership_evidence: "specs_manifest_template_hash_match".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: None,
                            diagnostic: "Untouched scaffold living spec".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                        removable_files.push((rel, "spec".into()));
                    } else {
                        authored.push(ClassifiedResource {
                            path: rel,
                            kind: "spec".into(),
                            classification: ResourceClass::Authored,
                            ownership_evidence: "authored_living_spec_prose".into(),
                            sha256: Some(actual_sha),
                            expected_sha256: None,
                            diagnostic: "Authored project spec prose preserved".into(),
                            owned_entries: Vec::new(),
                            preserved_entries: Vec::new(),
                        });
                    }
                } else {
                    unknown.push(ClassifiedResource {
                        path: rel,
                        kind: "spec".into(),
                        classification: ResourceClass::Unknown,
                        ownership_evidence: "missing_specs_manifest_entry".into(),
                        sha256: Some(actual_sha),
                        expected_sha256: None,
                        diagnostic: "Spec file without manifest provenance preserved".into(),
                        owned_entries: Vec::new(),
                        preserved_entries: Vec::new(),
                    });
                }
            }
        }
    }

    // Retained durable project state & protected governance
    {
        let rel = ".decapod/governance.json";
        handled_paths.insert(rel.into());
        if let Ok(Some(meta)) = inspect_no_symlink(repo_root, rel)
            && meta.is_file()
        {
            let bytes = fs::read(repo_root.join(rel)).map_err(DecapodError::IoError)?;
            authored.push(ClassifiedResource {
                path: rel.into(),
                kind: "protected_governance".into(),
                classification: ResourceClass::Authored,
                ownership_evidence: "canonical_governance_artifact".into(),
                sha256: Some(sha256_bytes(&bytes)),
                expected_sha256: None,
                diagnostic: "Protected governance document preserved by default".into(),
                owned_entries: Vec::new(),
                preserved_entries: Vec::new(),
            });
        }
    }

    // Walk remaining entries under .decapod/ to classify retained state vs unknown files
    let mut stack = vec![".decapod".to_string()];
    let mut visited = 0usize;
    while let Some(dir_rel) = stack.pop() {
        visited += 1;
        if visited > MAX_SCAN_ENTRIES {
            break;
        }
        let Ok(entries) = fs::read_dir(repo_root.join(&dir_rel)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let rel = format!("{dir_rel}/{name}");
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.is_dir() {
                if rel == ".decapod/workspaces" {
                    continue;
                }
                stack.push(rel);
                continue;
            }
            if handled_paths.contains(&rel) {
                continue;
            }
            if rel.starts_with(".decapod/data/")
                || rel.starts_with(".decapod/backups/")
                || rel.starts_with(".decapod/contracts/")
                || rel.starts_with(".decapod/policy/")
                || rel.starts_with(".decapod/governance/")
                || rel.starts_with(".decapod/managed/artifacts/")
            {
                authored.push(ClassifiedResource {
                    path: rel,
                    kind: "retained_state".into(),
                    classification: ResourceClass::Authored,
                    ownership_evidence: "canonical_project_state".into(),
                    sha256: None,
                    expected_sha256: None,
                    diagnostic: "Retained project data/proof history preserved".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
            } else {
                unknown.push(ClassifiedResource {
                    path: rel,
                    kind: "unknown_decapod_file".into(),
                    classification: ResourceClass::Unknown,
                    ownership_evidence: "unrecognized_file".into(),
                    sha256: None,
                    expected_sha256: None,
                    diagnostic: "Unrecognized file preserved without deletion".into(),
                    owned_entries: Vec::new(),
                    preserved_entries: Vec::new(),
                });
            }
        }
    }

    Ok(())
}

/// Read-only preview of Decapod repository integration ownership, edits, and blockers.
pub fn preview_integration(
    raw_dir: Option<PathBuf>,
) -> Result<IntegrationPreviewReport, DecapodError> {
    let root = validate_target_root(raw_dir)?;
    Ok(inspect_repository(&root)?.report)
}

fn strip_owned_gitignore_rules(content: &str, rules: &[String]) -> String {
    let rule_set: BTreeSet<&str> = rules.iter().map(String::as_str).collect();
    let had_trailing_newline = content.ends_with('\n');
    let kept: Vec<&str> = content
        .lines()
        .filter(|line| !rule_set.contains(line.trim()))
        .collect();
    // Collapse trailing blank lines that only separated removed Decapod block
    let mut trimmed = kept;
    while trimmed.last().is_some_and(|l| l.trim().is_empty()) {
        trimmed.pop();
    }
    if trimmed.is_empty() {
        return String::new();
    }
    let mut out = trimmed.join("\n");
    if had_trailing_newline || !out.is_empty() {
        out.push('\n');
    }
    out
}

fn prune_empty_managed_dirs(repo_root: &Path) {
    let candidates = [
        ".decapod/managed/specs",
        ".decapod/managed/policy",
        ".decapod/managed/migrations",
        ".decapod/managed/artifacts/custody",
        ".decapod/managed/artifacts",
        ".decapod/managed/awareness",
        ".decapod/managed/sessions",
        ".decapod/managed/context",
        ".decapod/managed",
    ];
    for rel in candidates {
        let p = repo_root.join(rel);
        if p.is_dir() && fs::read_dir(&p).is_ok_and(|mut r| r.next().is_none()) {
            let _ = fs::remove_dir(&p);
        }
    }
}

/// Explicitly authorized bounded removal of unchanged generated repository integration.
#[allow(clippy::too_many_arguments)]
pub fn remove_integration(
    raw_dir: Option<PathBuf>,
    authorize: bool,
    dry_run: bool,
    purge_state: bool,
    remove_protected_governance: bool,
) -> Result<IntegrationApplyReport, DecapodError> {
    if purge_state {
        return Err(DecapodError::ValidationError(
            "PERMANENT_STATE_DELETION_BOUNDARY: permanent deletion of .decapod/data and retained project history requires a separate explicit state-purge authority and is refused by integration remove".into(),
        ));
    }
    if remove_protected_governance {
        return Err(DecapodError::ValidationError(
            "PROTECTED_GOVERNANCE_REMOVAL_BLOCKED: removing protected governance artifacts (.decapod/governance.json, custody records) requires separate governance authority and is refused by integration remove".into(),
        ));
    }

    let root = validate_target_root(raw_dir)?;
    let plan = inspect_repository(&root)?;
    if dry_run {
        return Err(DecapodError::ValidationError(
            "INTEGRATION_PREVIEW_ONLY: --dry-run requested; use `decapod integration preview` for read-only inspection".into(),
        ));
    }
    if !authorize {
        return Err(DecapodError::ValidationError(
            "INTEGRATION_AUTHORIZATION_REQUIRED: bounded integration removal requires explicit `--authorize`; run `decapod integration preview` first to inspect generated, edited, authored, unknown, and active resources".into(),
        ));
    }
    if let Some(first_blocker) = plan.report.blockers.first() {
        return Err(DecapodError::ValidationError(format!(
            "{}: {} ({})",
            first_blocker.code, first_blocker.detail, first_blocker.path
        )));
    }

    let manifest_abs = root.join(INTEGRATION_RESTORE_MANIFEST_PATH);
    let existing_manifest = read_restore_manifest_at(&root, &manifest_abs)?;

    // Idempotent replay when nothing generated remains to remove and a prior manifest exists
    if plan.removable_files.is_empty()
        && plan.gitignore_state.is_none()
        && let Some(prev) = existing_manifest
    {
        return Ok(IntegrationApplyReport {
            schema_version: "1.0.0".into(),
            status: "already_removed".into(),
            repository_root: root.display().to_string(),
            authorized: true,
            idempotent_replay: true,
            restore_manifest_path: INTEGRATION_RESTORE_MANIFEST_PATH.into(),
            removed_paths: Vec::new(),
            gitignore_rules_removed: Vec::new(),
            preserved_paths: prev.preserved_paths,
            blockers: Vec::new(),
            authority_boundaries: plan.report.authority_boundaries,
        });
    }

    let now = time::now_epoch_z();
    let mut captured_files: BTreeMap<String, RestoredFileEntry> = existing_manifest
        .as_ref()
        .map(|m| {
            m.files
                .iter()
                .map(|f| (f.path.clone(), f.clone()))
                .collect()
        })
        .unwrap_or_default();

    for (rel, kind) in &plan.removable_files {
        let abs = root.join(rel);
        let meta = fs::symlink_metadata(&abs).map_err(DecapodError::IoError)?;
        let bytes = fs::read(&abs).map_err(DecapodError::IoError)?;
        captured_files.insert(
            rel.clone(),
            RestoredFileEntry {
                path: rel.clone(),
                kind: kind.clone(),
                sha256: sha256_bytes(&bytes),
                mode: file_mode(&meta),
                content_hex: encode_hex(&bytes),
            },
        );
    }

    let gitignore_restore_entry =
        if let Some((remove_file, existed, ref rules, ref orig_sha)) = plan.gitignore_state {
            Some(RestoredGitignoreState {
                file_existed: existed,
                remove_created_file: remove_file,
                removed_rules: rules.clone(),
                original_sha256: Some(orig_sha.clone()),
            })
        } else {
            existing_manifest.as_ref().and_then(|m| m.gitignore.clone())
        };

    let mut preserved_paths: Vec<String> = plan
        .report
        .edited
        .iter()
        .chain(plan.report.authored.iter())
        .chain(plan.report.unknown.iter())
        .map(|r| r.path.clone())
        .collect();
    preserved_paths.sort();
    preserved_paths.dedup();

    // Write restore manifest atomically with status="in_progress" BEFORE deleting anything
    // so interrupted removal remains recoverable and idempotent on retry.
    let mut manifest = IntegrationRestoreManifest {
        schema_version: RESTORE_MANIFEST_SCHEMA_VERSION.into(),
        status: "in_progress".into(),
        decapod_release: env!("CARGO_PKG_VERSION").into(),
        created_at: existing_manifest
            .as_ref()
            .map(|m| m.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now.clone(),
        files: captured_files.into_values().collect(),
        gitignore: gitignore_restore_entry,
        preserved_paths: preserved_paths.clone(),
    };
    if let Some(parent) = manifest_abs.parent() {
        fs_permissions::ensure_private_dir(parent).map_err(DecapodError::IoError)?;
    }
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    atomic::write_atomic(&manifest_abs, &manifest_bytes).map_err(DecapodError::IoError)?;

    // Apply bounded removals
    let mut removed_paths = Vec::new();
    for (rel, _) in &plan.removable_files {
        let abs = root.join(rel);
        if fs::symlink_metadata(&abs).is_ok() {
            fs::remove_file(&abs).map_err(DecapodError::IoError)?;
            removed_paths.push(rel.clone());
        }
    }

    let mut gitignore_rules_removed = Vec::new();
    if let Some((remove_file, _, rules, _)) = plan.gitignore_state {
        let gi_path = root.join(".gitignore");
        if remove_file {
            if fs::symlink_metadata(&gi_path).is_ok() {
                fs::remove_file(&gi_path).map_err(DecapodError::IoError)?;
                removed_paths.push(".gitignore".into());
            }
            gitignore_rules_removed = rules;
        } else if let Ok(existing_gi) = fs::read_to_string(&gi_path) {
            let updated_gi = strip_owned_gitignore_rules(&existing_gi, &rules);
            write_repo_file(&root, ".gitignore", updated_gi.as_bytes())?;
            gitignore_rules_removed = rules;
        }
    }

    prune_empty_managed_dirs(&root);

    manifest.status = "applied".into();
    manifest.updated_at = time::now_epoch_z();
    let final_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    atomic::write_atomic(&manifest_abs, &final_bytes).map_err(DecapodError::IoError)?;

    Ok(IntegrationApplyReport {
        schema_version: "1.0.0".into(),
        status: "removed".into(),
        repository_root: root.display().to_string(),
        authorized: true,
        idempotent_replay: false,
        restore_manifest_path: INTEGRATION_RESTORE_MANIFEST_PATH.into(),
        removed_paths,
        gitignore_rules_removed,
        preserved_paths,
        blockers: Vec::new(),
        authority_boundaries: plan.report.authority_boundaries,
    })
}

/// Restore previously removed Decapod repository integration from a restoration manifest.
pub fn restore_integration(
    raw_dir: Option<PathBuf>,
    manifest_override: Option<PathBuf>,
) -> Result<IntegrationRestoreReport, DecapodError> {
    let root = validate_target_root(raw_dir)?;
    let manifest_abs = match manifest_override {
        Some(p) if p.is_absolute() => p,
        Some(p) => root.join(p),
        None => root.join(INTEGRATION_RESTORE_MANIFEST_PATH),
    };

    let manifest = read_restore_manifest_at(&root, &manifest_abs)?.ok_or_else(|| {
        DecapodError::ValidationError(format!(
            "RESTORE_MANIFEST_MISSING: no restoration manifest found at '{}'; run `decapod init` to re-initialize if integration was removed without a manifest",
            manifest_abs.display()
        ))
    })?;

    // Fail closed if any destination path or parent has been replaced by a symlink,
    // or if a destination path now holds conflicting user-authored bytes.
    let mut conflicts = Vec::new();
    let mut decoded_entries: Vec<(&RestoredFileEntry, Vec<u8>)> = Vec::new();
    for entry in &manifest.files {
        let bytes = decode_hex(&entry.content_hex)?;
        if sha256_bytes(&bytes) != entry.sha256 {
            return Err(DecapodError::ValidationError(format!(
                "RESTORE_MANIFEST_DIGEST_MISMATCH: payload digest mismatch for '{}'",
                entry.path
            )));
        }
        match inspect_no_symlink(&root, &entry.path) {
            Ok(Some(meta)) if meta.is_file() => {
                let current = fs::read(root.join(&entry.path)).map_err(DecapodError::IoError)?;
                if current != bytes {
                    conflicts.push(RemovalBlocker {
                        code: "RESTORE_DESTINATION_CONFLICT".into(),
                        path: entry.path.clone(),
                        detail: "Existing file differs from restoration manifest bytes; refusing to overwrite authored changes".into(),
                    });
                }
            }
            Ok(Some(_)) => {
                conflicts.push(RemovalBlocker {
                    code: "RESTORE_DESTINATION_CONFLICT".into(),
                    path: entry.path.clone(),
                    detail: "Destination path exists and is not a regular file".into(),
                });
            }
            Ok(None) => {}
            Err(err) => {
                conflicts.push(RemovalBlocker {
                    code: "INTEGRATION_SYMLINK_REFUSED".into(),
                    path: entry.path.clone(),
                    detail: err.to_string(),
                });
            }
        }
        decoded_entries.push((entry, bytes));
    }

    if manifest.gitignore.is_some()
        && let Err(err) = inspect_no_symlink(&root, ".gitignore")
    {
        conflicts.push(RemovalBlocker {
            code: "INTEGRATION_SYMLINK_REFUSED".into(),
            path: ".gitignore".into(),
            detail: err.to_string(),
        });
    }

    if let Some(first) = conflicts.first() {
        return Err(DecapodError::ValidationError(format!(
            "{}: {} ({})",
            first.code, first.detail, first.path
        )));
    }

    let mut restored_paths = Vec::new();
    let mut skipped_matching_paths = Vec::new();
    for (entry, bytes) in decoded_entries {
        let abs = root.join(&entry.path);
        if abs.is_file() {
            skipped_matching_paths.push(entry.path.clone());
            continue;
        }
        if let Some(parent) = abs.parent() {
            if entry.path.starts_with(".decapod/") {
                fs_permissions::ensure_private_dir(parent).map_err(DecapodError::IoError)?;
            } else {
                fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
            }
        }
        write_repo_file(&root, &entry.path, &bytes)?;
        apply_file_mode(&abs, entry.mode)?;
        restored_paths.push(entry.path.clone());
    }

    let mut gitignore_rules_restored = Vec::new();
    if let Some(gi) = &manifest.gitignore
        && !gi.removed_rules.is_empty()
    {
        let gi_path = root.join(".gitignore");
        let existing = fs::read_to_string(&gi_path).unwrap_or_default();
        let mut updated = existing.clone();
        for rule in &gi.removed_rules {
            if !updated.lines().any(|l| l.trim() == rule) {
                if !updated.is_empty() && !updated.ends_with('\n') {
                    updated.push('\n');
                }
                updated.push_str(rule);
                updated.push('\n');
                gitignore_rules_restored.push(rule.clone());
            }
        }
        if updated != existing || (!gi_path.exists() && gi.remove_created_file) {
            write_repo_file(&root, ".gitignore", updated.as_bytes())?;
        }
    }

    let idempotent_replay = restored_paths.is_empty() && gitignore_rules_restored.is_empty();
    let rel_manifest = manifest_abs
        .strip_prefix(&root)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| manifest_abs.display().to_string());

    Ok(IntegrationRestoreReport {
        schema_version: "1.0.0".into(),
        status: if idempotent_replay {
            "already_restored".into()
        } else {
            "restored".into()
        },
        repository_root: root.display().to_string(),
        manifest_path: rel_manifest,
        idempotent_replay,
        restored_paths,
        gitignore_rules_restored,
        skipped_matching_paths,
        conflicts,
        authority_boundaries: build_authority_boundaries(&root, &[]),
    })
}

pub fn format_preview_text(report: &IntegrationPreviewReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Decapod Integration Preview ({})\n",
        report.repository_root
    ));
    out.push_str(&format!(
        "  Can apply removal: {} | Ownership manifest: {} | Restore manifest: {}\n",
        report.can_apply, report.ownership_manifest_present, report.restore_manifest_present
    ));
    out.push_str(&format!(
        "  Generated (removable): {}\n",
        report.generated.len()
    ));
    for item in &report.generated {
        out.push_str(&format!(
            "    - [{}] {} ({})\n",
            item.kind, item.path, item.ownership_evidence
        ));
    }
    out.push_str(&format!("  Edited (preserved): {}\n", report.edited.len()));
    for item in &report.edited {
        out.push_str(&format!(
            "    - [{}] {} ({})\n",
            item.kind, item.path, item.diagnostic
        ));
    }
    out.push_str(&format!(
        "  Authored (preserved): {}\n",
        report.authored.len()
    ));
    for item in &report.authored {
        out.push_str(&format!(
            "    - [{}] {} ({})\n",
            item.kind, item.path, item.diagnostic
        ));
    }
    out.push_str(&format!(
        "  Unknown (preserved): {}\n",
        report.unknown.len()
    ));
    for item in &report.unknown {
        out.push_str(&format!(
            "    - [{}] {} ({})\n",
            item.kind, item.path, item.diagnostic
        ));
    }
    out.push_str(&format!(
        "  Active custody / conflicts: {}\n",
        report.active.len()
    ));
    for item in &report.active {
        out.push_str(&format!(
            "    - [{}] {}: {}\n",
            item.kind, item.path_or_id, item.detail
        ));
    }
    if !report.blockers.is_empty() {
        out.push_str(&format!("  Blockers: {}\n", report.blockers.len()));
        for b in &report.blockers {
            out.push_str(&format!("    - [{}] {}: {}\n", b.code, b.path, b.detail));
        }
    }
    out
}

pub fn format_apply_text(report: &IntegrationApplyReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Decapod Integration Remove ({}): {}\n",
        report.status, report.repository_root
    ));
    out.push_str(&format!(
        "  Restore manifest: {}\n",
        report.restore_manifest_path
    ));
    out.push_str(&format!(
        "  Removed files: {}\n",
        report.removed_paths.len()
    ));
    for p in &report.removed_paths {
        out.push_str(&format!("    - {p}\n"));
    }
    if !report.gitignore_rules_removed.is_empty() {
        out.push_str(&format!(
            "  Removed .gitignore entries: {}\n",
            report.gitignore_rules_removed.len()
        ));
    }
    out.push_str(&format!(
        "  Preserved paths: {}\n",
        report.preserved_paths.len()
    ));
    out
}

pub fn format_restore_text(report: &IntegrationRestoreReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Decapod Integration Restore ({}): {}\n",
        report.status, report.repository_root
    ));
    out.push_str(&format!("  Manifest: {}\n", report.manifest_path));
    out.push_str(&format!(
        "  Restored files: {}\n",
        report.restored_paths.len()
    ));
    for p in &report.restored_paths {
        out.push_str(&format!("    - {p}\n"));
    }
    if !report.gitignore_rules_restored.is_empty() {
        out.push_str(&format!(
            "  Restored .gitignore entries: {}\n",
            report.gitignore_rules_restored.len()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_validation_rejects_invalid_versions() {
        assert!(is_valid_semver("0.149.2"));
        assert!(is_valid_semver("1.0.0"));
        assert!(!is_valid_semver("01.0.0"));
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("1.0.0-alpha"));
    }

    #[test]
    fn hex_roundtrip_and_reject_invalid() {
        let bytes = b"decapod-1704-\x00\xff";
        let hex = encode_hex(bytes);
        assert_eq!(decode_hex(&hex).unwrap(), bytes);
        assert!(decode_hex("abc").is_err());
        assert!(decode_hex("zz").is_err());
    }

    #[test]
    fn strip_owned_gitignore_rules_preserves_user_lines() {
        let content = "# User\n/dist/\n.decapod/data\n*.tmp\n.decapod/workspaces/\n";
        let owned = vec![
            ".decapod/data".to_string(),
            ".decapod/workspaces/".to_string(),
        ];
        let stripped = strip_owned_gitignore_rules(content, &owned);
        assert_eq!(stripped, "# User\n/dist/\n*.tmp\n");
    }

    #[test]
    fn entrypoint_markers_require_release_or_fingerprint_header() {
        let canonical = crate::core::assets::canonical_template("AGENTS.md").unwrap();
        assert!(!has_decapod_entrypoint_markers(&canonical));
        assert!(has_decapod_entrypoint_markers(
            "<!-- decapod-release: 0.149.2 -->\n"
        ));
        assert!(has_decapod_entrypoint_markers(
            "<!-- decapod-fingerprint: abc -->\n"
        ));
    }
}

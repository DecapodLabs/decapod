//! Internalized Context Artifacts plugin.
//!
//! Provides governance-native lifecycle for context internalization:
//! turning long documents into mountable, verifiable context adapters
//! so agents stop paying the long-context tax over and over.
//!
//! Artifacts are produced by pluggable "internalizer profiles" (external
//! executables) and stored under `.decapod/generated/artifacts/internalizations/`.
//!
//! Truth label: REAL
//! Proof surface: `decapod internalize inspect --id <id>`

use crate::core::store::Store;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(clap::Args, Debug)]
pub struct InternalizeCli {
    #[clap(subcommand)]
    pub command: InternalizeCommand,
}

#[derive(Subcommand, Debug)]
pub enum InternalizeCommand {
    /// Produce an internalized context artifact from a source document
    Create {
        #[clap(long)]
        source: String,
        #[clap(long)]
        model: String,
        #[clap(long, default_value = "noop")]
        profile: String,
        #[clap(long, default_value_t = 0)]
        ttl: u64,
        #[clap(long = "scope", value_delimiter = ',')]
        scopes: Vec<String>,
        #[clap(long, default_value = "json")]
        format: String,
    },
    /// Attach an internalized context artifact to a session-scoped mount lease
    Attach {
        #[clap(long)]
        id: String,
        #[clap(long)]
        session: String,
        #[clap(long, default_value = "decapod-cli")]
        tool: String,
        #[clap(long, default_value_t = 1800)]
        lease_seconds: u64,
        #[clap(long, default_value = "json")]
        format: String,
    },
    /// Explicitly revoke a session-scoped internalization mount
    Detach {
        #[clap(long)]
        id: String,
        #[clap(long)]
        session: String,
        #[clap(long, default_value = "json")]
        format: String,
    },
    /// Inspect an internalized context artifact (manifest + integrity)
    Inspect {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "json")]
        format: String,
    },
}

pub const SCHEMA_VERSION: &str = "1.2.0";
pub const DEFAULT_ATTACH_LEASE_SECONDS: u64 = 1800;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeterminismClass {
    Deterministic,
    BestEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    Replayable,
    NonReplayable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InternalizationManifest {
    pub schema_version: String,
    pub id: String,
    pub source_hash: String,
    pub source_path: String,
    pub extraction_method: String,
    pub chunking_params: BTreeMap<String, serde_json::Value>,
    pub base_model_id: String,
    pub internalizer_profile: String,
    pub internalizer_version: String,
    pub adapter_format: String,
    pub created_at: String,
    pub ttl_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    pub provenance: Vec<ProvenanceEntry>,
    pub replay_recipe: ReplayRecipe,
    pub adapter_hash: String,
    pub adapter_path: String,
    pub capabilities_contract: CapabilitiesContract,
    pub risk_tier: RiskTier,
    pub determinism_class: DeterminismClass,
    pub binary_hash: String,
    pub runtime_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProvenanceEntry {
    pub op: String,
    pub timestamp: String,
    pub actor: String,
    pub inputs_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayRecipe {
    pub mode: ReplayClass,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilitiesContract {
    pub allowed_scopes: Vec<String>,
    pub permitted_tools: Vec<String>,
    pub allow_code_gen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskTier {
    pub creation: String,
    pub attach: String,
    pub inspect: String,
}

impl Default for RiskTier {
    fn default() -> Self {
        Self {
            creation: "compute-risky".to_string(),
            attach: "behavior-changing".to_string(),
            inspect: "read-only".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationCreateResult {
    pub schema_version: String,
    pub success: bool,
    pub artifact_id: String,
    pub artifact_path: String,
    pub cache_hit: bool,
    pub manifest: InternalizationManifest,
    pub source_hash: String,
    pub adapter_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationAttachResult {
    pub schema_version: String,
    pub success: bool,
    pub artifact_id: String,
    pub session_id: String,
    pub tool: String,
    pub attached_at: String,
    pub lease_id: String,
    pub lease_seconds: u64,
    pub lease_expires_at: String,
    pub expires_at: Option<String>,
    pub capabilities_contract: CapabilitiesContract,
    pub risk_classification: String,
    pub source_verification: String,
    pub provenance_entry: ProvenanceEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationDetachResult {
    pub schema_version: String,
    pub success: bool,
    pub artifact_id: String,
    pub session_id: String,
    pub detached_at: String,
    pub lease_id: String,
    pub detached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationInspectResult {
    pub schema_version: String,
    pub artifact_id: String,
    pub manifest: InternalizationManifest,
    pub integrity: IntegrityCheck,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheck {
    pub source_hash_valid: bool,
    pub source_verification: String,
    pub adapter_hash_valid: bool,
    pub manifest_consistent: bool,
    pub expired: bool,
    pub replayable_claim_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizerProfile {
    pub name: String,
    pub version: String,
    pub executable: String,
    pub default_params: BTreeMap<String, serde_json::Value>,
    pub adapter_format: String,
    pub determinism_class: DeterminismClass,
}

impl InternalizerProfile {
    pub fn noop() -> Self {
        Self {
            name: "noop".to_string(),
            version: "1.0.0".to_string(),
            executable: "builtin:noop".to_string(),
            default_params: BTreeMap::new(),
            adapter_format: "noop".to_string(),
            determinism_class: DeterminismClass::Deterministic,
        }
    }

    pub fn resolve(name: &str, store_root: &Path) -> Result<Self, InternalizeError> {
        if name == "noop" {
            return Ok(Self::noop());
        }
        let profile_path = control_root(store_root)
            .join("generated")
            .join("profiles")
            .join("internalizers")
            .join(format!("{}.json", name));
        if !profile_path.exists() {
            return Err(InternalizeError::ProfileNotFound(name.to_string()));
        }
        let raw = fs::read_to_string(&profile_path).map_err(InternalizeError::Io)?;
        serde_json::from_str(&raw).map_err(InternalizeError::Json)
    }

    pub fn binary_hash(&self) -> Result<String, InternalizeError> {
        if self.executable == "builtin:noop" {
            return sha256_bytes(self.executable.as_bytes());
        }
        let path = Path::new(&self.executable);
        if !path.exists() {
            return Err(InternalizeError::ProfileExecution(format!(
                "Internalizer binary not found: {}",
                self.executable
            )));
        }
        sha256_file(path)
    }

    pub fn runtime_fingerprint(&self) -> String {
        format!(
            "os={} arch={} executable={}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            self.executable
        )
    }

    pub fn execute(
        &self,
        source_path: &Path,
        base_model: &str,
        output_dir: &Path,
    ) -> Result<(PathBuf, BTreeMap<String, serde_json::Value>), InternalizeError> {
        let adapter_file = output_dir.join("adapter.bin");

        if self.executable == "builtin:noop" {
            fs::write(&adapter_file, b"").map_err(InternalizeError::Io)?;
            return Ok((adapter_file, self.default_params.clone()));
        }

        let input = serde_json::json!({
            "source_path": source_path.to_string_lossy(),
            "base_model": base_model,
            "output_dir": output_dir.to_string_lossy(),
            "params": self.default_params,
        });

        let output = ProcessCommand::new(&self.executable)
            .arg("--input")
            .arg(serde_json::to_string(&input).unwrap_or_default())
            .output()
            .map_err(InternalizeError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(InternalizeError::ProfileExecution(format!(
                "Internalizer '{}' failed: {}",
                self.name, stderr
            )));
        }

        if !adapter_file.exists() {
            return Err(InternalizeError::ProfileExecution(format!(
                "Internalizer '{}' did not produce adapter at {}",
                self.name,
                adapter_file.display()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let params = serde_json::from_str(&stdout).unwrap_or_else(|_| self.default_params.clone());

        Ok((adapter_file, params))
    }
}

#[derive(Debug)]
pub enum InternalizeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ProfileNotFound(String),
    ProfileExecution(String),
    ArtifactNotFound(String),
    MountNotFound {
        artifact_id: String,
        session_id: String,
    },
    SourceIntegrityFailed {
        expected: String,
        actual: String,
    },
    AdapterIntegrityFailed {
        expected: String,
        actual: String,
    },
    Expired {
        artifact_id: String,
        expired_at: String,
    },
    ToolNotPermitted {
        tool: String,
        artifact_id: String,
    },
    ValidationError(String),
}

impl std::fmt::Display for InternalizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Json(e) => write!(f, "JSON error: {}", e),
            Self::ProfileNotFound(n) => write!(f, "Internalizer profile '{}' not found", n),
            Self::ProfileExecution(s) => write!(f, "Profile execution error: {}", s),
            Self::ArtifactNotFound(id) => write!(f, "Artifact '{}' not found", id),
            Self::MountNotFound {
                artifact_id,
                session_id,
            } => write!(
                f,
                "No active mount for artifact '{}' in session '{}'",
                artifact_id, session_id
            ),
            Self::SourceIntegrityFailed { expected, actual } => write!(
                f,
                "Source integrity check failed: expected {}, got {}",
                expected, actual
            ),
            Self::AdapterIntegrityFailed { expected, actual } => write!(
                f,
                "Adapter integrity check failed: expected {}, got {}",
                expected, actual
            ),
            Self::Expired {
                artifact_id,
                expired_at,
            } => write!(
                f,
                "Artifact '{}' expired at {}; renew with a new create",
                artifact_id, expired_at
            ),
            Self::ToolNotPermitted { tool, artifact_id } => write!(
                f,
                "Tool '{}' is not permitted to mount artifact '{}'",
                tool, artifact_id
            ),
            Self::ValidationError(s) => write!(f, "Validation error: {}", s),
        }
    }
}

impl std::error::Error for InternalizeError {}

impl From<InternalizeError> for crate::core::error::DecapodError {
    fn from(e: InternalizeError) -> Self {
        crate::core::error::DecapodError::ValidationError(e.to_string())
    }
}

fn sha256_file(path: &Path) -> Result<String, InternalizeError> {
    let bytes = fs::read(path).map_err(InternalizeError::Io)?;
    sha256_bytes(&bytes)
}

fn sha256_bytes(bytes: &[u8]) -> Result<String, InternalizeError> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn iso8601_from_epoch(secs: u64) -> String {
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let mut year = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md as i64 {
            month = i;
            break;
        }
        remaining_days -= md as i64;
    }
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn now_iso8601() -> String {
    iso8601_from_epoch(now_unix())
}

fn iso8601_after_secs(secs: u64) -> String {
    iso8601_from_epoch(now_unix().saturating_add(secs))
}

fn control_root(store_root: &Path) -> PathBuf {
    if store_root.file_name().and_then(|s| s.to_str()) == Some("data")
        && store_root
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            == Some(".decapod")
    {
        store_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| store_root.to_path_buf())
    } else {
        store_root.to_path_buf()
    }
}

fn artifacts_dir(store_root: &Path) -> PathBuf {
    control_root(store_root)
        .join("generated")
        .join("artifacts")
        .join("internalizations")
}

fn artifact_dir(store_root: &Path, id: &str) -> PathBuf {
    artifacts_dir(store_root).join(id)
}

fn session_dir(store_root: &Path, session_id: &str) -> PathBuf {
    control_root(store_root)
        .join("generated")
        .join("sessions")
        .join(session_id)
}

fn mount_dir(store_root: &Path, session_id: &str) -> PathBuf {
    session_dir(store_root, session_id).join("internalize_mounts")
}

fn mount_id(artifact_id: &str) -> String {
    format!("mount_{}", artifact_id)
}

fn mount_path(store_root: &Path, session_id: &str, artifact_id: &str) -> PathBuf {
    mount_dir(store_root, session_id).join(format!("{}.json", mount_id(artifact_id)))
}

fn is_non_local_source(source: &str) -> bool {
    source == "-" || source.starts_with("http://") || source.starts_with("https://")
}

fn is_expired(expires_at: Option<&str>) -> bool {
    expires_at.is_some_and(|exp| now_iso8601().as_str() > exp)
}

fn verify_source_binding(
    manifest: &InternalizationManifest,
) -> Result<(bool, String), InternalizeError> {
    if manifest.source_path == "-" {
        return Ok((false, "best-effort-stdin-source".to_string()));
    }
    if manifest.source_path.starts_with("http://") || manifest.source_path.starts_with("https://") {
        return Ok((false, "best-effort-nonlocal-source".to_string()));
    }

    let source_path = Path::new(&manifest.source_path);
    if !source_path.exists() {
        return Ok((false, "best-effort-source-unavailable".to_string()));
    }

    let actual = sha256_file(source_path)?;
    if actual == manifest.source_hash {
        Ok((true, "verified".to_string()))
    } else {
        Ok((false, "mismatch".to_string()))
    }
}

fn tool_is_permitted(contract: &CapabilitiesContract, tool: &str) -> bool {
    contract
        .permitted_tools
        .iter()
        .any(|entry| entry == "*" || entry == tool)
}

fn artifact_id_for_request(
    source_hash: &str,
    source_path: &str,
    model: &str,
    profile: &InternalizerProfile,
    ttl: u64,
    scopes: &[String],
) -> Result<String, InternalizeError> {
    let mut normalized_scopes = scopes.to_vec();
    normalized_scopes.sort();
    normalized_scopes.dedup();
    let binding = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "source_hash": source_hash,
        "source_path": source_path,
        "base_model_id": model,
        "internalizer_profile": profile.name,
        "internalizer_version": profile.version,
        "adapter_format": profile.adapter_format,
        "determinism_class": profile.determinism_class,
        "ttl_seconds": ttl,
        "scopes": normalized_scopes,
        "chunking_params": profile.default_params,
    });
    let bytes = serde_json::to_vec(&binding).map_err(InternalizeError::Json)?;
    let hex = sha256_bytes(&bytes)?;
    Ok(format!("int_{}", &hex[..24]))
}

fn build_replay_recipe(
    profile: &InternalizerProfile,
    binary_hash: &str,
    source_path: &str,
    model: &str,
    ttl: u64,
    scopes: &[String],
) -> ReplayRecipe {
    let mut replay_args = vec![
        "internalize".to_string(),
        "create".to_string(),
        "--source".to_string(),
        source_path.to_string(),
        "--model".to_string(),
        model.to_string(),
        "--profile".to_string(),
        profile.name.clone(),
    ];
    if ttl > 0 {
        replay_args.push("--ttl".to_string());
        replay_args.push(ttl.to_string());
    }
    for scope in scopes {
        replay_args.push("--scope".to_string());
        replay_args.push(scope.clone());
    }

    let (mode, reason) = match profile.determinism_class {
        DeterminismClass::Deterministic if !binary_hash.is_empty() => (
            ReplayClass::Replayable,
            "deterministic profile with pinned binary hash".to_string(),
        ),
        DeterminismClass::Deterministic => (
            ReplayClass::NonReplayable,
            "deterministic profile missing pinned binary hash".to_string(),
        ),
        DeterminismClass::BestEffort => (
            ReplayClass::NonReplayable,
            "best_effort profile may depend on nondeterministic runtime or hardware".to_string(),
        ),
    };

    ReplayRecipe {
        mode,
        command: "decapod".to_string(),
        args: replay_args,
        env: BTreeMap::new(),
        reason,
    }
}

fn replayable_claim_valid(manifest: &InternalizationManifest) -> bool {
    match manifest.replay_recipe.mode {
        ReplayClass::Replayable => {
            manifest.determinism_class == DeterminismClass::Deterministic
                && !manifest.binary_hash.trim().is_empty()
        }
        ReplayClass::NonReplayable => {
            if manifest.determinism_class == DeterminismClass::BestEffort {
                !manifest.binary_hash.trim().is_empty()
                    && !manifest.runtime_fingerprint.trim().is_empty()
            } else {
                true
            }
        }
    }
}

pub fn create_internalization(
    store_root: &Path,
    source: &str,
    model: &str,
    profile_name: &str,
    ttl: u64,
    scopes: &[String],
) -> Result<InternalizationCreateResult, InternalizeError> {
    if is_non_local_source(source) {
        return Err(InternalizeError::ValidationError(
            "MVP only supports local file sources; URL and stdin sources are intentionally not implemented"
                .to_string(),
        ));
    }

    let source_path = Path::new(source);
    if !source_path.exists() {
        return Err(InternalizeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Source document not found: {}", source),
        )));
    }
    let canonical_source = fs::canonicalize(source_path).map_err(InternalizeError::Io)?;
    let source_hash = sha256_file(&canonical_source)?;
    let profile = InternalizerProfile::resolve(profile_name, store_root)?;

    let effective_scopes = if scopes.is_empty() {
        vec!["qa".to_string()]
    } else {
        let mut normalized = scopes.to_vec();
        normalized.sort();
        normalized.dedup();
        normalized
    };
    let allow_code_gen = effective_scopes.iter().any(|s| s == "code-gen");
    let binary_hash = profile.binary_hash()?;
    let runtime_fingerprint = profile.runtime_fingerprint();
    let source_path_string = canonical_source.to_string_lossy().to_string();
    let artifact_id = artifact_id_for_request(
        &source_hash,
        &source_path_string,
        model,
        &profile,
        ttl,
        &effective_scopes,
    )?;
    let art_dir = artifact_dir(store_root, &artifact_id);
    let manifest_path = art_dir.join("manifest.json");
    if manifest_path.exists() {
        let raw = fs::read_to_string(&manifest_path).map_err(InternalizeError::Io)?;
        let manifest: InternalizationManifest =
            serde_json::from_str(&raw).map_err(InternalizeError::Json)?;
        return Ok(InternalizationCreateResult {
            schema_version: SCHEMA_VERSION.to_string(),
            success: true,
            artifact_id,
            artifact_path: art_dir.to_string_lossy().to_string(),
            cache_hit: true,
            source_hash: manifest.source_hash.clone(),
            adapter_hash: manifest.adapter_hash.clone(),
            manifest,
        });
    }

    fs::create_dir_all(&art_dir).map_err(InternalizeError::Io)?;
    let (adapter_path, chunking_params) = profile.execute(&canonical_source, model, &art_dir)?;
    let adapter_hash = sha256_file(&adapter_path)?;
    let now = now_iso8601();
    let expires_at = if ttl > 0 {
        Some(iso8601_after_secs(ttl))
    } else {
        None
    };

    let replay_recipe = build_replay_recipe(
        &profile,
        &binary_hash,
        &source_path_string,
        model,
        ttl,
        &effective_scopes,
    );
    let provenance_entry = ProvenanceEntry {
        op: "internalize.create".to_string(),
        timestamp: now.clone(),
        actor: "decapod-cli".to_string(),
        inputs_hash: source_hash.clone(),
    };

    let manifest = InternalizationManifest {
        schema_version: SCHEMA_VERSION.to_string(),
        id: artifact_id.clone(),
        source_hash: source_hash.clone(),
        source_path: source_path_string,
        extraction_method: profile.name.clone(),
        chunking_params,
        base_model_id: model.to_string(),
        internalizer_profile: profile.name.clone(),
        internalizer_version: profile.version.clone(),
        adapter_format: profile.adapter_format.clone(),
        created_at: now,
        ttl_seconds: ttl,
        expires_at,
        provenance: vec![provenance_entry],
        replay_recipe,
        adapter_hash: adapter_hash.clone(),
        adapter_path: "adapter.bin".to_string(),
        capabilities_contract: CapabilitiesContract {
            allowed_scopes: effective_scopes,
            permitted_tools: vec!["decapod-cli".to_string()],
            allow_code_gen,
        },
        risk_tier: RiskTier::default(),
        determinism_class: profile.determinism_class,
        binary_hash,
        runtime_fingerprint,
    };

    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(InternalizeError::Json)?;
    fs::write(&manifest_path, manifest_json).map_err(InternalizeError::Io)?;

    Ok(InternalizationCreateResult {
        schema_version: SCHEMA_VERSION.to_string(),
        success: true,
        artifact_id,
        artifact_path: art_dir.to_string_lossy().to_string(),
        cache_hit: false,
        manifest,
        source_hash,
        adapter_hash,
    })
}

pub fn inspect_internalization(
    store_root: &Path,
    id: &str,
) -> Result<InternalizationInspectResult, InternalizeError> {
    let art_dir = artifact_dir(store_root, id);
    let manifest_path = art_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(InternalizeError::ArtifactNotFound(id.to_string()));
    }

    let raw = fs::read_to_string(&manifest_path).map_err(InternalizeError::Io)?;
    let manifest: InternalizationManifest =
        serde_json::from_str(&raw).map_err(InternalizeError::Json)?;

    let (source_hash_valid, source_verification) = verify_source_binding(&manifest)?;
    let adapter_full_path = art_dir.join(&manifest.adapter_path);
    let adapter_hash_valid = if adapter_full_path.exists() {
        sha256_file(&adapter_full_path)? == manifest.adapter_hash
    } else {
        false
    };
    let expired = is_expired(manifest.expires_at.as_deref());
    let replayable_claim_valid = replayable_claim_valid(&manifest);

    let status = if expired {
        "expired".to_string()
    } else if !adapter_hash_valid || source_verification == "mismatch" || !replayable_claim_valid {
        "integrity-failed".to_string()
    } else if source_verification.starts_with("best-effort") {
        "best-effort".to_string()
    } else {
        "valid".to_string()
    };

    Ok(InternalizationInspectResult {
        schema_version: SCHEMA_VERSION.to_string(),
        artifact_id: id.to_string(),
        manifest,
        integrity: IntegrityCheck {
            source_hash_valid,
            source_verification,
            adapter_hash_valid,
            manifest_consistent: true,
            expired,
            replayable_claim_valid,
        },
        status,
    })
}

pub fn attach_internalization(
    store_root: &Path,
    id: &str,
    session_id: &str,
    tool: &str,
    lease_seconds: u64,
) -> Result<InternalizationAttachResult, InternalizeError> {
    let inspection = inspect_internalization(store_root, id)?;

    if inspection.integrity.expired {
        return Err(InternalizeError::Expired {
            artifact_id: id.to_string(),
            expired_at: inspection
                .manifest
                .expires_at
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        });
    }
    if inspection.integrity.source_verification == "mismatch" {
        let actual = if Path::new(&inspection.manifest.source_path).exists() {
            sha256_file(Path::new(&inspection.manifest.source_path))?
        } else {
            "unavailable".to_string()
        };
        return Err(InternalizeError::SourceIntegrityFailed {
            expected: inspection.manifest.source_hash.clone(),
            actual,
        });
    }
    if !inspection.integrity.adapter_hash_valid {
        return Err(InternalizeError::AdapterIntegrityFailed {
            expected: inspection.manifest.adapter_hash.clone(),
            actual: "corrupted".to_string(),
        });
    }
    if !inspection.integrity.replayable_claim_valid {
        return Err(InternalizeError::ValidationError(
            "Artifact replayability metadata is inconsistent with determinism policy".to_string(),
        ));
    }
    if !tool_is_permitted(&inspection.manifest.capabilities_contract, tool) {
        return Err(InternalizeError::ToolNotPermitted {
            tool: tool.to_string(),
            artifact_id: id.to_string(),
        });
    }

    let attached_at = now_iso8601();
    let lease_id = mount_id(id);
    let lease_expires_at = iso8601_after_secs(lease_seconds);
    let provenance_entry = ProvenanceEntry {
        op: "internalize.attach".to_string(),
        timestamp: attached_at.clone(),
        actor: format!("session:{}", session_id),
        inputs_hash: inspection.manifest.adapter_hash.clone(),
    };

    let mounts_dir = mount_dir(store_root, session_id);
    fs::create_dir_all(&mounts_dir).map_err(InternalizeError::Io)?;
    let mount = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "artifact_id": id,
        "session_id": session_id,
        "tool": tool,
        "lease_id": lease_id,
        "lease_seconds": lease_seconds,
        "mounted_at": attached_at,
        "lease_expires_at": lease_expires_at,
        "adapter_hash": inspection.manifest.adapter_hash,
        "source_verification": inspection.integrity.source_verification,
        "capabilities_contract": inspection.manifest.capabilities_contract,
        "risk_classification": inspection.manifest.risk_tier.attach
    });
    fs::write(
        mount_path(store_root, session_id, id),
        serde_json::to_string_pretty(&mount).map_err(InternalizeError::Json)?,
    )
    .map_err(InternalizeError::Io)?;

    let session_prov_dir = session_dir(store_root, session_id);
    fs::create_dir_all(&session_prov_dir).map_err(InternalizeError::Io)?;
    let attach_log = session_prov_dir.join(format!("internalize_attach_{}.json", id));
    let attach_entry = serde_json::json!({
        "op": "internalize.attach",
        "artifact_id": id,
        "session_id": session_id,
        "tool": tool,
        "lease_id": lease_id,
        "lease_seconds": lease_seconds,
        "lease_expires_at": lease_expires_at,
        "timestamp": attached_at,
        "adapter_hash": inspection.manifest.adapter_hash,
        "capabilities_contract": inspection.manifest.capabilities_contract,
        "risk_classification": inspection.manifest.risk_tier.attach,
        "source_verification": inspection.integrity.source_verification,
    });
    fs::write(
        attach_log,
        serde_json::to_string_pretty(&attach_entry).map_err(InternalizeError::Json)?,
    )
    .map_err(InternalizeError::Io)?;

    Ok(InternalizationAttachResult {
        schema_version: SCHEMA_VERSION.to_string(),
        success: true,
        artifact_id: id.to_string(),
        session_id: session_id.to_string(),
        tool: tool.to_string(),
        attached_at,
        lease_id,
        lease_seconds,
        lease_expires_at,
        expires_at: inspection.manifest.expires_at,
        capabilities_contract: inspection.manifest.capabilities_contract,
        risk_classification: inspection.manifest.risk_tier.attach,
        source_verification: inspection.integrity.source_verification,
        provenance_entry,
    })
}

pub fn detach_internalization(
    store_root: &Path,
    id: &str,
    session_id: &str,
) -> Result<InternalizationDetachResult, InternalizeError> {
    let mount_file = mount_path(store_root, session_id, id);
    if !mount_file.exists() {
        return Err(InternalizeError::MountNotFound {
            artifact_id: id.to_string(),
            session_id: session_id.to_string(),
        });
    }

    let raw = fs::read_to_string(&mount_file).map_err(InternalizeError::Io)?;
    let mount: serde_json::Value = serde_json::from_str(&raw).map_err(InternalizeError::Json)?;
    let lease_id = mount
        .get("lease_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    fs::remove_file(&mount_file).map_err(InternalizeError::Io)?;

    let detached_at = now_iso8601();
    let session_prov_dir = session_dir(store_root, session_id);
    fs::create_dir_all(&session_prov_dir).map_err(InternalizeError::Io)?;
    let detach_log = session_prov_dir.join(format!("internalize_detach_{}.json", id));
    let detach_entry = serde_json::json!({
        "op": "internalize.detach",
        "artifact_id": id,
        "session_id": session_id,
        "lease_id": lease_id,
        "timestamp": detached_at,
    });
    fs::write(
        detach_log,
        serde_json::to_string_pretty(&detach_entry).map_err(InternalizeError::Json)?,
    )
    .map_err(InternalizeError::Io)?;

    Ok(InternalizationDetachResult {
        schema_version: SCHEMA_VERSION.to_string(),
        success: true,
        artifact_id: id.to_string(),
        session_id: session_id.to_string(),
        detached_at,
        lease_id,
        detached: true,
    })
}

pub fn manifest_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://decapod.dev/schemas/internalization/manifest-1.2.0.json",
        "title": "InternalizationManifest",
        "type": "object",
        "required": [
            "schema_version", "id", "source_hash", "source_path", "base_model_id",
            "internalizer_profile", "internalizer_version", "adapter_format", "created_at",
            "ttl_seconds", "provenance", "replay_recipe", "adapter_hash", "adapter_path",
            "capabilities_contract", "risk_tier", "determinism_class", "binary_hash",
            "runtime_fingerprint"
        ],
        "properties": {
            "schema_version": { "const": SCHEMA_VERSION },
            "id": { "type": "string", "pattern": "^int_[a-f0-9]{24}$" },
            "source_hash": { "type": "string", "pattern": "^[a-f0-9]{64}$" },
            "determinism_class": { "enum": ["deterministic", "best_effort"] },
            "binary_hash": { "type": "string", "minLength": 1 },
            "runtime_fingerprint": { "type": "string", "minLength": 1 }
        }
    })
}

pub fn create_result_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://decapod.dev/schemas/internalization/create-result-1.2.0.json",
        "title": "InternalizationCreateResult",
        "type": "object",
        "required": [
            "schema_version", "success", "artifact_id", "artifact_path",
            "cache_hit", "manifest", "source_hash", "adapter_hash"
        ]
    })
}

pub fn attach_result_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://decapod.dev/schemas/internalization/attach-result-1.2.0.json",
        "title": "InternalizationAttachResult",
        "type": "object",
        "required": [
            "schema_version", "success", "artifact_id", "session_id", "tool",
            "attached_at", "lease_id", "lease_seconds", "lease_expires_at"
        ]
    })
}

pub fn detach_result_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://decapod.dev/schemas/internalization/detach-result-1.2.0.json",
        "title": "InternalizationDetachResult",
        "type": "object",
        "required": [
            "schema_version", "success", "artifact_id", "session_id",
            "detached_at", "lease_id", "detached"
        ]
    })
}

pub fn inspect_result_json_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://decapod.dev/schemas/internalization/inspect-result-1.2.0.json",
        "title": "InternalizationInspectResult",
        "type": "object",
        "required": ["schema_version", "artifact_id", "manifest", "integrity", "status"]
    })
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "internalize",
        "version": SCHEMA_VERSION,
        "description": "Internalized context artifact lifecycle with explicit create, attach lease, detach, and inspect gates",
        "commands": [
            { "name": "create", "parameters": ["source", "model", "profile", "ttl", "scope", "format"] },
            { "name": "attach", "parameters": ["id", "session", "tool", "lease_seconds", "format"] },
            { "name": "detach", "parameters": ["id", "session", "format"] },
            { "name": "inspect", "parameters": ["id", "format"] }
        ]
    })
}

pub fn run_internalize_cli(
    _store: &Store,
    store_root: &Path,
    cli: InternalizeCli,
) -> Result<(), crate::core::error::DecapodError> {
    match cli.command {
        InternalizeCommand::Create {
            source,
            model,
            profile,
            ttl,
            scopes,
            format,
        } => {
            let result =
                create_internalization(store_root, &source, &model, &profile, ttl, &scopes)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Created internalization artifact: {}", result.artifact_id);
            }
        }
        InternalizeCommand::Attach {
            id,
            session,
            tool,
            lease_seconds,
            format,
        } => {
            let result = attach_internalization(store_root, &id, &session, &tool, lease_seconds)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!(
                    "Attached {} to session {} until {}",
                    result.artifact_id, result.session_id, result.lease_expires_at
                );
            }
        }
        InternalizeCommand::Detach {
            id,
            session,
            format,
        } => {
            let result = detach_internalization(store_root, &id, &session)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!(
                    "Detached {} from session {}",
                    result.artifact_id, result.session_id
                );
            }
        }
        InternalizeCommand::Inspect { id, format } => {
            let result = inspect_internalization(store_root, &id)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Artifact: {}", result.artifact_id);
                println!("  Status: {}", result.status);
            }
        }
    }
    Ok(())
}

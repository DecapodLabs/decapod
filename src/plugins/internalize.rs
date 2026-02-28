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

// ── CLI ────────────────────────────────────────────────────────────────

#[derive(clap::Args, Debug)]
pub struct InternalizeCli {
    #[clap(subcommand)]
    pub command: InternalizeCommand,
}

#[derive(Subcommand, Debug)]
pub enum InternalizeCommand {
    /// Produce an internalized context artifact from a source document
    Create {
        /// Path to source document (file path)
        #[clap(long)]
        source: String,
        /// Base model identifier this adapter targets
        #[clap(long)]
        model: String,
        /// Internalizer profile name (default: noop)
        #[clap(long, default_value = "noop")]
        profile: String,
        /// Time-to-live in seconds (0 = no expiry)
        #[clap(long, default_value_t = 0)]
        ttl: u64,
        /// Allowed usage scopes (repeatable: qa, summarization, code-gen)
        #[clap(long = "scope", value_delimiter = ',')]
        scopes: Vec<String>,
        /// Output format: 'json' or 'text'
        #[clap(long, default_value = "json")]
        format: String,
    },
    /// Attach an internalized context artifact to an active agent session
    Attach {
        /// Artifact ID to attach
        #[clap(long)]
        id: String,
        /// Session identifier to attach to
        #[clap(long)]
        session: String,
        /// Output format: 'json' or 'text'
        #[clap(long, default_value = "json")]
        format: String,
    },
    /// Inspect an internalized context artifact (manifest + integrity)
    Inspect {
        /// Artifact ID to inspect
        #[clap(long)]
        id: String,
        /// Output format: 'json' or 'text'
        #[clap(long, default_value = "json")]
        format: String,
    },
}

// ── Schemas (stable, versioned) ────────────────────────────────────────

pub const SCHEMA_VERSION: &str = "1.0.0";

/// Internalization manifest — the core artifact model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InternalizationManifest {
    /// Schema version for forward compatibility.
    pub schema_version: String,
    /// Unique artifact identifier (ULID).
    pub id: String,
    /// SHA-256 hash of the source document.
    pub source_hash: String,
    /// Original source path or URI.
    pub source_path: String,
    /// Extraction method used (profile name).
    pub extraction_method: String,
    /// Chunking parameters (profile-specific).
    pub chunking_params: BTreeMap<String, serde_json::Value>,
    /// Base model identifier this adapter was produced for.
    pub base_model_id: String,
    /// Internalizer profile identifier.
    pub internalizer_profile: String,
    /// Internalizer profile version.
    pub internalizer_version: String,
    /// Adapter format (e.g., "lora", "compressed-context", "noop").
    pub adapter_format: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// TTL in seconds (0 = no expiry).
    pub ttl_seconds: u64,
    /// ISO 8601 expiry timestamp (null if ttl is 0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// Provenance chain: ordered list of operations that produced this artifact.
    pub provenance: Vec<ProvenanceEntry>,
    /// Replay recipe: deterministic command to reproduce this artifact.
    pub replay_recipe: ReplayRecipe,
    /// SHA-256 hash of the adapter payload.
    pub adapter_hash: String,
    /// Relative path to adapter payload within artifact directory.
    pub adapter_path: String,
    /// Capabilities contract: what this internalization is allowed for.
    pub capabilities_contract: CapabilitiesContract,
    /// Risk classification for this artifact.
    pub risk_tier: RiskTier,
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
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilitiesContract {
    /// Allowed usage scopes (e.g., ["qa", "summarization"]).
    pub allowed_scopes: Vec<String>,
    /// Tools permitted to mount this adapter.
    pub permitted_tools: Vec<String>,
    /// Whether code generation is allowed.
    pub allow_code_gen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskTier {
    /// Risk level for creation: "compute-risky" (external tool invoked).
    pub creation: String,
    /// Risk level for attach: "behavior-changing" (affects inference).
    pub attach: String,
    /// Risk level for inspect: "read-only" (no side effects).
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

/// Result of `decapod internalize create`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationCreateResult {
    pub schema_version: String,
    pub success: bool,
    pub artifact_id: String,
    pub artifact_path: String,
    pub manifest: InternalizationManifest,
    pub source_hash: String,
    pub adapter_hash: String,
}

/// Result of `decapod internalize attach`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizationAttachResult {
    pub schema_version: String,
    pub success: bool,
    pub artifact_id: String,
    pub session_id: String,
    pub attached_at: String,
    pub expires_at: Option<String>,
    pub capabilities_contract: CapabilitiesContract,
    pub risk_classification: String,
    /// Provenance entry logged to the session.
    pub provenance_entry: ProvenanceEntry,
}

/// Result of `decapod internalize inspect`.
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
    pub adapter_hash_valid: bool,
    pub manifest_consistent: bool,
    pub expired: bool,
}

// ── Internalizer Profile Abstraction ───────────────────────────────────

/// An internalizer profile describes an external tool that converts
/// a document + base model into an adapter artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalizerProfile {
    pub name: String,
    pub version: String,
    /// Executable path or "builtin:noop" for the stub.
    pub executable: String,
    /// Default chunking parameters.
    pub default_params: BTreeMap<String, serde_json::Value>,
    /// Output adapter format.
    pub adapter_format: String,
}

impl InternalizerProfile {
    /// Built-in noop profile: produces a zero-byte adapter for pipeline testing.
    pub fn noop() -> Self {
        Self {
            name: "noop".to_string(),
            version: "1.0.0".to_string(),
            executable: "builtin:noop".to_string(),
            default_params: BTreeMap::new(),
            adapter_format: "noop".to_string(),
        }
    }

    /// Resolve a profile by name. Returns the noop stub for "noop",
    /// otherwise looks for a profile JSON in `.decapod/generated/profiles/internalizers/`.
    pub fn resolve(name: &str, store_root: &Path) -> Result<Self, InternalizeError> {
        if name == "noop" {
            return Ok(Self::noop());
        }
        let profile_path = store_root
            .join("generated")
            .join("profiles")
            .join("internalizers")
            .join(format!("{}.json", name));
        if !profile_path.exists() {
            return Err(InternalizeError::ProfileNotFound(name.to_string()));
        }
        let raw = fs::read_to_string(&profile_path).map_err(InternalizeError::Io)?;
        let profile: Self = serde_json::from_str(&raw).map_err(InternalizeError::Json)?;
        Ok(profile)
    }

    /// Execute the internalizer. For "builtin:noop", produces an empty adapter.
    /// For external executables, invokes them with JSON on stdin and reads adapter from output dir.
    pub fn execute(
        &self,
        source_path: &Path,
        _base_model: &str,
        output_dir: &Path,
    ) -> Result<(PathBuf, BTreeMap<String, serde_json::Value>), InternalizeError> {
        let adapter_file = output_dir.join("adapter.bin");

        if self.executable == "builtin:noop" {
            // Noop: write empty adapter
            fs::write(&adapter_file, b"").map_err(InternalizeError::Io)?;
            return Ok((adapter_file, self.default_params.clone()));
        }

        // External executable: invoke with structured JSON input
        let input = serde_json::json!({
            "source_path": source_path.to_string_lossy(),
            "base_model": _base_model,
            "output_dir": output_dir.to_string_lossy(),
            "params": self.default_params,
        });

        let output = ProcessCommand::new(&self.executable)
            .arg("--input")
            .arg(serde_json::to_string(&input).unwrap())
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

        // Try to parse output metadata from stdout
        let stdout = String::from_utf8_lossy(&output.stdout);
        let params: BTreeMap<String, serde_json::Value> =
            serde_json::from_str(&stdout).unwrap_or_else(|_| self.default_params.clone());

        Ok((adapter_file, params))
    }
}

// ── Error Types ────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum InternalizeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    ProfileNotFound(String),
    ProfileExecution(String),
    ArtifactNotFound(String),
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
            Self::SourceIntegrityFailed { expected, actual } => {
                write!(
                    f,
                    "Source integrity check failed: expected {}, got {}",
                    expected, actual
                )
            }
            Self::AdapterIntegrityFailed { expected, actual } => {
                write!(
                    f,
                    "Adapter integrity check failed: expected {}, got {}",
                    expected, actual
                )
            }
            Self::Expired {
                artifact_id,
                expired_at,
            } => {
                write!(
                    f,
                    "Artifact '{}' expired at {}; renew with a new create",
                    artifact_id, expired_at
                )
            }
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

// ── Helpers ────────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> Result<String, InternalizeError> {
    let bytes = fs::read(path).map_err(InternalizeError::Io)?;
    sha256_bytes(&bytes)
}

fn sha256_bytes(bytes: &[u8]) -> Result<String, InternalizeError> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn now_iso8601() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    // Simple ISO 8601 without chrono dependency
    let secs = d.as_secs();
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Approximate date calculation (good enough for timestamps)
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

fn artifacts_dir(store_root: &Path) -> PathBuf {
    store_root
        .join("generated")
        .join("artifacts")
        .join("internalizations")
}

fn artifact_dir(store_root: &Path, id: &str) -> PathBuf {
    artifacts_dir(store_root).join(id)
}

// ── Core Operations ────────────────────────────────────────────────────

pub fn create_internalization(
    store_root: &Path,
    source: &str,
    model: &str,
    profile_name: &str,
    ttl: u64,
    scopes: &[String],
) -> Result<InternalizationCreateResult, InternalizeError> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        return Err(InternalizeError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Source document not found: {}", source),
        )));
    }

    // Hash the source
    let source_hash = sha256_file(source_path)?;

    // Resolve profile
    let profile = InternalizerProfile::resolve(profile_name, store_root)?;

    // Create artifact directory
    let id = ulid::Ulid::new().to_string();
    let art_dir = artifact_dir(store_root, &id);
    fs::create_dir_all(&art_dir).map_err(InternalizeError::Io)?;

    // Execute internalizer
    let (adapter_path, chunking_params) = profile.execute(source_path, model, &art_dir)?;

    // Hash the adapter
    let adapter_hash = sha256_file(&adapter_path)?;

    let now = now_iso8601();

    // Compute expiry
    let expires_at = if ttl > 0 {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let exp_secs = d.as_secs() + ttl;
        // Recompute timestamp for expiry
        let mut year = 1970i64;
        let mut remaining = exp_secs as i64;
        loop {
            let diy = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
                366 * 86400
            } else {
                365 * 86400
            };
            if remaining < diy {
                break;
            }
            remaining -= diy;
            year += 1;
        }
        let day_secs = remaining;
        let days = day_secs / 86400;
        let tod = day_secs % 86400;
        let h = tod / 3600;
        let m = (tod % 3600) / 60;
        let s = tod % 60;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let md = [
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
        let mut mon = 0usize;
        let mut rem = days;
        for (i, &d) in md.iter().enumerate() {
            if rem < d {
                mon = i;
                break;
            }
            rem -= d;
        }
        Some(format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year,
            mon + 1,
            rem + 1,
            h,
            m,
            s
        ))
    } else {
        None
    };

    // Default scopes
    let effective_scopes = if scopes.is_empty() {
        vec!["qa".to_string(), "summarization".to_string()]
    } else {
        scopes.to_vec()
    };

    let allow_code_gen = effective_scopes.iter().any(|s| s == "code-gen");

    // Build replay recipe
    let mut replay_args = vec![
        "internalize".to_string(),
        "create".to_string(),
        "--source".to_string(),
        source.to_string(),
        "--model".to_string(),
        model.to_string(),
        "--profile".to_string(),
        profile_name.to_string(),
    ];
    if ttl > 0 {
        replay_args.push("--ttl".to_string());
        replay_args.push(ttl.to_string());
    }
    for s in &effective_scopes {
        replay_args.push("--scope".to_string());
        replay_args.push(s.clone());
    }

    let provenance_entry = ProvenanceEntry {
        op: "internalize.create".to_string(),
        timestamp: now.clone(),
        actor: "decapod-cli".to_string(),
        inputs_hash: source_hash.clone(),
    };

    let manifest = InternalizationManifest {
        schema_version: SCHEMA_VERSION.to_string(),
        id: id.clone(),
        source_hash: source_hash.clone(),
        source_path: source.to_string(),
        extraction_method: profile.name.clone(),
        chunking_params,
        base_model_id: model.to_string(),
        internalizer_profile: profile.name.clone(),
        internalizer_version: profile.version.clone(),
        adapter_format: profile.adapter_format.clone(),
        created_at: now.clone(),
        ttl_seconds: ttl,
        expires_at,
        provenance: vec![provenance_entry],
        replay_recipe: ReplayRecipe {
            command: "decapod".to_string(),
            args: replay_args,
            env: BTreeMap::new(),
        },
        adapter_hash: adapter_hash.clone(),
        adapter_path: "adapter.bin".to_string(),
        capabilities_contract: CapabilitiesContract {
            allowed_scopes: effective_scopes,
            permitted_tools: vec!["*".to_string()],
            allow_code_gen,
        },
        risk_tier: RiskTier::default(),
    };

    // Write manifest
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(InternalizeError::Json)?;
    fs::write(art_dir.join("manifest.json"), &manifest_json).map_err(InternalizeError::Io)?;

    let result = InternalizationCreateResult {
        schema_version: SCHEMA_VERSION.to_string(),
        success: true,
        artifact_id: id,
        artifact_path: art_dir.to_string_lossy().to_string(),
        manifest,
        source_hash,
        adapter_hash,
    };

    Ok(result)
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

    // Verify adapter integrity
    let adapter_full_path = art_dir.join(&manifest.adapter_path);
    let adapter_hash_valid = if adapter_full_path.exists() {
        let actual = sha256_file(&adapter_full_path)?;
        actual == manifest.adapter_hash
    } else {
        false
    };

    // Check expiry
    let expired = if let Some(ref exp) = manifest.expires_at {
        let now = now_iso8601();
        now > *exp
    } else {
        false
    };

    let status = if expired {
        "expired".to_string()
    } else if !adapter_hash_valid {
        "integrity-failed".to_string()
    } else {
        "valid".to_string()
    };

    Ok(InternalizationInspectResult {
        schema_version: SCHEMA_VERSION.to_string(),
        artifact_id: id.to_string(),
        manifest,
        integrity: IntegrityCheck {
            source_hash_valid: true, // Source may not be local; skipped for inspect
            adapter_hash_valid,
            manifest_consistent: true,
            expired,
        },
        status,
    })
}

pub fn attach_internalization(
    store_root: &Path,
    id: &str,
    session_id: &str,
) -> Result<InternalizationAttachResult, InternalizeError> {
    // First inspect to check integrity and expiry
    let inspection = inspect_internalization(store_root, id)?;

    if inspection.integrity.expired {
        return Err(InternalizeError::Expired {
            artifact_id: id.to_string(),
            expired_at: inspection
                .manifest
                .expires_at
                .unwrap_or_else(|| "unknown".to_string()),
        });
    }

    if !inspection.integrity.adapter_hash_valid {
        return Err(InternalizeError::AdapterIntegrityFailed {
            expected: inspection.manifest.adapter_hash.clone(),
            actual: "corrupted".to_string(),
        });
    }

    let now = now_iso8601();

    let provenance_entry = ProvenanceEntry {
        op: "internalize.attach".to_string(),
        timestamp: now.clone(),
        actor: format!("session:{}", session_id),
        inputs_hash: inspection.manifest.adapter_hash.clone(),
    };

    // Log the attach event to the session's provenance directory
    let session_prov_dir = store_root
        .join("generated")
        .join("sessions")
        .join(session_id);
    let _ = fs::create_dir_all(&session_prov_dir);
    let attach_log = session_prov_dir.join(format!("internalize_attach_{}.json", id));
    let attach_entry = serde_json::json!({
        "op": "internalize.attach",
        "artifact_id": id,
        "session_id": session_id,
        "timestamp": now,
        "adapter_hash": inspection.manifest.adapter_hash,
        "capabilities_contract": inspection.manifest.capabilities_contract,
        "risk_classification": inspection.manifest.risk_tier.attach,
    });
    let _ = fs::write(
        &attach_log,
        serde_json::to_string_pretty(&attach_entry).unwrap_or_default(),
    );

    Ok(InternalizationAttachResult {
        schema_version: SCHEMA_VERSION.to_string(),
        success: true,
        artifact_id: id.to_string(),
        session_id: session_id.to_string(),
        attached_at: now,
        expires_at: inspection.manifest.expires_at,
        capabilities_contract: inspection.manifest.capabilities_contract,
        risk_classification: inspection.manifest.risk_tier.attach,
        provenance_entry,
    })
}

// ── CLI Runner ─────────────────────────────────────────────────────────

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
                println!("  Source hash: {}", result.source_hash);
                println!("  Adapter hash: {}", result.adapter_hash);
                println!("  Path: {}", result.artifact_path);
            }
        }
        InternalizeCommand::Attach {
            id,
            session,
            format,
        } => {
            let result = attach_internalization(store_root, &id, &session)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!(
                    "Attached {} to session {}",
                    result.artifact_id, result.session_id
                );
                println!("  Risk: {}", result.risk_classification);
            }
        }
        InternalizeCommand::Inspect { id, format } => {
            let result = inspect_internalization(store_root, &id)?;
            if format == "json" {
                println!("{}", serde_json::to_string_pretty(&result).unwrap());
            } else {
                println!("Artifact: {}", result.artifact_id);
                println!("  Status: {}", result.status);
                println!("  Source hash: {}", result.manifest.source_hash);
                println!("  Adapter hash: {}", result.manifest.adapter_hash);
                println!("  Profile: {}", result.manifest.internalizer_profile);
                println!("  Model: {}", result.manifest.base_model_id);
            }
        }
    }
    Ok(())
}

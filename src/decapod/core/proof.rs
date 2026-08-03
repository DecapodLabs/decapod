use crate::ProofCommandCli;
use crate::core::events;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::store::Store;
use crate::error::DecapodError;
use crate::plugins::health;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Canonical live authority for project proof commands (repository-relative).
pub const PROOF_CONFIG_AUTHORITY: &str = ".decapod/config.toml";
/// Obsolete dual registry path (repository-relative). Present only for fail-closed
/// dual-authority detection and a transitional read when config has no commands.
pub const LEGACY_PROOF_REGISTRY: &str = ".decapod/proofs.toml";

/// A proof definition from project proof configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProofDef {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
}

/// Project-level proof declarations stored in `.decapod/config.toml`.
///
/// This is the sole live authority for project proof commands. Guided init
/// writes here; runtime resolution, validate, and provenance must agree.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ProjectProofConfig {
    #[serde(default)]
    pub commands: Vec<ProofDef>,
}

/// Resolved proof commands bound to an explicit repository-local authority.
#[derive(Debug, Clone)]
pub struct ResolvedProofConfig {
    pub config: ProofConfig,
    /// Repository-relative path of the selected authority
    /// (`.decapod/config.toml` or transitional `.decapod/proofs.toml`).
    pub authority: String,
}

/// Result of running a single proof
#[derive(Debug, Clone, Serialize)]
pub struct ProofResult {
    pub name: String,
    pub command: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub passed: bool,
    pub output: String,
    pub required: bool,
}

/// Event logged for each proof run
#[derive(Debug, Clone, Serialize)]
pub struct ProofEvent {
    pub ts: String,
    pub event_id: String,
    pub run_id: String,
    pub proof_name: String,
    pub command: String,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub passed: bool,
    pub store: String,
    pub root: String,
    pub actor: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_conditions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_requirements: Option<Vec<String>>,
}

/// Summary of a proof run
#[derive(Debug, Clone, Serialize)]
pub struct ProofRunSummary {
    pub run_id: String,
    pub ts: String,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub all_passed: bool,
    pub results: Vec<ProofResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_conditions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof_requirements: Option<Vec<String>>,
}

/// Result of running a single proof
fn run_single_proof(
    proof_def: &ProofDef,
    working_dir: &Path,
    store_root: &Path,
) -> Result<ProofResult, DecapodError> {
    let start_time = Instant::now();

    let args: Vec<&str> = proof_def.args.iter().map(|s| s.as_str()).collect();
    let output = external_action::execute(
        store_root,
        ExternalCapability::ProofExec,
        &format!("proof.{}", proof_def.name),
        &proof_def.command,
        &args,
        working_dir,
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    let duration_ms = start_time.elapsed().as_millis();
    let passed = exit_code == 0;

    // Truncate very long output
    let output_truncated: String = stdout.chars().take(1000).collect();

    Ok(ProofResult {
        name: proof_def.name.clone(),
        command: proof_def.command.clone(),
        exit_code,
        duration_ms: duration_ms.try_into().unwrap(),
        passed,
        output: format!("{output_truncated}\n{stderr}"),
        required: proof_def.required,
    })
}

/// Resolve the project root from a path that may be:
/// - the project root (contains `.decapod/`)
/// - the store root (`<project>/.decapod/data`)
/// - the `.decapod` directory itself
///
/// Resolution stays under the project governance root and does not load proof
/// config from outside the repository.
pub fn resolve_project_root(path: &Path) -> Result<PathBuf, DecapodError> {
    let path = if path.exists() {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    // Store root: .../.decapod/data
    if path
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name == "data")
        && let Some(decapod_dir) = path.parent()
        && decapod_dir
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name == ".decapod")
        && let Some(project_root) = decapod_dir.parent()
    {
        return Ok(project_root.to_path_buf());
    }

    // .decapod directory itself
    if path
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|name| name == ".decapod")
        && let Some(project_root) = path.parent()
    {
        return Ok(project_root.to_path_buf());
    }

    // Project root containing .decapod/
    if path.join(".decapod").is_dir() || path.join(".decapod").is_file() {
        return Ok(path);
    }

    // Parent is .decapod (e.g. a file under .decapod/)
    if let Some(parent) = path.parent()
        && parent
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|name| name == ".decapod")
        && let Some(project_root) = parent.parent()
    {
        return Ok(project_root.to_path_buf());
    }

    Err(DecapodError::ValidationError(format!(
        "PROOF_PROJECT_ROOT_UNRESOLVED: cannot resolve project root from '{}' for proof config (expected project root, .decapod/, or .decapod/data)",
        path.display()
    )))
}

fn read_legacy_proof_registry(legacy_path: &Path) -> Result<Option<ProofConfig>, DecapodError> {
    if !legacy_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(legacy_path).map_err(DecapodError::IoError)?;
    let config: ProofConfig = toml::from_str(&content).map_err(|e| {
        DecapodError::ValidationError(format!("invalid {}: {e}", LEGACY_PROOF_REGISTRY))
    })?;
    if config.proof.is_empty() {
        return Ok(None);
    }
    for proof in &config.proof {
        if proof.name.trim().is_empty() || proof.command.trim().is_empty() {
            return Err(DecapodError::ValidationError(format!(
                "invalid {}: proof entries must have non-empty name and command",
                LEGACY_PROOF_REGISTRY
            )));
        }
    }
    Ok(Some(config))
}

fn read_config_toml_proof_commands(
    config_path: &Path,
) -> Result<Option<Vec<ProofDef>>, DecapodError> {
    if !config_path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(config_path).map_err(DecapodError::IoError)?;
    let project: crate::cli::DecapodProjectConfig = toml::from_str(&content)
        .map_err(|e| DecapodError::ValidationError(format!("Invalid project config: {e}")))?;
    if project.proof.commands.is_empty() {
        return Ok(None);
    }
    for proof in &project.proof.commands {
        if proof.name.trim().is_empty() || proof.command.trim().is_empty() {
            return Err(DecapodError::ValidationError(format!(
                "invalid {}: proof.commands entries must have non-empty name and command",
                PROOF_CONFIG_AUTHORITY
            )));
        }
    }
    Ok(Some(project.proof.commands))
}

/// Resolve project proof commands from a single repository-local authority.
///
/// Accepts project root, store root (`.decapod/data`), or the `.decapod` directory.
///
/// Authority rules:
/// 1. Non-empty `[proof].commands` in `.decapod/config.toml` is the live authority.
/// 2. If both `config.toml` and a non-empty `.decapod/proofs.toml` are present → fail closed.
/// 3. If only the legacy registry has commands → transitional read with explicit provenance
///    (migrate into `config.toml` and remove the legacy file).
/// 4. Neither present → empty command set (not an error).
pub fn resolve_proof_config(path: &Path) -> Result<ResolvedProofConfig, DecapodError> {
    let project_root = resolve_project_root(path)?;
    let config_path = project_root.join(".decapod").join("config.toml");
    let legacy_path = project_root.join(".decapod").join("proofs.toml");

    let config_commands = read_config_toml_proof_commands(&config_path)?;
    let legacy_config = read_legacy_proof_registry(&legacy_path)?;

    match (config_commands, legacy_config) {
        (Some(_), Some(_)) => Err(DecapodError::ValidationError(format!(
            "PROOF_DUAL_AUTHORITY: both {PROOF_CONFIG_AUTHORITY} and {LEGACY_PROOF_REGISTRY} declare project proof commands. \
Keep a single authority: move commands into {PROOF_CONFIG_AUTHORITY} [proof].commands and remove {LEGACY_PROOF_REGISTRY}."
        ))),
        (Some(commands), None) => Ok(ResolvedProofConfig {
            config: ProofConfig { proof: commands },
            authority: PROOF_CONFIG_AUTHORITY.to_string(),
        }),
        (None, Some(legacy)) => Ok(ResolvedProofConfig {
            config: legacy,
            authority: LEGACY_PROOF_REGISTRY.to_string(),
        }),
        (None, None) => Ok(ResolvedProofConfig {
            config: ProofConfig::default(),
            authority: PROOF_CONFIG_AUTHORITY.to_string(),
        }),
    }
}

/// Load proof config (commands only). Prefer [`resolve_proof_config`] when provenance is needed.
///
/// Accepts either the project root or the store root (`.decapod/data`).
pub fn load_proof_config(path: &Path) -> Result<ProofConfig, DecapodError> {
    Ok(resolve_proof_config(path)?.config)
}

/// Run all configured proofs
pub fn run_proofs(
    store: &Store,
    path: &Path,
    actor: &str,
) -> Result<ProofRunSummary, DecapodError> {
    let project_root = resolve_project_root(path)?;
    let resolved = resolve_proof_config(path)?;
    let run_id = crate::core::ulid::new_ulid();
    let ts = format!(
        "{}Z",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    // Initialize health database and sync proof claims
    health::initialize_health_db(&store.root)?;
    sync_proof_claims_to_health(store, &resolved)?;

    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for proof_def in &resolved.config.proof {
        // Execute against the project root, not the store root.
        let result = run_single_proof(proof_def, &project_root, &store.root)?;

        let event = ProofEvent {
            ts: ts.clone(),
            event_id: crate::core::ulid::new_ulid(),
            run_id: run_id.clone(),
            proof_name: proof_def.name.clone(),
            command: format!("{} {}", proof_def.command, proof_def.args.join(" ")),
            exit_code: result.exit_code,
            duration_ms: result.duration_ms,
            passed: result.passed,
            store: format!("{:?}", store.kind),
            root: store.root.to_string_lossy().to_string(),
            actor: actor.to_string(),
            stop_conditions: None,
            proof_requirements: None,
        };

        append_proof_event(store, &event)?;

        // Also record to health database for claim tracking
        let health_result = if result.passed { "pass" } else { "fail" };
        let _ = health::record_proof(
            store,
            &format!("proof.{}", proof_def.name),
            &format!("{} {}", proof_def.command, proof_def.args.join(" ")),
            health_result,
            86400, // 24 hour SLA for proofs
        );

        if result.passed {
            passed += 1;
        } else if result.required {
            failed += 1;
        }

        results.push(result);
    }

    Ok(ProofRunSummary {
        run_id,
        ts,
        total: results.len(),
        passed,
        failed,
        skipped: 0,
        all_passed: failed == 0,
        results,
        stop_conditions: None,
        proof_requirements: None,
    })
}

/// Sync proof definitions to health claims with accurate authority provenance.
fn sync_proof_claims_to_health(
    store: &Store,
    resolved: &ResolvedProofConfig,
) -> Result<(), DecapodError> {
    for proof_def in &resolved.config.proof {
        let claim_id = format!("proof.{}", proof_def.name);
        let subject = proof_def.name.clone();
        let kind = if proof_def.required {
            "REQUIRED"
        } else {
            "OPTIONAL"
        };
        let provenance = resolved.authority.clone();

        // Try to add claim - ignore duplicate errors
        let _ = health::add_claim(store, &claim_id, &subject, kind, &provenance);
    }
    Ok(())
}

/// Append proof event to store
fn append_proof_event(store: &Store, event: &ProofEvent) -> Result<(), DecapodError> {
    events::append(
        &store.root,
        events::VERIFICATION,
        &serde_json::to_value(event).map_err(|e| DecapodError::ValidationError(e.to_string()))?,
    )?;
    Ok(())
}

/// Runtime proof command set (source-agnostic after resolution).
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProofConfig {
    #[serde(default)]
    pub proof: Vec<ProofDef>,
}

/// Run proof CLI command
pub fn execute_proof_cli(cli: &ProofCommandCli, store_root: &Path) -> Result<(), DecapodError> {
    match &cli.command {
        crate::ProofSubCommand::Run => {
            let result = run_proofs(
                &Store {
                    kind: super::store::StoreKind::Repo,
                    root: store_root.to_path_buf(),
                },
                store_root,
                "cli",
            )?;
            if result.failed == 0 {
                println!("✅ All required proofs passed for Epoch 1!");
            } else {
                for proof_result in &result.results {
                    if !proof_result.passed {
                        eprintln!(
                            "❌ Proof '{}' failed with exit code {}: {}",
                            proof_result.name, proof_result.exit_code, proof_result.output
                        );
                    }
                }
                return Err(DecapodError::NotImplemented(
                    "Proof validation failed".to_string(),
                ));
            }
            println!("✅ All required proofs passed for Epoch 1!");
            Ok(())
        }
        crate::ProofSubCommand::Test { name } => {
            println!("Running specific proof: {name}");
            // TODO: Implement single proof test
            Err(DecapodError::NotImplemented(
                "Individual proof testing not yet implemented".to_string(),
            ))
        }
        crate::ProofSubCommand::List => {
            let resolved = resolve_proof_config(store_root)?;
            println!("Available proofs (authority: {}):", resolved.authority);
            for (i, proof_def) in resolved.config.proof.iter().enumerate() {
                println!(
                    "  {}. {} - {} (required: {})",
                    i + 1,
                    proof_def.name,
                    proof_def.description,
                    proof_def.required
                );
                println!("     Command: {}", proof_def.command);
            }
            Ok(())
        }
    }
}

/// Get the schema for the proof subsystem
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "proof",
        "version": "0.2.0",
        "description": "Configurable proof registry - executable checks with audit trail",
        "config_file": PROOF_CONFIG_AUTHORITY,
        "legacy_config_file": LEGACY_PROOF_REGISTRY,
        "authority_policy": "single_source_fail_closed",
        "config_schema": {
            "proof": {
                "commands": [{
                    "name": "string (required)",
                    "command": "string (required)",
                    "args": ["string array (optional)"],
                    "description": "string (optional)",
                    "required": "bool (default: true)"
                }]
            }
        },
        "events": ["proof.run"],
        "storage": ["decapod.db:verification_events"]
    })
}
#[cfg(test)]
#[path = "../../../tests/unit/core/proof_tests.rs"]
mod tests;

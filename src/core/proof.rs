use crate::ProofCommandCli;
use crate::core::store::Store;
use crate::error::DecapodError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use ulid::Ulid;

/// A proof definition from proofs.toml
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
}

/// Result of running a single proof
fn run_single_proof(proof_def: &ProofDef, working_dir: &Path) -> Result<ProofResult, DecapodError> {
    let start_time = Instant::now();

    let mut cmd = Command::new(&proof_def.command);

    // Add arguments to command
    for arg in &proof_def.args {
        cmd.arg(arg);
    }

    let output = cmd.current_dir(working_dir).output().map_err(|e| {
        DecapodError::ValidationError(format!("Command failed: {} - {}", proof_def.name, e))
    })?;

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
        output: format!("{}\n{}", output_truncated, stderr),
        required: proof_def.required,
    })
}

/// Load proof config from .decapod/proofs.toml
/// Accepts either the project root (parent of .decapod) or the store root (.decapod/data)
pub fn load_proof_config(decapod_dir: &Path) -> Result<ProofConfig, DecapodError> {
    // Try the project root path first (.decapod/proofs.toml)
    let config_path = decapod_dir.join(".decapod").join("proofs.toml");

    if config_path.exists() {
        let content = fs::read_to_string(&config_path).map_err(DecapodError::IoError)?;
        let config: ProofConfig =
            toml::from_str(&content).map_err(|e| DecapodError::ValidationError(e.to_string()))?;
        return Ok(config);
    }

    // If that doesn't exist, try the parent directory (for when store_root is passed)
    if let Some(parent) = decapod_dir.parent() {
        let config_path = parent.join("proofs.toml");
        if config_path.exists() {
            let content = fs::read_to_string(&config_path).map_err(DecapodError::IoError)?;
            let config: ProofConfig = toml::from_str(&content)
                .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
            return Ok(config);
        }
    }

    // No config = no proofs configured (not an error)
    Ok(ProofConfig::default())
}

/// Run all configured proofs
pub fn run_proofs(
    store: &Store,
    decapod_dir: &Path,
    actor: &str,
) -> Result<ProofRunSummary, DecapodError> {
    let config = load_proof_config(decapod_dir)?;
    let run_id = Ulid::new().to_string();
    let ts = format!(
        "{}Z",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );

    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for proof_def in &config.proof {
        let result = run_single_proof(proof_def, decapod_dir)?;

        // Log event
        let event = ProofEvent {
            ts: ts.clone(),
            event_id: Ulid::new().to_string(),
            run_id: run_id.clone(),
            proof_name: proof_def.name.clone(),
            command: format!("{} {}", proof_def.command, proof_def.args.join(" ")),
            exit_code: result.exit_code,
            duration_ms: result.duration_ms,
            passed: result.passed,
            store: format!("{:?}", store.kind),
            root: store.root.to_string_lossy().to_string(),
            actor: actor.to_string(),
        };

        append_proof_event(store, &event)?;

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
    })
}

/// Append proof event to store
fn append_proof_event(store: &Store, event: &ProofEvent) -> Result<(), DecapodError> {
    use std::io::Write;

    let events_path = store.root.join("proof.events.jsonl");
    let event_json = serde_json::to_string(event).map_err(|e| {
        DecapodError::IoError(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    let event_line = format!("{}\n", event_json);

    // Append to file instead of overwriting
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_path)
        .map_err(DecapodError::IoError)?;

    file.write_all(event_line.as_bytes())
        .map_err(DecapodError::IoError)?;

    Ok(())
}

/// The proofs.toml config structure
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
            println!("Running specific proof: {}", name);
            // TODO: Implement single proof test
            Err(DecapodError::NotImplemented(
                "Individual proof testing not yet implemented".to_string(),
            ))
        }
        crate::ProofSubCommand::List => {
            let config = load_proof_config(store_root)?;
            println!("Available proofs:");
            for (i, proof_def) in config.proof.iter().enumerate() {
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
        "version": "0.1.0",
        "description": "Configurable proof registry - executable checks with audit trail",
        "config_file": ".decapod/proofs.toml",
        "config_schema": {
            "proof": [{
                "name": "string (required)",
                "command": "string (required)",
                "args": ["string array (optional)"],
                "description": "string (optional)",
                "required": "bool (default: true)"
            }]
        },
        "events": ["proof.run"],
        "storage": ["proof.events.jsonl"]
    })
}

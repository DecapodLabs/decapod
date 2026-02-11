use crate::core::error::DecapodError;
use crate::core::store::Store;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
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
    #[serde(default = "default_true")]
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
    let output_truncated = stdout.chars().take(1000).collect();

    Ok(ProofResult {
        name: proof_def.name.clone(),
        command: proof_def.command.clone(),
        exit_code,
        duration_ms,
        passed,
        output: format!("{}\n{}", output_truncated, stderr),
        required: proof_def.required,
    })
}

/// Load proof config from .decapod/proofs.toml
pub fn load_proof_config(decapod_dir: &Path) -> Result<ProofConfig, DecapodError> {
    let config_path = decapod_dir.join(".decapod").join("proofs.toml");

    if !config_path.exists() {
        // No config = no proofs configured (not an error)
        return Ok(ProofConfig::default());
    }

    let content = fs::read_to_string(&config_path).map_err(DecapodError::IoError)?;
    let config: ProofConfig =
        toml::from_str(&content).map_err(|e| DecapodError::ValidationError(e.to_string()))?;

    Ok(config)
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
    let events_path = store.root.join("proof.events.jsonl");
    let event_json = serde_json::to_string(event)?;
    let event_line = format!("{}\n", event_json);

    std::fs::write(&events_path, event_line).map_err(DecapodError::IoError)?;

    Ok(())
}

/// The proofs.toml config structure
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProofConfig {
    #[serde(default)]
    pub proof: Vec<ProofDef>,
}

/// Run proof CLI command
pub fn execute_proof_cli(
    cli: &super::cli::ProofCommandCli,
    store_root: &Path,
) -> Result<(), super::DecapodError> {
    match cli.command {
        super::cli::ProofCommand::Run => {
            let result = run_proofs(
                &Store {
                    kind: super::store::StoreKind::Repo,
                    root: store_root.to_path_buf(),
                },
                store_root,
                "epoch1",
            )?;
            println!(
                "Proof run completed: {} total, {} passed, {} failed",
                result.total, result.passed, result.failed
            );
            for proof_result in &result.results {
                if !proof_result.passed && proof_result.required {
                    return Err(super::error::DecapodError::ValidationError(format!(
                        "Required proof failed: {} - {}",
                        proof_result.name, proof_result.output
                    )));
                }
            }
            println!("✅ All required proofs passed for Epoch 1!");
        }
        super::cli::ProofCommand::Test { name } => {
            println!("Running specific proof: {}", name);
            // TODO: Implement single proof test
            return Err(super::error::DecapodError::NotImplemented(
                "Individual proof testing not yet implemented".to_string(),
            ));
        }
        super::cli::ProofCommand::List => {
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
                println!("     Command: {}", proof_def.command.join(" "));
            }
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

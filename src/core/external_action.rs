use crate::core::error;
use crate::core::store::{Store, StoreKind};
use crate::plugins::policy;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalCapability {
    VcsRead,
    VcsWrite,
    ProofExec,
    VerificationExec,
    SystemInspect,
}

impl ExternalCapability {
    fn as_str(self) -> &'static str {
        match self {
            ExternalCapability::VcsRead => "vcs_read",
            ExternalCapability::VcsWrite => "vcs_write",
            ExternalCapability::ProofExec => "proof_exec",
            ExternalCapability::VerificationExec => "verification_exec",
            ExternalCapability::SystemInspect => "system_inspect",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionRule {
    capability: String,
    allowed_bins: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionConfig {
    rules: Vec<ExternalActionRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionEvent {
    ts: String,
    event_id: String,
    capability: String,
    scope: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    status: String,
    exit_code: Option<i32>,
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

fn default_config() -> ExternalActionConfig {
    ExternalActionConfig {
        rules: vec![
            ExternalActionRule {
                capability: "vcs_read".to_string(),
                allowed_bins: vec!["git".to_string()],
            },
            ExternalActionRule {
                capability: "vcs_write".to_string(),
                allowed_bins: vec!["git".to_string()],
            },
            ExternalActionRule {
                capability: "proof_exec".to_string(),
                allowed_bins: vec![
                    "cargo".to_string(),
                    "decapod".to_string(),
                    "git".to_string(),
                    "bash".to_string(),
                    "sh".to_string(),
                ],
            },
            ExternalActionRule {
                capability: "verification_exec".to_string(),
                allowed_bins: vec!["decapod".to_string()],
            },
            ExternalActionRule {
                capability: "system_inspect".to_string(),
                allowed_bins: vec!["lsof".to_string()],
            },
        ],
    }
}

fn maybe_load_config(store_root: &Path) -> ExternalActionConfig {
    let repo_root = store_root.parent().and_then(|p| p.parent());
    let Some(repo_root) = repo_root else {
        return default_config();
    };
    let path = repo_root.join(".decapod").join("EXTERNAL_ACTIONS.json");
    if !path.exists() {
        return default_config();
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return default_config();
    };
    serde_json::from_str(&content).unwrap_or_else(|_| default_config())
}

fn allowed_for_capability(
    config: &ExternalActionConfig,
    capability: ExternalCapability,
) -> Vec<String> {
    config
        .rules
        .iter()
        .find(|r| r.capability == capability.as_str())
        .map(|r| r.allowed_bins.clone())
        .unwrap_or_default()
}

fn command_bin(command: &str) -> String {
    Path::new(command)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| command.to_string())
}

fn require_external_approval(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
) -> Result<(), error::DecapodError> {
    // Only write-like external capabilities require approval.
    if capability != ExternalCapability::VcsWrite {
        return Ok(());
    }
    // Scoped low-risk internal reconciliation path.
    if scope == "todo.handoff.reconcile" {
        return Ok(());
    }
    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.to_path_buf(),
    };
    let approval_scope = format!("external:{}:{}", capability.as_str(), scope);
    let risk = policy::RiskLevel::HIGH;
    let requires_human = policy::human_in_loop_required(&store, &approval_scope, risk, true);
    if !requires_human {
        return Ok(());
    }
    policy::initialize_policy_db(store_root)?;
    if !policy::check_approval(&store, &approval_scope, None, "global")? {
        return Err(error::DecapodError::ValidationError(format!(
            "External action denied: capability '{}' scope '{}' requires approval. Run: decapod govern policy approve --id '{}' --scope global",
            capability.as_str(),
            scope,
            approval_scope
        )));
    }
    Ok(())
}

fn external_events_path(store_root: &Path) -> PathBuf {
    store_root.join("external_actions.events.jsonl")
}

fn log_event(store_root: &Path, event: &ExternalActionEvent) -> Result<(), error::DecapodError> {
    let path = external_events_path(store_root);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(event).unwrap()).map_err(error::DecapodError::IoError)
}

pub fn execute(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
    command: &str,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, error::DecapodError> {
    let config = maybe_load_config(store_root);
    let allowed_bins = allowed_for_capability(&config, capability);
    let bin = command_bin(command);
    let is_allowed = allowed_bins.iter().any(|b| b == &bin)
        || (capability == ExternalCapability::VerificationExec && bin.starts_with("decapod"));
    if !is_allowed {
        return Err(error::DecapodError::ValidationError(format!(
            "External action denied: capability '{}' does not allow binary '{}'",
            capability.as_str(),
            bin
        )));
    }

    require_external_approval(store_root, capability, scope)?;

    let output = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(error::DecapodError::IoError)?;

    let event = ExternalActionEvent {
        ts: now_iso(),
        event_id: Ulid::new().to_string(),
        capability: capability.as_str().to_string(),
        scope: scope.to_string(),
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: cwd.to_string_lossy().to_string(),
        status: if output.status.success() {
            "success".to_string()
        } else {
            "error".to_string()
        },
        exit_code: output.status.code(),
    };
    let _ = log_event(store_root, &event);

    Ok(output)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "external_action",
        "version": "0.1.0",
        "description": "Capability-scoped broker for external commands with default-deny allowlists",
        "capabilities": [
            "vcs_read",
            "vcs_write",
            "proof_exec",
            "verification_exec",
            "system_inspect"
        ],
        "config": ".decapod/EXTERNAL_ACTIONS.json",
        "storage": ["external_actions.events.jsonl"]
    })
}

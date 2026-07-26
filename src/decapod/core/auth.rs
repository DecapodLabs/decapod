use crate::core::ansi::AnsiExt;
use crate::core::error::DecapodError;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub const CLOUD_ACCESS_TOKEN_ENV: &str = "DECAPOD_ACCESS_TOKEN";

fn machine_data_dir() -> Result<PathBuf, DecapodError> {
    if let Ok(data_home) = env::var("XDG_DATA_HOME") {
        let trimmed = data_home.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed).join("decapod"));
        }
    }

    let home = env::var("HOME").map_err(|_| {
        DecapodError::ValidationError(
            "HOME is required to locate ~/.local/share/decapod for cloud credentials.".to_string(),
        )
    })?;
    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join("decapod"))
}

fn machine_session_token_path() -> Result<PathBuf, DecapodError> {
    Ok(machine_data_dir()?.join("session_token.json"))
}

/// The non-repository credential sources are intentionally ordered from most
/// explicit to least explicit.  None of these values are copied into project
/// configuration or included in error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    Explicit,
    Environment,
    MachineFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudCredential {
    pub token: String,
    pub source: CredentialSource,
}

pub fn cloud_credential_path() -> Result<PathBuf, DecapodError> {
    machine_session_token_path()
}

/// Resolve a credential without touching process environment or the filesystem.
/// This small seam keeps precedence deterministic and makes the security
/// boundary directly testable without mutating global test state.
pub fn resolve_cloud_credential(
    explicit: Option<&str>,
    environment: Option<&str>,
    machine_file: Option<&str>,
) -> Result<CloudCredential, DecapodError> {
    for (source, value) in [
        (CredentialSource::Explicit, explicit),
        (CredentialSource::Environment, environment),
        (CredentialSource::MachineFile, machine_file),
    ] {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        if value.chars().any(char::is_control) || value.chars().any(char::is_whitespace) {
            return Err(DecapodError::SessionError(format!(
                "cloud credential from {source:?} contains whitespace or control characters"
            )));
        }
        return Ok(CloudCredential {
            token: value.to_string(),
            source,
        });
    }

    Err(DecapodError::SessionError(
        "no Propodus bearer configured; set DECAPOD_ACCESS_TOKEN or provision the machine credential file at ~/.local/share/decapod/session_token.json (cloud login is unavailable until the Propodus GitHub exchange contract is published)".to_string(),
    ))
}

fn read_machine_credential() -> Result<Option<String>, DecapodError> {
    let path = machine_session_token_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(DecapodError::IoError)?;
    let value: serde_json::Value = serde_json::from_str(&raw).map_err(|_| {
        DecapodError::SessionError("cloud credential file is not valid JSON".to_string())
    })?;
    value
        .get("token")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("session_token")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            DecapodError::SessionError("cloud credential file does not contain a token".to_string())
        })
        .map(Some)
}

pub fn load_cloud_credential(explicit: Option<&str>) -> Result<CloudCredential, DecapodError> {
    resolve_cloud_credential(
        explicit,
        env::var(CLOUD_ACCESS_TOKEN_ENV).ok().as_deref(),
        read_machine_credential()?.as_deref(),
    )
}

pub fn perform_cloud_auth(_target_dir: &Path) -> Result<(), DecapodError> {
    Err(DecapodError::NotImplemented(
        "Propodus-compatible GitHub login is not available in Decapod yet. Configure a Propodus-issued bearer JWT through DECAPOD_ACCESS_TOKEN or ~/.local/share/decapod/session_token.json; the Propodus service owns issuer, audience, GitHub-subject, refresh, and revocation checks.".to_string(),
    ))
}

pub fn is_token_valid(_target_dir: &Path) -> bool {
    load_cloud_credential(None).is_ok()
}

pub trait CloudAuthGate: Send + Sync {
    fn check_and_trigger(&self, root: &Path) -> Result<(), DecapodError>;
}

pub struct NoOpAuthGate;
impl CloudAuthGate for NoOpAuthGate {
    fn check_and_trigger(&self, _root: &Path) -> Result<(), DecapodError> {
        Ok(())
    }
}

pub struct InteractiveAuthGate;
impl CloudAuthGate for InteractiveAuthGate {
    fn check_and_trigger(&self, root: &Path) -> Result<(), DecapodError> {
        if !is_token_valid(root) {
            println!(
                "{} {}",
                "◢".bright_cyan().bold(),
                "Cloud authentication required".bright_white().bold()
            );
            perform_cloud_auth(root)?;
        }
        Ok(())
    }
}

pub fn get_cloud_auth_gate() -> Box<dyn CloudAuthGate> {
    use std::io::IsTerminal;
    // Trigger auth only if in a terminal and not in GITHUB_ACTIONS CI
    if std::io::stdin().is_terminal() && std::env::var("GITHUB_ACTIONS").is_err() {
        return Box::new(InteractiveAuthGate);
    }
    Box::new(NoOpAuthGate)
}

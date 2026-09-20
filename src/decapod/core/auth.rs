use crate::core::ansi::AnsiExt;
use crate::core::cloud_backend::CloudSession;
use crate::core::error::DecapodError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CLOUD_ACCESS_TOKEN_ENV: &str = "DECAPOD_ACCESS_TOKEN";
pub const TYPESAFE_API_KEY_ENV: &str = "TYPESAFE_API_KEY";
pub const CLOUD_SESSION_REFRESH_LEAD_SECS: i64 = 30 * 60;
const MACHINE_SECRETS_FILE: &str = "secrets.json";

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

fn machine_secrets_path() -> Result<PathBuf, DecapodError> {
    Ok(machine_data_dir()?.join(MACHINE_SECRETS_FILE))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct MachineSecretsRecord {
    #[serde(default)]
    typesafe_api_key: Option<String>,
}

fn normalize_secret(value: &str, name: &str) -> Result<Option<String>, DecapodError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(char::is_control) || trimmed.chars().any(char::is_whitespace) {
        return Err(DecapodError::SessionError(format!(
            "{name} contains whitespace or control characters"
        )));
    }
    Ok(Some(trimmed.to_string()))
}

/// Resolve the TypeSafe API key from the process environment first, then the
/// machine-local secret store. Neither source is repository state.
pub fn load_typesafe_api_key() -> Result<Option<String>, DecapodError> {
    let environment = env::var(TYPESAFE_API_KEY_ENV).ok();
    let machine_file = {
        let path = machine_secrets_path()?;
        if !path.exists() {
            None
        } else {
            let raw = fs::read_to_string(path).map_err(DecapodError::IoError)?;
            let record: MachineSecretsRecord = serde_json::from_str(&raw).map_err(|_| {
                DecapodError::SessionError("machine secret file is not valid JSON".to_string())
            })?;
            record.typesafe_api_key
        }
    };
    resolve_typesafe_api_key(environment.as_deref(), machine_file.as_deref())
}

/// Resolve provider credentials from already-loaded sources. This pure seam
/// keeps precedence and validation deterministic without mutating process
/// environment in tests.
pub fn resolve_typesafe_api_key(
    environment: Option<&str>,
    machine_file: Option<&str>,
) -> Result<Option<String>, DecapodError> {
    for value in [environment, machine_file].into_iter().flatten() {
        if let Some(value) = normalize_secret(value, TYPESAFE_API_KEY_ENV)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Persist a TypeSafe API key in the machine-local secret store.
///
/// This file is deliberately separate from `session_token.json`, whose schema
/// is reserved for the Propodus cloud session. The repository never receives
/// this value.
pub fn store_typesafe_api_key(value: &str) -> Result<(), DecapodError> {
    let value = normalize_secret(value, TYPESAFE_API_KEY_ENV)?.ok_or_else(|| {
        DecapodError::SessionError("typesafe API key must not be empty".to_string())
    })?;
    let path = machine_secrets_path()?;
    let parent = path.parent().ok_or_else(|| {
        DecapodError::SessionError("machine secret path has no parent directory".to_string())
    })?;
    fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(DecapodError::IoError)?;
    }

    let record = MachineSecretsRecord {
        typesafe_api_key: Some(value),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| {
        DecapodError::SessionError(format!("failed to serialize machine secrets: {error}"))
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp_path = parent.join(format!(
        ".{MACHINE_SECRETS_FILE}.tmp-{}-{nonce}",
        std::process::id()
    ));
    let write_result = (|| {
        fs::write(&temp_path, bytes).map_err(DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))
                .map_err(DecapodError::IoError)?;
        }
        fs::rename(&temp_path, &path).map_err(DecapodError::IoError)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

/// During Jev-enabled initialization, migrate an explicitly supplied
/// environment key into the machine-local secret store. Missing credentials
/// remain allowed because Jev is optional and will fail closed at observation
/// time.
pub fn persist_typesafe_api_key_from_environment() -> Result<bool, DecapodError> {
    let Some(value) = env::var(TYPESAFE_API_KEY_ENV)
        .ok()
        .and_then(|value| normalize_secret(&value, TYPESAFE_API_KEY_ENV).transpose())
        .transpose()?
    else {
        return Ok(false);
    };
    store_typesafe_api_key(&value)?;
    Ok(true)
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
    pub refresh_token: Option<String>,
    pub session_id: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MachineSessionRecord {
    #[serde(alias = "token", alias = "session_token")]
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingCloudOnboarding {
    pub api_url: String,
    pub repo_id: String,
    pub flow_id: String,
    pub url: String,
    pub expires_at: String,
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
            refresh_token: None,
            session_id: None,
            expires_at: None,
        });
    }

    Err(DecapodError::SessionError(
        "no cloud session is configured; complete setup with `decapod init --backend cloud`, then retry".to_string(),
    ))
}

fn read_machine_session_record() -> Result<Option<MachineSessionRecord>, DecapodError> {
    let path = machine_session_token_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(DecapodError::IoError)?;
    let record: MachineSessionRecord = serde_json::from_str(&raw).map_err(|_| {
        DecapodError::SessionError("cloud credential file is not valid JSON".to_string())
    })?;
    if record.access_token.trim().is_empty() {
        return Err(DecapodError::SessionError(
            "cloud credential file does not contain an access token".to_string(),
        ));
    }
    Ok(Some(record))
}

pub fn load_cloud_credential(explicit: Option<&str>) -> Result<CloudCredential, DecapodError> {
    let environment = env::var(CLOUD_ACCESS_TOKEN_ENV).ok();
    if explicit.is_some() || environment.is_some() {
        return resolve_cloud_credential(explicit, environment.as_deref(), None);
    }
    load_machine_session()?.ok_or_else(|| {
        DecapodError::SessionError(
            "no cloud session is configured; complete setup with `decapod init --backend cloud`, then retry".to_string(),
        )
    })
}

pub fn perform_cloud_auth(_target_dir: &Path) -> Result<(), DecapodError> {
    Err(DecapodError::SessionError(
        "cloud setup requires the configured project cloud endpoint; complete setup with `decapod init --backend cloud`, then retry".to_string(),
    ))
}

pub fn load_machine_session() -> Result<Option<CloudCredential>, DecapodError> {
    let Some(record) = read_machine_session_record()? else {
        return Ok(None);
    };
    Ok(Some(CloudCredential {
        token: record.access_token,
        source: CredentialSource::MachineFile,
        refresh_token: record.refresh_token,
        session_id: record.session_id,
        expires_at: record.expires_at,
    }))
}

pub fn store_machine_session(session: &CloudSession) -> Result<(), DecapodError> {
    session.validate()?;
    let path = machine_session_token_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }
    let record = MachineSessionRecord {
        access_token: session.access_token.clone(),
        refresh_token: session.refresh_token.clone(),
        session_id: session.session_id.clone(),
        expires_at: session.expires_at.clone(),
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| {
        DecapodError::SessionError(format!("failed to serialize cloud session: {error}"))
    })?;
    let parent = path.parent().ok_or_else(|| {
        DecapodError::SessionError("cloud session path has no parent directory".to_string())
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp_path = parent.join(format!(
        ".session_token.json.tmp-{}-{nonce}",
        std::process::id()
    ));
    let write_result = (|| {
        fs::write(&temp_path, bytes).map_err(DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o600))
                .map_err(DecapodError::IoError)?;
        }
        fs::rename(&temp_path, &path).map_err(DecapodError::IoError)
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result?;
    Ok(())
}

pub fn cloud_session_is_expired(credential: &CloudCredential) -> bool {
    let Some(expires_at) = credential.expires_at.as_deref() else {
        return false;
    };
    DateTime::parse_from_rfc3339(expires_at)
        .map(|value| value.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(true)
}

pub fn cloud_session_needs_refresh(credential: &CloudCredential) -> bool {
    let Some(expires_at) = credential.expires_at.as_deref() else {
        return false;
    };
    DateTime::parse_from_rfc3339(expires_at)
        .map(|value| {
            value.with_timezone(&Utc)
                <= Utc::now() + chrono::Duration::seconds(CLOUD_SESSION_REFRESH_LEAD_SECS)
        })
        .unwrap_or(true)
}

pub fn store_pending_cloud_onboarding(
    pending: &PendingCloudOnboarding,
) -> Result<(), DecapodError> {
    let path = machine_data_dir()?.join("onboarding.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }
    let bytes = serde_json::to_vec_pretty(pending).map_err(|error| {
        DecapodError::SessionError(format!(
            "failed to serialize cloud onboarding state: {error}"
        ))
    })?;
    fs::write(&path, bytes).map_err(DecapodError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
            .map_err(DecapodError::IoError)?;
    }
    Ok(())
}

pub fn load_pending_cloud_onboarding() -> Result<Option<PendingCloudOnboarding>, DecapodError> {
    let path = machine_data_dir()?.join("onboarding.json");
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(DecapodError::IoError)?;
    serde_json::from_str(&raw).map(Some).map_err(|_| {
        DecapodError::SessionError("cloud onboarding state is not valid JSON".to_string())
    })
}

pub fn clear_pending_cloud_onboarding() -> Result<(), DecapodError> {
    let path = machine_data_dir()?.join("onboarding.json");
    if path.exists() {
        fs::remove_file(path).map_err(DecapodError::IoError)?;
    }
    Ok(())
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

#[cfg(test)]
#[path = "../../../tests/unit/core/auth_tests.rs"]
mod tests;

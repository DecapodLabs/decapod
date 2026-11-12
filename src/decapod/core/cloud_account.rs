//! Account setup and server-issued project bindings; never billing policy.

use std::io::Read;
use std::path::{Path, PathBuf};

use dactyl_db::{AccountOperation, CloudConnector};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::core::auth::{CloudCredential, CloudSessionStore};
use crate::core::cloud_session::{backend, custody, lock, read, save};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus};
use crate::core::propodus::PropodusClientError;
use crate::core::repo_identity::RepositoryIdentity;

#[derive(Serialize, Deserialize)]
struct Registration {
    version: u16,
    origin: String,
    repository: String,
    request_id: String,
    project_name: String,
    project_id: Option<String>,
}

#[derive(Deserialize)]
struct AccountStatus {
    version: u16,
    account_id: String,
    status: String,
    backend_authorized: bool,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    setup_url: Option<String>,
}

#[derive(Debug)]
pub(crate) struct AuthorizedProject {
    project_id: String,
}

impl AuthorizedProject {
    /// Keep the existing application-owned RPC envelope. Its legacy repository
    /// field carries only the project reference returned by the account backend.
    pub fn bind(&self, identity: &RepositoryIdentity) -> RepositoryIdentity {
        let (owner, repository) = self
            .project_id
            .split_once('/')
            .expect("validated project ID");
        RepositoryIdentity {
            canonical_name: self.project_id.clone(),
            owner: owner.into(),
            repository: repository.into(),
            remote_url: identity.remote_url.clone(),
        }
    }
}

fn protocol() -> PropodusClientError {
    PropodusClientError::Decode("cloud account returned an invalid project binding".into())
}

fn project_id_valid(value: &str) -> bool {
    value.len() <= 512
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && value
            .split_once('/')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
}

fn uuid_valid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

fn request_id() -> Result<String, PropodusClientError> {
    let mut bytes = [0u8; 16];
    custody(custody(std::fs::File::open("/dev/urandom"))?.read_exact(&mut bytes))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

fn registration_path(directory: &Path, repository: &str) -> PathBuf {
    directory.join(format!(
        "project-{:x}.json",
        Sha256::digest(repository.as_bytes())
    ))
}

fn validate_registration(value: &Registration, origin: &str, repository: &str) -> bool {
    value.version == 1
        && value.origin == origin
        && value.repository == repository
        && uuid_valid(&value.request_id)
        && (1..=200).contains(&value.project_name.chars().count())
        && value.project_id.as_deref().is_none_or(project_id_valid)
}

pub(crate) fn prepare(
    origin: &str,
    identity: &RepositoryIdentity,
    project_name: &str,
    credential: &CloudCredential,
) -> Result<AuthorizedProject, PropodusClientError> {
    let store = custody(CloudSessionStore::for_service(origin))?;
    let connector = backend(CloudConnector::from_runtime(origin))?;
    let selected = std::env::var("DECAPOD_CLOUD_PROJECT_ID").ok();
    prepare_with_selected(
        custody(store.scope_directory())?,
        origin,
        &identity.canonical_name,
        project_name,
        &credential.token,
        &connector,
        selected.as_deref(),
    )
}

/// Read a cached routing reference, never an authorization decision. Every
/// actual data request still passes the server's live identity/seat checks.
pub(crate) fn project_id(
    origin: &str,
    identity: &RepositoryIdentity,
) -> Result<String, PropodusClientError> {
    let store = custody(CloudSessionStore::for_service(origin))?;
    let path = registration_path(custody(store.scope_directory())?, &identity.canonical_name);
    let value: Registration = read(&path)?.ok_or_else(protocol)?;
    if !validate_registration(&value, origin, &identity.canonical_name) {
        return Err(protocol());
    }
    value.project_id.ok_or_else(protocol)
}

fn setup_action(url: Option<&str>, project: Option<&str>) -> String {
    if let Some(url) = url
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        && !url.contains('#')
        && let Some(rest) = url.strip_prefix("https://")
    {
        let authority = rest.split(['/', '?']).next().unwrap_or_default();
        let public_query = match url.split_once('?') {
            None => true,
            Some((_, query)) => project.is_some_and(|project| {
                let encoded: String = project
                    .bytes()
                    .map(|b| match b {
                        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                            (b as char).to_string()
                        }
                        _ => format!("%{b:02X}"),
                    })
                    .collect();
                query == format!("project_id={encoded}")
            }),
        };
        if public_query && CloudConnector::validate_origin(&format!("https://{authority}")).is_ok()
        {
            return format!("complete account setup at {url}, then rerun init");
        }
    }
    "complete linking and workspace seat setup in the deployment's account portal, then rerun init"
        .into()
}

#[cfg(test)]
fn prepare_with(
    directory: &Path,
    origin: &str,
    repository: &str,
    project_name: &str,
    bearer: &str,
    connector: &CloudConnector,
) -> Result<AuthorizedProject, PropodusClientError> {
    prepare_with_selected(
        directory,
        origin,
        repository,
        project_name,
        bearer,
        connector,
        None,
    )
}

fn prepare_with_selected(
    directory: &Path,
    origin: &str,
    repository: &str,
    project_name: &str,
    bearer: &str,
    connector: &CloudConnector,
    selected: Option<&str>,
) -> Result<AuthorizedProject, PropodusClientError> {
    let _lock = lock(directory)?;
    let path = registration_path(directory, repository);
    let mut registration: Registration = match read(&path)? {
        Some(value) => value,
        None => {
            let project_name = project_name.trim();
            if !(1..=200).contains(&project_name.chars().count())
                || project_name.chars().any(char::is_control)
            {
                return Err(PropodusClientError::Validation(
                    "cloud project display name must contain 1–200 characters".into(),
                ));
            }
            let value = Registration {
                version: 1,
                origin: origin.into(),
                repository: repository.into(),
                request_id: request_id()?,
                project_name: project_name.into(),
                project_id: None,
            };
            // Persist before dispatch. An interrupted registration can safely
            // resume with the same backend-idempotent init UUID.
            save(&path, &value)?;
            value
        }
    };
    if !validate_registration(&registration, origin, repository) {
        return Err(protocol());
    }
    if let Some(selected) = selected {
        if !project_id_valid(selected)
            || registration
                .project_id
                .as_deref()
                .is_some_and(|previous| previous != selected)
        {
            return Err(protocol());
        }
        registration.project_id = Some(selected.to_owned());
        save(&path, &registration)?;
    }
    let (operation, payload) = match registration.project_id.as_deref() {
        Some(project_id) => (
            AccountOperation::Status,
            json!({"version":1,"project_id":project_id}),
        ),
        None => (
            AccountOperation::Register,
            json!({"version":1,"request_id":registration.request_id,"project_name":registration.project_name}),
        ),
    };
    // A managed bearer is sufficient. A sign-in provider token can be Google
    // and is never an inferred SCM credential for ordinary account setup.
    let value = backend(connector.account_request(operation, &payload, bearer, None))?;
    let response: AccountStatus = serde_json::from_value(value).map_err(|_| protocol())?;
    if response.version != 1 || !uuid_valid(&response.account_id) {
        return Err(protocol());
    }
    if let Some(project_id) = response.project_id.as_deref() {
        if !project_id_valid(project_id)
            || registration
                .project_id
                .as_deref()
                .is_some_and(|previous| previous != project_id)
        {
            return Err(protocol());
        }
        registration.project_id = Some(project_id.into());
        save(&path, &registration)?;
    }
    if response.backend_authorized {
        if response.status != "active" {
            return Err(protocol());
        }
        return Ok(AuthorizedProject {
            project_id: response.project_id.ok_or_else(protocol)?,
        });
    }
    let state = match response.status.as_str() {
        "code_host_link_required" => "code_host_link_required",
        "registration_required" => "registration_required",
        "registered" => "registered",
        "entitlement_required" => "entitlement_required",
        "admin_approval_required" => "admin_approval_required",
        _ => "account_setup_required",
    };
    Err(PropodusClientError::Authentication(
        CloudAuthDiagnostic::new(
            if state == "code_host_link_required" {
                CloudAuthStatus::UnauthorizedIdentity
            } else {
                CloudAuthStatus::OnboardingPending
            },
            format!("cloud account setup is pending ({state}); the sign-in session is retained"),
            setup_action(
                response.setup_url.as_deref(),
                response.project_id.as_deref(),
            ),
        ),
    ))
}

#[cfg(test)]
// Dedicated bounded socket fixtures; production HTTP remains in Dactyl.
#[allow(clippy::disallowed_types)]
#[path = "../../../tests/unit/core/cloud_account_tests.rs"]
mod tests;

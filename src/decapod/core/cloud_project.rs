//! Explicit account-project routing. Service responses establish identity and
//! eligibility; the private record is only a routing reference and retry journal.

use crate::core::cloud_client::transport_error;
use crate::core::cloud_session::SessionGuard;
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::repo_identity::RepositoryIdentity;
use dactyl_db::{AccountOperation, CloudConnector};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    project_id: String,
    name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    account_id: String,
    project: Project,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Creation {
    account_id: String,
    request_id: String,
    name: String,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    selection: Option<Selection>,
    directory_account: Option<String>,
    #[serde(default)]
    directory: Vec<Project>,
    creation: Option<Creation>,
}

#[derive(Deserialize)]
struct Response {
    version: u16,
    account_id: String,
    status: String,
    backend_authorized: bool,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    projects: Vec<Project>,
}

#[derive(Clone, Serialize)]
pub struct ProjectView {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend_authorized: Option<bool>,
}

#[derive(Serialize)]
pub struct ProjectOutput {
    pub version: u16,
    pub projects: Vec<ProjectView>,
    pub selected: bool,
}

pub enum Operation {
    List,
    Use(String),
    Create(String),
}

fn invalid() -> DecapodError {
    DecapodError::SessionError(
        "The cloud account returned an invalid project response; no routing reference was changed"
            .into(),
    )
}

fn attention(message: &str, action: &str) -> DecapodError {
    DecapodError::CloudAuth(CloudAuthDiagnostic::new(
        CloudAuthStatus::ActionRequired,
        message,
        action,
    ))
}

fn name_valid(value: &str) -> bool {
    !value.trim().is_empty() && value.chars().count() <= 200 && !value.chars().any(char::is_control)
}

fn uuid_valid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

fn reference_valid(value: &str) -> bool {
    value.len() <= 512
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
        && value
            .split_once('/')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('/'))
}

fn load(guard: &SessionGuard) -> Result<Record, DecapodError> {
    let Some(value) = guard.project_record()? else {
        return Ok(Record::default());
    };
    let record: Record = serde_json::from_value(value).map_err(|_| invalid())?;
    if record.directory.len() > 100
        || record
            .directory_account
            .as_deref()
            .is_some_and(|v| !uuid_valid(v))
        || record
            .directory
            .iter()
            .any(|p| !reference_valid(&p.project_id) || !name_valid(&p.name))
        || record.selection.as_ref().is_some_and(|s| {
            !uuid_valid(&s.account_id)
                || !reference_valid(&s.project.project_id)
                || !name_valid(&s.project.name)
        })
        || record.creation.as_ref().is_some_and(|p| {
            !uuid_valid(&p.account_id)
                || !uuid_valid(&p.request_id)
                || !name_valid(&p.name)
                || p.project_id
                    .as_deref()
                    .is_some_and(|id| !reference_valid(id))
        })
    {
        return Err(invalid());
    }
    Ok(record)
}

fn save(guard: &mut SessionGuard, record: &Record) -> Result<(), DecapodError> {
    guard.replace_project_record(serde_json::to_value(record).map_err(|_| invalid())?)
}

fn request(
    connector: &CloudConnector,
    bearer: &str,
    operation: AccountOperation,
    body: Value,
) -> Result<Response, DecapodError> {
    let value = connector
        .account_request(operation, &body, bearer, None)
        .map_err(transport_error)?;
    let mut result: Response = serde_json::from_value(value).map_err(|_| invalid())?;
    if result.version != 1
        || !uuid_valid(&result.account_id)
        || result.status.is_empty()
        || result.status.len() > 100
        || !result
            .status
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b == b'_')
        || result.projects.len() > 100
        || result.projects.iter().any(|p| {
            !reference_valid(&p.project_id)
                || p.name.chars().count() > 200
                || p.name.chars().any(char::is_control)
        })
        || result
            .project_id
            .as_deref()
            .is_some_and(|p| !reference_valid(p))
        || (result.backend_authorized && result.status != "active")
    {
        return Err(invalid());
    }
    for project in &mut result.projects {
        if project.name.trim().is_empty() {
            project.name = "Unnamed project".into();
        }
    }
    Ok(result)
}

fn identity(connector: &CloudConnector, bearer: &str) -> Result<Response, DecapodError> {
    let response = request(
        connector,
        bearer,
        AccountOperation::Status,
        json!({"version":1}),
    )?;
    if response.status == "code_host_link_required" {
        return Err(attention(
            "The managed account needs a verified code-host link.",
            "Complete account linking in the cloud portal, then retry.",
        ));
    }
    Ok(response)
}

fn status(
    connector: &CloudConnector,
    bearer: &str,
    account: &str,
    project: &str,
) -> Result<Response, DecapodError> {
    let response = request(
        connector,
        bearer,
        AccountOperation::Status,
        json!({"version":1,"project_id":project}),
    )?;
    if response.account_id != account {
        return Err(attention(
            "The saved project belongs to a different managed account.",
            "Run `decapod cloud projects` and explicitly select a project for the current account.",
        ));
    }
    if response.project_id.as_deref() != Some(project) {
        return Err(invalid());
    }
    Ok(response)
}

fn new_request_id() -> Result<String, DecapodError> {
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 15) | 0x40;
    bytes[8] = (bytes[8] & 63) | 0x80;
    let text: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &text[..8],
        &text[8..12],
        &text[12..16],
        &text[16..20],
        &text[20..]
    ))
}

fn pending_creation() -> DecapodError {
    attention(
        "A cloud project creation has an unresolved outcome. No new project or data operation was attempted.",
        "Rerun `decapod cloud project create` with the original name and managed account to reconcile its saved request.",
    )
}

pub(crate) fn execute(
    connector: &CloudConnector,
    guard: &mut SessionGuard,
    bearer: &str,
    operation: Operation,
) -> Result<ProjectOutput, DecapodError> {
    let mut record = load(guard)?;
    let current = identity(connector, bearer)?;
    if record
        .creation
        .as_ref()
        .is_some_and(|p| p.project_id.is_none() && p.account_id != current.account_id)
    {
        return Err(pending_creation());
    }
    match operation {
        Operation::List => {
            let mut projects = current.projects;
            projects.sort_by(|a, b| a.name.cmp(&b.name).then(a.project_id.cmp(&b.project_id)));
            record.directory_account = Some(current.account_id);
            record.directory = projects;
            save(guard, &record)?;
            Ok(ProjectOutput {
                version: 1,
                projects: record
                    .directory
                    .into_iter()
                    .map(|p| ProjectView {
                        name: p.name,
                        status: "available".into(),
                        backend_authorized: None,
                    })
                    .collect(),
                selected: false,
            })
        }
        Operation::Use(choice) => {
            if record
                .creation
                .as_ref()
                .is_some_and(|p| p.project_id.is_none())
            {
                return Err(pending_creation());
            }
            // Numeric choices refer to the exact previously printed directory,
            // never a fresh list whose ordering may have changed concurrently.
            let project = if let Ok(index) = choice.parse::<usize>() {
                if index == 0
                    || record.directory_account.as_deref() != Some(current.account_id.as_str())
                {
                    return Err(attention(
                        "No matching saved project directory is available.",
                        "Run `decapod cloud projects` before selecting its numbered entry.",
                    ));
                }
                record.directory.get(index - 1).cloned().ok_or_else(|| {
                    attention(
                        "The selected project is outside the saved directory.",
                        "Run `decapod cloud projects` and choose an available entry.",
                    )
                })?
            } else {
                let matches: Vec<_> = current
                    .projects
                    .into_iter()
                    .filter(|p| p.name == choice || p.project_id == choice)
                    .collect();
                if matches.len() != 1 {
                    return Err(attention(
                        "The project name is missing or ambiguous for this managed account.",
                        "Run `decapod cloud projects` and select its numbered entry; no project was guessed or created.",
                    ));
                }
                matches.into_iter().next().expect("one selected project")
            };
            let response = status(connector, bearer, &current.account_id, &project.project_id)?;
            if matches!(
                response.status.as_str(),
                "registration_required" | "admin_approval_required" | "code_host_link_required"
            ) {
                return Err(attention(
                    "The service did not confirm this account's project association.",
                    "Ask the project administrator to review membership and assignment, then retry.",
                ));
            }
            let view = ProjectView {
                name: project.name.clone(),
                status: response.status,
                backend_authorized: Some(response.backend_authorized),
            };
            record.selection = Some(Selection {
                account_id: current.account_id,
                project,
            });
            save(guard, &record)?;
            Ok(ProjectOutput {
                version: 1,
                projects: vec![view],
                selected: true,
            })
        }
        Operation::Create(name) => {
            if !name_valid(&name) {
                return Err(attention(
                    "A project needs a nonempty name of at most 200 characters.",
                    "Choose a project name without control characters.",
                ));
            }
            let mut creation = match &record.creation {
                Some(p) if p.name == name && p.account_id == current.account_id => p.clone(),
                Some(p) if p.project_id.is_none() => return Err(pending_creation()),
                _ => Creation {
                    account_id: current.account_id.clone(),
                    request_id: new_request_id()?,
                    name,
                    project_id: None,
                },
            };
            if let Some(project_id) = &creation.project_id {
                let checked = status(connector, bearer, &creation.account_id, project_id)?;
                record.selection = Some(Selection {
                    account_id: creation.account_id.clone(),
                    project: Project {
                        project_id: project_id.clone(),
                        name: creation.name.clone(),
                    },
                });
                save(guard, &record)?;
                return Ok(ProjectOutput {
                    version: 1,
                    projects: vec![ProjectView {
                        name: creation.name,
                        status: checked.status,
                        backend_authorized: Some(checked.backend_authorized),
                    }],
                    selected: true,
                });
            }
            record.creation = Some(creation.clone());
            save(guard, &record)?;
            // The existing server register contract is idempotent for this
            // request UUID. A timeout leaves it saved; no second UUID is made.
            let response = match request(
                connector,
                bearer,
                AccountOperation::Register,
                json!({"version":1,"request_id":creation.request_id,"project_name":creation.name}),
            ) {
                Err(DecapodError::CloudAuth(diagnostic))
                    if matches!(
                        diagnostic.status,
                        CloudAuthStatus::Offline | CloudAuthStatus::ProviderUnavailable
                    ) =>
                {
                    return Err(pending_creation());
                }
                result => result?,
            };
            if response.account_id != creation.account_id {
                return Err(invalid());
            }
            let project_id = response.project_id.ok_or_else(invalid)?;
            let checked = status(connector, bearer, &creation.account_id, &project_id)?;
            record.selection = Some(Selection {
                account_id: creation.account_id.clone(),
                project: Project {
                    project_id: project_id.clone(),
                    name: creation.name.clone(),
                },
            });
            creation.project_id = Some(project_id);
            record.creation = Some(creation.clone());
            save(guard, &record)?;
            Ok(ProjectOutput {
                version: 1,
                projects: vec![ProjectView {
                    name: creation.name,
                    status: checked.status,
                    backend_authorized: Some(checked.backend_authorized),
                }],
                selected: true,
            })
        }
    }
}

/// Return a selected routing reference after a fresh account response. Every
/// following data query/batch is still independently authorized by the service.
/// Absence preserves the existing canonical-repository native compatibility path.
pub(crate) fn route(
    connector: &CloudConnector,
    guard: &SessionGuard,
    bearer: &str,
    original: &RepositoryIdentity,
) -> Result<RepositoryIdentity, DecapodError> {
    let record = load(guard)?;
    if record
        .creation
        .as_ref()
        .is_some_and(|p| p.project_id.is_none())
    {
        return Err(pending_creation());
    }
    let Some(selection) = record.selection else {
        return Ok(original.clone());
    };
    let response = status(
        connector,
        bearer,
        &selection.account_id,
        &selection.project.project_id,
    )?;
    if !response.backend_authorized {
        return Err(attention(
            "The service has not authorized the selected cloud project.",
            "Resolve account access in the cloud portal or ask its administrator, then retry. Selection and browser completion alone do not grant data access.",
        ));
    }
    let (owner, repository) = selection
        .project
        .project_id
        .split_once('/')
        .ok_or_else(invalid)?;
    Ok(RepositoryIdentity {
        canonical_name: selection.project.project_id.clone(),
        owner: owner.into(),
        repository: repository.into(),
        remote_url: original.remote_url.clone(),
    })
}

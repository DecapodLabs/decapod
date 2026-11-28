//! Explicit account-project routing. Service responses establish identity and
//! eligibility; the private record is only a routing reference and retry journal.

use crate::core::cloud_client::{
    diagnostic_for_status_code, is_valid_repo_slug, pricing_enrollment_url,
    transport_error_for_repository,
};
use crate::core::cloud_session::SessionGuard;
use crate::core::error::{CloudAuthDiagnostic, CloudAuthErrorCode, CloudAuthStatus, DecapodError};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    organization_id: Option<String>,
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
    authorization_state: Option<String>,
    #[serde(default)]
    access_disposition: Option<String>,
    #[serde(default)]
    admin_contact_required: Option<bool>,
    #[serde(default)]
    pending_fulfillment: Option<bool>,
    #[serde(default)]
    cloud_org_id: Option<String>,
    #[serde(default)]
    admin: Option<bool>,
    #[serde(default)]
    repo_assigned: Option<bool>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    organization_id: Option<String>,
    #[serde(default)]
    organizations: Vec<Organization>,
    #[serde(default)]
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Organization {
    id: String,
    #[serde(default)]
    admin: Option<bool>,
}

#[derive(Clone, Serialize)]
pub struct ProjectView {
    pub name: String,
    pub project_id: String,
    pub organization_id: Option<String>,
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
    Use {
        choice: String,
        organization_id: Option<String>,
    },
    Create {
        name: String,
        organization_id: Option<String>,
    },
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

fn organization_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && !value.contains('/')
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn status_token_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
}

fn check_organization(
    requested: Option<&str>,
    authorized: Option<&str>,
) -> Result<(), DecapodError> {
    if requested.is_some() && requested != authorized {
        return Err(attention(
            "The service did not confirm the project belongs to the requested organization.",
            "Run `decapod cloud projects` and verify the organization and project you selected.",
        ));
    }
    Ok(())
}

fn view(project: Project, status: String, backend_authorized: Option<bool>) -> ProjectView {
    ProjectView {
        name: project.name,
        project_id: project.project_id,
        organization_id: project.organization_id,
        status,
        backend_authorized,
    }
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
        || record.directory.iter().any(|p| {
            !reference_valid(&p.project_id)
                || !name_valid(&p.name)
                || p.organization_id
                    .as_deref()
                    .is_some_and(|id| !organization_valid(id))
        })
        || record.selection.as_ref().is_some_and(|s| {
            !uuid_valid(&s.account_id)
                || !reference_valid(&s.project.project_id)
                || !name_valid(&s.project.name)
                || s.project
                    .organization_id
                    .as_deref()
                    .is_some_and(|id| !organization_valid(id))
        })
        || record.creation.as_ref().is_some_and(|p| {
            !uuid_valid(&p.account_id)
                || !uuid_valid(&p.request_id)
                || !name_valid(&p.name)
                || p.organization_id
                    .as_deref()
                    .is_some_and(|id| !organization_valid(id))
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
    repo_slug: Option<&str>,
) -> Result<Response, DecapodError> {
    let value = connector
        .account_request(operation, &body, bearer, None)
        .map_err(|error| transport_error_for_repository(error, repo_slug))?;
    let mut result: Response = serde_json::from_value(value).map_err(|_| invalid())?;
    if result.version != 1
        || !uuid_valid(&result.account_id)
        || !status_token_valid(&result.status)
        || result
            .authorization_state
            .as_deref()
            .is_some_and(|state| !status_token_valid(state))
        || result
            .cloud_org_id
            .as_deref()
            .is_some_and(|id| !organization_valid(id))
        || result.projects.len() > 100
        || result.organizations.len() > 100
        || result
            .organizations
            .iter()
            .any(|organization| !organization_valid(&organization.id))
        || result.projects.iter().any(|p| {
            !reference_valid(&p.project_id)
                || p.name.chars().count() > 200
                || p.name.chars().any(char::is_control)
                || p.organization_id
                    .as_deref()
                    .is_some_and(|id| !organization_valid(id))
        })
        || result
            .project_id
            .as_deref()
            .is_some_and(|p| !reference_valid(p))
        || result
            .organization_id
            .as_deref()
            .is_some_and(|id| !organization_valid(id))
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

fn identity(
    connector: &CloudConnector,
    bearer: &str,
    repo_slug: Option<&str>,
) -> Result<Response, DecapodError> {
    let mut response = request(
        connector,
        bearer,
        AccountOperation::Status,
        json!({"version":1}),
        repo_slug,
    )?;
    if response.status == "registration_required" {
        let registered = request(
            connector,
            bearer,
            AccountOperation::Register,
            json!({"version":1,"operation":"account"}),
            repo_slug,
        )?;
        if registered.account_id != response.account_id {
            return Err(invalid());
        }
        require_identity(&registered)?;
        let refreshed = request(
            connector,
            bearer,
            AccountOperation::Status,
            json!({"version":1}),
            repo_slug,
        )?;
        if refreshed.account_id != response.account_id {
            return Err(invalid());
        }
        response = refreshed;
    }
    require_identity(&response)?;
    Ok(response)
}

fn require_identity(response: &Response) -> Result<(), DecapodError> {
    match response.status.as_str() {
        "code_host_link_required" | "registration_required" | "provider_link_stepup_required" => {
            let diagnostic = diagnostic_for_status_code(response.status.as_str(), None)
                .expect("identity status diagnostic");
            Err(DecapodError::CloudAuth(diagnostic))
        }
        _ => Ok(()),
    }
}

fn status(
    connector: &CloudConnector,
    bearer: &str,
    account: &str,
    project: &str,
    repo_slug: Option<&str>,
) -> Result<Response, DecapodError> {
    let response = request(
        connector,
        bearer,
        AccountOperation::Status,
        json!({"version":1,"project_id":project}),
        repo_slug.or(Some(project)),
    )?;
    if response.account_id != account {
        return Err(attention(
            "The saved project belongs to a different managed account.",
            "Run `decapod cloud projects` and explicitly select a project for the current account.",
        ));
    }
    require_identity(&response)?;
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

fn is_safe_public_org_id(org_id: &str) -> bool {
    let is_physical_tenant_schema = org_id.len() == 34
        && org_id.starts_with("t_")
        && org_id[2..].bytes().all(|b| b.is_ascii_hexdigit());
    !is_physical_tenant_schema && !org_id.contains("://") && !org_id.contains('@')
}

fn enrich_diagnostic(
    mut diagnostic: CloudAuthDiagnostic,
    response: &Response,
) -> CloudAuthDiagnostic {
    match diagnostic.status {
        CloudAuthStatus::OrganizationAlreadyEnrolled => {
            if let Some(org_id) = response
                .cloud_org_id
                .as_deref()
                .or(response.organization_id.as_deref())
                .filter(|id| is_safe_public_org_id(id))
            {
                diagnostic.message = format!(
                    "This organization is already enrolled under immutable Cloud organization ID {org_id}."
                );
            }
        }
        CloudAuthStatus::AdminApprovalRequired
            if response.admin == Some(true) && response.repo_assigned == Some(false) =>
        {
            diagnostic.message = "Organization admin privileges do not grant repository data access without an assigned paid repository seat (admin=true, repo_assigned=false).".into();
        }
        _ => {}
    }
    if response.admin_contact_required == Some(true) {
        let had_enrollment_url = diagnostic.enrollment_url.take().is_some();
        if had_enrollment_url
            || diagnostic.next_action.contains("pricing")
            || diagnostic.next_action.contains("Cloud Admin ->")
            || diagnostic.next_action.contains("Purchase")
            || diagnostic
                .next_action
                .contains("Complete repository enrollment")
        {
            diagnostic.next_action =
                "Contact your organization administrator to request access to this repository."
                    .into();
        }
    }
    diagnostic
}

fn project_auth_diagnostic(response: &Response, repo_slug: Option<&str>) -> CloudAuthDiagnostic {
    if let Err(DecapodError::CloudAuth(diagnostic)) = require_identity(response) {
        return diagnostic;
    }
    let effective_repo = response
        .project_id
        .as_deref()
        .filter(|slug| is_valid_repo_slug(slug))
        .or(repo_slug);
    let pending_token = |value: &str| {
        matches!(
            value,
            "pending_fulfillment" | "payment_pending" | "order_pending"
        )
    };
    if response.pending_fulfillment == Some(true)
        || response
            .access_disposition
            .as_deref()
            .is_some_and(pending_token)
        || response
            .authorization_state
            .as_deref()
            .is_some_and(pending_token)
        || pending_token(response.status.as_str())
    {
        return enrich_diagnostic(
            diagnostic_for_status_code("pending_fulfillment", effective_repo)
                .expect("pending_fulfillment diagnostic"),
            response,
        );
    }
    if let Some(disp) = response.access_disposition.as_deref()
        && disp != "ready"
        && let Some(diagnostic) = diagnostic_for_status_code(disp, effective_repo)
    {
        return enrich_diagnostic(diagnostic, response);
    }
    if let Some(auth_state) = response.authorization_state.as_deref()
        && !matches!(auth_state, "active" | "authorized" | "ready")
        && let Some(diagnostic) = diagnostic_for_status_code(auth_state, effective_repo)
    {
        return enrich_diagnostic(diagnostic, response);
    }
    if let Some(diagnostic) = diagnostic_for_status_code(response.status.as_str(), effective_repo) {
        return enrich_diagnostic(diagnostic, response);
    }
    if response.admin == Some(true) && response.repo_assigned == Some(false) {
        return enrich_diagnostic(
            diagnostic_for_status_code("admin_approval_required", effective_repo)
                .expect("admin_approval_required diagnostic"),
            response,
        );
    }
    if response.admin_contact_required == Some(true) {
        return CloudAuthDiagnostic::new(
            CloudAuthStatus::AdminApprovalRequired,
            "The service has not granted seat access for the selected repository.",
            "Contact your organization administrator to request access to this repository.",
        );
    }
    let enroll_url = pricing_enrollment_url(effective_repo);
    CloudAuthDiagnostic::new(
        CloudAuthStatus::ActionRequired,
        "The service has not authorized the selected cloud project.",
        format!(
            "Enroll this repository at {enroll_url} or ask your organization administrator to assign a paid seat (no Admin setup wizard is required), then retry. Selection and browser completion alone do not grant data access."
        ),
    )
    .with_enrollment_url(enroll_url)
}

fn association_blocked(response: &Response) -> bool {
    let blocked_code = |code: &str| {
        matches!(
            code,
            "admin_approval_required"
                | "organization_already_enrolled"
                | "authorization_denied"
                | "repository_not_authorized"
                | "entitlement_denied"
                | "repository_not_approved"
                | "repository_capacity_exhausted"
                | "repository_reservation_exhausted"
                | "reservation_available_for_binding"
                | "seat_available_for_assignment"
                | "repository_access_denied"
        )
    };
    if blocked_code(response.status.as_str()) {
        return true;
    }
    if response
        .access_disposition
        .as_deref()
        .is_some_and(blocked_code)
    {
        return true;
    }
    if response
        .authorization_state
        .as_deref()
        .is_some_and(blocked_code)
    {
        return true;
    }
    response.admin == Some(true)
        && response.repo_assigned == Some(false)
        && !matches!(
            response.status.as_str(),
            "entitlement_required"
                | "organization_seat_required"
                | "seat_required"
                | "seat_unassigned"
                | "payment_required"
                | "pending_fulfillment"
                | "payment_pending"
        )
}

fn require_association(response: &Response, repo_slug: Option<&str>) -> Result<(), DecapodError> {
    require_identity(response)?;
    if association_blocked(response) {
        return Err(DecapodError::CloudAuth(project_auth_diagnostic(
            response, repo_slug,
        )));
    }
    Ok(())
}

fn pending_creation() -> DecapodError {
    attention(
        "A cloud project creation has an unresolved outcome. No new project or data operation was attempted.",
        "Rerun `decapod cloud project create` with the original name, organization and managed account to reconcile its saved request.",
    )
}

pub(crate) fn execute(
    connector: &CloudConnector,
    guard: &mut SessionGuard,
    bearer: &str,
    operation: Operation,
    repo_slug: Option<&str>,
) -> Result<ProjectOutput, DecapodError> {
    let organization = match &operation {
        Operation::List => None,
        Operation::Use {
            organization_id, ..
        }
        | Operation::Create {
            organization_id, ..
        } => organization_id.as_deref(),
    };
    if organization.is_some_and(|id| !organization_valid(id)) {
        return Err(attention(
            "The organization ID is invalid.",
            "Use the exact organization ID from `decapod cloud projects --format json`.",
        ));
    }
    let mut record = load(guard)?;
    let current = identity(connector, bearer, repo_slug)?;
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
            // The older directory wire format lists organization membership and
            // project references independently. Read each missing association
            // through the existing scoped status operation.
            if !current.organizations.is_empty() {
                for project in &mut projects {
                    if project.organization_id.is_none() {
                        project.organization_id = status(
                            connector,
                            bearer,
                            &current.account_id,
                            &project.project_id,
                            repo_slug,
                        )?
                        .organization_id;
                    }
                }
            }
            projects.sort_by(|a, b| a.name.cmp(&b.name).then(a.project_id.cmp(&b.project_id)));
            record.directory_account = Some(current.account_id);
            record.directory = projects;
            save(guard, &record)?;
            Ok(ProjectOutput {
                version: 1,
                projects: record
                    .directory
                    .into_iter()
                    .map(|p| view(p, "available".into(), None))
                    .collect(),
                selected: false,
            })
        }
        Operation::Use {
            choice,
            organization_id,
        } => {
            if record
                .creation
                .as_ref()
                .is_some_and(|p| p.project_id.is_none())
            {
                return Err(pending_creation());
            }
            // Numeric choices refer to the exact previously printed directory,
            // never a fresh list whose ordering may have changed concurrently.
            let mut project = if let Ok(index) = choice.parse::<usize>() {
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
                let candidates = current
                    .projects
                    .into_iter()
                    .filter(|p| p.name == choice || p.project_id == choice);
                let mut matches = Vec::new();
                for mut project in candidates {
                    if let Some(organization) = organization_id.as_deref() {
                        if project.organization_id.is_none() {
                            project.organization_id = status(
                                connector,
                                bearer,
                                &current.account_id,
                                &project.project_id,
                                repo_slug,
                            )?
                            .organization_id;
                        }
                        if project.organization_id.as_deref() != Some(organization) {
                            continue;
                        }
                    }
                    matches.push(project);
                }
                if matches.len() != 1 {
                    return Err(attention(
                        "The project name is missing or ambiguous for this managed account.",
                        "Run `decapod cloud projects` and select a numbered entry or specify `--org` with the exact project name.",
                    ));
                }
                matches.into_iter().next().expect("one selected project")
            };
            let response = status(
                connector,
                bearer,
                &current.account_id,
                &project.project_id,
                repo_slug,
            )?;
            require_association(&response, repo_slug)?;
            check_organization(
                organization_id
                    .as_deref()
                    .or(project.organization_id.as_deref()),
                response.organization_id.as_deref(),
            )?;
            if response.organization_id.is_some() {
                project.organization_id = response.organization_id;
            }
            let view = view(
                project.clone(),
                response.status,
                Some(response.backend_authorized),
            );
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
        Operation::Create {
            name,
            organization_id,
        } => {
            if !name_valid(&name) {
                return Err(attention(
                    "A project needs a nonempty name of at most 200 characters.",
                    "Choose a project name without control characters.",
                ));
            }
            let mut creation = match &record.creation {
                Some(p)
                    if p.name == name
                        && p.account_id == current.account_id
                        && p.organization_id == organization_id =>
                {
                    p.clone()
                }
                Some(p) if p.project_id.is_none() => return Err(pending_creation()),
                _ => Creation {
                    account_id: current.account_id.clone(),
                    request_id: new_request_id()?,
                    name,
                    organization_id,
                    project_id: None,
                },
            };
            let project_id = if let Some(project_id) = &creation.project_id {
                project_id.clone()
            } else {
                record.creation = Some(creation.clone());
                save(guard, &record)?;
                let mut body = json!({
                    "version":1,
                    "request_id":creation.request_id,
                    "project_name":creation.name,
                });
                if let Some(organization) = &creation.organization_id {
                    body["organization_id"] = json!(organization);
                }
                // A timeout leaves this request UUID and organization saved.
                let response = match request(
                    connector,
                    bearer,
                    AccountOperation::Register,
                    body,
                    repo_slug,
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
                require_identity(&response)?;
                if association_blocked(&response) {
                    return Err(DecapodError::CloudAuth(project_auth_diagnostic(
                        &response, repo_slug,
                    )));
                }
                response.project_id.ok_or_else(invalid)?
            };
            let checked = status(
                connector,
                bearer,
                &creation.account_id,
                &project_id,
                repo_slug,
            )?;
            require_association(&checked, repo_slug)?;
            check_organization(
                creation.organization_id.as_deref(),
                checked.organization_id.as_deref(),
            )?;
            let project = Project {
                project_id: project_id.clone(),
                name: creation.name.clone(),
                organization_id: checked.organization_id,
            };
            record.selection = Some(Selection {
                account_id: creation.account_id.clone(),
                project: project.clone(),
            });
            creation.project_id = Some(project_id);
            record.creation = Some(creation.clone());
            save(guard, &record)?;
            Ok(ProjectOutput {
                version: 1,
                projects: vec![view(
                    project,
                    checked.status,
                    Some(checked.backend_authorized),
                )],
                selected: true,
            })
        }
    }
}

fn route_from_status(
    response: &Response,
    original: &RepositoryIdentity,
) -> Result<RepositoryIdentity, DecapodError> {
    if !response.backend_authorized
        || response.status != "active"
        || response.pending_fulfillment == Some(true)
    {
        return Err(DecapodError::CloudAuth(project_auth_diagnostic(
            response,
            Some(&original.canonical_name),
        )));
    }
    let resolved_id = response
        .project_id
        .as_deref()
        .unwrap_or(&original.canonical_name);
    let (owner, repository) = resolved_id.split_once('/').ok_or_else(invalid)?;
    Ok(RepositoryIdentity {
        canonical_name: resolved_id.to_owned(),
        owner: owner.into(),
        repository: repository.into(),
        remote_url: original.remote_url.clone(),
    })
}

/// Resolve only the checkout's originating repository through live admission.
pub(crate) fn contributor_route(
    connector: &CloudConnector,
    bearer: &str,
    original: &RepositoryIdentity,
) -> Result<RepositoryIdentity, DecapodError> {
    // An already bound device session cannot perform human account activation.
    // The service still checks live repository access on its next operation.
    if crate::core::cloud_dpop::is_dpop_access_token(bearer) {
        return Ok(original.clone());
    }
    let response = request(
        connector,
        bearer,
        AccountOperation::Register,
        json!({"version":1,"operation":"contributor_access","project_id":original.canonical_name}),
        Some(&original.canonical_name),
    )
    .map_err(|error| match error {
        DecapodError::CloudAuth(ref diagnostic)
            if diagnostic.enrollment_url.is_some()
                || matches!(
                    diagnostic.status,
                    CloudAuthStatus::AdminApprovalRequired
                        | CloudAuthStatus::EntitlementDenied
                        | CloudAuthStatus::RepositoryDenied
                        | CloudAuthStatus::RegistrationRequired
                        | CloudAuthStatus::ProviderLinkStepupRequired
                        | CloudAuthStatus::UnauthorizedIdentity
                        | CloudAuthStatus::PaymentRequired
                        | CloudAuthStatus::PendingFulfillment
                        | CloudAuthStatus::OrganizationAlreadyEnrolled
                ) =>
        {
            contributor_denied()
        }
        _ => error,
    })?;
    if response.backend_authorized
        && response.status == "active"
        && response.pending_fulfillment != Some(true)
    {
        if response.project_id.is_none() {
            return Err(invalid());
        }
        return route_from_status(&response, original);
    }
    Err(contributor_denied())
}

fn contributor_denied() -> DecapodError {
    DecapodError::CloudAuth(
        CloudAuthDiagnostic::new(
            CloudAuthStatus::AdminApprovalRequired,
            "The service did not authorize this GitHub identity for the repository.",
            "Contact your organization administrator to request access to this repository.",
        )
        .with_error_code(CloudAuthErrorCode::AdminApprovalRequired),
    )
}

fn reconcile_originating_repository(
    connector: &CloudConnector,
    bearer: &str,
    original: &RepositoryIdentity,
) -> Result<RepositoryIdentity, DecapodError> {
    let mut response = request(
        connector,
        bearer,
        AccountOperation::Status,
        json!({"version":1,"project_id":original.canonical_name}),
        Some(&original.canonical_name),
    )?;
    if response.status == "registration_required" {
        let registered_account = response.account_id.clone();
        let _ = identity(connector, bearer, Some(&original.canonical_name))?;
        response = request(
            connector,
            bearer,
            AccountOperation::Status,
            json!({"version":1,"project_id":original.canonical_name}),
            Some(&original.canonical_name),
        )?;
        if response.account_id != registered_account {
            return Err(invalid());
        }
    }
    require_identity(&response)?;
    if response.backend_authorized
        && response.status == "active"
        && response.pending_fulfillment != Some(true)
    {
        return route_from_status(&response, original);
    }

    // If the service already resolved this originating repository into an
    // organization or a specific non-not-approved disposition (e.g.
    // entitlement_required, payment_required, pending_fulfillment,
    // reservation_available_for_binding, admin_approval_required,
    // repository_capacity_exhausted), surface that exact diagnostic directly.
    let unresolved_lookup = response.organization_id.is_none()
        && response.pending_fulfillment != Some(true)
        && matches!(
            response.access_disposition.as_deref(),
            None | Some("repository_not_approved")
        )
        && response.status == "admin_approval_required";
    if !unresolved_lookup {
        return Err(DecapodError::CloudAuth(project_auth_diagnostic(
            &response,
            Some(&original.canonical_name),
        )));
    }

    // Fall back to the account's project/organization directory only to match
    // an unambiguous project by slug/name or to distinguish an un-enrolled
    // admin/new-customer repo from a restricted collaborator repo.
    let directory = identity(connector, bearer, Some(&original.canonical_name))?;
    if directory.account_id != response.account_id {
        return Err(invalid());
    }
    let matches: Vec<&Project> = directory
        .projects
        .iter()
        .filter(|p| {
            p.project_id.eq_ignore_ascii_case(&original.canonical_name)
                || p.name.eq_ignore_ascii_case(&original.canonical_name)
        })
        .collect();
    if matches.len() > 1 {
        return Err(attention(
            "Multiple cloud projects match this repository for your account.",
            "Run `decapod cloud projects` and select the intended entry with `decapod cloud project use <choice> --org <organization_id>`.",
        ));
    }
    if let Some(matched) = matches.first() {
        let checked = status(
            connector,
            bearer,
            &directory.account_id,
            &matched.project_id,
            Some(&original.canonical_name),
        )?;
        return route_from_status(&checked, original);
    }

    if response.admin_contact_required != Some(true) {
        if directory.organizations.is_empty() {
            return Err(DecapodError::CloudAuth(
                diagnostic_for_status_code("signup_required", Some(&original.canonical_name))
                    .expect("signup_required diagnostic"),
            ));
        }
        if directory
            .organizations
            .iter()
            .any(|org| org.admin == Some(true))
        {
            let enroll_url = pricing_enrollment_url(Some(&original.canonical_name));
            return Err(DecapodError::CloudAuth(
                CloudAuthDiagnostic::new(
                    CloudAuthStatus::ActionRequired,
                    "This repository is not yet enrolled or enabled in your Decapod Cloud organization.",
                    format!(
                        "Bind an existing repository reservation in Cloud Admin -> Repositories, enroll it via `decapod cloud project enroll`, or subscribe at {enroll_url}, then retry."
                    ),
                )
                .with_enrollment_url(enroll_url)
                .with_error_code(CloudAuthErrorCode::RepositoryNotApproved),
            ));
        }
    }

    Err(DecapodError::CloudAuth(project_auth_diagnostic(
        &response,
        Some(&original.canonical_name),
    )))
}

/// Return a selected routing reference after a fresh account response, or
/// automatically reconcile an unambiguous originating repository when no
/// explicit selection overrides it. Every following data query/batch is still
/// independently authorized by the service.
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
        if crate::core::cloud_dpop::is_dpop_access_token(bearer) {
            return Ok(original.clone());
        }
        return reconcile_originating_repository(connector, bearer, original);
    };
    let response = status(
        connector,
        bearer,
        &selection.account_id,
        &selection.project.project_id,
        Some(&original.canonical_name),
    )?;
    check_organization(
        selection.project.organization_id.as_deref(),
        response.organization_id.as_deref(),
    )?;
    if !response.backend_authorized
        || response.status != "active"
        || response.pending_fulfillment == Some(true)
    {
        return Err(DecapodError::CloudAuth(project_auth_diagnostic(
            &response,
            Some(&original.canonical_name),
        )));
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

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_project_tests.rs"]
mod tests;

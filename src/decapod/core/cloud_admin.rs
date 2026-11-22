//! First-class Decapod Cloud onboarding, organization, member, invitation,
//! identity, and billing administration over Dactyl `0.18.0`.
//!
//! All control-plane calls route strictly through
//! `dactyl_db::AccountOperation::{Register, Status, Manage, Portal}` using the
//! repository-scoped cloud session custody (`cloud_client` + `cloud_session`).
//! Local SQLite stores (`decapod.db` / `todo.db`) are never opened or created.

use crate::core::error::DecapodError;
#[cfg(feature = "supabase-cloud")]
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus};
#[cfg(feature = "supabase-cloud")]
use crate::core::propodus::CloudMetadataWarning;
use crate::core::repo_identity::RepositoryIdentity;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
#[cfg(feature = "supabase-cloud")]
use serde_json::json;
use std::fmt;
use std::io::{self, Read};
use std::path::Path;
use std::str::FromStr;

const MAX_STDIN_SECRET_BYTES: usize = 4096;
#[cfg(feature = "supabase-cloud")]
const MAX_PAGINATION_PAGES: usize = 64;

pub const CLOUD_ORGS_SCHEMA_VERSION: &str = "decapod.cloud.orgs.v1";
pub const CLOUD_MEMBERS_SCHEMA_VERSION: &str = "decapod.cloud.members.v1";
pub const CLOUD_INVITES_SCHEMA_VERSION: &str = "decapod.cloud.invites.v1";
pub const CLOUD_IDENTITY_SCHEMA_VERSION: &str = "decapod.cloud.identity.v1";
pub const CLOUD_BILLING_SCHEMA_VERSION: &str = "decapod.cloud.billing.v1";

pub const ORTHOGONAL_SEAT_ACCESS_NOTE: &str = "Organization admin privileges and paid repository seats are orthogonal (#215): admin=true alone does not grant repository data access without an assigned paid repository seat.";
pub const INVITE_ZERO_PAID_SEATS_NOTE: &str = "Workspace membership grants zero paid repository seats. After acceptance, an organization administrator must explicitly run `decapod cloud members assign --project-id <id> --slot <n>`.";

/// One-time credential material read from stdin or RPC stdin payloads.
/// Never printed in `Debug` or `Display`, never serialized, and zeroized on drop.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(raw: impl Into<String>) -> Result<Self, DecapodError> {
        let owned = raw.into();
        let trimmed = owned.trim_end_matches(&['\r', '\n'][..]);
        if trimmed.trim().is_empty() || trimmed.len() > MAX_STDIN_SECRET_BYTES {
            let mut bytes = owned.into_bytes();
            crate::core::cloud_dpop::zeroize_bytes(&mut bytes);
            return Err(DecapodError::ValidationError(
                "Secret input must be non-empty and at most 4096 bytes.".into(),
            ));
        }
        let cleaned = trimmed.to_owned();
        let mut bytes = owned.into_bytes();
        crate::core::cloud_dpop::zeroize_bytes(&mut bytes);
        Ok(Self(cleaned))
    }

    #[cfg(any(feature = "supabase-cloud", test))]
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("\"[REDACTED]\"")
    }
}

impl fmt::Display for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Drop for RedactedSecret {
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.0).into_bytes();
        crate::core::cloud_dpop::zeroize_bytes(&mut bytes);
    }
}

impl<'de> Deserialize<'de> for RedactedSecret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum CloudIdentityProvider {
    Google,
    Github,
    Gitlab,
}

impl CloudIdentityProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Github => "github",
            Self::Gitlab => "gitlab",
        }
    }
}

impl fmt::Display for CloudIdentityProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CloudIdentityProvider {
    type Err = DecapodError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "google" => Ok(Self::Google),
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            _ => Err(DecapodError::ValidationError(
                "Invalid cloud identity provider; expected google, github, or gitlab.".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum CloudQuarantineDecision {
    Confirm,
    Revoke,
}

impl CloudQuarantineDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirm => "confirm",
            Self::Revoke => "revoke",
        }
    }
}

impl fmt::Display for CloudQuarantineDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CloudQuarantineDecision {
    type Err = DecapodError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "confirm" => Ok(Self::Confirm),
            "revoke" => Ok(Self::Revoke),
            _ => Err(DecapodError::ValidationError(
                "Invalid quarantine resolution decision; expected confirm or revoke.".into(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudMembersOp {
    List {
        organization_id: Option<String>,
        project_id: Option<String>,
        after: Option<u32>,
    },
    Assign {
        organization_id: Option<String>,
        project_id: String,
        slot: u32,
    },
    RevokeSeat {
        organization_id: Option<String>,
        project_id: Option<String>,
        slot: Option<u32>,
        member_account_id: Option<String>,
    },
    RevokeMember {
        organization_id: Option<String>,
        member_account_id: String,
    },
    TransferOwnership {
        organization_id: Option<String>,
        member_account_id: String,
        expected_revision: Option<u64>,
    },
    ProjectSettings {
        organization_id: Option<String>,
        project_id: String,
        project_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudInvitesOp {
    List {
        organization_id: Option<String>,
        after: Option<u32>,
    },
    Create {
        organization_id: String,
        recipient_account_id: String,
    },
    Cancel {
        organization_id: String,
        invitation_id: String,
    },
    Accept {
        organization_id: Option<String>,
        invitation_id: String,
        invitation_secret: RedactedSecret,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudIdentityOp {
    PrepareLink {
        provider: CloudIdentityProvider,
    },
    ConfirmLink {
        intent_id: String,
        stepup_secret: RedactedSecret,
    },
    ResolveQuarantine {
        provider: CloudIdentityProvider,
        decision: CloudQuarantineDecision,
    },
    RevokeLink {
        provider: CloudIdentityProvider,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudBillingOp {
    Portal { organization_id: Option<String> },
    Limits { project_id: Option<String> },
}

/// Read a one-time secret exclusively from stdin, zeroizing temporary buffers.
pub fn read_secret_from_stdin(label: &str) -> Result<RedactedSecret, DecapodError> {
    let mut raw = Vec::new();
    let read_res = io::stdin()
        .take((MAX_STDIN_SECRET_BYTES + 1) as u64)
        .read_to_end(&mut raw);
    if let Err(err) = read_res {
        crate::core::cloud_dpop::zeroize_bytes(&mut raw);
        return Err(DecapodError::IoError(err));
    }
    if raw.len() > MAX_STDIN_SECRET_BYTES {
        crate::core::cloud_dpop::zeroize_bytes(&mut raw);
        return Err(DecapodError::ValidationError(format!(
            "Secret read from stdin for {label} exceeds {MAX_STDIN_SECRET_BYTES} bytes."
        )));
    }
    let text = match String::from_utf8(raw) {
        Ok(text) => text,
        Err(err) => {
            let mut bytes = err.into_bytes();
            crate::core::cloud_dpop::zeroize_bytes(&mut bytes);
            return Err(DecapodError::ValidationError(format!(
                "Secret read from stdin for {label} must be valid UTF-8."
            )));
        }
    };
    if text.trim_end_matches(&['\r', '\n'][..]).is_empty() {
        let mut bytes = text.into_bytes();
        crate::core::cloud_dpop::zeroize_bytes(&mut bytes);
        return Err(DecapodError::ValidationError(format!(
            "Missing {label} on stdin; pipe the one-time secret via stdin with `--secret-stdin`."
        )));
    }
    RedactedSecret::new(text)
}

/// Mask any URL fragment (`#...`) before placing a URL into logs or errors.
pub fn redact_url_fragment(raw: &str) -> String {
    match raw.split_once('#') {
        Some((prefix, _fragment)) => format!("{prefix}#[REDACTED]"),
        None => raw.to_owned(),
    }
}

/// Recursively redact sensitive fields and URL fragments inside a JSON value
/// for logs, error details, or RPC trace receipts.
pub fn redact_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if lower.contains("secret")
                    || lower.contains("token")
                    || lower.contains("password")
                    || lower.contains("credential")
                    || lower.contains("authorization")
                    || lower == "apikey"
                    || lower == "api_key"
                {
                    out.insert(k.clone(), Value::String("[REDACTED]".to_owned()));
                } else if (lower.ends_with("_url") || lower == "url")
                    && let Some(raw_url) = v.as_str()
                {
                    out.insert(k.clone(), Value::String(redact_url_fragment(raw_url)));
                } else {
                    out.insert(k.clone(), redact_json_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_json_value).collect()),
        other => other.clone(),
    }
}

/// Enforce strict HTTPS origin and reject userinfo (`user@host`), fragments,
/// control chars, or credential query parameters on returned billing portal URLs.
pub fn validate_https_portal_url(raw: &str) -> Result<String, DecapodError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(DecapodError::ValidationError(
            "Cloud billing portal URL is empty or contains whitespace/control characters.".into(),
        ));
    }
    if !trimmed.starts_with("https://") {
        return Err(DecapodError::ValidationError(
            "Cloud billing portal URL must use an explicit `https://` scheme.".into(),
        ));
    }
    if trimmed.contains('#') {
        return Err(DecapodError::ValidationError(
            "Cloud billing portal URL must not include a URI fragment.".into(),
        ));
    }
    let after_scheme = &trimmed["https://".len()..];
    let authority = after_scheme.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return Err(DecapodError::ValidationError(
            "Cloud billing portal URL must have a valid host and must not embed credentials or userinfo."
                .into(),
        ));
    }
    if !valid_https_authority(authority) {
        return Err(DecapodError::ValidationError(
            "Cloud billing portal URL authority/host is malformed.".into(),
        ));
    }
    if let Some((_prefix, query)) = trimmed.split_once('?') {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let key = pair
                .split('=')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if is_credential_query_key(&key) {
                return Err(DecapodError::ValidationError(format!(
                    "Cloud billing portal URL must not embed credential query parameter `{key}`."
                )));
            }
        }
    }
    Ok(trimmed.to_owned())
}

#[cfg(any(feature = "supabase-cloud", test))]
fn validate_https_url_basic(raw: &str, label: &str) -> Result<String, DecapodError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || !trimmed.starts_with("https://")
        || trimmed.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(DecapodError::ValidationError(format!(
            "Cloud {label} must use a valid `https://` URL (got `{}`).",
            redact_url_fragment(trimmed)
        )));
    }
    let after_scheme = &trimmed["https://".len()..];
    let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty()
        || authority.contains('@')
        || authority.contains('\\')
        || !valid_https_authority(authority)
    {
        return Err(DecapodError::ValidationError(format!(
            "Cloud {label} contains an invalid host or embedded userinfo (got `{}`).",
            redact_url_fragment(trimmed)
        )));
    }
    Ok(trimmed.to_owned())
}

fn valid_https_authority(authority: &str) -> bool {
    let (host, port_opt) = if let Some(rest) = authority.strip_prefix('[') {
        let Some((ipv6_host, suffix)) = rest.split_once(']') else {
            return false;
        };
        if ipv6_host.is_empty() || !ipv6_host.chars().all(|c| c.is_ascii_hexdigit() || c == ':') {
            return false;
        }
        if suffix.is_empty() {
            (ipv6_host, None)
        } else if let Some(port) = suffix.strip_prefix(':') {
            (ipv6_host, Some(port))
        } else {
            return false;
        }
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        if host.contains(':') {
            return false;
        }
        (host, Some(port))
    } else {
        (authority, None)
    };

    if let Some(port) = port_opt
        && !matches!(port.parse::<u16>(), Ok(1..=u16::MAX))
    {
        return false;
    }

    if !authority.starts_with('[') {
        if host.is_empty() || host.len() > 253 || host.starts_with('.') || host.ends_with('.') {
            return false;
        }
        for label in host.split('.') {
            if label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            {
                return false;
            }
        }
    }
    true
}

fn is_credential_query_key(key: &str) -> bool {
    matches!(
        key,
        "token"
            | "access_token"
            | "refresh_token"
            | "id_token"
            | "secret"
            | "client_secret"
            | "invitation_secret"
            | "stepup_secret"
            | "session_secret"
            | "password"
            | "pwd"
            | "api_key"
            | "apikey"
            | "private_key"
            | "bearer"
            | "auth"
            | "signature"
    )
}

pub fn is_uuid_str(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(idx, &b)| match idx {
        8 | 13 | 18 | 23 => b == b'-',
        _ => b.is_ascii_hexdigit(),
    })
}

#[cfg(any(feature = "supabase-cloud", test))]
fn validate_uuid(raw: &str, field: &str) -> Result<String, DecapodError> {
    let trimmed = raw.trim();
    if !is_uuid_str(trimmed) {
        return Err(DecapodError::ValidationError(format!(
            "Parameter `{field}` must be a canonical UUID string."
        )));
    }
    Ok(trimmed.to_owned())
}

#[cfg(any(feature = "supabase-cloud", test))]
fn validate_bounded_id(raw: &str, field: &str) -> Result<String, DecapodError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 512
        || trimmed.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(DecapodError::ValidationError(format!(
            "Parameter `{field}` must be a non-empty bounded identifier without whitespace."
        )));
    }
    Ok(trimmed.to_owned())
}

#[cfg(any(feature = "supabase-cloud", test))]
fn validate_org_id_str(raw: &str) -> Result<String, DecapodError> {
    validate_bounded_id(raw, "organization_id")
}

#[cfg(any(feature = "supabase-cloud", test))]
fn validate_project_id_str(raw: &str) -> Result<String, DecapodError> {
    validate_bounded_id(raw, "project_id")
}

pub fn orgs(
    project_root: &Path,
    identity: &RepositoryIdentity,
    project_id: Option<&str>,
) -> Result<Value, DecapodError> {
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (project_root, identity, project_id);
        crate::core::cloud_client::selected_origin()?;
        unreachable!("cloud origin requires supabase-cloud feature");
    }
    #[cfg(feature = "supabase-cloud")]
    {
        let project_filter = project_id.map(validate_project_id_str).transpose()?;
        crate::core::cloud_client::with_admin_session(
            project_root,
            identity,
            |connector, _guard, bearer, mut warnings| {
                let status_body = match project_filter.as_deref() {
                    Some(pid) => json!({ "version": 1, "project_id": pid }),
                    None => json!({ "version": 1 }),
                };
                let status = account_call(
                    connector,
                    bearer,
                    dactyl_db::AccountOperation::Status,
                    &status_body,
                    Some(&identity.canonical_name),
                    false,
                    false,
                    &mut warnings,
                )?;
                let projects = status
                    .get("projects")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let org_entries = status
                    .get("organizations")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();

                let mut enriched = Vec::with_capacity(org_entries.len());
                for org in org_entries {
                    let org_id = org
                        .get("id")
                        .or_else(|| org.get("organization_id"))
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let cloud_org_id = org
                        .get("cloud_org_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let slug = org
                        .get("scm_owner_slug")
                        .or_else(|| org.get("slug"))
                        .or_else(|| org.get("display_name"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let display_name = org
                        .get("display_name")
                        .or_else(|| org.get("name"))
                        .or_else(|| org.get("scm_owner_slug"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let is_admin = org.get("admin").and_then(Value::as_bool).unwrap_or(false);

                    let matching_projects: Vec<&Value> = projects
                        .iter()
                        .filter(|p| {
                            let p_org = p.get("organization_id").and_then(Value::as_str);
                            let p_cloud_org = p.get("cloud_org_id").and_then(Value::as_str);
                            (!org_id.is_empty() && p_org == Some(org_id.as_str()))
                                || (cloud_org_id.is_some()
                                    && p_cloud_org == cloud_org_id.as_deref())
                        })
                        .collect();

                    let authorized_repo_count = matching_projects
                        .iter()
                        .filter(|p| {
                            p.get("backend_authorized").and_then(Value::as_bool) == Some(true)
                        })
                        .count() as u64;

                    let paid_repo_seat_assigned = org
                        .get("paid_repo_seat_assigned")
                        .and_then(Value::as_bool)
                        .or_else(|| {
                            project_filter.as_deref().and_then(|pid| {
                                matching_projects
                                    .iter()
                                    .find(|p| project_matches_id(p, pid))
                                    .and_then(|p| {
                                        p.get("backend_authorized").and_then(Value::as_bool)
                                    })
                            })
                        })
                        .unwrap_or(authorized_repo_count > 0);

                    let mut paid_repo_count = org
                        .get("paid_repo_count")
                        .or_else(|| org.get("active_paid_repositories"))
                        .and_then(Value::as_u64);
                    let mut billing_status: Option<Value> = None;

                    if paid_repo_count.is_none() && is_admin && !org_id.is_empty() {
                        match account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Manage,
                            &json!({
                                "version": 1,
                                "action": "status",
                                "organization_id": org_id,
                            }),
                            Some(&identity.canonical_name),
                            false,
                            false,
                            &mut warnings,
                        ) {
                            Ok(billing) => {
                                paid_repo_count = extract_paid_repo_count(&billing);
                                billing_status = Some(billing);
                            }
                            Err(err) if is_benign_billing_probe_error(&err) => {
                                paid_repo_count = Some(authorized_repo_count);
                            }
                            Err(err) => return Err(err),
                        }
                    }

                    let resolved_paid_repo_count = paid_repo_count.unwrap_or(authorized_repo_count);
                    let mut entry = json!({
                        "organization_id": org_id,
                        "cloud_org_id": cloud_org_id,
                        "scm_provider": org.get("scm_provider").cloned().unwrap_or(Value::Null),
                        "scm_owner_id": org.get("scm_owner_id").cloned().unwrap_or(Value::Null),
                        "slug": slug,
                        "display_name": display_name,
                        "admin": is_admin,
                        "paid_repo_seat_assigned": paid_repo_seat_assigned,
                        "paid_repo_count": resolved_paid_repo_count,
                        "authorized_project_count": authorized_repo_count,
                        "org_admin_without_paid_seat": is_admin && !paid_repo_seat_assigned,
                    });
                    if let Some(billing) = billing_status {
                        if let Some(cap) = billing
                            .get("allocated_repositories")
                            .or_else(|| billing.get("quantity"))
                            .cloned()
                        {
                            entry["allocated_repositories"] = cap;
                        }
                        if let Some(sub) = billing
                            .get("subscription_status")
                            .or_else(|| billing.get("status"))
                            .cloned()
                        {
                            entry["subscription_status"] = sub;
                        }
                        if let Some(humans) = billing.get("distinct_assigned_humans").cloned() {
                            entry["distinct_assigned_humans"] = humans;
                        }
                    }
                    enriched.push(entry);
                }

                let mut out = json!({
                    "schema_version": CLOUD_ORGS_SCHEMA_VERSION,
                    "repository": identity.canonical_name,
                    "access_model_note": ORTHOGONAL_SEAT_ACCESS_NOTE,
                    "organizations": enriched,
                });
                if let Some(pid) = project_filter {
                    out["project_id"] = json!(pid);
                }
                attach_warnings(&mut out, warnings);
                Ok(out)
            },
        )
    }
}

pub fn members(
    project_root: &Path,
    identity: &RepositoryIdentity,
    op: CloudMembersOp,
) -> Result<Value, DecapodError> {
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (project_root, identity, op);
        crate::core::cloud_client::selected_origin()?;
        unreachable!("cloud origin requires supabase-cloud feature");
    }
    #[cfg(feature = "supabase-cloud")]
    {
        crate::core::cloud_client::with_admin_session(
            project_root,
            identity,
            |connector, _guard, bearer, mut warnings| {
                let repo = Some(identity.canonical_name.as_str());
                let mut result = match op {
                    CloudMembersOp::List {
                        organization_id,
                        project_id,
                        after,
                    } => {
                        let pid = project_id
                            .map(|p| validate_project_id_str(&p))
                            .transpose()?;
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            pid.as_deref(),
                            repo,
                            &mut warnings,
                        )?;
                        let mut body = json!({
                            "version": 1,
                            "operation": "members",
                            "organization_id": org_id,
                        });
                        if let Some(cursor) = after {
                            body["after"] = json!(cursor);
                        }
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &body,
                            repo,
                            false,
                            false,
                            &mut warnings,
                        )?;
                        let access_details = if let Some(ref pid) = pid {
                            Some(account_call(
                                connector,
                                bearer,
                                dactyl_db::AccountOperation::Status,
                                &json!({
                                    "version": 1,
                                    "project_id": pid,
                                    "operation": "access_details",
                                }),
                                repo,
                                false,
                                false,
                                &mut warnings,
                            )?)
                        } else {
                            None
                        };
                        let mut out = json!({
                            "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                            "operation": "list",
                            "organization_id": org_id,
                            "members": response.get("members").cloned().unwrap_or_else(|| json!([])),
                            "distinct_human_seats": response.get("distinct_human_seats").cloned().unwrap_or(Value::Null),
                            "next": response.get("next").cloned().unwrap_or(Value::Null),
                            "seat_access_note": ORTHOGONAL_SEAT_ACCESS_NOTE,
                        });
                        if let Some(details) = access_details {
                            out["project_id"] = json!(pid);
                            out["access_details"] = details;
                        }
                        out
                    }
                    CloudMembersOp::Assign {
                        organization_id,
                        project_id,
                        slot,
                    } => {
                        let pid = validate_project_id_str(&project_id)?;
                        if slot == 0 {
                            return Err(DecapodError::ValidationError(
                                "Seat `--slot` must be >= 1.".into(),
                            ));
                        }
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            Some(&pid),
                            repo,
                            &mut warnings,
                        )?;
                        let member_account_id = find_member_account_id_for_slot(
                            connector,
                            bearer,
                            &org_id,
                            slot,
                            repo,
                            &mut warnings,
                        )?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Manage,
                            &json!({
                                "version": 1,
                                "action": "assign",
                                "organization_id": org_id,
                                "project_id": pid,
                                "account_id": member_account_id,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        json!({
                            "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                            "operation": "assign",
                            "organization_id": org_id,
                            "project_id": pid,
                            "slot": slot,
                            "account_id": member_account_id,
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("assigned")),
                            "response": response,
                        })
                    }
                    CloudMembersOp::RevokeSeat {
                        organization_id,
                        project_id,
                        slot,
                        member_account_id,
                    } => {
                        let pid = project_id
                            .map(|p| validate_project_id_str(&p))
                            .transpose()?;
                        let explicit_member = member_account_id
                            .map(|id| validate_uuid(&id, "member_account_id"))
                            .transpose()?;
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            pid.as_deref(),
                            repo,
                            &mut warnings,
                        )?;
                        if let Some(pid) = pid {
                            let target_account_id = match (explicit_member, slot) {
                                (Some(acct), _) => acct,
                                (None, Some(slot_num)) => {
                                    if slot_num == 0 {
                                        return Err(DecapodError::ValidationError(
                                            "Seat `--slot` must be >= 1.".into(),
                                        ));
                                    }
                                    find_member_account_id_for_slot(
                                        connector,
                                        bearer,
                                        &org_id,
                                        slot_num,
                                        repo,
                                        &mut warnings,
                                    )?
                                }
                                (None, None) => {
                                    return Err(DecapodError::ValidationError(
                                        "Provide `--slot <n>` or `--member-account-id <uuid>` to revoke a project seat."
                                            .into(),
                                    ));
                                }
                            };
                            let response = account_call(
                                connector,
                                bearer,
                                dactyl_db::AccountOperation::Manage,
                                &json!({
                                    "version": 1,
                                    "action": "revoke",
                                    "organization_id": org_id,
                                    "project_id": pid,
                                    "account_id": target_account_id,
                                }),
                                repo,
                                true,
                                true,
                                &mut warnings,
                            )?;
                            json!({
                                "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                                "operation": "revoke_seat",
                                "organization_id": org_id,
                                "project_id": pid,
                                "slot": slot,
                                "account_id": target_account_id,
                                "status": response.get("status").cloned().unwrap_or_else(|| json!("seat_revoked")),
                                "response": response,
                            })
                        } else {
                            let target_slot = slot.ok_or_else(|| {
                                DecapodError::ValidationError(
                                    "Organization-wide `revoke-seat` without `--project-id` requires `--slot <n>`."
                                        .into(),
                                )
                            })?;
                            if target_slot == 0 {
                                return Err(DecapodError::ValidationError(
                                    "Seat `--slot` must be >= 1.".into(),
                                ));
                            }
                            let mut body = json!({
                                "version": 1,
                                "operation": "revoke_seat",
                                "organization_id": org_id,
                                "slot": target_slot,
                            });
                            if let Some(ref acct) = explicit_member {
                                body["member_account_id"] = json!(acct);
                            }
                            let response = account_call(
                                connector,
                                bearer,
                                dactyl_db::AccountOperation::Register,
                                &body,
                                repo,
                                true,
                                true,
                                &mut warnings,
                            )?;
                            json!({
                                "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                                "operation": "revoke_seat",
                                "organization_id": org_id,
                                "slot": target_slot,
                                "member_account_id": explicit_member,
                                "status": response.get("status").cloned().unwrap_or_else(|| json!("seat_revoked")),
                                "seat_access_note": "Revoking a paid repository seat never revokes orthogonal organization_admins authority.",
                                "response": response,
                            })
                        }
                    }
                    CloudMembersOp::RevokeMember {
                        organization_id,
                        member_account_id,
                    } => {
                        let member_id = validate_uuid(&member_account_id, "member_account_id")?;
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            None,
                            repo,
                            &mut warnings,
                        )?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "revoke_membership",
                                "organization_id": org_id,
                                "member_account_id": member_id,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        json!({
                            "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                            "operation": "revoke_member",
                            "organization_id": org_id,
                            "member_account_id": member_id,
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("membership_revoked")),
                            "response": response,
                        })
                    }
                    CloudMembersOp::TransferOwnership {
                        organization_id,
                        member_account_id,
                        expected_revision,
                    } => {
                        let member_id = validate_uuid(&member_account_id, "member_account_id")?;
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            None,
                            repo,
                            &mut warnings,
                        )?;
                        let mut body = json!({
                            "version": 1,
                            "operation": "transfer_ownership",
                            "organization_id": org_id,
                            "member_account_id": member_id,
                        });
                        if let Some(rev) = expected_revision {
                            if rev == 0 {
                                return Err(DecapodError::ValidationError(
                                    "`--expected-revision` must be >= 1.".into(),
                                ));
                            }
                            body["expected_revision"] = json!(rev);
                        }
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &body,
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        json!({
                            "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                            "operation": "transfer_ownership",
                            "organization_id": org_id,
                            "member_account_id": member_id,
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("ownership_transferred")),
                            "response": response,
                        })
                    }
                    CloudMembersOp::ProjectSettings {
                        organization_id,
                        project_id,
                        project_name,
                    } => {
                        let pid = validate_project_id_str(&project_id)?;
                        let trimmed_name = project_name.trim();
                        if trimmed_name.is_empty()
                            || trimmed_name.chars().count() > 200
                            || trimmed_name.chars().any(char::is_control)
                        {
                            return Err(DecapodError::ValidationError(
                                "Parameter `project_name` must be 1..=200 non-control characters."
                                    .into(),
                            ));
                        }
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            Some(&pid),
                            repo,
                            &mut warnings,
                        )?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "project_settings",
                                "organization_id": org_id,
                                "project_id": pid,
                                "project_name": trimmed_name,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        json!({
                            "schema_version": CLOUD_MEMBERS_SCHEMA_VERSION,
                            "operation": "project_settings",
                            "organization_id": org_id,
                            "project_id": pid,
                            "project_name": trimmed_name,
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("project_updated")),
                            "response": response,
                        })
                    }
                };
                attach_warnings(&mut result, warnings);
                Ok(result)
            },
        )
    }
}

pub fn invites(
    project_root: &Path,
    identity: &RepositoryIdentity,
    op: CloudInvitesOp,
) -> Result<Value, DecapodError> {
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (project_root, identity, op);
        crate::core::cloud_client::selected_origin()?;
        unreachable!("cloud origin requires supabase-cloud feature");
    }
    #[cfg(feature = "supabase-cloud")]
    {
        crate::core::cloud_client::with_admin_session(
            project_root,
            identity,
            |connector, _guard, bearer, mut warnings| {
                let repo = Some(identity.canonical_name.as_str());
                let mut result = match op {
                    CloudInvitesOp::List {
                        organization_id,
                        after,
                    } => {
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            None,
                            repo,
                            &mut warnings,
                        )?;
                        let mut body = json!({
                            "version": 1,
                            "operation": "invitations",
                            "organization_id": org_id,
                        });
                        if let Some(cursor) = after {
                            body["after"] = json!(cursor);
                        }
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &body,
                            repo,
                            false,
                            false,
                            &mut warnings,
                        )?;
                        let sanitized_invitations =
                            redact_json_value(response.get("invitations").unwrap_or(&json!([])));
                        json!({
                            "schema_version": CLOUD_INVITES_SCHEMA_VERSION,
                            "operation": "list",
                            "organization_id": org_id,
                            "invitations": sanitized_invitations,
                            "next": response.get("next").cloned().unwrap_or(Value::Null),
                            "membership_grants_paid_seats": false,
                            "seat_assignment_note": INVITE_ZERO_PAID_SEATS_NOTE,
                        })
                    }
                    CloudInvitesOp::Create {
                        organization_id,
                        recipient_account_id,
                    } => {
                        let org_id = validate_org_id_str(&organization_id)?;
                        let recipient =
                            validate_uuid(&recipient_account_id, "recipient_account_id")?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "invite",
                                "organization_id": org_id,
                                "recipient_account_id": recipient,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        if let Some(raw_url) =
                            response.get("invitation_url").and_then(Value::as_str)
                        {
                            validate_https_url_basic(raw_url, "invitation_url")?;
                        }
                        let mut out = json!({
                            "schema_version": CLOUD_INVITES_SCHEMA_VERSION,
                            "operation": "create",
                            "organization_id": response.get("organization_id").cloned().unwrap_or_else(|| json!(org_id)),
                            "recipient_account_id": recipient,
                            "invitation_id": response.get("invitation_id").cloned().unwrap_or(Value::Null),
                            "slot": response.get("slot").cloned().unwrap_or(Value::Null),
                            "expires_at": response.get("expires_at").cloned().unwrap_or(Value::Null),
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("invitation_created")),
                            "membership_grants_paid_seats": false,
                            "seat_assignment_note": INVITE_ZERO_PAID_SEATS_NOTE,
                        });
                        if let Some(secret) = response.get("invitation_secret").cloned() {
                            out["invitation_secret"] = secret;
                        }
                        if let Some(url) = response.get("invitation_url").cloned() {
                            out["invitation_url"] = url;
                        }
                        out
                    }
                    CloudInvitesOp::Cancel {
                        organization_id,
                        invitation_id,
                    } => {
                        let org_id = validate_org_id_str(&organization_id)?;
                        let invite_id = validate_uuid(&invitation_id, "invitation_id")?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "revoke_invitation",
                                "organization_id": org_id,
                                "invitation_id": invite_id,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        json!({
                            "schema_version": CLOUD_INVITES_SCHEMA_VERSION,
                            "operation": "cancel",
                            "organization_id": org_id,
                            "invitation_id": invite_id,
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("invitation_revoked")),
                        })
                    }
                    CloudInvitesOp::Accept {
                        organization_id,
                        invitation_id,
                        invitation_secret,
                    } => {
                        let invite_id = validate_uuid(&invitation_id, "invitation_id")?;
                        let mut body = json!({
                            "version": 1,
                            "operation": "accept_invite",
                            "invitation_id": invite_id,
                            "invitation_secret": invitation_secret.expose(),
                        });
                        if let Some(ref org_id) = organization_id {
                            body["organization_id"] = json!(validate_org_id_str(org_id)?);
                        }
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &body,
                            repo,
                            true,
                            false,
                            &mut warnings,
                        )?;
                        let safe_response = redact_json_value(&response);
                        json!({
                            "schema_version": CLOUD_INVITES_SCHEMA_VERSION,
                            "operation": "accept",
                            "invitation_id": invite_id,
                            "organization_id": safe_response.get("organization_id").cloned().unwrap_or(Value::Null),
                            "status": safe_response.get("status").cloned().unwrap_or_else(|| json!("invitation_accepted")),
                            "backend_authorized": safe_response.get("backend_authorized").and_then(Value::as_bool).unwrap_or(false),
                            "membership_grants_paid_seats": false,
                            "seat_assignment_note": INVITE_ZERO_PAID_SEATS_NOTE,
                        })
                    }
                };
                attach_warnings(&mut result, warnings);
                Ok(result)
            },
        )
    }
}

pub fn identity(
    project_root: &Path,
    identity: &RepositoryIdentity,
    op: CloudIdentityOp,
) -> Result<Value, DecapodError> {
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (project_root, identity, op);
        crate::core::cloud_client::selected_origin()?;
        unreachable!("cloud origin requires supabase-cloud feature");
    }
    #[cfg(feature = "supabase-cloud")]
    {
        crate::core::cloud_client::with_admin_session(
            project_root,
            identity,
            |connector, _guard, bearer, mut warnings| {
                let repo = Some(identity.canonical_name.as_str());
                let mut result = match op {
                    CloudIdentityOp::PrepareLink { provider } => {
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "prepare_provider_link",
                                "target_provider": provider.as_str(),
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        for key in ["authorization_url", "stepup_url", "browser_url"] {
                            if let Some(raw_url) = response.get(key).and_then(Value::as_str) {
                                validate_https_url_basic(raw_url, key)?;
                            }
                        }
                        let mut out = json!({
                            "schema_version": CLOUD_IDENTITY_SCHEMA_VERSION,
                            "operation": "prepare_link",
                            "target_provider": provider.as_str(),
                            "link_intent_id": response.get("link_intent_id").cloned().unwrap_or(Value::Null),
                            "expires_at": response.get("expires_at").cloned().unwrap_or(Value::Null),
                            "status": response.get("status").cloned().unwrap_or_else(|| json!("link_intent_created")),
                        });
                        if let Some(secret) = response.get("stepup_secret").cloned() {
                            out["stepup_secret"] = secret;
                        }
                        if let Some(url) = response
                            .get("authorization_url")
                            .or_else(|| response.get("stepup_url"))
                            .cloned()
                        {
                            out["authorization_url"] = url;
                        }
                        out
                    }
                    CloudIdentityOp::ConfirmLink {
                        intent_id,
                        stepup_secret,
                    } => {
                        let link_intent_id = validate_uuid(&intent_id, "intent_id")?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "confirm_provider_link",
                                "link_intent_id": link_intent_id,
                                "stepup_secret": stepup_secret.expose(),
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        let safe = redact_json_value(&response);
                        json!({
                            "schema_version": CLOUD_IDENTITY_SCHEMA_VERSION,
                            "operation": "confirm_link",
                            "link_intent_id": link_intent_id,
                            "status": safe.get("status").cloned().unwrap_or_else(|| json!("confirmed")),
                            "account": safe,
                        })
                    }
                    CloudIdentityOp::ResolveQuarantine { provider, decision } => {
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "resolve_provider_quarantine",
                                "provider": provider.as_str(),
                                "decision": decision.as_str(),
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        let safe = redact_json_value(&response);
                        json!({
                            "schema_version": CLOUD_IDENTITY_SCHEMA_VERSION,
                            "operation": "resolve_quarantine",
                            "provider": provider.as_str(),
                            "decision": decision.as_str(),
                            "status": safe.get("status").cloned().unwrap_or_else(|| json!("resolved")),
                            "account": safe,
                        })
                    }
                    CloudIdentityOp::RevokeLink { provider } => {
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Register,
                            &json!({
                                "version": 1,
                                "operation": "revoke_provider_link",
                                "provider": provider.as_str(),
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        let safe = redact_json_value(&response);
                        json!({
                            "schema_version": CLOUD_IDENTITY_SCHEMA_VERSION,
                            "operation": "revoke_link",
                            "provider": provider.as_str(),
                            "status": safe.get("status").cloned().unwrap_or_else(|| json!("revoked")),
                            "account": safe,
                        })
                    }
                };
                attach_warnings(&mut result, warnings);
                Ok(result)
            },
        )
    }
}

pub fn billing(
    project_root: &Path,
    identity: &RepositoryIdentity,
    op: CloudBillingOp,
) -> Result<Value, DecapodError> {
    #[cfg(not(feature = "supabase-cloud"))]
    {
        let _ = (project_root, identity, op);
        crate::core::cloud_client::selected_origin()?;
        unreachable!("cloud origin requires supabase-cloud feature");
    }
    #[cfg(feature = "supabase-cloud")]
    {
        crate::core::cloud_client::with_admin_session(
            project_root,
            identity,
            |connector, guard, bearer, mut warnings| {
                let repo = Some(identity.canonical_name.as_str());
                let mut result = match op {
                    CloudBillingOp::Portal { organization_id } => {
                        let org_id = resolve_org_id(
                            connector,
                            bearer,
                            organization_id.as_deref(),
                            None,
                            repo,
                            &mut warnings,
                        )?;
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Portal,
                            &json!({
                                "version": 1,
                                "organization_id": org_id,
                            }),
                            repo,
                            true,
                            true,
                            &mut warnings,
                        )?;
                        let raw_portal_url = response
                            .get("portal_url")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                                    CloudAuthStatus::ProviderUnavailable,
                                    "Cloud billing portal response omitted `portal_url`.",
                                    "Retry later or open Cloud Admin directly to access billing settings.",
                                ))
                            })?;
                        let portal_url = validate_https_portal_url(raw_portal_url)?;
                        json!({
                            "schema_version": CLOUD_BILLING_SCHEMA_VERSION,
                            "operation": "portal",
                            "organization_id": org_id,
                            "portal_url": portal_url,
                        })
                    }
                    CloudBillingOp::Limits { project_id } => {
                        let target_project_id = match project_id {
                            Some(pid) => validate_project_id_str(&pid)?,
                            None => {
                                crate::core::cloud_project::route(
                                    connector, guard, bearer, identity,
                                )?
                                .canonical_name
                            }
                        };
                        let response = account_call(
                            connector,
                            bearer,
                            dactyl_db::AccountOperation::Status,
                            &json!({
                                "version": 1,
                                "project_id": target_project_id,
                                "operation": "resource_limits",
                            }),
                            repo,
                            false,
                            false,
                            &mut warnings,
                        )?;
                        let resources = response
                            .get("resource_limits")
                            .filter(|v| v.is_object())
                            .ok_or_else(|| {
                                DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                                    CloudAuthStatus::ProviderUnavailable,
                                    "Cloud billing limits response did not contain a `resource_limits` object.",
                                    "Check project selection and billing status in Cloud Admin.",
                                ))
                            })?;
                        let spend_cap_status = resources
                            .pointer("/paid_overages/status")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        let spend_cap_warning = spend_cap_status.as_deref().is_some_and(|status| {
                            matches!(
                                status,
                                "warning" | "overage_cap_exhausted" | "exhausted" | "cap_reached"
                            )
                        });
                        json!({
                            "schema_version": CLOUD_BILLING_SCHEMA_VERSION,
                            "operation": "limits",
                            "project_id": target_project_id,
                            "backend_authorized": response.get("backend_authorized").cloned().unwrap_or(Value::Null),
                            "resource_limits": resources,
                            "spend_cap_status": spend_cap_status,
                            "spend_cap_warning": spend_cap_warning,
                        })
                    }
                };
                attach_warnings(&mut result, warnings);
                Ok(result)
            },
        )
    }
}

#[cfg(feature = "supabase-cloud")]
#[allow(clippy::too_many_arguments)]
fn account_call(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    operation: dactyl_db::AccountOperation,
    body: &Value,
    repo_slug: Option<&str>,
    mutating: bool,
    aal2_gated: bool,
    warnings: &mut Vec<CloudMetadataWarning>,
) -> Result<Value, DecapodError> {
    let mut req_meta = dactyl_db::CloudResponseMetadata::default();
    let response = connector
        .account_request_with_metadata(operation, body, bearer, None, None, &mut req_meta)
        .map_err(|err| {
            crate::core::cloud_client::admin_transport_error(
                err,
                repo_slug,
                mutating,
                aal2_gated,
                Some(bearer),
            )
        })?;
    warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
        &req_meta,
    ));
    if response.get("version").is_some_and(|v| v != &json!(1)) {
        return Err(DecapodError::CloudAuth(CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderUnavailable,
            "Cloud admin service returned an unsupported response envelope version.",
            "Upgrade Decapod or check service compatibility headers.",
        )));
    }
    if let Some(code) = response
        .get("code")
        .or_else(|| response.get("status"))
        .and_then(Value::as_str)
        && code == "mfa_step_up_required"
        && let Some(diag) =
            crate::core::cloud_client::diagnostic_for_status_code("mfa_step_up_required", repo_slug)
    {
        return Err(DecapodError::CloudAuth(diag));
    }
    Ok(response)
}

#[cfg(feature = "supabase-cloud")]
fn resolve_org_id(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    supplied_org: Option<&str>,
    project_id: Option<&str>,
    repo_slug: Option<&str>,
    warnings: &mut Vec<CloudMetadataWarning>,
) -> Result<String, DecapodError> {
    if let Some(org) = supplied_org {
        return validate_org_id_str(org);
    }
    let body = match project_id {
        Some(pid) => json!({ "version": 1, "project_id": pid }),
        None => json!({ "version": 1 }),
    };
    let status = account_call(
        connector,
        bearer,
        dactyl_db::AccountOperation::Status,
        &body,
        repo_slug,
        false,
        false,
        warnings,
    )?;
    if let Some(pid) = project_id
        && let Some(projects) = status.get("projects").and_then(Value::as_array)
        && let Some(org_id) = projects
            .iter()
            .find(|p| project_matches_id(p, pid))
            .and_then(|p| p.get("organization_id").and_then(Value::as_str))
    {
        return validate_org_id_str(org_id);
    }
    if let Some(org_id) = status.get("organization_id").and_then(Value::as_str) {
        return validate_org_id_str(org_id);
    }
    let orgs = status
        .get("organizations")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    match orgs {
        [] => Err(DecapodError::CloudAuth(CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "No managed cloud organization was found for this account.",
            "Enroll a repository or accept a workspace invitation first, or pass `--organization-id <uuid>` explicitly.",
        ))),
        [only] => only
            .get("id")
            .or_else(|| only.get("organization_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                    CloudAuthStatus::ProviderUnavailable,
                    "Cloud status returned an organization entry without an identifier.",
                    "Pass `--organization-id <uuid>` explicitly.",
                ))
            })
            .and_then(validate_org_id_str),
        _ => Err(DecapodError::ValidationError(
            "Multiple cloud organizations are available; specify `--organization-id <uuid>` explicitly (run `decapod cloud orgs`)."
                .into(),
        )),
    }
}

#[cfg(feature = "supabase-cloud")]
fn find_member_account_id_for_slot(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    org_id: &str,
    target_slot: u32,
    repo_slug: Option<&str>,
    warnings: &mut Vec<CloudMetadataWarning>,
) -> Result<String, DecapodError> {
    let mut after: Option<u64> = None;
    for _ in 0..MAX_PAGINATION_PAGES {
        let mut body = json!({
            "version": 1,
            "operation": "members",
            "organization_id": org_id,
        });
        if let Some(cursor) = after {
            body["after"] = json!(cursor);
        }
        let page = account_call(
            connector,
            bearer,
            dactyl_db::AccountOperation::Register,
            &body,
            repo_slug,
            false,
            false,
            warnings,
        )?;
        if let Some(members) = page.get("members").and_then(Value::as_array) {
            for entry in members {
                let entry_slot = entry
                    .get("slot")
                    .or_else(|| entry.get("seat_slot"))
                    .and_then(Value::as_u64);
                if entry_slot == Some(u64::from(target_slot)) {
                    let pending = entry
                        .get("pending")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let account_id = entry
                        .get("account_id")
                        .or_else(|| entry.get("member_account_id"))
                        .and_then(Value::as_str);
                    if pending || account_id.is_none() {
                        return Err(DecapodError::ValidationError(format!(
                            "Seat slot {target_slot} in organization `{org_id}` is pending acceptance and has no assigned member account ID yet."
                        )));
                    }
                    return validate_uuid(account_id.unwrap(), "account_id");
                }
            }
        }
        match page.get("next").and_then(Value::as_u64) {
            Some(next_cursor) if Some(next_cursor) != after => after = Some(next_cursor),
            _ => break,
        }
    }
    Err(DecapodError::ValidationError(format!(
        "Seat slot {target_slot} was not found among active members in organization `{org_id}` (run `decapod cloud members list`)."
    )))
}

#[cfg(feature = "supabase-cloud")]
fn project_matches_id(project: &Value, pid: &str) -> bool {
    project.get("project_id").and_then(Value::as_str) == Some(pid)
        || project.get("id").and_then(Value::as_str) == Some(pid)
        || project.get("canonical_name").and_then(Value::as_str) == Some(pid)
        || project.get("repo_id").and_then(Value::as_str) == Some(pid)
}

#[cfg(feature = "supabase-cloud")]
fn extract_paid_repo_count(billing: &Value) -> Option<u64> {
    billing
        .get("paid_repo_count")
        .or_else(|| billing.get("active_paid_repositories"))
        .or_else(|| billing.get("allocated_repositories"))
        .and_then(Value::as_u64)
        .or_else(|| {
            billing
                .get("capacities")
                .or_else(|| billing.get("repositories"))
                .and_then(Value::as_array)
                .map(|repos| repos.len() as u64)
        })
}

#[cfg(feature = "supabase-cloud")]
fn is_benign_billing_probe_error(err: &DecapodError) -> bool {
    let DecapodError::CloudAuth(diag) = err else {
        return false;
    };
    matches!(
        diag.error_code_str(),
        Some("billing_not_configured" | "billing_unavailable" | "admin_required" | "not_found")
    )
}

#[cfg(feature = "supabase-cloud")]
fn attach_warnings(result: &mut Value, warnings: Vec<CloudMetadataWarning>) {
    if !warnings.is_empty()
        && let Some(obj) = result.as_object_mut()
    {
        obj.insert("warnings".to_owned(), json!(warnings));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_secret_masks_debug_display_and_error_output() {
        let secret = RedactedSecret::new("super_secret_invite_token_123\r\n").unwrap();
        assert_eq!(secret.expose(), "super_secret_invite_token_123");
        let debug_str = format!("{secret:?}");
        let display_str = format!("{secret}");
        assert!(!debug_str.contains("super_secret_invite_token_123"));
        assert!(!display_str.contains("super_secret_invite_token_123"));
        assert!(debug_str.contains("[REDACTED]"));
        assert!(display_str.contains("[REDACTED]"));

        let invite_op = CloudInvitesOp::Accept {
            organization_id: None,
            invitation_id: "11111111-1111-4111-8111-111111111111".into(),
            invitation_secret: secret.clone(),
        };
        assert!(!format!("{invite_op:?}").contains("super_secret_invite_token_123"));

        let id_op = CloudIdentityOp::ConfirmLink {
            intent_id: "11111111-1111-4111-8111-111111111111".into(),
            stepup_secret: secret,
        };
        assert!(!format!("{id_op:?}").contains("super_secret_invite_token_123"));

        let empty_err = RedactedSecret::new("   \n").unwrap_err();
        assert!(!format!("{empty_err}").is_empty());
    }

    #[test]
    fn portal_url_validator_requires_strict_https_and_rejects_credentials() {
        assert_eq!(
            validate_https_portal_url("https://billing.stripe.com/p/session/live_abc?locale=en")
                .unwrap(),
            "https://billing.stripe.com/p/session/live_abc?locale=en"
        );
        assert_eq!(
            validate_https_portal_url("https://[::1]:8443/portal").unwrap(),
            "https://[::1]:8443/portal"
        );

        for bad in [
            "",
            "http://billing.stripe.com/p/session/123",
            "https://user:pass@billing.stripe.com/p/session/123",
            "https://user@billing.stripe.com/p/session/123",
            "https://billing.stripe.com/p/session/123#secret=xyz",
            "https://billing.stripe.com/p/session/123?access_token=abc",
            "https://billing.stripe.com/p/session/123?secret=abc",
            "https://billing.stripe.com/p/session/123?invitation_secret=abc",
            "https://billing.stripe.com/p/session/123?stepup_secret=abc",
            "https://billing.stripe.com/p/session/123?api_key=abc",
            "https:///missing-host",
            "https://bad host/portal",
            "https://billing.stripe.com:0/portal",
            "javascript:alert(1)",
        ] {
            assert!(
                validate_https_portal_url(bad).is_err(),
                "portal validator should reject `{bad}`"
            );
        }
    }

    #[test]
    fn redact_url_fragment_and_json_mask_secrets() {
        let masked = redact_url_fragment(
            "https://cloud.decapod.sh/accept?invitation_id=123#invitation_secret=topsecret999",
        );
        assert!(!masked.contains("topsecret999"));
        assert!(masked.ends_with("#[REDACTED]"));

        let err = validate_https_url_basic(
            "http://cloud.decapod.sh/accept#invitation_secret=leak_check",
            "invitation_url",
        )
        .unwrap_err();
        let err_text = err.to_string();
        assert!(
            !err_text.contains("leak_check"),
            "error leaked secret: {err_text}"
        );
        assert!(err_text.contains("[REDACTED]"));

        let input = serde_json::json!({
            "operation": "accept",
            "invitation_id": "11111111-1111-4111-8111-111111111111",
            "invitation_secret": "raw_invite_secret",
            "stepup_secret": "raw_stepup_secret",
            "invitation_url": "https://cloud.decapod.sh/accept?id=1#invitation_secret=raw_frag",
        });
        let redacted = redact_json_value(&input);
        let serialized = redacted.to_string();
        assert!(!serialized.contains("raw_invite_secret"));
        assert!(!serialized.contains("raw_stepup_secret"));
        assert!(!serialized.contains("raw_frag"));
        assert_eq!(redacted["invitation_secret"], "[REDACTED]");
        assert_eq!(redacted["stepup_secret"], "[REDACTED]");
        assert_eq!(
            redacted["invitation_url"],
            "https://cloud.decapod.sh/accept?id=1#[REDACTED]"
        );
    }

    #[test]
    fn validators_enforce_uuids_and_bounded_identifiers() {
        assert!(is_uuid_str("11111111-1111-4111-8111-111111111111"));
        assert!(!is_uuid_str("not-a-uuid"));
        assert!(!is_uuid_str("11111111-1111-4111-8111-11111111111z"));
        assert!(validate_uuid("11111111-1111-4111-8111-111111111111", "id").is_ok());
        assert!(validate_uuid("bad", "id").is_err());
        assert!(validate_org_id_str("org-acme-1").is_ok());
        assert!(validate_org_id_str("org acme").is_err());
        assert!(validate_project_id_str("owner/repo").is_ok());
        assert!(validate_project_id_str("").is_err());

        assert_eq!(
            "github".parse::<CloudIdentityProvider>().unwrap(),
            CloudIdentityProvider::Github
        );
        assert!("bitbucket".parse::<CloudIdentityProvider>().is_err());
        assert_eq!(
            "confirm".parse::<CloudQuarantineDecision>().unwrap(),
            CloudQuarantineDecision::Confirm
        );
        assert!("ignore".parse::<CloudQuarantineDecision>().is_err());
    }
}

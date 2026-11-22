//! Repository-bound cloud session orchestration. Dactyl owns every network hop;
//! this module owns private custody and decides which operations may be retried.

use crate::core::backend::StorageContext;
#[cfg(feature = "supabase-cloud")]
use crate::core::backend::{BackendRoute, CloudDatastore};
#[cfg(feature = "supabase-cloud")]
use crate::core::cloud_session::{Bootstrap, Phase, Session, SessionGuard};
use crate::core::cloud_session::{SessionState, SessionStore};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthErrorCode, CloudAuthStatus, DecapodError};
use crate::core::repo_identity::RepositoryIdentity;
use std::path::Path;
use std::time::Duration;

const LOCK_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(feature = "supabase-cloud")]
const REFRESH_LEAD_SECONDS: i64 = 60;
const RESTART: &str = "Agent: preserve the interrupted task and operation ID, then run `decapod cloud login --restart` to start a fresh sign-in without replaying the interrupted exchange. Share its browser link with the user, resume the saved flow, and retry the original task. The user does not need another CLI command.";
pub const CLOUD_PRICING_URL: &str = "https://decapodlabs.ai/pricing";

fn valid_repo_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment.len() <= 200
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

fn encode_rfc3986(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            let _ = write!(&mut encoded, "%{byte:02X}");
        }
    }
    encoded
}

pub(crate) fn is_valid_repo_slug(slug: &str) -> bool {
    let Some((owner, repo)) = slug.split_once('/') else {
        return false;
    };
    valid_repo_segment(owner) && valid_repo_segment(repo)
}

/// Build the repository-scoped Decapod Cloud enrollment URL when `repo_slug` is
/// a valid `<owner>/<repo>` slug; otherwise fall back to [`CLOUD_PRICING_URL`].
pub fn pricing_enrollment_url(repo_slug: Option<&str>) -> String {
    let Some(slug) = repo_slug.filter(|s| is_valid_repo_slug(s)) else {
        return CLOUD_PRICING_URL.to_string();
    };
    format!("{CLOUD_PRICING_URL}?repo={}", encode_rfc3986(slug))
}

fn diagnostic(
    status: CloudAuthStatus,
    message: &str,
    next_action: impl Into<String>,
) -> DecapodError {
    DecapodError::CloudAuth(CloudAuthDiagnostic::new(status, message, next_action))
}

#[cfg(feature = "supabase-cloud")]
fn restart_required() -> DecapodError {
    diagnostic(
        CloudAuthStatus::RestartRequired,
        "A prior sign-in or refresh has an uncertain outcome. Its one-time request was not repeated.",
        RESTART,
    )
}

/// Staged first-party Decapod Cloud custom HTTPS origin (`dactyl-db 0.19.0` `#138`).
/// Default traffic remains on `DEFAULT_CLOUD_ORIGIN` until custom-domain cutover
/// is explicitly opted into via `DECAPOD_CLOUD_FIRST_PARTY_ORIGIN=1`.
#[cfg(feature = "supabase-cloud")]
pub const FIRST_PARTY_CLOUD_ORIGIN: &str = dactyl_db::FIRST_PARTY_CLOUD_ORIGIN;
#[cfg(not(feature = "supabase-cloud"))]
pub const FIRST_PARTY_CLOUD_ORIGIN: &str = "https://api.decapodlabs.ai";

#[cfg(feature = "supabase-cloud")]
pub const FIRST_PARTY_AUTH_PATH: &str = dactyl_db::FIRST_PARTY_AUTH_PATH;
#[cfg(not(feature = "supabase-cloud"))]
pub const FIRST_PARTY_AUTH_PATH: &str = "/auth";

#[cfg(feature = "supabase-cloud")]
pub const FIRST_PARTY_REPO_PATH_PREFIX: &str = dactyl_db::FIRST_PARTY_REPO_PATH_PREFIX;
#[cfg(not(feature = "supabase-cloud"))]
pub const FIRST_PARTY_REPO_PATH_PREFIX: &str = "/decapod";

/// Whether staged first-party custom HTTPS origin opt-in (`#138`) is active.
pub fn first_party_origin_opted_in() -> bool {
    std::env::var("DECAPOD_CLOUD_FIRST_PARTY_ORIGIN")
        .ok()
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

/// Build the canonical `/auth` application URL bound strictly to `connector`'s
/// verified origin (`dactyl-db 0.19.0` `#138`).
#[cfg(feature = "supabase-cloud")]
pub fn canonical_auth_url(connector: &dactyl_db::CloudConnector) -> Result<String, DecapodError> {
    connector.canonical_auth_url().map_err(transport_error)
}

/// Build the canonical `/decapod/<org>/<repo>` application URL bound strictly
/// to `connector`'s verified origin (`dactyl-db 0.19.0` `#138`).
#[cfg(feature = "supabase-cloud")]
pub fn canonical_repository_url(
    connector: &dactyl_db::CloudConnector,
    identity: &RepositoryIdentity,
) -> Result<String, DecapodError> {
    let Some((org, repo)) = identity.canonical_name.split_once('/') else {
        return Err(DecapodError::ValidationError(
            "Repository identity must be in canonical `<org>/<repo>` form.".into(),
        ));
    };
    connector
        .canonical_repository_url(org, repo)
        .map_err(|err| transport_error_for_repository(err, Some(&identity.canonical_name)))
}

/// Select only a logical cloud connection. The built-in deployment and its
/// physical transport belong to Dactyl. A loopback-only fixture override is
/// explicit, process-scoped and never persisted as project policy.
#[cfg(feature = "supabase-cloud")]
pub fn selected_origin() -> Result<String, DecapodError> {
    if let Some(origin) = std::env::var_os("DECAPOD_CLOUD_TEST_ORIGIN") {
        if std::env::var("DECAPOD_TEST_MODE").as_deref() != Ok("1") {
            return Err(DecapodError::Config(
                "The cloud fixture origin requires explicit test mode".into(),
            ));
        }
        let origin = origin
            .into_string()
            .map_err(|_| DecapodError::Config("Invalid cloud fixture origin".into()))?;
        // Dactyl owns URL parsing and transport policy. Restrict this CLI-only
        // fixture seam before any credential can be loaded or transmitted.
        dactyl_db::CloudConnector::validate_origin(&origin)?;
        if !(origin.starts_with("http://127.0.0.1:") || origin.starts_with("http://[::1]:")) {
            return Err(DecapodError::Config(
                "The cloud fixture origin must use a literal loopback address".into(),
            ));
        }
        return Ok(origin.trim_end_matches('/').to_owned());
    }
    if first_party_origin_opted_in() {
        return Ok(FIRST_PARTY_CLOUD_ORIGIN.to_owned());
    }
    Ok(dactyl_db::CloudConnector::default_origin())
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn selected_origin() -> Result<String, DecapodError> {
    Err(DecapodError::Config(
        "Cloud requires the supabase-cloud capability; no local fallback is permitted".into(),
    ))
}

pub(crate) fn diagnostic_for_status_code(
    code: &str,
    repo_slug: Option<&str>,
) -> Option<CloudAuthDiagnostic> {
    let enroll_url = pricing_enrollment_url(repo_slug);
    let diagnostic = match code {
        "mfa_step_up_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::MfaRequired,
            "Account-level AAL2 MFA step-up is required before device enrollment or privileged custody.",
            "Complete account MFA verification in the browser, then retry the command.",
        )
        .with_error_code(CloudAuthErrorCode::MfaStepUpRequired),
        "admin_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Unauthorized,
            "Organization administrator privileges are required for this operation.",
            "Ask an organization administrator to perform this action, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::AdminRequired),
        "owner_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Unauthorized,
            "Organization owner privileges are required to transfer ownership or modify owner custody.",
            "Ask the current organization owner to perform this action, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::OwnerRequired),
        "provider_quarantined" | "quarantined_provider_link" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "This OAuth provider identity link is currently quarantined and requires explicit confirmation or revocation.",
            "Run `decapod cloud identity resolve-quarantine --provider <google|github|gitlab> --decision <confirm|revoke>` before continuing.",
        )
        .with_error_code(
            CloudAuthErrorCode::from_code_str(code)
                .unwrap_or(CloudAuthErrorCode::ProviderQuarantined),
        ),
        "custody_conflict" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "Cloud organization or member custody changed concurrently.",
            "Refresh live organization/member state (`decapod cloud orgs`, `decapod cloud members list`) and retry with the current revision.",
        )
        .with_error_code(CloudAuthErrorCode::CustodyConflict),
        "owner_protected" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Unauthorized,
            "Organization owner membership or seat is protected until ownership is explicitly transferred.",
            "Use `decapod cloud members transfer-ownership` before revoking the current owner.",
        )
        .with_error_code(CloudAuthErrorCode::OwnerProtected),
        "member_not_found" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "The target member or seat slot was not found in this organization.",
            "Run `decapod cloud members list` to inspect current slot and member UUIDs.",
        )
        .with_error_code(CloudAuthErrorCode::MemberNotFound),
        "invitation_expired" | "invitation_not_found" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Expired,
            "The workspace invitation was not found, has already been consumed, or has expired.",
            "Run `decapod cloud invites list` to check pending invitations or request a new invite.",
        )
        .with_error_code(
            CloudAuthErrorCode::from_code_str(code)
                .unwrap_or(CloudAuthErrorCode::InvitationNotFound),
        ),
        "project_not_found" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "The target cloud repository project was not found or is not accessible to this organization.",
            "Run `decapod cloud projects` to verify the exact project identifier.",
        )
        .with_error_code(CloudAuthErrorCode::ProjectNotFound),
        "seat_revoked" => CloudAuthDiagnostic::new(
            CloudAuthStatus::EntitlementDenied,
            "This paid repository seat was revoked.",
            "Ask an organization administrator to assign an active slot via `decapod cloud members assign`.",
        )
        .with_error_code(CloudAuthErrorCode::SeatRevoked),
        "billing_not_configured" | "billing_unavailable" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderUnavailable,
            "Cloud billing or portal configuration is unavailable for this organization.",
            "Check organization subscription enrollment or retry once billing integration is configured.",
        )
        .with_error_code(
            CloudAuthErrorCode::from_code_str(code)
                .unwrap_or(CloudAuthErrorCode::BillingUnavailable),
        ),
        "registration_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::RegistrationRequired,
            "The service has not registered this account.",
            "Complete account registration in Cloud Admin, then retry.",
        ),
        "provider_link_stepup_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderLinkStepupRequired,
            "The service requires provider-link verification for this account.",
            "Complete provider verification in Cloud Admin, then retry.",
        ),
        "code_host_link_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "The managed account needs a verified code-host link.",
            "Complete account linking in the cloud portal, then retry.",
        ),
        "payment_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::PaymentRequired,
            "This repository requires an active Decapod Cloud subscription before governed access can resume.",
            format!(
                "Open {enroll_url} to subscribe and enroll this repository (no Admin setup wizard is required), then retry your original command once checkout completes."
            ),
        )
        .with_enrollment_url(enroll_url),
        "pending_fulfillment" | "payment_pending" => CloudAuthDiagnostic::new(
            CloudAuthStatus::PendingFulfillment,
            "Payment is complete and Decapod Cloud is finalizing repository fulfillment.",
            "Keep the saved session (do not restart sign-in), wait briefly for webhook fulfillment to settle, and retry `decapod cloud login --format json` or your original command.",
        ),
        "admin_approval_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::AdminApprovalRequired,
            "This account does not have an assigned paid repository seat; organization admin privileges alone do not grant repository data access.",
            "Organization admin access and paid repository seats are separate. Ask an organization administrator to assign a paid seat and approve access for this repository, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::AdminApprovalRequired),
        "organization_already_enrolled" => CloudAuthDiagnostic::new(
            CloudAuthStatus::OrganizationAlreadyEnrolled,
            "This organization is already enrolled under an existing immutable Cloud organization record.",
            "Do not purchase a duplicate organization subscription. Ask an existing organization administrator for an invite or seat assignment for this repository, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::OrganizationAlreadyEnrolled),
        "paid_seat_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::EntitlementDenied,
            "A paid repository seat is required to access this cloud repository.",
            "Ask an organization administrator to assign a paid seat (`decapod cloud members assign`) or enroll additional capacity, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::PaidSeatRequired),
        "seat_capacity_exceeded" => CloudAuthDiagnostic::new(
            CloudAuthStatus::QuotaExceeded,
            "Organization paid repository seat capacity is full.",
            "Revoke an unused seat (`decapod cloud members revoke-seat`) or increase subscription capacity via `decapod cloud billing portal`, then retry.",
        )
        .with_error_code(CloudAuthErrorCode::SeatCapacityExceeded),
        "tenant_upgrade_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "This cloud organization requires a tenant schema or plan upgrade before continuing.",
            "Run `decapod cloud upgrades status` and apply the required organization upgrade (`decapod cloud upgrades request` / `apply`).",
        )
        .with_error_code(CloudAuthErrorCode::TenantUpgradeRequired),
        "tenant_upgrading" => CloudAuthDiagnostic::new(
            CloudAuthStatus::PendingFulfillment,
            "This cloud organization tenant upgrade is currently in progress.",
            "Wait for the upgrade to complete and check `decapod cloud upgrades status` before retrying.",
        )
        .with_error_code(CloudAuthErrorCode::TenantUpgrading),
        "tenant_suspended" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Unauthorized,
            "This cloud organization tenant is suspended.",
            "Inspect billing and recovery state (`decapod cloud billing portal` or `decapod cloud recovery status`) with an organization administrator.",
        )
        .with_error_code(CloudAuthErrorCode::TenantSuspended),
        "tenant_archived" => CloudAuthDiagnostic::new(
            CloudAuthStatus::RepositoryDenied,
            "This cloud organization tenant has been archived and is read-restricted.",
            "Confirm organization status (`decapod cloud orgs`) or initiate recovery (`decapod cloud recovery status`) if eligible.",
        )
        .with_error_code(CloudAuthErrorCode::TenantArchived),
        "org_recovery_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "Organization custody recovery is required before administrative mutations can proceed.",
            "Inspect recovery status (`decapod cloud recovery status`) or initiate recovery (`decapod cloud recovery initiate`).",
        )
        .with_error_code(CloudAuthErrorCode::OrgRecoveryRequired),
        "policy_template_conflict" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "The requested cloud policy template conflicts with the organization's current revision or constraints.",
            "Inspect current templates (`decapod cloud policies list`) and retry with an updated template payload.",
        )
        .with_error_code(CloudAuthErrorCode::PolicyTemplateConflict),
        "grant_revoked" | "grant_expired" | "scope_denied" | "repo_access_denied" => {
            CloudAuthDiagnostic::new(
                CloudAuthStatus::RepositoryDenied,
                "The organization session or repository grant does not authorize this scope.",
                "Verify organization/repository access (`decapod cloud orgs`, `decapod cloud projects`) or re-establish a scoped session via `decapod cloud login --restart`.",
            )
            .with_error_code(
                CloudAuthErrorCode::from_code_str(code)
                    .unwrap_or(CloudAuthErrorCode::ScopeDenied),
            )
        }
        "provenance_origin_mismatch" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderUnavailable,
            "Cloud response provenance did not match the verified service origin.",
            "Verify the configured cloud origin (`decapod cloud status`) and retry without following untrusted redirects.",
        )
        .with_error_code(CloudAuthErrorCode::ProvenanceOriginMismatch),
        "idempotency_conflict" | "idempotency_in_progress" | "version_conflict" => {
            CloudAuthDiagnostic::new(
                CloudAuthStatus::ActionRequired,
                "The cloud service reported a concurrency, version, or idempotency conflict.",
                "Inspect live cloud state before retrying with a fresh attempt or current revision.",
            )
            .with_error_code(
                CloudAuthErrorCode::from_code_str(code)
                    .unwrap_or(CloudAuthErrorCode::Conflict),
            )
        }
        "transaction_aborted" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderUnavailable,
            "The cloud service aborted the transaction due to serialization contention.",
            "Retry the operation after inspecting current state.",
        )
        .with_error_code(CloudAuthErrorCode::TransactionAborted),
        "quota_exceeded" => CloudAuthDiagnostic::new(
            CloudAuthStatus::QuotaExceeded,
            "The cloud service declined the request because the included quota is exhausted.",
            "Ask a billing administrator to review the plan or eligible paid overages in Cloud Admin before trying again.",
        ),
        "overage_cap_exhausted" => CloudAuthDiagnostic::new(
            CloudAuthStatus::OverageCapExhausted,
            "The cloud service declined new billable usage because the spend cap was reached.",
            "Ask a billing administrator to review the current spend cap in Cloud Admin. Already admitted tasks can still use supported terminal operations.",
        ),
        "provider_disabled" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ProviderDisabled,
            "This sign-in provider is disabled for the cloud deployment.",
            "Choose an enabled provider on the Cloud Admin sign-in page.",
        ),
        "entitlement_denied" => CloudAuthDiagnostic::new(
            CloudAuthStatus::EntitlementDenied,
            "The service denied repository entitlement for this account.",
            "Ask the account owner to assign or restore a seat for this repository, then retry.",
        ),
        "device_revoked" | "device_key_expired" | "device_not_enrolled" => CloudAuthDiagnostic::new(
            CloudAuthStatus::DeviceRevoked,
            "This enrolled device key has been revoked or expired on the cloud service.",
            "Run `decapod cloud revoke` to clear local device state, then `decapod cloud login --restart` to enroll a fresh device key.",
        )
        .with_error_code(
            CloudAuthErrorCode::from_code_str(code)
                .unwrap_or(CloudAuthErrorCode::DeviceRevoked),
        ),
        "entitlement_required"
        | "organization_seat_required"
        | "seat_required"
        | "seat_unassigned"
        | "account_setup_required"
        | "identity_link_required"
        | "quote_confirmation_required"
        | "billing_binding_required" => CloudAuthDiagnostic::new(
            CloudAuthStatus::ActionRequired,
            "The service has not confirmed the repository account entitlement.",
            format!(
                "Complete repository enrollment at {enroll_url} if this repository is not yet enrolled, or ask your organization administrator to assign a seat (no Admin setup wizard is required), then retry. Payment or browser completion alone does not establish access."
            ),
        )
        .with_enrollment_url(enroll_url),
        "authorization_denied" | "repository_not_authorized" => CloudAuthDiagnostic::new(
            CloudAuthStatus::RepositoryDenied,
            "The service denied access to this repository.",
            "Run `decapod cloud projects` and explicitly select the intended project with `decapod cloud project use`, or ask its administrator to review access.",
        ),
        "outcome_unknown" => CloudAuthDiagnostic::new(
            CloudAuthStatus::RestartRequired,
            "A prior sign-in or refresh has an uncertain outcome. Its one-time request was not repeated.",
            RESTART,
        ),
        "flow_expired" | "flow_not_found" | "invalid_flow_state" => CloudAuthDiagnostic::new(
            CloudAuthStatus::Expired,
            "The pending sign-in can no longer be completed.",
            RESTART,
        ),
        "invalid_dpop_proof"
        | "use_dpop_nonce"
        | "dpop_replay"
        | "invalid_binding"
        | "dpop_proof_required"
        | "missing_dpop_proof"
        | "dpop_nonce_mismatch"
        | "dpop_clock_skew"
        | "dpop_ath_mismatch"
        | "dpop_htu_mismatch"
        | "dpop_htm_mismatch"
        | "dpop_jkt_mismatch"
        | "dpop_aud_mismatch"
        | "stolen_bearer" => CloudAuthDiagnostic::new(
            CloudAuthStatus::AuthRequired,
            "The service rejected the request DPoP proof, audience binding, or replay nonce.",
            "Retry the command with a fresh proof; if the device key was rotated or revoked, run `decapod cloud login --restart`.",
        )
        .with_error_code(
            CloudAuthErrorCode::from_code_str(code)
                .unwrap_or(CloudAuthErrorCode::InvalidDpopProof),
        ),
        "invalid_token"
        | "session_expired"
        | "session_revoked"
        | "session_not_found"
        | "bad_jwt"
        | "invalid_grant"
        | "refresh_token_not_found"
        | "refresh_token_already_used"
        | "refresh_reuse_detected"
        | "expired_credential" => {
            let mut diag = CloudAuthDiagnostic::new(
                CloudAuthStatus::Revoked,
                "The service rejected the managed session.",
                RESTART,
            );
            if let Some(err_code) = CloudAuthErrorCode::from_code_str(code) {
                diag = diag.with_error_code(err_code);
            }
            diag
        }
        _ => return None,
    };
    Some(diagnostic)
}

pub fn transport_error(error: dactyl_db::DactylError) -> DecapodError {
    transport_error_for_repository(error, None)
}

pub fn transport_error_for_repository(
    error: dactyl_db::DactylError,
    repo_slug: Option<&str>,
) -> DecapodError {
    use dactyl_db::AdapterErrorKind;
    if let Some(code) = error.adapter_code()
        && let Some(diagnostic) = diagnostic_for_status_code(code, repo_slug)
    {
        return DecapodError::CloudAuth(diagnostic);
    }
    let (status, message, action) = match error {
        dactyl_db::DactylError::Adapter {
            kind: AdapterErrorKind::Authentication,
            ..
        } => (
            CloudAuthStatus::AuthRequired,
            "The service requires a valid managed sign-in.",
            RESTART,
        ),
        dactyl_db::DactylError::Adapter {
            kind: AdapterErrorKind::Authorization,
            ..
        } => (
            CloudAuthStatus::RepositoryDenied,
            "The service denied this account operation.",
            "Ask the account owner to review access, then retry.",
        ),
        dactyl_db::DactylError::Adapter {
            kind: AdapterErrorKind::Quota,
            ..
        } => (
            CloudAuthStatus::QuotaExceeded,
            "The cloud service declined the request because the quota or payload limit was exceeded.",
            "Ask a billing administrator to review the plan or eligible paid overages in Cloud Admin before trying again.",
        ),
        dactyl_db::DactylError::Adapter {
            kind: AdapterErrorKind::Timeout | AdapterErrorKind::Transport,
            ..
        } => (
            CloudAuthStatus::Offline,
            "The cloud service could not be reached within the bounded request.",
            "Restore connectivity and retry. No cloud data operation was retried or redirected to local storage.",
        ),
        _ => (
            CloudAuthStatus::ProviderUnavailable,
            "The cloud service did not return a supported successful response.",
            "Retry after the service is available. Persisted one-time operations require explicit sign-in restart.",
        ),
    };
    diagnostic(status, message, action)
}

#[cfg(feature = "supabase-cloud")]
fn bearer_aal2_claim(token: &str) -> Option<bool> {
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let _sig = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let decoded = crate::core::cloud_dpop::base64url_decode(payload).ok()?;
    let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    if let Some(aal) = json.get("aal").and_then(|v| v.as_str()) {
        Some(aal.eq_ignore_ascii_case("aal2"))
    } else {
        // JWTs without an `aal` claim have not established AAL2 step-up.
        Some(false)
    }
}

#[cfg(feature = "supabase-cloud")]
pub(crate) fn admin_transport_error(
    error: dactyl_db::DactylError,
    repo_slug: Option<&str>,
    mutating: bool,
    aal2_gated: bool,
    bearer: Option<&str>,
) -> DecapodError {
    use dactyl_db::{AdapterErrorKind, MutationOutcome};

    if error.mutation_outcome() == MutationOutcome::Uncertain
        || (mutating
            && (error.adapter_code() == Some("outcome_unknown")
                || matches!(
                    error.adapter_kind(),
                    Some(
                        AdapterErrorKind::Transport
                            | AdapterErrorKind::Timeout
                            | AdapterErrorKind::Unavailable
                    )
                )))
    {
        return DecapodError::CloudAuth(
            CloudAuthDiagnostic::new(
                CloudAuthStatus::OutcomeUnknown,
                "The cloud admin mutation failed after sending or returned an uncertain outcome; Decapod did not retry it automatically.",
                "Inspect live server state (`decapod cloud orgs`, `decapod cloud members list`, `decapod cloud invites list`) before retrying so the change is not applied twice.",
            )
            .with_error_code(CloudAuthErrorCode::OutcomeUnknown),
        );
    }

    if let Some(code) = error.adapter_code()
        && let Some(diagnostic) = diagnostic_for_status_code(code, repo_slug)
    {
        return DecapodError::CloudAuth(diagnostic);
    }

    if error.adapter_kind() == Some(AdapterErrorKind::Authorization)
        && error.adapter_code().is_none()
    {
        if aal2_gated && bearer.and_then(bearer_aal2_claim) != Some(true) {
            return DecapodError::CloudAuth(
                CloudAuthDiagnostic::new(
                    CloudAuthStatus::MfaRequired,
                    "This privileged cloud admin mutation requires account-level AAL2 MFA step-up (`mfa_step_up_required`) or organization administrator authorization.",
                    "Complete AAL2 MFA verification in Cloud Admin (or verify organization admin/owner permissions) and retry.",
                )
                .with_error_code(CloudAuthErrorCode::MfaStepUpRequired),
            );
        }
        return DecapodError::CloudAuth(
            CloudAuthDiagnostic::new(
                CloudAuthStatus::Unauthorized,
                "Organization administrator or owner authorization is required for this cloud admin operation.",
                "Ask an organization administrator or owner to grant permission or perform this operation.",
            )
            .with_error_code(CloudAuthErrorCode::AdminRequired),
        );
    }

    if error.adapter_kind() == Some(AdapterErrorKind::Conflict) && error.adapter_code().is_none() {
        return DecapodError::CloudAuth(
            CloudAuthDiagnostic::new(
                CloudAuthStatus::ActionRequired,
                "The cloud service rejected this operation due to a state conflict (such as a stale membership revision, protected owner, or duplicate pending invite).",
                "Refresh live state (`decapod cloud orgs`, `decapod cloud members list`, `decapod cloud invites list`) and retry with current identifiers/revision.",
            )
            .with_error_code(CloudAuthErrorCode::Conflict),
        );
    }

    if error.adapter_kind() == Some(AdapterErrorKind::NotFound)
        && matches!(error.adapter_code(), None | Some("not_found"))
    {
        return DecapodError::CloudAuth(
            CloudAuthDiagnostic::new(
                CloudAuthStatus::ActionRequired,
                "The requested cloud organization, member, invitation, link intent, or project was not found.",
                "List live cloud resources first (`decapod cloud orgs`, `decapod cloud members list`, `decapod cloud invites list`) and retry with an existing UUID.",
            )
            .with_error_code(CloudAuthErrorCode::NotFound),
        );
    }

    transport_error_for_repository(error, repo_slug)
}

#[cfg(feature = "supabase-cloud")]
fn secrets(bootstrap: &Bootstrap) -> Result<dactyl_db::AuthenticationSecrets, DecapodError> {
    // The registry API supports private persistence through its Serde contract.
    // Restore the caller-owned record; Dactyl validates it before transport.
    serde_json::from_value(serde_json::json!({
        "verifier": bootstrap.pkce_verifier,
        "poll_secret": bootstrap.poll_secret,
    }))
    .map_err(|_| DecapodError::SessionError("Invalid private cloud sign-in record".into()))
}

#[cfg(feature = "supabase-cloud")]
fn one_time_error(error: dactyl_db::DactylError, repo_slug: Option<&str>) -> DecapodError {
    if matches!(
        error.adapter_kind(),
        Some(
            dactyl_db::AdapterErrorKind::Transport
                | dactyl_db::AdapterErrorKind::Timeout
                | dactyl_db::AdapterErrorKind::Protocol
                | dactyl_db::AdapterErrorKind::Unavailable
        )
    ) {
        restart_required()
    } else {
        transport_error_for_repository(error, repo_slug)
    }
}

#[cfg(feature = "supabase-cloud")]
fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

#[cfg(feature = "supabase-cloud")]
fn managed_session(session: dactyl_db::UserSession) -> Result<Session, DecapodError> {
    let expires_at = i64::try_from(session.expires_at_unix).map_err(|_| {
        diagnostic(
            CloudAuthStatus::ProviderUnavailable,
            "The service returned an invalid session expiry.",
            RESTART,
        )
    })?;
    if expires_at <= now() {
        return Err(diagnostic(
            CloudAuthStatus::Expired,
            "The service returned an expired session.",
            RESTART,
        ));
    }
    Ok(Session {
        access_token: session.access_token,
        refresh_token: Some(session.refresh_token),
        expires_at,
    })
}

#[cfg(feature = "supabase-cloud")]
fn handoff_expiry(value: &str) -> Result<i64, DecapodError> {
    let expiry = crate::core::time::parse_rfc3339(value)
        .map_err(|_| {
            diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "The service returned an invalid sign-in expiry.",
                RESTART,
            )
        })?
        .unix_timestamp();
    if expiry <= now() || expiry > now().saturating_add(30 * 60) {
        return Err(diagnostic(
            CloudAuthStatus::Expired,
            "The service returned an expired or unsupported sign-in lifetime.",
            RESTART,
        ));
    }
    Ok(expiry)
}

#[cfg(feature = "supabase-cloud")]
fn pending(bootstrap: &Bootstrap) -> DecapodError {
    // The validated one-time browser handoff is intentionally shown to the
    // human. Access tokens, refresh tokens, verifier and polling secret are not.
    let Some(url) = bootstrap.browser_url.as_deref() else {
        return restart_required();
    };
    DecapodError::CloudAuth(
        CloudAuthDiagnostic::new(
            CloudAuthStatus::OnboardingPending,
            "Sign in once to connect this repository. Your agent will take it from here.",
            "Agent: share authorization_url with the user, then run `decapod cloud login --format json` every 2 seconds for up to 5 minutes. Continue the original task as soon as sign-in completes, without waiting for another user message. Preserve the saved connection if still pending. The user does not need another CLI command.",
        )
        .with_authorization_url(url),
    )
}

#[cfg(feature = "supabase-cloud")]
fn context(
    origin: &str,
    identity: &RepositoryIdentity,
    session: &Session,
) -> Result<StorageContext, DecapodError> {
    StorageContext::from_route(
        BackendRoute::cloud(identity.clone(), origin)?,
        Some(&session.access_token),
    )?
    .with_cloud_datastore(CloudDatastore::Managed)
}

#[cfg(feature = "supabase-cloud")]
fn persist_restart(guard: &mut SessionGuard, mut state: SessionState) -> Result<(), DecapodError> {
    state.phase = Phase::RestartRequired;
    guard.replace(state)
}

#[cfg(feature = "supabase-cloud")]
fn terminal_session_error(error: &dactyl_db::DactylError) -> bool {
    error.adapter_kind() == Some(dactyl_db::AdapterErrorKind::Authentication)
        || matches!(
            error.adapter_code(),
            Some("flow_expired" | "flow_not_found" | "invalid_flow_state")
        )
}

/// Stop reusing a credential explicitly rejected by a data operation. Compare
/// under the same refresh lock, so an old in-flight response cannot invalidate
/// a credential that a concurrent command has already rotated successfully.
pub fn record_rejected_credential(
    project_root: &Path,
    context: &StorageContext,
) -> Result<(), DecapodError> {
    let Some(origin) = context.route().cloud_uri() else {
        return Ok(());
    };
    // Data routing may use a server project reference. Credential custody is
    // still scoped to the original Git checkout, not that routing reference.
    let identity = crate::core::repo_identity::resolve_repository_identity(project_root)?;
    let Some(rejected) = context.bearer() else {
        return Ok(());
    };
    let store = SessionStore::for_project(origin, &identity.canonical_name, project_root)?;
    let mut guard = store.lock(LOCK_TIMEOUT)?;
    let mut state = guard.state().clone();
    if state
        .session
        .as_ref()
        .is_some_and(|session| session.access_token == rejected)
    {
        state.phase = crate::core::cloud_session::Phase::RestartRequired;
        guard.replace(state)?;
    }
    #[cfg(feature = "supabase-cloud")]
    {
        let matched_repo = guard
            .device_session(&identity.canonical_name)?
            .is_some_and(|session| session.access_token == rejected);
        let matched_routed = match context.route() {
            BackendRoute::Cloud { repository, .. }
                if repository.canonical_name != identity.canonical_name =>
            {
                guard
                    .device_session(&repository.canonical_name)?
                    .is_some_and(|session| session.access_token == rejected)
            }
            _ => false,
        };
        if matched_repo || matched_routed {
            guard.clear_device_session()?;
        }
    }
    Ok(())
}

/// Called by both initialization and normal cloud commands. A new invocation
/// polls an existing handoff even when stdin is not a terminal. One invocation
/// performs at most one status check and never sleeps or repeats a mutation.
#[cfg(feature = "supabase-cloud")]
fn acquire(
    project_root: &Path,
    identity: &RepositoryIdentity,
    restart: bool,
) -> Result<(dactyl_db::CloudConnector, SessionGuard), DecapodError> {
    let (connector, guard, _meta) = acquire_with_metadata(project_root, identity, restart)?;
    Ok((connector, guard))
}

#[cfg(feature = "supabase-cloud")]
fn acquire_with_metadata(
    project_root: &Path,
    identity: &RepositoryIdentity,
    restart: bool,
) -> Result<
    (
        dactyl_db::CloudConnector,
        SessionGuard,
        dactyl_db::CloudResponseMetadata,
    ),
    DecapodError,
> {
    let selected = selected_origin()?;
    let (mut connector, discover_meta) =
        dactyl_db::CloudConnector::discover_with_metadata(&selected).map_err(|error| {
            transport_error_for_repository(error, Some(&identity.canonical_name))
        })?;
    if let Some(signer) = crate::core::cloud_dpop::active_request_proof_signer(
        connector.origin(),
        &identity.canonical_name,
        project_root,
    ) {
        connector = connector.with_proof_signer(signer);
    }
    let store =
        SessionStore::for_project(connector.origin(), &identity.canonical_name, project_root)?;
    let mut guard = store.lock(LOCK_TIMEOUT)?;
    if restart {
        guard.clear_for_restart()?;
    }
    let mut state = guard.state().clone();
    if state.phase == Phase::RestartRequired {
        return Err(restart_required());
    }

    if state
        .session
        .as_ref()
        .is_some_and(|s| s.expires_at <= now().saturating_add(REFRESH_LEAD_SECONDS))
    {
        let refresh = state.session.as_ref().and_then(|s| s.refresh_token.clone());
        if let Some(refresh) = refresh {
            let following_phase = if state.bootstrap.is_some() {
                Phase::CheckingReadiness
            } else {
                Phase::Ready
            };
            state.phase = Phase::Refreshing;
            guard.replace(state.clone())?;
            let current_access_token = state
                .session
                .as_ref()
                .map(|s| s.access_token.clone())
                .unwrap_or_default();
            let refreshed = if crate::core::cloud_dpop::is_dpop_refresh_token(&refresh) {
                refresh_device_session_with_connector(
                    &connector,
                    project_root,
                    identity,
                    &current_access_token,
                    &refresh,
                )
                .map_err(|err| match err {
                    DpopStepError::Dactyl(error) => {
                        one_time_error(error, Some(&identity.canonical_name))
                    }
                    DpopStepError::Decapod(error) => error,
                })
            } else {
                connector
                    .refresh_session(&refresh)
                    .map_err(|error| one_time_error(error, Some(&identity.canonical_name)))
                    .and_then(managed_session)
            };
            match refreshed {
                Ok(session) => {
                    // Persist a received rotation before any subsequent call.
                    state.session = Some(session);
                    state.phase = following_phase;
                    guard.replace(state.clone())?;
                }
                Err(error) => {
                    persist_restart(&mut guard, state)?;
                    return Err(error);
                }
            }
        } else {
            // A known-expired credential without refresh capability has no
            // uncertain server mutation to recover. A new sign-in is required.
            state = SessionState::default();
            guard.replace(state.clone())?;
        }
    }

    if state.session.is_none() && state.phase == Phase::Empty {
        let bootstrap = Bootstrap::new()?;
        let private = secrets(&bootstrap)?;
        state.bootstrap = Some(bootstrap);
        state.phase = Phase::Preparing;
        guard.replace(state.clone())?;
        let flow = match connector.start_authentication(&identity.canonical_name, &private) {
            Ok(flow) => flow,
            Err(error) => {
                persist_restart(&mut guard, state)?;
                return Err(one_time_error(error, Some(&identity.canonical_name)));
            }
        };
        let expires_at = handoff_expiry(&flow.expires_at)?;
        let bootstrap = state.bootstrap.as_mut().expect("prepared bootstrap");
        bootstrap.flow_id = Some(flow.flow_id);
        bootstrap.browser_url = Some(flow.browser_url);
        bootstrap.expires_at = Some(expires_at);
        state.phase = Phase::Waiting;
        guard.replace(state.clone())?;
        return Err(pending(state.bootstrap.as_ref().expect("saved bootstrap")));
    }

    if state.session.is_none() && state.phase == Phase::Waiting {
        let bootstrap = state.bootstrap.as_ref().expect("validated waiting state");
        if bootstrap.expires_at.is_none_or(|expiry| expiry <= now()) {
            persist_restart(&mut guard, state)?;
            return Err(diagnostic(
                CloudAuthStatus::Expired,
                "The pending browser sign-in expired.",
                RESTART,
            ));
        }
        let private = secrets(bootstrap)?;
        let flow_id = bootstrap
            .flow_id
            .as_deref()
            .expect("validated flow")
            .to_owned();
        let status = connector
            .authentication_status(&flow_id, &private)
            .map_err(|error| {
                transport_error_for_repository(error, Some(&identity.canonical_name))
            })?;
        // The published connector returns only the validated flow status. The
        // saved flow is bound to this origin/repository/checkout; an authorized
        // repository read below remains the independent readiness requirement.
        match status.as_str() {
            "pending" => return Err(pending(bootstrap)),
            "ready" => {}
            "consumed" | "complete" => {
                persist_restart(&mut guard, state)?;
                return Err(restart_required());
            }
            _ => return Err(restart_required()),
        }
        state.phase = Phase::ExchangingBootstrap;
        guard.replace(state.clone())?;
        let exchange = match connector.exchange_authentication_code(&flow_id, &private) {
            Ok(exchange) => exchange,
            Err(error) => {
                persist_restart(&mut guard, state)?;
                return Err(one_time_error(error, Some(&identity.canonical_name)));
            }
        };
        state.bootstrap.as_mut().expect("exchange bootstrap").code = Some(exchange);
        state.phase = Phase::HaveCode;
        guard.replace(state.clone())?;
    }

    if state.session.is_none() && state.phase == Phase::HaveCode {
        let bootstrap = state.bootstrap.as_ref().expect("validated code state");
        if bootstrap.expires_at.is_none_or(|expiry| expiry <= now()) {
            persist_restart(&mut guard, state)?;
            return Err(diagnostic(
                CloudAuthStatus::Expired,
                "The pending sign-in expired before session exchange.",
                RESTART,
            ));
        }
        let private = secrets(bootstrap)?;
        let code = bootstrap
            .code
            .as_deref()
            .expect("validated saved code")
            .to_owned();
        state.phase = Phase::ExchangingSession;
        guard.replace(state.clone())?;
        let session = match connector.exchange_session(&code, &private) {
            Ok(session) => session,
            Err(error) => {
                persist_restart(&mut guard, state)?;
                return Err(one_time_error(error, Some(&identity.canonical_name)));
            }
        };
        state.session = Some(managed_session(session)?);
        state.bootstrap.as_mut().expect("session bootstrap").code = None;
        state.phase = Phase::CheckingReadiness;
        guard.replace(state.clone())?;
    }

    let session = state.session.as_ref().ok_or_else(restart_required)?;
    if state.phase == Phase::CheckingReadiness {
        let bootstrap = state
            .bootstrap
            .as_ref()
            .expect("validated completion state");
        let private = secrets(bootstrap)?;
        match connector.complete_authentication(
            bootstrap
                .flow_id
                .as_deref()
                .expect("validated completion flow"),
            &private,
            &session.access_token,
        ) {
            Ok(()) => (),
            Err(error) => {
                if error.adapter_code() == Some("outcome_unknown")
                    || matches!(
                        error.adapter_kind(),
                        Some(
                            dactyl_db::AdapterErrorKind::Transport
                                | dactyl_db::AdapterErrorKind::Timeout
                                | dactyl_db::AdapterErrorKind::Unavailable
                        )
                    )
                {
                    // Account completion is idempotent for the saved session.
                    // Its lost acknowledgement must not ask the human to
                    // repeat the one-time bootstrap or token exchanges.
                    return Err(diagnostic(
                        CloudAuthStatus::OnboardingPending,
                        "Account confirmation has an uncertain outcome; the issued session remains saved and readiness is unconfirmed.",
                        "Agent: retry `decapod cloud login --format json` every 2 seconds for up to 5 minutes to recheck account completion with the saved session, then resume the original task. Preserve the session if still pending. The user does not need to repeat sign-in or run a CLI command.",
                    ));
                }
                if terminal_session_error(&error) {
                    persist_restart(&mut guard, state)?;
                }
                return Err(transport_error_for_repository(
                    error,
                    Some(&identity.canonical_name),
                ));
            }
        };
        // Completion is service-confirmed and idempotent for this session.
        // Denied/action-required responses retain both the issued credential
        // and pending completion, so a later invocation can recheck safely.
        state.bootstrap = None;
        state.phase = Phase::Ready;
        guard.replace(state.clone())?;
    }
    let session = state.session.as_ref().ok_or_else(restart_required)?;
    if session.expires_at <= now() || state.phase != Phase::Ready {
        return Err(restart_required());
    }
    Ok((connector, guard, discover_meta))
}

#[cfg(feature = "supabase-cloud")]
pub(crate) fn with_admin_session<T, F>(
    project_root: &Path,
    identity: &RepositoryIdentity,
    f: F,
) -> Result<T, DecapodError>
where
    F: FnOnce(
        &dactyl_db::CloudConnector,
        &SessionGuard,
        &str,
        Vec<crate::core::propodus::CloudMetadataWarning>,
    ) -> Result<T, DecapodError>,
{
    let (connector, mut guard, discover_meta) =
        acquire_with_metadata(project_root, identity, false)?;
    let connector = connector.with_device_gateway();
    let bearer = guard
        .state()
        .session
        .as_ref()
        .ok_or_else(restart_required)?
        .access_token
        .clone();
    let initial_warnings = crate::core::propodus::warnings_from_cloud_metadata(&discover_meta);
    let result = f(&connector, &guard, &bearer, initial_warnings);
    account_result(&mut guard, result)
}

#[cfg(feature = "supabase-cloud")]
pub fn connect(
    project_root: &Path,
    identity: &RepositoryIdentity,
    restart: bool,
    check_readiness: bool,
) -> Result<StorageContext, DecapodError> {
    let (connector, mut guard) = acquire(project_root, identity, restart)?;
    let state = guard.state().clone();
    let human_session = state.session.as_ref().ok_or_else(restart_required)?.clone();
    let control_connector = connector.clone().with_device_gateway();
    let result = crate::core::cloud_project::route(
        &control_connector,
        &guard,
        &human_session.access_token,
        identity,
    );
    let routed = account_result(&mut guard, result)?;
    let (_active_connector, active_session) = resolve_operational_session(
        &connector,
        &mut guard,
        project_root,
        &human_session,
        &routed,
    )?;
    let used_device_sidecar = active_session.access_token != human_session.access_token;
    let context = context(connector.origin(), &routed, &active_session)?;
    if check_readiness {
        // A valid cached token is not evidence of current account access.
        // This uses the service's existing authorized task-read contract,
        // which checks grants/entitlement before executing the read. There is
        // no generic SELECT 1 or health-check authorization shortcut.
        if let Err(error) = crate::core::dactyl_todo::DactylTodoStore::new(
            context.clone(),
            routed.canonical_name.clone(),
        )
        .check_read_access()
        {
            return Err(match error {
                DecapodError::DactylError(error) => {
                    if terminal_session_error(&error) {
                        if used_device_sidecar {
                            guard.clear_device_session()?;
                        } else {
                            persist_restart(&mut guard, state)?;
                        }
                    }
                    transport_error_for_repository(error, Some(&routed.canonical_name))
                }
                error @ DecapodError::CloudAuth(_) => error,
                _ => diagnostic(
                    CloudAuthStatus::ProviderUnavailable,
                    "The service did not confirm repository read access.",
                    "Retry after the account or service issue is resolved.",
                ),
            });
        }
    }
    Ok(context)
}

/// Account/project administration stays reachable before data eligibility.
#[cfg(feature = "supabase-cloud")]
pub fn projects(
    project_root: &Path,
    identity: &RepositoryIdentity,
    operation: crate::core::cloud_project::Operation,
) -> Result<crate::core::cloud_project::ProjectOutput, DecapodError> {
    let (connector, mut guard) = acquire(project_root, identity, false)?;
    let connector = connector.with_device_gateway();
    let bearer = guard
        .state()
        .session
        .as_ref()
        .ok_or_else(restart_required)?
        .access_token
        .clone();
    let result = crate::core::cloud_project::execute(
        &connector,
        &mut guard,
        &bearer,
        operation,
        Some(&identity.canonical_name),
    );
    account_result(&mut guard, result)
}

#[cfg(feature = "supabase-cloud")]
fn account_result<T>(
    guard: &mut SessionGuard,
    result: Result<T, DecapodError>,
) -> Result<T, DecapodError> {
    if let Err(DecapodError::CloudAuth(diagnostic)) = &result
        && matches!(
            diagnostic.status,
            CloudAuthStatus::AuthRequired | CloudAuthStatus::Revoked
        )
    {
        let state = guard.state().clone();
        persist_restart(guard, state)?;
    }
    result
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn connect(
    _project_root: &Path,
    _identity: &RepositoryIdentity,
    _restart: bool,
    _check_readiness: bool,
) -> Result<StorageContext, DecapodError> {
    selected_origin()?;
    unreachable!("disabled cloud returns an error")
}

/// Inspect private state without sending a credential or claiming live access.
pub fn status(
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    let origin = selected_origin()?;
    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let guard = store.lock(LOCK_TIMEOUT)?;
    Ok(local_status(project_root, identity, &origin, guard.state()))
}

fn local_status(
    project_root: &Path,
    identity: &RepositoryIdentity,
    origin: &str,
    state: &SessionState,
) -> serde_json::Value {
    let device_key = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        origin,
        &identity.canonical_name,
        project_root,
    )
    .ok()
    .and_then(|ks| ks.load().ok().flatten());
    serde_json::json!({
        "schema_version": "decapod.cloud.session.v1",
        "repository": identity.canonical_name,
        "first_party_origin_staged": origin == FIRST_PARTY_CLOUD_ORIGIN,
        "phase": state.phase,
        "credential_present": state.session.is_some(),
        "expires_at": state.session.as_ref().map(|session| session.expires_at),
        "device_key_present": device_key.is_some(),
        "device_jkt": device_key.as_ref().map(|k| k.jkt.clone()),
        "device_keystore": device_key.as_ref().map(|k| k.backend.as_str()),
        "server_readiness": "not_checked",
    })
}

#[cfg(feature = "supabase-cloud")]
pub fn status_live(
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    use dactyl_db::{AccountOperation, CloudConnector, CloudResponseMetadata};

    let origin = selected_origin()?;
    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let guard = store.lock(LOCK_TIMEOUT)?;
    let state = guard.state();
    let session = state
        .session
        .as_ref()
        .filter(|session| {
            matches!(state.phase, Phase::Ready | Phase::CheckingReadiness)
                && session.expires_at > now()
        })
        .ok_or_else(|| {
            diagnostic(
                CloudAuthStatus::AuthRequired,
                "Live cloud status requires a current managed session.",
                "Run `decapod cloud login` to establish a session, then repeat live status.",
            )
        })?;
    let (mut connector, discover_meta) =
        CloudConnector::discover_with_metadata(&origin).map_err(transport_error)?;
    connector = connector.with_device_gateway();
    let mut warnings = crate::core::propodus::warnings_from_cloud_metadata(&discover_meta);
    let selected =
        crate::core::cloud_project::route(&connector, &guard, &session.access_token, identity)?;
    let mut req_meta = CloudResponseMetadata::default();
    let response = connector
        .account_request_with_metadata(
            AccountOperation::Status,
            &serde_json::json!({
                "version": 1,
                "project_id": selected.canonical_name,
                "operation": "resource_limits",
            }),
            &session.access_token,
            None,
            None,
            &mut req_meta,
        )
        .map_err(transport_error)?;
    warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
        &req_meta,
    ));
    let resources = response
        .get("resource_limits")
        .filter(|value| response["version"] == 1 && value.is_object())
        .ok_or_else(|| {
            diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "The service did not return a resource-limit summary.",
                "Check the selected project's usage and spend cap in Cloud Admin.",
            )
        })?;
    let mut result = local_status(project_root, identity, &origin, state);
    result["resource_limits"] = resources.clone();
    if let Some(spend_status) = resources
        .pointer("/paid_overages/status")
        .and_then(|v| v.as_str())
    {
        result["spend_cap_status"] = serde_json::json!(spend_status);
        result["spend_cap_warning"] = serde_json::json!(matches!(
            spend_status,
            "warning" | "overage_cap_exhausted" | "exhausted" | "cap_reached"
        ));
    }
    if !warnings.is_empty() {
        result["warnings"] = serde_json::json!(warnings);
    }
    Ok(result)
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn status_live(
    _project_root: &Path,
    _identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    selected_origin()?;
    unreachable!("cloud origin is unavailable without the supabase-cloud feature")
}

pub fn devices(
    project_root: &Path,
    identity: &RepositoryIdentity,
    live: bool,
) -> Result<serde_json::Value, DecapodError> {
    let origin = selected_origin()?;
    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let guard = store.lock(LOCK_TIMEOUT)?;
    let state = guard.state();
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        &origin,
        &identity.canonical_name,
        project_root,
    )?;
    let local_device = key_store.inspect(&origin, &identity.canonical_name)?;
    let mut devices_list = Vec::new();
    if let Some(entry) = &local_device {
        devices_list.push(serde_json::json!(entry));
    }
    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut server_readiness = "not_checked";
    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut warnings: Vec<crate::core::propodus::CloudMetadataWarning> = Vec::new();
    #[cfg(feature = "supabase-cloud")]
    if live {
        use dactyl_db::{AccountOperation, CloudConnector, CloudResponseMetadata};

        let session = state
            .session
            .as_ref()
            .filter(|session| {
                matches!(state.phase, Phase::Ready | Phase::CheckingReadiness)
                    && session.expires_at > now()
            })
            .ok_or_else(|| {
                diagnostic(
                    CloudAuthStatus::AuthRequired,
                    "Live cloud device inspection requires a current managed session.",
                    "Run `decapod cloud login` to establish a session, then repeat `decapod cloud devices --live`.",
                )
            })?;
        let (mut connector, discover_meta) =
            CloudConnector::discover_with_metadata(&origin).map_err(transport_error)?;
        connector = connector.with_device_gateway();
        warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
            &discover_meta,
        ));
        let selected =
            crate::core::cloud_project::route(&connector, &guard, &session.access_token, identity)?;
        let mut req_meta = CloudResponseMetadata::default();
        let response = connector
            .account_request_with_metadata(
                AccountOperation::Status,
                &serde_json::json!({
                    "version": 1,
                    "project_id": selected.canonical_name,
                }),
                &session.access_token,
                None,
                None,
                &mut req_meta,
            )
            .map_err(transport_error)?;
        warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
            &req_meta,
        ));
        if response.get("backend_authorized").and_then(|v| v.as_bool()) == Some(true) {
            server_readiness = "checked";
        }
    }
    #[cfg(not(feature = "supabase-cloud"))]
    if live {
        let _ = state;
        unreachable!("cloud origin is unavailable without the supabase-cloud feature");
    }
    Ok(serde_json::json!({
        "schema_version": "decapod.cloud.devices.v1",
        "repository": identity.canonical_name,
        "phase": state.phase,
        "credential_present": state.session.is_some(),
        "keystore_policy": crate::core::cloud_dpop::keystore_policy_label(),
        "os_keychain_available": crate::core::cloud_dpop::os_keychain_available(),
        "server_readiness": server_readiness,
        "devices": devices_list,
        "warnings": warnings,
    }))
}

pub fn logout(
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    let origin = selected_origin()?;
    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let mut guard = store.lock(LOCK_TIMEOUT)?;
    let had_session = guard.state().session.is_some();
    guard.clear_for_restart()?;
    Ok(serde_json::json!({
        "schema_version": "decapod.cloud.logout.v1",
        "repository": identity.canonical_name,
        "session_cleared": had_session,
        "status": "logged_out",
    }))
}

#[cfg(feature = "supabase-cloud")]
enum DpopStepError {
    Dactyl(dactyl_db::DactylError),
    Decapod(DecapodError),
}

#[cfg(feature = "supabase-cloud")]
fn default_device_label() -> String {
    std::env::var("HOSTNAME")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "decapod-cli".to_string())
}

#[cfg(feature = "supabase-cloud")]
fn parse_device_session_response(response: &serde_json::Value) -> Result<Session, DecapodError> {
    let access_token = response
        .get("access_token")
        .and_then(|v| v.as_str())
        .filter(|t| crate::core::cloud_dpop::is_dpop_access_token(t))
        .ok_or_else(|| {
            diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "The service returned an invalid or missing DPoP device access token.",
                "Retry the command after verifying the device session endpoint.",
            )
        })?
        .to_owned();
    let refresh_token = match response.get("refresh_token").and_then(|v| v.as_str()) {
        Some(rt) => {
            if !crate::core::cloud_dpop::is_dpop_refresh_token(rt) {
                return Err(diagnostic(
                    CloudAuthStatus::ProviderUnavailable,
                    "The service returned an invalid DPoP device refresh token.",
                    "Retry the command after verifying the device session endpoint.",
                ));
            }
            Some(rt.to_owned())
        }
        None => None,
    };
    let current = now();
    let expires_at = if let Some(ts) = response.get("expires_at_unix").and_then(|v| v.as_i64()) {
        ts
    } else if let Some(ts) = response.get("expires_at").and_then(|v| v.as_i64()) {
        ts
    } else if let Some(ts_str) = response.get("expires_at").and_then(|v| v.as_str()) {
        crate::core::time::parse_rfc3339(ts_str)
            .map_err(|_| {
                diagnostic(
                    CloudAuthStatus::ProviderUnavailable,
                    "The service returned an invalid device session expiry timestamp.",
                    RESTART,
                )
            })?
            .unix_timestamp()
    } else {
        let expires_in = response
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(900);
        if !(1..=86_400).contains(&expires_in) {
            return Err(diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "The service returned an invalid device session lifetime.",
                RESTART,
            ));
        }
        current.saturating_add(expires_in)
    };
    if expires_at <= current {
        return Err(diagnostic(
            CloudAuthStatus::Expired,
            "The service returned an already-expired device session.",
            RESTART,
        ));
    }
    Ok(Session {
        access_token,
        refresh_token,
        expires_at,
    })
}

#[cfg(feature = "supabase-cloud")]
fn enroll_device_raw(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    project_root: &Path,
    identity: &RepositoryIdentity,
    label: &str,
) -> Result<serde_json::Value, DpopStepError> {
    let challenge = connector
        .clone()
        .with_device_gateway()
        .account_request(
            dactyl_db::AccountOperation::DeviceChallenge,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(DpopStepError::Dactyl)?;
    let nonce = challenge
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            DpopStepError::Decapod(diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "Device enrollment challenge response omitted required nonce.",
                "Retry device enrollment after the service is healthy.",
            ))
        })?;
    let audience = challenge
        .get("audience")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| connector.origin());
    let challenge_id = challenge.get("challenge_id").and_then(|v| v.as_str());

    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        connector.origin(),
        &identity.canonical_name,
        project_root,
    )
    .map_err(DpopStepError::Decapod)?;
    let key = std::sync::Arc::new(key_store.load_or_create().map_err(DpopStepError::Decapod)?);
    let signer = std::sync::Arc::new(
        crate::core::cloud_dpop::DeviceDpopSigner::from_arc_with_audience(
            std::sync::Arc::clone(&key),
            audience,
        ),
    );
    dactyl_db::RequestProofSigner::observe_nonce(signer.as_ref(), "", nonce);

    let mut enroll_body = serde_json::json!({
        "version": 1,
        "nonce": nonce,
        "label": label,
    });
    if let Some(cid) = challenge_id {
        enroll_body["challenge_id"] = serde_json::json!(cid);
    } else {
        enroll_body["project_id"] = serde_json::json!(identity.canonical_name);
    }
    let response = connector
        .clone()
        .with_device_gateway()
        .with_proof_signer(signer)
        .account_request(
            dactyl_db::AccountOperation::DeviceEnroll,
            &enroll_body,
            bearer,
            None,
        )
        .map_err(DpopStepError::Dactyl)?;
    if let Some(device_key_id) = response
        .get("device_key_id")
        .or_else(|| response.pointer("/device_key/id"))
        .and_then(|v| v.as_str())
    {
        let _ = key_store.record_device_key_id(&key.jkt, device_key_id);
    }
    Ok(response)
}

#[cfg(feature = "supabase-cloud")]
fn exchange_device_session_raw(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    key: std::sync::Arc<crate::core::cloud_dpop::DeviceKeyMaterial>,
    identity: &RepositoryIdentity,
) -> Result<(Session, Option<String>), DpopStepError> {
    let signer = std::sync::Arc::new(crate::core::cloud_dpop::DeviceDpopSigner::from_arc(key));
    let response = connector
        .clone()
        .with_device_gateway()
        .with_proof_signer(signer)
        .account_request(
            dactyl_db::AccountOperation::DeviceSession,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(DpopStepError::Dactyl)?;
    let session = parse_device_session_response(&response).map_err(DpopStepError::Decapod)?;
    let device_key_id = response
        .get("device_key_id")
        .or_else(|| response.pointer("/device_key/id"))
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);
    Ok((session, device_key_id))
}

#[cfg(feature = "supabase-cloud")]
fn refresh_device_session_with_connector(
    connector: &dactyl_db::CloudConnector,
    project_root: &Path,
    identity: &RepositoryIdentity,
    current_access_token: &str,
    refresh_token: &str,
) -> Result<Session, DpopStepError> {
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        connector.origin(),
        &identity.canonical_name,
        project_root,
    )
    .map_err(DpopStepError::Decapod)?;
    let key = key_store
        .load()
        .map_err(DpopStepError::Decapod)?
        .ok_or_else(|| {
            DpopStepError::Decapod(diagnostic(
                CloudAuthStatus::KeyLost,
                "Refreshing a DPoP device session requires the enrolled local device key.",
                "Run `decapod cloud revoke --local-only`, then `decapod cloud login --restart`.",
            ))
        })?;
    let signer = std::sync::Arc::new(crate::core::cloud_dpop::DeviceDpopSigner::new(key));
    let response = connector
        .clone()
        .with_device_gateway()
        .with_proof_signer(signer)
        .account_request(
            dactyl_db::AccountOperation::DeviceRefresh,
            &serde_json::json!({
                "version": 1,
                "refresh_token": refresh_token,
                "project_id": identity.canonical_name,
            }),
            current_access_token,
            None,
        )
        .map_err(DpopStepError::Dactyl)?;
    parse_device_session_response(&response).map_err(DpopStepError::Decapod)
}

#[cfg(feature = "supabase-cloud")]
fn resolve_operational_session(
    connector: &dactyl_db::CloudConnector,
    guard: &mut SessionGuard,
    project_root: &Path,
    human_session: &Session,
    routed: &RepositoryIdentity,
) -> Result<(dactyl_db::CloudConnector, Session), DecapodError> {
    if std::env::var("DECAPOD_CONTAINER_SIGNER_SOCKET")
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
    {
        let active = guard
            .device_session(&routed.canonical_name)?
            .unwrap_or_else(|| human_session.clone());
        let mut active_connector = connector.clone();
        if crate::core::cloud_dpop::is_dpop_access_token(&active.access_token) {
            active_connector = active_connector.with_device_gateway();
        }
        if let Some(signer) = crate::core::cloud_dpop::request_proof_signer_for_bearer(
            connector.origin(),
            &routed.canonical_name,
            project_root,
            &active.access_token,
        ) {
            active_connector = active_connector
                .with_device_gateway()
                .with_proof_signer(signer);
        }
        return Ok((active_connector, active));
    }
    if crate::core::cloud_dpop::cloud_auth_scheme_override() == Some("bearer")
        && !crate::core::cloud_dpop::is_dpop_access_token(&human_session.access_token)
    {
        return Ok((connector.clone(), human_session.clone()));
    }
    if crate::core::cloud_dpop::is_dpop_access_token(&human_session.access_token) {
        let signer = crate::core::cloud_dpop::request_proof_signer_for_bearer(
            connector.origin(),
            &routed.canonical_name,
            project_root,
            &human_session.access_token,
        )
        .ok_or_else(|| {
            diagnostic(
                CloudAuthStatus::KeyLost,
                "Cloud DPoP session requires an enrolled local device key to sign requests.",
                "Run `decapod cloud revoke --local-only`, then `decapod cloud login --restart`.",
            )
        })?;
        return Ok((
            connector
                .clone()
                .with_device_gateway()
                .with_proof_signer(signer),
            human_session.clone(),
        ));
    }

    let strict_dpop = crate::core::cloud_dpop::cloud_auth_scheme_override() == Some("dpop");
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        connector.origin(),
        &routed.canonical_name,
        project_root,
    )?;

    if let Some(cached) = guard.device_session(&routed.canonical_name)? {
        if let Some(key) = key_store.load()? {
            if cached.expires_at > now().saturating_add(REFRESH_LEAD_SECONDS) {
                let signer =
                    std::sync::Arc::new(crate::core::cloud_dpop::DeviceDpopSigner::new(key));
                return Ok((
                    connector
                        .clone()
                        .with_device_gateway()
                        .with_proof_signer(signer),
                    cached,
                ));
            }
            if let Some(rt) = cached
                .refresh_token
                .as_deref()
                .filter(|t| crate::core::cloud_dpop::is_dpop_refresh_token(t))
            {
                match refresh_device_session_with_connector(
                    connector,
                    project_root,
                    routed,
                    &cached.access_token,
                    rt,
                ) {
                    Ok(refreshed) => {
                        guard.replace_device_session(&routed.canonical_name, refreshed.clone())?;
                        let signer = std::sync::Arc::new(
                            crate::core::cloud_dpop::DeviceDpopSigner::new(key),
                        );
                        return Ok((
                            connector
                                .clone()
                                .with_device_gateway()
                                .with_proof_signer(signer),
                            refreshed,
                        ));
                    }
                    Err(DpopStepError::Dactyl(err)) => {
                        if matches!(
                            err.adapter_code(),
                            Some(
                                "refresh_reuse_detected" | "device_revoked" | "device_key_expired"
                            )
                        ) {
                            guard.clear_device_session()?;
                            return Err(transport_error_for_repository(
                                err,
                                Some(&routed.canonical_name),
                            ));
                        }
                        if matches!(
                            err.adapter_code(),
                            Some(
                                "invalid_token"
                                    | "session_expired"
                                    | "session_revoked"
                                    | "session_not_found"
                                    | "invalid_grant"
                                    | "refresh_token_not_found"
                            )
                        ) {
                            guard.clear_device_session()?;
                        } else {
                            return Err(transport_error_for_repository(
                                err,
                                Some(&routed.canonical_name),
                            ));
                        }
                    }
                    Err(DpopStepError::Decapod(err)) => return Err(err),
                }
            } else {
                guard.clear_device_session()?;
            }
        } else {
            guard.clear_device_session()?;
        }
    }

    if let Some(existing_key) = key_store.load()? {
        let existing_key = std::sync::Arc::new(existing_key);
        match exchange_device_session_raw(
            connector,
            &human_session.access_token,
            std::sync::Arc::clone(&existing_key),
            routed,
        ) {
            Ok((dpop_session, key_id)) => {
                if let Some(id) = key_id.as_deref() {
                    let _ = key_store.record_device_key_id(&existing_key.jkt, id);
                }
                guard.replace_device_session(&routed.canonical_name, dpop_session.clone())?;
                let signer = std::sync::Arc::new(
                    crate::core::cloud_dpop::DeviceDpopSigner::from_arc(existing_key),
                );
                return Ok((
                    connector
                        .clone()
                        .with_device_gateway()
                        .with_proof_signer(signer),
                    dpop_session,
                ));
            }
            Err(DpopStepError::Dactyl(err)) => {
                if err.adapter_kind() == Some(dactyl_db::AdapterErrorKind::NotFound) && !strict_dpop
                {
                    return Ok((connector.clone(), human_session.clone()));
                }
                if !matches!(
                    err.adapter_code(),
                    Some("device_not_enrolled" | "device_key_not_found" | "invalid_dpop_proof")
                ) {
                    return Err(transport_error_for_repository(
                        err,
                        Some(&routed.canonical_name),
                    ));
                }
            }
            Err(DpopStepError::Decapod(err)) => return Err(err),
        }
    }

    match enroll_device_raw(
        connector,
        &human_session.access_token,
        project_root,
        routed,
        &default_device_label(),
    ) {
        Ok(_) => (),
        Err(DpopStepError::Dactyl(err)) => {
            if err.adapter_kind() == Some(dactyl_db::AdapterErrorKind::NotFound) && !strict_dpop {
                return Ok((connector.clone(), human_session.clone()));
            }
            return Err(transport_error_for_repository(
                err,
                Some(&routed.canonical_name),
            ));
        }
        Err(DpopStepError::Decapod(err)) => return Err(err),
    }

    let enrolled_key = std::sync::Arc::new(key_store.load()?.ok_or_else(|| {
        diagnostic(
            CloudAuthStatus::KeyLost,
            "Enrolled local device key could not be loaded after device registration.",
            "Run `decapod cloud revoke --local-only`, then `decapod cloud login --restart`.",
        )
    })?);
    let (dpop_session, key_id) = exchange_device_session_raw(
        connector,
        &human_session.access_token,
        std::sync::Arc::clone(&enrolled_key),
        routed,
    )
    .map_err(|err| match err {
        DpopStepError::Dactyl(error) => {
            transport_error_for_repository(error, Some(&routed.canonical_name))
        }
        DpopStepError::Decapod(error) => error,
    })?;
    if let Some(id) = key_id.as_deref() {
        let _ = key_store.record_device_key_id(&enrolled_key.jkt, id);
    }
    guard.replace_device_session(&routed.canonical_name, dpop_session.clone())?;
    let signer = std::sync::Arc::new(crate::core::cloud_dpop::DeviceDpopSigner::from_arc(
        enrolled_key,
    ));
    Ok((
        connector
            .clone()
            .with_device_gateway()
            .with_proof_signer(signer),
        dpop_session,
    ))
}

/// Inspect enrolled server-side device keys via `dactyl-db 0.19.0`
/// `AccountOperation::DeviceKeys` (`#133`).
#[cfg(feature = "supabase-cloud")]
pub fn list_device_keys_with_connector(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    connector
        .clone()
        .with_device_gateway()
        .account_request(
            dactyl_db::AccountOperation::DeviceKeys,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(|error| transport_error_for_repository(error, Some(&identity.canonical_name)))
}

/// Inspect active server-side DPoP device sessions via `dactyl-db 0.19.0`
/// `AccountOperation::DeviceSessions` (`#133`).
#[cfg(feature = "supabase-cloud")]
pub fn list_device_sessions_with_connector(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    connector
        .clone()
        .with_device_gateway()
        .account_request(
            dactyl_db::AccountOperation::DeviceSessions,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(|error| transport_error_for_repository(error, Some(&identity.canonical_name)))
}

/// Acquire an authenticated `CloudConnector`, routed repository identity, and
/// current bearer token for control-plane RPC calls (`propodus_repository_access_v1`,
/// `propodus_proof_acceptance_v1`, `propodus_proof_acceptance_get_v1`).
#[cfg(feature = "supabase-cloud")]
pub fn connect_control(
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<(dactyl_db::CloudConnector, RepositoryIdentity, String), DecapodError> {
    let (connector, mut guard) = acquire(project_root, identity, false)?;
    let state = guard.state().clone();
    let human_session = state.session.as_ref().ok_or_else(restart_required)?.clone();
    let control_connector = connector.clone().with_device_gateway();
    let result = crate::core::cloud_project::route(
        &control_connector,
        &guard,
        &human_session.access_token,
        identity,
    );
    let routed = account_result(&mut guard, result)?;
    let (active_connector, active_session) = resolve_operational_session(
        &connector,
        &mut guard,
        project_root,
        &human_session,
        &routed,
    )?;
    Ok((active_connector, routed, active_session.access_token))
}

/// Perform challenge-response Ed25519 device key enrollment via `dactyl-db 0.19.0`
/// `AccountOperation::{DeviceChallenge, DeviceEnroll}` (`#1778`, `#1878`).
#[cfg(feature = "supabase-cloud")]
pub fn enroll_device_with_connector(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    project_root: &Path,
    identity: &RepositoryIdentity,
    label: &str,
) -> Result<serde_json::Value, DecapodError> {
    enroll_device_raw(connector, bearer, project_root, identity, label).map_err(|err| match err {
        DpopStepError::Dactyl(error) => {
            transport_error_for_repository(error, Some(&identity.canonical_name))
        }
        DpopStepError::Decapod(error) => error,
    })
}

/// Request an account-level AAL2 MFA step-up challenge via `dactyl-db 0.19.0`
/// `AccountOperation::DeviceStepUp` (`#1778`).
#[cfg(feature = "supabase-cloud")]
pub fn request_mfa_step_up(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    project_root: &Path,
    identity: &RepositoryIdentity,
) -> Result<serde_json::Value, DecapodError> {
    let _ = project_root;
    connector
        .clone()
        .with_device_gateway()
        .account_request(
            dactyl_db::AccountOperation::DeviceStepUp,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(|error| transport_error_for_repository(error, Some(&identity.canonical_name)))
}

pub fn revoke(
    project_root: &Path,
    identity: &RepositoryIdentity,
    all: bool,
    device_id: Option<&str>,
    session_logout: bool,
    local_only: bool,
) -> Result<serde_json::Value, DecapodError> {
    if local_only && session_logout {
        return Err(DecapodError::ValidationError(
            "CONFLICTING_FLAGS: --local-only cannot be combined with --session-logout".into(),
        ));
    }
    let origin = selected_origin()?;
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        &origin,
        &identity.canonical_name,
        project_root,
    )?;
    let existing_key = key_store.load()?;
    let existing_jkt = existing_key.as_ref().map(|k| k.jkt.clone());
    let stored_device_key_id = existing_jkt
        .as_deref()
        .and_then(|jkt| key_store.registered_device_key_id(jkt));
    let requested_device_id = device_id.map(str::trim).filter(|s| !s.is_empty());
    let matches_local = match requested_device_id {
        None => true,
        Some(target) => {
            existing_jkt.as_deref() == Some(target)
                || stored_device_key_id.as_deref() == Some(target)
        }
    };

    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let mut guard = store.lock(LOCK_TIMEOUT)?;
    let sidecar_dpop = {
        #[cfg(feature = "supabase-cloud")]
        {
            guard.device_session(&identity.canonical_name)?
        }
        #[cfg(not(feature = "supabase-cloud"))]
        {
            None::<crate::core::cloud_session::Session>
        }
    };
    let has_session = guard.state().session.is_some() || sidecar_dpop.is_some();

    if let Some(target) = requested_device_id
        && !matches_local
        && (local_only || session_logout || !has_session)
        && existing_jkt.is_some()
    {
        return Err(DecapodError::ValidationError(format!(
            "DEVICE_KEY_NOT_FOUND: requested device key '{}' does not match enrolled device '{}'",
            target,
            existing_jkt.as_deref().unwrap_or_default()
        )));
    }

    if local_only {
        key_store.delete()?;
        key_store.clear_registered_device_key_id();
        guard.clear_for_restart()?;
        return Ok(serde_json::json!({
            "schema_version": "decapod.cloud.revoke.v1",
            "repository": identity.canonical_name,
            "operation": "local_only",
            "local_only": true,
            "all_scopes": all,
            "revoked_device_jkt": existing_jkt,
            "revoked_device_key_id": stored_device_key_id,
            "remote_revoked": false,
            "status": "revoked",
        }));
    }

    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut remote_revoked = false;
    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut operation = "local_orphan_cleanup";
    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut warnings: Vec<crate::core::propodus::CloudMetadataWarning> = Vec::new();
    #[cfg_attr(not(feature = "supabase-cloud"), allow(unused_mut))]
    let mut revoked_device_key_id = stored_device_key_id.clone();

    if session_logout && !has_session {
        return Err(diagnostic(
            CloudAuthStatus::AuthRequired,
            "Session logout requires an active saved cloud session.",
            "Run `decapod cloud login` first, or use `decapod cloud revoke --local-only` to clear local device state.",
        ));
    }

    #[cfg(feature = "supabase-cloud")]
    if guard.state().session.is_some() || sidecar_dpop.is_some() {
        use dactyl_db::{AccountOperation, CloudResponseMetadata};

        let state_session = guard.state().session.clone();
        let is_dpop_session_token = state_session
            .as_ref()
            .is_some_and(|session| session.access_token.starts_with("dpc_at_"));
        let use_session_logout = session_logout
            || ((is_dpop_session_token || (sidecar_dpop.is_some() && state_session.is_none()))
                && !all
                && requested_device_id.is_none());

        let (mut connector, discover_meta) =
            dactyl_db::CloudConnector::discover_with_metadata(&origin).map_err(transport_error)?;
        warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
            &discover_meta,
        ));

        let (bearer_for_revoke, payload) = if use_session_logout {
            operation = "logout";
            let logout_session = sidecar_dpop
                .clone()
                .filter(|s| crate::core::cloud_dpop::is_dpop_access_token(&s.access_token))
                .or_else(|| {
                    state_session
                        .clone()
                        .filter(|s| crate::core::cloud_dpop::is_dpop_access_token(&s.access_token))
                })
                .or(state_session)
                .ok_or_else(|| {
                    diagnostic(
                        CloudAuthStatus::AuthRequired,
                        "Session logout requires an active saved cloud session.",
                        "Run `decapod cloud login` first, or use `decapod cloud revoke --local-only` to clear local device state.",
                    )
                })?;
            let signer = crate::core::cloud_dpop::request_proof_signer_for_bearer(
                connector.origin(),
                &identity.canonical_name,
                project_root,
                &logout_session.access_token,
            )
            .or_else(|| {
                key_store.load().ok().flatten().map(|key| {
                    std::sync::Arc::new(crate::core::cloud_dpop::DeviceDpopSigner::new(key))
                        as std::sync::Arc<dyn dactyl_db::RequestProofSigner>
                })
            })
            .ok_or_else(|| {
                diagnostic(
                    CloudAuthStatus::KeyLost,
                    "DPoP session logout requires an enrolled local device key to sign the proof.",
                    "Use `decapod cloud revoke --local-only` to clear local state, then run `decapod cloud login --restart`.",
                )
            })?;
            connector = connector.with_device_gateway().with_proof_signer(signer);
            (
                logout_session.access_token,
                serde_json::json!({
                    "version": 1,
                    "project_id": identity.canonical_name,
                    "action": "logout",
                }),
            )
        } else if all {
            let human_session = state_session.ok_or_else(|| {
                diagnostic(
                    CloudAuthStatus::AuthRequired,
                    "Revoking all device keys requires a human managed session.",
                    "Run `decapod cloud login` first, or use `--session-logout` / `--local-only`.",
                )
            })?;
            connector = connector.with_device_gateway();
            operation = "revoke_all";
            (
                human_session.access_token,
                serde_json::json!({
                    "version": 1,
                    "project_id": identity.canonical_name,
                    "action": "revoke_all",
                    "revoke_all": true,
                }),
            )
        } else {
            let human_session = state_session.ok_or_else(|| {
                diagnostic(
                    CloudAuthStatus::AuthRequired,
                    "Revoking a device key requires a human managed session.",
                    "Run `decapod cloud login` first, or use `--session-logout` / `--local-only`.",
                )
            })?;
            connector = connector.with_device_gateway();
            operation = "revoke_key";
            let target_key_id = requested_device_id
                .filter(|t| existing_jkt.as_deref() != Some(*t))
                .map(ToOwned::to_owned)
                .or_else(|| stored_device_key_id.clone());
            revoked_device_key_id = target_key_id.clone();
            let mut body = serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
                "action": "revoke_key",
                "revoke_all": false,
            });
            if let Some(key_id) = target_key_id {
                body["device_key_id"] = serde_json::json!(key_id);
            }
            if matches_local && let Some(jkt) = existing_jkt.as_deref() {
                body["jwk_thumbprint"] = serde_json::json!(jkt);
            }
            (human_session.access_token, body)
        };

        let mut req_meta = CloudResponseMetadata::default();
        // Fail closed without deleting local key/session if remote revocation fails,
        // so the command remains safely retryable.
        connector
            .account_request_with_metadata(
                AccountOperation::DeviceRevoke,
                &payload,
                &bearer_for_revoke,
                None,
                None,
                &mut req_meta,
            )
            .map_err(transport_error)?;
        warnings.extend(crate::core::propodus::warnings_from_cloud_metadata(
            &req_meta,
        ));
        remote_revoked = true;
    }

    if operation == "logout" {
        guard.clear_for_restart()?;
        return Ok(serde_json::json!({
            "schema_version": "decapod.cloud.revoke.v1",
            "repository": identity.canonical_name,
            "operation": operation,
            "local_only": false,
            "all_scopes": false,
            "device_key_preserved": existing_jkt.is_some(),
            "revoked_device_jkt": existing_jkt,
            "remote_revoked": remote_revoked,
            "warnings": warnings,
            "status": "revoked",
        }));
    }

    if all || matches_local {
        key_store.delete()?;
        key_store.clear_registered_device_key_id();
    }
    if all || requested_device_id.is_none() || matches_local {
        guard.clear_for_restart()?;
    }
    Ok(serde_json::json!({
        "schema_version": "decapod.cloud.revoke.v1",
        "repository": identity.canonical_name,
        "operation": operation,
        "local_only": false,
        "all_scopes": all,
        "revoked_device_jkt": if matches_local { existing_jkt } else { None },
        "revoked_device_key_id": revoked_device_key_id,
        "remote_revoked": remote_revoked,
        "warnings": warnings,
        "status": "revoked",
    }))
}

#[cfg(all(test, feature = "supabase-cloud"))]
mod cloud_admin_client_tests {
    use super::*;
    use dactyl_db::{AdapterErrorKind, DactylError};

    fn fake_jwt(payload_json: serde_json::Value) -> String {
        let header = crate::core::cloud_dpop::base64url_encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload =
            crate::core::cloud_dpop::base64url_encode(payload_json.to_string().as_bytes());
        format!("{header}.{payload}.sig")
    }

    #[test]
    fn admin_transport_error_distinguishes_outcome_unknown_between_mutations_and_reads() {
        let err = DactylError::Adapter {
            kind: AdapterErrorKind::Timeout,
            code: None,
            message: "remote state uncertain: timed out".into(),
        };
        let mut_err = admin_transport_error(err, Some("owner/repo"), true, true, Some("dpc_at_x"));
        match mut_err {
            DecapodError::CloudAuth(diag) => {
                assert_eq!(diag.status, CloudAuthStatus::OutcomeUnknown);
                assert_eq!(diag.error_code_str(), Some("outcome_unknown"));
            }
            other => panic!("expected CloudAuth OutcomeUnknown, got {other:?}"),
        }

        let read_transport = DactylError::Adapter {
            kind: AdapterErrorKind::Timeout,
            code: None,
            message: "cloud authentication timed out".into(),
        };
        let read_err =
            admin_transport_error(read_transport, Some("owner/repo"), false, false, None);
        match read_err {
            DecapodError::CloudAuth(diag) => {
                assert_eq!(diag.status, CloudAuthStatus::Offline);
            }
            other => panic!("expected CloudAuth Offline for read, got {other:?}"),
        }
    }

    #[test]
    fn admin_transport_error_surfaces_mfa_step_up_vs_admin_required_on_aal2_ops() {
        let aal1_jwt = fake_jwt(serde_json::json!({ "sub": "u1", "aal": "aal1" }));
        let err_403 = DactylError::Adapter {
            kind: AdapterErrorKind::Authorization,
            code: None,
            message: "cloud authentication request was rejected".into(),
        };
        let mfa_diag =
            admin_transport_error(err_403, Some("owner/repo"), true, true, Some(&aal1_jwt));
        match mfa_diag {
            DecapodError::CloudAuth(diag) => {
                assert_eq!(diag.status, CloudAuthStatus::MfaRequired);
                assert_eq!(diag.error_code_str(), Some("mfa_step_up_required"));
            }
            other => panic!("expected MfaRequired, got {other:?}"),
        }

        let aal2_jwt = fake_jwt(serde_json::json!({ "sub": "u1", "aal": "aal2" }));
        let err_403_admin = DactylError::Adapter {
            kind: AdapterErrorKind::Authorization,
            code: None,
            message: "cloud authentication request was rejected".into(),
        };
        let admin_diag = admin_transport_error(
            err_403_admin,
            Some("owner/repo"),
            true,
            true,
            Some(&aal2_jwt),
        );
        match admin_diag {
            DecapodError::CloudAuth(diag) => {
                assert_eq!(diag.status, CloudAuthStatus::Unauthorized);
                assert_eq!(diag.error_code_str(), Some("admin_required"));
            }
            other => panic!("expected Unauthorized admin_required, got {other:?}"),
        }
    }

    #[test]
    fn diagnostic_for_status_code_maps_admin_and_mfa_codes() {
        let mfa = diagnostic_for_status_code("mfa_step_up_required", None).unwrap();
        assert_eq!(mfa.status, CloudAuthStatus::MfaRequired);
        assert_eq!(mfa.error_code_str(), Some("mfa_step_up_required"));

        let owner = diagnostic_for_status_code("owner_required", None).unwrap();
        assert_eq!(owner.status, CloudAuthStatus::Unauthorized);
        assert_eq!(owner.error_code_str(), Some("owner_required"));

        let quar = diagnostic_for_status_code("provider_quarantined", None).unwrap();
        assert_eq!(quar.status, CloudAuthStatus::ActionRequired);
        assert_eq!(quar.error_code_str(), Some("provider_quarantined"));
    }

    #[test]
    fn dactyl_019_problem_codes_and_staged_first_party_origin_helpers() {
        assert_eq!(FIRST_PARTY_CLOUD_ORIGIN, "https://api.decapodlabs.ai");
        assert_eq!(FIRST_PARTY_AUTH_PATH, "/auth");
        assert_eq!(FIRST_PARTY_REPO_PATH_PREFIX, "/decapod");

        let cfg =
            dactyl_db::SupabaseRpcConfig::v1(FIRST_PARTY_CLOUD_ORIGIN, "sb_publishable_fixture");
        let connector = dactyl_db::CloudConnector::with_config(cfg).expect("connector");
        assert_eq!(
            canonical_auth_url(&connector).expect("auth url"),
            "https://api.decapodlabs.ai/auth"
        );
        let identity = RepositoryIdentity {
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            canonical_name: "DecapodLabs/decapod".to_string(),
            remote_url: "https://github.com/DecapodLabs/decapod.git".to_string(),
        };
        assert_eq!(
            canonical_repository_url(&connector, &identity).expect("repo url"),
            "https://api.decapodlabs.ai/decapod/DecapodLabs/decapod"
        );

        for (code, expected_status) in [
            ("dpop_aud_mismatch", CloudAuthStatus::AuthRequired),
            ("use_dpop_nonce", CloudAuthStatus::AuthRequired),
            ("device_key_expired", CloudAuthStatus::DeviceRevoked),
            ("device_not_enrolled", CloudAuthStatus::DeviceRevoked),
            ("refresh_reuse_detected", CloudAuthStatus::Revoked),
            ("expired_credential", CloudAuthStatus::Revoked),
            ("grant_revoked", CloudAuthStatus::RepositoryDenied),
            ("grant_expired", CloudAuthStatus::RepositoryDenied),
            ("scope_denied", CloudAuthStatus::RepositoryDenied),
            ("repo_access_denied", CloudAuthStatus::RepositoryDenied),
            ("paid_seat_required", CloudAuthStatus::EntitlementDenied),
            ("seat_capacity_exceeded", CloudAuthStatus::QuotaExceeded),
            ("tenant_upgrade_required", CloudAuthStatus::ActionRequired),
            ("tenant_upgrading", CloudAuthStatus::PendingFulfillment),
            ("tenant_suspended", CloudAuthStatus::Unauthorized),
            ("tenant_archived", CloudAuthStatus::RepositoryDenied),
            ("org_recovery_required", CloudAuthStatus::ActionRequired),
            ("policy_template_conflict", CloudAuthStatus::ActionRequired),
            (
                "provenance_origin_mismatch",
                CloudAuthStatus::ProviderUnavailable,
            ),
            ("idempotency_conflict", CloudAuthStatus::ActionRequired),
            ("idempotency_in_progress", CloudAuthStatus::ActionRequired),
            ("version_conflict", CloudAuthStatus::ActionRequired),
            ("transaction_aborted", CloudAuthStatus::ProviderUnavailable),
        ] {
            let diag = diagnostic_for_status_code(code, Some("DecapodLabs/decapod"))
                .unwrap_or_else(|| panic!("missing diagnostic for `{code}`"));
            assert_eq!(diag.status, expected_status, "wrong status for `{code}`");
            assert_eq!(diag.error_code_str(), Some(code), "wrong code for `{code}`");
        }
    }
}

//! Repository-bound cloud session orchestration. Dactyl owns every network hop;
//! this module owns private custody and decides which operations may be retried.

use crate::core::backend::StorageContext;
#[cfg(feature = "supabase-cloud")]
use crate::core::backend::{BackendRoute, CloudDatastore};
use crate::core::cloud_session::SessionState;
use crate::core::cloud_session::SessionStore;
#[cfg(feature = "supabase-cloud")]
use crate::core::cloud_session::{Bootstrap, Phase, Session, SessionGuard};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::repo_identity::RepositoryIdentity;
use std::path::Path;
use std::time::Duration;

const LOCK_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(feature = "supabase-cloud")]
const REFRESH_LEAD_SECONDS: i64 = 60;
const RESTART: &str = "Agent: preserve the interrupted task and operation ID, then run `decapod cloud login --restart` to start a fresh sign-in without replaying the interrupted exchange. Share its browser link with the user, resume the saved flow, and retry the original task. The user does not need another CLI command.";

fn diagnostic(
    status: CloudAuthStatus,
    message: &str,
    next_action: impl Into<String>,
) -> DecapodError {
    DecapodError::CloudAuth(CloudAuthDiagnostic::new(status, message, next_action))
}

fn restart_required() -> DecapodError {
    diagnostic(
        CloudAuthStatus::RestartRequired,
        "A prior sign-in or refresh has an uncertain outcome. Its one-time request was not repeated.",
        RESTART,
    )
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
    Ok(dactyl_db::CloudConnector::default_origin())
}

#[cfg(not(feature = "supabase-cloud"))]
pub fn selected_origin() -> Result<String, DecapodError> {
    Err(DecapodError::Config(
        "Cloud requires the supabase-cloud capability; no local fallback is permitted".into(),
    ))
}

pub fn transport_error(error: dactyl_db::DactylError) -> DecapodError {
    use dactyl_db::AdapterErrorKind;
    let code = error.adapter_code();
    let (status, message, action) = match code {
        Some("mfa_step_up_required") => (
            CloudAuthStatus::MfaRequired,
            "Account-level AAL2 MFA step-up is required before device enrollment or privileged custody.",
            "Complete account MFA verification in the browser, then retry the command.",
        ),
        Some("payment_pending") => (
            CloudAuthStatus::PaymentPending,
            "Payment confirmation is still pending for this repository entitlement.",
            "Wait for billing settlement or ask the account owner to complete payment, then retry.",
        ),
        Some("quota_exceeded") => (
            CloudAuthStatus::QuotaExceeded,
            "The cloud service declined the request because the included quota is exhausted.",
            "Ask a billing administrator to review the plan or eligible paid overages in Cloud Admin before trying again.",
        ),
        Some("overage_cap_exhausted") => (
            CloudAuthStatus::OverageCapExhausted,
            "The cloud service declined new billable usage because the spend cap was reached.",
            "Ask a billing administrator to review the current spend cap in Cloud Admin. Already admitted tasks can still use supported terminal operations.",
        ),
        Some("provider_disabled") => (
            CloudAuthStatus::ProviderDisabled,
            "This sign-in provider is disabled for the cloud deployment.",
            "Choose an enabled provider on the Cloud Admin sign-in page.",
        ),
        Some("entitlement_denied") => (
            CloudAuthStatus::EntitlementDenied,
            "The service denied repository entitlement for this account.",
            "Ask the account owner to assign or restore a seat for this repository, then retry.",
        ),
        Some("device_revoked") => (
            CloudAuthStatus::DeviceRevoked,
            "This enrolled device key has been revoked on the cloud service.",
            "Run `decapod cloud revoke` to clear local device state, then `decapod cloud login --restart` to enroll a fresh device key.",
        ),
        Some(
            "entitlement_required"
            | "organization_seat_required"
            | "seat_required"
            | "seat_unassigned"
            | "account_setup_required"
            | "identity_link_required"
            | "quote_confirmation_required"
            | "billing_binding_required",
        ) => (
            CloudAuthStatus::ActionRequired,
            "The service has not confirmed the repository account entitlement.",
            "Ask the account owner to resolve the service-reported account requirement, then retry. Payment or browser completion alone does not establish access.",
        ),
        Some("authorization_denied" | "repository_not_authorized") => (
            CloudAuthStatus::RepositoryDenied,
            "The service denied access to this repository.",
            "Run `decapod cloud projects` and explicitly select the intended project with `decapod cloud project use`, or ask its administrator to review access.",
        ),
        Some("outcome_unknown") => return restart_required(),
        Some("flow_expired" | "flow_not_found" | "invalid_flow_state") => (
            CloudAuthStatus::Expired,
            "The pending sign-in can no longer be completed.",
            RESTART,
        ),
        Some("invalid_dpop_proof" | "dpop_replay") => (
            CloudAuthStatus::AuthRequired,
            "The service rejected the request DPoP proof or replay nonce.",
            "Retry the command with a fresh proof; if the device key was rotated or revoked, run `decapod cloud login --restart`.",
        ),
        Some(
            "invalid_token"
            | "session_expired"
            | "session_revoked"
            | "session_not_found"
            | "bad_jwt"
            | "invalid_grant"
            | "refresh_token_not_found"
            | "refresh_token_already_used",
        ) => (
            CloudAuthStatus::Revoked,
            "The service rejected the managed session.",
            RESTART,
        ),
        _ => match error {
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
        },
    };
    diagnostic(status, message, action)
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
fn one_time_error(error: dactyl_db::DactylError) -> DecapodError {
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
        transport_error(error)
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
    let selected = selected_origin()?;
    let mut connector = dactyl_db::CloudConnector::discover(&selected).map_err(transport_error)?;
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
            match connector.refresh_session(&refresh) {
                Ok(session) => {
                    // Persist a received rotation before any subsequent call.
                    state.session = Some(managed_session(session)?);
                    state.phase = following_phase;
                    guard.replace(state.clone())?;
                }
                Err(error) => {
                    persist_restart(&mut guard, state)?;
                    return Err(one_time_error(error));
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
                return Err(one_time_error(error));
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
            .map_err(transport_error)?;
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
                return Err(one_time_error(error));
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
                return Err(one_time_error(error));
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
                return Err(transport_error(error));
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
    Ok((connector, guard))
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
    let session = state.session.as_ref().ok_or_else(restart_required)?;
    let result =
        crate::core::cloud_project::route(&connector, &guard, &session.access_token, identity);
    let routed = account_result(&mut guard, result)?;
    let context = context(connector.origin(), &routed, session)?;
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
                        persist_restart(&mut guard, state)?;
                    }
                    transport_error(error)
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
    let bearer = guard
        .state()
        .session
        .as_ref()
        .ok_or_else(restart_required)?
        .access_token
        .clone();
    let result = crate::core::cloud_project::execute(&connector, &mut guard, &bearer, operation);
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
    use dactyl_db::{AccountOperation, CloudConnector};

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
    let mut connector = CloudConnector::discover(&origin).map_err(transport_error)?;
    if let Some(signer) = crate::core::cloud_dpop::active_request_proof_signer(
        connector.origin(),
        &identity.canonical_name,
        project_root,
    ) {
        connector = connector.with_proof_signer(signer);
    }
    let selected =
        crate::core::cloud_project::route(&connector, &guard, &session.access_token, identity)?;
    let response = connector
        .account_request(
            AccountOperation::Status,
            &serde_json::json!({
                "version": 1,
                "project_id": selected.canonical_name,
                "operation": "resource_limits",
            }),
            &session.access_token,
            None,
        )
        .map_err(transport_error)?;
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
    let session = state.session.as_ref().ok_or_else(restart_required)?;
    let result =
        crate::core::cloud_project::route(&connector, &guard, &session.access_token, identity);
    let routed = account_result(&mut guard, result)?;
    Ok((connector, routed, session.access_token.clone()))
}

/// Perform challenge-response Ed25519 device key enrollment via `dactyl-db 0.18.0`
/// `AccountOperation::{DeviceChallenge, DeviceEnroll}` (`#1778`).
#[cfg(feature = "supabase-cloud")]
pub fn enroll_device_with_connector(
    connector: &dactyl_db::CloudConnector,
    bearer: &str,
    project_root: &Path,
    identity: &RepositoryIdentity,
    label: &str,
) -> Result<serde_json::Value, DecapodError> {
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        connector.origin(),
        &identity.canonical_name,
        project_root,
    )?;
    let key = key_store.load_or_create()?;
    let challenge = connector
        .account_request(
            dactyl_db::AccountOperation::DeviceChallenge,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(transport_error)?;
    let nonce = challenge
        .get("nonce")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            diagnostic(
                CloudAuthStatus::ProviderUnavailable,
                "Device enrollment challenge response omitted required nonce.",
                "Retry device enrollment after the service is healthy.",
            )
        })?;
    let audience = challenge
        .get("audience")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| connector.origin());
    let enroll_url = format!(
        "{}/functions/v1/decapod-onboarding/device-enroll",
        connector.origin().trim_end_matches('/')
    );
    let dpop_proof = key.sign_dpop_proof(
        "POST",
        &enroll_url,
        Some(bearer),
        Some(nonce),
        Some(audience),
        None,
    )?;
    connector
        .account_request(
            dactyl_db::AccountOperation::DeviceEnroll,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
                "public_jwk": key.public_jwk,
                "jwk_thumbprint": key.jkt,
                "algorithm": "EdDSA",
                "label": label,
                "nonce": nonce,
                "dpop_proof": dpop_proof,
            }),
            bearer,
            None,
        )
        .map_err(transport_error)
}

/// Request an account-level AAL2 MFA step-up challenge via `dactyl-db 0.18.0`
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
        .account_request(
            dactyl_db::AccountOperation::DeviceStepUp,
            &serde_json::json!({
                "version": 1,
                "project_id": identity.canonical_name,
            }),
            bearer,
            None,
        )
        .map_err(transport_error)
}

pub fn revoke(
    project_root: &Path,
    identity: &RepositoryIdentity,
    all: bool,
    device_id: Option<&str>,
) -> Result<serde_json::Value, DecapodError> {
    let origin = selected_origin()?;
    let key_store = crate::core::cloud_dpop::DeviceKeyStore::for_project(
        &origin,
        &identity.canonical_name,
        project_root,
    )?;
    let existing_jkt = key_store.load()?.map(|k| k.jkt.clone());
    if let (Some(target), Some(current_jkt)) = (device_id, existing_jkt.as_deref())
        && target.trim() != current_jkt
    {
        return Err(DecapodError::ValidationError(format!(
            "DEVICE_KEY_NOT_FOUND: requested device key '{}' does not match enrolled device '{}'",
            target.trim(),
            current_jkt
        )));
    }
    let store = SessionStore::for_project(&origin, &identity.canonical_name, project_root)?;
    let mut guard = store.lock(LOCK_TIMEOUT)?;
    let mut remote_revoked = false;
    #[cfg(feature = "supabase-cloud")]
    if let Some(session) = guard.state().session.as_ref()
        && let Ok(connector) = dactyl_db::CloudConnector::discover(&origin)
    {
        let payload = serde_json::json!({
            "version": 1,
            "project_id": identity.canonical_name,
            "revoke_all": all,
            "jwk_thumbprint": existing_jkt,
        });
        if connector
            .account_request(
                dactyl_db::AccountOperation::DeviceRevoke,
                &payload,
                &session.access_token,
                None,
            )
            .is_ok()
        {
            remote_revoked = true;
        }
    }
    key_store.delete()?;
    guard.clear_for_restart()?;
    Ok(serde_json::json!({
        "schema_version": "decapod.cloud.revoke.v1",
        "repository": identity.canonical_name,
        "all_scopes": all,
        "revoked_device_jkt": existing_jkt,
        "remote_revoked": remote_revoked,
        "status": "revoked",
    }))
}

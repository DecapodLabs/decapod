//! Error types for Decapod operations.
//!
//! This module defines the canonical error type used throughout Decapod.
//! All subsystems return `Result<T, DecapodError>` for error handling.

use serde::Serialize;
use std::env;
use std::fmt;
use std::io;

/// Machine-readable outcomes for the provider-neutral cloud onboarding handoff.
/// These values are part of the CLI contract; credential material is never
/// represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthStatus {
    AuthRequired,
    Missing,
    OnboardingPending,
    /// The service requires account action; its reason is not inferred locally.
    ActionRequired,
    RegistrationRequired,
    ProviderLinkStepupRequired,
    PaymentRequired,
    PendingFulfillment,
    AdminApprovalRequired,
    OrganizationAlreadyEnrolled,
    /// A one-time request has an uncertain outcome and cannot be replayed.
    RestartRequired,
    /// A mutating cloud request failed after send with an uncertain outcome.
    OutcomeUnknown,
    Expired,
    RefreshFailed,
    Unauthorized,
    Revoked,
    DeviceRevoked,
    KeyLost,
    MfaRequired,
    PaymentPending,
    QuotaExceeded,
    OverageCapExhausted,
    EntitlementDenied,
    UnauthorizedIdentity,
    RepositoryDenied,
    Offline,
    ProviderUnavailable,
    ProviderDisabled,
    NonInteractive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudAuthErrorCode {
    MfaStepUpRequired,
    OutcomeUnknown,
    AdminRequired,
    OwnerRequired,
    ProviderQuarantined,
    QuarantinedProviderLink,
    CustodyConflict,
    OwnerProtected,
    MemberNotFound,
    InvitationExpired,
    InvitationNotFound,
    ProjectNotFound,
    SeatRevoked,
    BillingNotConfigured,
    BillingUnavailable,
    Conflict,
    NotFound,
    InvalidRequest,
    // dactyl-db 0.19.0 problem codes (#132, #133, #134, #136, #139, #140)
    UseDpopNonce,
    InvalidDpopProof,
    DpopAudMismatch,
    DpopReplay,
    DpopNonceMismatch,
    DpopClockSkew,
    DpopAthMismatch,
    DpopHtuMismatch,
    DpopHtmMismatch,
    DpopJktMismatch,
    InvalidBinding,
    DpopProofRequired,
    MissingDpopProof,
    StolenBearer,
    DeviceRevoked,
    DeviceKeyExpired,
    DeviceNotEnrolled,
    RefreshReuseDetected,
    ExpiredCredential,
    GrantRevoked,
    GrantExpired,
    ScopeDenied,
    RepoAccessDenied,
    PaidSeatRequired,
    SeatCapacityExceeded,
    OrganizationAlreadyEnrolled,
    AdminApprovalRequired,
    TenantUpgradeRequired,
    TenantUpgrading,
    TenantSuspended,
    TenantArchived,
    OrgRecoveryRequired,
    PolicyTemplateConflict,
    ProvenanceOriginMismatch,
    IdempotencyConflict,
    IdempotencyInProgress,
    VersionConflict,
    TransactionAborted,
    SignupRequired,
    RepositoryCapacityExhausted,
    RepositoryReservationExhausted,
    ReservationAvailableForBinding,
    RepositoryNotApproved,
    SeatAvailableForAssignment,
    RepositoryAccessDenied,
}

impl CloudAuthErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MfaStepUpRequired => "mfa_step_up_required",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::AdminRequired => "admin_required",
            Self::OwnerRequired => "owner_required",
            Self::ProviderQuarantined => "provider_quarantined",
            Self::QuarantinedProviderLink => "quarantined_provider_link",
            Self::CustodyConflict => "custody_conflict",
            Self::OwnerProtected => "owner_protected",
            Self::MemberNotFound => "member_not_found",
            Self::InvitationExpired => "invitation_expired",
            Self::InvitationNotFound => "invitation_not_found",
            Self::ProjectNotFound => "project_not_found",
            Self::SeatRevoked => "seat_revoked",
            Self::BillingNotConfigured => "billing_not_configured",
            Self::BillingUnavailable => "billing_unavailable",
            Self::Conflict => "conflict",
            Self::NotFound => "not_found",
            Self::InvalidRequest => "invalid_request",
            Self::UseDpopNonce => "use_dpop_nonce",
            Self::InvalidDpopProof => "invalid_dpop_proof",
            Self::DpopAudMismatch => "dpop_aud_mismatch",
            Self::DpopReplay => "dpop_replay",
            Self::DpopNonceMismatch => "dpop_nonce_mismatch",
            Self::DpopClockSkew => "dpop_clock_skew",
            Self::DpopAthMismatch => "dpop_ath_mismatch",
            Self::DpopHtuMismatch => "dpop_htu_mismatch",
            Self::DpopHtmMismatch => "dpop_htm_mismatch",
            Self::DpopJktMismatch => "dpop_jkt_mismatch",
            Self::InvalidBinding => "invalid_binding",
            Self::DpopProofRequired => "dpop_proof_required",
            Self::MissingDpopProof => "missing_dpop_proof",
            Self::StolenBearer => "stolen_bearer",
            Self::DeviceRevoked => "device_revoked",
            Self::DeviceKeyExpired => "device_key_expired",
            Self::DeviceNotEnrolled => "device_not_enrolled",
            Self::RefreshReuseDetected => "refresh_reuse_detected",
            Self::ExpiredCredential => "expired_credential",
            Self::GrantRevoked => "grant_revoked",
            Self::GrantExpired => "grant_expired",
            Self::ScopeDenied => "scope_denied",
            Self::RepoAccessDenied => "repo_access_denied",
            Self::PaidSeatRequired => "paid_seat_required",
            Self::SeatCapacityExceeded => "seat_capacity_exceeded",
            Self::OrganizationAlreadyEnrolled => "organization_already_enrolled",
            Self::AdminApprovalRequired => "admin_approval_required",
            Self::TenantUpgradeRequired => "tenant_upgrade_required",
            Self::TenantUpgrading => "tenant_upgrading",
            Self::TenantSuspended => "tenant_suspended",
            Self::TenantArchived => "tenant_archived",
            Self::OrgRecoveryRequired => "org_recovery_required",
            Self::PolicyTemplateConflict => "policy_template_conflict",
            Self::ProvenanceOriginMismatch => "provenance_origin_mismatch",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::IdempotencyInProgress => "idempotency_in_progress",
            Self::VersionConflict => "version_conflict",
            Self::TransactionAborted => "transaction_aborted",
            Self::SignupRequired => "signup_required",
            Self::RepositoryCapacityExhausted => "repository_capacity_exhausted",
            Self::RepositoryReservationExhausted => "repository_reservation_exhausted",
            Self::ReservationAvailableForBinding => "reservation_available_for_binding",
            Self::RepositoryNotApproved => "repository_not_approved",
            Self::SeatAvailableForAssignment => "seat_available_for_assignment",
            Self::RepositoryAccessDenied => "repository_access_denied",
        }
    }

    pub fn from_code_str(code: &str) -> Option<Self> {
        match code {
            "mfa_step_up_required" => Some(Self::MfaStepUpRequired),
            "outcome_unknown" => Some(Self::OutcomeUnknown),
            "admin_required" => Some(Self::AdminRequired),
            "owner_required" => Some(Self::OwnerRequired),
            "provider_quarantined" => Some(Self::ProviderQuarantined),
            "quarantined_provider_link" => Some(Self::QuarantinedProviderLink),
            "custody_conflict" => Some(Self::CustodyConflict),
            "owner_protected" => Some(Self::OwnerProtected),
            "member_not_found" => Some(Self::MemberNotFound),
            "invitation_expired" => Some(Self::InvitationExpired),
            "invitation_not_found" => Some(Self::InvitationNotFound),
            "project_not_found" => Some(Self::ProjectNotFound),
            "seat_revoked" => Some(Self::SeatRevoked),
            "billing_not_configured" => Some(Self::BillingNotConfigured),
            "billing_unavailable" => Some(Self::BillingUnavailable),
            "conflict" => Some(Self::Conflict),
            "not_found" => Some(Self::NotFound),
            "invalid_request" => Some(Self::InvalidRequest),
            "use_dpop_nonce" => Some(Self::UseDpopNonce),
            "invalid_dpop_proof" => Some(Self::InvalidDpopProof),
            "dpop_aud_mismatch" => Some(Self::DpopAudMismatch),
            "dpop_replay" => Some(Self::DpopReplay),
            "dpop_nonce_mismatch" => Some(Self::DpopNonceMismatch),
            "dpop_clock_skew" => Some(Self::DpopClockSkew),
            "dpop_ath_mismatch" => Some(Self::DpopAthMismatch),
            "dpop_htu_mismatch" => Some(Self::DpopHtuMismatch),
            "dpop_htm_mismatch" => Some(Self::DpopHtmMismatch),
            "dpop_jkt_mismatch" => Some(Self::DpopJktMismatch),
            "invalid_binding" => Some(Self::InvalidBinding),
            "dpop_proof_required" => Some(Self::DpopProofRequired),
            "missing_dpop_proof" => Some(Self::MissingDpopProof),
            "stolen_bearer" => Some(Self::StolenBearer),
            "device_revoked" => Some(Self::DeviceRevoked),
            "device_key_expired" => Some(Self::DeviceKeyExpired),
            "device_not_enrolled" => Some(Self::DeviceNotEnrolled),
            "refresh_reuse_detected" => Some(Self::RefreshReuseDetected),
            "expired_credential" => Some(Self::ExpiredCredential),
            "grant_revoked" => Some(Self::GrantRevoked),
            "grant_expired" => Some(Self::GrantExpired),
            "scope_denied" => Some(Self::ScopeDenied),
            "repo_access_denied" => Some(Self::RepoAccessDenied),
            "paid_seat_required" => Some(Self::PaidSeatRequired),
            "seat_capacity_exceeded" => Some(Self::SeatCapacityExceeded),
            "organization_already_enrolled" => Some(Self::OrganizationAlreadyEnrolled),
            "admin_approval_required" => Some(Self::AdminApprovalRequired),
            "tenant_upgrade_required" => Some(Self::TenantUpgradeRequired),
            "tenant_upgrading" => Some(Self::TenantUpgrading),
            "tenant_suspended" => Some(Self::TenantSuspended),
            "tenant_archived" => Some(Self::TenantArchived),
            "org_recovery_required" => Some(Self::OrgRecoveryRequired),
            "policy_template_conflict" => Some(Self::PolicyTemplateConflict),
            "provenance_origin_mismatch" => Some(Self::ProvenanceOriginMismatch),
            "idempotency_conflict" => Some(Self::IdempotencyConflict),
            "idempotency_in_progress" => Some(Self::IdempotencyInProgress),
            "version_conflict" => Some(Self::VersionConflict),
            "transaction_aborted" => Some(Self::TransactionAborted),
            "signup_required" => Some(Self::SignupRequired),
            "repository_capacity_exhausted" => Some(Self::RepositoryCapacityExhausted),
            "repository_reservation_exhausted" => Some(Self::RepositoryReservationExhausted),
            "reservation_available_for_binding" => Some(Self::ReservationAvailableForBinding),
            "repository_not_approved" => Some(Self::RepositoryNotApproved),
            "seat_available_for_assignment" => Some(Self::SeatAvailableForAssignment),
            "repository_access_denied" => Some(Self::RepositoryAccessDenied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CloudAuthDiagnostic {
    pub schema_version: &'static str,
    pub status: CloudAuthStatus,
    pub message: String,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enrollment_url: Option<Box<str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<CloudAuthErrorCode>,
}

impl CloudAuthDiagnostic {
    pub fn new(
        status: CloudAuthStatus,
        message: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: "decapod.cloud.auth.v1",
            status,
            message: message.into(),
            next_action: next_action.into(),
            authorization_url: None,
            enrollment_url: None,
            error_code: None,
        }
    }

    pub fn with_authorization_url(mut self, url: impl Into<String>) -> Self {
        self.authorization_url = Some(url.into());
        self
    }

    pub fn with_enrollment_url(mut self, url: impl Into<String>) -> Self {
        self.enrollment_url = Some(url.into().into_boxed_str());
        self
    }

    pub fn with_error_code(mut self, code: CloudAuthErrorCode) -> Self {
        self.error_code = Some(code);
        self
    }

    pub fn error_code_str(&self) -> Option<&'static str> {
        self.error_code.map(CloudAuthErrorCode::as_str)
    }
}

/// Backend-neutral classification for failures that may cross the storage
/// boundary. Dactyl supplies physical errors; Decapod retains retry,
/// validation, and governance policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageFailureKind {
    Contention,
    Io,
    Corrupt,
    Constraint,
    Query,
    Value,
    Capability,
    Unknown,
}

impl StorageFailureKind {
    /// Whether Decapod may retry the operation under its bounded policy.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Contention | Self::Io)
    }

    pub fn is_contention(self) -> bool {
        self == Self::Contention
    }
}

/// Canonical error type for all Decapod operations.
#[derive(Debug)]
pub enum DecapodError {
    /// A read-only diagnostic already emitted its structured result.
    DiagnosticExit(i32),
    /// Application storage error normalized by the Dactyl facade.
    StorageError(crate::core::db::Error),
    /// Dactyl physical storage error, normalized at the Decapod boundary.
    DactylError(dactyl_db::DactylError),
    /// I/O error (auto-converts from `std::io::Error`)
    IoError(io::Error),
    /// Database initialization failure
    DatabaseInitializationError(String),
    /// Path resolution or validation error
    PathError(String),
    /// Environment variable error (auto-converts from `std::env::VarError`)
    EnvVarError(env::VarError),
    /// Validation harness failure (proof gate, schema check, etc.)
    ValidationError(String),
    /// An explicit policy gate refusal before entering a mutation.
    PolicyDenied(String),
    /// The command established that its requested mutation did not run.
    MutationRejectedBeforeWrite(String),
    /// Resource not found (missing file, task, claim, etc.)
    NotFound(String),
    /// Feature not yet implemented
    NotImplemented(String),
    /// Configuration error (config.toml parsing, etc.)
    Config(String),
    /// Context pack/archive error
    ContextPackError(String),
    /// Session token error (not found, invalid, expired, etc.)
    SessionError(String),
    /// A safe, actionable cloud authentication handoff result.
    CloudAuth(CloudAuthDiagnostic),
    /// Non-secret cloud mutation outcome and exact retry identity.
    CloudTodo(crate::core::cloud_todo_operation::Diagnostic),
}

impl fmt::Display for DecapodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DiagnosticExit(code) => write!(
                f,
                "Workspace diagnostic returned status {code}; see its report"
            ),
            Self::StorageError(e) => write!(f, "storage error: {e}"),
            Self::DactylError(e) => write!(f, "Dactyl error: {e}"),
            Self::IoError(e) => {
                if e.kind() == std::io::ErrorKind::InvalidInput && e.to_string().contains("SUN_LEN")
                {
                    write!(
                        f,
                        "broker path workspace unavailable in this environment (socket path limitation)"
                    )
                } else {
                    write!(f, "I/O error: {e}")
                }
            }
            Self::DatabaseInitializationError(s) => write!(f, "Failed to initialize database: {s}"),
            Self::PathError(s) => write!(f, "Path error: {s}"),
            Self::EnvVarError(e) => write!(f, "Environment variable error: {e}"),
            Self::ValidationError(s) => {
                if let Some(msg) = s.strip_prefix("NEEDS_HUMAN_INPUT: ") {
                    write!(f, "context: NEEDS_HUMAN_INPUT: {msg}")
                } else if s.starts_with("NEEDS_HUMAN_INPUT") {
                    write!(f, "context: NEEDS_HUMAN_INPUT: execution needs human input")
                } else {
                    write!(f, "Validation error: {s}")
                }
            }
            Self::PolicyDenied(s) | Self::MutationRejectedBeforeWrite(s) => {
                write!(f, "Validation error: {s}")
            }
            Self::NotFound(s) => write!(f, "Not found: {s}"),
            Self::NotImplemented(s) => write!(f, "Not implemented: {s}"),
            Self::Config(s) => write!(f, "Configuration error: {s}"),
            Self::ContextPackError(s) => write!(f, "Context pack error: {s}"),
            Self::SessionError(s) => write!(f, "Session error: {s}"),
            Self::CloudTodo(diagnostic) => write!(f, "{diagnostic}"),
            Self::CloudAuth(diagnostic) => {
                write!(
                    f,
                    "Cloud authentication {:?}: {}",
                    diagnostic.status, diagnostic.message
                )?;
                if let Some(url) = &diagnostic.authorization_url {
                    write!(f, "; sign in: {url}")?;
                }
                if let Some(url) = &diagnostic.enrollment_url {
                    write!(f, "; enroll: {url}")?;
                }
                write!(f, "; next action: {}", diagnostic.next_action)
            }
        }
    }
}

impl std::error::Error for DecapodError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::StorageError(e) => Some(e),
            Self::DactylError(e) => Some(e),
            Self::IoError(e) => Some(e),
            Self::EnvVarError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::core::db::Error> for DecapodError {
    fn from(e: crate::core::db::Error) -> Self {
        Self::StorageError(e)
    }
}

impl From<dactyl_db::DactylError> for DecapodError {
    fn from(e: dactyl_db::DactylError) -> Self {
        Self::DactylError(e)
    }
}

impl From<io::Error> for DecapodError {
    fn from(e: io::Error) -> Self {
        Self::IoError(e)
    }
}

impl From<env::VarError> for DecapodError {
    fn from(e: env::VarError) -> Self {
        Self::EnvVarError(e)
    }
}
impl DecapodError {
    /// Classify storage failures without requiring callers to know the active
    /// backend. This is the application-side seam that dactyl can implement
    /// against later.
    pub fn storage_failure_kind(&self) -> StorageFailureKind {
        match self {
            Self::StorageError(err) => classify_storage_error(err),
            Self::DactylError(err) => classify_dactyl_error(err),
            Self::IoError(err)
                if err
                    .to_string()
                    .to_ascii_lowercase()
                    .contains("disk i/o error") =>
            {
                StorageFailureKind::Io
            }
            Self::IoError(_) => StorageFailureKind::Unknown,
            Self::ValidationError(message) => classify_storage_message(message),
            _ => StorageFailureKind::Unknown,
        }
    }
}

fn classify_dactyl_error(err: &dactyl_db::DactylError) -> StorageFailureKind {
    use dactyl_db::AdapterErrorKind;

    match err {
        dactyl_db::DactylError::Adapter { kind, .. } => match kind {
            AdapterErrorKind::Busy | AdapterErrorKind::Locked | AdapterErrorKind::Timeout => {
                StorageFailureKind::Contention
            }
            AdapterErrorKind::Storage
            | AdapterErrorKind::Transport
            | AdapterErrorKind::Unavailable => StorageFailureKind::Io,
            AdapterErrorKind::Corrupt => StorageFailureKind::Corrupt,
            AdapterErrorKind::Constraint
            | AdapterErrorKind::Conflict
            | AdapterErrorKind::VersionConflict
            | AdapterErrorKind::IdempotencyConflict
            | AdapterErrorKind::IdempotencyInProgress => StorageFailureKind::Constraint,
            AdapterErrorKind::Capability | AdapterErrorKind::ReadOnly => {
                StorageFailureKind::Capability
            }
            AdapterErrorKind::Value => StorageFailureKind::Value,
            AdapterErrorKind::Query | AdapterErrorKind::InvalidOperation => {
                StorageFailureKind::Query
            }
            AdapterErrorKind::TransactionAborted => StorageFailureKind::Query,
            AdapterErrorKind::Authentication
            | AdapterErrorKind::Authorization
            | AdapterErrorKind::RateLimited
            | AdapterErrorKind::Quota
            | AdapterErrorKind::NotFound
            | AdapterErrorKind::Cancellation
            | AdapterErrorKind::Protocol
            | AdapterErrorKind::Unknown => StorageFailureKind::Unknown,
        },
        dactyl_db::DactylError::Config(_) | dactyl_db::DactylError::UnsupportedOperation(_) => {
            StorageFailureKind::Capability
        }
        dactyl_db::DactylError::ColumnNotFound(_) | dactyl_db::DactylError::Conversion(_) => {
            StorageFailureKind::Value
        }
    }
}

fn classify_storage_error(err: &crate::core::db::Error) -> StorageFailureKind {
    match err {
        crate::core::db::Error::Dactyl(error) => classify_dactyl_error(error),
        crate::core::db::Error::Io(_) => StorageFailureKind::Io,
        crate::core::db::Error::StorageOwner(_) => StorageFailureKind::Unknown,
        crate::core::db::Error::QueryReturnedNoRows => StorageFailureKind::Query,
        crate::core::db::Error::FromSqlConversionFailure(_, _, _)
        | crate::core::db::Error::ToSqlConversionFailure(_)
        | crate::core::db::Error::InvalidColumnType(_, _, _) => StorageFailureKind::Value,
        crate::core::db::Error::InvalidParameterName(_) | crate::core::db::Error::InvalidQuery => {
            StorageFailureKind::Query
        }
    }
}

fn classify_storage_message(message: &str) -> StorageFailureKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("storage_lock_timeout")
        || lower.contains("storage lock timeout")
        || lower.contains("storage_pool_lock_timeout")
        || lower.contains("storage pool lock timeout")
        || lower.contains("database is locked")
        || lower.contains("databasebusy")
        || lower.contains("storage contention")
        || lower.contains("extended_code: 522")
    {
        StorageFailureKind::Contention
    } else if lower.contains("disk i/o error") {
        StorageFailureKind::Io
    } else if lower.contains("constraint failed")
        || lower.contains("unique constraint")
        || lower.contains("foreign key constraint")
    {
        StorageFailureKind::Constraint
    } else {
        StorageFailureKind::Unknown
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/core/error_tests.rs"]
mod tests;

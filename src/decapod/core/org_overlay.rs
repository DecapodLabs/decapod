use crate::core::assets::{self, ResolvedOverrideEvidence};
use crate::core::error::DecapodError;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ORG_OVERLAY_SCHEMA_VERSION: &str = "decapod.org-constitution-overlay.v1";
pub const AUTHORITY_CHAIN_SCHEMA_VERSION: &str = "decapod.authority-chain.v1";
pub const IMPORTED_ORG_OVERLAY_REL_PATH: &str = ".decapod/managed/organization-overlay.json";
pub const MAX_ORG_OVERLAY_BYTES: usize = 512 * 1024;

pub const LAYER_BINARY_EMBEDDED: &str = "binary_embedded";
pub const LAYER_ORGANIZATION_OVERLAY: &str = "organization_overlay";
pub const LAYER_REPOSITORY_PROJECT_OVERRIDE: &str = "repository_project_override";
pub const LAYER_TASK_CONTEXT_PROJECTION: &str = "task_context_projection";

/// Explicit local configuration under `[context.organization_overlay]` in `.decapod/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OrganizationOverlayConfig {
    /// Explicit local path (relative to repository root, or absolute path to an org cache file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Expected organization ID. Name/ID alone never grants trust without an explicit file/bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    /// Expected SHA-256 hex digest of the canonical bundle payload (`bundle_sha256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Optional 32-byte Ed25519 public key (64 hex chars) for offline cryptographic signature verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key_hex: Option<String>,
    /// Optional maximum age in seconds from `issued_at_unix` before the overlay becomes stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u64>,
    /// Freshness policy: `"strict"` (default, fail closed on stale), `"offline_cached"`, or `"offline_stale_degraded"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freshness_policy: Option<String>,
    /// Optional path override for imported overlay bundle (defaults to `.decapod/managed/organization-overlay.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_bundle_path: Option<String>,
}

impl OrganizationOverlayConfig {
    pub fn is_empty(&self) -> bool {
        self.path.is_none()
            && self.org_id.is_none()
            && self.sha256.is_none()
            && self.public_key_hex.is_none()
            && self.max_age_seconds.is_none()
            && self.freshness_policy.is_none()
            && self.imported_bundle_path.is_none()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveOverridability {
    Overridable,
    ExtendOnly,
    NonOverridable,
}

impl DirectiveOverridability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Overridable => "overridable",
            Self::ExtendOnly => "extend_only",
            Self::NonOverridable => "non_overridable",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveVisibility {
    #[default]
    Shared,
    OrganizationPrivate,
}

impl DirectiveVisibility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::OrganizationPrivate => "organization_private",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationDirective {
    pub directive_id: String,
    pub overridability: DirectiveOverridability,
    #[serde(default)]
    pub visibility: DirectiveVisibility,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prohibited_weakening_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationOverlayBundle {
    pub schema_version: String,
    pub org_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_name: Option<String>,
    pub version: String,
    pub source: String,
    pub issued_at_unix: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<u64>,
    #[serde(default)]
    pub offline_cached: bool,
    pub directives: Vec<OrganizationDirective>,
    pub bundle_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signer_key_id: Option<String>,
}

#[derive(Serialize)]
struct CanonicalOverlayPayload<'a> {
    schema_version: &'a str,
    org_id: &'a str,
    org_name: &'a Option<String>,
    version: &'a str,
    source: &'a str,
    issued_at_unix: u64,
    expires_at_unix: Option<u64>,
    offline_cached: bool,
    directives: &'a [OrganizationDirective],
}

impl OrganizationOverlayBundle {
    pub fn canonical_payload_bytes(&self) -> Result<Vec<u8>, DecapodError> {
        let mut sorted_directives = self.directives.clone();
        sorted_directives.sort_by(|a, b| a.directive_id.cmp(&b.directive_id));
        let payload = CanonicalOverlayPayload {
            schema_version: &self.schema_version,
            org_id: &self.org_id,
            org_name: &self.org_name,
            version: &self.version,
            source: &self.source,
            issued_at_unix: self.issued_at_unix,
            expires_at_unix: self.expires_at_unix,
            offline_cached: self.offline_cached,
            directives: &sorted_directives,
        };
        serde_json::to_vec(&payload)
            .map_err(|e| DecapodError::ValidationError(format!("canonical overlay serialize: {e}")))
    }

    pub fn computed_bundle_sha256(&self) -> Result<String, DecapodError> {
        let bytes = self.canonical_payload_bytes()?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    pub fn with_recomputed_digest(mut self) -> Result<Self, DecapodError> {
        self.directives
            .sort_by(|a, b| a.directive_id.cmp(&b.directive_id));
        self.bundle_sha256 = self.computed_bundle_sha256()?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayTrustState {
    Absent,
    LocalConfigured,
    VerifiedDigest,
    VerifiedSignature,
    Tampered,
    Invalid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OverlayFreshnessState {
    Absent,
    Fresh,
    OfflineCached,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationOverlayStatus {
    pub present: bool,
    pub source_kind: Option<String>,
    pub resolved_path: Option<String>,
    pub org_id: Option<String>,
    pub org_name: Option<String>,
    pub version: Option<String>,
    pub source: Option<String>,
    pub bundle_sha256: Option<String>,
    pub trust_state: OverlayTrustState,
    pub freshness_state: OverlayFreshnessState,
    pub offline_usable: bool,
    pub directive_count: usize,
    pub issued_at_unix: Option<u64>,
    pub expires_at_unix: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityLayerSummary {
    pub rank: u8,
    pub layer: String,
    pub authoritative: bool,
    pub present: bool,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedDirectiveRecord {
    pub directive_id: String,
    pub effective_layer: String,
    pub precedence_decision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overridability: Option<DirectiveOverridability>,
    pub visibility: DirectiveVisibility,
    pub base_present: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_body_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_body_hash: Option<String>,
    pub effective_source: String,
    pub effective_body_hash: String,
    pub redacted_for_portability: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityChainProvenance {
    pub schema_version: String,
    pub authoritative_order: Vec<String>,
    pub non_authoritative_projection: String,
    pub organization_overlay: OrganizationOverlayStatus,
    pub layers: Vec<AuthorityLayerSummary>,
    pub directives: Vec<ResolvedDirectiveRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    pub offline_usable: bool,
    pub task_projection_isolation_enforced: bool,
}

#[derive(Debug, Clone)]
pub struct LoadedOrganizationOverlay {
    pub status: OrganizationOverlayStatus,
    pub bundle: Option<OrganizationOverlayBundle>,
    pub directives_by_id: BTreeMap<String, OrganizationDirective>,
}

#[derive(Debug, Clone)]
pub struct ResolvedAuthorityResult {
    pub chain: AuthorityChainProvenance,
    pub org_overlay: LoadedOrganizationOverlay,
    pub effective_bodies: BTreeMap<String, String>,
    pub evidence: Vec<ResolvedOverrideEvidence>,
}

pub fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn normalize_sha256(value: &str) -> String {
    value
        .trim()
        .strip_prefix("sha256:")
        .unwrap_or(value.trim())
        .to_ascii_lowercase()
}

fn decode_hex_fixed<const N: usize>(value: &str) -> Option<[u8; N]> {
    let trimmed = value.trim();
    if trimmed.len() != N * 2 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; N];
    for (idx, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&trimmed[idx * 2..idx * 2 + 2], 16).ok()?;
    }
    Some(out)
}

fn resolve_overlay_path(repo_root: &Path, raw_path: &str) -> Result<PathBuf, DecapodError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(DecapodError::ValidationError(
            "ORG_OVERLAY_INVALID_CONFIG: organization_overlay path cannot be empty".to_string(),
        ));
    }
    let candidate = Path::new(trimmed);
    if candidate.is_absolute() {
        Ok(candidate.to_path_buf())
    } else {
        Ok(repo_root.join(candidate))
    }
}

fn verify_and_classify_bundle(
    config: &OrganizationOverlayConfig,
    source_kind: &str,
    resolved_path: &Path,
    raw_bytes: &[u8],
    now_unix: u64,
) -> (OrganizationOverlayStatus, Option<OrganizationOverlayBundle>) {
    let make_error_status = |trust_state: OverlayTrustState,
                             freshness_state: OverlayFreshnessState,
                             bundle: Option<&OrganizationOverlayBundle>,
                             diagnostic: String| {
        OrganizationOverlayStatus {
            present: true,
            source_kind: Some(source_kind.to_string()),
            resolved_path: Some(resolved_path.display().to_string()),
            org_id: bundle.map(|b| b.org_id.clone()),
            org_name: bundle.and_then(|b| b.org_name.clone()),
            version: bundle.map(|b| b.version.clone()),
            source: bundle.map(|b| b.source.clone()),
            bundle_sha256: bundle.map(|b| b.bundle_sha256.clone()),
            trust_state,
            freshness_state,
            offline_usable: false,
            directive_count: bundle.map(|b| b.directives.len()).unwrap_or(0),
            issued_at_unix: bundle.map(|b| b.issued_at_unix),
            expires_at_unix: bundle.and_then(|b| b.expires_at_unix),
            diagnostic: Some(diagnostic),
        }
    };

    if raw_bytes.len() > MAX_ORG_OVERLAY_BYTES {
        return (
            make_error_status(
                OverlayTrustState::Invalid,
                OverlayFreshnessState::Absent,
                None,
                format!(
                    "ORG_OVERLAY_INVALID_BUNDLE: bundle exceeds maximum size ({} bytes)",
                    MAX_ORG_OVERLAY_BYTES
                ),
            ),
            None,
        );
    }

    let bundle: OrganizationOverlayBundle = match serde_json::from_slice(raw_bytes) {
        Ok(parsed) => parsed,
        Err(err) => {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    None,
                    format!("ORG_OVERLAY_INVALID_BUNDLE: malformed JSON: {err}"),
                ),
                None,
            );
        }
    };

    if bundle.schema_version != ORG_OVERLAY_SCHEMA_VERSION {
        return (
            make_error_status(
                OverlayTrustState::Invalid,
                OverlayFreshnessState::Absent,
                Some(&bundle),
                format!(
                    "ORG_OVERLAY_SCHEMA_MISMATCH: expected '{}', got '{}'",
                    ORG_OVERLAY_SCHEMA_VERSION, bundle.schema_version
                ),
            ),
            None,
        );
    }

    if bundle.org_id.trim().is_empty()
        || bundle.version.trim().is_empty()
        || bundle.source.trim().is_empty()
    {
        return (
            make_error_status(
                OverlayTrustState::Invalid,
                OverlayFreshnessState::Absent,
                Some(&bundle),
                "ORG_OVERLAY_INVALID_BUNDLE: org_id, version, and source must be non-empty"
                    .to_string(),
            ),
            None,
        );
    }

    if let Some(expected_org_id) = config
        .org_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && bundle.org_id != expected_org_id
    {
        return (
            make_error_status(
                OverlayTrustState::Tampered,
                OverlayFreshnessState::Absent,
                Some(&bundle),
                format!(
                    "ORG_OVERLAY_ORG_ID_MISMATCH: configured org_id '{}' does not match bundle org_id '{}'",
                    expected_org_id, bundle.org_id
                ),
            ),
            None,
        );
    }

    let mut seen_ids = BTreeSet::new();
    for directive in &bundle.directives {
        if directive.body.trim().is_empty() {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    format!(
                        "ORG_OVERLAY_INVALID_DIRECTIVE: directive '{}' has empty body",
                        directive.directive_id
                    ),
                ),
                None,
            );
        }
        let Some(canonical) = assets::canonical_override_id(&directive.directive_id) else {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    format!(
                        "ORG_OVERLAY_UNKNOWN_DIRECTIVE: '{}' is not a recognized constitution directive ID",
                        directive.directive_id
                    ),
                ),
                None,
            );
        };
        if canonical != directive.directive_id {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    format!(
                        "ORG_OVERLAY_NON_CANONICAL_DIRECTIVE: expected canonical ID '{}', got '{}'",
                        canonical, directive.directive_id
                    ),
                ),
                None,
            );
        }
        if !seen_ids.insert(directive.directive_id.clone()) {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    format!(
                        "ORG_OVERLAY_DUPLICATE_DIRECTIVE: duplicate directive '{}'",
                        directive.directive_id
                    ),
                ),
                None,
            );
        }
    }

    let computed_sha = match bundle.computed_bundle_sha256() {
        Ok(hash) => hash,
        Err(err) => {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    err.to_string(),
                ),
                None,
            );
        }
    };
    if normalize_sha256(&bundle.bundle_sha256) != computed_sha {
        return (
            make_error_status(
                OverlayTrustState::Tampered,
                OverlayFreshnessState::Absent,
                Some(&bundle),
                format!(
                    "ORG_OVERLAY_TAMPERED: bundle_sha256 '{}' does not match canonical payload digest '{}'",
                    bundle.bundle_sha256, computed_sha
                ),
            ),
            None,
        );
    }

    let mut digest_pinned = false;
    if let Some(expected_sha) = config
        .sha256
        .as_deref()
        .map(normalize_sha256)
        .filter(|s| !s.is_empty())
    {
        if expected_sha != computed_sha {
            return (
                make_error_status(
                    OverlayTrustState::Tampered,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    format!(
                        "ORG_OVERLAY_TAMPERED: bundle digest '{}' does not match configured sha256 pin '{}'",
                        computed_sha, expected_sha
                    ),
                ),
                None,
            );
        }
        digest_pinned = true;
    }

    let mut signature_verified = false;
    if let Some(pk_hex) = config
        .public_key_hex
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let Some(pk_bytes) = decode_hex_fixed::<32>(pk_hex) else {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    "ORG_OVERLAY_INVALID_PUBLIC_KEY: public_key_hex must be 64 hex characters (32-byte Ed25519 key)".to_string(),
                ),
                None,
            );
        };
        let Some(sig_hex) = bundle
            .signature_hex
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return (
                make_error_status(
                    OverlayTrustState::Tampered,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    "ORG_OVERLAY_SIGNATURE_MISSING: public_key_hex is configured but overlay bundle is unsigned".to_string(),
                ),
                None,
            );
        };
        let Some(sig_bytes) = decode_hex_fixed::<64>(sig_hex) else {
            return (
                make_error_status(
                    OverlayTrustState::Tampered,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    "ORG_OVERLAY_TAMPERED: signature_hex is malformed".to_string(),
                ),
                None,
            );
        };
        let Ok(canonical_bytes) = bundle.canonical_payload_bytes() else {
            return (
                make_error_status(
                    OverlayTrustState::Invalid,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    "ORG_OVERLAY_INVALID_BUNDLE: cannot encode canonical payload for signature check".to_string(),
                ),
                None,
            );
        };
        if UnparsedPublicKey::new(&ED25519, pk_bytes)
            .verify(&canonical_bytes, &sig_bytes)
            .is_err()
        {
            return (
                make_error_status(
                    OverlayTrustState::Tampered,
                    OverlayFreshnessState::Absent,
                    Some(&bundle),
                    "ORG_OVERLAY_TAMPERED: Ed25519 signature verification failed".to_string(),
                ),
                None,
            );
        }
        signature_verified = true;
    }

    let trust_state = if signature_verified {
        OverlayTrustState::VerifiedSignature
    } else if digest_pinned || source_kind == "imported_bundle" {
        OverlayTrustState::VerifiedDigest
    } else {
        OverlayTrustState::LocalConfigured
    };

    let expired_by_bundle = bundle.expires_at_unix.is_some_and(|exp| now_unix > exp);
    let expired_by_age = config
        .max_age_seconds
        .is_some_and(|max_age| now_unix > bundle.issued_at_unix.saturating_add(max_age));
    let is_stale = expired_by_bundle || expired_by_age;

    let policy = config
        .freshness_policy
        .as_deref()
        .map(str::trim)
        .unwrap_or("strict");
    let offline_mode =
        bundle.offline_cached || matches!(policy, "offline_cached" | "offline_stale_degraded");
    let freshness_state = if is_stale {
        OverlayFreshnessState::Stale
    } else if offline_mode || source_kind == "imported_bundle" {
        OverlayFreshnessState::OfflineCached
    } else {
        OverlayFreshnessState::Fresh
    };

    let diagnostic = if is_stale {
        if policy == "offline_stale_degraded" {
            Some("ORG_OVERLAY_STALE_DEGRADED: overlay is past freshness window; permitted under explicit offline_stale_degraded policy".to_string())
        } else {
            Some(format!(
                "ORG_OVERLAY_STALE: organization overlay '{}' (version {}) expired at {} / max_age={:?}, now={}",
                bundle.org_id,
                bundle.version,
                bundle.expires_at_unix.unwrap_or(0),
                config.max_age_seconds,
                now_unix
            ))
        }
    } else {
        None
    };

    let offline_usable = !is_stale || policy == "offline_stale_degraded";

    let status = OrganizationOverlayStatus {
        present: true,
        source_kind: Some(source_kind.to_string()),
        resolved_path: Some(resolved_path.display().to_string()),
        org_id: Some(bundle.org_id.clone()),
        org_name: bundle.org_name.clone(),
        version: Some(bundle.version.clone()),
        source: Some(bundle.source.clone()),
        bundle_sha256: Some(computed_sha),
        trust_state,
        freshness_state,
        offline_usable,
        directive_count: bundle.directives.len(),
        issued_at_unix: Some(bundle.issued_at_unix),
        expires_at_unix: bundle.expires_at_unix,
        diagnostic,
    };
    (status, Some(bundle))
}

pub fn load_organization_overlay(
    repo_root: &Path,
) -> Result<LoadedOrganizationOverlay, DecapodError> {
    load_organization_overlay_with_now(repo_root, now_unix_seconds())
}

pub fn load_organization_overlay_with_now(
    repo_root: &Path,
    now_unix: u64,
) -> Result<LoadedOrganizationOverlay, DecapodError> {
    let config_path = repo_root.join(".decapod").join("config.toml");
    let project_cfg = if config_path.exists() {
        crate::cli::DecapodProjectConfig::load(repo_root)?
    } else {
        crate::cli::DecapodProjectConfig::default()
    };
    let overlay_cfg = project_cfg
        .context
        .organization_overlay
        .as_ref()
        .cloned()
        .unwrap_or_default();

    let default_imported = repo_root.join(
        overlay_cfg
            .imported_bundle_path
            .as_deref()
            .unwrap_or(IMPORTED_ORG_OVERLAY_REL_PATH),
    );

    let selected: Option<(&'static str, PathBuf)> = if let Some(explicit) =
        overlay_cfg.path.as_deref()
    {
        let resolved = resolve_overlay_path(repo_root, explicit)?;
        if !resolved.exists() {
            let status = OrganizationOverlayStatus {
                present: true,
                source_kind: Some("local_config".to_string()),
                resolved_path: Some(resolved.display().to_string()),
                org_id: overlay_cfg.org_id.clone(),
                org_name: None,
                version: None,
                source: None,
                bundle_sha256: overlay_cfg.sha256.clone(),
                trust_state: OverlayTrustState::Invalid,
                freshness_state: OverlayFreshnessState::Absent,
                offline_usable: false,
                directive_count: 0,
                issued_at_unix: None,
                expires_at_unix: None,
                diagnostic: Some(format!(
                    "ORG_OVERLAY_MISSING: configured organization overlay file does not exist: {}",
                    resolved.display()
                )),
            };
            return Ok(LoadedOrganizationOverlay {
                status,
                bundle: None,
                directives_by_id: BTreeMap::new(),
            });
        }
        Some(("local_config", resolved))
    } else if default_imported.exists() {
        Some(("imported_bundle", default_imported))
    } else if overlay_cfg
        .org_id
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
    {
        // Issue #877 non-goal: never treat an organization name/ID alone as proof of authority.
        let status = OrganizationOverlayStatus {
            present: true,
            source_kind: Some("unverified_org_name_only".to_string()),
            resolved_path: None,
            org_id: overlay_cfg.org_id.clone(),
            org_name: None,
            version: None,
            source: None,
            bundle_sha256: overlay_cfg.sha256.clone(),
            trust_state: OverlayTrustState::Invalid,
            freshness_state: OverlayFreshnessState::Absent,
            offline_usable: false,
            directive_count: 0,
            issued_at_unix: None,
            expires_at_unix: None,
            diagnostic: Some(format!(
                "ORG_OVERLAY_UNVERIFIED_ORG_IDENTITY: configured org_id '{}' without a local overlay file or imported bundle cannot establish authority",
                overlay_cfg.org_id.as_deref().unwrap_or("")
            )),
        };
        return Ok(LoadedOrganizationOverlay {
            status,
            bundle: None,
            directives_by_id: BTreeMap::new(),
        });
    } else {
        None
    };

    let Some((source_kind, path)) = selected else {
        return Ok(LoadedOrganizationOverlay {
            status: OrganizationOverlayStatus {
                present: false,
                source_kind: None,
                resolved_path: None,
                org_id: None,
                org_name: None,
                version: None,
                source: None,
                bundle_sha256: None,
                trust_state: OverlayTrustState::Absent,
                freshness_state: OverlayFreshnessState::Absent,
                offline_usable: true,
                directive_count: 0,
                issued_at_unix: None,
                expires_at_unix: None,
                diagnostic: None,
            },
            bundle: None,
            directives_by_id: BTreeMap::new(),
        });
    };

    let raw_bytes = fs::read(&path).map_err(DecapodError::IoError)?;
    let (status, bundle) =
        verify_and_classify_bundle(&overlay_cfg, source_kind, &path, &raw_bytes, now_unix);
    let directives_by_id = bundle
        .as_ref()
        .map(|b| {
            b.directives
                .iter()
                .map(|d| (d.directive_id.clone(), d.clone()))
                .collect()
        })
        .unwrap_or_default();
    Ok(LoadedOrganizationOverlay {
        status,
        bundle,
        directives_by_id,
    })
}

pub fn ensure_overlay_admissible(loaded: &LoadedOrganizationOverlay) -> Result<(), DecapodError> {
    if !loaded.status.present {
        return Ok(());
    }
    match loaded.status.trust_state {
        OverlayTrustState::Tampered | OverlayTrustState::Invalid => {
            return Err(DecapodError::ValidationError(
                loaded
                    .status
                    .diagnostic
                    .clone()
                    .unwrap_or_else(|| "ORG_OVERLAY_INVALID: verification failed".to_string()),
            ));
        }
        OverlayTrustState::Absent
        | OverlayTrustState::LocalConfigured
        | OverlayTrustState::VerifiedDigest
        | OverlayTrustState::VerifiedSignature => {}
    }
    if loaded.status.freshness_state == OverlayFreshnessState::Stale
        && !loaded.status.offline_usable
    {
        return Err(DecapodError::ValidationError(
            loaded.status.diagnostic.clone().unwrap_or_else(|| {
                "ORG_OVERLAY_STALE: organization overlay is expired".to_string()
            }),
        ));
    }
    Ok(())
}

const DEFAULT_WEAKENING_PATTERNS: &[&str] = &[
    "weaken",
    "weakens",
    "waive",
    "waived",
    "exempt",
    "exemption",
    "bypass",
    "do not enforce",
    "not required",
    "no longer required",
    "optional instead",
    "override organization",
    "replace organization",
    "ignore organization",
    "disable ",
    "skip_gate",
    "allow without",
    "allow unverified",
];

fn detect_extend_only_weakening(org: &OrganizationDirective, project_body: &str) -> Option<String> {
    let lowered = project_body.to_ascii_lowercase();
    for pat in DEFAULT_WEAKENING_PATTERNS {
        if lowered.contains(pat) {
            return Some(format!("contains weakening phrase '{}'", pat.trim()));
        }
    }
    for pat in &org.prohibited_weakening_terms {
        let trimmed = pat.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && lowered.contains(&trimmed) {
            return Some(format!(
                "contains prohibited weakening term '{}'",
                pat.trim()
            ));
        }
    }
    for req in &org.required_terms {
        let req_lower = req.trim().to_ascii_lowercase();
        if req_lower.is_empty() {
            continue;
        }
        for prefix in ["no ", "without ", "skip ", "disable ", "drop "] {
            let negated = format!("{prefix}{req_lower}");
            if lowered.contains(&negated) {
                return Some(format!("negates required control term '{}'", req.trim()));
            }
        }
    }
    None
}

fn sha256_hex(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn resolve_authority_chain(repo_root: &Path) -> Result<ResolvedAuthorityResult, DecapodError> {
    resolve_authority_chain_with_now(repo_root, now_unix_seconds())
}

pub fn resolve_authority_chain_with_now(
    repo_root: &Path,
    now_unix: u64,
) -> Result<ResolvedAuthorityResult, DecapodError> {
    let org_overlay = load_organization_overlay_with_now(repo_root, now_unix)?;
    ensure_overlay_admissible(&org_overlay)?;

    let project_evidence = assets::raw_project_override_entries(repo_root)?;
    let mut project_by_id: BTreeMap<String, (ResolvedOverrideEvidence, String)> = BTreeMap::new();
    for (evidence, body) in project_evidence {
        if evidence.byte_count > 0 {
            project_by_id.insert(evidence.directive_id.clone(), (evidence, body));
        }
    }

    let mut all_ids = BTreeSet::new();
    for id in org_overlay.directives_by_id.keys() {
        all_ids.insert(id.clone());
    }
    for id in project_by_id.keys() {
        all_ids.insert(id.clone());
    }

    let mut conflicts = Vec::new();
    let mut directives = Vec::new();
    let mut effective_bodies = BTreeMap::new();
    let mut evidence_out = Vec::new();

    for directive_id in all_ids {
        let base_present = assets::get_embedded_doc(&directive_id).is_some();
        let org_dir = org_overlay.directives_by_id.get(&directive_id);
        let proj = project_by_id.get(&directive_id);

        match (org_dir, proj) {
            (Some(org), None) => {
                let org_source = org_overlay
                    .status
                    .resolved_path
                    .clone()
                    .unwrap_or_else(|| "organization_overlay".to_string());
                let org_hash = sha256_hex(&org.body);
                effective_bodies.insert(directive_id.clone(), org.body.clone());
                let precedence_decision = match org.overridability {
                    DirectiveOverridability::NonOverridable => {
                        "organization_non_overridable_enforced"
                    }
                    DirectiveOverridability::ExtendOnly => "organization_extend_only_enforced",
                    DirectiveOverridability::Overridable => "organization_overlay_applied",
                }
                .to_string();
                directives.push(ResolvedDirectiveRecord {
                    directive_id: directive_id.clone(),
                    effective_layer: LAYER_ORGANIZATION_OVERLAY.to_string(),
                    precedence_decision: precedence_decision.clone(),
                    overridability: Some(org.overridability),
                    visibility: org.visibility,
                    base_present,
                    org_source: Some(org_source.clone()),
                    org_body_hash: Some(org_hash.clone()),
                    project_source: None,
                    project_body_hash: None,
                    effective_source: org_source.clone(),
                    effective_body_hash: org_hash.clone(),
                    redacted_for_portability: org.visibility
                        == DirectiveVisibility::OrganizationPrivate,
                    reasons: vec![
                        format!("layer:{}", LAYER_ORGANIZATION_OVERLAY),
                        format!("overridability:{}", org.overridability.as_str()),
                    ],
                });
                evidence_out.push(ResolvedOverrideEvidence {
                    directive_id,
                    source: org_source,
                    source_hash: org_overlay
                        .status
                        .bundle_sha256
                        .clone()
                        .unwrap_or_else(|| org_hash.clone()),
                    body_hash: org_hash,
                    byte_count: org.body.len(),
                    precedence: LAYER_ORGANIZATION_OVERLAY.to_string(),
                });
            }
            (None, Some((proj_ev, proj_body))) => {
                effective_bodies.insert(directive_id.clone(), proj_body.clone());
                directives.push(ResolvedDirectiveRecord {
                    directive_id: directive_id.clone(),
                    effective_layer: LAYER_REPOSITORY_PROJECT_OVERRIDE.to_string(),
                    precedence_decision: "project_override_applied".to_string(),
                    overridability: None,
                    visibility: DirectiveVisibility::Shared,
                    base_present,
                    org_source: None,
                    org_body_hash: None,
                    project_source: Some(proj_ev.source.clone()),
                    project_body_hash: Some(proj_ev.body_hash.clone()),
                    effective_source: proj_ev.source.clone(),
                    effective_body_hash: proj_ev.body_hash.clone(),
                    redacted_for_portability: false,
                    reasons: vec![format!("layer:{}", LAYER_REPOSITORY_PROJECT_OVERRIDE)],
                });
                evidence_out.push(proj_ev.clone());
            }
            (Some(org), Some((proj_ev, proj_body))) => {
                let org_source = org_overlay
                    .status
                    .resolved_path
                    .clone()
                    .unwrap_or_else(|| "organization_overlay".to_string());
                let org_hash = sha256_hex(&org.body);
                match org.overridability {
                    DirectiveOverridability::NonOverridable => {
                        let msg = format!(
                            "ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT: .decapod/OVERRIDE.md attempts to override non_overridable organization directive '{}' (org '{}')",
                            directive_id,
                            org_overlay.status.org_id.as_deref().unwrap_or("unknown")
                        );
                        conflicts.push(msg.clone());
                        return Err(DecapodError::ValidationError(msg));
                    }
                    DirectiveOverridability::ExtendOnly => {
                        if let Some(reason) = detect_extend_only_weakening(org, proj_body) {
                            let msg = format!(
                                "ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED: .decapod/OVERRIDE.md weakens extend_only organization directive '{}': {}",
                                directive_id, reason
                            );
                            conflicts.push(msg.clone());
                            return Err(DecapodError::ValidationError(msg));
                        }
                        let combined = format!(
                            "{}\n\n### Project Extension (.decapod/OVERRIDE.md)\n\n{}",
                            org.body.trim(),
                            proj_body.trim()
                        );
                        let combined_hash = sha256_hex(&combined);
                        effective_bodies.insert(directive_id.clone(), combined.clone());
                        directives.push(ResolvedDirectiveRecord {
                            directive_id: directive_id.clone(),
                            effective_layer: "organization_plus_project_extension".to_string(),
                            precedence_decision: "organization_extended_by_project".to_string(),
                            overridability: Some(org.overridability),
                            visibility: org.visibility,
                            base_present,
                            org_source: Some(org_source.clone()),
                            org_body_hash: Some(org_hash),
                            project_source: Some(proj_ev.source.clone()),
                            project_body_hash: Some(proj_ev.body_hash.clone()),
                            effective_source: format!("{org_source} + {}", proj_ev.source),
                            effective_body_hash: combined_hash.clone(),
                            redacted_for_portability: org.visibility
                                == DirectiveVisibility::OrganizationPrivate,
                            reasons: vec![
                                format!("layer:{}", LAYER_ORGANIZATION_OVERLAY),
                                format!("layer:{}", LAYER_REPOSITORY_PROJECT_OVERRIDE),
                                "overridability:extend_only".to_string(),
                            ],
                        });
                        evidence_out.push(ResolvedOverrideEvidence {
                            directive_id,
                            source: format!("{org_source} + {}", proj_ev.source),
                            source_hash: sha256_hex(&format!(
                                "{}:{}",
                                org_overlay.status.bundle_sha256.as_deref().unwrap_or(""),
                                proj_ev.source_hash
                            )),
                            body_hash: combined_hash,
                            byte_count: combined.len(),
                            precedence: "organization_plus_project_extension".to_string(),
                        });
                    }
                    DirectiveOverridability::Overridable => {
                        effective_bodies.insert(directive_id.clone(), proj_body.clone());
                        directives.push(ResolvedDirectiveRecord {
                            directive_id: directive_id.clone(),
                            effective_layer: LAYER_REPOSITORY_PROJECT_OVERRIDE.to_string(),
                            precedence_decision: "organization_overridden_by_project".to_string(),
                            overridability: Some(org.overridability),
                            visibility: DirectiveVisibility::Shared,
                            base_present,
                            org_source: Some(org_source),
                            org_body_hash: Some(org_hash),
                            project_source: Some(proj_ev.source.clone()),
                            project_body_hash: Some(proj_ev.body_hash.clone()),
                            effective_source: proj_ev.source.clone(),
                            effective_body_hash: proj_ev.body_hash.clone(),
                            redacted_for_portability: false,
                            reasons: vec![
                                format!("layer:{}", LAYER_ORGANIZATION_OVERLAY),
                                format!("layer:{}", LAYER_REPOSITORY_PROJECT_OVERRIDE),
                                "overridability:overridable".to_string(),
                            ],
                        });
                        evidence_out.push(proj_ev.clone());
                    }
                }
            }
            (None, None) => {}
        }
    }

    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");
    let layers = vec![
        AuthorityLayerSummary {
            rank: 1,
            layer: LAYER_BINARY_EMBEDDED.to_string(),
            authoritative: true,
            present: true,
            source: "embedded://constitution".to_string(),
            status: Some("active".to_string()),
            digest: Some(env!("CARGO_PKG_VERSION").to_string()),
            note: Some("Baseline binary-embedded constitution".to_string()),
        },
        AuthorityLayerSummary {
            rank: 2,
            layer: LAYER_ORGANIZATION_OVERLAY.to_string(),
            authoritative: true,
            present: org_overlay.status.present,
            source: org_overlay
                .status
                .resolved_path
                .clone()
                .unwrap_or_else(|| "absent".to_string()),
            status: Some(format!("{:?}", org_overlay.status.trust_state)),
            digest: org_overlay.status.bundle_sha256.clone(),
            note: org_overlay.status.diagnostic.clone(),
        },
        AuthorityLayerSummary {
            rank: 3,
            layer: LAYER_REPOSITORY_PROJECT_OVERRIDE.to_string(),
            authoritative: true,
            present: override_path.exists() && !project_by_id.is_empty(),
            source: ".decapod/OVERRIDE.md".to_string(),
            status: Some(if override_path.exists() {
                "active".to_string()
            } else {
                "absent".to_string()
            }),
            digest: project_by_id
                .values()
                .next()
                .map(|(ev, _)| ev.source_hash.clone()),
            note: Some(format!("{} active directive override(s)", project_by_id.len())),
        },
        AuthorityLayerSummary {
            rank: 4,
            layer: LAYER_TASK_CONTEXT_PROJECTION.to_string(),
            authoritative: false,
            present: true,
            source: "context.resolve".to_string(),
            status: Some("projection_only".to_string()),
            digest: None,
            note: Some(
                "Task-scoped resolved context is strictly an advisory/execution projection and never overrides Layers 1-3."
                    .to_string(),
            ),
        },
    ];

    let chain = AuthorityChainProvenance {
        schema_version: AUTHORITY_CHAIN_SCHEMA_VERSION.to_string(),
        authoritative_order: vec![
            LAYER_BINARY_EMBEDDED.to_string(),
            LAYER_ORGANIZATION_OVERLAY.to_string(),
            LAYER_REPOSITORY_PROJECT_OVERRIDE.to_string(),
        ],
        non_authoritative_projection: LAYER_TASK_CONTEXT_PROJECTION.to_string(),
        offline_usable: org_overlay.status.offline_usable,
        organization_overlay: org_overlay.status.clone(),
        layers,
        directives,
        conflicts,
        task_projection_isolation_enforced: true,
    };

    Ok(ResolvedAuthorityResult {
        chain,
        org_overlay,
        effective_bodies,
        evidence: evidence_out,
    })
}

pub fn import_overlay_bundle(
    repo_root: &Path,
    bundle_file: &Path,
) -> Result<OrganizationOverlayStatus, DecapodError> {
    let bytes = fs::read(bundle_file).map_err(DecapodError::IoError)?;
    let config_path = repo_root.join(".decapod").join("config.toml");
    let cfg = if config_path.exists() {
        crate::cli::DecapodProjectConfig::load(repo_root)?
    } else {
        crate::cli::DecapodProjectConfig::default()
    };
    let overlay_cfg = cfg
        .context
        .organization_overlay
        .as_ref()
        .cloned()
        .unwrap_or_default();
    let dest = repo_root.join(
        overlay_cfg
            .imported_bundle_path
            .as_deref()
            .unwrap_or(IMPORTED_ORG_OVERLAY_REL_PATH),
    );
    let (mut status, parsed) = verify_and_classify_bundle(
        &overlay_cfg,
        "imported_bundle",
        &dest,
        &bytes,
        now_unix_seconds(),
    );
    let mut bundle = parsed.ok_or_else(|| {
        DecapodError::ValidationError(
            status
                .diagnostic
                .clone()
                .unwrap_or_else(|| "ORG_OVERLAY_IMPORT_REJECTED".to_string()),
        )
    })?;
    if matches!(
        status.trust_state,
        OverlayTrustState::Tampered | OverlayTrustState::Invalid
    ) {
        return Err(DecapodError::ValidationError(
            status
                .diagnostic
                .clone()
                .unwrap_or_else(|| "ORG_OVERLAY_IMPORT_REJECTED".to_string()),
        ));
    }
    if status.freshness_state == OverlayFreshnessState::Stale && !status.offline_usable {
        return Err(DecapodError::ValidationError(
            status.diagnostic.clone().unwrap_or_else(|| {
                "ORG_OVERLAY_STALE: refusing to import expired overlay".to_string()
            }),
        ));
    }
    bundle.offline_cached = true;
    // Preserve bundle_sha256 only if offline_cached did not change signed payload;
    // keep the raw imported bytes verbatim so digest & Ed25519 signature remain verifiable offline!
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }
    fs::write(&dest, &bytes).map_err(DecapodError::IoError)?;
    status.freshness_state = if status.freshness_state == OverlayFreshnessState::Stale {
        OverlayFreshnessState::Stale
    } else {
        OverlayFreshnessState::OfflineCached
    };
    Ok(status)
}

pub fn clear_imported_overlay(repo_root: &Path) -> Result<bool, DecapodError> {
    let config_path = repo_root.join(".decapod").join("config.toml");
    let cfg = if config_path.exists() {
        crate::cli::DecapodProjectConfig::load(repo_root)?
    } else {
        crate::cli::DecapodProjectConfig::default()
    };
    let rel = cfg
        .context
        .organization_overlay
        .as_ref()
        .and_then(|o| o.imported_bundle_path.as_deref())
        .unwrap_or(IMPORTED_ORG_OVERLAY_REL_PATH);
    let path = repo_root.join(rel);
    if path.exists() {
        fs::remove_file(&path).map_err(DecapodError::IoError)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub fn private_directive_ids(chain: &AuthorityChainProvenance) -> BTreeSet<String> {
    chain
        .directives
        .iter()
        .filter(|d| d.visibility == DirectiveVisibility::OrganizationPrivate)
        .map(|d| d.directive_id.clone())
        .collect()
}

pub fn private_directive_bodies(loaded: &LoadedOrganizationOverlay) -> Vec<String> {
    let Some(bundle) = &loaded.bundle else {
        return Vec::new();
    };
    bundle
        .directives
        .iter()
        .filter(|d| d.visibility == DirectiveVisibility::OrganizationPrivate)
        .map(|d| d.body.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use tempfile::tempdir;

    fn encode_hex(bytes: &[u8]) -> String {
        let mut out = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }

    fn signed_bundle(
        org_id: &str,
        directives: Vec<OrganizationDirective>,
        issued_at_unix: u64,
        expires_at_unix: Option<u64>,
    ) -> (OrganizationOverlayBundle, String) {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let pk_hex = encode_hex(keypair.public_key().as_ref());

        let mut bundle = OrganizationOverlayBundle {
            schema_version: ORG_OVERLAY_SCHEMA_VERSION.to_string(),
            org_id: org_id.to_string(),
            org_name: Some("Acme Security Org".to_string()),
            version: "2026.11.21".to_string(),
            source: "org-registry://acme/constitution".to_string(),
            issued_at_unix,
            expires_at_unix,
            offline_cached: false,
            directives,
            bundle_sha256: String::new(),
            signature_hex: None,
            signer_key_id: Some("org-key-1".to_string()),
        }
        .with_recomputed_digest()
        .unwrap();

        let payload = bundle.canonical_payload_bytes().unwrap();
        let sig = keypair.sign(&payload);
        bundle.signature_hex = Some(encode_hex(sig.as_ref()));
        (bundle, pk_hex)
    }

    fn write_config(repo: &Path, body: &str) {
        fs::create_dir_all(repo.join(".decapod")).unwrap();
        let header = "schema_version = \"1.0.0\"\n[init]\ndiagram_style = \"mermaid\"\nentrypoints = []\n[repo]\nbackend = \"local\"\nproduct_summary = \"Org overlay unit fixture\"\narchitecture_direction = \"cli\"\ncontainer_workspaces = false\n[custody]\nisolation = \"worktree\"\n";
        fs::write(
            repo.join(".decapod/config.toml"),
            format!("{header}\n{body}"),
        )
        .unwrap();
    }

    fn write_override(repo: &Path, directive: &str, body: &str) {
        fs::create_dir_all(repo.join(".decapod")).unwrap();
        fs::write(
            repo.join(".decapod/OVERRIDE.md"),
            format!(
                "<!-- CHANGES ARE NOT PERMITTED ABOVE THIS LINE -->\n### {directive}\n{body}\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn org_overlay_non_overridable_fails_closed_when_project_attempts_override() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path();
        let (bundle, pk_hex) = signed_bundle(
            "org_acme",
            vec![OrganizationDirective {
                directive_id: "core/DECAPOD".to_string(),
                overridability: DirectiveOverridability::NonOverridable,
                visibility: DirectiveVisibility::Shared,
                body: "Organization control: mandatory CI proof gate.".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            }],
            1_700_000_000,
            Some(4_000_000_000),
        );
        let overlay_path = repo.join(".decapod/org-overlay.json");
        fs::create_dir_all(repo.join(".decapod")).unwrap();
        fs::write(&overlay_path, serde_json::to_vec_pretty(&bundle).unwrap()).unwrap();
        write_config(
            repo,
            &format!(
                "[context.organization_overlay]\npath = \".decapod/org-overlay.json\"\norg_id = \"org_acme\"\nsha256 = \"{}\"\npublic_key_hex = \"{}\"\n",
                bundle.bundle_sha256, pk_hex
            ),
        );
        write_override(repo, "core/DECAPOD", "Project wants to waive CI gate.");

        let err = resolve_authority_chain_with_now(repo, 1_750_000_000).unwrap_err();
        assert!(
            err.to_string()
                .contains("ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT")
        );
    }

    #[test]
    fn org_overlay_extend_only_allows_addition_and_rejects_weakening() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path();
        let (bundle, pk_hex) = signed_bundle(
            "org_acme",
            vec![OrganizationDirective {
                directive_id: "core/DEMANDS".to_string(),
                overridability: DirectiveOverridability::ExtendOnly,
                visibility: DirectiveVisibility::Shared,
                body: "Organization control: enforce audit trail.".to_string(),
                required_terms: vec!["audit trail".to_string()],
                prohibited_weakening_terms: vec![],
                rationale: None,
            }],
            1_700_000_000,
            Some(4_000_000_000),
        );
        fs::create_dir_all(repo.join(".decapod")).unwrap();
        fs::write(
            repo.join(".decapod/org-overlay.json"),
            serde_json::to_vec_pretty(&bundle).unwrap(),
        )
        .unwrap();
        write_config(
            repo,
            &format!(
                "[context.organization_overlay]\npath = \".decapod/org-overlay.json\"\norg_id = \"org_acme\"\npublic_key_hex = \"{}\"\n",
                pk_hex
            ),
        );

        write_override(
            repo,
            "core/DEMANDS",
            "Additional repository rule: keep audit trail retention for 90 days.",
        );
        let ok = resolve_authority_chain_with_now(repo, 1_750_000_000).unwrap();
        assert_eq!(ok.chain.directives.len(), 1);
        assert_eq!(
            ok.chain.directives[0].precedence_decision,
            "organization_extended_by_project"
        );
        assert!(ok.effective_bodies["core/DEMANDS"].contains("enforce audit trail"));
        assert!(ok.effective_bodies["core/DEMANDS"].contains("90 days"));

        write_override(
            repo,
            "core/DEMANDS",
            "Project amendment: waive audit trail checks for quick hotfixes.",
        );
        let err = resolve_authority_chain_with_now(repo, 1_750_000_000).unwrap_err();
        assert!(
            err.to_string()
                .contains("ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED")
        );
    }

    #[test]
    fn org_overlay_tampered_and_stale_and_unverified_org_id_fail_closed() {
        let tmp = tempdir().unwrap();
        let repo = tmp.path();

        // Org ID alone must never grant authority.
        write_config(
            repo,
            "[context.organization_overlay]\norg_id = \"org_acme\"\n",
        );
        let err = resolve_authority_chain_with_now(repo, 1_750_000_000).unwrap_err();
        assert!(
            err.to_string()
                .contains("ORG_OVERLAY_UNVERIFIED_ORG_IDENTITY")
        );

        // Tampered digest / signature fails closed.
        let (mut bundle, pk_hex) = signed_bundle(
            "org_acme",
            vec![OrganizationDirective {
                directive_id: "core/DECAPOD".to_string(),
                overridability: DirectiveOverridability::Overridable,
                visibility: DirectiveVisibility::OrganizationPrivate,
                body: "Secret internal rule".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            }],
            1_700_000_000,
            Some(1_800_000_000),
        );
        bundle.directives[0].body = "Tampered body".to_string();
        fs::write(
            repo.join(".decapod/org-overlay.json"),
            serde_json::to_vec_pretty(&bundle).unwrap(),
        )
        .unwrap();
        write_config(
            repo,
            &format!(
                "[context.organization_overlay]\npath = \".decapod/org-overlay.json\"\norg_id = \"org_acme\"\npublic_key_hex = \"{}\"\n",
                pk_hex
            ),
        );
        let err = resolve_authority_chain_with_now(repo, 1_750_000_000).unwrap_err();
        assert!(err.to_string().contains("ORG_OVERLAY_TAMPERED"));

        // Valid signature but stale fails closed when strict.
        let (valid_bundle, pk_hex) = signed_bundle(
            "org_acme",
            vec![OrganizationDirective {
                directive_id: "core/DECAPOD".to_string(),
                overridability: DirectiveOverridability::Overridable,
                visibility: DirectiveVisibility::Shared,
                body: "Valid body".to_string(),
                required_terms: vec![],
                prohibited_weakening_terms: vec![],
                rationale: None,
            }],
            1_700_000_000,
            Some(1_800_000_000),
        );
        fs::write(
            repo.join(".decapod/org-overlay.json"),
            serde_json::to_vec_pretty(&valid_bundle).unwrap(),
        )
        .unwrap();
        write_config(
            repo,
            &format!(
                "[context.organization_overlay]\npath = \".decapod/org-overlay.json\"\norg_id = \"org_acme\"\npublic_key_hex = \"{}\"\n",
                pk_hex
            ),
        );
        let err = resolve_authority_chain_with_now(repo, 1_900_000_000).unwrap_err();
        assert!(err.to_string().contains("ORG_OVERLAY_STALE"));
    }
}

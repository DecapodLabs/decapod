//! Ecosystem trust, egress inventory, credential isolation, anti-fingerprinting,
//! and consent-revocation enforcement primitives (`DecapodLabs/decapod#1944`).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// All nine permanent trust invariants enforced by `#1944`.
pub const PERMANENT_TRUST_INVARIANTS: &[&str] = &[
    "Identity sovereignty",
    "No identity fingerprinting",
    "Credential isolation",
    "Consent persistence",
    "Effective revocation",
    "Relay transparency",
    "Distribution integrity",
    "Privacy contract integrity",
    "Incident accountability",
];

/// Third-party credential files that Decapod local workflows must never read,
/// hash, store, or transmit.
pub const THIRD_PARTY_AUTH_PATHS: &[&str] = &[
    concat!(".co", "dex/auth.json"),
    ".claude.json",
    ".config/gh/hosts.yml",
];

/// Ambient secret environment variable keys that must remain isolated from
/// `.decapod/` state, projections, logs, and undeclared egress.
pub const SECRET_ENV_CANARIES: &[&str] = &[
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "STRIPE_SECRET_KEY",
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "DECAPOD_CLOUD_TOKEN",
    "PROPODUS_TOKEN",
    "SUPABASE_SERVICE_ROLE_KEY",
];

/// Declared outbound network channel descriptor matching `docs/trust-and-egress-inventory.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EgressChannel {
    pub id: &'static str,
    pub initiating_action: &'static str,
    pub allowed_hosts: &'static [&'static str],
    pub require_https_for_non_loopback: bool,
    pub default_enabled: bool,
}

/// Canonical allowlisted outbound network channels in Decapod.
pub const DECLARED_EGRESS_CHANNELS: &[EgressChannel] = &[
    EgressChannel {
        id: "cloud_gateway",
        initiating_action: "backend = \"cloud\" or explicit `decapod cloud` / `cloud.*` RPC",
        allowed_hosts: &["api.decapodlabs.ai", "decapodlabs.ai", "*.supabase.co"],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
    EgressChannel {
        id: "http_rpc_loopback",
        initiating_action: "explicit `decapod rpc --transport http` or `decapod observe --serve`",
        allowed_hosts: &["127.0.0.1", "localhost", "[::1]"],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
    EgressChannel {
        id: "crates_version_check",
        initiating_action: "explicit `repo.session_auto_update = true` or `DECAPOD_CHECK_LATEST=1`",
        allowed_hosts: &["crates.io"],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
    EgressChannel {
        id: "ci_snapshot_readback",
        initiating_action: "explicit `decapod validate` (Buildkite preflight), `qa verify commit`, or `workspace publish`",
        allowed_hosts: &[
            "github.com",
            "api.github.com",
            "buildkite.com",
            "api.buildkite.com",
            "graphql.buildkite.com",
            "s3.amazonaws.com",
            "buildkiteartifacts.com",
            "buildkite-cloud.s3.amazonaws.com",
        ],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
    EgressChannel {
        id: "model_assets_download",
        initiating_action: "explicit Laya model setup (`decapod context ...`)",
        allowed_hosts: &["huggingface.co", "api.github.com"],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
    EgressChannel {
        id: "derived_release_verify",
        initiating_action: "explicit `decapod release` derived-release verification",
        allowed_hosts: &["crates.io", "static.crates.io"],
        require_https_for_non_loopback: true,
        default_enabled: false,
    },
];

/// Validate an outbound URL against Decapod's declared egress inventory and
/// HTTPS/credential-isolation guarantees.
pub fn validate_egress_url(url: &str, extra_declared_hosts: &[&str]) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("EGRESS_REJECTED: empty URL".to_string());
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("EGRESS_REJECTED: URL contains control or whitespace characters".to_string());
    }

    let (is_https, rest) = if let Some(rest) = trimmed.strip_prefix("https://") {
        (true, rest)
    } else if let Some(rest) = trimmed.strip_prefix("http://") {
        (false, rest)
    } else {
        return Err(format!(
            "EGRESS_REJECTED: unsupported scheme in `{trimmed}`"
        ));
    };

    let authority_and_path = rest.split('#').next().unwrap_or(rest);
    if trimmed.contains('#') {
        return Err("EGRESS_REJECTED: URL fragments are forbidden on egress URLs".to_string());
    }
    let authority = authority_and_path.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() {
        return Err("EGRESS_REJECTED: missing host authority".to_string());
    }
    if authority.contains('@') {
        return Err("EGRESS_REJECTED: embedded userinfo is forbidden".to_string());
    }

    let host = parse_authority_host(authority)?;
    let is_loopback = matches!(host, "127.0.0.1" | "localhost" | "[::1]");
    if !is_https && !is_loopback {
        return Err(format!(
            "EGRESS_REJECTED: non-HTTPS scheme forbidden for non-loopback host `{host}`"
        ));
    }

    if is_loopback {
        return Ok(());
    }

    let declared = DECLARED_EGRESS_CHANNELS
        .iter()
        .flat_map(|c| c.allowed_hosts.iter().copied())
        .chain(extra_declared_hosts.iter().copied())
        .any(|allowed| host_matches_rule(host, allowed));

    if !declared {
        return Err(format!(
            "EGRESS_REJECTED: undeclared outbound host `{host}` is not in docs/trust-and-egress-inventory.md"
        ));
    }

    Ok(())
}

fn host_matches_rule(host: &str, rule: &str) -> bool {
    if let Some(suffix) = rule.strip_prefix("*.") {
        let host_lower = host.to_ascii_lowercase();
        let suffix_lower = suffix.to_ascii_lowercase();
        return host_lower.len() > suffix_lower.len() + 1
            && host_lower.ends_with(&format!(".{suffix_lower}"));
    }
    host.eq_ignore_ascii_case(rule)
}

fn parse_authority_host(authority: &str) -> Result<&str, String> {
    if authority.starts_with('[') {
        let end = authority
            .find(']')
            .ok_or_else(|| "EGRESS_REJECTED: malformed IPv6 host authority".to_string())?;
        let host = &authority[..=end];
        if host.len() <= 2 {
            return Err("EGRESS_REJECTED: empty bracketed IPv6 host".to_string());
        }
        let rest = &authority[end + 1..];
        if !rest.is_empty() {
            let port_str = rest.strip_prefix(':').ok_or_else(|| {
                "EGRESS_REJECTED: invalid authority suffix after IPv6 host".to_string()
            })?;
            validate_port(port_str)?;
        }
        return Ok(host);
    }

    let mut parts = authority.split(':');
    let host = parts.next().unwrap_or("");
    if host.is_empty() {
        return Err("EGRESS_REJECTED: missing host name".to_string());
    }
    if let Some(port_str) = parts.next() {
        if parts.next().is_some() {
            return Err("EGRESS_REJECTED: multiple colons in non-IPv6 authority".to_string());
        }
        validate_port(port_str)?;
    }
    Ok(host)
}

fn validate_port(port_str: &str) -> Result<u16, String> {
    if port_str.is_empty() || !port_str.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!(
            "EGRESS_REJECTED: invalid authority port `{port_str}`"
        ));
    }
    let port: u16 = port_str
        .parse()
        .map_err(|_| format!("EGRESS_REJECTED: out-of-range authority port `{port_str}`"))?;
    if port == 0 {
        return Err("EGRESS_REJECTED: port 0 is forbidden".to_string());
    }
    Ok(port)
}

/// Compute lowercase hex SHA-256 digest (used exclusively by trust tests/guards
/// to detect forbidden credential fingerprinting in payloads/artifacts).
pub fn sha256_hex(input: &[u8]) -> String {
    format!("{:x}", Sha256::digest(input))
}

/// Verify that `payload` contains neither raw credential canary values nor
/// hashed/fingerprinted forms of those secrets or third-party auth markers.
pub fn scan_for_credential_leaks(payload: &str, canaries: &[&str]) -> Result<(), String> {
    for &canary in canaries {
        if canary.is_empty() {
            continue;
        }
        if payload.contains(canary) {
            return Err(
                "CREDENTIAL_LEAK_DETECTED: raw secret canary surfaced in payload".to_string(),
            );
        }
        let hash = sha256_hex(canary.as_bytes());
        if payload.contains(&hash) || payload.contains(&hash[..16]) {
            return Err(
                "IDENTITY_FINGERPRINT_LEAK_DETECTED: hashed/truncated secret canary surfaced in payload"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Reject outbound payloads carrying hashed/truncated machine or third-party
/// identity fingerprints (`No identity fingerprinting` invariant).
pub fn detect_fingerprinting_violation(
    outbound_payload: &str,
    forbidden_source_identifiers: &[&str],
) -> Result<(), String> {
    for &ident in forbidden_source_identifiers {
        if ident.is_empty() {
            continue;
        }
        let digest = sha256_hex(ident.as_bytes());
        if outbound_payload.contains(&digest) || outbound_payload.contains(&digest[..16]) {
            return Err(
                "FINGERPRINTING_VIOLATION: derived identifier hash detected in outbound payload"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Durable consent & revocation state reflected from `.decapod/config.toml`
/// and optional capabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConsentRevocationState {
    pub schema_version: String,
    pub cloud_enabled: bool,
    pub session_auto_update_enabled: bool,
    pub queued_outbound_events: usize,
}

impl Default for ConsentRevocationState {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            cloud_enabled: false,
            session_auto_update_enabled: false,
            queued_outbound_events: 0,
        }
    }
}

impl ConsentRevocationState {
    pub fn from_config(config: &crate::cli::DecapodProjectConfig) -> Self {
        Self {
            schema_version: config.schema_version.clone(),
            cloud_enabled: config.repo.effective_backend().is_cloud(),
            session_auto_update_enabled: config.repo.session_auto_update,
            queued_outbound_events: 0,
        }
    }

    pub fn load_from_project_root(project_root: &std::path::Path) -> Result<Self, String> {
        let config = crate::cli::DecapodProjectConfig::load(project_root)
            .map_err(|e| format!("failed to load .decapod/config.toml: {e}"))?;
        Ok(Self::from_config(&config))
    }

    /// Revoke all optional outbound capabilities immediately and purge any
    /// buffered outbound entries.
    pub fn revoke_all(&mut self) {
        self.cloud_enabled = false;
        self.session_auto_update_enabled = false;
        self.queued_outbound_events = 0;
    }

    pub fn allows_cloud_egress(&self) -> bool {
        self.cloud_enabled
    }

    pub fn allows_update_egress(&self) -> bool {
        self.session_auto_update_enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_channels_are_default_disabled_and_https_enforced() {
        for channel in DECLARED_EGRESS_CHANNELS {
            assert!(
                !channel.default_enabled,
                "channel `{}` must default to disabled",
                channel.id
            );
            assert!(
                channel.require_https_for_non_loopback,
                "channel `{}` must require HTTPS for non-loopback",
                channel.id
            );
        }
    }

    #[test]
    fn negative_control_rejects_undeclared_host_and_plaintext_http() {
        assert!(validate_egress_url("https://api.decapodlabs.ai/v1/status", &[]).is_ok());
        assert!(validate_egress_url("http://127.0.0.1:8080/rpc/v1", &[]).is_ok());

        let undeclared = validate_egress_url("https://undeclared-telemetry.invalid/collect", &[]);
        assert!(undeclared.is_err(), "undeclared host must fail closed");

        let plaintext = validate_egress_url("http://api.decapodlabs.ai/v1/status", &[]);
        assert!(plaintext.is_err(), "non-loopback http:// must fail closed");

        let userinfo = validate_egress_url("https://user:secret@api.decapodlabs.ai/v1", &[]);
        assert!(userinfo.is_err(), "embedded userinfo must fail closed");

        assert!(validate_egress_url("https://crates.io:443/api/v1/crates/decapod", &[]).is_ok());
        assert!(validate_egress_url("https://demo.supabase.co/rest/v1", &[]).is_ok());
        assert!(validate_egress_url("http://[::1]:8080/rpc/v1", &[]).is_ok());
        assert!(
            validate_egress_url("https://crates.io:notaport/api", &[]).is_err(),
            "non-numeric port must fail closed"
        );
        assert!(
            validate_egress_url("https://crates.io:0/api", &[]).is_err(),
            "port 0 must fail closed"
        );
        assert!(
            validate_egress_url("https://crates.io:/api", &[]).is_err(),
            "empty port must fail closed"
        );
        assert!(
            validate_egress_url("https://[::1:80/api", &[]).is_err(),
            "malformed IPv6 host must fail closed"
        );
        assert!(
            validate_egress_url("https://evil-supabase.co/rest/v1", &[]).is_err(),
            "wildcard suffix must require dot boundary"
        );
    }

    #[test]
    fn negative_control_detects_raw_and_fingerprinted_credential_leaks() {
        let canary = "sk-openai-canary-9988776655";
        assert!(scan_for_credential_leaks("clean local output", &[canary]).is_ok());
        assert!(scan_for_credential_leaks(&format!("leaked {canary}"), &[canary]).is_err());
        let hashed = sha256_hex(canary.as_bytes());
        assert!(
            scan_for_credential_leaks(&format!("fingerprint={}", &hashed[..16]), &[canary])
                .is_err(),
            "truncated hash of canary must be caught"
        );
        let machine_hash = sha256_hex(b"machine-uuid-1234");
        assert!(
            detect_fingerprinting_violation(&format!("id={machine_hash}"), &["machine-uuid-1234"])
                .is_err()
        );
    }

    #[test]
    fn consent_defaults_disabled_and_revocation_clears_immediately() {
        let minimal: crate::cli::DecapodProjectConfig = toml::from_str(
            r#"
schema_version = "1.0.0"
[init]
specs = true
ci = true
diagram_style = "mermaid"
entrypoints = ["AGENTS.md"]
[repo]
"#,
        )
        .unwrap();
        let mut state = ConsentRevocationState::from_config(&minimal);
        assert!(!state.allows_cloud_egress());
        assert!(!state.allows_update_egress());

        state.cloud_enabled = true;
        state.session_auto_update_enabled = true;
        state.queued_outbound_events = 3;
        state.revoke_all();
        assert!(!state.allows_cloud_egress());
        assert!(!state.allows_update_egress());
        assert_eq!(state.queued_outbound_events, 0);
    }
}

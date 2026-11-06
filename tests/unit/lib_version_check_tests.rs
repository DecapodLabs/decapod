//! Regression tests for #1533: the crates.io version check must send an
//! identifying User-Agent (crates.io answers HTTP 403 without one) and must
//! parse the top-level `crate.max_version` field, and the automatic
//! `cargo install` on `session acquire` must stay an explicit opt-in.

use super::*;

#[test]
fn parse_crates_max_version_reads_crate_field() {
    // Real shape of a 200 response from /api/v1/crates/decapod: the version
    // lives under `crate.max_version`; there is no top-level `version.num`.
    let body = br#"{"categories":[],"crate":{"max_version":"0.126.3","newest_version":"0.126.3"},"keywords":[],"versions":[]}"#;
    assert_eq!(parse_crates_max_version(body).as_deref(), Some("0.126.3"));
}

#[test]
fn parse_crates_max_version_rejects_legacy_version_num_shape() {
    let body = br#"{"version":{"num":"0.126.3"}}"#;
    assert_eq!(parse_crates_max_version(body), None);
}

#[test]
fn parse_crates_max_version_rejects_garbage() {
    assert_eq!(
        parse_crates_max_version(b"<html>403 Forbidden</html>"),
        None
    );
    assert_eq!(parse_crates_max_version(b""), None);
}

#[test]
fn crates_io_user_agent_identifies_client() {
    let ua = crates_io_user_agent();
    assert!(
        ua.starts_with("decapod/"),
        "user agent must name the client: {ua}"
    );
    assert!(
        ua.contains("github.com/DecapodLabs/decapod"),
        "user agent must identify the project: {ua}"
    );
}

#[test]
fn session_auto_update_defaults_to_opt_in_false() {
    // Without the key, `session acquire` must only report a newer release,
    // never run `cargo install decapod` (#1533).
    let config: cli::DecapodProjectConfig = toml::from_str(
        r#"
schema_version = "1.0.0"

[init]
specs = true
ci = true
diagram_style = "mermaid"
entrypoints = ["src/main.rs"]

[repo]
"#,
    )
    .expect("minimal config must parse");
    assert!(!config.repo.session_auto_update);

    let opted_in: cli::DecapodProjectConfig = toml::from_str(
        r#"
schema_version = "1.0.0"

[init]
specs = true
ci = true
diagram_style = "mermaid"
entrypoints = ["src/main.rs"]

[repo]
session_auto_update = true
"#,
    )
    .expect("opt-in config must parse");
    assert!(opted_in.repo.session_auto_update);
}

// ---------------------------------------------------------------------------
// Behavioral regression tests for the #1533 follow-up: version detection may
// inform execution but must never independently authorize a toolchain
// mutation. `session acquire` is the only path that can install, and only
// behind the explicit repo.session_auto_update opt-in.
// ---------------------------------------------------------------------------

use crate::core::rpc::{VersionCheckStatus, compute_is_latest};

#[test]
fn session_update_opt_in_authorizes_install_of_newer_release() {
    assert_eq!(
        decide_session_update("0.126.5", Some("0.127.0"), true),
        SessionUpdateDecision::Install
    );
}

#[test]
fn session_update_opt_out_never_installs() {
    // Default (session_auto_update = false): a newer release is only
    // reported; the decision must not be Install.
    assert_eq!(
        decide_session_update("0.126.5", Some("0.127.0"), false),
        SessionUpdateDecision::ReportOnly
    );
    assert_ne!(
        decide_session_update("0.126.5", Some("0.127.0"), false),
        SessionUpdateDecision::Install
    );
}

#[test]
fn session_update_unavailable_registry_never_authorizes_mutation() {
    // Registry unreachable: currency is unknown, so nothing may be
    // installed — even with the opt-in enabled.
    assert_eq!(
        decide_session_update("0.126.5", None, true),
        SessionUpdateDecision::SkipUnknownRegistry
    );
    assert_eq!(
        decide_session_update("0.126.5", None, false),
        SessionUpdateDecision::SkipUnknownRegistry
    );
    assert_ne!(
        decide_session_update("0.126.5", None, true),
        SessionUpdateDecision::Install
    );
}

#[test]
fn session_update_current_version_is_up_to_date() {
    assert_eq!(
        decide_session_update("0.126.5", Some("0.126.5"), true),
        SessionUpdateDecision::UpToDate
    );
    assert_eq!(
        decide_session_update("0.126.5", Some("0.126.4"), false),
        SessionUpdateDecision::UpToDate
    );
}

#[test]
fn is_latest_true_only_when_registry_confirms_running_version() {
    assert!(compute_is_latest(
        VersionCheckStatus::Ok,
        Some("0.126.5"),
        "0.126.5"
    ));
    // Registry reached but reports a newer release: not latest.
    assert!(!compute_is_latest(
        VersionCheckStatus::Ok,
        Some("0.127.0"),
        "0.126.5"
    ));
    // Registry unreachable: must NOT report is_latest true, even though the
    // running version is the only version we know (#1533 ambiguity fix).
    assert!(!compute_is_latest(
        VersionCheckStatus::Unavailable,
        None,
        "0.126.5"
    ));
    assert!(!compute_is_latest(
        VersionCheckStatus::Unavailable,
        Some("0.126.5"),
        "0.126.5"
    ));
}

#[test]
fn version_check_status_serializes_unambiguously() {
    assert_eq!(
        serde_json::to_string(&VersionCheckStatus::Ok).unwrap(),
        "\"ok\""
    );
    assert_eq!(
        serde_json::to_string(&VersionCheckStatus::Unavailable).unwrap(),
        "\"unavailable\""
    );
}

#[test]
fn generated_entrypoints_never_invoke_cargo_install() {
    // The generated AGENTS/CLAUDE/CODEX/GEMINI contracts must not contain
    // any auto-install bootstrap; `decapod session acquire` is the only
    // update path they may name (#1533 follow-up).
    for surface in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let body = crate::core::entrypoint_integrity::render_entrypoint(surface)
            .expect("every governed entrypoint must render");
        assert!(
            !body.contains("cargo install"),
            "{surface} must not invoke cargo install: version detection never authorizes a toolchain change"
        );
        assert!(
            !body.contains("is_latest\":true"),
            "{surface} must not gate execution on is_latest"
        );
        assert!(
            body.contains("decapod session acquire"),
            "{surface} must route updates through decapod session acquire"
        );
        assert!(
            body.contains("never authorizes a toolchain change"),
            "{surface} must state the no-independent-authorization rule"
        );
    }
}

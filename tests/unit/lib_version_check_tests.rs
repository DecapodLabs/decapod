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

use std::fs;
use std::process::{Command, Output};

fn run_decapod(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("failed to execute decapod")
}

#[test]
fn clap_syntax_failures_use_status_two() {
    let output = run_decapod(&["--definitely-not-a-command"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error:"));
}

#[test]
fn domain_errors_use_status_one_and_preserve_display_context() {
    let output = run_decapod(&[
        "docs",
        "show",
        "docs/agent/does-not-exist.md",
        "--source",
        "embedded",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Not found:"));
}

#[test]
fn error_reference_matches_the_current_error_contract() {
    let docs = fs::read_to_string("docs/book/src/reference/errors.md")
        .expect("failed to read the error reference");

    for variant in [
        "RusqliteError",
        "IoError",
        "DatabaseInitializationError",
        "PathError",
        "EnvVarError",
        "ValidationError",
        "NotFound",
        "NotImplemented",
        "Config",
        "ContextPackError",
        "SessionError",
    ] {
        assert!(
            docs.contains(variant),
            "error reference is missing DecapodError::{variant}"
        );
    }

    assert!(docs.contains("| 1 | Decapod operation failure |"));
    assert!(docs.contains("| 2 | CLI syntax failure |"));
    assert!(docs.contains("| 127 | Shell command not found |"));
    assert!(docs.contains("Result<T, DecapodError>"));
    assert!(!docs.contains("Conflict(\""));
    assert!(!docs.contains("RiskGate(\""));
}

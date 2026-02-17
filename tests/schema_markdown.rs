use std::process::Command;
use tempfile::TempDir;

#[test]
fn schema_markdown_format_is_rendered() {
    let tmp = TempDir::new().expect("tempdir");
    let dir = tmp.path();

    let init = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["init", "--force"])
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to initialize decapod workspace");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["session", "acquire"])
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to acquire session");
    assert!(
        session.status.success(),
        "session acquire failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(dir)
        .args(["data", "schema", "--format", "md", "--deterministic"])
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("failed to execute decapod");

    assert!(
        output.status.success(),
        "schema command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# Decapod Schema"));
    assert!(
        !stdout.contains("Markdown schema format not yet implemented"),
        "markdown output should not fallback to JSON warning"
    );
}

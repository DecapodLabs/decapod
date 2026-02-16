use std::process::Command;

#[test]
fn schema_markdown_format_is_rendered() {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["data", "schema", "--format", "md", "--deterministic"])
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

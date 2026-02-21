use std::path::PathBuf;
use std::process::Command;

#[test]
fn claude_workflow_example_contains_required_ops() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow = std::fs::read_to_string(root.join("examples/claude_code_workflow.md"))
        .expect("read claude workflow example");
    assert!(workflow.contains("decapod session init"));
    assert!(workflow.contains("decapod validate"));
    assert!(workflow.contains("decapod handshake"));
    assert!(workflow.contains("decapod workspace publish"));
}

#[test]
fn release_check_surface_exists_and_runs() {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["release", "check"])
        .output()
        .expect("run release check");
    assert!(
        output.status.success(),
        "release check failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("\"status\":\"ok\""),
        "release check should emit ok envelope"
    );
}

use std::path::PathBuf;
use std::process::Command;

#[test]
fn python_and_typescript_examples_parse_fixture_envelopes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = root.join("examples/fixtures/validate_envelope.json");

    let py = Command::new("python3")
        .arg(root.join("examples/python_validate_demo.py"))
        .arg("--fixture")
        .arg(&fixture)
        .output()
        .expect("run python demo");
    assert!(
        py.status.success(),
        "python demo failed: {}",
        String::from_utf8_lossy(&py.stderr)
    );
    let py_out = String::from_utf8_lossy(&py.stdout);
    assert!(py_out.contains("\"demo\": \"python\""));
    assert!(py_out.contains("\"status\": \"ok\""));

    let ts = Command::new("node")
        .arg(root.join("examples/ts_validate_demo.js"))
        .arg("--fixture")
        .arg(&fixture)
        .output()
        .expect("run ts demo");
    assert!(
        ts.status.success(),
        "ts demo failed: {}",
        String::from_utf8_lossy(&ts.stderr)
    );
    let ts_out = String::from_utf8_lossy(&ts.stdout);
    assert!(ts_out.contains("\"demo\": \"typescript\""));
    assert!(ts_out.contains("\"status\": \"ok\""));
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

//! Exercise the production maintenance entrypoint, not a second lock rewriter.
#![cfg(feature = "build-tools")]

use std::{fs, process::Command};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"decapod\"\nversion = \"0.121.0\"\n",
    )
    .unwrap();
    let worker = root.path().join("assets/tools/vega-native");
    fs::create_dir_all(&worker).unwrap();
    fs::write(worker.join("Cargo.toml"), "[package]\nname = \"vega\"\n").unwrap();
    fs::write(
        worker.join("Cargo.lock"),
        "version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"0.120.1\"\ndependencies = [\n \"pinned-runtime\",\n]\n\n[[package]]\nname = \"pinned-runtime\"\nversion = \"1.0.0\"\nsource = \"git+https://example.invalid/runtime#fixed\"\n",
    )
    .unwrap();
    root
}

#[test]
fn production_entrypoint_projects_only_the_actual_version_and_is_idempotent() {
    let root = fixture();
    let lock = root.path().join("assets/tools/vega-native/Cargo.lock");
    let before = fs::read_to_string(&lock).unwrap();
    for changed in [true, false] {
        let output = Command::new(env!("CARGO_BIN_EXE_decapod-release-native-lock"))
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("native_worker_lock_changed={changed}\n")
        );
        assert_eq!(
            fs::read_to_string(&lock).unwrap(),
            before.replacen("version = \"0.120.1\"", "version = \"0.121.0\"", 1)
        );
    }
}

#[test]
fn production_entrypoint_rejects_arguments_without_mutating_source() {
    let root = fixture();
    let lock = root.path().join("assets/tools/vega-native/Cargo.lock");
    let before = fs::read(&lock).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_decapod-release-native-lock"))
        .current_dir(root.path())
        .arg("--version=0.999.0")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("accepts no arguments"));
    assert_eq!(fs::read(lock).unwrap(), before);
}

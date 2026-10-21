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

/// An opt-in execution diagnostic, never an authenticated production receipt or
/// four-platform packaging proof. Logs persist outside Git even on failure.
#[test]
#[ignore = "requires reviewed installed release tools and public registry access"]
fn actual_release_plz_preparation_in_an_owned_clone() {
    use decapod::core::{
        bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination},
        release_toolchain::ReleaseToolchain,
    };
    use std::{path::PathBuf, time::Duration};

    let output = PathBuf::from(std::env::var_os("DECAPOD_RELEASE_PROBE_OUTPUT").unwrap());
    assert!(output.is_absolute());
    fs::create_dir(&output).unwrap(); // refuse stale output reuse
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .canonicalize()
        .unwrap();
    assert!(!output.starts_with(&source));
    let plz = PathBuf::from(std::env::var_os("DECAPOD_RELEASE_PLZ").unwrap())
        .canonicalize()
        .unwrap();
    let rust_bin = PathBuf::from(std::env::var_os("DECAPOD_RELEASE_RUST_BIN").unwrap())
        .canonicalize()
        .unwrap();
    let pins = ReleaseToolchain::parse(include_str!("../.github/release-tools.toml")).unwrap();
    let home = output.join("home");
    fs::create_dir(&home).unwrap();
    let path = std::env::join_paths([
        rust_bin.as_path(),
        plz.parent().unwrap(),
        std::path::Path::new("/usr/bin"),
        std::path::Path::new("/bin"),
    ])
    .unwrap();
    let started = std::time::Instant::now();
    let budget = Duration::from_secs(900);
    let run = |label: &str, program: &std::path::Path, args: &[&str], cwd: &std::path::Path| {
        let remaining = budget.checked_sub(started.elapsed()).unwrap();
        let capture = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .env_clear()
            .env("PATH", &path)
            .env("HOME", &home)
            .env("CARGO_HOME", home.join("cargo"))
            .env("RUSTC", rust_bin.join("rustc"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .bounded_capture(
                ProcessLimits {
                    timeout: remaining,
                    max_output_bytes: 4 * 1024 * 1024,
                },
                &Cancellation::default(),
            )
            .unwrap();
        fs::write(
            output.join(format!("{label}.stdout")),
            &capture.output.stdout,
        )
        .unwrap();
        fs::write(
            output.join(format!("{label}.stderr")),
            &capture.output.stderr,
        )
        .unwrap();
        assert_eq!(capture.termination, Termination::Exited, "{label}");
        assert!(
            capture.output.status.success(),
            "{label}: see private diagnostic logs"
        );
        String::from_utf8(capture.output.stdout).unwrap()
    };
    let git = std::path::Path::new("/usr/bin/git");
    let head = run("source-head", git, &["rev-parse", "HEAD"], &source);
    let actual_version = run("release-plz-version", &plz, &["--version"], &source);
    assert_eq!(
        actual_version.trim(),
        format!("release-plz {}", pins.release_plz)
    );
    let clone = output.join("source");
    run(
        "clone",
        git,
        &[
            "clone",
            "--no-local",
            "--no-hardlinks",
            source.to_str().unwrap(),
            clone.to_str().unwrap(),
        ],
        &output,
    );
    assert_eq!(run("clone-head", git, &["rev-parse", "HEAD"], &clone), head);
    let manifest_version = || {
        let manifest: toml::Value =
            toml::from_str(&fs::read_to_string(clone.join("Cargo.toml")).unwrap()).unwrap();
        manifest["package"]["version"].as_str().unwrap().to_owned()
    };
    let original_version = manifest_version();
    run(
        "public-origin",
        git,
        &[
            "remote",
            "set-url",
            "origin",
            "https://github.com/DecapodLabs/decapod.git",
        ],
        &clone,
    );
    run(
        "update",
        &plz,
        &[
            "update",
            "--config",
            ".github/release.toml",
            "--manifest-path",
            "Cargo.toml",
            "--release-date",
            &chrono::Utc::now().format("%F").to_string(),
            "--repo-url",
            "https://github.com/DecapodLabs/decapod",
        ],
        &clone,
    );
    let generated_version = manifest_version();
    assert_ne!(
        original_version, generated_version,
        "no actual version transformation observed"
    );
    let helper = std::path::Path::new(env!("CARGO_BIN_EXE_decapod-release-native-lock"));
    run("project-worker-lock", helper, &[], &clone);
    assert_eq!(
        run("idempotent-worker-lock", helper, &[], &clone),
        "native_worker_lock_changed=false\n"
    );
    let changed = run("changed-paths", git, &["diff", "--name-only"], &clone);
    assert!(changed.lines().all(|p| matches!(
        p,
        "Cargo.toml" | "Cargo.lock" | "CHANGELOG.md" | "assets/tools/vega-native/Cargo.lock"
    )));
    let record = serde_json::json!({
        "scope": "nonpublishing_release_preparation_diagnostic",
        "source_head": head.trim(), "release_plz": actual_version.trim(),
        "original_version": original_version, "generated_version": generated_version,
        "release_plz_sha256": decapod::core::derived_release_profile::sha256(&fs::read(plz).unwrap()),
        "changed_paths": changed.lines().collect::<Vec<_>>(),
        "elapsed_ms": started.elapsed().as_millis(),
        "packaging_proven": false, "publication_ready": false
    });
    fs::write(
        output.join("diagnostic.json"),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
}

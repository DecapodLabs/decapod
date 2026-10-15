//! Dormant finite native Vega recipes. Only the independently recognized future
//! workflow contract may add these checks to required readiness inventory.
use super::*;
#[allow(dead_code)]
#[path = "native_vega_model.rs"]
pub mod model;
pub use model::{
    BACKBONE_ID, BACKBONE_REVISION, MODEL_FILES, MODEL_ID, MODEL_REVISION, SOURCE_ID,
    SOURCE_REVISION,
};

pub const SOURCES: &[(&str, &[u8])] = &[
    (
        ".cargo/config.toml",
        include_bytes!("../../.cargo/config.toml"),
    ),
    (
        "assets/tools/source_ci_native_vega.rs",
        include_bytes!("source_ci_native_vega.rs"),
    ),
    (
        "assets/tools/native_vega_driver.rs",
        include_bytes!("native_vega_driver.rs"),
    ),
    (
        "assets/tools/native_vega_model.rs",
        include_bytes!("native_vega_model.rs"),
    ),
    (
        "assets/tools/native_vega_driver/Cargo.toml",
        include_bytes!("native_vega_driver/Cargo.toml"),
    ),
    (
        "assets/tools/native_vega_driver/Cargo.lock",
        include_bytes!("native_vega_driver/Cargo.lock"),
    ),
    (
        "assets/tools/native_metal_probe.m",
        include_bytes!("native_metal_probe.m"),
    ),
    (
        "assets/tools/source_ci/contracts/vega-config.json.fixture",
        include_bytes!("source_ci/contracts/vega-config.json.fixture"),
    ),
    (
        "assets/tools/source_ci/contracts/ci-native.yml.fixture",
        include_bytes!("source_ci/contracts/ci-native.yml.fixture"),
    ),
];
pub const CHECKS: &[Check] = &[
    Check {
        id: "native-vega-gpu",
        workflow: "ci.yml",
        job: "native-vega",
        matrix: None,
        platform: Platform::MacosAarch64,
        capabilities: &["cargo", "rustc:1.96.1", "xcrun"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "native-vega-worker",
        workflow: "ci.yml",
        job: "native-vega",
        matrix: None,
        platform: Platform::MacosAarch64,
        capabilities: &["cargo", "rustc:1.96.1", "xcrun"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "native-vega-model-setup",
        workflow: "ci.yml",
        job: "native-vega",
        matrix: None,
        platform: Platform::MacosAarch64,
        capabilities: &["xcrun", "curl"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
    Check {
        id: "native-vega-inference",
        workflow: "ci.yml",
        job: "native-vega",
        matrix: None,
        platform: Platform::MacosAarch64,
        capabilities: &["xcrun"],
        selection: Selection::Always,
        skipped_for_release_only: true,
    },
];
pub fn is_check(id: &str) -> bool {
    CHECKS.iter().any(|known| known.id == id)
}
pub fn source_files(check: &Check) -> Vec<(&'static str, &'static [u8])> {
    if is_check(check.id) {
        SOURCES.to_vec()
    } else {
        Vec::new()
    }
}
pub fn commands(check: &Check) -> Option<Vec<Invocation>> {
    Some(match check.id {
        "native-vega-gpu" => vec![
            cargo(&[
                "+1.96.1",
                "build",
                "--release",
                "--locked",
                "--manifest-path",
                "assets/tools/native_vega_driver/Cargo.toml",
                "--target-dir",
                "target/source-ci-native-vega/driver-build",
                "--bin",
                "native-vega-driver",
            ]),
            Invocation::new(
                "xcrun",
                &[
                    "--sdk",
                    "macosx",
                    "clang",
                    "-fobjc-arc",
                    "-Wall",
                    "-Wextra",
                    "-framework",
                    "Foundation",
                    "-framework",
                    "Metal",
                    "assets/tools/native_metal_probe.m",
                    "-o",
                    "target/source-ci-native-vega/metal-probe",
                ],
            ),
            Invocation::new(
                "./target/source-ci-native-vega/metal-probe",
                &["target/source-ci-native-vega/gpu.json"],
            ),
        ],
        "native-vega-worker" => vec![
            cargo(&[
                "+1.96.1",
                "build",
                "--locked",
                "--release",
                "--target",
                "aarch64-apple-darwin",
                "--features",
                "vega-native",
                "--bin",
                "decapod",
            ]),
            cargo(&[
                "+1.96.1",
                "build",
                "--locked",
                "--release",
                "--target",
                "aarch64-apple-darwin",
                "--manifest-path",
                "assets/tools/vega-native/Cargo.toml",
                "--target-dir",
                "target/source-ci-native-vega/worker-build",
                "--bin",
                "decapod-vega-infer",
            ]),
            cargo(&[
                "+1.96.1",
                "test",
                "--locked",
                "--release",
                "--target",
                "aarch64-apple-darwin",
                "--features",
                "vega-native",
                "--lib",
                "vega_",
                "--",
                "--nocapture",
            ]),
            cargo(&[
                "+1.96.1",
                "test",
                "--locked",
                "--release",
                "--target",
                "aarch64-apple-darwin",
                "--manifest-path",
                "assets/tools/vega-native/Cargo.toml",
                "--target-dir",
                "target/source-ci-native-vega/worker-build",
                "--bin",
                "decapod-vega-infer",
            ]),
            Invocation::new(
                "./target/source-ci-native-vega/driver-build/release/native-vega-driver",
                &["capture-worker"],
            ),
        ],
        "native-vega-model-setup" => vec![
            // The exact bounded setup CLI is pinned with the future contract.
            Invocation::new(
                "./target/source-ci-native-vega/driver-build/release/native-vega-driver",
                &["setup-model"],
            ),
        ],
        "native-vega-inference" => vec![Invocation::new(
            "./target/source-ci-native-vega/driver-build/release/native-vega-driver",
            &["infer"],
        )],
        _ => return None,
    })
}
pub fn input_files(check: &Check) -> Vec<String> {
    let files: &[&str] = match check.id {
        "native-vega-worker" => &[
            "target/source-ci-native-vega/driver-build/release/native-vega-driver",
            "target/source-ci-native-vega/gpu.json",
        ],
        "native-vega-model-setup" | "native-vega-inference" => &[
            "target/source-ci-native-vega/driver-build/release/native-vega-driver",
            "target/source-ci-native-vega/gpu.json",
            "target/aarch64-apple-darwin/release/decapod",
            "target/source-ci-native-vega/worker-build/aarch64-apple-darwin/release/decapod-vega-infer",
            "target/source-ci-native-vega/worker.json",
        ],
        _ => &[],
    };
    let mut paths: Vec<_> = files.iter().map(|s| s.to_string()).collect();
    if check.id == "native-vega-inference" {
        paths.push("target/source-ci-native-vega/model.json".into());
    }
    paths
}
pub fn artifact_paths(check: &Check) -> Vec<String> {
    let files: &[&str] = match check.id {
        "native-vega-gpu" => &[
            "target/source-ci-native-vega/driver-build/release/native-vega-driver",
            "target/source-ci-native-vega/metal-probe",
            "target/source-ci-native-vega/gpu.json",
        ],
        "native-vega-worker" => &[
            "target/aarch64-apple-darwin/release/decapod",
            "target/source-ci-native-vega/worker-build/aarch64-apple-darwin/release/decapod-vega-infer",
            "target/source-ci-native-vega/worker.json",
        ],
        "native-vega-model-setup" => &[
            "target/source-ci-native-vega/model.json",
            "target/source-ci-native-vega/setup.json",
            "target/source-ci-native-vega/setup.stderr",
        ],
        "native-vega-inference" => &[
            "target/source-ci-native-vega/request.json",
            "target/source-ci-native-vega/response.json",
            "target/source-ci-native-vega/worker.stderr",
            "target/source-ci-native-vega/inference.json",
            "target/source-ci-native-vega/native-proof.json",
        ],
        _ => &[],
    };
    let mut paths: Vec<_> = files.iter().map(|s| s.to_string()).collect();
    if is_check(check.id) && check.id != "native-vega-gpu" {
        paths.push("target/source-ci-native-vega/driver-build/release/native-vega-driver".into());
    }
    paths
}

/// Exact source recognition, never a runner-label inference or a missing-file default.
pub fn contract_enabled(repo: &Path) -> bool {
    let expected = include_bytes!("source_ci/contracts/ci-native.yml.fixture");
    let path = repo.join(".github/workflows/ci.yml");
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return false;
    };
    metadata.file_type().is_file()
        && metadata.len() == expected.len() as u64
        && fs::read(path).is_ok_and(|bytes| bytes == expected.as_slice())
}

pub fn is_driver_build(invocation: &Invocation) -> bool {
    invocation.program == "cargo"
        && invocation.args.windows(2).any(|a| {
            a == [
                "--manifest-path",
                "assets/tools/native_vega_driver/Cargo.toml",
            ]
        })
}
pub fn preparation(repo: &Path, executor: &Path, check: &Check) -> io::Result<()> {
    if !is_check(check.id) {
        return Ok(());
    }
    if !contract_enabled(repo) {
        return Err(io::Error::other(
            "native Vega recipes require the exact reviewed future workflow contract",
        ));
    }
    for (key, value) in env::vars() {
        if !value.is_empty()
            && (key.starts_with("DYLD_")
                || key.starts_with("CANDLE_")
                || key.starts_with("CMAKE_")
                || key.starts_with("CARGO_TARGET_AARCH64_APPLE_DARWIN_")
                || key.starts_with("CARGO_PROFILE_")
                || [
                    "SDKROOT",
                    "MACOSX_DEPLOYMENT_TARGET",
                    "DEVELOPER_DIR",
                    "METAL_DEVICE_WRAPPER_TYPE",
                    "MTL_CAPTURE_ENABLED",
                ]
                .contains(&key.as_str()))
        {
            return Err(io::Error::other(format!(
                "undeclared native runtime override: {key}"
            )));
        }
    }
    if check.id == "native-vega-gpu" {
        // Cargo's helper build is rooted in R. No candidate/ambient Cargo config
        // may alter the accepted proof driver or inject code into its compilation.
        if executor.join(".cargo/config").exists()
            || fs::read(executor.join(".cargo/config.toml"))?
                != include_bytes!("../../.cargo/config.toml").as_slice()
        {
            return Err(io::Error::other(
                "native proof driver Cargo configuration differs from reviewed R",
            ));
        }
        for ancestor in executor.ancestors().skip(1) {
            if ancestor.join(".cargo/config").exists()
                || ancestor.join(".cargo/config.toml").exists()
            {
                return Err(io::Error::other(
                    "native proof driver has undeclared ancestor Cargo configuration",
                ));
            }
        }
        let parent = repo.join("target");
        fs::create_dir_all(&parent)?;
        let output = parent.join("source-ci-native-vega");
        if !output.exists() {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&output)?;
        }
        if fs::canonicalize(&output)? != output || !output.symlink_metadata()?.is_dir() {
            return Err(io::Error::other(
                "native proof output is not an ordinary canonical directory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if output.metadata()?.permissions().mode() & 0o077 != 0 {
                return Err(io::Error::other(
                    "native proof output directory is not private",
                ));
            }
        }
    }
    Ok(())
}

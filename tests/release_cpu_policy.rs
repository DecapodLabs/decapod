//! Release CPU selection and its deliberately local workflow boundary.
#![allow(dead_code)]
#[path = "../assets/tools/release_cpu.rs"]
mod release_cpu;

use std::collections::BTreeSet;
use std::num::NonZeroUsize;

#[test]
fn release_jobs_use_available_capacity_instead_of_the_repository_cap() {
    for capacity in [1, 2, 4, 8, 16, 64] {
        assert_eq!(
            release_cpu::select_jobs(NonZeroUsize::new(capacity), None),
            Ok(capacity)
        );
    }
}

#[test]
fn release_jobs_preserve_lower_memory_budgets_without_oversubscribing() {
    let available = NonZeroUsize::new(8);
    assert_eq!(release_cpu::select_jobs(available, Some("2")), Ok(2));
    assert_eq!(release_cpu::select_jobs(available, Some("8")), Ok(8));
    assert_eq!(release_cpu::select_jobs(available, Some("32")), Ok(8));
}

#[test]
fn unavailable_cpu_capacity_falls_back_to_one_job() {
    assert_eq!(release_cpu::select_jobs(None, None), Ok(1));
    assert_eq!(release_cpu::select_jobs(None, Some("8")), Ok(1));
}

#[test]
fn invalid_job_budgets_fail_before_artifact_compilation() {
    for requested in [
        "",
        "0",
        "-1",
        "+4",
        "1.5",
        " 4",
        "4\n",
        "default",
        "999999999999999999999999999999",
    ] {
        assert!(
            release_cpu::select_jobs(NonZeroUsize::new(8), Some(requested)).is_err(),
            "invalid budget accepted: {requested:?}"
        );
    }
}

#[test]
fn release_cpu_override_is_confined_to_local_artifact_builds() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let (_, local_and_rest) = workflow.split_once("\n  build-local-artifacts:\n").unwrap();
    let (local, _) = local_and_rest
        .split_once("\n  build-global-artifacts:\n")
        .unwrap();
    let invocation = "bash .github/scripts/release-build.sh";
    assert_eq!(workflow.matches(invocation).count(), 1);
    assert!(local.contains(invocation));
    assert!(!workflow.contains("CARGO_BUILD_JOBS:"));
    assert!(local.contains("fail-fast: false"));
    assert!(!local.contains("max-parallel:"));
    let dist: toml::Value = toml::from_str(include_str!("../dist-workspace.toml")).unwrap();
    let declared: Vec<_> = dist["dist"]["targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|target| target.as_str().unwrap())
        .collect();
    let declared_set: BTreeSet<_> = declared.iter().copied().collect();
    assert_eq!(declared.len(), declared_set.len(), "duplicate dist target");
    let expected: BTreeSet<_> = [
        "aarch64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-gnu",
    ]
    .into_iter()
    .collect();
    assert_eq!(declared_set, expected);
    let matrix = local
        .split_once("      matrix:\n        include:\n")
        .unwrap()
        .1
        .split_once("\n    runs-on:")
        .unwrap()
        .0;
    let mut actual = Vec::new();
    for row in matrix.split("          - targets: ").skip(1) {
        let (target, body) = row.split_once('\n').unwrap();
        actual.push(target);
        let arguments: Vec<_> = body
            .lines()
            .filter_map(|line| line.trim().strip_prefix("dist_args: "))
            .collect();
        assert_eq!(
            arguments,
            [format!("\"--artifacts=local --target={target}\"")],
            "each row must build only its declared target: {target}"
        );
    }
    let actual_set: BTreeSet<_> = actual.iter().copied().collect();
    assert_eq!(actual.len(), actual_set.len(), "duplicate matrix target");
    assert_eq!(actual_set, declared_set, "matrix differs from dist targets");
    let config: toml::Value = toml::from_str(include_str!("../.cargo/config.toml")).unwrap();
    assert_eq!(config["build"]["jobs"].as_integer(), Some(4));
}

#[cfg(unix)]
#[test]
fn artifact_wrapper_passes_scoped_jobs_and_arguments_and_preserves_failure() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    // Test the shell boundary without compiling or publishing real artifacts.
    // Selection itself is tested above; rustc and Cargo are explicit fixtures.
    let rustc = bin.join("rustc");
    fs::write(
        &rustc,
        "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$@\" > \"$FIXTURE_ROOT/rustc-args\"\nif [ \"$FIXTURE_RUSTC_EXIT\" != 0 ]; then exit \"$FIXTURE_RUSTC_EXIT\"; fi\nwhile [ \"$#\" -gt 0 ]; do if [ \"$1\" = '-o' ]; then shift; break; fi; shift; done\ncp \"$FIXTURE_ROOT/probe\" \"$1\"\nchmod +x \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&rustc, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        root.path().join("probe"),
        "#!/bin/sh\nprintf '%s\\n' \"$CARGO_BUILD_JOBS\" > \"$FIXTURE_ROOT/probe-budget\"\nif [ \"$FIXTURE_PROBE_EXIT\" != 0 ]; then exit \"$FIXTURE_PROBE_EXIT\"; fi\nprintf '3\\n'\n",
    )
    .unwrap();
    let cargo = bin.join("cargo");
    fs::write(
        &cargo,
        "#!/bin/sh\nprintf '%s\\n' \"$CARGO_BUILD_JOBS\" > \"$FIXTURE_ROOT/jobs\"\nprintf '%s\\n' \"$@\" > \"$FIXTURE_ROOT/cargo-args\"\nprintf '{\"fixture\":true}\\n'\nexit \"$FIXTURE_EXIT\"\n",
    )
    .unwrap();
    fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let wrapper = root.path().join("release-build.sh");
    fs::write(
        &wrapper,
        include_str!("../.github/scripts/release-build.sh"),
    )
    .unwrap();
    for (rustc_code, probe_code, cargo_code, expected_code) in
        [(0, 0, 0, 0), (0, 0, 23, 23), (41, 0, 0, 41), (0, 42, 0, 42)]
    {
        for file in ["jobs", "cargo-args", "probe-budget"] {
            let _ = fs::remove_file(root.path().join(file));
        }
        let output = Command::new("/bin/bash")
            .arg(&wrapper)
            .args([
                "--artifacts=local",
                "--target=x86_64-unknown-linux-gnu",
                "--tag=v0.110.0",
                "--output-format=json",
            ])
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("CARGO_HOME", root.path().join("empty-cargo-home"))
            .env("RUNNER_TEMP", root.path())
            .env("FIXTURE_ROOT", root.path())
            .env("FIXTURE_RUSTC_EXIT", rustc_code.to_string())
            .env("FIXTURE_PROBE_EXIT", probe_code.to_string())
            .env("FIXTURE_EXIT", cargo_code.to_string())
            .env("CARGO_BUILD_JOBS", "5")
            .current_dir(root.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(expected_code), "{output:?}");
        if rustc_code == 0 {
            assert_eq!(
                fs::read_to_string(root.path().join("probe-budget")).unwrap(),
                "5\n"
            );
        }
        if rustc_code == 0 && probe_code == 0 {
            assert_eq!(output.stdout, b"{\"fixture\":true}\n");
            assert_eq!(fs::read_to_string(root.path().join("jobs")).unwrap(), "3\n");
            assert_eq!(
                fs::read_to_string(root.path().join("cargo-args")).unwrap(),
                "dist\nbuild\n--artifacts=local\n--target=x86_64-unknown-linux-gnu\n--tag=v0.110.0\n--output-format=json\n"
            );
        } else {
            assert!(output.stdout.is_empty());
            assert!(!root.path().join("cargo-args").exists());
        }
        assert!(
            fs::read_to_string(root.path().join("rustc-args"))
                .unwrap()
                .starts_with("--edition=2024\nassets/tools/release_cpu.rs\n-o\n")
        );
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("decapod-release-cpu.")
        }));
    }
}

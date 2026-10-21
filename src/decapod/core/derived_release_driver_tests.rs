use super::*;
fn profile() -> ReleaseProfile {
    super::super::derived_release_profile::profile(
        include_str!("../../../tests/fixtures/derived-release/Cargo.toml.fixture"),
        include_str!("../../../tests/fixtures/derived-release/.github_release.toml"),
        include_str!("../../../tests/fixtures/derived-release/dist-workspace.toml"),
        include_str!("../../../tests/fixtures/derived-release/dist.toml"),
    )
    .unwrap()
}
#[test]
fn shared_helper_executes_only_explicit_build_argv() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nversion=\"0.107.6\"\n",
    )
    .unwrap();
    fs::write(tmp.path().join("Cargo.lock"), "fixed").unwrap();
    let p = profile();
    let mut called = false;
    let result = build_artifacts(
        tmp.path(),
        &p,
        &Stage::Local {
            target: "x86_64-unknown-linux-gnu".into(),
        },
        Some("v0.107.6"),
        |args| {
            called = true;
            assert_eq!(
                args,
                [
                    "dist",
                    "build",
                    "--tag",
                    "v0.107.6",
                    "--output-format=json",
                    "--print=linkage",
                    "--artifacts=local",
                    "--target",
                    "x86_64-unknown-linux-gnu"
                ]
            );
            Ok(b"{}".to_vec())
        },
    )
    .unwrap();
    assert!(called);
    assert_eq!(result, b"{}");
}
#[test]
fn shared_helper_fails_before_execution_for_tag_and_target_drift() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nversion=\"0.107.6\"\n",
    )
    .unwrap();
    fs::write(tmp.path().join("Cargo.lock"), "fixed").unwrap();
    for stage in [
        Stage::Local {
            target: "unknown-target".into(),
        },
        Stage::Plan,
    ] {
        assert!(
            build_artifacts(
                tmp.path(),
                &profile(),
                &stage,
                Some("v0.107.6"),
                |_| panic!("must not execute")
            )
            .is_err()
        );
    }
    for tag in ["v0.107.5", "v1.2.3;echo", "--help"] {
        assert!(
            build_artifacts(
                tmp.path(),
                &profile(),
                &Stage::Global,
                Some(tag),
                |_| panic!("must not execute")
            )
            .is_err()
        );
    }
}
#[test]
fn shared_helper_detects_lock_mutation_even_when_command_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[package]\nversion=\"0.107.6\"\n",
    )
    .unwrap();
    fs::write(tmp.path().join("Cargo.lock"), "fixed").unwrap();
    let result = build_artifacts(
        tmp.path(),
        &profile(),
        &Stage::Global,
        Some("v0.107.6"),
        |_| {
            fs::write(tmp.path().join("Cargo.lock"), "changed")?;
            Ok(vec![])
        },
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("changed Cargo.lock")
    );
}
#[test]
fn production_and_preflight_use_one_build_helper() {
    assert!(
        include_str!("../../../assets/tools/release_artifacts.rs").contains("build_artifacts(")
    );
    assert!(include_str!("derived_release_driver.rs").contains("build_artifacts("));
}
#[test]
fn archive_binary_architecture_is_verified() {
    let mut elf = vec![0; 64];
    elf[..6].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1]);
    elf[18] = 62;
    assert!(binary_architecture(&elf, "x86_64-unknown-linux-gnu").is_ok());
    assert!(binary_architecture(&elf, "aarch64-unknown-linux-gnu").is_err());
    let mut macho = vec![0; 32];
    macho[..4].copy_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
    macho[4..8].copy_from_slice(&0x0100_000cu32.to_le_bytes());
    assert!(binary_architecture(&macho, "aarch64-apple-darwin").is_ok());
    assert!(binary_architecture(&macho, "x86_64-apple-darwin").is_err());
    assert!(binary_architecture(b"#!/bin/sh", "x86_64-unknown-linux-gnu").is_err());
}
#[test]
fn source_and_scratch_must_be_disjoint_before_any_external_command() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().canonicalize().unwrap();
    let scratch = source.join("scratch");
    fs::create_dir(&scratch).unwrap();
    let no = PathBuf::from("/no/tool/may/run");
    let tools = DriverTools {
        git: no.clone(),
        cargo: no.clone(),
        rustc: no.clone(),
        cargo_dist: no.clone(),
        release_plz: no.clone(),
        cargo_semver_checks: no.clone(),
        curl: no.clone(),
        xz: no,
    };
    let input = DriverInputs {
        source_root: source,
        source_head: "a".repeat(40),
        accepted_base: "b".repeat(40),
        release_date: "2026-10-12".into(),
        tools,
        production_tools: BTreeMap::new(),
        semver_checks_production_version: "".into(),
        semver_checks_installer_definition_sha256: "".into(),
        build_environment: BTreeMap::new(),
    };
    let cancel = Cancellation::default();
    let error = prepare(&input, &scratch, DriverLimits::default(), &cancel)
        .err()
        .unwrap();
    assert!(error.to_string().contains("disjoint"));
    assert!(scratch.read_dir().unwrap().next().is_none());
}
#[test]
fn shared_artifact_build_rejects_worker_lock_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("Cargo.toml"), "[package]\nversion=\"0.107.6\"\n").unwrap();
    fs::write(root.join("Cargo.lock"), "fixed root lock").unwrap();
    let native = root.join(super::super::release_metadata::NATIVE_WORKER_LOCK);
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    fs::write(&native, "fixed worker lock").unwrap();
    let error = build_artifacts(root, &profile(), &Stage::Global, Some("v0.107.6"), |_| {
        fs::write(&native, "substituted worker lock")?;
        Ok(Vec::new())
    })
    .unwrap_err();
    assert!(error.to_string().contains("native worker lock"));
}

#[test]
fn worker_release_projection_is_atomic_idempotent_and_preserves_failure_state() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let worker = root.join("assets/tools/vega-native");
    fs::create_dir_all(&worker).unwrap();
    fs::write(worker.join("Cargo.toml"), "[package]\nname = \"worker\"\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"decapod\"\nversion = \"0.121.0\"\n",
    )
    .unwrap();
    let lock = worker.join("Cargo.lock");
    let before = "version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"0.120.1\"\n";
    fs::write(&lock, before).unwrap();
    assert!(reconcile_worker_lock(&root, 4096).unwrap());
    let installed = fs::read(&lock).unwrap();
    assert_eq!(installed, before.replace("0.120.1", "0.121.0").as_bytes());
    assert!(!reconcile_worker_lock(&root, 4096).unwrap());
    assert_eq!(fs::read(&lock).unwrap(), installed);
    assert!(reconcile_worker_lock(&root, 4).is_err());
    assert_eq!(fs::read(&lock).unwrap(), installed);
    assert_eq!(fs::read_dir(&worker).unwrap().count(), 2);
    fs::remove_file(&lock).unwrap();
    assert!(reconcile_worker_lock(&root, 4096).is_err());
    assert!(!lock.exists());
}

#[cfg(unix)]
#[test]
fn worker_release_projection_rejects_hardlinks_and_symlinked_source() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let worker = root.join("assets/tools/vega-native");
    fs::create_dir_all(&worker).unwrap();
    fs::write(worker.join("Cargo.toml"), "[package]\nname = \"worker\"\n").unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"decapod\"\nversion = \"0.121.0\"\n",
    )
    .unwrap();
    let before = "version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"0.120.1\"\n";
    let outside = root.join("unrelated.lock");
    fs::write(&outside, before).unwrap();
    let lock = worker.join("Cargo.lock");
    fs::hard_link(&outside, &lock).unwrap();
    assert!(reconcile_worker_lock(&root, 4096).is_err());
    assert_eq!(fs::read_to_string(&lock).unwrap(), before);
    assert_eq!(fs::read_to_string(&outside).unwrap(), before);
    fs::remove_file(&lock).unwrap();
    symlink(&outside, &lock).unwrap();
    assert!(reconcile_worker_lock(&root, 4096).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), before);
}

#[test]
fn worker_lock_projection_preserves_dependencies_and_rejects_unsafe_identities() {
    use super::super::release_metadata::align_path_dependency_lock;
    let manifest = "[package]\nname = \"decapod\"\nversion = \"0.121.0\"\n";
    let lock = "version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"0.120.1\"\ndependencies = [\n \"pinned\",\n]\n\n[[package]]\nname = \"pinned\"\nversion = \"1.2.3\"\nsource = \"git+https://example.invalid/runtime#fixed\"\n";
    let updated = align_path_dependency_lock(manifest, lock).unwrap();
    assert_eq!(updated, lock.replacen("0.120.1", "0.121.0", 1));
    assert_eq!(
        align_path_dependency_lock(manifest, &updated).unwrap(),
        updated
    );
    assert!(align_path_dependency_lock(manifest, &lock.replace("0.120.1", "0.122.0")).is_err());
    assert!(
        align_path_dependency_lock(
            manifest,
            &format!("{lock}\n[[package]]\nname = \"decapod\"\nversion = \"0.120.1\"\n")
        )
        .is_err()
    );
    assert!(
        align_path_dependency_lock(
            manifest,
            &lock.replacen(
                "dependencies =",
                "source = \"registry+https://example.invalid\"\ndependencies =",
                1
            )
        )
        .is_err()
    );
    assert!(align_path_dependency_lock(manifest, &lock.replace('\n', "\r\n")).is_err());
    assert!(align_path_dependency_lock(manifest, "version = 4\n").is_err());
    assert!(align_path_dependency_lock(&manifest.replace("0.121.0", "next"), lock).is_err());
}

#[test]
fn production_release_setup_matches_shared_tool_pins() {
    let pins =
        ReleaseToolchain::parse(include_str!("../../../.github/release-tools.toml")).unwrap();
    let workflow = include_str!("../../../.github/workflows/release.yml");
    let rust_references: Vec<_> = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("uses: dtolnay/rust-toolchain@"))
        .collect();
    assert_eq!(rust_references, [pins.rust.as_str()]);
    let release_references: Vec<_> = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("uses: release-plz/action@"))
        .collect();
    assert_eq!(
        release_references,
        [pins.release_plz_action
            .strip_prefix("release-plz/action@")
            .unwrap(); 2]
    );
    // Check the actual action's inputs, rather than a matching version string
    // elsewhere in the workflow. The supported release step has one `with`
    // block; any changed layout needs an explicit contract review.
    let action = format!("        uses: {}\n", pins.release_plz_action);
    for step in workflow.split(&action).skip(1) {
        let inputs: Vec<_> = step
            .lines()
            .take_while(|line| line.is_empty() || line.starts_with("        "))
            .collect();
        assert_eq!(
            inputs
                .iter()
                .filter(|line| **line == "        with:")
                .count(),
            1
        );
        let versions: Vec<_> = inputs
            .iter()
            .filter_map(|line| line.strip_prefix("          version: "))
            .collect();
        assert_eq!(versions, [format!("\"{}\"", pins.release_plz)]);
    }
}

#[test]
fn semver_constraint_is_not_an_exact_observation() {
    let pins =
        ReleaseToolchain::parse(include_str!("../../../.github/release-tools.toml")).unwrap();
    assert!(pins.accepts_semver_checks("0.51.0"));
    assert!(pins.accepts_semver_checks("0.51.9"));
    assert!(!pins.accepts_semver_checks("0.51"));
    assert!(!pins.accepts_semver_checks("0.52.0"));
    assert!(
        ReleaseToolchain::parse(
            &include_str!("../../../.github/release-tools.toml")
                .replace("f393a7b38aca9d9b4c58447b291dbac963901df6", "v0.5")
        )
        .is_err()
    );
}

#[test]
fn xz_expansion_is_bounded_before_decompression() {
    let small = "name\ta.tar.xz\nfile\t1\t1\t76\t1024\t0.074\tCRC64\t0\ntotals\t1\t1\t76\t1024\t0.074\tCRC64\t0\t1\n";
    assert!(verify_xz_size(small, 2048).is_ok());
    assert!(verify_xz_size(small, 512).is_err());
    assert!(verify_xz_size("", 2048).is_err());
    assert!(verify_xz_size(&format!("{small}{small}"), 2048).is_err());
}

#[test]
fn default_generator_registry_baseline_must_be_observed() {
    let tmp = tempfile::tempdir().unwrap();
    let cache = tmp.path().join("cargo/registry/cache/index-fixture");
    fs::create_dir_all(&cache).unwrap();
    let bytes = b"verified fixture crate";
    let baseline = RegistryBaseline {
        version: "0.107.5".into(),
        checksum: super::super::derived_release_profile::sha256(bytes),
    };
    assert!(observed_registry_crate(tmp.path(), &baseline, 1024).is_err());
    fs::write(cache.join("decapod-0.107.5.crate"), bytes).unwrap();
    assert_eq!(
        observed_registry_crate(tmp.path(), &baseline, 1024).unwrap(),
        baseline.checksum
    );
    fs::write(cache.join("decapod-0.107.4.crate"), bytes).unwrap();
    assert!(observed_registry_crate(tmp.path(), &baseline, 1024).is_err());
}

#[test]
fn advertised_recipes_match_real_shared_build_arguments() {
    let p = profile();
    let recipes = p.verification_commands("0.107.6", &p.targets).unwrap();
    for target in &p.targets {
        let args = artifact_build_arguments(
            &p,
            &Stage::Local {
                target: target.clone(),
            },
            Some("v0.107.6"),
        )
        .unwrap();
        assert!(recipes.iter().any(|r| r.args == args));
    }
    let global = artifact_build_arguments(&p, &Stage::Global, Some("v0.107.6")).unwrap();
    assert!(recipes.iter().any(|r| r.args == global));
}

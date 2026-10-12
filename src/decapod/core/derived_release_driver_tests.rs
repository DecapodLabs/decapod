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

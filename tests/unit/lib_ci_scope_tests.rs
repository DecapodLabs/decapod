use super::*;

#[test]
fn publisher_readiness_boundary_ignores_ci_environment_probe() {
    if std::env::var("DECAPOD_CI_BOUNDARY_TEST").as_deref() != Ok("1") {
        return;
    }
    assert_eq!(ValidationScope::public_validate(), ValidationScope::CiLocal);
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".decapod")).unwrap();
    let mut config = crate::cli::DecapodProjectConfig::default();
    config.proof.validation_buildkite_preflights = true;
    fs::write(
        root.path().join(".decapod/config.toml"),
        toml::to_string_pretty(&config).unwrap(),
    )
    .unwrap();
    // This is the exact full boundary called by the publisher after bounded
    // prerequisites. It must not dispatch to partial scope from CI variables.
    let report =
        run_current_workbranch_readiness_with(root.path(), &ReadinessInvocation::default());
    assert!(!report.ready);
    assert_eq!(report.scope, core::validation_runtime::READINESS_SCOPE);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.detail.contains("READINESS_PROFILE_REQUIRED")),
        "{report:?}"
    );
}

#[test]
fn publisher_full_boundary_cannot_be_downgraded_by_spoofed_ci_flags() {
    for provider in ["GITHUB_ACTIONS", "BUILDKITE"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ci_scope_tests::publisher_readiness_boundary_ignores_ci_environment_probe",
            ])
            .env_remove("GITHUB_ACTIONS")
            .env_remove("BUILDKITE")
            .env(provider, "true")
            .env("DECAPOD_CI_BOUNDARY_TEST", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }
}

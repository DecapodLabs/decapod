use super::*;
use crate::core::readiness_profile::ProfileDeclaration;
use tempfile::tempdir;

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}
fn has(report: &ReconciliationReport, code: &str) -> bool {
    report.repair_tasks.iter().any(|task| task.code == code)
}
fn declare(root: &Path) -> ProjectProofConfig {
    ProjectProofConfig {
        required_profile: Some(ProfileDeclaration {
            id: readiness_profile::REQUIRED_PROFILE.into(),
            reviewed_sources_digest: discover_source_inventory(root).unwrap().digest().unwrap(),
        }),
        ..ProjectProofConfig::default()
    }
}

#[test]
fn rust_worker_review_aid_exposes_digest_without_approving_stale_recipe_or_proof() {
    use crate::core::validation_runtime::rust_worker::{Configuration, Run, Worker, recipe_digest};
    let root = tempdir().unwrap();
    let workers = vec![Worker {
        name: "probe".into(),
        manifest: "tools/probe/Cargo.toml".into(),
        features: vec![],
        metal_toolchain: false,
        runs: vec![Run {
            arguments: vec!["--probe".into()],
            proof_output: false,
        }],
        timeout_seconds: 60,
    }];
    let expected = recipe_digest(&workers).unwrap();
    let proof = ProjectProofConfig {
        rust_workers: Some(Configuration {
            reviewed_recipes_digest: "unreviewed".into(),
            workers,
        }),
        ..Default::default()
    };
    let report = inspect(root.path(), &proof, None);
    assert_eq!(report.rust_worker_recipes_digest, Some(expected));
    assert!(has(&report, "RUST_WORKER_CONTRACT_INVALID"));
    assert!(!report.merge_ready);
    assert_eq!(report.proof_status, "not_run");
    assert_eq!(
        proof.rust_workers.unwrap().reviewed_recipes_digest,
        "unreviewed"
    );
}

#[test]
fn fresh_repository_never_reports_empty_proof_success() {
    let root = tempdir().unwrap();
    for buildkite in [false, true] {
        let proof = ProjectProofConfig {
            validation_buildkite_preflights: buildkite,
            ..Default::default()
        };
        let report = inspect(root.path(), &proof, None);
        assert!(!report.merge_ready);
        assert_eq!(report.proof_status, "not_run");
        assert_eq!(report.status, "needs_reconciliation");
        assert!(has(&report, "NO_BUILD_OR_CI_CONTRACT"));
        assert!(has(&report, "MISSING_REQUIRED_PROFILE"));
        assert!(has(&report, "REQUIRED_PROOF_NOT_RUN"));
    }
}

#[test]
fn adoption_discovers_build_ci_release_and_helpers_without_executing_or_writing() {
    let root = tempdir().unwrap();
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = 'service'\nversion = '0.1.0'\n",
        ),
        (
            ".github/workflows/custom.yml",
            "jobs:\n  dangerous:\n    steps:\n      - run: touch DO_NOT_EXECUTE\n",
        ),
        (
            ".github/actions/local/action.yml",
            "runs:\n  using: composite\n",
        ),
        ("assets/tools/check.rs", "fn main() {}\n"),
        ("dist.toml", "[dist]\n"),
    ];
    for (path, text) in files {
        write(root.path(), path, text);
    }
    let report = inspect(root.path(), &ProjectProofConfig::default(), Some("service"));
    for (path, text) in files {
        assert!(report.discovered_sources.contains_key(path));
        assert_eq!(fs::read_to_string(root.path().join(path)).unwrap(), text);
    }
    assert!(!root.path().join("DO_NOT_EXECUTE").exists());
    assert!(!root.path().join(".decapod").exists());
    assert!(has(&report, "MISSING_REQUIRED_PROFILE"));
    assert!(has(&report, "UNKNOWN_CI_CONTRACT"));
}

#[test]
fn authored_intent_conflict_is_reported_without_replacement() {
    let root = tempdir().unwrap();
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname='implementation-name'\n",
    );
    let report = inspect(
        root.path(),
        &ProjectProofConfig::default(),
        Some("Authored Product"),
    );
    assert!(has(&report, "INTENT_BUILD_MISMATCH"));
    assert!(
        report
            .repair_tasks
            .iter()
            .any(|task| task.action.contains("Authored Product"))
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).unwrap(),
        "[package]\nname='implementation-name'\n"
    );
}

#[test]
fn stale_inventory_is_reported_and_authored_declaration_stays_unchanged() {
    let root = tempdir().unwrap();
    write(root.path(), "Cargo.toml", "[package]\nname='service'\n");
    let proof = declare(root.path());
    let before = serde_json::to_value(&proof).unwrap();
    write(root.path(), "assets/tools/check.rs", "new selection helper");
    let report = inspect(root.path(), &proof, Some("service"));
    assert!(has(&report, "STALE_SOURCE_INVENTORY"));
    assert_eq!(serde_json::to_value(&proof).unwrap(), before);
    assert!(!report.merge_ready);
}

#[test]
fn matching_digest_does_not_bless_unknown_workflows_or_missing_profile_sources() {
    let root = tempdir().unwrap();
    write(root.path(), ".github/workflows/unknown.yml", "jobs: {}\n");
    let proof = declare(root.path());
    let report = inspect(root.path(), &proof, None);
    assert!(!has(&report, "STALE_SOURCE_INVENTORY"));
    assert!(has(&report, "UNKNOWN_CI_CONTRACT"));
    assert!(has(&report, "MISSING_PROFILE_SOURCE"));
    assert!(has(&report, "REQUIRED_PROOF_NOT_RUN"));
}

#[test]
fn matching_digest_does_not_bless_modified_known_workflow() {
    let root = tempdir().unwrap();
    write(root.path(), ".github/workflows/ci.yml", "jobs: {}\n");
    let proof = declare(root.path());
    let report = inspect(root.path(), &proof, None);
    assert!(has(&report, "MISMATCHED_CI_CONTRACT"));
    assert!(!report.merge_ready);
}

#[test]
fn unknown_profile_retains_intent_and_emits_an_actionable_adapter_task() {
    let root = tempdir().unwrap();
    let mut proof = declare(root.path());
    proof.required_profile.as_mut().unwrap().id = "authored.custom.v1".into();
    let report = inspect(root.path(), &proof, None);
    assert!(has(&report, "UNKNOWN_REQUIRED_PROFILE"));
    assert_eq!(proof.required_profile.unwrap().id, "authored.custom.v1");
}

#[test]
fn source_inventory_is_deterministic_and_excludes_its_own_declaration() {
    let first = tempdir().unwrap();
    let second = tempdir().unwrap();
    let files = [
        ("Cargo.toml", "a"),
        (".github/workflows/ci.yml", "b"),
        ("scripts/test.sh", "c"),
    ];
    for (path, value) in files {
        write(first.path(), path, value);
    }
    for (path, value) in files.into_iter().rev() {
        write(second.path(), path, value);
    }
    write(
        first.path(),
        ".decapod/config.toml",
        "a different reviewed digest",
    );
    assert_eq!(
        discover_source_inventory(first.path()).unwrap(),
        discover_source_inventory(second.path()).unwrap()
    );
}

#[test]
fn provider_selection_does_not_weaken_reconciliation_obligations() {
    let root = tempdir().unwrap();
    write(root.path(), "Cargo.toml", "[package]\nname='service'\n");
    let mut proof = declare(root.path());
    let native = inspect(root.path(), &proof, Some("service"));
    proof.validation_buildkite_preflights = true;
    let remote = inspect(root.path(), &proof, Some("service"));
    assert_eq!(native.repair_tasks, remote.repair_tasks);
    assert_eq!(native.discovered_sources, remote.discovered_sources);
    assert_eq!(native.provider, "native");
    assert_eq!(remote.provider, "buildkite_workbranch");
}

#[test]
fn discovery_bounds_fail_closed_instead_of_returning_partial_inventory() {
    let root = tempdir().unwrap();
    write(root.path(), "Cargo.toml", &"x".repeat(MAX_FILE_BYTES + 1));
    assert!(discover_source_inventory(root.path()).is_err());
    let report = inspect(root.path(), &ProjectProofConfig::default(), None);
    assert!(has(&report, "DISCOVERY_BLOCKED"));
    assert!(report.inventory_digest.is_none());
    assert!(report.discovered_sources.is_empty());
    assert!(!report.merge_ready);
}

#[cfg(unix)]
#[test]
fn discovery_does_not_follow_symlinked_source_roots() {
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    write(outside.path(), "workflows/ci.yml", "outside root");
    std::os::unix::fs::symlink(outside.path(), root.path().join(".github")).unwrap();
    assert!(discover_source_inventory(root.path()).is_err());
}

#[test]
fn non_github_provider_definitions_are_visible_and_not_silently_accepted() {
    let root = tempdir().unwrap();
    for path in [
        ".buildkite/pipeline.yml",
        ".circleci/config.yml",
        ".gitlab-ci.yml",
        "Jenkinsfile",
        ".drone.yml",
        ".woodpecker/pipeline.yml",
    ] {
        write(root.path(), path, "authored provider contract\n");
    }
    let report = inspect(root.path(), &declare(root.path()), None);
    let unknown = report
        .repair_tasks
        .iter()
        .find(|task| task.code == "UNKNOWN_CI_CONTRACT")
        .unwrap();
    assert_eq!(unknown.sources.len(), 6);
}

#[test]
fn source_inventory_entry_and_depth_limits_fail_closed() {
    let wide = tempdir().unwrap();
    for index in 0..MAX_ENTRIES {
        write(
            wide.path(),
            &format!("scripts/{index}.sh"),
            "# bounded fixture",
        );
    }
    assert!(discover_source_inventory(wide.path()).is_err());
    let deep = tempdir().unwrap();
    let path = format!("scripts/{}check.sh", "nested/".repeat(MAX_DEPTH + 1));
    write(deep.path(), &path, "# bounded fixture");
    assert!(discover_source_inventory(deep.path()).is_err());
}

#[test]
fn discovery_rejects_nonportable_control_character_source_names() {
    let root = tempdir().unwrap();
    write(
        root.path(),
        "scripts/misleading\ncheck.sh",
        "# never execute",
    );
    assert!(discover_source_inventory(root.path()).is_err());
}

#[cfg(unix)]
#[test]
fn only_reviewed_toolchain_alias_binds_link_and_regular_destination() {
    use std::os::unix::fs::symlink;
    let root = tempdir().unwrap();
    write(
        root.path(),
        TOOLCHAIN_TARGET,
        "[toolchain]\nchannel='1.96.1'\n",
    );
    symlink(TOOLCHAIN_TARGET, root.path().join(TOOLCHAIN_ALIAS)).unwrap();
    let inventory = discover_source_inventory(root.path()).unwrap();
    assert_eq!(
        inventory.files[TOOLCHAIN_ALIAS],
        format!("sha256:{:x}", Sha256::digest(TOOLCHAIN_TARGET.as_bytes()))
    );
    assert_eq!(
        inventory.files[TOOLCHAIN_TARGET],
        format!(
            "sha256:{:x}",
            Sha256::digest(b"[toolchain]\nchannel='1.96.1'\n")
        )
    );
    write(
        root.path(),
        TOOLCHAIN_TARGET,
        "[toolchain]\nchannel='changed'\n",
    );
    assert_ne!(
        inventory.digest().unwrap(),
        discover_source_inventory(root.path())
            .unwrap()
            .digest()
            .unwrap()
    );
    for target in ["../outside", "/etc/passwd", ".config/build/other.toml"] {
        fs::remove_file(root.path().join(TOOLCHAIN_ALIAS)).unwrap();
        symlink(target, root.path().join(TOOLCHAIN_ALIAS)).unwrap();
        assert!(discover_source_inventory(root.path()).is_err());
    }
    fs::remove_file(root.path().join(TOOLCHAIN_ALIAS)).unwrap();
    symlink(TOOLCHAIN_TARGET, root.path().join(TOOLCHAIN_ALIAS)).unwrap();
    fs::remove_file(root.path().join(TOOLCHAIN_TARGET)).unwrap();
    symlink("/etc/passwd", root.path().join(TOOLCHAIN_TARGET)).unwrap();
    assert!(discover_source_inventory(root.path()).is_err());
}

#[test]
fn configuration_repair_is_agent_owned_and_returns_to_plain_validation() {
    let root = tempdir().unwrap();
    let report = inspect(
        root.path(),
        &ProjectProofConfig::default(),
        Some("product intent"),
    );
    assert!(!report.merge_ready);
    assert_eq!(report.proof_status, "not_run");
    for task in &report.repair_tasks {
        assert_eq!(task.owner, "agent");
        assert_eq!(task.resume_command, "decapod validate");
    }
    let configure = report
        .repair_tasks
        .iter()
        .find(|task| task.code == "NO_BUILD_OR_CI_CONTRACT")
        .unwrap();
    assert!(configure.action.contains("actual build, test and CI tools"));
    let validation = report
        .repair_tasks
        .iter()
        .find(|task| task.code == "REQUIRED_PROOF_NOT_RUN")
        .unwrap();
    assert!(
        validation
            .action
            .contains("Validation owns configured build prerequisites")
    );
    assert!(validation.action.contains("do not build separately first"));
    assert!(validation.action.contains("decision gates"));
    assert!(!root.path().join(".decapod").exists());
}

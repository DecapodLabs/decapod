//! Regression tests for #1531: `repo.pr-readiness.v1` source_coverage rows
//! can declare a reviewed CI-only limitation (`ci_only = true` + `reason`),
//! which is pinned by `reviewed_commands_digest`, reported as a NonApplicable
//! limitation stage, and never counted as a passed check.

use super::*;
use crate::core::proof::ProofDef;
use crate::core::readiness_evidence::ApplicabilityFacts;
use crate::core::readiness_profile::{ProfileDeclaration, SourceInventory};

const LINT_WORKFLOW: &str = ".github/workflows/lint.yml";
const CI_ONLY_WORKFLOW: &str = ".github/workflows/ci-only.yml";

fn digest_of(content: &str) -> String {
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(content.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn inventory() -> SourceInventory {
    let mut files = BTreeMap::new();
    files.insert(LINT_WORKFLOW.to_string(), digest_of("lint"));
    files.insert(CI_ONLY_WORKFLOW.to_string(), digest_of("nix eval"));
    files.insert("src/main.rs".to_string(), digest_of("fn main() {}"));
    SourceInventory { files }
}

fn declaration(sources: &SourceInventory) -> ProfileDeclaration {
    ProfileDeclaration {
        id: REQUIRED_PROFILE.into(),
        reviewed_sources_digest: sources.digest().unwrap(),
    }
}

fn commands() -> Vec<ProofDef> {
    vec![ProofDef {
        name: "lint".into(),
        command: "shellcheck".into(),
        args: vec!["scripts/hello.sh".into()],
        description: String::new(),
        required: true,
    }]
}

fn execution() -> RepositoryExecution {
    let mut toolchain_files = BTreeMap::new();
    toolchain_files.insert("shellcheck".to_string(), "/usr/bin/shellcheck".into());
    RepositoryExecution {
        program: "/usr/bin/shellcheck".into(),
        toolchain_files,
        platforms: BTreeMap::new(),
        environment: BTreeMap::new(),
        directory: ".".into(),
        timeout_seconds: 60,
    }
}

fn contract(coverage: Vec<SourceCoverage>) -> RepositoryContract {
    let mut executions = BTreeMap::new();
    executions.insert("lint".to_string(), execution());
    let mut contract = RepositoryContract {
        canonical_local_target: false,
        reviewed_commands_digest: String::new(),
        source_coverage: coverage,
        executions,
    };
    contract.reviewed_commands_digest = command_contract_digest(&commands(), &contract).unwrap();
    contract
}

fn facts() -> ApplicabilityFacts {
    ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: BTreeSet::new(),
    }
}

fn mapped_coverage() -> Vec<SourceCoverage> {
    vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: None,
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: Some("runs nix eval, which has no local proof profile".into()),
        },
    ]
}

#[test]
fn ci_only_row_with_reason_passes_contract_validation() {
    let sources = inventory();
    let contract = contract(mapped_coverage());
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();
}

#[test]
fn ci_only_row_is_a_non_applicable_limitation_stage_and_not_a_check() {
    let sources = inventory();
    let contract = contract(mapped_coverage());
    let catalog = required_catalog(
        &declaration(&sources),
        &sources,
        &facts(),
        &commands(),
        &contract,
    )
    .unwrap();

    // The ci-only workflow produces no obligation: it is never a required
    // check and can never be counted as a passed one.
    assert_eq!(catalog.obligations().len(), 1);
    assert_eq!(catalog.obligations()[0].key.job, "lint");

    let stages = catalog.stage_dispositions();
    let limitation = stages
        .iter()
        .find(|row| row.stage == CI_ONLY_WORKFLOW)
        .expect("ci-only workflow must appear as a stage disposition");
    assert_eq!(limitation.disposition, StageDispositionKind::NonApplicable);
    assert!(limitation.key.is_none());
    assert!(limitation.reason.contains("runs nix eval"));
    assert!(
        limitation
            .reason
            .contains("never counted as a passed check")
    );
    // The real mapping stays Real.
    assert!(
        stages
            .iter()
            .any(|row| row.stage == "lint" && row.disposition == StageDispositionKind::Real)
    );
}

#[test]
fn ci_only_without_reason_fails_closed() {
    let sources = inventory();
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: None,
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: None,
        },
    ];
    let contract = contract(coverage);
    let error =
        validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("ci_only coverage requires a nonempty reviewed reason"),
        "unexpected error: {error}"
    );
}

#[test]
fn ci_only_with_mapped_commands_fails_closed_as_false_mapping() {
    let sources = inventory();
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: None,
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: true,
            reason: Some("claims shellcheck covers nix eval".into()),
        },
    ];
    let contract = contract(coverage);
    let error =
        validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap_err();
    assert!(
        error.to_string().contains("must not map local commands"),
        "unexpected error: {error}"
    );
}

#[test]
fn reason_without_ci_only_fails_closed() {
    let sources = inventory();
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: Some("stray reason".into()),
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: Some("nix eval".into()),
        },
    ];
    let contract = contract(coverage);
    let error =
        validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("reason is only valid together with ci_only"),
        "unexpected error: {error}"
    );
}

#[test]
fn legacy_rows_without_ci_only_fields_parse_and_serialize_identically() {
    // Pre-#1531 declarations keep their exact digest: the new fields default
    // and are skipped during serialization.
    let legacy = r#"{"source":".github/workflows/lint.yml","commands":["lint"]}"#;
    let row: SourceCoverage = serde_json::from_str(legacy).unwrap();
    assert!(!row.ci_only);
    assert_eq!(row.reason, None);
    assert_eq!(serde_json::to_string(&row).unwrap(), legacy);
}

#[test]
fn ci_only_row_serializes_with_new_fields() {
    let row: SourceCoverage = serde_json::from_str(
        r#"{"source":".github/workflows/ci-only.yml","commands":[],"ci_only":true,"reason":"nix eval has no local profile"}"#,
    )
    .unwrap();
    let value = serde_json::to_value(&row).unwrap();
    assert_eq!(value["ci_only"], true);
    assert_eq!(value["reason"], "nix eval has no local profile");
}

#[test]
fn ci_only_row_may_omit_commands_in_toml_config() {
    // The #1531 reproducer declares the row without a `commands` key:
    // `source_coverage = [ { source = "...", ci_only = true, reason = "..." } ]`.
    #[derive(serde::Deserialize)]
    struct CoverageDoc {
        source_coverage: Vec<SourceCoverage>,
    }
    let doc: CoverageDoc = toml::from_str(
        r#"source_coverage = [ { source = ".github/workflows/ci-only.yml", ci_only = true, reason = "runs nix eval, which has no local proof profile" } ]"#,
    )
    .unwrap();
    let mut rows = doc.source_coverage;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].commands.is_empty());
    assert!(rows[0].ci_only);
    assert_eq!(
        rows[0].reason.as_deref(),
        Some("runs nix eval, which has no local proof profile")
    );

    // The omitted-commands row validates like an explicit empty list.
    let sources = inventory();
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: None,
        },
        rows.pop().unwrap(),
    ];
    let contract = contract(coverage);
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();
}

#[test]
fn all_workflows_ci_only_keeps_local_commands_executing() {
    // #1531 follow-up: a repository whose discovered workflows are all
    // legitimately CI-only still has a real local build/test contract. The
    // mapped-commands invariant must not reject it, the local commands must
    // still produce obligations (they execute normally), and every workflow
    // must surface as a NonApplicable limitation stage.
    let sources = inventory();
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: Some("lint workflow only runs in CI".into()),
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: Some("runs nix eval, which has no local proof profile".into()),
        },
    ];
    let contract = contract(coverage);
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();

    let catalog = required_catalog(
        &declaration(&sources),
        &sources,
        &facts(),
        &commands(),
        &contract,
    )
    .unwrap();
    // Required local commands still execute: one obligation per command.
    assert_eq!(catalog.obligations().len(), 1);
    assert_eq!(catalog.obligations()[0].key.job, "lint");
    // Every discovered workflow is an explicit limitation, never a check.
    for workflow in [LINT_WORKFLOW, CI_ONLY_WORKFLOW] {
        let stage = catalog
            .stage_dispositions()
            .iter()
            .find(|row| row.stage == workflow)
            .expect("ci-only workflow must appear as a stage disposition");
        assert_eq!(stage.disposition, StageDispositionKind::NonApplicable);
        assert!(stage.key.is_none());
    }
    // Only real local command executions are Real stages; no ci_only workflow
    // is ever a Real (or any) check.
    let real: Vec<_> = catalog
        .stage_dispositions()
        .iter()
        .filter(|row| row.disposition == StageDispositionKind::Real)
        .map(|row| row.stage.as_str())
        .collect();
    assert_eq!(real, vec!["lint"]);
}

#[test]
fn reviewed_commands_digest_pins_the_ci_only_reason() {
    let sources = inventory();
    let mut contract = contract(mapped_coverage());
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();

    // Editing the reviewed reason without re-reviewing the digest fails closed.
    contract.source_coverage[1].reason = Some("different reason".into());
    let error =
        validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap_err();
    assert!(
        error.to_string().contains("STALE_COMMAND_CONTRACT"),
        "unexpected error: {error}"
    );
}

#[test]
fn canonical_local_target_is_review_bound_and_cannot_replace_accepted_authority() {
    let commands = commands();
    let accepted = contract(mapped_coverage());
    let legacy = crate::core::readiness_evidence::digest(&(
        REQUIRED_PROFILE,
        &commands,
        &accepted.source_coverage,
        &accepted.executions,
    ))
    .unwrap();
    assert_eq!(accepted.reviewed_commands_digest, legacy);
    assert!(
        serde_json::to_value(&accepted)
            .unwrap()
            .get("canonical_local_target")
            .is_none()
    );
    let mut local = accepted.clone();
    local.canonical_local_target = true;
    local.reviewed_commands_digest = command_contract_digest(&commands, &local).unwrap();
    assert_ne!(
        local.reviewed_commands_digest,
        accepted.reviewed_commands_digest
    );
    assert!(preserve_accepted(&commands, Some(&local), &commands, Some(&accepted)).is_err());
    assert!(preserve_accepted(&commands, Some(&accepted), &commands, Some(&local)).is_err());
    preserve_accepted(&commands, Some(&local), &commands, Some(&local)).unwrap();
}

fn per_platform_execution() -> RepositoryExecution {
    RepositoryExecution {
        program: String::new(),
        toolchain_files: BTreeMap::new(),
        platforms: BTreeMap::from([
            (
                "linux-x86_64".into(),
                PlatformExecution {
                    program: "/nix/store/aaaa-shellcheck-linux/bin/shellcheck".into(),
                    toolchain_files: BTreeMap::from([(
                        "shellcheck".into(),
                        "/nix/store/aaaa-shellcheck-linux/bin/shellcheck".into(),
                    )]),
                },
            ),
            (
                "macos-aarch64".into(),
                PlatformExecution {
                    program: "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck".into(),
                    toolchain_files: BTreeMap::from([(
                        "shellcheck".into(),
                        "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck".into(),
                    )]),
                },
            ),
        ]),
        environment: BTreeMap::new(),
        directory: ".".into(),
        timeout_seconds: 60,
    }
}

fn per_platform_contract() -> RepositoryContract {
    let mut executions = BTreeMap::new();
    executions.insert("lint".to_string(), per_platform_execution());
    let mut contract = RepositoryContract {
        canonical_local_target: false,
        reviewed_commands_digest: String::new(),
        source_coverage: mapped_coverage(),
        executions,
    };
    contract.reviewed_commands_digest = command_contract_digest(&commands(), &contract).unwrap();
    contract
}

#[test]
fn legacy_single_platform_execution_preserves_json_and_digest() {
    let legacy_json = r#"{"program":"/usr/bin/shellcheck","toolchain_files":{"shellcheck":"/usr/bin/shellcheck"},"environment":{},"directory":".","timeout_seconds":60}"#;
    let parsed: RepositoryExecution = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(parsed, execution());
    assert_eq!(serde_json::to_string(&parsed).unwrap(), legacy_json);

    let sources = inventory();
    let contract = contract(mapped_coverage());
    let expected = crate::core::readiness_evidence::digest(&(
        REQUIRED_PROFILE,
        &commands(),
        &contract.source_coverage,
        &contract.executions,
    ))
    .unwrap();
    assert_eq!(contract.reviewed_commands_digest, expected);
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();
}

#[test]
fn per_platform_execution_parses_from_toml_validates_and_pins_all_platforms_in_digest() {
    #[derive(serde::Deserialize)]
    struct ConfigDoc {
        executions: BTreeMap<String, RepositoryExecution>,
    }
    let doc: ConfigDoc = toml::from_str(
        r#"
[executions.lint]
directory = "."
timeout_seconds = 60

[executions.lint.platforms.linux-x86_64]
program = "/nix/store/aaaa-shellcheck-linux/bin/shellcheck"
toolchain_files = { shellcheck = "/nix/store/aaaa-shellcheck-linux/bin/shellcheck" }

[executions.lint.platforms.macos-aarch64]
program = "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck"
toolchain_files = { shellcheck = "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck" }
"#,
    )
    .unwrap();
    let contract = per_platform_contract();
    assert_eq!(doc.executions, contract.executions);

    let sources = inventory();
    validate_contract(&declaration(&sources), &sources, &commands(), &contract).unwrap();
    let catalog = required_catalog(
        &declaration(&sources),
        &sources,
        &facts(),
        &commands(),
        &contract,
    )
    .unwrap();
    assert_eq!(catalog.obligations().len(), 1);

    // Deterministic across repeated invocations.
    assert_eq!(
        command_contract_digest(&commands(), &contract).unwrap(),
        contract.reviewed_commands_digest
    );

    // Mutating any platform entry (program, toolchain_files, or adding/removing a platform)
    // invalidates the reviewed digest.
    for mutation in ["program", "toolchain", "add-platform", "remove-platform"] {
        let mut mutated = contract.clone();
        let platforms = &mut mutated.executions.get_mut("lint").unwrap().platforms;
        match mutation {
            "program" => {
                platforms.get_mut("linux-x86_64").unwrap().program =
                    "/nix/store/cccc-shellcheck-linux/bin/shellcheck".into();
            }
            "toolchain" => {
                platforms
                    .get_mut("macos-aarch64")
                    .unwrap()
                    .toolchain_files
                    .insert(
                        "libc".into(),
                        "/nix/store/dddd/lib/libSystem.B.dylib".into(),
                    );
            }
            "add-platform" => {
                platforms.insert(
                    "linux-aarch64".into(),
                    PlatformExecution {
                        program: "/nix/store/eeee-shellcheck-arm64/bin/shellcheck".into(),
                        toolchain_files: BTreeMap::from([(
                            "shellcheck".into(),
                            "/nix/store/eeee-shellcheck-arm64/bin/shellcheck".into(),
                        )]),
                    },
                );
            }
            "remove-platform" => {
                platforms.remove("macos-aarch64");
            }
            _ => unreachable!(),
        }
        assert_ne!(
            command_contract_digest(&commands(), &mutated).unwrap(),
            contract.reviewed_commands_digest,
            "{mutation} must change command_contract_digest"
        );
        let error =
            validate_contract(&declaration(&sources), &sources, &commands(), &mutated).unwrap_err();
        assert!(
            error.to_string().contains("STALE_COMMAND_CONTRACT"),
            "{mutation}: unexpected error: {error}"
        );
    }
}

#[test]
fn per_platform_schema_rejects_unknown_fields_and_duplicate_keys_in_toml() {
    let unknown_field = toml::from_str::<RepositoryExecution>(
        r#"
directory = "."
timeout_seconds = 60

[platforms.linux-x86_64]
program = "/nix/store/aaaa/bin/shellcheck"
toolchain_files = { shellcheck = "/nix/store/aaaa/bin/shellcheck" }
extra = "forbidden"
"#,
    );
    assert!(unknown_field.is_err());

    let duplicate_key = toml::from_str::<RepositoryExecution>(
        r#"
directory = "."
timeout_seconds = 60

[platforms.linux-x86_64]
program = "/nix/store/aaaa/bin/shellcheck"
toolchain_files = { shellcheck = "/nix/store/aaaa/bin/shellcheck" }

[platforms.linux-x86_64]
program = "/nix/store/bbbb/bin/shellcheck"
toolchain_files = { shellcheck = "/nix/store/bbbb/bin/shellcheck" }
"#,
    );
    assert!(duplicate_key.is_err());
}

#[test]
fn per_platform_validation_rejects_invalid_keys_mixed_fields_relative_paths_and_substitution() {
    let sources = inventory();
    for bad_key in [
        "",
        "linux",
        "darwin-arm64",
        "linux-x86_64 ",
        "Linux-X86_64",
        "windows-x86_64",
        "x86_64-unknown-linux-gnu",
    ] {
        let mut contract = per_platform_contract();
        let entry = contract
            .executions
            .get_mut("lint")
            .unwrap()
            .platforms
            .remove("linux-x86_64")
            .unwrap();
        contract
            .executions
            .get_mut("lint")
            .unwrap()
            .platforms
            .insert(bad_key.into(), entry);
        contract.reviewed_commands_digest =
            command_contract_digest(&commands(), &contract).unwrap();
        let err = validate_contract(&declaration(&sources), &sources, &commands(), &contract)
            .unwrap_err();
        assert!(
            err.to_string().contains("unsupported platform key"),
            "{bad_key:?}: {err}"
        );
    }

    // Mixing top-level program/toolchain_files with platforms is forbidden.
    for mix in ["program", "toolchain_files"] {
        let mut contract = per_platform_contract();
        let exec = contract.executions.get_mut("lint").unwrap();
        match mix {
            "program" => exec.program = "/usr/bin/shellcheck".into(),
            "toolchain_files" => {
                exec.toolchain_files
                    .insert("extra".into(), "/usr/bin/shellcheck".into());
            }
            _ => unreachable!(),
        }
        contract.reviewed_commands_digest =
            command_contract_digest(&commands(), &contract).unwrap();
        let err = validate_contract(&declaration(&sources), &sources, &commands(), &contract)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot mix top-level program/toolchain_files with platforms"),
            "{mix}: {err}"
        );
    }

    // Empty platforms with omitted program/toolchain_files is forbidden.
    let mut empty_platforms = per_platform_contract();
    empty_platforms
        .executions
        .get_mut("lint")
        .unwrap()
        .platforms
        .clear();
    empty_platforms.reviewed_commands_digest =
        command_contract_digest(&commands(), &empty_platforms).unwrap();
    assert!(
        validate_contract(
            &declaration(&sources),
            &sources,
            &commands(),
            &empty_platforms
        )
        .is_err()
    );

    // Relative paths, empty toolchain_files, reserved label, and executable substitution fail closed.
    for defect in [
        "rel-program",
        "rel-toolchain",
        "empty-toolchain",
        "program-label",
        "substituted-binary",
    ] {
        let mut contract = per_platform_contract();
        let entry = contract
            .executions
            .get_mut("lint")
            .unwrap()
            .platforms
            .get_mut("macos-aarch64")
            .unwrap();
        match defect {
            "rel-program" => entry.program = "bin/shellcheck".into(),
            "rel-toolchain" => {
                entry
                    .toolchain_files
                    .insert("runtime".into(), "lib/runtime.so".into());
            }
            "empty-toolchain" => entry.toolchain_files.clear(),
            "program-label" => {
                entry
                    .toolchain_files
                    .insert("program".into(), "/usr/bin/shellcheck".into());
            }
            "substituted-binary" => {
                entry.program = "/nix/store/bbbb-bash/bin/sh".into();
            }
            _ => unreachable!(),
        }
        contract.reviewed_commands_digest =
            command_contract_digest(&commands(), &contract).unwrap();
        assert!(
            validate_contract(&declaration(&sources), &sources, &commands(), &contract).is_err(),
            "{defect} should fail validation"
        );
    }
}

#[test]
fn preserve_accepted_rejects_candidate_platform_add_remove_or_modify_and_allows_additive_check() {
    let accepted_commands = commands();
    let accepted = per_platform_contract();
    preserve_accepted(
        &accepted_commands,
        Some(&accepted),
        &accepted_commands,
        Some(&accepted),
    )
    .unwrap();

    for change in [
        "add-platform",
        "remove-platform",
        "modify-program",
        "modify-toolchain",
        "switch-to-single",
    ] {
        let mut candidate = accepted.clone();
        let exec = candidate.executions.get_mut("lint").unwrap();
        match change {
            "add-platform" => {
                exec.platforms.insert(
                    "macos-x86_64".into(),
                    PlatformExecution {
                        program: "/nix/store/cccc-shellcheck/bin/shellcheck".into(),
                        toolchain_files: BTreeMap::from([(
                            "shellcheck".into(),
                            "/nix/store/cccc-shellcheck/bin/shellcheck".into(),
                        )]),
                    },
                );
            }
            "remove-platform" => {
                exec.platforms.remove("linux-x86_64");
            }
            "modify-program" => {
                exec.platforms.get_mut("linux-x86_64").unwrap().program =
                    "/nix/store/evil-shellcheck/bin/shellcheck".into();
            }
            "modify-toolchain" => {
                exec.platforms
                    .get_mut("macos-aarch64")
                    .unwrap()
                    .toolchain_files
                    .insert("extra".into(), "/nix/store/evil/lib/x".into());
            }
            "switch-to-single" => {
                exec.platforms.clear();
                exec.program = "/usr/bin/shellcheck".into();
                exec.toolchain_files
                    .insert("shellcheck".into(), "/usr/bin/shellcheck".into());
            }
            _ => unreachable!(),
        }
        candidate.reviewed_commands_digest =
            command_contract_digest(&accepted_commands, &candidate).unwrap();
        let err = preserve_accepted(
            &accepted_commands,
            Some(&candidate),
            &accepted_commands,
            Some(&accepted),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "{change}: {err}"
        );
    }

    // Switching an accepted single-platform contract to per-platform also fails closed.
    let accepted_single = contract(mapped_coverage());
    let err = preserve_accepted(
        &accepted_commands,
        Some(&accepted),
        &accepted_commands,
        Some(&accepted_single),
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED")
    );

    // Additive new required check with per-platform declarations preserves accepted checks.
    let mut additive_commands = accepted_commands.clone();
    additive_commands.push(ProofDef {
        name: "lint-extra".into(),
        command: "shellcheck".into(),
        args: vec!["scripts/extra.sh".into()],
        description: String::new(),
        required: true,
    });
    let mut additive_contract = accepted.clone();
    additive_contract
        .executions
        .insert("lint-extra".into(), per_platform_execution());
    additive_contract.source_coverage[0]
        .commands
        .push("lint-extra".into());
    additive_contract.reviewed_commands_digest =
        command_contract_digest(&additive_commands, &additive_contract).unwrap();
    preserve_accepted(
        &additive_commands,
        Some(&additive_contract),
        &accepted_commands,
        Some(&accepted),
    )
    .unwrap();
}

#[test]
fn resolve_for_platform_selects_only_matching_host_and_fails_closed_otherwise() {
    use crate::core::native_proof::NativePlatform;

    let exec = per_platform_execution();
    let (linux_prog, linux_tc) = exec
        .resolve_for_platform(NativePlatform::LinuxX86_64)
        .unwrap();
    assert_eq!(
        linux_prog,
        "/nix/store/aaaa-shellcheck-linux/bin/shellcheck"
    );
    assert_eq!(
        linux_tc["shellcheck"],
        "/nix/store/aaaa-shellcheck-linux/bin/shellcheck"
    );

    let (macos_prog, macos_tc) = exec
        .resolve_for_platform(NativePlatform::MacosAarch64)
        .unwrap();
    assert_eq!(
        macos_prog,
        "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck"
    );
    assert_eq!(
        macos_tc["shellcheck"],
        "/nix/store/bbbb-shellcheck-darwin/bin/shellcheck"
    );

    let missing_err = exec
        .resolve_for_platform(NativePlatform::LinuxAarch64)
        .unwrap_err();
    assert!(
        missing_err
            .to_string()
            .contains("MISSING_PLATFORM_EXECUTION")
    );

    let unsupported_err = exec
        .resolve_for_platform(NativePlatform::WindowsX86_64)
        .unwrap_err();
    assert!(unsupported_err.to_string().contains("UNSUPPORTED_PLATFORM"));
}

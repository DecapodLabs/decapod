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
        targets: BTreeMap::new(),
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

fn multi_target_commands() -> Vec<ProofDef> {
    vec![
        ProofDef {
            name: "check-shared".into(),
            command: "shellcheck".into(),
            args: vec!["shared/check.sh".into()],
            description: String::new(),
            required: true,
        },
        ProofDef {
            name: "check-linux".into(),
            command: "shellcheck".into(),
            args: vec!["linux/check.sh".into()],
            description: String::new(),
            required: true,
        },
        ProofDef {
            name: "check-macos".into(),
            command: "shellcheck".into(),
            args: vec!["macos/check.sh".into()],
            description: String::new(),
            required: true,
        },
    ]
}

fn multi_target_contract() -> RepositoryContract {
    let mut executions = BTreeMap::new();
    for name in ["check-shared", "check-linux", "check-macos"] {
        executions.insert(name.into(), execution());
    }
    let coverage = vec![
        SourceCoverage {
            source: LINT_WORKFLOW.into(),
            commands: vec![
                "check-shared".into(),
                "check-linux".into(),
                "check-macos".into(),
            ],
            ci_only: false,
            reason: None,
        },
        SourceCoverage {
            source: CI_ONLY_WORKFLOW.into(),
            commands: vec![],
            ci_only: true,
            reason: Some("ci only".into()),
        },
    ];
    let targets = BTreeMap::from([
        (
            "shared".into(),
            DeploymentTargetContract {
                commands: vec!["check-shared".into()],
                depends_on: vec![],
                shared_inputs: vec!["shared/".into()],
                platforms: vec![],
            },
        ),
        (
            "linux".into(),
            DeploymentTargetContract {
                commands: vec!["check-linux".into()],
                depends_on: vec!["shared".into()],
                shared_inputs: vec!["deploy/linux/".into()],
                platforms: vec!["linux-x86_64".into()],
            },
        ),
        (
            "macos".into(),
            DeploymentTargetContract {
                commands: vec!["check-macos".into()],
                depends_on: vec!["shared".into()],
                shared_inputs: vec!["deploy/macos/".into(), "**/*.nix".into()],
                platforms: vec!["macos-aarch64".into()],
            },
        ),
    ]);
    let mut contract = RepositoryContract {
        canonical_local_target: false,
        reviewed_commands_digest: String::new(),
        source_coverage: coverage,
        executions,
        targets,
    };
    contract.reviewed_commands_digest =
        command_contract_digest(&multi_target_commands(), &contract).unwrap();
    contract
}

#[test]
fn empty_targets_preserve_command_contract_digest_and_nonempty_targets_bind_digest() {
    let _commands = commands();
    let accepted = contract(mapped_coverage());
    let serialized = serde_json::to_value(&accepted).unwrap();
    assert!(serialized.get("targets").is_none());

    let mut target_contract = multi_target_contract();
    let sources = inventory();
    validate_contract(
        &declaration(&sources),
        &sources,
        &multi_target_commands(),
        &target_contract,
    )
    .unwrap();

    target_contract
        .targets
        .get_mut("linux")
        .unwrap()
        .shared_inputs
        .push("extra/".into());
    let error = validate_contract(
        &declaration(&sources),
        &sources,
        &multi_target_commands(),
        &target_contract,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("STALE_COMMAND_CONTRACT"),
        "unexpected error: {error}"
    );
}

#[test]
fn invalid_deployment_target_definitions_fail_closed() {
    let sources = inventory();
    let cmds = multi_target_commands();
    for defect in [
        "empty-commands",
        "unknown-command",
        "unknown-dep",
        "self-dep",
        "cycle",
        "bad-platform",
        "bad-glob-parent",
        "bad-glob-abs",
        "bad-glob-double-star",
    ] {
        let mut contract = multi_target_contract();
        match defect {
            "empty-commands" => contract.targets.get_mut("linux").unwrap().commands.clear(),
            "unknown-command" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .commands
                .push("nonexistent-cmd".into()),
            "unknown-dep" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .depends_on
                .push("missing-target".into()),
            "self-dep" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .depends_on
                .push("linux".into()),
            "cycle" => {
                contract
                    .targets
                    .get_mut("shared")
                    .unwrap()
                    .depends_on
                    .push("macos".into());
            }
            "bad-platform" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .platforms
                .push("windows-x86_64".into()),
            "bad-glob-parent" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .shared_inputs
                .push("../escape".into()),
            "bad-glob-abs" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .shared_inputs
                .push("/etc/passwd".into()),
            "bad-glob-double-star" => contract
                .targets
                .get_mut("linux")
                .unwrap()
                .shared_inputs
                .push("a**b".into()),
            _ => unreachable!(),
        }
        contract.reviewed_commands_digest = command_contract_digest(&cmds, &contract).unwrap();
        let error =
            validate_contract(&declaration(&sources), &sources, &cmds, &contract).unwrap_err();
        assert!(
            error.to_string().contains("REPOSITORY_CONTRACT_INVALID"),
            "{defect}: unexpected error {error}"
        );
    }
}

#[test]
fn target_closure_resolves_depends_on_and_shared_inputs_and_fails_on_missing_diff() {
    let sources = inventory();
    let cmds = multi_target_commands();
    let contract = multi_target_contract();

    let missing_diff = resolve_target_closure(&contract, "linux", None).unwrap_err();
    assert!(
        missing_diff
            .to_string()
            .contains("MISSING_SHARED_INPUT_DIFF_CONTEXT"),
        "{missing_diff}"
    );

    let unknown = resolve_target_closure(&contract, "unknown", Some(&BTreeSet::new())).unwrap_err();
    assert!(
        unknown.to_string().contains("UNKNOWN_DEPLOYMENT_TARGET"),
        "{unknown}"
    );

    // Unrelated file change only activates `linux` + its `depends_on` (`shared`).
    let mut facts_linux = facts();
    facts_linux
        .changed_paths
        .insert("deploy/linux/app.sh".into());
    let scope =
        resolve_target_closure(&contract, "linux", Some(&facts_linux.changed_paths)).unwrap();
    assert_eq!(scope.resolved_targets, vec!["linux", "shared"]);
    assert_eq!(
        scope.commands,
        BTreeSet::from(["check-linux".into(), "check-shared".into()])
    );

    let catalog = required_catalog_for_target(
        &declaration(&sources),
        &sources,
        &facts_linux,
        &cmds,
        &contract,
        Some("linux"),
    )
    .unwrap();
    let jobs: BTreeSet<_> = catalog
        .obligations()
        .iter()
        .map(|o| o.key.job.as_str())
        .collect();
    assert_eq!(jobs, BTreeSet::from(["check-linux", "check-shared"]));
    assert!(catalog.stage_dispositions().iter().any(|stage| {
        stage.stage == "check-macos"
            && stage.disposition == StageDispositionKind::NonApplicable
            && stage.evidence_scope == "out_of_target_scope_no_assurance"
    }));

    // Shared input glob change (`**/*.nix`) expands closure to include `macos`.
    let mut facts_nix = facts();
    facts_nix
        .changed_paths
        .insert("nix/modules/base.nix".into());
    let expanded =
        resolve_target_closure(&contract, "linux", Some(&facts_nix.changed_paths)).unwrap();
    assert_eq!(expanded.resolved_targets, vec!["linux", "macos", "shared"]);
    assert!(expanded.shared_input_targets.contains("macos"));
    assert_eq!(
        expanded.commands,
        BTreeSet::from([
            "check-linux".into(),
            "check-macos".into(),
            "check-shared".into()
        ])
    );
}

#[test]
fn preserve_accepted_rejects_candidate_target_removal_and_scope_weakening() {
    let cmds = multi_target_commands();
    let accepted = multi_target_contract();

    for defect in [
        "remove-target",
        "remove-command",
        "remove-depends-on",
        "narrow-shared-inputs",
        "change-platforms",
    ] {
        let mut candidate = accepted.clone();
        match defect {
            "remove-target" => {
                candidate.targets.remove("macos");
            }
            "remove-command" => {
                candidate.targets.get_mut("macos").unwrap().commands.clear();
                candidate
                    .targets
                    .get_mut("macos")
                    .unwrap()
                    .commands
                    .push("check-shared".into());
            }
            "remove-depends-on" => {
                candidate
                    .targets
                    .get_mut("linux")
                    .unwrap()
                    .depends_on
                    .clear();
            }
            "narrow-shared-inputs" => {
                candidate
                    .targets
                    .get_mut("macos")
                    .unwrap()
                    .shared_inputs
                    .retain(|p| p != "**/*.nix");
            }
            "change-platforms" => {
                candidate
                    .targets
                    .get_mut("macos")
                    .unwrap()
                    .platforms
                    .push("linux-x86_64".into());
            }
            _ => unreachable!(),
        }
        candidate.reviewed_commands_digest = command_contract_digest(&cmds, &candidate).unwrap();
        let error = preserve_accepted(&cmds, Some(&candidate), &cmds, Some(&accepted)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "{defect}: unexpected error {error}"
        );
    }

    // Additive new target or additive shared_inputs on an existing target is allowed.
    let mut additive = accepted.clone();
    additive
        .targets
        .get_mut("linux")
        .unwrap()
        .shared_inputs
        .push("deploy/common/".into());
    additive.targets.insert(
        "edge".into(),
        DeploymentTargetContract {
            commands: vec!["check-shared".into()],
            depends_on: vec![],
            shared_inputs: vec![],
            platforms: vec![],
        },
    );
    additive.reviewed_commands_digest = command_contract_digest(&cmds, &additive).unwrap();
    preserve_accepted(&cmds, Some(&additive), &cmds, Some(&accepted)).unwrap();
}

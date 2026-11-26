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
fn preserve_accepted_rejects_candidate_target_map_changes() {
    let cmds = multi_target_commands();
    let accepted = multi_target_contract();

    for defect in [
        "remove-target",
        "remove-command",
        "remove-depends-on",
        "narrow-shared-inputs",
        "change-platforms",
        "expand-shared-inputs",
        "add-target",
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
            "expand-shared-inputs" => {
                candidate
                    .targets
                    .get_mut("linux")
                    .unwrap()
                    .shared_inputs
                    .push("deploy/common/".into());
            }
            "add-target" => {
                candidate.targets.insert(
                    "edge".into(),
                    DeploymentTargetContract {
                        commands: vec!["check-shared".into()],
                        depends_on: vec![],
                        shared_inputs: vec![],
                        platforms: vec![],
                    },
                );
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
}

#[test]
fn repo_policy_transition_id_is_deterministic_and_binds_all_inputs() {
    let base = "a".repeat(40);
    let head = "b".repeat(40);
    let src_a = digest_of("src-a");
    let src_b = digest_of("src-b");
    let cmd_a = digest_of("cmd-a");
    let cmd_b = digest_of("cmd-b");

    let id1 = repo_policy_transition_id(&base, &head, &src_a, &src_b, &cmd_a, &cmd_b).unwrap();
    let id2 = repo_policy_transition_id(&base, &head, &src_a, &src_b, &cmd_a, &cmd_b).unwrap();
    assert_eq!(id1, id2);
    assert_eq!(id1.len(), 64);
    assert!(
        id1.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );

    assert_ne!(
        id1,
        repo_policy_transition_id(&"c".repeat(40), &head, &src_a, &src_b, &cmd_a, &cmd_b).unwrap()
    );
    assert_ne!(
        id1,
        repo_policy_transition_id(&base, &"d".repeat(40), &src_a, &src_b, &cmd_a, &cmd_b).unwrap()
    );
    assert_ne!(
        id1,
        repo_policy_transition_id(&base, &head, &digest_of("src-x"), &src_b, &cmd_a, &cmd_b)
            .unwrap()
    );
    assert_ne!(
        id1,
        repo_policy_transition_id(&base, &head, &src_a, &digest_of("src-y"), &cmd_a, &cmd_b)
            .unwrap()
    );
    assert_ne!(
        id1,
        repo_policy_transition_id(&base, &head, &src_a, &src_b, &digest_of("cmd-x"), &cmd_b)
            .unwrap()
    );
    assert_ne!(
        id1,
        repo_policy_transition_id(&base, &head, &src_a, &src_b, &cmd_a, &digest_of("cmd-y"))
            .unwrap()
    );
}

#[test]
fn policy_transition_approval_requires_exact_scope_actor_and_live_expiry() {
    use clap::Parser;
    use decapod::core::store::{Store, StoreKind};
    use decapod::plugins::policy::{
        PolicyCli, approve_action, approve_action_with_expiry, policy_db_path, run_policy_cli_as,
    };

    let tmp = tempfile::tempdir().unwrap();
    let repo_root = tmp.path();
    let run_git = |args: &[&str]| {
        let out = std::process::Command::new("/usr/bin/git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    };
    run_git(&["init", "-b", "main"]);
    let data_root = repo_root.join(".decapod/data");
    std::fs::create_dir_all(&data_root).unwrap();
    let store = Store {
        kind: StoreKind::Repo,
        root: data_root.clone(),
    };
    let transition_id = repo_policy_transition_id(
        &"1".repeat(40),
        &"2".repeat(40),
        &digest_of("s1"),
        &digest_of("s2"),
        &digest_of("c1"),
        &digest_of("c2"),
    )
    .unwrap();

    // Missing DB -> false (and never initializes DB during read-only check).
    assert!(!has_active_policy_transition_approval(repo_root, &transition_id).unwrap());
    assert!(!policy_db_path(&data_root).exists());

    // Wrong scope ("global") -> false.
    crate::core::todo::initialize_todo_db(&data_root).unwrap();
    crate::plugins::policy::initialize_policy_db(&data_root).unwrap();
    run_policy_cli_as(
        &store,
        PolicyCli::try_parse_from([
            "policy",
            "approve",
            "--id",
            &transition_id,
            "--scope",
            "global",
        ])
        .unwrap(),
        "operator",
    )
    .unwrap();
    assert!(!has_active_policy_transition_approval(repo_root, &transition_id).unwrap());

    // Unbounded expiry (expires_at = None) -> false.
    approve_action(
        &store,
        &transition_id,
        None,
        "operator",
        POLICY_TRANSITION_SCOPE,
    )
    .unwrap();
    assert!(!has_active_policy_transition_approval(repo_root, &transition_id).unwrap());

    // Unknown actor -> rejected by CLI and ignored if inserted directly.
    assert!(
        run_policy_cli_as(
            &store,
            PolicyCli::try_parse_from([
                "policy",
                "approve",
                "--id",
                &transition_id,
                "--scope",
                POLICY_TRANSITION_SCOPE,
                "--actor",
                "unknown",
            ])
            .unwrap(),
            "operator",
        )
        .is_err()
    );
    let future = format!(
        "{}Z",
        crate::core::time::now_epoch_z()
            .trim_end_matches('Z')
            .parse::<u64>()
            .unwrap()
            + 600
    );
    assert!(
        approve_action_with_expiry(
            &store,
            &transition_id,
            None,
            "unknown",
            POLICY_TRANSITION_SCOPE,
            Some(&future),
        )
        .is_err()
    );
    // Even if an "unknown" actor row is injected directly into SQLite, the
    // read-side check must reject it fail-closed.
    let conn = crate::core::db::Connection::open(policy_db_path(&data_root)).unwrap();
    conn.execute(
        "INSERT INTO approvals(approval_id, action_fingerprint, actor, ts, scope, expires_at) VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
        crate::core::db::params![
            crate::core::ulid::new_ulid(),
            transition_id,
            "unknown",
            crate::core::time::now_epoch_z(),
            POLICY_TRANSITION_SCOPE,
            future
        ],
    )
    .unwrap();
    drop(conn);
    assert!(!has_active_policy_transition_approval(repo_root, &transition_id).unwrap());

    // Valid policy.control-authorized bounded approval -> true.
    run_policy_cli_as(
        &store,
        PolicyCli::try_parse_from([
            "policy",
            "approve",
            "--id",
            &transition_id,
            "--scope",
            POLICY_TRANSITION_SCOPE,
        ])
        .unwrap(),
        "operator",
    )
    .unwrap();
    assert!(has_active_policy_transition_approval(repo_root, &transition_id).unwrap());
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
        targets: BTreeMap::new(),
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

mod release_only_inventory_transition_tests {
    use super::*;
    use crate::core::init_reconciliation::discover_source_inventory;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    fn git(repo: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("utf8 git output")
            .trim()
            .to_owned()
    }

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-b", "main"]);
        git(
            dir.path(),
            &["config", "user.email", "release@example.test"],
        );
        git(dir.path(), &["config", "user.name", "Release Bot"]);
        dir
    }

    fn commit_all(repo: &Path, message: &str) -> String {
        git(repo, &["add", "-A"]);
        git(repo, &["commit", "-m", message]);
        git(repo, &["rev-parse", "HEAD"])
    }

    fn cargo_toml(name: &str, version: &str, extra_dep: &str) -> String {
        format!(
            "[package]\nname = \"{name}\"\nversion = \"{version}\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1.0\"\n{extra_dep}"
        )
    }

    fn cargo_lock(name: &str, version: &str, dep_version: &str) -> String {
        format!(
            "version = 4\n\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\ndependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"serde\"\nversion = \"{dep_version}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n"
        )
    }

    fn setup_dactyl_style_repo() -> (tempfile::TempDir, String, String) {
        let dir = init_repo();
        let root = dir.path();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::write(
            root.join(".github/workflows/rust-contract.yml"),
            "name: Rust Contract\non: [push, pull_request]\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.17.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.17.0", "1.0.210"),
        )
        .unwrap();
        fs::create_dir_all(root.join(".decapod")).unwrap();
        fs::write(
            root.join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n\n[repo]\nbase_branch = \"main\"\nbackend = \"local\"\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn version() -> u32 { 1 }\n").unwrap();
        let prior_sha = commit_all(root, "chore: baseline 0.17.0");
        let prior_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        (dir, prior_sha, prior_digest)
    }

    fn approval_input(ci_local: bool) -> transition_approval::ApprovalInput {
        transition_approval::ApprovalInput {
            ci_local,
            signed: None,
            ci_target: if ci_local { Some("main".into()) } else { None },
        }
    }

    fn dactyl_contract() -> (Vec<ProofDef>, RepositoryContract) {
        let cmds = commands();
        let cov = vec![SourceCoverage {
            source: ".github/workflows/rust-contract.yml".into(),
            commands: vec!["lint".into()],
            ci_only: false,
            reason: None,
        }];
        let c = contract(cov);
        (cmds, c)
    }

    #[test]
    fn release_only_semver_bump_qualifies_in_local_and_ci_local_without_approval() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        // Simulate release-please / release-plz cutting 0.17.0 -> 0.18.0 on main.
        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.18.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.18.0", "1.0.210"),
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: release 0.18.0");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        assert_ne!(prior_digest, base_digest);

        // Candidate branch modifies non-inventory source only and reconciles digest.
        fs::write(root.join("src/lib.rs"), "pub fn version() -> u32 { 2 }\n").unwrap();
        let head_sha = commit_all(root, "feat: runtime selector update");

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        for ci_local in [false, true] {
            verify_accepted_transition(
                root,
                &base_sha,
                &head_sha,
                Some(&accepted_decl),
                Some(&candidate_decl),
                &cmds,
                &cmds,
                Some(&repo_contract),
                Some(&repo_contract),
                approval_input(ci_local),
            )
            .unwrap_or_else(|e| {
                panic!("expected release-only bump to qualify (ci_local={ci_local}): {e}")
            });
        }
    }

    #[cfg(unix)]
    #[test]
    fn git_tree_inventory_matches_filesystem_discovery_including_toolchain_alias() {
        let (dir, _prior_sha, _prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();
        fs::create_dir_all(root.join(".config/build")).unwrap();
        fs::write(
            root.join(".config/build/rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.96.1\"\n",
        )
        .unwrap();
        std::os::unix::fs::symlink(
            ".config/build/rust-toolchain.toml",
            root.join("rust-toolchain.toml"),
        )
        .unwrap();
        let head = commit_all(root, "chore: add reviewed toolchain symlink alias");

        let fs_inv = discover_source_inventory(root).unwrap();
        let mut cache = BTreeMap::new();
        let git_inv =
            compute_git_tree_source_inventory(root, &head, &mut cache).expect("git tree inventory");
        assert_eq!(git_inv.inventory, fs_inv);
        assert_eq!(
            git_inv.inventory.digest().unwrap(),
            fs_inv.digest().unwrap()
        );
    }

    #[test]
    fn release_bump_with_modified_workflow_fails_closed() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.18.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.18.0", "1.0.210"),
        )
        .unwrap();
        fs::write(
            root.join(".github/workflows/rust-contract.yml"),
            "name: Rust Contract\non: [push]\n",
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: release 0.18.0 + sneaky workflow change");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        let err = verify_accepted_transition(
            root,
            &base_sha,
            &base_sha,
            Some(&accepted_decl),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(true),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn release_bump_with_cargo_toml_dependency_change_fails_closed() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.18.0", "tokio = \"1.0\"\n"),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.18.0", "1.0.210"),
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: release 0.18.0 + new dep");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        let err = verify_accepted_transition(
            root,
            &base_sha,
            &base_sha,
            Some(&accepted_decl),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(false),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn release_bump_with_cargo_lock_dependency_change_fails_closed() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.18.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.18.0", "1.0.215"),
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: release 0.18.0 + serde bump");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        let err = verify_accepted_transition(
            root,
            &base_sha,
            &base_sha,
            Some(&accepted_decl),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(true),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn candidate_modified_cargo_toml_fails_closed() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        // Main cuts 0.18.0.
        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.18.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.18.0", "1.0.210"),
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: release 0.18.0");

        // Candidate modifies Cargo.toml further (0.19.0) and sets its reviewed digest to match HEAD.
        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.19.0", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.19.0", "1.0.210"),
        )
        .unwrap();
        let head_sha = commit_all(root, "feat: candidate tries self-bumping Cargo.toml");
        let candidate_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: candidate_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        let err = verify_accepted_transition(
            root,
            &base_sha,
            &head_sha,
            Some(&accepted_decl),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(false),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn version_downgrade_and_missing_ancestor_fail_closed() {
        let (dir, _prior_sha, prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();

        // Downgrade 0.17.0 -> 0.16.9 must not qualify.
        fs::write(
            root.join("Cargo.toml"),
            cargo_toml("dactyl-db", "0.16.9", ""),
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            cargo_lock("dactyl-db", "0.16.9", "1.0.210"),
        )
        .unwrap();
        let downgrade_sha = commit_all(root, "chore: downgrade 0.16.9");
        let downgrade_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: prior_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: downgrade_digest,
        };
        let (cmds, repo_contract) = dactyl_contract();

        let err = verify_accepted_transition(
            root,
            &downgrade_sha,
            &downgrade_sha,
            Some(&accepted_decl),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(true),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );

        // Missing ancestor digest (unknown accepted digest) must not qualify.
        let unknown_accepted = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: format!("sha256:{}", "f".repeat(64)),
        };
        let err = verify_accepted_transition(
            root,
            &downgrade_sha,
            &downgrade_sha,
            Some(&unknown_accepted),
            Some(&candidate_decl),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(true),
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "unexpected error: {err}"
        );
    }

    fn clack_style_swift_repo() -> (
        tempfile::TempDir,
        String,
        String,
        Vec<ProofDef>,
        RepositoryContract,
    ) {
        let dir = init_repo();
        let root = dir.path();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/Info.plist"), "<string>0.9.17</string>\n").unwrap();
        fs::write(
            root.join("scripts/release-app.sh"),
            "#!/bin/sh\necho release\n",
        )
        .unwrap();
        fs::write(
            root.join("scripts/check_helper.sh"),
            "#!/bin/sh\necho check\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".decapod")).unwrap();
        fs::write(
            root.join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n\n[repo]\nbase_branch = \"main\"\nbackend = \"local\"\n",
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: baseline clack-style repo");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        let cmds = vec![ProofDef {
            name: "swift-tests".into(),
            command: "/usr/bin/swift".into(),
            args: vec!["test".into()],
            description: "Run swift test".into(),
            required: true,
        }];
        let mut toolchain_files = BTreeMap::new();
        toolchain_files.insert("swift".to_string(), "/usr/bin/swift".into());
        let mut executions = BTreeMap::new();
        executions.insert(
            "swift-tests".to_string(),
            RepositoryExecution {
                program: "/usr/bin/swift".into(),
                toolchain_files,
                platforms: BTreeMap::new(),
                environment: BTreeMap::new(),
                directory: ".".into(),
                timeout_seconds: 720,
            },
        );
        let mut repo_contract = RepositoryContract {
            canonical_local_target: false,
            reviewed_commands_digest: String::new(),
            source_coverage: vec![],
            executions,
            targets: BTreeMap::new(),
        };
        repo_contract.reviewed_commands_digest =
            command_contract_digest(&cmds, &repo_contract).unwrap();
        (dir, base_sha, base_digest, cmds, repo_contract)
    }

    #[test]
    fn unreferenced_scripts_transition_qualifies_without_manual_approval() {
        let (dir, base_sha, base_digest, cmds, repo_contract) = clack_style_swift_repo();
        let root = dir.path();

        fs::write(root.join("scripts/Info.plist"), "<string>0.9.18</string>\n").unwrap();
        fs::write(
            root.join("scripts/release-app.sh"),
            "#!/bin/sh\nbash ./scripts/package-dmg.sh\n",
        )
        .unwrap();
        fs::write(root.join("scripts/package-dmg.sh"), "#!/bin/sh\necho dmg\n").unwrap();
        fs::write(
            root.join("scripts/render-dmg-background.swift"),
            "print(\"bg\")\n",
        )
        .unwrap();
        let head_sha = commit_all(root, "chore(release): bump to v0.9.18 with mascot DMG");
        let head_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: head_digest,
        };

        for ci_local in [false, true] {
            verify_accepted_transition(
                root,
                &base_sha,
                &head_sha,
                Some(&accepted_decl),
                Some(&candidate_decl),
                &cmds,
                &cmds,
                Some(&repo_contract),
                Some(&repo_contract),
                approval_input(ci_local),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "expected unreferenced scripts transition to qualify (ci_local={ci_local}): {e}"
                )
            });
        }
    }

    #[test]
    fn proof_referenced_script_and_transitively_referenced_helper_still_fail_closed_with_diagnostics()
     {
        // 1. Direct proof.commands script reference must fail closed.
        let (dir, _prior_sha, _prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/hello.sh"), "#!/bin/sh\necho hi\n").unwrap();
        fs::write(
            root.join("scripts/lib_helper.sh"),
            "#!/bin/sh\necho helper\n",
        )
        .unwrap();
        fs::write(
            root.join("Justfile"),
            "check:\n    sh scripts/lib_helper.sh\n",
        )
        .unwrap();
        let base_sha = commit_all(root, "chore: add proof-referenced scripts");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        let (cmds, repo_contract) = dactyl_contract();

        fs::write(root.join("scripts/hello.sh"), "#!/bin/sh\necho tampered\n").unwrap();
        let head1 = commit_all(root, "chore: modify direct proof script");
        let head1_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        let err1 = verify_accepted_transition(
            root,
            &base_sha,
            &head1,
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: base_digest.clone(),
            }),
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: head1_digest.clone(),
            }),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(false),
        )
        .unwrap_err()
        .to_string();
        let expected_tid1 = repo_policy_transition_id(
            &base_sha,
            &head1,
            &base_digest,
            &head1_digest,
            &repo_contract.reviewed_commands_digest,
            &repo_contract.reviewed_commands_digest,
        )
        .unwrap();
        assert!(
            err1.contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "{err1}"
        );
        assert!(
            err1.contains(&format!("transition_id={expected_tid1}")),
            "{err1}"
        );
        assert!(err1.contains("scripts/hello.sh"), "{err1}");

        // 2. Helper transitively referenced from Justfile must also fail closed.
        let root2 = init_repo();
        let r2 = root2.path();
        fs::create_dir_all(r2.join(".github/workflows")).unwrap();
        fs::write(
            r2.join(".github/workflows/rust-contract.yml"),
            "name: Rust Contract\non: [push]\n",
        )
        .unwrap();
        fs::create_dir_all(r2.join(".decapod")).unwrap();
        fs::write(
            r2.join(".decapod/config.toml"),
            "schema_version = \"1.0.0\"\n\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n\n[repo]\nbase_branch = \"main\"\nbackend = \"local\"\n",
        )
        .unwrap();
        fs::create_dir_all(r2.join("scripts")).unwrap();
        fs::write(r2.join("scripts/lib_helper.sh"), "#!/bin/sh\necho ok\n").unwrap();
        fs::write(
            r2.join("Justfile"),
            "check:\n    sh scripts/lib_helper.sh\n",
        )
        .unwrap();
        let b2 = commit_all(r2, "chore: baseline justfile helper");
        let b2_digest = discover_source_inventory(r2).unwrap().digest().unwrap();
        fs::write(r2.join("scripts/lib_helper.sh"), "#!/bin/sh\necho edited\n").unwrap();
        let h2 = commit_all(r2, "chore: modify justfile-referenced helper");
        let h2_digest = discover_source_inventory(r2).unwrap().digest().unwrap();
        let err2 = verify_accepted_transition(
            r2,
            &b2,
            &h2,
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: b2_digest.clone(),
            }),
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: h2_digest.clone(),
            }),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(false),
        )
        .unwrap_err()
        .to_string();
        let expected_tid2 = repo_policy_transition_id(
            &b2,
            &h2,
            &b2_digest,
            &h2_digest,
            &repo_contract.reviewed_commands_digest,
            &repo_contract.reviewed_commands_digest,
        )
        .unwrap();
        assert!(
            err2.contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "{err2}"
        );
        assert!(
            err2.contains(&format!("transition_id={expected_tid2}")),
            "{err2}"
        );
        assert!(err2.contains("scripts/lib_helper.sh"), "{err2}");
    }

    #[test]
    fn unreferenced_non_ecosystem_manifests_qualify_without_manual_approval() {
        let (dir, base_sha, base_digest, cmds, repo_contract) = clack_style_swift_repo();
        let root = dir.path();

        fs::write(root.join("Dockerfile"), "FROM swift:6.0\n").unwrap();
        fs::write(
            root.join("package.json"),
            "{\"name\":\"helper-tools\",\"version\":\"1.0.0\"}\n",
        )
        .unwrap();
        fs::write(
            root.join("package-lock.json"),
            "{\"name\":\"helper-tools\",\"lockfileVersion\":3}\n",
        )
        .unwrap();
        fs::write(
            root.join("release-plz.toml"),
            "[workspace]\npr_labels = [\"release\"]\n",
        )
        .unwrap();
        let head_sha = commit_all(root, "chore: add uncoupled non-proof manifests");
        let head_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let accepted_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: base_digest,
        };
        let candidate_decl = ProfileDeclaration {
            id: REQUIRED_PROFILE.into(),
            reviewed_sources_digest: head_digest,
        };

        for ci_local in [false, true] {
            verify_accepted_transition(
                root,
                &base_sha,
                &head_sha,
                Some(&accepted_decl),
                Some(&candidate_decl),
                &cmds,
                &cmds,
                Some(&repo_contract),
                Some(&repo_contract),
                approval_input(ci_local),
            )
            .unwrap_or_else(|e| {
                panic!(
                    "expected uncoupled non-proof manifests to qualify (ci_local={ci_local}): {e}"
                )
            });
        }
    }

    #[test]
    fn ecosystem_coupled_manifest_changes_fail_closed_without_manual_cli_prompt() {
        let cases: &[(&str, &str, &str, &str, &[&str])] = &[
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
                "[package]\nname = \"demo\"\nversion = \"0.1.1\"\n",
                "/usr/bin/cargo",
                &["test"],
            ),
            (
                "package.json",
                "{\"name\":\"demo\",\"version\":\"1.0.0\"}\n",
                "{\"name\":\"demo\",\"version\":\"1.0.1\"}\n",
                "/usr/bin/npm",
                &["test"],
            ),
            (
                "pyproject.toml",
                "[project]\nname = \"demo\"\nversion = \"0.1.0\"\n",
                "[project]\nname = \"demo\"\nversion = \"0.1.1\"\n",
                "/usr/bin/pytest",
                &["-q"],
            ),
            (
                "go.mod",
                "module example.com/demo\n\ngo 1.22\n",
                "module example.com/demo\n\ngo 1.23\n",
                "/usr/bin/go",
                &["test", "./..."],
            ),
            (
                "Dockerfile",
                "FROM alpine:3.20\n",
                "FROM alpine:3.21\n",
                "/usr/bin/docker",
                &["build", "."],
            ),
        ];

        for (manifest_path, initial, updated, program, args) in cases {
            let dir = init_repo();
            let root = dir.path();
            fs::create_dir_all(root.join(".decapod")).unwrap();
            fs::write(
                root.join(".decapod/config.toml"),
                "schema_version = \"1.0.0\"\n\n[init]\ndiagram_style = \"ascii\"\nentrypoints = []\n\n[repo]\nbase_branch = \"main\"\nbackend = \"local\"\n",
            )
            .unwrap();
            fs::write(root.join(manifest_path), initial).unwrap();
            let base_sha = commit_all(root, "chore: baseline manifest");
            let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();

            fs::write(root.join(manifest_path), updated).unwrap();
            let head_sha = commit_all(root, "chore: update ecosystem-coupled manifest");
            let head_digest = discover_source_inventory(root).unwrap().digest().unwrap();

            let cmds = vec![ProofDef {
                name: "check".into(),
                command: (*program).into(),
                args: args.iter().map(|s| (*s).to_owned()).collect(),
                description: "Ecosystem check".into(),
                required: true,
            }];
            let mut toolchain_files = BTreeMap::new();
            toolchain_files.insert("tool".to_string(), (*program).into());
            let mut executions = BTreeMap::new();
            executions.insert(
                "check".to_string(),
                RepositoryExecution {
                    program: (*program).into(),
                    toolchain_files,
                    platforms: BTreeMap::new(),
                    environment: BTreeMap::new(),
                    directory: ".".into(),
                    timeout_seconds: 120,
                },
            );
            let mut repo_contract = RepositoryContract {
                canonical_local_target: false,
                reviewed_commands_digest: String::new(),
                source_coverage: vec![],
                executions,
                targets: BTreeMap::new(),
            };
            repo_contract.reviewed_commands_digest =
                command_contract_digest(&cmds, &repo_contract).unwrap();

            let err = verify_accepted_transition(
                root,
                &base_sha,
                &head_sha,
                Some(&ProfileDeclaration {
                    id: REQUIRED_PROFILE.into(),
                    reviewed_sources_digest: base_digest.clone(),
                }),
                Some(&ProfileDeclaration {
                    id: REQUIRED_PROFILE.into(),
                    reviewed_sources_digest: head_digest.clone(),
                }),
                &cmds,
                &cmds,
                Some(&repo_contract),
                Some(&repo_contract),
                approval_input(false),
            )
            .unwrap_err()
            .to_string();

            let expected_tid = repo_policy_transition_id(
                &base_sha,
                &head_sha,
                &base_digest,
                &head_digest,
                &repo_contract.reviewed_commands_digest,
                &repo_contract.reviewed_commands_digest,
            )
            .unwrap();
            assert!(
                err.contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
                "{manifest_path}: {err}"
            );
            assert!(
                err.contains(&format!("transition_id={expected_tid}")),
                "{manifest_path}: {err}"
            );
            assert!(err.contains(manifest_path), "{manifest_path}: {err}");
            assert!(
                !err.contains("decapod govern policy approve"),
                "{manifest_path}: {err}"
            );
        }
    }

    #[test]
    fn multi_hop_and_same_dir_transitive_helper_references_fail_closed() {
        let (dir, _prior_sha, _prior_digest) = setup_dactyl_style_repo();
        let root = dir.path();
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(
            root.join("scripts/hello.sh"),
            "#!/bin/sh\nsh scripts/mid.sh\n",
        )
        .unwrap();
        fs::write(root.join("scripts/mid.sh"), "#!/bin/sh\n./leaf.sh\n").unwrap();
        fs::write(root.join("scripts/leaf.sh"), "#!/bin/sh\necho leaf\n").unwrap();
        let base_sha = commit_all(root, "chore: baseline multi-hop helper chain");
        let base_digest = discover_source_inventory(root).unwrap().digest().unwrap();
        let (cmds, repo_contract) = dactyl_contract();

        fs::write(root.join("scripts/leaf.sh"), "#!/bin/sh\necho tampered\n").unwrap();
        let head_sha = commit_all(root, "chore: tamper transitive same-dir leaf helper");
        let head_digest = discover_source_inventory(root).unwrap().digest().unwrap();

        let err = verify_accepted_transition(
            root,
            &base_sha,
            &head_sha,
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: base_digest.clone(),
            }),
            Some(&ProfileDeclaration {
                id: REQUIRED_PROFILE.into(),
                reviewed_sources_digest: head_digest.clone(),
            }),
            &cmds,
            &cmds,
            Some(&repo_contract),
            Some(&repo_contract),
            approval_input(false),
        )
        .unwrap_err()
        .to_string();

        let expected_tid = repo_policy_transition_id(
            &base_sha,
            &head_sha,
            &base_digest,
            &head_digest,
            &repo_contract.reviewed_commands_digest,
            &repo_contract.reviewed_commands_digest,
        )
        .unwrap();
        assert!(
            err.contains("ACCEPTED_REPOSITORY_CONTRACT_CHANGED"),
            "{err}"
        );
        assert!(
            err.contains(&format!("transition_id={expected_tid}")),
            "{err}"
        );
        assert!(err.contains("scripts/leaf.sh"), "{err}");
        assert!(!err.contains("decapod govern policy approve"), "{err}");
    }
}

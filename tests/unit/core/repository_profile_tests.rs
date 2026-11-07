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

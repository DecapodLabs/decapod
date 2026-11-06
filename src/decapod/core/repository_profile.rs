//! Explicit, repository-authored local proof. This is not a CI/YAML interpreter
//! or evidence that a hosted workflow ran. Review binds the complete discovered
//! source inventory and the declared command/execution/coverage contract. The
//! existing frozen native lifecycle proves those commands actually ran against
//! the exact candidate and accepted target, with a byte-measured declared tool
//! closure; it cannot infer semantic equivalence or transitive completeness.
use super::proof::ProofDef;
use super::readiness_evidence::{ApplicabilityFacts, CheckKey, CheckKind, Provider};
use super::readiness_profile::{
    InvocationRole, Obligation, PlanningFailure, ProfileDeclaration, RequiredCatalog,
    SourceInventory, StageDisposition, StageDispositionKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

pub const REQUIRED_PROFILE: &str = "repo.pr-readiness.v1";
pub const READINESS_SCOPE: &str = "repository_local_readiness";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceCoverage {
    pub source: String,
    pub commands: Vec<String>,
}

fn root_directory() -> String {
    ".".into()
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryExecution {
    /// Installed canonical executable, never a candidate-produced executable.
    pub program: String,
    /// Reviewed transitive tool/runtime files. Completeness is authored, while
    /// every declared byte and the direct executable are measured by the runner.
    pub toolchain_files: BTreeMap<String, String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "root_directory")]
    pub directory: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryContract {
    pub reviewed_commands_digest: String,
    pub source_coverage: Vec<SourceCoverage>,
    pub executions: BTreeMap<String, RepositoryExecution>,
}

/// Read-only review aid; never writes or approves the declaration. Includes the
/// execution environment/tool paths and coverage, avoiding a self-reference.
pub fn command_contract_digest(
    commands: &[ProofDef],
    contract: &RepositoryContract,
) -> Result<String, PlanningFailure> {
    super::readiness_evidence::digest(&(
        REQUIRED_PROFILE,
        commands,
        &contract.source_coverage,
        &contract.executions,
    ))
    .map_err(|_| PlanningFailure::Serialization)
}

fn invalid(message: impl Into<String>) -> PlanningFailure {
    PlanningFailure::InvalidPlan(format!("REPOSITORY_CONTRACT_INVALID: {}", message.into()))
}
fn relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

pub fn validate_contract(
    declaration: &ProfileDeclaration,
    sources: &SourceInventory,
    commands: &[ProofDef],
    contract: &RepositoryContract,
) -> Result<(), PlanningFailure> {
    if declaration.id != REQUIRED_PROFILE {
        return Err(PlanningFailure::UnknownProfile(declaration.id.clone()));
    }
    if sources.files.is_empty()
        || sources
            .files
            .keys()
            .any(|path| !relative(path) || path == ".decapod/config.toml")
        || sources
            .files
            .values()
            .any(|digest| !super::readiness_profile::valid_digest(digest))
        || sources.digest()? != declaration.reviewed_sources_digest
    {
        return Err(PlanningFailure::SourceContractChanged);
    }
    if command_contract_digest(commands, contract)? != contract.reviewed_commands_digest {
        return Err(invalid(
            "STALE_COMMAND_CONTRACT: review commands, execution inputs and source coverage before updating the digest",
        ));
    }
    if commands.is_empty() || commands.len() > 256 {
        return Err(invalid(
            "a bounded nonempty required command set is mandatory",
        ));
    }
    let mut names = BTreeSet::new();
    for command in commands {
        if command.name.trim().is_empty()
            || command.command.trim().is_empty()
            || !command.required
            || !names.insert(command.name.clone())
        {
            return Err(invalid(
                "commands need unique nonempty names, programs and required=true",
            ));
        }
        let execution = contract
            .executions
            .get(&command.name)
            .ok_or_else(|| invalid(format!("missing execution declaration: {}", command.name)))?;
        if !Path::new(&execution.program).is_absolute()
            || execution.program.contains('\0')
            || execution.toolchain_files.is_empty()
            || execution.toolchain_files.iter().any(|(name, path)| {
                name.is_empty() || !Path::new(path).is_absolute() || path.contains('\0')
            })
            || (execution.directory != "." && !relative(&execution.directory))
            || execution.timeout_seconds == 0
            || execution.timeout_seconds > 7200
        {
            return Err(invalid(format!(
                "invalid explicit execution/toolchain/budget: {}",
                command.name
            )));
        }
        // A reviewed canonical program may resolve a named installed command;
        // absolute proof commands must match it exactly. No silent substitution.
        let declared = Path::new(&command.command);
        if (declared.is_absolute() && command.command != execution.program)
            || (!declared.is_absolute()
                && (declared.components().count() != 1
                    || declared.file_name() != Path::new(&execution.program).file_name()))
        {
            return Err(invalid(format!(
                "execution program differs from proof command: {}",
                command.name
            )));
        }
    }
    if contract.executions.keys().cloned().collect::<BTreeSet<_>>() != names {
        return Err(invalid(
            "execution declarations must exactly cover required commands",
        ));
    }
    let expected: BTreeSet<_> = sources
        .files
        .keys()
        .filter(|source| super::init_reconciliation::is_workflow(source))
        .cloned()
        .collect();
    let mut covered = BTreeSet::new();
    let mut mapped = BTreeSet::new();
    for row in &contract.source_coverage {
        if !expected.contains(&row.source)
            || !covered.insert(row.source.clone())
            || row.commands.is_empty()
        {
            return Err(invalid(
                "coverage must identify each discovered workflow exactly once, without exclusions",
            ));
        }
        let mut row_names = BTreeSet::new();
        for name in &row.commands {
            if !names.contains(name) || !row_names.insert(name) {
                return Err(invalid(
                    "source coverage references an unknown or duplicate required command",
                ));
            }
            mapped.insert(name.clone());
        }
    }
    if covered != expected {
        return Err(invalid(
            "one or more discovered workflows have no reviewed local proof mapping",
        ));
    }
    // Repositories without hosted workflows still have their declared local
    // build/test contract. All commands execute regardless of source mapping.
    if !expected.is_empty() && mapped != names {
        return Err(invalid(
            "every required command must appear in the reviewed workflow coverage",
        ));
    }
    Ok(())
}

pub fn required_catalog(
    declaration: &ProfileDeclaration,
    sources: &SourceInventory,
    facts: &ApplicabilityFacts,
    commands: &[ProofDef],
    contract: &RepositoryContract,
) -> Result<RequiredCatalog, PlanningFailure> {
    validate_contract(declaration, sources, commands, contract)?;
    if facts.event != "pull_request" || facts.target_branch.trim().is_empty() {
        return Err(PlanningFailure::InvalidFacts);
    }
    let source_inventory_digest = sources.digest()?;
    let mut obligations = Vec::new();
    for command in commands {
        let key = CheckKey {
            provider: Provider::Native,
            workflow: ".decapod/config.toml".into(),
            job: command.name.clone(),
            matrix: BTreeMap::new(),
            profile: REQUIRED_PROFILE.into(),
        };
        let definition_digest = super::readiness_evidence::digest(&(
            &source_inventory_digest,
            &contract.reviewed_commands_digest,
            command,
            &contract.executions[&command.name],
        ))
        .map_err(|_| PlanningFailure::Serialization)?;
        obligations.push(Obligation {
            key,
            kind: CheckKind::RepositoryLocal,
            role: InvocationRole::SourceCheck,
            definition_digest,
        });
    }
    let mut stages: Vec<_> = obligations
        .iter()
        .map(|row| StageDisposition {
            stage: row.key.job.clone(),
            key: Some(row.key.clone()),
            disposition: StageDispositionKind::Real,
            policy: REQUIRED_PROFILE.into(),
            reason:
                "Required repository-declared local command; actual bounded execution is mandatory"
                    .into(),
            evidence_scope: READINESS_SCOPE.into(),
        })
        .collect();
    stages.push(StageDisposition { stage: "hosted-ci-and-release".into(), key: None,
        disposition: StageDispositionKind::NonApplicable, policy: REQUIRED_PROFILE.into(),
        reason: "Local contract mappings do not attest hosted workflow, platform matrix, release packaging or deployment execution".into(),
        evidence_scope: "no_hosted_ci_or_release_assurance".into() });
    Ok(RequiredCatalog::repository_local(
        obligations,
        stages,
        facts.clone(),
        source_inventory_digest,
    ))
}

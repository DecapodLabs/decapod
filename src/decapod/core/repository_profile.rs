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
    /// Opt-in connection to an already running, independently provisioned engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dagger: Option<DaggerExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DaggerExecution {
    /// Explicit supported argv/config protocol; not a claim about other CLI releases.
    pub cli_contract: String,
    /// Full immutable Docker container ID, never a mutable name.
    pub engine_container: String,
    /// Docker's content-addressed image configuration ID (sha256), not a tag.
    pub engine_image: String,
    /// Explicit local Docker socket; remote endpoints and ambient contexts are unsupported.
    pub docker_socket: String,
}

impl DaggerExecution {
    pub fn validate(
        &self,
        command: &ProofDef,
        execution: &RepositoryExecution,
        sources: &SourceInventory,
    ) -> Result<(), PlanningFailure> {
        let hex = |v: &str| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        if self.cli_contract != "dagger-1.0-workspace-check.v1"
            || !hex(&self.engine_container)
            || !self.engine_image.strip_prefix("sha256:").is_some_and(hex)
            || !Path::new(&self.docker_socket).is_absolute()
            || self.docker_socket.contains(['\0', '\n', '\r'])
            || Path::new(&execution.program)
                .file_name()
                .and_then(|n| n.to_str())
                != Some("dagger")
            || execution.directory != "."
            || command.args != ["check"]
            || !execution.toolchain_files.contains_key("dagger-docker")
            || execution.environment.keys().any(|k| {
                k.starts_with("DAGGER_")
                    || k.starts_with("_EXPERIMENTAL_DAGGER_")
                    || k.starts_with("DOCKER_")
            })
            || !sources.files.contains_key("dagger.toml")
            || !sources.files.contains_key("dagger.lock")
        {
            return Err(invalid(
                "DAGGER_CONTRACT_INVALID: explicit pinned local engine, inventoried module/lock, measured docker and a single check command are required",
            ));
        }
        Ok(())
    }
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
        if let Some(dagger) = &execution.dagger {
            dagger.validate(command, execution, sources)?;
        } else if Path::new(&execution.program)
            .file_name()
            .and_then(|n| n.to_str())
            == Some("dagger")
        {
            return Err(invalid(
                "DAGGER_CONTRACT_REQUIRED: engine execution is not a local-only command",
            ));
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

/// Candidate configuration cannot authorize its own external engine boundary.
pub fn preserve_accepted_dagger(
    candidate: Option<&RepositoryContract>,
    accepted: Option<&RepositoryContract>,
) -> Result<(), PlanningFailure> {
    if let Some(contract) = candidate {
        for (name, execution) in &contract.executions {
            if execution.dagger.is_some()
                && accepted.and_then(|c| c.executions.get(name)) != Some(execution)
            {
                return Err(invalid(
                    "DAGGER_POLICY_NOT_ACCEPTED: exact execution and engine contract must already be accepted on the target",
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod dagger_contract_tests {
    use super::*;
    fn fixture() -> (
        DaggerExecution,
        ProofDef,
        RepositoryExecution,
        SourceInventory,
    ) {
        let policy = DaggerExecution {
            cli_contract: "dagger-1.0-workspace-check.v1".into(),
            engine_container: "a".repeat(64),
            engine_image: format!("sha256:{}", "b".repeat(64)),
            docker_socket: "/var/run/docker.sock".into(),
        };
        let command = ProofDef {
            name: "dagger-check".into(),
            command: "dagger".into(),
            args: vec!["check".into()],
            description: String::new(),
            required: true,
        };
        let execution = RepositoryExecution {
            program: "/opt/reviewed/bin/dagger".into(),
            toolchain_files: BTreeMap::from([(
                "dagger-docker".into(),
                "/opt/reviewed/bin/docker".into(),
            )]),
            environment: BTreeMap::new(),
            directory: ".".into(),
            timeout_seconds: 30,
            dagger: Some(policy.clone()),
        };
        let sources = SourceInventory {
            files: BTreeMap::from([
                ("dagger.toml".into(), format!("sha256:{}", "c".repeat(64))),
                ("dagger.lock".into(), format!("sha256:{}", "d".repeat(64))),
            ]),
        };
        (policy, command, execution, sources)
    }
    #[test]
    fn dagger_requires_explicit_module_lock_and_pinned_engine() {
        let (policy, command, execution, sources) = fixture();
        assert!(policy.validate(&command, &execution, &sources).is_ok());
        let mut missing = sources.clone();
        missing.files.remove("dagger.lock");
        assert!(policy.validate(&command, &execution, &missing).is_err());
        let mut named = policy.clone();
        named.engine_container = "dagger-engine".into();
        assert!(named.validate(&command, &execution, &sources).is_err());
        let mut tagged = policy.clone();
        tagged.engine_image = "registry.dagger.io/engine:latest".into();
        assert!(tagged.validate(&command, &execution, &sources).is_err());
    }
    #[test]
    fn dagger_cannot_override_engine_source_or_execute_arbitrary_functions() {
        let (policy, command, execution, sources) = fixture();
        for args in [
            vec!["check", "--engine=cloud"],
            vec!["call", "publish"],
            vec!["--mod", "/tmp/unbound", "check"],
        ] {
            let mut changed = command.clone();
            changed.args = args.into_iter().map(str::to_owned).collect();
            assert!(policy.validate(&changed, &execution, &sources).is_err());
        }
        for key in [
            "DAGGER_ENGINE",
            "DOCKER_HOST",
            "_EXPERIMENTAL_DAGGER_RUNNER_HOST",
            "DOCKER_CONTEXT",
        ] {
            let mut changed = execution.clone();
            changed.environment.insert(key.into(), "override".into());
            assert!(policy.validate(&command, &changed, &sources).is_err());
        }
        let mut escaped = policy.clone();
        escaped.cli_contract = "legacy-unknown".into();
        assert!(escaped.validate(&command, &execution, &sources).is_err());
    }
    #[test]
    fn candidate_cannot_authorize_new_or_changed_engine() {
        let (_, _, execution, _) = fixture();
        let accepted = RepositoryContract {
            reviewed_commands_digest: String::new(),
            source_coverage: vec![],
            executions: BTreeMap::from([("dagger-check".into(), execution)]),
        };
        assert!(preserve_accepted_dagger(Some(&accepted), None).is_err());
        assert!(preserve_accepted_dagger(Some(&accepted), Some(&accepted)).is_ok());
        let mut changed = accepted.clone();
        changed
            .executions
            .get_mut("dagger-check")
            .unwrap()
            .dagger
            .as_mut()
            .unwrap()
            .engine_container = "f".repeat(64);
        assert!(preserve_accepted_dagger(Some(&changed), Some(&accepted)).is_err());
        assert!(preserve_accepted_dagger(None, None).is_ok());
    }
    #[test]
    fn dagger_engine_contract_participates_in_review_digest() {
        let (policy, command, execution, _) = fixture();
        let mut contract = RepositoryContract {
            reviewed_commands_digest: String::new(),
            source_coverage: vec![],
            executions: BTreeMap::from([("dagger-check".into(), execution)]),
        };
        let before = command_contract_digest(std::slice::from_ref(&command), &contract).unwrap();
        contract
            .executions
            .get_mut("dagger-check")
            .unwrap()
            .dagger
            .as_mut()
            .unwrap()
            .engine_image = format!("sha256:{}", "e".repeat(64));
        assert_ne!(
            before,
            command_contract_digest(&[command], &contract).unwrap()
        );
        assert_eq!(policy.cli_contract, "dagger-1.0-workspace-check.v1");
    }
}

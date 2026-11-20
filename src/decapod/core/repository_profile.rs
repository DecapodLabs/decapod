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
pub const POLICY_TRANSITION_SCOPE: &str = REQUIRED_PROFILE;
pub const READINESS_SCOPE: &str = "repository_local_readiness";
pub const LOCAL_GIT_READINESS_SCOPE: &str = "repository_local_git_readiness";

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SourceCoverage {
    pub source: String,
    /// Mapped required local commands. May be omitted entirely on a reviewed
    /// `ci_only` row (#1531), which declares no local proof instead.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Reviewed declaration that this workflow has no local proof mapping
    /// (#1531), e.g. a Nix evaluation or Dagger call. Requires a nonempty
    /// `reason` and an empty `commands` list; the row is pinned by
    /// `reviewed_commands_digest` and reported as an explicit limitation, never
    /// counted as a passed check. Defaults keep pre-#1531 digests stable.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ci_only: bool,
    /// Reviewed reason required together with `ci_only = true`; rejected
    /// without it, so a limitation is never unexplained.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

pub const CANONICAL_PLATFORM_KEYS: [&str; 4] = [
    "linux-aarch64",
    "linux-x86_64",
    "macos-aarch64",
    "macos-x86_64",
];

pub fn platform_key(platform: super::native_proof::NativePlatform) -> Option<&'static str> {
    match platform {
        super::native_proof::NativePlatform::LinuxX86_64 => Some("linux-x86_64"),
        super::native_proof::NativePlatform::LinuxAarch64 => Some("linux-aarch64"),
        super::native_proof::NativePlatform::MacosX86_64 => Some("macos-x86_64"),
        super::native_proof::NativePlatform::MacosAarch64 => Some("macos-aarch64"),
        super::native_proof::NativePlatform::WindowsX86_64 => None,
    }
}

pub fn is_valid_platform_key(key: &str) -> bool {
    CANONICAL_PLATFORM_KEYS.contains(&key)
}

fn root_directory() -> String {
    ".".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlatformExecution {
    /// Installed canonical executable for this platform, never a candidate-produced executable.
    pub program: String,
    /// Reviewed transitive tool/runtime files for this platform. Completeness is
    /// authored, while every declared byte and the direct executable are measured
    /// by the runner.
    pub toolchain_files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryExecution {
    /// Installed canonical executable, never a candidate-produced executable.
    /// Required for single-platform declarations (`platforms` empty) and must
    /// be omitted when `platforms` is non-empty.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub program: String,
    /// Reviewed transitive tool/runtime files. Completeness is authored, while
    /// every declared byte and the direct executable are measured by the runner.
    /// Required for single-platform declarations (`platforms` empty) and must
    /// be omitted when `platforms` is non-empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub toolchain_files: BTreeMap<String, String>,
    /// Reviewed per-platform `program` + `toolchain_files` declarations keyed
    /// by canonical platform key (`linux-x86_64`, `linux-aarch64`,
    /// `macos-x86_64`, `macos-aarch64`). Empty for single-platform declarations
    /// so legacy `command_contract_digest` values remain byte-identical.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub platforms: BTreeMap<String, PlatformExecution>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "root_directory")]
    pub directory: String,
    pub timeout_seconds: u64,
}

impl RepositoryExecution {
    /// Resolve the reviewed executable path and toolchain closure for `platform`.
    /// Single-platform declarations (`platforms` empty) return their sole
    /// executable; per-platform declarations select only the exact matching
    /// canonical platform key and fail closed if missing or unsupported.
    pub fn resolve_for_platform(
        &self,
        platform: super::native_proof::NativePlatform,
    ) -> Result<(&str, &BTreeMap<String, String>), PlanningFailure> {
        if self.platforms.is_empty() {
            if self.program.is_empty() || self.toolchain_files.is_empty() {
                return Err(invalid(
                    "missing single-platform execution program or toolchain_files",
                ));
            }
            return Ok((self.program.as_str(), &self.toolchain_files));
        }
        if !self.program.is_empty() || !self.toolchain_files.is_empty() {
            return Err(invalid(
                "mixed top-level program/toolchain_files and per-platform declarations are unsupported",
            ));
        }
        let key = platform_key(platform).ok_or_else(|| {
            invalid(
                "UNSUPPORTED_PLATFORM: current host platform has no canonical per-platform execution key",
            )
        })?;
        let selected = self.platforms.get(key).ok_or_else(|| {
            invalid(format!(
                "MISSING_PLATFORM_EXECUTION: no reviewed execution entry for host platform {key}"
            ))
        })?;
        Ok((selected.program.as_str(), &selected.toolchain_files))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryContract {
    /// Explicit local authority; must already be accepted on the canonical
    /// repository target. Absence preserves the existing GitHub transport.
    #[serde(default, skip_serializing_if = "is_false")]
    pub canonical_local_target: bool,
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
    let original = (
        REQUIRED_PROFILE,
        commands,
        &contract.source_coverage,
        &contract.executions,
    );
    // Preserve all pre-local-target review digests byte for byte.
    if contract.canonical_local_target {
        super::readiness_evidence::digest(&(original, "canonical_local_git.v1"))
    } else {
        super::readiness_evidence::digest(&original)
    }
    .map_err(|_| PlanningFailure::Serialization)
}

fn invalid(message: impl Into<String>) -> PlanningFailure {
    PlanningFailure::InvalidPlan(format!("REPOSITORY_CONTRACT_INVALID: {}", message.into()))
}

/// Candidate review digests establish internal consistency, not authority to
/// replace requirements already accepted on the target. Additive checks are
/// allowed; accepted execution and coverage obligations must remain intact.
pub(crate) fn preserve_accepted(
    commands: &[ProofDef],
    contract: Option<&RepositoryContract>,
    accepted_commands: &[ProofDef],
    accepted_contract: Option<&RepositoryContract>,
) -> Result<(), PlanningFailure> {
    let changed = || {
        invalid(
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: preserve the accepted local checks, execution declarations and coverage; replacement requires a separately accepted policy transition, not a candidate-authored review digest",
        )
    };
    let accepted = accepted_contract.ok_or_else(changed)?;
    let candidate = contract.ok_or_else(changed)?;
    if accepted.canonical_local_target != candidate.canonical_local_target
        || accepted_commands.is_empty()
        || command_contract_digest(accepted_commands, accepted)?
            != accepted.reviewed_commands_digest
    {
        return Err(changed());
    }
    for required in accepted_commands {
        let current = commands
            .iter()
            .find(|command| command.name == required.name)
            .ok_or_else(changed)?;
        if !required.required
            || !current.required
            || current.command != required.command
            || current.args != required.args
            || !accepted.executions.contains_key(&required.name)
            || candidate.executions.get(&required.name) != accepted.executions.get(&required.name)
        {
            return Err(changed());
        }
    }
    for coverage in &accepted.source_coverage {
        let current = candidate
            .source_coverage
            .iter()
            .find(|row| row.source == coverage.source)
            .ok_or_else(changed)?;
        if coverage.ci_only {
            // A candidate cannot turn an accepted external limitation into an
            // unsupported local-equivalence claim.
            if current != coverage {
                return Err(changed());
            }
        } else if current.ci_only
            || !coverage
                .commands
                .iter()
                .all(|name| current.commands.contains(name))
        {
            return Err(changed());
        }
    }
    Ok(())
}

/// Canonical deterministic policy-transition approval identifier bound to the
/// exact accepted base, candidate head, source-inventory digests, and
/// command-contract digests under scope `"repo.pr-readiness.v1"`.
pub fn repo_policy_transition_id(
    accepted_base_sha: &str,
    candidate_head_sha: &str,
    accepted_sources_digest: &str,
    candidate_sources_digest: &str,
    accepted_commands_digest: &str,
    candidate_commands_digest: &str,
) -> Result<String, PlanningFailure> {
    let raw = super::readiness_evidence::digest(&(
        "repo.pr-readiness.policy-transition.v1",
        POLICY_TRANSITION_SCOPE,
        accepted_base_sha,
        candidate_head_sha,
        accepted_sources_digest,
        candidate_sources_digest,
        accepted_commands_digest,
        candidate_commands_digest,
    ))
    .map_err(|_| PlanningFailure::Serialization)?;
    Ok(raw.strip_prefix("sha256:").unwrap_or(&raw).to_string())
}

pub(crate) fn has_active_policy_transition_approval(
    checkout_root: &Path,
    transition_id: &str,
) -> Result<bool, PlanningFailure> {
    let Ok(main_root) = super::workspace::get_main_repo_root(checkout_root) else {
        return Ok(false);
    };
    let data_root = main_root.join(".decapod").join("data");
    let db_path = crate::plugins::policy::policy_db_path(&data_root);
    match std::fs::symlink_metadata(&db_path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Ok(false),
        Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
            return Err(invalid(
                "POLICY_STORE_UNSAFE: canonical policy datastore must be a regular file",
            ));
        }
        Ok(_) => {}
    }
    let Some(db_str) = db_path.to_str() else {
        return Err(invalid(
            "POLICY_STORE_UNSAFE: canonical policy datastore path is not valid UTF-8",
        ));
    };
    let conn = super::db::db_connect_for_validate(db_str)
        .map_err(|e| invalid(format!("POLICY_STORE_UNAVAILABLE: {e}")))?;
    crate::plugins::policy::has_live_scoped_approval_on_conn(
        &conn,
        transition_id,
        POLICY_TRANSITION_SCOPE,
    )
    .map_err(|e| invalid(format!("POLICY_APPROVAL_CHECK_FAILED: {e}")))
}

/// Enforce accepted `repo.pr-readiness.v1` preservation unless the repository's
/// canonical main `.decapod/data` policy ledger holds an active, bounded,
/// `policy.control`-authorized approval for this exact base, head, source, and
/// command-contract transition.
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_accepted_transition(
    checkout_root: &Path,
    accepted_base_sha: &str,
    candidate_head_sha: &str,
    accepted_profile: Option<&ProfileDeclaration>,
    candidate_profile: Option<&ProfileDeclaration>,
    accepted_commands: &[ProofDef],
    candidate_commands: &[ProofDef],
    accepted_contract: Option<&RepositoryContract>,
    candidate_contract: Option<&RepositoryContract>,
) -> Result<(), PlanningFailure> {
    let Some(accepted_decl) = accepted_profile.filter(|p| p.id == REQUIRED_PROFILE) else {
        return Ok(());
    };
    let Some(candidate_decl) = candidate_profile.filter(|p| p.id == REQUIRED_PROFILE) else {
        return Err(invalid(
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: another profile cannot replace accepted repository-local checks",
        ));
    };
    let accepted = accepted_contract.ok_or_else(|| {
        invalid("ACCEPTED_REPOSITORY_CONTRACT_CHANGED: accepted repository contract is missing")
    })?;
    let candidate = candidate_contract.ok_or_else(|| {
        invalid("ACCEPTED_REPOSITORY_CONTRACT_CHANGED: preserve the accepted local checks, execution declarations and coverage; candidate repository_contract is missing")
    })?;
    if accepted.canonical_local_target != candidate.canonical_local_target {
        return Err(invalid(
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: canonical_local_target cannot change through a candidate policy transition",
        ));
    }
    if accepted_commands.is_empty()
        || command_contract_digest(accepted_commands, accepted)?
            != accepted.reviewed_commands_digest
    {
        return Err(invalid(
            "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: accepted repository contract digest is invalid",
        ));
    }
    let sources_unchanged =
        candidate_decl.reviewed_sources_digest == accepted_decl.reviewed_sources_digest;
    let commands_preserved = preserve_accepted(
        candidate_commands,
        Some(candidate),
        accepted_commands,
        Some(accepted),
    )
    .is_ok();
    if sources_unchanged && commands_preserved {
        return Ok(());
    }
    if command_contract_digest(candidate_commands, candidate)? != candidate.reviewed_commands_digest
    {
        return Err(invalid(
            "STALE_COMMAND_CONTRACT: review commands, execution inputs and source coverage before updating the digest",
        ));
    }
    let transition_id = repo_policy_transition_id(
        accepted_base_sha,
        candidate_head_sha,
        &accepted_decl.reviewed_sources_digest,
        &candidate_decl.reviewed_sources_digest,
        &accepted.reviewed_commands_digest,
        &candidate.reviewed_commands_digest,
    )?;
    if has_active_policy_transition_approval(checkout_root, &transition_id)? {
        return Ok(());
    }
    Err(invalid(format!(
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: repository proof policy or source inventory changed without an active policy.control transition approval (base={accepted_base_sha}, head={candidate_head_sha}, sources: {} -> {}, commands: {} -> {}). Review the diff and run `decapod govern policy approve --id {transition_id} --scope {POLICY_TRANSITION_SCOPE}` as a policy.control-authorized maintainer before revalidation",
        accepted_decl.reviewed_sources_digest,
        candidate_decl.reviewed_sources_digest,
        accepted.reviewed_commands_digest,
        candidate.reviewed_commands_digest,
    )))
}
fn relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0'])
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn validate_execution_target(
    command: &ProofDef,
    program: &str,
    toolchain_files: &BTreeMap<String, String>,
) -> Result<(), PlanningFailure> {
    if !Path::new(program).is_absolute()
        || program.contains('\0')
        || toolchain_files.is_empty()
        || toolchain_files.iter().any(|(name, path)| {
            name.trim().is_empty()
                || name == "program"
                || !Path::new(path).is_absolute()
                || path.contains('\0')
        })
    {
        return Err(invalid(format!(
            "invalid explicit execution/toolchain/budget: {}",
            command.name
        )));
    }
    // A reviewed canonical program may resolve a named installed command;
    // absolute proof commands must match it exactly. No silent substitution.
    let declared = Path::new(&command.command);
    if (declared.is_absolute() && command.command != program)
        || (!declared.is_absolute()
            && (declared.components().count() != 1
                || declared.file_name() != Path::new(program).file_name()))
    {
        return Err(invalid(format!(
            "execution program differs from proof command: {}",
            command.name
        )));
    }
    Ok(())
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
        if (execution.directory != "." && !relative(&execution.directory))
            || execution.timeout_seconds == 0
            || execution.timeout_seconds > 7200
        {
            return Err(invalid(format!(
                "invalid explicit execution/toolchain/budget: {}",
                command.name
            )));
        }
        if execution.platforms.is_empty() {
            validate_execution_target(command, &execution.program, &execution.toolchain_files)?;
        } else {
            if !execution.program.is_empty() || !execution.toolchain_files.is_empty() {
                return Err(invalid(format!(
                    "execution {} cannot mix top-level program/toolchain_files with platforms",
                    command.name
                )));
            }
            for (platform_key, platform_exec) in &execution.platforms {
                if !is_valid_platform_key(platform_key) {
                    return Err(invalid(format!(
                        "unsupported platform key {platform_key:?} in execution {}",
                        command.name
                    )));
                }
                validate_execution_target(
                    command,
                    &platform_exec.program,
                    &platform_exec.toolchain_files,
                )?;
            }
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
        if !expected.contains(&row.source) || !covered.insert(row.source.clone()) {
            return Err(invalid(
                "coverage must identify each discovered workflow exactly once, without exclusions",
            ));
        }
        if row.ci_only {
            // #1531: an honest CI-only declaration is a reviewed limitation, not
            // a coverage claim, so it must carry a reason and no local commands.
            if row.commands.is_empty()
                && row
                    .reason
                    .as_deref()
                    .map(str::trim)
                    .is_none_or(str::is_empty)
            {
                return Err(invalid(
                    "ci_only coverage requires a nonempty reviewed reason",
                ));
            }
            if !row.commands.is_empty() {
                return Err(invalid(
                    "ci_only coverage must not map local commands; declare the limitation, not a false mapping",
                ));
            }
            continue;
        }
        if row.reason.is_some() {
            return Err(invalid("reason is only valid together with ci_only = true"));
        }
        if row.commands.is_empty() {
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
    // #1531: when every discovered workflow is a reviewed ci_only limitation,
    // no row maps commands by design, so the local contract stands on its own
    // and this invariant must not reject it.
    if !expected.is_empty() && !mapped.is_empty() && mapped != names {
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
    // #1531: a ci_only row is reported as an explicit reviewed limitation, so
    // validation-results.v3 shows it as NonApplicable and never counts it as a
    // passed check.
    for row in &contract.source_coverage {
        if row.ci_only {
            stages.push(StageDisposition {
                stage: row.source.clone(),
                key: None,
                disposition: StageDispositionKind::NonApplicable,
                policy: REQUIRED_PROFILE.into(),
                reason: format!(
                    "Reviewed CI-only limitation: no local proof mapping for this workflow; never counted as a passed check. Reason: {}",
                    row.reason.clone().unwrap_or_default()
                ),
                evidence_scope: READINESS_SCOPE.into(),
            });
        }
    }
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

#[cfg(test)]
#[path = "../../../tests/unit/core/repository_profile_tests.rs"]
mod tests;

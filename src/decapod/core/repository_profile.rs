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

#[path = "repository_policy_approval.rs"]
pub(crate) mod transition_approval;

pub const REQUIRED_PROFILE: &str = "repo.pr-readiness.v1";
pub const POLICY_TRANSITION_SCOPE: &str = REQUIRED_PROFILE;
pub const READINESS_SCOPE: &str = "repository_local_readiness";
pub const LOCAL_GIT_READINESS_SCOPE: &str = "repository_local_git_readiness";
pub const TARGET_READINESS_SCOPE: &str = "repository_target_readiness";
pub const LOCAL_GIT_TARGET_READINESS_SCOPE: &str = "repository_local_git_target_readiness";

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
pub struct DeploymentTargetContract {
    /// Required local proof command names executed for this deployment target slice.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Reviewed target names that this target depends on transitively.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// Repository-relative path prefixes or globs whose modifications expand
    /// target-scoped proof to include this target and its dependencies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_inputs: Vec<String>,
    /// Optional canonical host platform keys compatible with this target's
    /// proof execution. Empty allows any supported native executor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResolvedTargetScope {
    pub target: String,
    pub resolved_targets: Vec<String>,
    pub commands: BTreeSet<String>,
    pub shared_input_targets: BTreeSet<String>,
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
    /// Optional reviewed map of deployment targets/slices to their required
    /// command names, dependencies, shared inputs, and compatible platforms.
    /// Empty preserves pre-#1740 `command_contract_digest` values byte for byte.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, DeploymentTargetContract>,
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
    // Preserve all pre-local-target and pre-deployment-target review digests byte for byte.
    match (contract.canonical_local_target, contract.targets.is_empty()) {
        (false, true) => super::readiness_evidence::digest(&original),
        (true, true) => super::readiness_evidence::digest(&(original, "canonical_local_git.v1")),
        (false, false) => super::readiness_evidence::digest(&(original, &contract.targets)),
        (true, false) => super::readiness_evidence::digest(&(
            original,
            "canonical_local_git.v1",
            &contract.targets,
        )),
    }
    .map_err(|_| PlanningFailure::Serialization)
}

fn invalid(message: impl Into<String>) -> PlanningFailure {
    PlanningFailure::InvalidPlan(format!("REPOSITORY_CONTRACT_INVALID: {}", message.into()))
}

/// Candidate review digests establish internal consistency, not authority to
/// replace requirements already accepted on the target. Additive checks are
/// allowed; accepted execution, coverage and deployment target obligations
/// must remain intact. The target map requires review even for additions.
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
        || accepted.targets != candidate.targets
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

#[cfg(feature = "supabase-cloud")]
fn transition_id_from_config(
    base: &str,
    head: &str,
    accepted: &[u8],
    candidate: &[u8],
) -> Result<String, PlanningFailure> {
    let parse = |bytes| {
        super::validation_runtime::proof_configuration(bytes)
            .map_err(|_| invalid("POLICY_APPROVAL_CONFIG_INVALID"))
    };
    let accepted = parse(accepted)?;
    let candidate = parse(candidate)?;
    let subject = |config: super::validation_runtime::ProofConfiguration| {
        config
            .required_profile
            .filter(|profile| profile.id == REQUIRED_PROFILE)
            .zip(config.repository_contract)
            .ok_or_else(|| invalid("POLICY_APPROVAL_REPOSITORY_PROFILE_REQUIRED"))
    };
    let (accepted_profile, accepted_contract) = subject(accepted)?;
    let (candidate_profile, candidate_contract) = subject(candidate)?;
    repo_policy_transition_id(
        base,
        head,
        &accepted_profile.reviewed_sources_digest,
        &candidate_profile.reviewed_sources_digest,
        &accepted_contract.reviewed_commands_digest,
        &candidate_contract.reviewed_commands_digest,
    )
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
    if [checkout_root, main_root.as_path()]
        .into_iter()
        .any(|root| {
            transition_approval::git_object(root, &["ls-files", "-z", "--", ".decapod/data"])
                .map_or(true, |tracked| !tracked.is_empty())
        })
    {
        return Err(invalid(
            "POLICY_STORE_UNSAFE: canonical policy data must be outside tracked repository content",
        ));
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

/// Enforce accepted `repo.pr-readiness.v1` preservation unless an independent
/// authority approved this exact base, head, source and command transition.
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
    approval: transition_approval::ApprovalInput,
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
    let approved = if let Some(envelope) = approval.signed {
        let envelope = envelope
            .to_str()
            .ok_or_else(|| invalid("POLICY_APPROVAL_ENVELOPE_INVALID: expected UTF-8 JSON"))?;
        transition_approval::verify_signed(
            checkout_root,
            accepted_base_sha,
            &transition_id,
            envelope.as_bytes(),
        )?
    } else if approval.ci_local {
        false
    } else if transition_approval::cloud_backend(checkout_root, accepted_base_sha)? {
        match std::env::var(transition_approval::CLOUD_DECISION_ENV) {
            Ok(decision) => transition_approval::verify_cloud(
                checkout_root,
                accepted_base_sha,
                candidate_head_sha,
                &transition_id,
                &decision,
            )?,
            Err(_) => false,
        }
    } else {
        has_active_policy_transition_approval(checkout_root, &transition_id)?
    };
    if approved {
        return Ok(());
    }
    Err(invalid(format!(
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: repository proof policy or source inventory changed without an active transition approval (base={accepted_base_sha}, head={candidate_head_sha}, sources: {} -> {}, commands: {} -> {}). Review the exact diff. A local or Cloud maintainer can use `decapod govern policy approve --id {transition_id} --scope {POLICY_TRANSITION_SCOPE}`. CI requires a signed approval in {}. Cloud verification uses a decision ID in {} or a signed approval.",
        accepted_decl.reviewed_sources_digest,
        candidate_decl.reviewed_sources_digest,
        accepted.reviewed_commands_digest,
        candidate.reviewed_commands_digest,
        transition_approval::SIGNED_APPROVAL_ENV,
        transition_approval::CLOUD_DECISION_ENV,
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
    validate_targets(contract, &names)?;
    Ok(())
}

fn valid_target_name(name: &str) -> bool {
    !name.is_empty()
        && name == name.trim()
        && name.len() <= 128
        && !name.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

fn valid_shared_input_pattern(pattern: &str) -> bool {
    if pattern.is_empty()
        || pattern != pattern.trim()
        || pattern.len() > 512
        || pattern.starts_with('/')
        || pattern.contains(['\\', '\0', '\n', '\r', '?'])
        || pattern.contains("***")
    {
        return false;
    }
    let trimmed = pattern.strip_suffix('/').unwrap_or(pattern);
    if trimmed.is_empty() || trimmed.contains("//") {
        return false;
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    if segments.len() > 64 {
        return false;
    }
    !segments.iter().any(|seg| {
        seg.is_empty() || *seg == "." || *seg == ".." || (seg.contains("**") && *seg != "**")
    })
}

fn segment_glob_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_p, mut star_t) = (None, 0usize);
    while ti < t.len() {
        if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_p = Some(pi);
            star_t = ti;
            pi += 1;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn matches_shared_input(pattern: &str, path: &str) -> bool {
    if !valid_shared_input_pattern(pattern) || !relative(path) {
        return false;
    }
    if pattern.ends_with('/') {
        return path.starts_with(pattern);
    }
    if !pattern.contains('*') {
        return path == pattern || path.starts_with(&format!("{pattern}/"));
    }
    let pat_segs: Vec<&str> = pattern.split('/').collect();
    let path_segs: Vec<&str> = path.split('/').collect();
    if pat_segs.len() > 64 || path_segs.len() > 64 {
        return false;
    }
    let m = pat_segs.len();
    let n = path_segs.len();
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;
    for i in 1..=m {
        if pat_segs[i - 1] == "**" {
            dp[i][0] = dp[i - 1][0];
        }
        for j in 1..=n {
            if pat_segs[i - 1] == "**" {
                dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
            } else if dp[i - 1][j - 1] && segment_glob_match(pat_segs[i - 1], path_segs[j - 1]) {
                dp[i][j] = true;
            }
        }
    }
    dp[m][n]
}

fn check_target_cycles(contract: &RepositoryContract) -> Result<(), PlanningFailure> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Mark {
        Visiting,
        Visited,
    }
    fn visit<'a>(
        name: &'a str,
        contract: &'a RepositoryContract,
        marks: &mut BTreeMap<&'a str, Mark>,
    ) -> Result<(), PlanningFailure> {
        match marks.get(name).copied() {
            Some(Mark::Visited) => return Ok(()),
            Some(Mark::Visiting) => {
                return Err(invalid(format!(
                    "DEPLOYMENT_TARGET_CYCLE: cyclic depends_on involving target {name:?}"
                )));
            }
            None => {}
        }
        marks.insert(name, Mark::Visiting);
        let def = contract.targets.get(name).ok_or_else(|| {
            invalid(format!(
                "deployment target dependency {name:?} is not declared"
            ))
        })?;
        for dep in &def.depends_on {
            if dep == name {
                return Err(invalid(format!(
                    "DEPLOYMENT_TARGET_CYCLE: target {name:?} cannot depend on itself"
                )));
            }
            visit(dep, contract, marks)?;
        }
        marks.insert(name, Mark::Visited);
        Ok(())
    }
    let mut marks = BTreeMap::new();
    for name in contract.targets.keys() {
        visit(name, contract, &mut marks)?;
    }
    Ok(())
}

fn validate_targets(
    contract: &RepositoryContract,
    known_commands: &BTreeSet<String>,
) -> Result<(), PlanningFailure> {
    if contract.targets.len() > 256 {
        return Err(invalid("deployment targets exceed bounded count (256)"));
    }
    for (name, target) in &contract.targets {
        if !valid_target_name(name) {
            return Err(invalid(format!("invalid deployment target name {name:?}")));
        }
        if target.commands.is_empty() || target.commands.len() > 256 {
            return Err(invalid(format!(
                "deployment target {name:?} must declare a bounded nonempty command list"
            )));
        }
        let mut seen_commands = BTreeSet::new();
        for cmd in &target.commands {
            if !known_commands.contains(cmd) || !seen_commands.insert(cmd) {
                return Err(invalid(format!(
                    "deployment target {name:?} references unknown or duplicate command {cmd:?}"
                )));
            }
        }
        if target.depends_on.len() > 256 {
            return Err(invalid(format!(
                "deployment target {name:?} depends_on exceeds bounded count (256)"
            )));
        }
        let mut seen_deps = BTreeSet::new();
        for dep in &target.depends_on {
            if dep == name || !contract.targets.contains_key(dep) || !seen_deps.insert(dep) {
                return Err(invalid(format!(
                    "deployment target {name:?} has unknown, cyclic or duplicate depends_on {dep:?}"
                )));
            }
        }
        if target.shared_inputs.len() > 256 {
            return Err(invalid(format!(
                "deployment target {name:?} shared_inputs exceed bounded count (256)"
            )));
        }
        let mut seen_inputs = BTreeSet::new();
        for pattern in &target.shared_inputs {
            if !valid_shared_input_pattern(pattern) || !seen_inputs.insert(pattern) {
                return Err(invalid(format!(
                    "deployment target {name:?} has invalid or duplicate shared_inputs pattern {pattern:?}"
                )));
            }
        }
        if target.platforms.len() > 16 {
            return Err(invalid(format!(
                "deployment target {name:?} platforms exceed bounded count (16)"
            )));
        }
        let mut seen_platforms = BTreeSet::new();
        for platform in &target.platforms {
            if !is_valid_platform_key(platform) || !seen_platforms.insert(platform) {
                return Err(invalid(format!(
                    "deployment target {name:?} has unsupported or duplicate platform key {platform:?}"
                )));
            }
        }
    }
    check_target_cycles(contract)
}

/// Resolve the transitive `depends_on` closure for `target`, expanded with any
/// additional targets whose `shared_inputs` intersect `changed_paths` (and
/// their transitive dependencies). Fails closed if diff context is missing,
/// `target` is unknown, or the target dependency graph is invalid/cyclic.
pub fn resolve_target_closure(
    contract: &RepositoryContract,
    target: &str,
    changed_paths: Option<&BTreeSet<String>>,
) -> Result<ResolvedTargetScope, PlanningFailure> {
    let changed_paths = changed_paths.ok_or_else(|| {
        invalid(
            "MISSING_SHARED_INPUT_DIFF_CONTEXT: changed paths between accepted base and candidate are required to resolve deployment target closure",
        )
    })?;
    if changed_paths.iter().any(|path| !relative(path)) {
        return Err(PlanningFailure::InvalidFacts);
    }
    if !valid_target_name(target) || !contract.targets.contains_key(target) {
        return Err(invalid(format!(
            "UNKNOWN_DEPLOYMENT_TARGET: deployment target {target:?} is not declared in proof.repository_contract.targets"
        )));
    }
    let known_commands: BTreeSet<String> = contract.executions.keys().cloned().collect();
    validate_targets(contract, &known_commands)?;

    let mut shared_input_targets = BTreeSet::new();
    let mut resolved_set = BTreeSet::new();
    let mut stack = vec![target.to_string()];
    for (candidate_name, candidate_def) in &contract.targets {
        if candidate_def.shared_inputs.iter().any(|pattern| {
            changed_paths
                .iter()
                .any(|changed| matches_shared_input(pattern, changed))
        }) {
            shared_input_targets.insert(candidate_name.clone());
            stack.push(candidate_name.clone());
        }
    }
    while let Some(current) = stack.pop() {
        if !resolved_set.insert(current.clone()) {
            continue;
        }
        let def = contract.targets.get(&current).ok_or_else(|| {
            invalid(format!(
                "UNKNOWN_DEPLOYMENT_TARGET: dependency {current:?} missing"
            ))
        })?;
        for dep in &def.depends_on {
            if !resolved_set.contains(dep) {
                stack.push(dep.clone());
            }
        }
    }
    let mut commands = BTreeSet::new();
    let resolved_targets: Vec<String> = resolved_set.into_iter().collect();
    for name in &resolved_targets {
        let def = &contract.targets[name];
        commands.extend(def.commands.iter().cloned());
    }
    if commands.is_empty() {
        return Err(invalid(format!(
            "deployment target {target:?} resolved to an empty command closure"
        )));
    }
    Ok(ResolvedTargetScope {
        target: target.to_string(),
        resolved_targets,
        commands,
        shared_input_targets,
    })
}

pub fn required_catalog(
    declaration: &ProfileDeclaration,
    sources: &SourceInventory,
    facts: &ApplicabilityFacts,
    commands: &[ProofDef],
    contract: &RepositoryContract,
) -> Result<RequiredCatalog, PlanningFailure> {
    required_catalog_for_target(declaration, sources, facts, commands, contract, None)
}

pub fn required_catalog_for_target(
    declaration: &ProfileDeclaration,
    sources: &SourceInventory,
    facts: &ApplicabilityFacts,
    commands: &[ProofDef],
    contract: &RepositoryContract,
    target: Option<&str>,
) -> Result<RequiredCatalog, PlanningFailure> {
    validate_contract(declaration, sources, commands, contract)?;
    if facts.event != "pull_request" || facts.target_branch.trim().is_empty() {
        return Err(PlanningFailure::InvalidFacts);
    }
    let target_scope = match target {
        Some(target_name) => Some(resolve_target_closure(
            contract,
            target_name,
            Some(&facts.changed_paths),
        )?),
        None => None,
    };
    let source_inventory_digest = sources.digest()?;
    let catalog_inventory_digest = match &target_scope {
        Some(scope) => super::readiness_evidence::digest(&(&source_inventory_digest, scope))
            .map_err(|_| PlanningFailure::Serialization)?,
        None => source_inventory_digest.clone(),
    };
    let evidence_scope = if target_scope.is_some() {
        TARGET_READINESS_SCOPE
    } else {
        READINESS_SCOPE
    };
    let mut obligations = Vec::new();
    let mut out_of_scope_stages = Vec::new();
    for command in commands {
        if let Some(scope) = &target_scope
            && !scope.commands.contains(&command.name)
        {
            out_of_scope_stages.push(StageDisposition {
                stage: command.name.clone(),
                key: None,
                disposition: StageDispositionKind::NonApplicable,
                policy: REQUIRED_PROFILE.into(),
                reason: format!(
                    "Command excluded by reviewed deployment target scope {:?} (resolved closure: {}); provides no full-repository or unrelated-target assurance",
                    scope.target,
                    scope.resolved_targets.join(", ")
                ),
                evidence_scope: "out_of_target_scope_no_assurance".into(),
            });
            continue;
        }
        let key = CheckKey {
            provider: Provider::Native,
            workflow: ".decapod/config.toml".into(),
            job: command.name.clone(),
            matrix: BTreeMap::new(),
            profile: REQUIRED_PROFILE.into(),
        };
        let definition_digest = match &target_scope {
            Some(scope) => super::readiness_evidence::digest(&(
                &source_inventory_digest,
                &contract.reviewed_commands_digest,
                command,
                &contract.executions[&command.name],
                scope,
            )),
            None => super::readiness_evidence::digest(&(
                &source_inventory_digest,
                &contract.reviewed_commands_digest,
                command,
                &contract.executions[&command.name],
            )),
        }
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
            evidence_scope: evidence_scope.into(),
        })
        .collect();
    stages.extend(out_of_scope_stages);
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
                evidence_scope: evidence_scope.into(),
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
        catalog_inventory_digest,
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/repository_profile_tests.rs"]
mod tests;

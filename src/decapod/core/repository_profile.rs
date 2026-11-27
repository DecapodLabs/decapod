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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
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
    if commands_preserved
        && candidate.reviewed_commands_digest == accepted.reviewed_commands_digest
        && (qualifies_release_only_inventory_transition(
            checkout_root,
            accepted_base_sha,
            &accepted_decl.reviewed_sources_digest,
            &candidate_decl.reviewed_sources_digest,
        ) || qualifies_non_proof_source_inventory_transition(
            checkout_root,
            accepted_base_sha,
            candidate_head_sha,
            &accepted_decl.reviewed_sources_digest,
            &candidate_decl.reviewed_sources_digest,
            accepted_commands,
            accepted,
        ) || qualifies_stale_accepted_base_inventory_reconciliation(
            checkout_root,
            accepted_base_sha,
            candidate_head_sha,
            &accepted_decl.reviewed_sources_digest,
            &candidate_decl.reviewed_sources_digest,
        ))
    {
        return Ok(());
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
    } else if transition_approval::cloud_backend(checkout_root, accepted_base_sha)? {
        // Cloud decision verification authenticates directly against the remote
        // control plane for the exact base/head/tree/transition tuple, so it
        // applies in both local and ci_local execution without local SQLite trust.
        match approval.cloud_decision {
            Some(decision) => {
                let decision = decision.to_str().ok_or_else(|| {
                    invalid("CLOUD_POLICY_APPROVAL_DECISION_ID_INVALID: expected UTF-8 UUID")
                })?;
                transition_approval::verify_cloud(
                    checkout_root,
                    accepted_base_sha,
                    candidate_head_sha,
                    &transition_id,
                    decision,
                )?
            }
            None => false,
        }
    } else if approval.ci_local {
        false
    } else {
        has_active_policy_transition_approval(checkout_root, &transition_id)?
    };
    if approved {
        return Ok(());
    }
    let changed_paths =
        changed_inventory_paths_between(checkout_root, accepted_base_sha, candidate_head_sha);
    Err(invalid(format!(
        "ACCEPTED_REPOSITORY_CONTRACT_CHANGED: repository proof policy or proof-relevant source inventory changed without an active transition approval (transition_id={transition_id}, scope={POLICY_TRANSITION_SCOPE}, base={accepted_base_sha}, head={candidate_head_sha}, sources: {} -> {}, commands: {} -> {}, changed_paths={changed_paths:?}). Review and merge the exact diff at PR review, or supply an independently verified approval for scope `{POLICY_TRANSITION_SCOPE}` via maintainer-signed receipt in {} or authenticated Cloud decision ID in {}.",
        accepted_decl.reviewed_sources_digest,
        candidate_decl.reviewed_sources_digest,
        accepted.reviewed_commands_digest,
        candidate.reviewed_commands_digest,
        transition_approval::SIGNED_APPROVAL_ENV,
        transition_approval::CLOUD_DECISION_ENV,
    )))
}

const RELEASE_ONLY_HISTORY_DEPTH: &str = "16";

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitTreeEntry {
    mode: String,
    oid: String,
    size: usize,
}

#[derive(Debug, Clone)]
struct GitTreeInventory {
    inventory: SourceInventory,
    entries: BTreeMap<String, GitTreeEntry>,
}

fn is_regular_git_mode(mode: &str) -> bool {
    mode == "100644" || mode == "100755"
}

fn is_valid_inventory_path(path: &str) -> bool {
    !path.is_empty()
        && path != ".decapod/config.toml"
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn source_discovery_roots() -> BTreeSet<&'static str> {
    super::init_reconciliation::SOURCE_DIRECTORIES
        .iter()
        .copied()
        .chain(super::init_reconciliation::SOURCE_FILES.iter().copied())
        .chain(
            super::readiness_profile::catalog_source_paths()
                .iter()
                .copied(),
        )
        .collect()
}

fn source_root_ancestors(roots: &BTreeSet<&str>) -> Vec<String> {
    let mut ancestors = BTreeSet::new();
    for root in roots {
        let mut cursor = *root;
        while let Some((parent, _)) = cursor.rsplit_once('/') {
            ancestors.insert(parent.to_owned());
            cursor = parent;
        }
    }
    ancestors.into_iter().collect()
}

fn relative_depth_under_roots(path: &str, roots: &BTreeSet<&str>) -> Option<usize> {
    if roots.contains(path) {
        return Some(0);
    }
    let mut best: Option<usize> = None;
    for root in roots {
        if let Some(tail) = path
            .strip_prefix(root)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            let depth = tail.split('/').count();
            best = Some(best.map_or(depth, |current| current.min(depth)));
        }
    }
    best
}

fn compute_git_tree_source_inventory(
    checkout_root: &Path,
    rev: &str,
    blob_cache: &mut BTreeMap<String, (usize, String)>,
) -> Option<GitTreeInventory> {
    use super::init_reconciliation::{
        MAX_DEPTH, MAX_ENTRIES, MAX_FILE_BYTES, MAX_TOTAL_BYTES, TOOLCHAIN_ALIAS, TOOLCHAIN_TARGET,
    };
    use sha2::{Digest, Sha256};

    let roots = source_discovery_roots();
    if roots.contains(".decapod/config.toml") {
        return None;
    }

    // Reject any symlink or submodule ancestor above discovery roots.
    // Group by component depth so no ancestor/descendant pair appears in the
    // same `git ls-tree` call (which would cause Git to list child entries).
    let ancestors = source_root_ancestors(&roots);
    let mut ancestors_by_depth: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
    for ancestor in &ancestors {
        let depth = ancestor.matches('/').count();
        ancestors_by_depth
            .entry(depth)
            .or_default()
            .push(ancestor.as_str());
    }
    for group in ancestors_by_depth.values() {
        let mut ancestor_args: Vec<&str> = vec!["ls-tree", "-z", rev, "--"];
        ancestor_args.extend(group.iter().copied());
        let ancestor_raw = transition_approval::git_object(checkout_root, &ancestor_args).ok()?;
        for record in ancestor_raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let text = std::str::from_utf8(record).ok()?;
            let (header, _) = text.split_once('\t')?;
            let mode = header.split_whitespace().next()?;
            if mode != "040000" {
                return None;
            }
        }
    }

    let mut tree_args: Vec<&str> = vec!["ls-tree", "-r", "-l", "-z", rev, "--"];
    tree_args.extend(roots.iter().copied());
    let tree_raw = transition_approval::git_object(checkout_root, &tree_args).ok()?;

    let mut entries = BTreeMap::new();
    let mut visited_dirs = BTreeSet::new();
    let mut total_bytes: usize = 0;

    for record in tree_raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let text = std::str::from_utf8(record).ok()?;
        let (header, path) = text.split_once('\t')?;
        if !is_valid_inventory_path(path) {
            return None;
        }
        let mut parts = header.split_whitespace();
        let mode = parts.next()?;
        let obj_type = parts.next()?;
        let oid = parts.next()?;
        let size_str = parts.next()?;
        if parts.next().is_some() || obj_type != "blob" {
            return None;
        }
        let is_alias = mode == "120000" && path == TOOLCHAIN_ALIAS;
        if !is_regular_git_mode(mode) && !is_alias {
            return None;
        }
        let depth = relative_depth_under_roots(path, &roots)?;
        if depth > MAX_DEPTH {
            return None;
        }
        let size: usize = size_str.parse().ok()?;
        if size > MAX_FILE_BYTES {
            return None;
        }
        total_bytes = total_bytes.checked_add(size)?;
        if total_bytes > MAX_TOTAL_BYTES {
            return None;
        }

        // Count intermediate directories visited under matching discovery roots.
        for root in &roots {
            if let Some(tail) = path
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                visited_dirs.insert((*root).to_owned());
                let mut acc = (*root).to_owned();
                let components: Vec<&str> = tail.split('/').collect();
                for dir in &components[..components.len().saturating_sub(1)] {
                    acc.push('/');
                    acc.push_str(dir);
                    visited_dirs.insert(acc.clone());
                }
            }
        }

        entries.insert(
            path.to_owned(),
            GitTreeEntry {
                mode: mode.to_owned(),
                oid: oid.to_owned(),
                size,
            },
        );
        if entries.len().saturating_add(visited_dirs.len()) > MAX_ENTRIES {
            return None;
        }
    }

    let mut files = BTreeMap::new();
    for (path, entry) in &entries {
        if entry.mode == "120000" {
            if path != TOOLCHAIN_ALIAS {
                return None;
            }
            let target_entry = entries.get(TOOLCHAIN_TARGET)?;
            if !is_regular_git_mode(&target_entry.mode) {
                return None;
            }
            let link_bytes =
                transition_approval::git_object(checkout_root, &["cat-file", "blob", &entry.oid])
                    .ok()?;
            if link_bytes.len() != entry.size || link_bytes != TOOLCHAIN_TARGET.as_bytes() {
                return None;
            }
            files.insert(
                path.clone(),
                format!("sha256:{:x}", Sha256::digest(&link_bytes)),
            );
            continue;
        }
        let digest = if let Some((cached_size, cached_digest)) = blob_cache.get(&entry.oid) {
            if *cached_size != entry.size {
                return None;
            }
            cached_digest.clone()
        } else {
            let bytes =
                transition_approval::git_object(checkout_root, &["cat-file", "blob", &entry.oid])
                    .ok()?;
            if bytes.len() != entry.size || bytes.len() > MAX_FILE_BYTES {
                return None;
            }
            let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
            blob_cache.insert(entry.oid.clone(), (bytes.len(), digest.clone()));
            digest
        };
        files.insert(path.clone(), digest);
    }

    Some(GitTreeInventory {
        inventory: SourceInventory { files },
        entries,
    })
}

fn parse_strict_semver(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let major = parts.next()?;
    let minor = parts.next()?;
    let patch = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let parse_component = |part: &str| -> Option<u64> {
        if part.is_empty()
            || !part.bytes().all(|b| b.is_ascii_digit())
            || (part.len() > 1 && part.starts_with('0'))
        {
            return None;
        }
        part.parse::<u64>().ok()
    };
    Some((
        parse_component(major)?,
        parse_component(minor)?,
        parse_component(patch)?,
    ))
}

fn parse_version_assignment_line(line: &str) -> Option<(&str, &str, &str)> {
    let first_quote = line.find('"')?;
    let prefix = &line[..=first_quote];
    let rest = &line[first_quote + 1..];
    let second_quote = rest.find('"')?;
    let version = &rest[..second_quote];
    let suffix = &rest[second_quote..];
    let before_quote = line[..first_quote].trim();
    let key = before_quote.strip_suffix('=')?.trim();
    if key != "version" {
        return None;
    }
    // Suffix starts with the closing quote; disallow trailing comments or tokens.
    if !suffix[1..].trim().is_empty() {
        return None;
    }
    Some((prefix, version, suffix))
}

fn parse_release_only_cargo_toml_bump(
    old_toml: &str,
    new_toml: &str,
) -> Option<(String, String, String)> {
    let old_lines: Vec<&str> = old_toml.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new_toml.split_inclusive('\n').collect();
    if old_lines.len() != new_lines.len() {
        return None;
    }
    let mut diff_index = None;
    for (idx, (old_line, new_line)) in old_lines.iter().zip(&new_lines).enumerate() {
        if old_line != new_line {
            if diff_index.is_some() {
                return None;
            }
            diff_index = Some(idx);
        }
    }
    let diff_idx = diff_index?;

    let mut current_section: Option<&str> = None;
    for line in &old_lines[..diff_idx] {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("[[") {
            current_section = None;
        } else if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            current_section = Some(inner.trim());
        }
    }
    if current_section != Some("package") {
        return None;
    }

    let (old_prefix, old_ver, old_suffix) = parse_version_assignment_line(old_lines[diff_idx])?;
    let (new_prefix, new_ver, new_suffix) = parse_version_assignment_line(new_lines[diff_idx])?;
    if old_prefix != new_prefix || old_suffix != new_suffix {
        return None;
    }
    if parse_strict_semver(new_ver)? <= parse_strict_semver(old_ver)? {
        return None;
    }

    let mut old_doc: toml::Value = toml::from_str(old_toml).ok()?;
    let new_doc: toml::Value = toml::from_str(new_toml).ok()?;
    let pkg_name = old_doc.get("package")?.get("name")?.as_str()?.to_owned();
    if pkg_name.is_empty() || new_doc.get("package")?.get("name")?.as_str()? != pkg_name {
        return None;
    }
    if old_doc.get("package")?.get("version")?.as_str()? != old_ver
        || new_doc.get("package")?.get("version")?.as_str()? != new_ver
    {
        return None;
    }
    old_doc
        .get_mut("package")?
        .as_table_mut()?
        .insert("version".into(), toml::Value::String(new_ver.to_owned()));
    if old_doc != new_doc {
        return None;
    }

    Some((pkg_name, old_ver.to_owned(), new_ver.to_owned()))
}

fn is_release_only_cargo_lock_bump(
    old_lock: &str,
    new_lock: &str,
    pkg_name: &str,
    old_ver: &str,
    new_ver: &str,
) -> bool {
    let old_lines: Vec<&str> = old_lock.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new_lock.split_inclusive('\n').collect();
    if old_lines.len() != new_lines.len() {
        return false;
    }
    let mut diff_index = None;
    for (idx, (old_line, new_line)) in old_lines.iter().zip(&new_lines).enumerate() {
        if old_line != new_line {
            if diff_index.is_some() {
                return false;
            }
            diff_index = Some(idx);
        }
    }
    let Some(diff_idx) = diff_index else {
        return false;
    };
    let Some((old_prefix, parsed_old, old_suffix)) =
        parse_version_assignment_line(old_lines[diff_idx])
    else {
        return false;
    };
    let Some((new_prefix, parsed_new, new_suffix)) =
        parse_version_assignment_line(new_lines[diff_idx])
    else {
        return false;
    };
    if old_prefix != new_prefix
        || old_suffix != new_suffix
        || parsed_old != old_ver
        || parsed_new != new_ver
    {
        return false;
    }

    let (Ok(mut old_doc), Ok(new_doc)) = (
        toml::from_str::<toml::Value>(old_lock),
        toml::from_str::<toml::Value>(new_lock),
    ) else {
        return false;
    };
    let (Some(old_pkgs), Some(new_pkgs)) = (
        old_doc.get("package").and_then(toml::Value::as_array),
        new_doc.get("package").and_then(toml::Value::as_array),
    ) else {
        return false;
    };
    if old_pkgs.len() != new_pkgs.len() {
        return false;
    }
    let mut diff_pkg = None;
    for (idx, (old_pkg, new_pkg)) in old_pkgs.iter().zip(new_pkgs).enumerate() {
        if old_pkg != new_pkg {
            if diff_pkg.is_some() {
                return false;
            }
            diff_pkg = Some(idx);
        }
    }
    let Some(pkg_idx) = diff_pkg else {
        return false;
    };
    let (Some(old_pkg), Some(new_pkg)) =
        (old_pkgs[pkg_idx].as_table(), new_pkgs[pkg_idx].as_table())
    else {
        return false;
    };
    if old_pkg.get("name").and_then(toml::Value::as_str) != Some(pkg_name)
        || new_pkg.get("name").and_then(toml::Value::as_str) != Some(pkg_name)
        || old_pkg.contains_key("source")
        || new_pkg.contains_key("source")
        || old_pkg.contains_key("checksum")
        || new_pkg.contains_key("checksum")
        || old_pkg.get("version").and_then(toml::Value::as_str) != Some(old_ver)
        || new_pkg.get("version").and_then(toml::Value::as_str) != Some(new_ver)
    {
        return false;
    }
    if let Some(pkg_table) = old_doc
        .get_mut("package")
        .and_then(toml::Value::as_array_mut)
        .and_then(|pkgs| pkgs.get_mut(pkg_idx))
        .and_then(toml::Value::as_table_mut)
    {
        pkg_table.insert("version".into(), toml::Value::String(new_ver.to_owned()));
    } else {
        return false;
    }
    old_doc == new_doc
}

fn read_bounded_utf8_blob(checkout_root: &Path, entry: &GitTreeEntry) -> Option<String> {
    if !is_regular_git_mode(&entry.mode) || entry.size > super::init_reconciliation::MAX_FILE_BYTES
    {
        return None;
    }
    let bytes =
        transition_approval::git_object(checkout_root, &["cat-file", "blob", &entry.oid]).ok()?;
    if bytes.len() != entry.size {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn is_release_only_cargo_bump(
    checkout_root: &Path,
    prior: &GitTreeInventory,
    base: &GitTreeInventory,
) -> bool {
    if prior.entries.len() != base.entries.len() || !prior.entries.keys().eq(base.entries.keys()) {
        return false;
    }
    for (path, base_entry) in &base.entries {
        if path != "Cargo.toml" && path != "Cargo.lock" {
            let Some(prior_entry) = prior.entries.get(path) else {
                return false;
            };
            if prior_entry.mode != base_entry.mode
                || prior_entry.oid != base_entry.oid
                || prior.inventory.files.get(path) != base.inventory.files.get(path)
            {
                return false;
            }
        }
    }

    let (Some(prior_toml_entry), Some(base_toml_entry)) = (
        prior.entries.get("Cargo.toml"),
        base.entries.get("Cargo.toml"),
    ) else {
        return false;
    };
    if prior_toml_entry.mode != base_toml_entry.mode || prior_toml_entry.oid == base_toml_entry.oid
    {
        return false;
    }
    let (Some(old_toml), Some(new_toml)) = (
        read_bounded_utf8_blob(checkout_root, prior_toml_entry),
        read_bounded_utf8_blob(checkout_root, base_toml_entry),
    ) else {
        return false;
    };
    let Some((pkg_name, old_ver, new_ver)) =
        parse_release_only_cargo_toml_bump(&old_toml, &new_toml)
    else {
        return false;
    };

    if let (Some(prior_lock_entry), Some(base_lock_entry)) = (
        prior.entries.get("Cargo.lock"),
        base.entries.get("Cargo.lock"),
    ) {
        if prior_lock_entry.mode != base_lock_entry.mode
            || prior_lock_entry.oid == base_lock_entry.oid
        {
            return false;
        }
        let (Some(old_lock), Some(new_lock)) = (
            read_bounded_utf8_blob(checkout_root, prior_lock_entry),
            read_bounded_utf8_blob(checkout_root, base_lock_entry),
        ) else {
            return false;
        };
        if !is_release_only_cargo_lock_bump(&old_lock, &new_lock, &pkg_name, &old_ver, &new_ver) {
            return false;
        }
    }

    true
}

/// Qualify an automatic `repo.pr-readiness.v1` source-inventory transition when
/// the candidate introduces zero reviewed-source changes relative to
/// `accepted_base_sha`, and `accepted_base_sha` itself advanced from a bounded
/// first-parent ancestor matching `accepted_sources_digest` solely via a strict
/// SemVer release version bump in root `Cargo.toml` (and matching root package
/// entry in `Cargo.lock`, when present).
pub(crate) fn qualifies_release_only_inventory_transition(
    checkout_root: &Path,
    accepted_base_sha: &str,
    accepted_sources_digest: &str,
    candidate_sources_digest: &str,
) -> bool {
    if accepted_sources_digest == candidate_sources_digest {
        return false;
    }
    let mut blob_cache = BTreeMap::new();
    let Some(base_tree) =
        compute_git_tree_source_inventory(checkout_root, accepted_base_sha, &mut blob_cache)
    else {
        return false;
    };
    if base_tree.inventory.digest().ok().as_deref() != Some(candidate_sources_digest) {
        return false;
    }

    let Ok(rev_list_raw) = transition_approval::git_object(
        checkout_root,
        &[
            "rev-list",
            "--first-parent",
            "--max-count",
            RELEASE_ONLY_HISTORY_DEPTH,
            accepted_base_sha,
        ],
    ) else {
        return false;
    };
    let Ok(rev_list) = std::str::from_utf8(&rev_list_raw) else {
        return false;
    };

    for prior_sha in rev_list.lines().map(str::trim).filter(|s| !s.is_empty()) {
        if prior_sha == accepted_base_sha {
            continue;
        }
        let Some(prior_tree) =
            compute_git_tree_source_inventory(checkout_root, prior_sha, &mut blob_cache)
        else {
            continue;
        };
        if prior_tree.inventory.digest().ok().as_deref() == Some(accepted_sources_digest) {
            return is_release_only_cargo_bump(checkout_root, &prior_tree, &base_tree);
        }
    }

    false
}

/// Qualify an automatic `repo.pr-readiness.v1` source-inventory reconciliation
/// when `accepted_base_sha` itself carries a stale `reviewed_sources_digest`
/// in `.decapod/config.toml` that does not match its own Git tree inventory,
/// `commands_preserved` and `reviewed_commands_digest` match verbatim, both
/// `accepted_base_sha` and `candidate_head_sha` have identical inventoried Git
/// trees matching `candidate_sources_digest`, and `changed_paths` is empty.
pub(crate) fn qualifies_stale_accepted_base_inventory_reconciliation(
    checkout_root: &Path,
    accepted_base_sha: &str,
    candidate_head_sha: &str,
    accepted_sources_digest: &str,
    candidate_sources_digest: &str,
) -> bool {
    if accepted_sources_digest == candidate_sources_digest
        || accepted_base_sha == candidate_head_sha
    {
        return false;
    }
    let resolve_commit = |rev: &str| -> Option<String> {
        let raw = transition_approval::git_object(
            checkout_root,
            &["rev-parse", "--verify", &format!("{rev}^{{commit}}")],
        )
        .ok()?;
        String::from_utf8(raw).ok().map(|s| s.trim().to_owned())
    };
    let (Some(base_oid), Some(head_oid)) = (
        resolve_commit(accepted_base_sha),
        resolve_commit(candidate_head_sha),
    ) else {
        return false;
    };
    if base_oid == head_oid {
        return false;
    }
    let mut blob_cache = BTreeMap::new();
    let Some(base_tree) =
        compute_git_tree_source_inventory(checkout_root, accepted_base_sha, &mut blob_cache)
    else {
        return false;
    };
    let Some(head_tree) =
        compute_git_tree_source_inventory(checkout_root, candidate_head_sha, &mut blob_cache)
    else {
        return false;
    };
    base_tree.inventory.digest().ok().as_deref() == Some(candidate_sources_digest)
        && head_tree.inventory.digest().ok().as_deref() == Some(candidate_sources_digest)
        && compute_inventory_diff_paths(&base_tree, &head_tree).is_empty()
}

fn compute_inventory_diff_paths(
    base_tree: &GitTreeInventory,
    head_tree: &GitTreeInventory,
) -> Vec<String> {
    let all_paths: BTreeSet<&str> = base_tree
        .entries
        .keys()
        .chain(head_tree.entries.keys())
        .map(String::as_str)
        .collect();
    let mut changed = Vec::new();
    for path in all_paths {
        let base_entry = base_tree.entries.get(path);
        let head_entry = head_tree.entries.get(path);
        let base_digest = base_tree.inventory.files.get(path);
        let head_digest = head_tree.inventory.files.get(path);
        let differs = match (base_entry, head_entry) {
            (Some(b), Some(h)) => b.mode != h.mode || b.oid != h.oid || base_digest != head_digest,
            _ => true,
        };
        if differs {
            changed.push(path.to_owned());
        }
    }
    changed
}

pub(crate) fn changed_inventory_paths_between(
    checkout_root: &Path,
    accepted_base_sha: &str,
    candidate_head_sha: &str,
) -> Vec<String> {
    let mut blob_cache = BTreeMap::new();
    let Some(base_tree) =
        compute_git_tree_source_inventory(checkout_root, accepted_base_sha, &mut blob_cache)
    else {
        return Vec::new();
    };
    let Some(head_tree) =
        compute_git_tree_source_inventory(checkout_root, candidate_head_sha, &mut blob_cache)
    else {
        return Vec::new();
    };
    compute_inventory_diff_paths(&base_tree, &head_tree)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestEcosystem {
    Rust,
    Node,
    Python,
    Go,
    Container,
    ReleaseMetadata,
}

fn routine_manifest_ecosystem(path: &str) -> Option<ManifestEcosystem> {
    match path {
        "Cargo.toml" | "Cargo.lock" => Some(ManifestEcosystem::Rust),
        "package.json" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" => {
            Some(ManifestEcosystem::Node)
        }
        "pyproject.toml" | "requirements.txt" | "uv.lock" | "poetry.lock" => {
            Some(ManifestEcosystem::Python)
        }
        "go.mod" | "go.sum" => Some(ManifestEcosystem::Go),
        "Dockerfile" => Some(ManifestEcosystem::Container),
        "dist-workspace.toml"
        | "dist.toml"
        | ".github/release.toml"
        | "release-plz.toml"
        | "cliff.toml"
        | ".releaserc"
        | ".releaserc.json"
        | "release-please-config.json"
        | "release-please-manifest.json" => Some(ManifestEcosystem::ReleaseMetadata),
        _ => None,
    }
}

fn ecosystem_is_coupled_to_text(ecosystem: ManifestEcosystem, text: &str) -> bool {
    let commands: &[&str] = match ecosystem {
        ManifestEcosystem::Rust => &[
            "cargo",
            "rustc",
            "rustup",
            "rustdoc",
            "clippy",
            "cargo-clippy",
            "cargo-nextest",
            "cargo-test",
            "cargo-check",
            "cargo-audit",
            "cargo-deny",
            "bazel",
            "bazelisk",
        ],
        ManifestEcosystem::Node => &[
            "npm",
            "npx",
            "node",
            "nodejs",
            "yarn",
            "pnpm",
            "bun",
            "deno",
            "tsx",
            "vitest",
            "jest",
            "mocha",
            "eslint",
            "tsc",
            "playwright",
            "turbo",
            "nx",
            "biome",
            "webpack",
            "vite",
        ],
        ManifestEcosystem::Python => &[
            "python", "python3", "pytest", "uv", "poetry", "pip", "pip3", "ruff", "mypy", "tox",
            "nox", "hatch", "pdm", "flake8", "black", "pyright", "unittest",
        ],
        ManifestEcosystem::Go => &["go", "gofmt", "golangci-lint", "staticcheck", "govulncheck"],
        ManifestEcosystem::Container => &[
            "docker",
            "podman",
            "buildah",
            "nerdctl",
            "hadolint",
            "kaniko",
            "docker-compose",
            "compose",
        ],
        ManifestEcosystem::ReleaseMetadata => &[],
    };
    if commands.is_empty() {
        return false;
    }
    text.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"'
                    | '`'
                    | ','
                    | ';'
                    | '('
                    | ')'
                    | '['
                    | ']'
                    | '{'
                    | '}'
                    | '='
                    | ':'
                    | '|'
                    | '&'
            )
    })
    .filter(|token| !token.is_empty())
    .any(|token| {
        let cmd = token
            .rsplit('/')
            .next()
            .unwrap_or(token)
            .to_ascii_lowercase();
        commands.contains(&cmd.as_str())
    })
}

fn is_non_ci_helper_path(path: &str) -> bool {
    const HELPER_PREFIXES: [&str; 4] = [
        "scripts/",
        "assets/scripts/",
        "assets/tools/",
        "assets/build/",
    ];
    HELPER_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix))
        && !super::init_reconciliation::is_workflow(path)
        && !super::init_reconciliation::SOURCE_FILES.contains(&path)
        && !super::readiness_profile::catalog_source_paths().contains(&path)
}

fn is_candidate_non_proof_path(
    path: &str,
    accepted_commands: &[ProofDef],
    accepted_contract: &RepositoryContract,
) -> bool {
    if is_non_ci_helper_path(path) {
        return true;
    }
    let Some(eco) = routine_manifest_ecosystem(path) else {
        return false;
    };
    !super::init_reconciliation::is_workflow(path)
        && !super::readiness_profile::catalog_source_paths().contains(&path)
        && !contract_referenced_strings(accepted_commands, accepted_contract)
            .into_iter()
            .any(|text| ecosystem_is_coupled_to_text(eco, text))
}

fn same_dir_helper_referenced_in_text(referrer: &str, target: &str, text: &str) -> bool {
    if !is_non_ci_helper_path(referrer) || !is_non_ci_helper_path(target) {
        return false;
    }
    let (Some((ref_dir, _)), Some((target_dir, target_base))) =
        (referrer.rsplit_once('/'), target.rsplit_once('/'))
    else {
        return false;
    };
    if ref_dir != target_dir || target_base.is_empty() {
        return false;
    }
    let slash_base = format!("/{target_base}");
    text.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '='
            )
    })
    .any(|raw_token| {
        let token = raw_token.trim_end_matches(':');
        if token.is_empty() {
            return false;
        }
        let normalized = token.strip_prefix("./").unwrap_or(token);
        normalized == target_base
            || normalized == slash_base
            || (normalized.starts_with('$') && normalized.ends_with(&slash_base))
    })
}

fn path_is_referenced_in_text(path: &str, text: &str) -> bool {
    let clean = path.strip_prefix("./").unwrap_or(path);
    if clean.is_empty() {
        return false;
    }
    if text.contains(clean) {
        return true;
    }
    let mut ancestors = Vec::new();
    let mut cursor = clean;
    while let Some((parent, _)) = cursor.rsplit_once('/') {
        ancestors.push(parent);
        cursor = parent;
    }
    if ancestors.is_empty() {
        return false;
    }
    for dir in &ancestors {
        if text.contains(&format!("{dir}/*")) || text.contains(&format!("{dir}/**")) {
            return true;
        }
    }
    for raw_token in text.split(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '\'' | '"' | '`' | ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '='
            )
    }) {
        let token = raw_token.trim_end_matches(':');
        if token.is_empty() {
            continue;
        }
        let normalized = token.strip_prefix("./").unwrap_or(token);
        let without_trailing = normalized.trim_end_matches('/');
        if ancestors.contains(&without_trailing) {
            return true;
        }
    }
    false
}

fn contract_referenced_strings<'a>(
    accepted_commands: &'a [ProofDef],
    accepted_contract: &'a RepositoryContract,
) -> Vec<&'a str> {
    let mut texts = Vec::new();
    for cmd in accepted_commands {
        texts.push(cmd.command.as_str());
        for arg in &cmd.args {
            texts.push(arg.as_str());
        }
    }
    for execution in accepted_contract.executions.values() {
        texts.push(execution.directory.as_str());
        if !execution.program.is_empty() {
            texts.push(execution.program.as_str());
        }
        for val in execution.environment.values() {
            texts.push(val.as_str());
        }
        for val in execution.toolchain_files.values() {
            texts.push(val.as_str());
        }
        for platform in execution.platforms.values() {
            texts.push(platform.program.as_str());
            for val in platform.toolchain_files.values() {
                texts.push(val.as_str());
            }
        }
    }
    for row in &accepted_contract.source_coverage {
        texts.push(row.source.as_str());
    }
    texts
}

fn path_is_directly_referenced_in_contract(
    path: &str,
    accepted_commands: &[ProofDef],
    accepted_contract: &RepositoryContract,
) -> bool {
    let clean = path.strip_prefix("./").unwrap_or(path);
    if contract_referenced_strings(accepted_commands, accepted_contract)
        .into_iter()
        .any(|text| path_is_referenced_in_text(clean, text))
    {
        return true;
    }
    for cmd in accepted_commands {
        let Some(execution) = accepted_contract.executions.get(&cmd.name) else {
            continue;
        };
        let dir = execution.directory.trim_end_matches('/');
        if dir.is_empty() || dir == "." {
            continue;
        }
        if let Some(rel) = clean.strip_prefix(&format!("{dir}/"))
            && (path_is_referenced_in_text(rel, &cmd.command)
                || cmd
                    .args
                    .iter()
                    .any(|arg| path_is_referenced_in_text(rel, arg))
                || execution
                    .environment
                    .values()
                    .any(|val| path_is_referenced_in_text(rel, val)))
        {
            return true;
        }
    }
    false
}

fn collect_proof_relevant_blobs(
    checkout_root: &Path,
    base_tree: &GitTreeInventory,
    accepted_commands: &[ProofDef],
    accepted_contract: &RepositoryContract,
) -> Option<BTreeMap<String, String>> {
    let mut proof_relevant = BTreeMap::new();
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();

    for path in base_tree.entries.keys() {
        if !is_candidate_non_proof_path(path, accepted_commands, accepted_contract)
            || path_is_directly_referenced_in_contract(path, accepted_commands, accepted_contract)
        {
            visited.insert(path.clone());
            queue.push_back(path.clone());
        }
    }

    while let Some(current) = queue.pop_front() {
        let entry = base_tree.entries.get(&current)?;
        // Fail closed if any proof-relevant file cannot be inspected as bounded UTF-8.
        let content = read_bounded_utf8_blob(checkout_root, entry)?;
        for candidate in base_tree.entries.keys() {
            if visited.contains(candidate) {
                continue;
            }
            let referenced = path_is_referenced_in_text(candidate, &content)
                || same_dir_helper_referenced_in_text(&current, candidate, &content)
                || routine_manifest_ecosystem(candidate)
                    .is_some_and(|eco| ecosystem_is_coupled_to_text(eco, &content));
            if referenced {
                visited.insert(candidate.clone());
                queue.push_back(candidate.clone());
            }
        }
        proof_relevant.insert(current, content);
    }

    Some(proof_relevant)
}

/// Qualify an automatic `repo.pr-readiness.v1` source-inventory transition when
/// `accepted_commands` and `accepted_contract` are preserved verbatim and every
/// added, modified, or removed inventoried path between `accepted_base_sha` and
/// `candidate_head_sha` is either:
/// - a non-workflow helper file under `scripts/`, `assets/scripts/`, `assets/tools/`,
///   or `assets/build/` that is not transitively referenced by `accepted_commands`,
///   `accepted_contract`, or any proof-relevant inventoried file in `accepted_base_sha`, or
/// - a routine package/container/release manifest or lockfile whose ecosystem is not
///   coupled to `accepted_commands`, `accepted_contract`, or any proof-relevant file.
pub(crate) fn qualifies_non_proof_source_inventory_transition(
    checkout_root: &Path,
    accepted_base_sha: &str,
    candidate_head_sha: &str,
    accepted_sources_digest: &str,
    candidate_sources_digest: &str,
    accepted_commands: &[ProofDef],
    accepted_contract: &RepositoryContract,
) -> bool {
    if accepted_sources_digest == candidate_sources_digest {
        return false;
    }
    let mut blob_cache = BTreeMap::new();
    let Some(base_tree) =
        compute_git_tree_source_inventory(checkout_root, accepted_base_sha, &mut blob_cache)
    else {
        return false;
    };
    let Some(head_tree) =
        compute_git_tree_source_inventory(checkout_root, candidate_head_sha, &mut blob_cache)
    else {
        return false;
    };
    if base_tree.inventory.digest().ok().as_deref() != Some(accepted_sources_digest)
        || head_tree.inventory.digest().ok().as_deref() != Some(candidate_sources_digest)
    {
        return false;
    }
    let changed_paths = compute_inventory_diff_paths(&base_tree, &head_tree);
    if changed_paths.is_empty() {
        return false;
    }
    let Some(proof_relevant) = collect_proof_relevant_blobs(
        checkout_root,
        &base_tree,
        accepted_commands,
        accepted_contract,
    ) else {
        return false;
    };
    changed_paths.iter().all(|path| {
        is_candidate_non_proof_path(path, accepted_commands, accepted_contract)
            && !proof_relevant.contains_key(path)
            && !path_is_directly_referenced_in_contract(path, accepted_commands, accepted_contract)
            && proof_relevant.iter().all(|(other, content)| {
                !path_is_referenced_in_text(path, content)
                    && !same_dir_helper_referenced_in_text(other, path, content)
                    && !routine_manifest_ecosystem(path)
                        .is_some_and(|eco| ecosystem_is_coupled_to_text(eco, content))
            })
    })
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

//! Executable, source-owned orchestration for the public readiness boundary.
//!
//! There is no receipt import, shell/YAML interpreter, push/PR path or automatic
//! authorization here. `Host` is a trusted compiled integration, not a user
//! plugin or JSON transport. It must use authenticated provider observations and
//! measured execution bytes, and preserve the existing external-action gate.
use super::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use super::derived_release_profile::{self, ReleaseProfile};
use super::native_proof::{self, IdentityInputs, Invocation, SuiteLimits};
use super::readiness_evidence::{
    self as evidence, ApplicabilityFacts, CheckKey, SuiteEvidence, Verdict,
};
use super::readiness_profile::{
    self as profile, ExecutionBinding, ExecutionPolicy, InvocationRole, ProfileDeclaration,
    ProjectPrerequisites, PublicationState, RequiredCatalog, SourceInventory, TargetObservation,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

pub const READINESS_SCOPE: &str = "source_pr_readiness";

const CONFIG: &str = ".decapod/config.toml";
const GOVERNANCE: &str = ".decapod/governance.json";
use super::readiness_profile::source;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct ProofConfiguration {
    #[serde(default)]
    pub validation_buildkite_preflights: bool,
    pub required_profile: Option<ProfileDeclaration>,
}
/// Parse only the canonical tracked project field. No environment fallback,
/// receipt-selected profile or auto-reconciliation is permitted.
pub fn proof_configuration(bytes: &[u8]) -> io::Result<ProofConfiguration> {
    let config: toml::Value = toml::from_str(std::str::from_utf8(bytes).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    match config.get("proof") {
        None => Ok(ProofConfiguration::default()),
        Some(value) => value.clone().try_into().map_err(io::Error::other),
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub root: PathBuf,
    /// Already installed, trusted, canonical Git binary. No PATH discovery.
    pub git: PathBuf,
    /// Canonical identity supplied by the existing project/remote resolver.
    pub repository: String,
    pub base_branch: String,
    pub accepted_base_ref: String,
    /// Must be outside the checkout AND its actual Git metadata directories.
    pub evidence_parent: PathBuf,
    pub timeout: Duration,
    pub max_input_bytes: u64,
    pub max_files: usize,
    pub max_output_bytes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Blocker {
    pub code: String,
    pub detail: String,
    pub check: Option<CheckKey>,
}
impl Blocker {
    pub fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            detail: detail.into(),
            check: None,
        }
    }
    pub fn missing(key: CheckKey, detail: impl Into<String>) -> Self {
        Self {
            code: "MISSING_REQUIRED_PROOF_ADAPTER".into(),
            detail: detail.into(),
            check: Some(key),
        }
    }
}
/// Agent-facing phase guidance; it never changes execution authority or proof.
/// The configured project tools stay inside the selected validation route.
#[derive(Debug, Serialize)]
pub struct ValidationGuidance {
    pub phase: &'static str,
    pub owner: &'static str,
    pub entry_command: &'static str,
    pub execution: &'static str,
    pub next_action: &'static str,
    pub instruction: &'static str,
    pub human_input_when: &'static str,
}
impl ValidationGuidance {
    pub fn for_outcome(ready: bool) -> Self {
        Self {
            phase: "validation",
            owner: "agent",
            entry_command: "decapod validate",
            execution: "Validation owns the configured build prerequisites, verification and tests through the selected provider and binds proof to the produced artifacts. Start once the source and required checkpoint are ready; no separate manual build is a prerequisite.",
            next_action: if ready {
                "review_delivery_readiness"
            } else {
                "repair_and_revalidate"
            },
            instruction: if ready {
                "Review the exact candidate evidence against human intent and delivery constraints. Success establishes only the reported readiness scope; delivery actions remain subject to existing authorization and review requirements."
            } else {
                "Agent: inspect the blockers and evidence, configure or repair the project's actual build, test and CI contract within the governed scope, checkpoint the repaired source, then rerun decapod validate. Preserve required checks and artifact custody; unsupported adapters remain blockers."
            },
            human_input_when: "Stop for unresolved intent, a decision gate, missing authorization or unavailable access; request the specific human input without delegating ordinary configuration, build orchestration or repair to the human.",
        }
    }

    fn for_controller_contract_unsupported() -> Self {
        Self {
            next_action: "obtain_supported_controller",
            instruction: "The controller does not support this reviewed source contract. Rerunning the same controller, repinning configuration or running a manual build cannot resolve this blocker. Use an independently supported controller for this exact contract, then run plain decapod validate; preserve the blocker if none is available. No automatic controller bootstrap is implemented.",
            human_input_when: "Request the specific access, authorization or controller-support decision needed if no independently supported controller is available; do not weaken the contract or substitute manual-build evidence.",
            ..Self::for_outcome(false)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub ready: bool,
    pub scope: &'static str,
    pub agent_guidance: ValidationGuidance,
    /// Governs execution intent; mocked dispositions claim no successful run.
    pub stage_dispositions: Vec<profile::StageDisposition>,
    pub blockers: Vec<Blocker>,
    pub evidence_path: Option<PathBuf>,
}
impl ValidationReport {
    pub fn exit_code(&self) -> i32 {
        if self.ready { 0 } else { 1 }
    }
}

/// The one invocation deadline is passed through every adapter. The adapter must
/// enforce it for every process, provider read, poll and cleanup operation.
pub struct RuntimeBudget<'a> {
    deadline: Instant,
    cancel: &'a Cancellation,
    max_output_bytes: usize,
    bytes_left: u64,
}
impl RuntimeBudget<'_> {
    pub fn remaining(&self) -> io::Result<Duration> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "validation cancelled",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "validation deadline exceeded"))
    }
    pub fn cancellation(&self) -> &Cancellation {
        self.cancel
    }
    pub fn output_limit(&self) -> usize {
        self.max_output_bytes
    }
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        self.remaining()?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > self.bytes_left
        {
            return Err(invalid("input is not a bounded regular file"));
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("input changed type"));
        }
        let mut bytes = Vec::new();
        file.take(self.bytes_left.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > self.bytes_left {
            return Err(invalid("input byte budget exceeded"));
        }
        self.bytes_left -= bytes.len() as u64;
        self.remaining()?;
        Ok(bytes)
    }
}

/// Independently loaded before execution. Construction remains runtime-owned.
pub struct CurrentProject {
    pub configuration: ProofConfiguration,
    pub sources: SourceInventory,
    pub facts: ApplicabilityFacts,
    /// Reserved for an explicitly scoped future predictive release run (#1399).
    /// Core PR readiness does not load or execute this optional profile.
    pub release: Option<ReleaseProfile>,
    pub catalog: RequiredCatalog,
    pub identity_inputs: IdentityInputs,
}

/// Trusted adapter bindings selected before execution, never from green rows.
/// Native closure files are rehashed by capture_current_identity at both ends.
/// Remote integrations must separately authenticate and measure their actual
/// toolchain/artifacts; copying the expected binding is never an observation.
pub struct ExecutionSelection {
    pub policy: ExecutionPolicy,
    pub bindings: BTreeMap<CheckKey, ExecutionBinding>,
    pub native_invocations: BTreeMap<CheckKey, Invocation>,
}
/// Produced in this invocation by a real runner. Deliberately not Deserialize.
pub struct ExecutedSuite {
    pub evidence: SuiteEvidence,
    pub failure: Option<String>,
}

pub trait Host {
    fn evidence_directory(&self) -> Option<&Path> {
        None
    }
    /// Native execution requires the actual partial project checks. Buildkite
    /// returns ProviderRequired and proves those checks within its source graph.
    /// A legacy receipt is never read, returned or treated as readiness here.
    fn project_check(&mut self, budget: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites>;
    /// Fresh provider-authenticated accepted branch query. Do not read a cached
    /// tracking ref/receipt; preserve existing external_action authorization.
    fn accepted_target(
        &mut self,
        repository: &str,
        branch: &str,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<String>;
    /// Discover complete actual capabilities and select finite reviewed adapters.
    /// Report ALL missing obligations; never filter the catalog to available rows.
    fn select(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>>;
    /// Execute/poll the immutable whole plan under the shared deadline. Runtime
    /// must wire only trusted compiled adapters; this is not an evidence importer.
    /// Logs/artifacts go in the fresh external evidence directory. No source writes.
    fn execute(
        &mut self,
        project: &CurrentProject,
        selection: &ExecutionSelection,
        plan: &evidence::ExpectedPlan,
        identity: &evidence::MaterialIdentity,
        evidence_directory: &Path,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite>;
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn text(bytes: Vec<u8>) -> io::Result<String> {
    Ok(String::from_utf8(bytes)
        .map_err(io::Error::other)?
        .trim_end_matches('\n')
        .into())
}
fn git(options: &Options, budget: &RuntimeBudget<'_>, args: &[&str]) -> io::Result<Vec<u8>> {
    let mut command = Command::new(&options.git);
    command
        .current_dir(&options.root)
        .env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
        ])
        .args(args);
    let capture = command.bounded_capture(
        ProcessLimits {
            timeout: budget.remaining()?.min(Duration::from_secs(30)),
            max_output_bytes: budget.max_output_bytes,
        },
        budget.cancel,
    )?;
    if capture.termination != Termination::Exited || !capture.output.status.success() {
        return Err(invalid(format!(
            "bounded Git {} failed ({:?}, {:?})",
            args.first().unwrap_or(&"read"),
            capture.termination,
            capture.output.status.code()
        )));
    }
    Ok(capture.output.stdout)
}
fn paths(bytes: &[u8]) -> io::Result<BTreeSet<String>> {
    if !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err(invalid("incomplete Git path inventory"));
    }
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| {
            let path = std::str::from_utf8(p).map_err(io::Error::other)?;
            if !safe_relative(path) {
                return Err(invalid("unsupported Git path"));
            }
            Ok(path.to_owned())
        })
        .collect()
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0', '\n', '\r'])
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && !path.split('/').any(|p| p == ".git")
}
fn file(root: &Path, relative: &str) -> io::Result<PathBuf> {
    if !safe_relative(relative) {
        return Err(invalid("unsafe project path"));
    }
    let mut cursor = root.to_path_buf();
    for part in relative.split('/') {
        cursor.push(part);
        if fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err(invalid("symlinked proof input is unsupported"));
        }
    }
    if !cursor.canonicalize()?.starts_with(root) {
        return Err(invalid("proof input escaped checkout"));
    }
    Ok(cursor)
}
fn validate_options(options: &Options, budget: &RuntimeBudget<'_>) -> io::Result<()> {
    if !options.root.is_absolute()
        || options.root.canonicalize()? != options.root
        || !options.git.is_absolute()
        || options.git.canonicalize()? != options.git
        || !options.git.is_file()
        || options.git.file_name().and_then(|n| n.to_str()) != Some("git")
        || options.repository.trim().is_empty()
        || options.repository.contains(['\n', '\r', '\0'])
        || options.base_branch.is_empty()
        || options.base_branch.starts_with('-')
        || !options.accepted_base_ref.starts_with("refs/")
        || options.accepted_base_ref.contains([' ', '\n', '\r', '\0'])
    {
        return Err(invalid("invalid canonical project/Git/base inputs"));
    }
    git(
        options,
        budget,
        &[
            "check-ref-format",
            &format!("refs/heads/{}", options.base_branch),
        ],
    )?;
    git(
        options,
        budget,
        &["check-ref-format", &options.accepted_base_ref],
    )?;
    let root = PathBuf::from(text(git(
        options,
        budget,
        &["rev-parse", "--show-toplevel"],
    )?)?);
    if root.canonicalize()? != options.root {
        return Err(invalid("root is not the actual Git checkout"));
    }
    let external = options.evidence_parent.canonicalize()?;
    for directory in [
        options.root.clone(),
        PathBuf::from(text(git(
            options,
            budget,
            &["rev-parse", "--path-format=absolute", "--git-dir"],
        )?)?),
        PathBuf::from(text(git(
            options,
            budget,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?)?),
    ] {
        if external.starts_with(directory.canonicalize()?) {
            return Err(invalid("evidence must be outside source/Git metadata"));
        }
    }
    Ok(())
}
fn clean(options: &Options, budget: &RuntimeBudget<'_>) -> io::Result<()> {
    if !git(
        options,
        budget,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?
    .is_empty()
    {
        return Err(invalid(
            "COMMIT_AND_RERUN: project prerequisites or earlier work left source/index/governance uncommitted",
        ));
    }
    Ok(())
}
fn load_project(options: &Options, budget: &mut RuntimeBudget<'_>) -> io::Result<CurrentProject> {
    clean(options, budget)?;
    let tracked = paths(&git(options, budget, &["ls-files", "-z", "--cached"])?)?;
    if tracked.len() > options.max_files
        || !tracked.contains(CONFIG)
        || !tracked.contains(GOVERNANCE)
    {
        return Err(invalid(
            "canonical config/governance must be tracked, and complete input inventory must fit limits",
        ));
    }
    let configuration = proof_configuration(&budget.read(&file(&options.root, CONFIG)?)?)?;
    match configuration.required_profile.as_ref() {
        None => {
            return Err(invalid(
                profile::PlanningFailure::MissingProfile.to_string(),
            ));
        }
        Some(profile) if profile.id != profile::REQUIRED_PROFILE => {
            return Err(invalid(
                profile::PlanningFailure::UnknownProfile(profile.id.clone()).to_string(),
            ));
        }
        _ => {}
    }
    let sources = discover_sources(options, budget)?;
    let head = text(git(
        options,
        budget,
        &["rev-parse", "--verify", "HEAD^{commit}"],
    )?)?;
    let base = text(git(
        options,
        budget,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", options.accepted_base_ref),
        ],
    )?)?;
    git(
        options,
        budget,
        &["merge-base", "--is-ancestor", &base, &head],
    )
    .map_err(|_| {
        invalid("ACCEPTED_BASE_NOT_ANCESTOR: incorporate the current accepted base before proof")
    })?;
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: options.base_branch.clone(),
        changed_paths: paths(&git(
            options,
            budget,
            &[
                "diff",
                "--name-only",
                "-z",
                "--no-renames",
                &base,
                &head,
                "--",
            ],
        )?)?,
    };
    let release = None;
    let catalog =
        profile::required_source_catalog(configuration.required_profile.as_ref(), &sources, &facts)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let configuration_paths = tracked
        .iter()
        .filter(|p| {
            *p == CONFIG
                || p.starts_with(".config/")
                || p.starts_with(".cargo/")
                || [
                    "Cargo.toml",
                    "Cargo.lock",
                    "rust-toolchain.toml",
                    "BUILD.bazel",
                    "MODULE.bazel",
                    "MODULE.bazel.lock",
                    ".bazelrc",
                    ".github/release.toml",
                    "dist-workspace.toml",
                    "dist.toml",
                ]
                .contains(&p.as_str())
        })
        .cloned()
        .collect();
    let identity_inputs = IdentityInputs {
        root: options.root.clone(),
        git_program: options.git.clone(),
        repository: options.repository.clone(),
        base_branch: options.base_branch.clone(),
        accepted_base_ref: options.accepted_base_ref.clone(),
        declared_untracked: BTreeSet::new(),
        configuration: configuration_paths,
        workflows: sources.files.keys().cloned().collect(),
        candidate_inputs: tracked
            .into_iter()
            .filter(|p| !evidence::material::is_bookkeeping(p))
            .collect(),
    };
    Ok(CurrentProject {
        configuration,
        sources,
        facts,
        release,
        catalog,
        identity_inputs,
    })
}
fn discover_sources(
    options: &Options,
    budget: &mut RuntimeBudget<'_>,
) -> io::Result<SourceInventory> {
    budget.remaining()?;
    // Initialization and readiness share exactly one bounded discovery policy.
    // Untracked/ignored build and provider definitions remain visible to both.
    let inventory = super::init_reconciliation::discover_source_inventory(&options.root)
        .map_err(|error| invalid(error.to_string()))?;
    budget.remaining()?;
    if inventory.files.len() > options.max_files {
        return Err(invalid("source definition inventory too large"));
    }
    // Charge actual bytes to this invocation and reject inputs that changed
    // during discovery. The shared scanner has its own entry/depth/byte caps.
    for (path, expected) in &inventory.files {
        let bytes = match super::init_reconciliation::reviewed_source_alias(&options.root, path)
            .map_err(|error| invalid(error.to_string()))?
        {
            Some(link) => {
                budget.remaining()?;
                budget.bytes_left = budget
                    .bytes_left
                    .checked_sub(link.len() as u64)
                    .ok_or_else(|| invalid("input byte budget exceeded"))?;
                link
            }
            None => budget.read(&file(&options.root, path)?)?,
        };
        let actual = hash(&bytes);
        if &actual != expected {
            return Err(invalid(format!(
                "source definition changed during discovery: {path}"
            )));
        }
    }
    Ok(inventory)
}
/// Read-only review aid. This discovers the same inventory used by validation;
/// returning its digest does not approve/update the project's declaration.
pub fn discover_source_inventory(
    options: &Options,
    cancel: &Cancellation,
) -> io::Result<SourceInventory> {
    let mut budget = RuntimeBudget {
        deadline: Instant::now() + options.timeout,
        cancel,
        max_output_bytes: options.max_output_bytes,
        bytes_left: options.max_input_bytes,
    };
    validate_options(options, &budget)?;
    discover_sources(options, &mut budget)
}
fn suite_limits(options: &Options, budget: &RuntimeBudget<'_>) -> io::Result<SuiteLimits> {
    Ok(SuiteLimits {
        timeout: budget.remaining()?,
        max_checks: 1024,
        max_hashed_bytes: options.max_input_bytes,
        max_output_bytes: options.max_output_bytes,
        git_output_limit: options.max_output_bytes,
    })
}
fn bind_remote_toolchains(
    identity: &mut evidence::MaterialIdentity,
    selection: &ExecutionSelection,
) -> io::Result<()> {
    if selection.policy.buildkite_preflights {
        // Remote binding digests are frozen from independently authenticated,
        // policy-checked preparation before substantive execution is accepted.
        // The provider adapter verifies the same measured closure after checks.
        let resolved: Vec<_> = selection
            .bindings
            .iter()
            .filter(|(_, binding)| binding.executor != evidence::Provider::Native)
            .map(|(key, binding)| (key, &binding.toolchain_digest))
            .collect();
        identity.toolchain_digest = hash(
            &serde_json::to_vec(&(&identity.toolchain_digest, resolved))
                .map_err(io::Error::other)?,
        );
    }
    Ok(())
}
fn publication_state(
    options: &Options,
    project: &CurrentProject,
    selection: &ExecutionSelection,
    budget: &mut RuntimeBudget<'_>,
) -> io::Result<PublicationState> {
    clean(options, budget)?;
    let mut identity = native_proof::capture_current_identity(
        &project.identity_inputs,
        &selection.native_invocations,
        suite_limits(options, budget)?,
        budget.cancel,
    )?;
    bind_remote_toolchains(&mut identity, selection)?;
    let work = budget.read(&file(&options.root, GOVERNANCE)?)?;
    let index = git(options, budget, &["show", &format!(":{GOVERNANCE}")])?;
    let commit = git(options, budget, &["show", &format!("HEAD:{GOVERNANCE}")])?;
    // Binding all committed governance is deliberately conservative. Mutable
    // legacy receipt fields do not establish execution and are never inspected.
    Ok(PublicationState {
        identity,
        governance_policy_digest: hash(&commit),
        governance_worktree_digest: hash(&work),
        governance_index_digest: hash(&index),
        governance_commit_digest: hash(&commit),
        uncommitted_paths: BTreeSet::new(),
    })
}
fn private_tempdir(parent: &Path, prefix: &str) -> io::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    builder.tempdir_in(parent)
}
fn private_directory(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut out = options.open(path)?;
    out.write_all(bytes)?;
    out.sync_all()
}

/// Public caller must use this report's exit_code, never the partial prerequisite
/// result. This function cannot open a PR, push a work branch, or mint a receipt.
pub fn validate(options: &Options, host: &mut dyn Host, cancel: &Cancellation) -> ValidationReport {
    let mut report = ValidationReport {
        ready: false,
        scope: READINESS_SCOPE,
        agent_guidance: ValidationGuidance::for_outcome(false),
        stage_dispositions: Vec::new(),
        blockers: Vec::new(),
        evidence_path: None,
    };
    let result = run(options, host, cancel, &mut report);
    if let Err(error) = result {
        let unsupported_controller = matches!(
            error
                .get_ref()
                .and_then(|cause| cause.downcast_ref::<profile::PlanningFailure>()),
            Some(profile::PlanningFailure::ControllerContractUnsupported(_))
        );
        let code = if unsupported_controller {
            report.agent_guidance = ValidationGuidance::for_controller_contract_unsupported();
            "CONTROLLER_CONTRACT_UNSUPPORTED"
        } else {
            "READINESS_BLOCKED"
        };
        report.blockers.push(Blocker::new(code, error.to_string()));
    }
    report
}
fn run(
    options: &Options,
    host: &mut dyn Host,
    cancel: &Cancellation,
    report: &mut ValidationReport,
) -> io::Result<()> {
    if options.timeout.is_zero()
        || options.timeout > Duration::from_secs(7200)
        || options.max_input_bytes == 0
        || options.max_files == 0
        || options.max_output_bytes == 0
        || options.max_output_bytes > 64 * 1024 * 1024
    {
        return Err(invalid("invalid bounded readiness limits"));
    }
    let mut budget = RuntimeBudget {
        deadline: Instant::now() + options.timeout,
        cancel,
        max_output_bytes: options.max_output_bytes,
        bytes_left: options.max_input_bytes,
    };
    validate_options(options, &budget)?;
    let prerequisites = host.project_check(&budget)?;
    if prerequisites == ProjectPrerequisites::Failed {
        return Err(invalid("PROJECT_PREREQUISITES_FAILED"));
    }
    let project = load_project(options, &mut budget)?;
    if prerequisites == ProjectPrerequisites::ProviderRequired
        && !project.configuration.validation_buildkite_preflights
    {
        return Err(invalid("PROJECT_PREREQUISITES_PROVIDER_MISMATCH"));
    }
    report.stage_dispositions = project.catalog.stage_dispositions().to_vec();
    // Freeze source/governance/accepted target before any remote preparation or
    // useful partial native execution performed by adapter selection. Execution
    // tooling is frozen separately after its independently measured preparation.
    let initial_identity = native_proof::capture_current_identity(
        &project.identity_inputs,
        &BTreeMap::new(),
        suite_limits(options, &budget)?,
        cancel,
    )?;
    let initial_governance = hash(&budget.read(&file(&options.root, GOVERNANCE)?)?);
    let initial_target =
        host.accepted_target(&options.repository, &options.base_branch, &budget)?;
    if initial_target != initial_identity.accepted_base_revision {
        return Err(invalid(
            "STALE_ACCEPTED_BASE: current remote target differs before adapter preparation",
        ));
    }
    let selection = match host.select(&project, &budget) {
        Ok(selection) => selection,
        Err(blockers) if !blockers.is_empty() => {
            report.blockers = blockers;
            report.evidence_path = host.evidence_directory().map(Path::to_path_buf);
            return Ok(());
        }
        Err(_) => return Err(invalid("empty adapter selection failure")),
    };
    if selection.policy.buildkite_preflights
        != project.configuration.validation_buildkite_preflights
    {
        return Err(invalid(
            "execution route differs from canonical proof.validation_buildkite_preflights",
        ));
    }
    let state = publication_state(options, &project, &selection, &mut budget)?;
    let mut source_before = initial_identity;
    source_before.toolchain_digest = state.identity.toolchain_digest.clone();
    if source_before != state.identity || initial_governance != state.governance_commit_digest {
        return Err(invalid(
            "SOURCE_CHANGED_DURING_ADAPTER_PREPARATION: commit and rerun the complete proof",
        ));
    }
    let compiled = profile::compile_plan(
        project.catalog.clone(),
        &selection.policy,
        &selection.bindings,
        &state.identity,
        &state.governance_policy_digest,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let directory = private_tempdir(&options.evidence_parent, "decapod-readiness-")?;
    // Keep incomplete evidence on errors as well. Nothing is written to Git.
    let directory = directory.keep();
    if let Some(previous) = host.evidence_directory() {
        let handoff = previous.join("snapshot-handoff.json");
        if handoff.try_exists()? {
            write_new(
                &directory.join("snapshot-handoff.json"),
                &budget.read(&handoff)?,
            )?;
        }
    }
    report.evidence_path = Some(directory.clone());
    let invocation = directory
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let target = TargetObservation {
        invocation_id: invocation.clone(),
        repository: options.repository.clone(),
        branch: options.base_branch.clone(),
        revision: host.accepted_target(&options.repository, &options.base_branch, &budget)?,
    };
    let frozen = profile::freeze(&invocation, prerequisites, &compiled, &state, &target)
        .map_err(|e| invalid(format!("FREEZE_FAILED: {e:?}")))?;
    write_new(
        &directory.join("required-catalog.json"),
        &serde_json::to_vec_pretty(&project.catalog).map_err(io::Error::other)?,
    )?;
    write_new(
        &directory.join("expected-plan.json"),
        &serde_json::to_vec_pretty(compiled.plan()).map_err(io::Error::other)?,
    )?;
    let result = host.execute(
        &project,
        &selection,
        compiled.plan(),
        &state.identity,
        &directory,
        &budget,
    )?;
    write_new(
        &directory.join("observations.json"),
        &serde_json::to_vec_pretty(&result.evidence).map_err(io::Error::other)?,
    )?;
    // Independently discover/recompile again. Do not trust the executed envelope
    // for source coverage, target identity, final material or applicability.
    let current_project = load_project(options, &mut budget)?;
    if current_project.configuration != project.configuration {
        return Err(invalid(
            "STALE_POLICY: canonical proof configuration changed",
        ));
    }
    let current = publication_state(options, &current_project, &selection, &mut budget)?;
    let current_profile = profile::compile_plan(
        current_project.catalog,
        &selection.policy,
        &selection.bindings,
        &current.identity,
        &current.governance_policy_digest,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let current_target = TargetObservation {
        invocation_id: invocation,
        repository: options.repository.clone(),
        branch: options.base_branch.clone(),
        revision: host.accepted_target(&options.repository, &options.base_branch, &budget)?,
    };
    budget.remaining()?;
    let outcome = match result.failure.as_deref() {
        Some(failure) => profile::ProviderOutcome::Failed(failure),
        None => profile::ProviderOutcome::Complete(&result.evidence),
    };
    let verdict = profile::finish(
        &frozen,
        &current_profile,
        &current,
        &current_target,
        outcome,
    )
    .map_err(|e| invalid(format!("NOT_READY: {e:?}")))?;
    report.ready = verdict == Verdict::Ready;
    report.agent_guidance = ValidationGuidance::for_outcome(report.ready);
    write_new(
        &directory.join("verdict.json"),
        &serde_json::to_vec_pretty(report).map_err(io::Error::other)?,
    )?;
    Ok(())
}

/// Concrete execution for a complete reviewed native selection. Missing adapters
/// fail during selection before this runs; one Linux process cannot supply Mac
/// evidence. The snapshots and scratch are owned, source refs remain untouched.
pub fn execute_native(
    project: &CurrentProject,
    selection: &ExecutionSelection,
    plan: &evidence::ExpectedPlan,
    identity: &evidence::MaterialIdentity,
    evidence_directory: &Path,
    budget: &RuntimeBudget<'_>,
) -> io::Result<ExecutedSuite> {
    if selection.policy.buildkite_preflights {
        return Err(invalid("remote policy cannot use native-only execution"));
    }
    let limits = native_proof::snapshot::SnapshotLimits {
        timeout: budget.remaining()?,
        max_snapshot_bytes: 4 * 1024 * 1024 * 1024,
        max_files: 200_000,
        max_io_bytes: 32 * 1024 * 1024 * 1024,
        git_output_limit: budget.max_output_bytes,
    };
    let snapshot = native_proof::snapshot::prepare_snapshot(
        &project.identity_inputs,
        &native_proof::snapshot::SnapshotReview::frozen(identity, BTreeSet::new()),
        limits,
        evidence_directory,
        budget.cancel,
    )?;
    let run = native_proof::run_native(
        plan,
        &selection.native_invocations,
        &project.identity_inputs,
        &project.facts,
        SuiteLimits {
            timeout: budget.remaining()?,
            max_checks: 1024,
            max_hashed_bytes: 32 * 1024 * 1024 * 1024,
            max_output_bytes: budget.max_output_bytes,
            git_output_limit: budget.max_output_bytes,
        },
        &snapshot,
        budget.cancel,
    )?;
    for (number, diagnostic) in run.diagnostics.iter().enumerate() {
        write_new(
            &evidence_directory.join(format!("native-{number}.stdout")),
            &diagnostic.stdout,
        )?;
        write_new(
            &evidence_directory.join(format!("native-{number}.stderr")),
            &diagnostic.stderr,
        )?;
    }
    Ok(ExecutedSuite {
        evidence: run.evidence,
        failure: run.failure,
    })
}

/// A finite inventory report for hosts without the complete execution adapters.
/// This is an actionable fail-closed default, never a partial readiness route.
pub fn missing_adapters(project: &CurrentProject) -> Vec<Blocker> {
    project.catalog.obligations().iter().map(|obligation| {
        let detail = match obligation.role {
            InvocationRole::DerivedRelease => "wire real derivative generation, every configured target, package/archive/artifact verification and measured toolchains",
            InvocationRole::CandidateBuild => "build the current candidate and measure the produced executable/package bytes",
            InvocationRole::ProjectPrerequisites => "execute the reviewed source project-check instance; an outer/legacy receipt is not execution evidence",
            _ if project.configuration.validation_buildkite_preflights => "authenticate this exact source-provider job/matrix and measure actual execution toolchain/log/artifact inputs",
            _ => "supply the exact reviewed native equivalent, including required platform and complete measured toolchain closure",
        };
        Blocker::missing(obligation.key.clone(), detail)
    }).collect()
}

/// A reviewed local tool installation, not user JSON. The host must inventory
/// the full compiler/linker/runtime closure and isolate Cargo home/cache from
/// credentials. Files are measured again around execution. No installation or
/// ambient tool lookup is performed by this adapter.
pub struct ReviewedNativeTools {
    pub programs: BTreeMap<String, PathBuf>,
    pub closure: BTreeMap<String, PathBuf>,
    pub environment: BTreeMap<String, native_proof::Value>,
}

/// Select a real finite single-command source adapter. Multi-command, Docker,
/// foreign-platform and workflow-owned obligations remain explicit errors.
pub fn source_invocation(
    check_id: &str,
    facts: &ApplicabilityFacts,
    tools: &ReviewedNativeTools,
    timeout: Duration,
    output_limit: usize,
) -> io::Result<Invocation> {
    let paths: Vec<_> = facts.changed_paths.iter().cloned().collect();
    let (program, arguments, directory, command_env, platform, purpose, artifacts) = if check_id
        == "candidate-build"
    {
        (
            "cargo",
            vec![
                native_proof::Value::Literal("build".into()),
                native_proof::Value::Literal("--locked".into()),
                native_proof::Value::Literal("--release".into()),
                native_proof::Value::Literal("--bin".into()),
                native_proof::Value::Literal("decapod".into()),
                native_proof::Value::Literal("--target-dir".into()),
                native_proof::Value::ScratchPath("target".into()),
            ],
            PathBuf::from("."),
            Vec::new(),
            native_proof::NativePlatform::current()?,
            native_proof::ExecutionPurpose::CandidateBuild,
            BTreeSet::from(["target/release/decapod".into()]),
        )
    } else {
        let check = source::find_check(check_id)?;
        if !check
            .platform
            .matches(std::env::consts::OS, std::env::consts::ARCH)
        {
            return Err(invalid(format!(
                "MISSING_NATIVE_PLATFORM: {} requires {}",
                check_id,
                check.platform.name()
            )));
        }
        let commands = source::commands(check, &paths)?;
        if commands.len() != 1 || !matches!(commands[0].program, "cargo" | "mdbook") {
            return Err(invalid(format!(
                "MISSING_NATIVE_SEQUENCE_ADAPTER: {check_id}"
            )));
        }
        for capability in check.capabilities {
            if let Some(version) = capability.strip_prefix("rustc:") {
                let resolution = tools.closure.get("resolution:rustc-version")
                        .ok_or_else(|| invalid("MISSING_NATIVE_RUST_RESOLUTION: perform authenticated local tool preparation first"))?;
                let bytes = fs::read(resolution)?;
                if bytes.len() > 64 * 1024
                    || !String::from_utf8_lossy(&bytes).starts_with(&format!("rustc {version} "))
                {
                    return Err(invalid(format!(
                        "NATIVE_RUST_VERSION_MISMATCH: source contract requires {version}"
                    )));
                }
            }
            if capability.starts_with("mdbook:") {
                return Err(invalid(
                    "MISSING_NATIVE_MDBOOK_RESOLUTION: independently measure the declared mdbook version before execution",
                ));
            }
            if !capability.contains(':') && !tools.programs.contains_key(*capability) {
                return Err(invalid(format!("MISSING_NATIVE_TOOL: {capability}")));
            }
            if let Some(target) = capability.strip_prefix("rust-target:")
                && !tools
                    .closure
                    .keys()
                    .any(|key| key.starts_with(&format!("rust-target:{target}:")))
            {
                return Err(invalid(format!("MISSING_NATIVE_TARGET: {target}")));
            }
        }
        let command = &commands[0];
        (
            command.program,
            command
                .args
                .iter()
                .map(|s| native_proof::Value::Literal((*s).into()))
                .collect(),
            PathBuf::from(command.cwd),
            command.env.clone(),
            native_proof::NativePlatform::current()?,
            native_proof::ExecutionPurpose::SourceCheck,
            BTreeSet::new(),
        )
    };
    let executable = tools
        .programs
        .get(program)
        .ok_or_else(|| invalid(format!("MISSING_NATIVE_TOOL: {program}")))?
        .clone();
    let mut toolchain_files = tools.closure.clone();
    for (name, executable) in &tools.programs {
        toolchain_files.insert(format!("tool:{name}"), executable.clone());
    }
    if !tools.programs.contains_key("rustc") || tools.closure.is_empty() {
        return Err(invalid(
            "MISSING_NATIVE_TOOLCHAIN_CLOSURE: compiler/linker/runtime inputs must be reviewed and measured",
        ));
    }
    let mut environment = tools.environment.clone();
    for (key, value) in command_env {
        if let Some(value) = value {
            environment.insert(key.into(), native_proof::Value::Literal(value.into()));
        } else {
            environment.remove(key);
        }
    }
    // Every Cargo check writes output only to owned scratch, including native
    // candidate builds. The reviewed tool environment supplies isolated caches.
    environment.insert(
        "CARGO_TARGET_DIR".into(),
        native_proof::Value::ScratchPath("target".into()),
    );
    environment.insert(
        "RUSTC".into(),
        native_proof::Value::Literal(tools.programs["rustc"].to_string_lossy().into_owned()),
    );
    Ok(Invocation {
        platform,
        program: executable,
        toolchain_files,
        arguments,
        directory,
        environment,
        purpose,
        capabilities: native_proof::ReviewedCapabilities::LocalCheckWithOwnedScratch,
        timeout_ms: timeout
            .as_millis()
            .try_into()
            .map_err(|_| invalid("native timeout overflow"))?,
        output_limit,
        artifacts,
    })
}

#[derive(Debug, Serialize)]
pub struct NativeSliceCheck {
    pub check: String,
    pub exit_code: Option<i32>,
    pub termination: String,
    pub invocation_digest: String,
    pub measured_toolchain_digest: String,
    pub output_digest: String,
    pub artifact_digest: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct NativeSliceReport {
    /// Always false. A useful partial execution is not whole PR readiness.
    pub ready: bool,
    pub checks: Vec<NativeSliceCheck>,
    pub blockers: Vec<Blocker>,
}
/// Useful partial execution when the full matrix is unavailable. This is never
/// returned as `ExecutedSuite`; it cannot satisfy the public readiness gate.
/// The owned snapshot executes actual current source. The caller must authorize
/// every invocation using the existing external_action gate in `authorize`.
#[allow(clippy::too_many_arguments)]
pub fn run_native_source_slice(
    inputs: &IdentityInputs,
    facts: &ApplicabilityFacts,
    check_ids: &[String],
    tools: &ReviewedNativeTools,
    external_directory: &Path,
    limits: SuiteLimits,
    cancel: &Cancellation,
    authorize: &mut dyn FnMut(&Invocation) -> io::Result<()>,
) -> io::Result<NativeSliceReport> {
    if check_ids.is_empty() || check_ids.len() > limits.max_checks {
        return Err(invalid("empty or excessive native slice"));
    }
    let deadline = Instant::now() + limits.timeout;
    let remaining = || {
        deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "native slice deadline exceeded")
            })
    };
    let mut report = NativeSliceReport {
        ready: false,
        checks: Vec::new(),
        blockers: Vec::new(),
    };
    let empty = BTreeMap::new();
    let before = native_proof::capture_current_identity(inputs, &empty, limits, cancel)?;
    let snapshot = native_proof::snapshot::prepare_snapshot(
        inputs,
        &native_proof::snapshot::SnapshotReview::frozen(&before, BTreeSet::new()),
        native_proof::snapshot::SnapshotLimits {
            timeout: remaining()?,
            max_snapshot_bytes: limits.max_hashed_bytes,
            max_files: 200_000,
            max_io_bytes: limits.max_hashed_bytes,
            git_output_limit: limits.git_output_limit,
        },
        external_directory,
        cancel,
    )?;
    for (index, id) in check_ids.iter().enumerate() {
        let invocation =
            match source_invocation(id, facts, tools, remaining()?, limits.max_output_bytes) {
                Ok(invocation) => invocation,
                Err(error) => {
                    report.blockers.push(Blocker::new(
                        "UNSUPPORTED_NATIVE_SLICE",
                        format!("{id}: {error}"),
                    ));
                    continue;
                }
            };
        authorize(&invocation)?;
        // Resolve only absolute reviewed native executables. Shells, runtime
        // interpreters and user-written scripts never enter this finite route.
        if !invocation.program.is_absolute()
            || invocation.program.canonicalize()? != invocation.program
            || invocation.program.starts_with(&inputs.root)
        {
            return Err(invalid("invalid native executable"));
        }
        let mut magic = [0u8; 4];
        fs::File::open(&invocation.program)?.read_exact(&mut magic)?;
        #[cfg(target_os = "linux")]
        if !magic.starts_with(b"\x7fELF") {
            return Err(invalid("native executable must be ELF"));
        }
        let measured = native_proof::toolchain_digest(&invocation)?;
        let scratch = private_tempdir(external_directory, "native-slice-")?;
        let expand = |value: &native_proof::Value| match value {
            native_proof::Value::Literal(value) => value.clone(),
            native_proof::Value::ScratchPath(relative) => {
                scratch.path().join(relative).to_string_lossy().into_owned()
            }
        };
        let mut command = Command::new(&invocation.program);
        command
            .current_dir(snapshot.root().join(&invocation.directory))
            .env_clear();
        for arg in &invocation.arguments {
            command.arg(expand(arg));
        }
        for (key, value) in &invocation.environment {
            if key.is_empty()
                || key.contains(['=', '\0'])
                || ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL"]
                    .iter()
                    .any(|s| key.to_ascii_uppercase().contains(s))
            {
                return Err(invalid("unsafe native environment"));
            }
            command.env(key, expand(value));
        }
        command
            .env("TMPDIR", scratch.path())
            .env("DECAPOD_NATIVE_PROOF_ACTIVE", "1");
        let capture = command.bounded_capture(
            ProcessLimits {
                timeout: remaining()?,
                max_output_bytes: limits.max_output_bytes,
            },
            cancel,
        )?;
        write_new(
            &external_directory.join(format!("slice-{index}.stdout")),
            &capture.output.stdout,
        )?;
        write_new(
            &external_directory.join(format!("slice-{index}.stderr")),
            &capture.output.stderr,
        )?;
        let mut artifact_hashes = BTreeMap::new();
        if capture.output.status.success() && capture.termination == Termination::Exited {
            for artifact in &invocation.artifacts {
                let path = scratch.path().join(artifact);
                if !path.is_file()
                    || fs::symlink_metadata(&path)?.file_type().is_symlink()
                    || path.metadata()?.len() == 0
                {
                    return Err(invalid("candidate artifact missing/empty/unsafe"));
                }
                artifact_hashes.insert(artifact, native_proof::executable_digest(&path)?);
            }
        }
        if measured != native_proof::toolchain_digest(&invocation)? {
            return Err(invalid("native toolchain changed during execution"));
        }
        if !capture.output.status.success() || capture.termination != Termination::Exited {
            report
                .blockers
                .push(Blocker::new("NATIVE_SLICE_FAILED", id.clone()));
        }
        report.checks.push(NativeSliceCheck {
            check: id.clone(),
            exit_code: capture.output.status.code(),
            termination: format!("{:?}", capture.termination),
            invocation_digest: invocation.digest()?,
            measured_toolchain_digest: measured,
            output_digest: evidence::digest(&(&capture.output.stdout, &capture.output.stderr))
                .map_err(io::Error::other)?,
            artifact_digest: if artifact_hashes.is_empty() {
                None
            } else {
                Some(evidence::digest(&artifact_hashes).map_err(io::Error::other)?)
            },
        });
    }
    let after = native_proof::capture_current_identity(
        inputs,
        &empty,
        SuiteLimits {
            timeout: remaining()?,
            ..limits
        },
        cancel,
    )?;
    if before != after {
        return Err(invalid("SOURCE_CHANGED_DURING_NATIVE_SLICE"));
    }
    report.blockers.push(Blocker::new(
        "PARTIAL_NATIVE_PROOF",
        "whole source-provider matrix, derived release and final readiness remain required",
    ));
    Ok(report)
}

/// Requests presented to the existing external-action authorization boundary.
/// This enum does not grant authority or persist an authorization decision.
pub enum NativeAction<'a> {
    ToolProbe {
        program: &'a Path,
        arguments: &'a [String],
    },
    TargetRead {
        program: &'a Path,
        remote_url: &'a str,
        full_ref: &'a str,
    },
    SourceCheck {
        invocation: &'a Invocation,
    },
}
type PrerequisiteCallback<'a> =
    Box<dyn FnMut(&RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> + 'a>;
type NativeAuthorizer<'a> = Box<dyn FnMut(NativeAction<'_>) -> io::Result<()> + 'a>;

/// Concrete safe default for the installed local route. It always retains all
/// unsupported obligations, but runs actual supported source/candidate checks
/// as useful partial evidence. Remote mode is an explicit missing provider host,
/// never silently replaced with a partial native result.
pub struct NativeHost<'a> {
    options: Options,
    tools: ReviewedNativeTools,
    remote_url: String,
    project_check: PrerequisiteCallback<'a>,
    authorize: NativeAuthorizer<'a>,
    partial_evidence: Option<PathBuf>,
    tool_storage: Option<tempfile::TempDir>,
}
impl<'a> NativeHost<'a> {
    pub fn new<P, A>(
        options: Options,
        tools: ReviewedNativeTools,
        remote_url: String,
        project_check: P,
        authorize: A,
    ) -> io::Result<Self>
    where
        P: FnMut(&RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> + 'a,
        A: FnMut(NativeAction<'_>) -> io::Result<()> + 'a,
    {
        let repository = remote_url
            .strip_prefix("https://github.com/")
            .and_then(|s| s.strip_suffix(".git"))
            .ok_or_else(|| invalid("expected canonical HTTPS GitHub remote"))?;
        let parts: Vec<_> = repository.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || part.starts_with('-')
                    || *part == "."
                    || *part == ".."
                    || !part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            })
            || ![repository, remote_url.as_str()].contains(&options.repository.as_str())
        {
            return Err(invalid(
                "canonical remote differs from independently resolved repository",
            ));
        }
        Ok(Self {
            options,
            tools,
            remote_url,
            project_check: Box::new(project_check),
            authorize: Box::new(authorize),
            partial_evidence: None,
            tool_storage: None,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn discover<P, A>(
        options: Options,
        required_rust_version: &str,
        remote_url: String,
        project_check: P,
        mut authorize: A,
        cancel: &Cancellation,
    ) -> io::Result<Self>
    where
        P: FnMut(&RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> + 'a,
        A: FnMut(NativeAction<'_>) -> io::Result<()> + 'a,
    {
        let discovered =
            discover_rust_tools(&options, required_rust_version, cancel, &mut authorize)?;
        let mut host = Self::new(
            options,
            discovered.tools,
            remote_url,
            project_check,
            authorize,
        )?;
        host.tool_storage = Some(discovered.storage);
        Ok(host)
    }
    pub fn partial_evidence(&self) -> Option<&Path> {
        self.partial_evidence.as_deref()
    }
}
impl Host for NativeHost<'_> {
    fn evidence_directory(&self) -> Option<&Path> {
        self.partial_evidence.as_deref()
    }
    fn project_check(&mut self, budget: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> {
        (self.project_check)(budget)
    }
    fn accepted_target(
        &mut self,
        repository: &str,
        branch: &str,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<String> {
        if repository != self.options.repository || branch != self.options.base_branch {
            return Err(invalid("target query identity changed"));
        }
        let full_ref = format!("refs/heads/{branch}");
        (self.authorize)(NativeAction::TargetRead {
            program: &self.options.git,
            remote_url: &self.remote_url,
            full_ref: &full_ref,
        })?;
        let scratch = private_tempdir(&self.options.evidence_parent, "accepted-target-")?;
        let mut command = Command::new(&self.options.git);
        command
            .current_dir(scratch.path())
            .env_clear()
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CEILING_DIRECTORIES", &self.options.evidence_parent)
            .env("GIT_CONFIG_COUNT", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .args([
                "-c",
                "credential.helper=",
                "-c",
                "protocol.allow=never",
                "-c",
                "protocol.https.allow=always",
                "-c",
                "http.followRedirects=false",
                "ls-remote",
                "--exit-code",
                "--refs",
                &self.remote_url,
                &full_ref,
            ]);
        // Preserve routing opaquely; values are neither inspected nor logged.
        for name in [
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "https_proxy",
            "http_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let capture = command.bounded_capture(
            ProcessLimits {
                timeout: budget.remaining()?.min(Duration::from_secs(30)),
                max_output_bytes: budget.max_output_bytes,
            },
            budget.cancel,
        )?;
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(invalid(
                "ACCEPTED_TARGET_UNAVAILABLE: explicit public HTTPS target read failed; no authentication fallback was attempted",
            ));
        }
        let value = text(capture.output.stdout)?;
        let rows: Vec<_> = value.lines().collect();
        let (oid, observed_ref) = rows
            .first()
            .and_then(|line| line.split_once('\t'))
            .ok_or_else(|| invalid("malformed accepted-target response"))?;
        if rows.len() != 1
            || observed_ref != full_ref
            || !matches!(oid.len(), 40 | 64)
            || !oid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || oid.bytes().all(|b| b == b'0')
        {
            return Err(invalid("accepted-target query was ambiguous or mismatched"));
        }
        Ok(oid.into())
    }
    fn select(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>> {
        let mut blockers = Vec::new();
        let mut invocations = BTreeMap::new();
        let mut bindings = BTreeMap::new();
        let mut executors = BTreeMap::new();
        let mut slice_ids = BTreeSet::new();
        let mut measured_closures = BTreeMap::<String, String>::new();
        for obligation in project.catalog.obligations() {
            if project.configuration.validation_buildkite_preflights {
                blockers.push(Blocker::missing(obligation.key.clone(), "SNAPSHOT_HOST_REQUIRED: use the authenticated Buildkite current-workbranch host; CLI credentials are not account-rights discovery"));
                continue;
            }
            let id = if obligation.role == InvocationRole::CandidateBuild {
                "candidate-build"
            } else {
                obligation.key.profile.rsplit('/').next().unwrap_or("")
            };
            let selected = if matches!(
                obligation.role,
                InvocationRole::DerivedRelease
                    | InvocationRole::ProjectPrerequisites
                    | InvocationRole::FullValidator
            ) {
                Err(invalid(
                    "reviewed typed derivative/project-check adapter is required",
                ))
            } else {
                source_invocation(
                    id,
                    &project.facts,
                    &self.tools,
                    budget.remaining().unwrap_or_default(),
                    budget.max_output_bytes,
                )
            };
            let invocation = match selected {
                Ok(invocation) => invocation,
                Err(error) => {
                    blockers.push(Blocker::missing(obligation.key.clone(), error.to_string()));
                    continue;
                }
            };
            if let Err(error) = budget.remaining() {
                blockers.push(Blocker::new("NATIVE_SELECTION_DEADLINE", error.to_string()));
                break;
            }
            let closure_key = evidence::digest(&(&invocation.program, &invocation.toolchain_files))
                .map_err(io::Error::other);
            let toolchain = closure_key.and_then(|key| {
                if let Some(measured) = measured_closures.get(&key) {
                    return Ok(measured.clone());
                }
                let measured = native_proof::toolchain_digest(&invocation)?;
                measured_closures.insert(key, measured.clone());
                Ok(measured)
            });
            let measured = match (invocation.digest(), toolchain) {
                (Ok(invocation_digest), Ok(toolchain_digest)) => {
                    (invocation_digest, toolchain_digest)
                }
                _ => {
                    blockers.push(Blocker::missing(
                        obligation.key.clone(),
                        "actual installed toolchain cannot be measured",
                    ));
                    continue;
                }
            };
            bindings.insert(
                obligation.key.clone(),
                ExecutionBinding {
                    role: obligation.role,
                    definition_digest: obligation.definition_digest.clone(),
                    invocation_digest: measured.0,
                    toolchain_digest: measured.1,
                    executor: evidence::Provider::Native,
                    dependencies: Vec::new(),
                },
            );
            executors.insert(obligation.key.clone(), evidence::Provider::Native);
            invocations.insert(obligation.key.clone(), invocation);
            slice_ids.insert(id.to_owned());
        }
        if blockers.is_empty() {
            return Ok(ExecutionSelection {
                policy: ExecutionPolicy {
                    buildkite_preflights: false,
                    executors,
                },
                bindings,
                native_invocations: invocations,
            });
        }
        if !slice_ids.is_empty() {
            let partial = (|| -> io::Result<()> {
                let directory =
                    private_tempdir(&self.options.evidence_parent, "decapod-native-partial-")?
                        .keep();
                self.partial_evidence = Some(directory.clone());
                let limits = suite_limits(&self.options, budget)?;
                let ids: Vec<_> = slice_ids.into_iter().collect();
                let authorize = &mut self.authorize;
                let report = run_native_source_slice(
                    &project.identity_inputs,
                    &project.facts,
                    &ids,
                    &self.tools,
                    &directory,
                    limits,
                    budget.cancel,
                    &mut |invocation| authorize(NativeAction::SourceCheck { invocation }),
                )?;
                write_new(
                    &directory.join("partial-native.json"),
                    &serde_json::to_vec_pretty(&report).map_err(io::Error::other)?,
                )?;
                for blocker in report.blockers {
                    blockers.push(blocker);
                }
                blockers.push(Blocker::new(
                    "PARTIAL_NATIVE_EVIDENCE",
                    directory.to_string_lossy(),
                ));
                Ok(())
            })();
            if let Err(error) = partial {
                blockers.push(Blocker::new("NATIVE_EXECUTION_BLOCKED", error.to_string()));
            }
        }
        Err(blockers)
    }
    fn execute(
        &mut self,
        project: &CurrentProject,
        selection: &ExecutionSelection,
        plan: &evidence::ExpectedPlan,
        identity: &evidence::MaterialIdentity,
        directory: &Path,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite> {
        for invocation in selection.native_invocations.values() {
            (self.authorize)(NativeAction::SourceCheck { invocation })?;
        }
        execute_native(project, selection, plan, identity, directory, budget)
    }
}

/// Owns fresh tool state, never copies an authenticated Cargo home. Use
/// NativeHost::discover to keep this storage alive through execution.
pub struct PreparedNativeTools {
    pub tools: ReviewedNativeTools,
    storage: tempfile::TempDir,
}

/// Resolve installed tools through explicit absolute paths in PATH and the
/// standard installed rustup directory. No rustup/install manager is executed.
/// Rust version comes from independently parsed project Cargo metadata.
pub fn discover_rust_tools(
    options: &Options,
    required_version: &str,
    cancel: &Cancellation,
    authorize: &mut dyn FnMut(NativeAction<'_>) -> io::Result<()>,
) -> io::Result<PreparedNativeTools> {
    if required_version.is_empty()
        || !required_version
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.')
    {
        return Err(invalid("invalid required Rust version"));
    }
    let mut budget = RuntimeBudget {
        deadline: Instant::now() + options.timeout.min(Duration::from_secs(120)),
        cancel,
        max_output_bytes: options.max_output_bytes.min(4 * 1024 * 1024),
        bytes_left: options.max_input_bytes,
    };
    validate_options(options, &budget)?;
    let storage = private_tempdir(&options.evidence_parent, "decapod-native-tools-")?;
    for directory in ["home", "cargo"] {
        private_directory(&storage.path().join(directory))?;
    }
    let search: Vec<_> = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|path| path.is_absolute() && !path.starts_with(&options.root))
        .collect();
    let locate = |name: &str| -> Option<PathBuf> {
        search
            .iter()
            .filter_map(|directory| directory.join(name).canonicalize().ok())
            .find(|path| {
                path.is_file()
                    && !path.starts_with(&options.root)
                    && path.file_name().and_then(|n| n.to_str()) != Some("rustup")
            })
    };
    let mut compiler_candidates = locate("rustc").into_iter().collect::<Vec<_>>();
    let rustup_home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".rustup")));
    if let Some(home) = rustup_home.filter(|p| p.is_absolute() && !p.starts_with(&options.root))
        && let Ok(entries) = fs::read_dir(home.join("toolchains"))
    {
        for entry in entries {
            let entry = entry?;
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{required_version}-"))
            {
                let path = entry.path().join("bin/rustc");
                if path.is_file() {
                    compiler_candidates.push(path.canonicalize()?);
                }
            }
        }
    }
    compiler_candidates.sort();
    compiler_candidates.dedup();
    let probe = |program: &Path,
                 args: &[&str],
                 budget: &RuntimeBudget<'_>,
                 authorize: &mut dyn FnMut(NativeAction<'_>) -> io::Result<()>|
     -> io::Result<Vec<u8>> {
        if !program.is_absolute()
            || !program.is_file()
            || program.canonicalize()? != program
            || program.starts_with(&options.root)
        {
            return Err(invalid("untrusted or missing installed native tool"));
        }
        let arguments: Vec<_> = args.iter().map(|s| (*s).to_owned()).collect();
        authorize(NativeAction::ToolProbe {
            program,
            arguments: &arguments,
        })?;
        let mut command = Command::new(program);
        command
            .current_dir(storage.path())
            .env_clear()
            .env("HOME", storage.path().join("home"))
            .env("CARGO_HOME", storage.path().join("cargo"))
            .args(args);
        let capture = command.bounded_capture(
            ProcessLimits {
                timeout: budget.remaining()?.min(Duration::from_secs(15)),
                max_output_bytes: budget.max_output_bytes,
            },
            cancel,
        )?;
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(invalid("installed tool probe failed"));
        }
        Ok(capture.output.stdout)
    };
    let mut compiler = None;
    for candidate in compiler_candidates {
        let version = probe(&candidate, &["--version", "--verbose"], &budget, authorize)?;
        if String::from_utf8_lossy(&version).starts_with(&format!("rustc {required_version} ")) {
            if compiler.is_some() {
                return Err(invalid(
                    "ambiguous installed Rust toolchains; use explicit PATH/RUSTUP_HOME",
                ));
            }
            compiler = Some((candidate, version));
        }
    }
    let (rustc, version) = compiler.ok_or_else(|| invalid(format!("MISSING_NATIVE_RUST: install/select the existing project toolchain {required_version}; validation does not install it")))?;
    let sysroot = PathBuf::from(text(probe(
        &rustc,
        &["--print", "sysroot"],
        &budget,
        authorize,
    )?)?)
    .canonicalize()?;
    if sysroot.starts_with(&options.root) || sysroot.join("bin/rustc").canonicalize()? != rustc {
        return Err(invalid("Rust compiler/sysroot identity mismatch"));
    }
    let host = String::from_utf8_lossy(&version)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| invalid("Rust compiler did not report host"))?;
    let mut programs = BTreeMap::from([("rustc".into(), rustc)]);
    for name in [
        "cargo",
        "rustfmt",
        "cargo-fmt",
        "cargo-clippy",
        "clippy-driver",
        "rustdoc",
    ] {
        let path = sysroot.join("bin").join(name);
        if path.is_file() {
            programs.insert(name.into(), path.canonicalize()?);
        }
    }
    if !programs.contains_key("cargo") {
        return Err(invalid(
            "MISSING_NATIVE_CARGO: pinned installation has no Cargo",
        ));
    }
    let cargo_version = probe(
        &programs["cargo"],
        &["--version", "--verbose"],
        &budget,
        authorize,
    )?;
    if !String::from_utf8_lossy(&cargo_version).starts_with(&format!("cargo {required_version} ")) {
        return Err(invalid("Cargo differs from required production toolchain"));
    }
    for name in [
        "cc",
        "ld",
        "as",
        "ar",
        "musl-gcc",
        "cargo-machete",
        "cargo-deny",
        "mdbook",
    ] {
        if let Some(path) = locate(name) {
            programs.insert(name.into(), path);
        }
    }
    if !programs.contains_key("cc") || !programs.contains_key("ld") {
        return Err(invalid(
            "MISSING_NATIVE_LINKER: cc and ld must be installed and explicitly measured",
        ));
    }
    let linker_version = probe(&programs["cc"], &["--version"], &budget, authorize)?;
    let mut closure = BTreeMap::new();
    for (name, bytes) in [
        ("rustc-version", version),
        ("cargo-version", cargo_version),
        ("linker-version", linker_version),
        (
            "platform",
            format!(
                "{}:{}:{host}\n",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
            .into_bytes(),
        ),
    ] {
        let path = storage.path().join(name);
        write_new(&path, &bytes)?;
        closure.insert(format!("resolution:{name}"), path);
    }
    // Installation manifests and actual declared compiler runtime/standard
    // library bytes, without trying to fingerprint every operating-system file.
    let rustlib = sysroot.join("lib/rustlib");
    for entry in fs::read_dir(&rustlib)? {
        budget.remaining()?;
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_file() {
            closure.insert(format!("sysroot:{name}"), entry.path().canonicalize()?);
        }
    }
    for entry in fs::read_dir(sysroot.join("lib"))? {
        budget.remaining()?;
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            closure.insert(
                format!("compiler-runtime:{}", entry.file_name().to_string_lossy()),
                path.canonicalize()?,
            );
        }
    }
    for target in [
        host.as_str(),
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
    ] {
        let directory = rustlib.join(target).join("lib");
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(directory)? {
            budget.remaining()?;
            let entry = entry?;
            if entry.path().is_file() {
                closure.insert(
                    format!(
                        "rust-target:{target}:{}",
                        entry.file_name().to_string_lossy()
                    ),
                    entry.path().canonicalize()?,
                );
            }
        }
    }
    if !closure
        .keys()
        .any(|name| name.starts_with(&format!("rust-target:{host}:")))
    {
        return Err(invalid(
            "missing installed host Rust standard-library closure",
        ));
    }
    if closure.len() > options.max_files {
        return Err(invalid("native toolchain inventory exceeds limit"));
    }
    // Verify all supplied files are bounded regular material before returning.
    // Execution and before/after captures will hash them again independently.
    for path in closure.values().chain(programs.values()) {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > budget.bytes_left
        {
            return Err(invalid("native toolchain closure exceeds material limits"));
        }
        budget.bytes_left -= metadata.len();
    }
    let mut directories = BTreeSet::new();
    directories.insert(sysroot.join("bin"));
    for path in programs.values() {
        if let Some(parent) = path.parent() {
            directories.insert(parent.to_path_buf());
        }
    }
    let path = std::env::join_paths(directories)
        .map_err(io::Error::other)?
        .to_string_lossy()
        .into_owned();
    let environment = BTreeMap::from([
        ("PATH".into(), native_proof::Value::Literal(path)),
        (
            "HOME".into(),
            native_proof::Value::Literal(
                storage.path().join("home").to_string_lossy().into_owned(),
            ),
        ),
        (
            "CARGO_HOME".into(),
            native_proof::Value::Literal(
                storage.path().join("cargo").to_string_lossy().into_owned(),
            ),
        ),
        (
            "RUSTFLAGS".into(),
            native_proof::Value::Literal(format!("-C linker={}", programs["cc"].display())),
        ),
    ]);
    Ok(PreparedNativeTools {
        tools: ReviewedNativeTools {
            programs,
            closure,
            environment,
        },
        storage,
    })
}

#[cfg(test)]
mod tests;

pub mod snapshot;

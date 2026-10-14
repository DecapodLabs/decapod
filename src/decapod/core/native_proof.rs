//! Bounded execution adapter for independently reviewed, local-only proof plans.
//!
//! This module is NOT a YAML interpreter, command authorizer, OS sandbox, source
//! inventory planner or evidence importer. The trusted caller reviews substantive
//! direct invocations and their transitive tools. Imported workflow text must
//! never be converted directly to Invocation. No shell/eval interface exists.
//! Process groups cover ordinary children, not malicious setsid/double-fork
//! escapes. Daemonizing, external writes/network, credentials and publication
//! must be excluded by review or an upstream sandbox; they are unsupported here.
use super::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use super::readiness_evidence::{
    self as evidence, ApplicabilityFacts, CheckKey, CheckKind, CheckResult, Conclusion, Dependency,
    ExpectedPlan, Failure, MaterialIdentity, Observation, Provider, Requirement, SuiteEvidence,
    material,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum NativePlatform {
    LinuxX86_64,
    LinuxAarch64,
    MacosX86_64,
    MacosAarch64,
    WindowsX86_64,
}
impl NativePlatform {
    pub fn current() -> io::Result<Self> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Ok(Self::LinuxX86_64),
            ("linux", "aarch64") => Ok(Self::LinuxAarch64),
            ("macos", "x86_64") => Ok(Self::MacosX86_64),
            ("macos", "aarch64") => Ok(Self::MacosAarch64),
            _ => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native execution platform has no supported adapter",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ExecutionPurpose {
    SourceCheck,
    ReleaseProof,
    CandidateBuild,
}

/// Reviewer assertion about the whole invocation, including subprocesses.
/// This is intentionally not Deserialize and is not a security sandbox claim.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum ReviewedCapabilities {
    LocalCheckWithOwnedScratch,
    /// Explicit repository policy, validated against the complete source,
    /// command and execution declaration digests before host construction.
    /// Its declared toolchain files are measured, not inferred complete. This
    /// is local command evidence and is not a sandbox or provider attestation.
    RepositoryDeclaredLocalCheck,
}

/// Values are passed as individual arguments, never interpreted as shell text.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Value {
    Literal(String),
    ScratchPath(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct Invocation {
    pub platform: NativePlatform,
    pub program: PathBuf,
    /// Reviewed local toolchain closure (compiler, linker, runtime libraries,
    /// pinned SDK metadata). RepositoryDeclaredLocalCheck uses the explicit
    /// repository declaration rather than claiming independently inferred
    /// completeness. 'program' is reserved; the direct executable is always
    /// included automatically. No ambient discovery.
    pub toolchain_files: BTreeMap<String, PathBuf>,
    pub arguments: Vec<Value>,
    /// Repository-relative directory; '.' means the root.
    pub directory: PathBuf,
    /// Exact reviewed environment. Ambient environment is cleared.
    pub environment: BTreeMap<String, Value>,
    pub purpose: ExecutionPurpose,
    pub capabilities: ReviewedCapabilities,
    pub timeout_ms: u64,
    pub output_limit: usize,
    /// Files that MUST be produced in this check's fresh owned scratch directory.
    pub artifacts: BTreeSet<String>,
}
impl Invocation {
    pub fn digest(&self) -> io::Result<String> {
        json_digest(self)
    }
}

/// Policy-selected input inventories, independent of observed successes.
/// Actual file bytes/modes and Git revisions/history are measured at each capture.
#[derive(Debug, Clone)]
pub struct IdentityInputs {
    pub root: PathBuf,
    pub git_program: PathBuf,
    /// Stable repository identity from trusted project context, not a receipt.
    pub repository: String,
    pub base_branch: String,
    /// Exact local ref for the accepted base, such as refs/remotes/origin/master.
    pub accepted_base_ref: String,
    pub declared_untracked: BTreeSet<String>,
    pub configuration: BTreeSet<String>,
    pub workflows: BTreeSet<String>,
    pub candidate_inputs: BTreeSet<String>,
}

/// Ownership seam: caller prepares a standalone Git copy/dirty overlay inside
/// a newly allocated TempDir, detaches it at source H, and transfers ownership.
/// Linked worktrees, symlinked metadata and mutable source roots are rejected.
/// Preparing it must not change source refs/index/control-plane state.
#[derive(Debug)]
pub struct OwnedExecutionSnapshot {
    storage: tempfile::TempDir,
    root: PathBuf,
}
impl OwnedExecutionSnapshot {
    pub fn prepared(storage: tempfile::TempDir, checkout: &Path) -> io::Result<Self> {
        let root = contained(storage.path(), checkout, false)?;
        if !root.is_dir() || root.canonicalize()? != root {
            return Err(invalid("snapshot checkout must be an owned directory"));
        }
        let metadata = fs::symlink_metadata(root.join(".git"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(invalid(
                "snapshot requires standalone .git directory, not a linked worktree",
            ));
        }
        if root.join(".git/objects/info/alternates").exists() {
            return Err(invalid("snapshot may not share an alternate object store"));
        }
        Ok(Self { storage, root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SuiteLimits {
    pub timeout: Duration,
    pub max_checks: usize,
    pub max_hashed_bytes: u64,
    pub max_output_bytes: usize,
    pub git_output_limit: usize,
}

#[derive(Debug)]
pub struct CheckDiagnostic {
    pub key: CheckKey,
    pub duration: Duration,
    pub termination: Termination,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct NativeRun {
    pub evidence: SuiteEvidence,
    pub diagnostics: Vec<CheckDiagnostic>,
    /// Set if execution stops early or post-run capture/cleanup cannot complete.
    /// Callers MUST fail closed on this field, even before calling verify().
    pub failure: Option<String>,
}

struct Budget<'a> {
    deadline: Instant,
    cancel: &'a Cancellation,
    remaining_bytes: u64,
    git_program: PathBuf,
    git_output_limit: usize,
}
impl Budget<'_> {
    fn remaining(&self) -> io::Result<Duration> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "native suite cancelled",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "native suite deadline exceeded")
            })
    }
    fn hash_file(&mut self, path: &Path) -> io::Result<String> {
        self.remaining()?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(invalid("expected a regular, non-symlink file"));
        }
        if metadata.len() > self.remaining_bytes {
            return Err(invalid("suite hashing byte budget exceeded"));
        }
        // O_NOFOLLOW prevents replacement with a symlink between inspection/open.
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let mut file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("file changed type during open"));
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            self.remaining()?;
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            self.remaining_bytes = self
                .remaining_bytes
                .checked_sub(n as u64)
                .ok_or_else(|| invalid("suite hashing byte budget exceeded"))?;
            hash.update(&buffer[..n]);
        }
        self.remaining()?;
        Ok(format!("sha256:{:x}", hash.finalize()))
    }
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        self.git_bounded(root, args, None)
    }
    // Preparation writes are only to freshly owned snapshot paths. The file
    // limit is child-local and prevents a bundle/index-pack file from exceeding
    // its allotted disk budget before the caller can inspect it.
    fn git_bounded(
        &mut self,
        root: &Path,
        args: &[&str],
        max_file_bytes: Option<u64>,
    ) -> io::Result<Vec<u8>> {
        self.git_accepted(root, args, max_file_bytes, &[0])
    }
    fn git_accepted(
        &mut self,
        root: &Path,
        args: &[&str],
        max_file_bytes: Option<u64>,
        accepted: &[i32],
    ) -> io::Result<Vec<u8>> {
        let timeout = self.remaining()?.min(Duration::from_secs(60));
        let mut command = Command::new(&self.git_program);
        command
            .arg("--no-optional-locks")
            .args([
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.untrackedCache=false",
                "-c",
                "protocol.allow=never",
                "-c",
                "submodule.recurse=false",
                "-c",
                "gc.auto=0",
                "-c",
                "maintenance.auto=false",
            ])
            .args(args)
            .current_dir(root)
            .env_clear()
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ALLOW_PROTOCOL", "")
            .env("GIT_PROTOCOL_FROM_USER", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_NO_LAZY_FETCH", "1");
        #[cfg(unix)]
        if let Some(limit) = max_file_bytes {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(move || {
                    let limit = libc::rlimit {
                        rlim_cur: limit as libc::rlim_t,
                        rlim_max: limit as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let result = command.bounded_capture(
            ProcessLimits {
                timeout,
                max_output_bytes: self.git_output_limit,
            },
            self.cancel,
        )?;
        if result.termination != Termination::Exited
            || !result
                .output
                .status
                .code()
                .is_some_and(|code| accepted.contains(&code))
        {
            return Err(io::Error::other(format!(
                "bounded material Git query failed: {:?}",
                result.termination
            )));
        }
        self.remaining()?;
        Ok(result.output.stdout)
    }
}
impl material::CaptureIo for Budget<'_> {
    fn git(&mut self, root: &Path, args: &[&str]) -> io::Result<Vec<u8>> {
        Budget::git(self, root, args)
    }
    fn file_digest(&mut self, path: &Path) -> io::Result<String> {
        self.hash_file(path)
    }
    fn checkpoint(&mut self) -> io::Result<()> {
        self.remaining().map(|_| ())
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn json_digest(value: &impl Serialize) -> io::Result<String> {
    evidence::digest(value).map_err(io::Error::other)
}
fn bytes_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn text(bytes: Vec<u8>) -> io::Result<String> {
    String::from_utf8(bytes)
        .map(|s| s.trim().to_owned())
        .map_err(|_| invalid("non-UTF-8 Git identity"))
}
fn relative(path: &Path, allow_root: bool) -> bool {
    if path.to_str().is_none_or(|p| p.contains(['\\', '\0'])) {
        return false;
    }
    (allow_root && path == Path::new("."))
        || (!path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, Component::Normal(p) if p != ".git" && p != ".decapod")))
}
fn contained(root: &Path, path: &Path, allow_root: bool) -> io::Result<PathBuf> {
    if !relative(path, allow_root) {
        return Err(invalid("unsafe repository-relative path"));
    }
    let mut current = root.to_path_buf();
    if path != Path::new(".") {
        for component in path.components() {
            current.push(component);
            if fs::symlink_metadata(&current)?.file_type().is_symlink() {
                return Err(invalid("symlink path in execution input"));
            }
        }
    }
    Ok(current)
}

fn group_digest(
    material: &material::WorkingMaterial,
    paths: &BTreeSet<String>,
) -> io::Result<String> {
    if paths.is_empty() {
        return Err(invalid("empty independently reviewed identity inventory"));
    }
    let mut entries = BTreeMap::new();
    for path in paths {
        let entry = material
            .files
            .get(path)
            .filter(|e| e.content_digest.is_some())
            .ok_or_else(|| invalid(format!("missing bound input: {path}")))?;
        entries.insert(path, entry);
    }
    json_digest(&entries)
}

fn capture_identity(
    inputs: &IdentityInputs,
    invocations: &BTreeMap<CheckKey, Invocation>,
    budget: &mut Budget<'_>,
) -> io::Result<MaterialIdentity> {
    let material = material::capture_with_io(&inputs.root, &inputs.declared_untracked, budget)?;
    let accepted_base_revision = text(budget.git(
        &inputs.root,
        &[
            "rev-parse",
            "--verify",
            &format!("{}^{{commit}}", inputs.accepted_base_ref),
        ],
    )?)?;
    let range = format!("{accepted_base_revision}..{}", material.head_revision);
    let history = budget.git(
        &inputs.root,
        &[
            "log",
            "--no-decorate",
            "--no-show-signature",
            "--format=raw",
            "--topo-order",
            &range,
            "--",
        ],
    )?;
    let mut toolchain = BTreeMap::new();
    for (key, invocation) in invocations {
        toolchain.insert(key.clone(), measured_toolchain(invocation, budget)?);
    }
    let git_identity = budget.hash_file(&inputs.git_program)?;
    let identity = MaterialIdentity {
        repository: inputs.repository.clone(),
        work_branch: material.work_branch.clone(),
        head_revision: material.head_revision.clone(),
        base_branch: inputs.base_branch.clone(),
        accepted_base_revision,
        history_digest: json_digest(&(
            bytes_digest(&history),
            filtered_index(inputs, budget)?,
            tag_refs(&inputs.root, budget)?,
        ))?,
        tree_digest: material.content_digest().map_err(io::Error::other)?,
        configuration_digest: group_digest(&material, &inputs.configuration)?,
        workflow_digest: group_digest(&material, &inputs.workflows)?,
        toolchain_digest: json_digest(&(git_identity, toolchain.into_iter().collect::<Vec<_>>()))?,
        candidate_digest: group_digest(&material, &inputs.candidate_inputs)?,
    };
    budget.remaining()?;
    Ok(identity)
}

/// Measured direct executable identity. Transitive compilers/images/libraries
/// must additionally be pinned and verified by the planner's execution profile.
pub fn executable_digest(program: &Path) -> io::Result<String> {
    // Convenience for trusted planning, deliberately bounded in bytes and time.
    let cancel = Cancellation::default();
    let mut budget = Budget {
        deadline: Instant::now() + Duration::from_secs(15),
        cancel: &cancel,
        remaining_bytes: 1024 * 1024 * 1024,
        git_program: PathBuf::new(),
        git_output_limit: 1,
    };
    budget.hash_file(program)
}

fn measured_toolchain(invocation: &Invocation, budget: &mut Budget<'_>) -> io::Result<String> {
    let mut files = BTreeMap::new();
    files.insert(
        "program".to_string(),
        budget.hash_file(&invocation.program)?,
    );
    for (name, path) in &invocation.toolchain_files {
        if name.trim().is_empty()
            || name == "program"
            || !path.is_absolute()
            || path.canonicalize()? != *path
        {
            return Err(invalid(
                "invalid independently reviewed toolchain inventory",
            ));
        }
        files.insert(name.clone(), budget.hash_file(path)?);
    }
    json_digest(&files)
}

/// Measure the planner-selected toolchain closure. Completeness of that closure
/// is a planner/reviewer obligation; unsupported ambient dependencies must fail
/// there rather than be silently omitted here.
pub fn toolchain_digest(invocation: &Invocation) -> io::Result<String> {
    let cancel = Cancellation::default();
    let mut budget = Budget {
        deadline: Instant::now() + Duration::from_secs(15),
        cancel: &cancel,
        remaining_bytes: 1024 * 1024 * 1024,
        git_program: PathBuf::new(),
        git_output_limit: 1,
    };
    measured_toolchain(invocation, &mut budget)
}

fn native_executable(program: &Path) -> io::Result<()> {
    if !program.is_absolute() || program.canonicalize()? != program {
        return Err(invalid("tool must be an absolute canonical path"));
    }
    let metadata = fs::symlink_metadata(program)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("tool must be a regular native executable"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(invalid("tool is not executable"));
        }
    }
    let mut magic = [0; 4];
    std::fs::File::open(program)?.read_exact(&mut magic)?;
    #[cfg(target_os = "linux")]
    if magic != *b"\x7fELF" {
        return Err(invalid(
            "native Linux tool must be ELF; scripts are unsupported",
        ));
    }
    #[cfg(target_os = "macos")]
    if ![
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xbe, 0xba, 0xfe, 0xca],
    ]
    .contains(&magic)
    {
        return Err(invalid(
            "native macOS tool must be Mach-O; scripts are unsupported",
        ));
    }
    Ok(())
}

fn validate_invocation(
    invocation: &Invocation,
    inputs: &IdentityInputs,
    budget: &mut Budget<'_>,
    self_digest: &str,
) -> io::Result<()> {
    if invocation.platform != NativePlatform::current()? {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "reviewed execution profile requires a different platform",
        ));
    }
    native_executable(&invocation.program)?;
    if invocation.program.starts_with(&inputs.root) {
        return Err(invalid(
            "proof executable must not come from mutable source worktree",
        ));
    }
    if !invocation.program.is_absolute() || invocation.program.canonicalize()? != invocation.program
    {
        return Err(invalid(
            "program must be an absolute canonical executable path",
        ));
    }
    let metadata = fs::metadata(&invocation.program)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
            return Err(invalid("tool is not executable"));
        }
    }
    let name = invocation
        .program
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or_else(|| invalid("non-UTF-8 program"))?;
    if [
        "sh",
        "bash",
        "dash",
        "zsh",
        "fish",
        "cmd",
        "powershell",
        "pwsh",
        "env",
        "sudo",
        "su",
        "ssh",
        "xargs",
        "timeout",
        "docker",
        "podman",
        "decapod",
        "decapod-native-proof",
    ]
    .contains(&name)
        || (invocation.capabilities != ReviewedCapabilities::RepositoryDeclaredLocalCheck
            && (name == "make"
                || name.starts_with("python")
                || name.starts_with("node")
                || name.starts_with("perl")
                || name.starts_with("ruby")))
    {
        return Err(invalid(
            "shell, interpreter, wrapper or recursive validator invocation is unsupported",
        ));
    }
    let this = std::env::current_exe()?.canonicalize()?;
    if invocation.program == this || budget.hash_file(&invocation.program)? == self_digest {
        return Err(invalid("recursive self invocation is forbidden"));
    }
    if !contained(&inputs.root, &invocation.directory, true)?.is_dir() {
        return Err(invalid("working directory is not a directory"));
    }
    if invocation.timeout_ms == 0
        || invocation.output_limit == 0
        || invocation.output_limit > 64 * 1024 * 1024
    {
        return Err(invalid("invalid per-check budget"));
    }
    if invocation.purpose == ExecutionPurpose::CandidateBuild && invocation.artifacts.is_empty() {
        return Err(invalid("candidate build must produce an artifact"));
    }
    for value in invocation
        .arguments
        .iter()
        .chain(invocation.environment.values())
    {
        match value {
            Value::Literal(value)
                if value.contains('\0')
                    || matches!(
                        value.as_str(),
                        "validate"
                            | "validate.run"
                            | "decapod"
                            | "publish"
                            | "deploy"
                            | "push"
                            | "upload"
                    ) =>
            {
                return Err(invalid(
                    "recursive validation/publication or malformed argument",
                ));
            }
            Value::ScratchPath(path) if !relative(Path::new(path), false) => {
                return Err(invalid("invalid scratch path"));
            }
            _ => {}
        }
    }
    for key in invocation.environment.keys() {
        let upper = key.to_ascii_uppercase();
        if key.is_empty()
            || key.contains(['=', '\0'])
            || ["TMPDIR", "TMP", "TEMP", "DECAPOD_NATIVE_PROOF_ACTIVE"].contains(&key.as_str())
            || ["TOKEN", "PASSWORD", "SECRET", "CREDENTIAL"]
                .iter()
                .any(|word| upper.contains(word))
        {
            return Err(invalid("unsafe or reserved execution environment key"));
        }
    }
    for artifact in &invocation.artifacts {
        if !relative(Path::new(artifact), false) {
            return Err(invalid("invalid artifact path"));
        }
    }
    Ok(())
}

fn expand(value: &Value, scratch: &Path) -> std::ffi::OsString {
    match value {
        Value::Literal(v) => v.into(),
        Value::ScratchPath(v) => scratch.join(v).into_os_string(),
    }
}
fn purpose(kind: CheckKind) -> Option<ExecutionPurpose> {
    match kind {
        CheckKind::SourceCi | CheckKind::RepositoryLocal => Some(ExecutionPurpose::SourceCheck),
        CheckKind::DerivedRelease => Some(ExecutionPurpose::ReleaseProof),
        CheckKind::CandidateBuild => Some(ExecutionPurpose::CandidateBuild),
        CheckKind::Validator => None,
    }
}

/// Execute only after source-policy review has bound every invocation digest and
/// direct tool digest in `plan`. No receipt, YAML or shell is read as authority.
/// A run is usable only when `failure` is None AND readiness_evidence::verify
/// accepts it against a freshly captured current identity and current facts.
pub fn run_native(
    plan: &ExpectedPlan,
    invocations: &BTreeMap<CheckKey, Invocation>,
    inputs: &IdentityInputs,
    facts: &ApplicabilityFacts,
    limits: SuiteLimits,
    snapshot: &OwnedExecutionSnapshot,
    cancel: &Cancellation,
) -> io::Result<NativeRun> {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native proof requires a Unix process-group adapter",
        ));
    }
    if std::env::var_os("DECAPOD_NATIVE_PROOF_ACTIVE").is_some() {
        return Err(invalid("recursive native proof execution is forbidden"));
    }
    if plan.checks.is_empty()
        || plan.checks.len() > limits.max_checks
        || limits.timeout.is_zero()
        || limits.max_hashed_bytes == 0
        || limits.max_output_bytes == 0
        || limits.max_output_bytes > 256 * 1024 * 1024
        || limits.git_output_limit == 0
        || limits.git_output_limit > 64 * 1024 * 1024
    {
        return Err(invalid("empty or invalid native suite budget/plan"));
    }
    if inputs.root.canonicalize()? != inputs.root
        || inputs.git_program.canonicalize()? != inputs.git_program
        || inputs.repository.trim().is_empty()
        || !inputs.accepted_base_ref.starts_with("refs/")
        || inputs.accepted_base_ref.contains(['\0', ' ', '\n'])
    {
        return Err(invalid("invalid independently supplied identity inputs"));
    }
    native_executable(&inputs.git_program)?;
    if inputs.git_program.file_name().and_then(|n| n.to_str()) != Some("git") {
        return Err(invalid("material capture requires the reviewed Git tool"));
    }
    let mut budget = Budget {
        deadline: Instant::now()
            .checked_add(limits.timeout)
            .ok_or_else(|| invalid("suite deadline overflow"))?,
        cancel,
        remaining_bytes: limits.max_hashed_bytes,
        git_program: inputs.git_program.clone(),
        git_output_limit: limits.git_output_limit,
    };
    budget.remaining()?;
    if snapshot.root.starts_with(&inputs.root) || inputs.root.starts_with(&snapshot.root) {
        return Err(invalid(
            "execution snapshot must be separate from source worktree",
        ));
    }
    let before = capture_identity(inputs, invocations, &mut budget)?;
    let snapshot_before = capture_snapshot(snapshot, inputs, &before, &mut budget)?;
    let mut results = Vec::new();
    // Synthesize only non-execution predicate evidence; verifier proves it from
    // the independently supplied current facts before any proof command starts.
    for check in &plan.checks {
        if let Requirement::NotApplicable(predicate) = &check.requirement {
            results.push(CheckResult {
                key: check.key.clone(),
                definition_digest: check.definition_digest.clone(),
                invocation_digest: check.invocation_digest.clone(),
                toolchain_digest: check.toolchain_digest.clone(),
                executor: check.executor,
                identity: before.clone(),
                observation: Observation::NotApplicable {
                    predicate_digest: json_digest(predicate)?,
                },
            });
        }
    }
    let preflight = SuiteEvidence {
        schema_version: evidence::SCHEMA_VERSION.into(),
        expected_plan_digest: plan.digest().map_err(io::Error::other)?,
        before: before.clone(),
        after: before.clone(),
        facts: facts.clone(),
        results: results.clone(),
    };
    let failures: Vec<_> = evidence::verify(plan, &before, facts, &preflight)
        .failures
        .into_iter()
        .filter(|f| !matches!(f, Failure::Missing(_)))
        .collect();
    if !failures.is_empty() {
        return Err(invalid(format!("invalid native plan: {failures:?}")));
    }
    let executing: BTreeSet<_> = plan
        .checks
        .iter()
        .filter(|c| c.requirement == Requirement::Execute)
        .map(|c| c.key.clone())
        .collect();
    if executing.is_empty() || executing != invocations.keys().cloned().collect() {
        return Err(invalid("native invocation inventory mismatch"));
    }
    let self_digest = budget.hash_file(&std::env::current_exe()?.canonicalize()?)?;
    for check in plan
        .checks
        .iter()
        .filter(|c| c.requirement == Requirement::Execute)
    {
        let invocation = &invocations[&check.key];
        if check.executor != Provider::Native
            || purpose(check.kind) != Some(invocation.purpose)
            || check.invocation_digest != invocation.digest()?
            || (invocation.capabilities == ReviewedCapabilities::RepositoryDeclaredLocalCheck
                && check.kind != CheckKind::RepositoryLocal)
        {
            return Err(invalid("unreviewed native invocation/purpose/backend"));
        }
        validate_invocation(invocation, inputs, &mut budget, &self_digest)?;
        if check.toolchain_digest != measured_toolchain(invocation, &mut budget)? {
            return Err(invalid("reviewed executable digest changed"));
        }
    }
    let mut done: BTreeSet<_> = results.iter().map(|r| r.key.clone()).collect();
    let mut diagnostics = Vec::new();
    let mut remaining_output = limits.max_output_bytes;
    let mut failure = None;
    while done.len() < plan.checks.len() {
        if let Err(error) = budget.remaining() {
            failure = Some(error.to_string());
            break;
        }
        let check = plan
            .checks
            .iter()
            .find(|c| {
                !done.contains(&c.key)
                    && c.dependencies
                        .iter()
                        .all(|d| matches!(d, Dependency::Check(k) if done.contains(k)))
            })
            .ok_or_else(|| invalid("native plan dependency cycle"))?;
        let invocation = &invocations[&check.key];
        // Detect direct tool replacement after preflight; material is recaptured
        // after the suite. Fully race-free execution requires an isolated snapshot.
        let actual_tool_digest = measured_toolchain(invocation, &mut budget)?;
        if actual_tool_digest != check.toolchain_digest {
            failure = Some("tool changed after preflight".into());
            break;
        }
        let scratch = tempfile::Builder::new()
            .prefix("native-proof-")
            .tempdir_in(snapshot.storage.path())?;
        if invocation.capabilities == ReviewedCapabilities::RepositoryDeclaredLocalCheck {
            // The repository host reserves HOME for this owned directory;
            // ambient HOME is not inherited. This is not an OS sandbox.
            fs::create_dir(scratch.path().join("home"))?;
        }
        let execution_id = format!(
            "native:{}:{}",
            std::process::id(),
            scratch.path().file_name().unwrap().to_string_lossy()
        );
        let mut command = Command::new(&invocation.program);
        command
            .current_dir(contained(&snapshot.root, &invocation.directory, true)?)
            .env_clear();
        for argument in &invocation.arguments {
            command.arg(expand(argument, scratch.path()));
        }
        for (key, value) in &invocation.environment {
            command.env(key, expand(value, scratch.path()));
        }
        command
            .env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path())
            .env("DECAPOD_NATIVE_PROOF_ACTIVE", "1");
        let timeout = budget
            .remaining()?
            .min(Duration::from_millis(invocation.timeout_ms));
        if remaining_output < 2 {
            let _ = scratch.close();
            failure = Some("suite output budget exhausted".into());
            break;
        }
        let captured = command.bounded_capture(
            ProcessLimits {
                timeout,
                max_output_bytes: invocation.output_limit.min(remaining_output / 2),
            },
            cancel,
        );
        let captured = match captured {
            Ok(capture) => capture,
            Err(error) => {
                let cleanup = scratch.close();
                failure = Some(format!(
                    "native spawn/capture failed: {error}; cleanup={cleanup:?}"
                ));
                break;
            }
        };
        remaining_output -= captured.output.stdout.len() + captured.output.stderr.len();
        let mut conclusion = match captured.termination {
            Termination::Exited if captured.output.status.success() => Conclusion::Success,
            Termination::Exited | Termination::OutputLimit => Conclusion::Failure,
            Termination::Cancelled => Conclusion::Cancelled,
            Termination::TimedOut => Conclusion::TimedOut,
        };
        let mut error = None;
        let mut artifact_digest = None;
        if conclusion == Conclusion::Success && !invocation.artifacts.is_empty() {
            let mut artifacts = BTreeMap::new();
            for artifact in &invocation.artifacts {
                let measured =
                    contained(scratch.path(), Path::new(artifact), false).and_then(|path| {
                        if fs::symlink_metadata(&path)?.len() == 0 {
                            return Err(invalid("produced artifact is empty"));
                        }
                        budget.hash_file(&path)
                    });
                match measured {
                    Ok(hash) => {
                        artifacts.insert(artifact, hash);
                    }
                    Err(cause) => {
                        error = Some(format!(
                            "required produced artifact missing/invalid: {cause}"
                        ));
                        conclusion = Conclusion::Failure;
                        break;
                    }
                }
            }
            if conclusion == Conclusion::Success {
                artifact_digest = Some(json_digest(&artifacts)?);
            }
        }
        if let Err(cause) = scratch.close() {
            error = Some(format!("owned scratch cleanup failed: {cause}"));
            conclusion = Conclusion::Failure;
        }
        let mut output_hash = Sha256::new();
        output_hash.update(b"native-capture-v1\0");
        output_hash.update(format!("{:?}\0", captured.termination));
        for stream in [&captured.output.stdout, &captured.output.stderr] {
            output_hash.update((stream.len() as u64).to_be_bytes());
            output_hash.update(stream);
        }
        let output_digest = format!("sha256:{:x}", output_hash.finalize());
        results.push(CheckResult {
            key: check.key.clone(),
            definition_digest: check.definition_digest.clone(),
            invocation_digest: invocation.digest()?,
            toolchain_digest: actual_tool_digest,
            executor: Provider::Native,
            identity: before.clone(),
            observation: Observation::Executed {
                execution_id,
                conclusion,
                exit_code: captured.output.status.code(),
                output_digest,
                artifact_digest,
            },
        });
        diagnostics.push(CheckDiagnostic {
            key: check.key.clone(),
            duration: captured.duration,
            termination: captured.termination,
            stdout: captured.output.stdout,
            stderr: captured.output.stderr,
            error,
        });
        done.insert(check.key.clone());
        if conclusion != Conclusion::Success {
            failure = Some(format!("native check did not succeed: {:?}", check.key));
            break;
        }
    }
    match capture_snapshot(snapshot, inputs, &before, &mut budget) {
        Ok(snapshot_after) if snapshot_after == snapshot_before => {}
        Ok(_) => {
            failure.get_or_insert_with(|| "execution snapshot changed during suite".into());
        }
        Err(error) => {
            failure.get_or_insert_with(|| format!("execution snapshot recapture failed: {error}"));
        }
    }
    // Never copy 'before' into 'after' as evidence of recapture. If recapture is
    // impossible, use an invalid sentinel so even consumers ignoring failure
    // cannot accidentally turn this incomplete run into readiness.
    let after = match capture_identity(inputs, invocations, &mut budget) {
        Ok(after) => after,
        Err(error) => {
            failure.get_or_insert_with(|| format!("post-run identity capture failed: {error}"));
            let mut invalid = before.clone();
            invalid.tree_digest.clear();
            invalid
        }
    };
    if before != after {
        failure.get_or_insert_with(|| "material changed during native suite".into());
    }
    // Snapshot/custody/cleanup failure must remain NotReady even if a caller
    // accidentally drops the diagnostic envelope and invokes the pure verifier.
    if let Some(cause) = &failure {
        if let Some(CheckResult {
            observation: Observation::Executed { conclusion, .. },
            ..
        }) = results
            .iter_mut()
            .rev()
            .find(|r| matches!(r.observation, Observation::Executed { .. }))
            && *conclusion == Conclusion::Success
        {
            *conclusion = Conclusion::Failure;
        }
        if let Some(diagnostic) = diagnostics.last_mut() {
            diagnostic.error.get_or_insert_with(|| cause.clone());
        }
    }
    Ok(NativeRun {
        evidence: SuiteEvidence {
            after,
            results,
            ..preflight
        },
        diagnostics,
        failure,
    })
}

/// Fresh independently measured identity for the final verification boundary.
/// Input inventories/profile trust has the same contract as run_native.
pub fn capture_current_identity(
    inputs: &IdentityInputs,
    invocations: &BTreeMap<CheckKey, Invocation>,
    limits: SuiteLimits,
    cancel: &Cancellation,
) -> io::Result<MaterialIdentity> {
    native_executable(&inputs.git_program)?;
    let mut budget = Budget {
        deadline: Instant::now()
            .checked_add(limits.timeout)
            .ok_or_else(|| invalid("suite deadline overflow"))?,
        cancel,
        remaining_bytes: limits.max_hashed_bytes,
        git_program: inputs.git_program.clone(),
        git_output_limit: limits.git_output_limit,
    };
    capture_identity(inputs, invocations, &mut budget)
}

fn capture_snapshot(
    snapshot: &OwnedExecutionSnapshot,
    inputs: &IdentityInputs,
    source: &MaterialIdentity,
    budget: &mut Budget<'_>,
) -> io::Result<String> {
    let actual_root = PathBuf::from(text(
        budget.git(&snapshot.root, &["rev-parse", "--show-toplevel"])?,
    )?);
    if actual_root.canonicalize()? != snapshot.root {
        return Err(invalid(
            "snapshot Git worktree resolves outside the owned checkout",
        ));
    }
    let captured =
        material::capture_detached_with_io(&snapshot.root, &inputs.declared_untracked, budget)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        for (path, entry) in &captured.files {
            budget.remaining()?;
            if matches!(entry.mode.as_str(), "100644" | "100755")
                && fs::symlink_metadata(snapshot.root.join(path))?.nlink() != 1
            {
                return Err(invalid(
                    "execution material must not be hardlinked to another checkout",
                ));
            }
        }
    }
    if tag_refs(&inputs.root, budget)? != tag_refs(&snapshot.root, budget)? {
        return Err(invalid(
            "snapshot tag inventory differs from original source",
        ));
    }
    let mut snapshot_inputs = inputs.clone();
    snapshot_inputs.root = snapshot.root.clone();
    if filtered_index(inputs, budget)? != filtered_index(&snapshot_inputs, budget)? {
        return Err(invalid(
            "snapshot staging does not match original source index",
        ));
    }
    let content = captured.content_digest().map_err(io::Error::other)?;
    if captured.head_revision != source.head_revision || content != source.tree_digest {
        return Err(invalid(
            "execution snapshot does not match source HEAD and material",
        ));
    }
    Ok(content)
}

/// Bounded, source-preserving creation of owned native execution snapshots.
pub mod snapshot;

fn filtered_index(inputs: &IdentityInputs, budget: &mut Budget<'_>) -> io::Result<Vec<String>> {
    let bytes = budget.git(&inputs.root, &["ls-files", "--stage", "-z"])?;
    let mut records = Vec::new();
    for record in bytes.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let record = std::str::from_utf8(record).map_err(|_| invalid("non-UTF-8 index input"))?;
        let (_, path) = record
            .split_once('\t')
            .ok_or_else(|| invalid("malformed index record"))?;
        if !material::is_bookkeeping(path) {
            records.push(record.to_owned());
        }
    }
    records.sort();
    Ok(records)
}

fn tag_refs(root: &Path, budget: &mut Budget<'_>) -> io::Result<BTreeMap<String, String>> {
    let bytes = budget.git(
        root,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            "refs/tags/",
        ],
    )?;
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid("non-UTF-8 tag ref"))?;
    let mut tags = BTreeMap::new();
    for line in text.lines() {
        let (name, oid) = line
            .split_once(' ')
            .ok_or_else(|| invalid("invalid tag ref"))?;
        if !name.starts_with("refs/tags/") || tags.insert(name.into(), oid.into()).is_some() {
            return Err(invalid("invalid/duplicate tag ref inventory"));
        }
    }
    Ok(tags)
}

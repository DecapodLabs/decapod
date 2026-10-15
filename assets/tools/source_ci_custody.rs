//! Measured, bounded custody around the actual finite source-CI recipe.
//! This manifest is not authenticated by its own contents. A collector must bind
//! the uploaded bytes to the exact provider job/attempt and independently review
//! the expected recipe/profile/toolchain. Whole-profile readiness remains the collector’s responsibility.
use super::*;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::process::ExitStatus;
use std::thread;

#[allow(dead_code)]
#[path = "integration_custody.rs"]
pub mod integration_custody;
use integration_custody as hashing;

#[allow(dead_code)]
#[path = "ci_event_context.rs"]
pub mod ci_context;

#[path = "source_ci_toolchain.rs"]
pub mod declared_toolchain;

pub const SCHEMA: &str = "decapod.source-ci.execution.v2";
pub const DUAL_ROOT_SCHEMA: &str = "decapod.source-ci.dual-root.v1";
pub const DUAL_ROOT_PREPARATION_SCHEMA: &str = "decapod.source-ci.preparation.v2";
pub const DUAL_ROOT_EXECUTION_SCHEMA: &str = "decapod.source-ci.execution.v3";
pub const UPLOAD_STEP_NAME: &str = "Upload measured source-CI custody";
pub fn artifact_name(job: &str, matrix: Option<&str>, attempt: &str) -> io::Result<String> {
    let matrix = matrix.unwrap_or("singleton");
    if [job, matrix].iter().any(|v| !token(v) || v.contains('/'))
        || attempt.is_empty()
        || !attempt.bytes().all(|v| v.is_ascii_digit())
    {
        return Err(io::Error::other("invalid source custody artifact identity"));
    }
    Ok(format!(
        "source-ci-custody-{job}-{matrix}-attempt-{attempt}"
    ))
}

const LOG_LIMIT: u64 = 4 * 1024 * 1024;

fn private_directory(path: &Path, allow_existing: bool) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => Ok(()),
        Err(error) if allow_existing && error.kind() == io::ErrorKind::AlreadyExists => {
            let metadata = path.symlink_metadata()?;
            if !metadata.file_type().is_dir() {
                return Err(io::Error::other(
                    "evidence root is not an owned ordinary directory",
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                unsafe extern "C" {
                    fn geteuid() -> u32;
                }
                if metadata.permissions().mode() & 0o077 != 0
                    || metadata.uid() != unsafe { geteuid() }
                {
                    return Err(io::Error::other(
                        "existing evidence root is not private; refusing to chmod shared state",
                    ));
                }
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}
fn private_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn sha(bytes: &[u8]) -> io::Result<String> {
    hashing::sha256(bytes)
        .map(|s| format!("sha256:{s}"))
        .map_err(io::Error::other)
}
fn file_sha(path: &Path) -> io::Result<String> {
    hashing::file_sha256(path)
        .map(|s| format!("sha256:{s}"))
        .map_err(io::Error::other)
}
fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn oid(value: &str) -> bool {
    immutable_sha(value)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
}
fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}

/// Exact source bytes used to compile this executor, with filenames and byte
/// lengths framed to avoid concatenation ambiguity. Never an expected value.
pub fn profile_sources() -> Vec<(&'static str, &'static [u8])> {
    let files: &[(&str, &[u8])] = &[
        (
            "source_ci_toolchain.rs",
            include_bytes!("source_ci_toolchain.rs"),
        ),
        ("ci_event_context.rs", include_bytes!("ci_event_context.rs")),
        (
            "source_ci_profile.rs",
            include_bytes!("source_ci_profile.rs"),
        ),
        (
            "source_ci_custody.rs",
            include_bytes!("source_ci_custody.rs"),
        ),
        (
            "integration_shards.rs",
            include_bytes!("integration_shards.rs"),
        ),
        (
            "integration_custody.rs",
            include_bytes!("integration_custody.rs"),
        ),
    ];
    files
        .iter()
        .chain(workflow_owned::SOURCES.iter())
        .chain(native_vega::SOURCES.iter())
        .copied()
        .chain(std::iter::once((
            "source_ci_workflow_owned.rs",
            include_bytes!("source_ci_workflow_owned.rs").as_slice(),
        )))
        .collect()
}

/// Preserve v1 profile framing while exposing the complete transitive closure.
pub fn profile_source_path(name: &str) -> PathBuf {
    if name.contains('/') {
        PathBuf::from(name)
    } else {
        Path::new("assets/tools").join(name)
    }
}
pub fn profile_digest() -> io::Result<String> {
    let mut bytes = Vec::new();
    for (name, content) in profile_sources() {
        bytes.extend_from_slice(format!("{name}\0{}\0", content.len()).as_bytes());
        bytes.extend_from_slice(content);
    }
    sha(&bytes)
}

/// Opt-in authority context. The caller independently authenticates this R;
/// these measurements do not turn a directory or a subprocess into a sandbox.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutorContext {
    pub root: PathBuf,
    pub revision: String,
    pub tree: String,
}
impl ExecutorContext {
    pub fn from_flags(flags: &BTreeMap<String, String>) -> io::Result<Option<Self>> {
        let names = [
            "--execution-context",
            "--executor-root",
            "--executor-revision",
            "--executor-tree",
        ];
        if names.iter().all(|key| !flags.contains_key(*key)) {
            return Ok(None);
        }
        if names.iter().any(|key| !flags.contains_key(*key))
            || flags["--execution-context"] != DUAL_ROOT_SCHEMA
            || flags["--executor-root"].is_empty()
            || !oid(&flags["--executor-revision"])
            || !oid(&flags["--executor-tree"])
        {
            return Err(io::Error::other(
                "missing, invalid, or unsupported dual-root execution context",
            ));
        }
        Ok(Some(Self {
            root: flags["--executor-root"].clone().into(),
            revision: flags["--executor-revision"].clone(),
            tree: flags["--executor-tree"].clone(),
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedExecutor {
    pub root: PathBuf,
    pub revision: String,
    pub tree: String,
    pub closure_digest: String,
}
impl VerifiedExecutor {
    /// No machine-local paths appear in the portable proof identity.
    pub fn context_json(&self, subject_revision: &str, subject_tree: &str) -> String {
        format!(
            "{{\"schema\":{},\"executor_revision\":{},\"executor_tree\":{},\"executor_profile_digest\":{},\"subject_revision\":{},\"subject_tree\":{},\"executor_closure_digest\":{}}}",
            json_string(DUAL_ROOT_SCHEMA),
            json_string(&self.revision),
            json_string(&self.tree),
            json_string(&self.closure_digest),
            json_string(subject_revision),
            json_string(subject_tree),
            json_string(&self.closure_digest)
        )
    }
}

/// Verify a separate, complete clean R and measure every embedded source/helper
/// and recipe, including the selector and packaged integration verifier closure.
/// Candidate build files and test programs remain subject inputs under H.
pub fn verify_executor_root(
    repo: &Path,
    executor: &ExecutorContext,
) -> io::Result<VerifiedExecutor> {
    if !oid(&executor.revision)
        || !oid(&executor.tree)
        || !executor.root.symlink_metadata()?.file_type().is_dir()
        || !repo.symlink_metadata()?.file_type().is_dir()
    {
        return Err(io::Error::other(
            "invalid dual-root directory or immutable identity",
        ));
    }
    let root = fs::canonicalize(&executor.root)?;
    let subject = fs::canonicalize(repo)?;
    if root.starts_with(&subject) || subject.starts_with(&root) {
        return Err(io::Error::other(
            "executor and subject roots must be separate, non-nested directories",
        ));
    }
    let actual = ci_context::resolve(&ci_context::Inputs {
        repo: root.clone(),
        event: "workflow_dispatch".into(),
        base: String::new(),
        head: executor.revision.clone(),
        pre_pr: false,
        created: false,
        accepted_base_ref: None,
        snapshot: false,
        trigger_ref: None,
    })
    .map_err(io::Error::other)?;
    if actual.checkout_head != executor.revision || actual.checkout_tree != executor.tree {
        return Err(io::Error::other(
            "executor revision/tree differs from independently reviewed R",
        ));
    }
    let index = immutable_git(&root, &["ls-files", "-v", "-z"])?;
    if !index.status.success()
        || index.stdout.len() > 8 * 1024 * 1024
        || index
            .stdout
            .split(|b| *b == 0)
            .filter(|v| !v.is_empty())
            .any(|v| !v.starts_with(b"H "))
        || !git_value(
            &root,
            &[
                "diff",
                "--cached",
                "--name-only",
                "--no-ext-diff",
                "--no-textconv",
                &executor.revision,
                "--",
            ],
        )?
        .is_empty()
    {
        return Err(io::Error::other(
            "executor requires a complete unchanged ordinary index",
        ));
    }
    let mut measured = Vec::new();
    for (name, expected) in profile_sources() {
        let relative = profile_source_path(name);
        let mut path = root.clone();
        for component in relative.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(io::Error::other("invalid executor closure path"));
            }
            path.push(component);
            if path.symlink_metadata()?.file_type().is_symlink() {
                return Err(io::Error::other(
                    "executor closure cannot traverse symlinks",
                ));
            }
        }
        if !path.symlink_metadata()?.file_type().is_file() {
            return Err(io::Error::other("executor closure requires ordinary files"));
        }
        let mut content = Vec::new();
        File::open(&path)?
            .take(expected.len() as u64 + 1)
            .read_to_end(&mut content)?;
        if content != expected {
            return Err(io::Error::other(format!(
                "executor closure differs from compiled reviewed profile: {}",
                relative.display()
            )));
        }
        // Also prove that the measured bytes belong to the exact captured tree.
        let object = immutable_git(
            &root,
            &[
                "show",
                &format!("{}:{}", executor.revision, relative.display()),
            ],
        )?;
        if !object.status.success() || object.stdout != content {
            return Err(io::Error::other(
                "executor closure differs from captured revision",
            ));
        }
        measured.extend_from_slice(format!("{name}\0{}\0", content.len()).as_bytes());
        measured.extend_from_slice(&content);
    }
    let closure_digest = sha(&measured)?;
    if closure_digest != profile_digest()? {
        return Err(io::Error::other(
            "measured executor closure digest mismatch",
        ));
    }
    Ok(VerifiedExecutor {
        root,
        revision: actual.checkout_head,
        tree: actual.checkout_tree,
        closure_digest,
    })
}

/// Compact UTF-8 JSON array, with field order fixed by invocation_json; no newline.
pub fn recipe_json(recipe: &[Invocation]) -> String {
    format!(
        "[{}]",
        recipe
            .iter()
            .map(invocation_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub fn invocation_digest(recipe: &[Invocation]) -> io::Result<String> {
    sha(recipe_json(recipe).as_bytes())
}

#[derive(Debug)]
pub struct Context {
    pub snapshot: String,
    pub validation_head: String,
    pub tree: String,
    pub provider: String,
    pub run_id: String,
    pub run_attempt: String,
    pub job_id: String,
    pub step_key: String,
    pub expected_profile: String,
    pub expected_invocation: String,
    pub expected_toolchain: Option<String>,
    pub preparation_file: Option<PathBuf>,
    pub evidence_dir: PathBuf,
    pub executor: Option<ExecutorContext>,
}
impl Context {
    pub fn from_flags(flags: &BTreeMap<String, String>) -> io::Result<Self> {
        let required = |key: &str| {
            flags
                .get(key)
                .cloned()
                .ok_or_else(|| io::Error::other(format!("run-evidence requires {key}")))
        };
        Ok(Self {
            snapshot: required("--snapshot")?,
            validation_head: required("--validation-head")?,
            tree: required("--tree")?,
            provider: required("--provider")?,
            run_id: required("--run-id")?,
            run_attempt: required("--run-attempt")?,
            job_id: required("--job-id")?,
            step_key: flags.get("--step-key").cloned().unwrap_or_default(),
            expected_profile: required("--expected-profile-sha256")?,
            expected_invocation: required("--expected-invocation-sha256")?,
            expected_toolchain: flags.get("--expected-toolchain-sha256").cloned(),
            preparation_file: flags.get("--preparation-file").map(PathBuf::from),
            evidence_dir: required("--evidence-dir")?.into(),
            executor: ExecutorContext::from_flags(flags)?,
        })
    }
    fn validate(
        &self,
        check: &Check,
        base: &str,
        head: &str,
        recipe: &[Invocation],
    ) -> io::Result<()> {
        if [
            base,
            head,
            &self.snapshot,
            &self.validation_head,
            &self.tree,
        ]
        .iter()
        .any(|s| !oid(s))
            || base == head
        {
            return Err(io::Error::other(
                "evidence requires distinct immutable B/H and immutable S/tree",
            ));
        }
        if recipe.is_empty() {
            return Err(io::Error::other("required source-CI recipe is empty"));
        }
        if !digest(&self.expected_profile) || self.expected_profile != profile_digest()? {
            return Err(io::Error::other("reviewed source profile digest mismatch"));
        }
        if !digest(&self.expected_invocation)
            || self.expected_invocation != invocation_digest(recipe)?
        {
            return Err(io::Error::other("reviewed invocation digest mismatch"));
        }
        let identity_fields = match self.provider.as_str() {
            "github" => [
                ("provider", true),
                ("run_id", self.run_id.parse::<u64>().is_ok_and(|v| v > 0)),
                (
                    "run_attempt",
                    self.run_attempt.parse::<u32>().is_ok_and(|v| v > 0),
                ),
                ("job_id", self.job_id == check.job),
                ("step_key", self.step_key.is_empty()),
            ],
            // The compatibility runtime exposes the build and logical job,
            // but deliberately withholds native job UUID/step key from shells.
            // This remains an unauthenticated claim until outer importer custody.
            "buildkite_gha" => [
                ("provider", true),
                ("run_id", uuid(&self.run_id)),
                ("run_attempt", self.run_attempt.parse::<u32>().is_ok()),
                ("job_id", self.job_id == check.job),
                ("step_key", self.step_key.is_empty()),
            ],
            "buildkite" => [
                ("provider", true),
                ("run_id", uuid(&self.run_id)),
                ("run_attempt", self.run_attempt.parse::<u32>().is_ok()),
                ("job_id", uuid(&self.job_id)),
                ("step_key", token(&self.step_key)),
            ],
            _ => [
                ("provider", false),
                ("run_id", false),
                ("run_attempt", false),
                ("job_id", false),
                ("step_key", false),
            ],
        };
        if identity_fields.iter().any(|(_, valid)| !valid) {
            // Name malformed fields without exporting provider environment values.
            let fields = identity_fields
                .iter()
                .filter_map(|(name, valid)| (!valid).then_some(*name))
                .collect::<Vec<_>>()
                .join(",");
            return Err(io::Error::other(format!(
                "invalid provider-native execution identity: invalid fields={fields}"
            )));
        }
        Ok(())
    }
}

fn git_value(repo: &Path, args: &[&str]) -> io::Result<String> {
    let out = immutable_git(repo, args)?;
    if !out.status.success() {
        return Err(io::Error::other("immutable snapshot inspection failed"));
    }
    Ok(String::from_utf8(out.stdout)
        .map_err(io::Error::other)?
        .trim()
        .to_string())
}
fn verify_snapshot(repo: &Path, context: &Context, base: &str, head: &str) -> io::Result<String> {
    // Reuse the authoritative verifier for complete checkout, content-filter,
    // hidden-index, and exact ordered B/H merge semantics. Provider transport
    // parent/message authentication remains the independent collector's job.
    let actual = ci_context::resolve(&ci_context::Inputs {
        repo: repo.to_owned(),
        event: "pull_request".into(),
        base: base.into(),
        head: head.into(),
        pre_pr: false,
        created: false,
        accepted_base_ref: None,
        snapshot: false,
        trigger_ref: None,
    })
    .map_err(io::Error::other)?;
    if actual.checkout_head != context.validation_head
        || actual.checkout_tree != context.tree
        || git_value(
            repo,
            &["rev-parse", &format!("{}^{{tree}}", context.snapshot)],
        )? != context.tree
    {
        return Err(io::Error::other(
            "execution revision or S/validation tree differs from reviewed context",
        ));
    }
    // Ordinary PRs run their proven ordered B/H merge, whose tree can contain
    // accepted-base changes absent from H. A transport was normalized from S
    // to H by ci-context, so only that distinct-S case imposes H tree parity.
    if context.snapshot != context.validation_head {
        if context.validation_head != head || actual.head_tree != context.tree {
            return Err(io::Error::other(
                "normalized snapshot must execute exact H/tree",
            ));
        }
        let raw = immutable_git(repo, &["cat-file", "commit", &context.snapshot])?;
        if !raw.status.success() || raw.stdout.len() > 8192 {
            return Err(io::Error::other(
                "transport commit unavailable or oversized",
            ));
        }
        let raw = String::from_utf8(raw.stdout).map_err(io::Error::other)?;
        let (headers, message) = raw
            .split_once("\n\n")
            .ok_or_else(|| io::Error::other("transport commit lacks metadata"))?;
        let parents: Vec<_> = headers
            .lines()
            .filter_map(|line| line.strip_prefix("parent "))
            .collect();
        let metadata = ci_context::SnapshotMetadata::parse(message).map_err(io::Error::other)?;
        if parents != [head] || metadata.source != head || metadata.base != base {
            return Err(io::Error::other(
                "transport does not bind exact original H/B",
            ));
        }
    }
    // The context adapter only imposes this on transport mode; apply it here
    // as well because measured source custody must not hide changed source.
    let index = immutable_git(repo, &["ls-files", "-v", "-z"])?;
    if !index.status.success()
        || index
            .stdout
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
            .any(|e| !e.starts_with(b"H "))
    {
        return Err(io::Error::other(
            "measured source requires a complete ordinary index",
        ));
    }
    Ok(actual.checkout_tree)
}

fn verify_final_revision(
    repo: &Path,
    check: &Check,
    head: &str,
    context: &Context,
    revision: &str,
) -> io::Result<()> {
    let (expected_revision, expected_tree) = if check.id == "docs-drift" {
        // The reviewed docs recipe selects authored H before building. Ordinary
        // PR preparation can start at a different-tree synthetic merge V.
        (
            head,
            git_value(repo, &["rev-parse", &format!("{head}^{{tree}}")])?,
        )
    } else {
        (context.validation_head.as_str(), context.tree.clone())
    };
    if revision != expected_revision
        || git_value(repo, &["rev-parse", &format!("{revision}^{{tree}}")])? != expected_tree
    {
        return Err(io::Error::other(
            "source revision/tree changed outside the finite recipe contract",
        ));
    }
    // A stable commit/tree alone does not prove the tested checkout stayed
    // unchanged: build tools can update a tracked lockfile without moving HEAD.
    for cached in [false, true] {
        let mut args = vec!["diff", "--name-only", "--no-ext-diff", "--no-textconv"];
        if cached {
            args.push("--cached");
        }
        args.extend([expected_revision, "--"]);
        let changed = git_value(repo, &args)?;
        if !changed.is_empty() {
            return Err(io::Error::other(format!(
                "tracked checkout changed during source execution: {changed}"
            )));
        }
    }
    if !git_value(repo, &["ls-files", "--others", "--exclude-standard", "-z"])?.is_empty() {
        return Err(io::Error::other(
            "source execution left undeclared untracked files",
        ));
    }
    Ok(())
}

/// Preserve the actual executable path (not a canonicalized rustup argv[0]).
fn resolve(program: &str, cwd: &Path) -> io::Result<PathBuf> {
    let candidates = if program.contains('/') {
        vec![cwd.join(program)]
    } else {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|p| {
                if p.is_absolute() {
                    p.join(program)
                } else {
                    cwd.join(p).join(program)
                }
            })
            .collect()
    };
    for path in candidates {
        if path.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if path.metadata()?.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            return Ok(path);
        }
    }
    Err(io::Error::other(format!(
        "cannot resolve executable {program}"
    )))
}

struct Log {
    path: String,
    total: u64,
    stored: u64,
    full_sha256: String,
    stored_sha256: String,
}
impl Log {
    fn json(&self) -> String {
        format!(
            "{{\"path\":{},\"bytes\":{},\"stored_bytes\":{},\"truncated\":{},\"sha256\":{},\"stored_sha256\":{}}}",
            json_string(&self.path),
            self.total,
            self.stored,
            self.total != self.stored,
            json_string(&self.full_sha256),
            json_string(&self.stored_sha256)
        )
    }
}
struct Tee<R> {
    source: R,
    file: File,
    total: u64,
    stored: u64,
    limit: u64,
    echo: bool,
}
impl<R: Read> Read for Tee<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.source.read(buf)?;
        self.total = self
            .total
            .checked_add(n as u64)
            .ok_or_else(|| io::Error::other("log length overflow"))?;
        let retain = n.min(self.limit.saturating_sub(self.stored) as usize);
        self.file.write_all(&buf[..retain])?;
        self.stored += retain as u64;
        if self.echo {
            io::stderr().lock().write_all(&buf[..n])?;
        }
        Ok(n)
    }
}
fn drain(source: impl Read, dir: &Path, name: &str, limit: u64, echo: bool) -> io::Result<Log> {
    let path = dir.join(name);
    let file = private_file(&path)?;
    let mut tee = Tee {
        source,
        file,
        total: 0,
        stored: 0,
        limit,
        echo,
    };
    let hash = hashing::sha256(&mut tee).map_err(io::Error::other)?;
    tee.file.sync_all()?;
    Ok(Log {
        path: name.into(),
        total: tee.total,
        stored: tee.stored,
        full_sha256: format!("sha256:{hash}"),
        stored_sha256: file_sha(&path)?,
    })
}
struct Execution {
    status: ExitStatus,
    stdout: Log,
    stderr: Log,
}
fn execute(mut command: Command, dir: &Path, prefix: &str, echo: bool) -> io::Result<Execution> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out_dir = dir.to_owned();
    let err_dir = dir.to_owned();
    let out_name = format!("{prefix}.stdout.log");
    let err_name = format!("{prefix}.stderr.log");
    // Drain both streams simultaneously, keeping memory/disk bounded without
    // truncating what the child can write or replacing its ExitStatus.
    let out = thread::spawn(move || drain(stdout, &out_dir, &out_name, LOG_LIMIT, echo));
    let err = thread::spawn(move || drain(stderr, &err_dir, &err_name, LOG_LIMIT, echo));
    let status = child.wait()?;
    let stdout = out
        .join()
        .map_err(|_| io::Error::other("stdout custody worker failed"))??;
    let stderr = err
        .join()
        .map_err(|_| io::Error::other("stderr custody worker failed"))??;
    Ok(Execution {
        status,
        stdout,
        stderr,
    })
}
fn exit_json(status: ExitStatus) -> String {
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    };
    #[cfg(not(unix))]
    let signal: Option<i32> = None;
    format!(
        "\"exit_code\":{},\"signal\":{},\"success\":{}",
        status
            .code()
            .map_or_else(|| "null".into(), |v| v.to_string()),
        signal.map_or_else(|| "null".into(), |v| v.to_string()),
        status.success()
    )
}
/// Resolve reviewed shell and native helper source operands against R. The
/// fixed standalone proof-driver compilation also runs under R; candidate
/// Cargo/Bazel builds and every produced artifact remain under H.
pub fn configured_arguments(
    invocation: &Invocation,
    executor_root: &Path,
) -> Vec<std::ffi::OsString> {
    invocation
        .args
        .iter()
        .map(|arg| {
            if (invocation.program == "bash"
                && workflow_owned::SOURCES
                    .iter()
                    .any(|(path, _)| path == arg && path.ends_with(".sh")))
                || native_vega::SOURCES.iter().any(|(path, _)| path == arg)
            {
                executor_root.join(arg).into_os_string()
            } else {
                std::ffi::OsString::from(*arg)
            }
        })
        .collect()
}
pub fn configured_arguments_for_subject(
    invocation: &Invocation,
    executor_root: &Path,
    subject: &Path,
) -> Vec<std::ffi::OsString> {
    let mut args = configured_arguments(invocation, executor_root);
    if native_vega::is_driver_build(invocation) {
        for index in 1..invocation.args.len() {
            if invocation.args[index - 1] == "--target-dir" {
                args[index] = subject.join(invocation.args[index]).into_os_string();
            }
        }
    }
    args
}
fn configured(
    invocation: &Invocation,
    repo: &Path,
    executable: &Path,
    bindings: &BTreeMap<String, String>,
    executor_root: &Path,
    profile_executable: &Path,
) -> Command {
    let mut command = Command::new(executable);
    command.envs(bindings);
    command
        .args(configured_arguments_for_subject(
            invocation,
            executor_root,
            repo,
        ))
        .env("SOURCE_CI_EXECUTOR_ROOT", executor_root)
        .env("SOURCE_CI_PROFILE_EXECUTABLE", profile_executable)
        .current_dir(if native_vega::is_driver_build(invocation) {
            executor_root.to_path_buf()
        } else {
            repo.join(invocation.cwd)
        });
    for (key, value) in &invocation.env {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command
}
fn new_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = private_file(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

fn measure_tools(
    check: &Check,
    recipe: &[Invocation],
    repo: &Path,
    dir: &Path,
    prefix: &str,
    executor_root: &Path,
) -> io::Result<(String, String, bool)> {
    // Resolve and bind the selected toolchain first. Bazelisk may initialize its
    // pinned cache on this probe; preserve that cold-start diagnostic in the
    // declared probe logs before capturing the stable launcher version output.
    let declared = declared_toolchain::measure_declared(check, recipe, repo, dir, prefix)?;
    let mut tools = Vec::new();
    let mut logs = Vec::new();
    let mut logs_complete = true;
    let mut programs = BTreeSet::new();
    for capability in check.capabilities {
        let program = capability.split(':').next().unwrap();
        if ![
            "docker-daemon",
            "image",
            "rust-target",
            "cross-rev",
            "qemu-aarch64-in-pinned-image",
            "submodules",
        ]
        .contains(&program)
        {
            programs.insert(program);
        }
    }
    for invocation in recipe {
        if !invocation.program.contains('/') {
            programs.insert(invocation.program);
        }
    }
    for (index, program) in programs.into_iter().enumerate() {
        let executable = resolve(program, repo)?;
        let executable_digest = file_sha(&executable)?;
        let mut probe = Command::new(&executable);
        let version_arg = if program == "ajv" {
            "help"
        } else {
            "--version"
        };
        probe.arg(version_arg).current_dir(repo);
        let outcome = execute(probe, dir, &format!("{prefix}tool-{index}"), false)?;
        logs_complete &= outcome.stdout.total == outcome.stdout.stored
            && outcome.stderr.total == outcome.stderr.stored;
        tools.push(format!("{{\"program\":{},\"executable_sha256\":{},\"version_args\":[{}],{},\"stdout_sha256\":{},\"stdout_bytes\":{},\"stderr_sha256\":{},\"stderr_bytes\":{}}}", json_string(program), json_string(&executable_digest), json_string(version_arg), exit_json(outcome.status), json_string(&outcome.stdout.full_sha256), outcome.stdout.total, json_string(&outcome.stderr.full_sha256), outcome.stderr.total));
        logs.push(format!(
            "{{\"program\":{},\"executable\":{},\"stdout\":{},\"stderr\":{}}}",
            json_string(program),
            json_string(&executable.to_string_lossy()),
            outcome.stdout.json(),
            outcome.stderr.json()
        ));
    }
    tools.extend(declared.0);
    logs.extend(declared.1);
    logs_complete &= declared.2;
    for path in workflow_owned::input_files(check, repo)
        .into_iter()
        .chain(native_vega::input_files(check))
    {
        let file = repo.join(&path);
        if !file.is_file() {
            return Err(io::Error::other(format!(
                "missing required custody input: {path}"
            )));
        }
        tools.push(format!(
            "{{\"kind\":\"input_artifact\",\"path\":{},\"bytes\":{},\"sha256\":{}}}",
            json_string(&path),
            file.metadata()?.len(),
            json_string(&file_sha(&file)?)
        ));
    }
    for (path, _) in workflow_owned::script_sources(check)
        .into_iter()
        .chain(native_vega::source_files(check))
    {
        let file = executor_root.join(path);
        tools.push(format!(
            "{{\"kind\":\"source_recipe\",\"path\":{},\"sha256\":{}}}",
            json_string(path),
            json_string(&file_sha(&file)?)
        ));
    }
    Ok((
        format!("[{}]", tools.join(",")),
        format!("[{}]", logs.join(",")),
        logs_complete,
    ))
}

fn verify_recipe_files(repo: &Path, check: &Check) -> io::Result<()> {
    for (path, expected) in workflow_owned::script_sources(check)
        .into_iter()
        .chain(native_vega::source_files(check))
    {
        let actual = repo.join(path);
        if !actual.symlink_metadata()?.file_type().is_file() || file_sha(&actual)? != sha(expected)?
        {
            return Err(io::Error::other(format!(
                "reviewed fixed source recipe differs: {path}"
            )));
        }
    }
    Ok(())
}
fn artifact_context(
    check: &Check,
    base: &str,
    head: &str,
    context: &Context,
) -> io::Result<hashing::Context> {
    let input = hashing::Context::from_env().map_err(io::Error::other)?;
    if input.base != base
        || input.head != head
        || input.validation_head != context.validation_head
        || input.tree != context.tree
        || (check.job == "test-integration"
            && !input
                .shards
                .iter()
                .any(|s| Some(s.as_str()) == check.matrix))
    {
        return Err(io::Error::other(
            "integration artifact context/selection differs from required source identity",
        ));
    }
    Ok(input)
}
fn context_bindings(
    repo: &Path,
    check: &Check,
    base: &str,
    head: &str,
    context: &Context,
) -> io::Result<BTreeMap<String, String>> {
    let artifact = workflow_owned::artifact_worker(check);
    let event = if artifact {
        "pull_request".to_string()
    } else {
        env::var("SOURCE_CI_EVENT").unwrap_or_else(|_| "pull_request".into())
    };
    if !["pull_request", "push", "workflow_dispatch"].contains(&event.as_str()) {
        return Err(io::Error::other("unsupported source event"));
    }
    let merge_base = if artifact {
        artifact_context(check, base, head, context)?.merge_base
    } else {
        git_value(repo, &["merge-base", "--all", base, head])?
    };
    if !oid(&merge_base) {
        return Err(io::Error::other("ambiguous immutable merge base"));
    }
    let release_only = !artifact && workflow_owned::release_only(repo, &event, base, head);
    let mut values = BTreeMap::from([
        ("SOURCE_CI_EVENT".into(), event.clone()),
        ("SOURCE_CI_BASE".into(), base.into()),
        ("SOURCE_CI_HEAD".into(), head.into()),
        ("SOURCE_CI_MERGE_BASE".into(), merge_base.clone()),
        ("SOURCE_CI_RELEASE_ONLY".into(), release_only.to_string()),
    ]);
    if check.id.starts_with("integration-") {
        values.extend([
            ("CHANGE_BASE".into(), base.into()),
            ("CHANGE_HEAD".into(), head.into()),
            ("CUSTODY_EVENT".into(), event),
            ("CUSTODY_BASE".into(), base.into()),
            ("CUSTODY_HEAD".into(), head.into()),
            ("CUSTODY_MERGE_BASE".into(), merge_base),
            (
                "CUSTODY_VALIDATION_HEAD".into(),
                context.validation_head.clone(),
            ),
            ("CUSTODY_TREE".into(), context.tree.clone()),
        ]);
    }
    if let Some(shard) = check.matrix.filter(|_| check.job == "test-integration") {
        values.insert("CUSTODY_SHARD".into(), shard.into());
    }
    for key in [
        "CUSTODY_SHARDS",
        "CUSTODY_ARCHIVE_SHA256",
        "CUSTODY_HELPER_SHA256",
        "CUSTODY_PREPARED_CONTEXT",
        "CUSTODY_COUNT",
        "CUSTODY_PREPARE_RESULT",
        "CUSTODY_WORKER_RESULT",
    ] {
        if let Ok(value) = env::var(key) {
            if value.len() > 4096 || value.contains(['\n', '\r', '\0']) {
                return Err(io::Error::other("invalid integration context value"));
            }
            values.insert(key.into(), value);
        }
    }
    Ok(values)
}
fn bindings_json(values: &BTreeMap<String, String>) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("{}:{}", json_string(key), json_string(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Exact finite produced-file inventory, shared with the independent runtime.
pub fn required_artifact_paths(check: &Check, _paths: &[String]) -> Vec<String> {
    let artifact_defaults: &[&str] = match check.id {
        "setup-bazel" => &["bazel-bin/decapod", "bazel-bin/release_projection_drift"],
        "musl-build-x86_64" => &["target/x86_64-unknown-linux-musl/release/decapod"],
        "musl-build-aarch64" => &["target/aarch64-unknown-linux-musl/release/decapod"],
        "golden-vectors-linux" | "golden-vectors-macos" => {
            &["tests/golden_vectors/target/release/golden_vectors"]
        }
        "docs-build" => &["docs/book/book/index.html"],
        _ => &[],
    };
    let mut artifact_paths = artifact_defaults.to_vec();
    match check.id {
        "integration-prepare" => {
            artifact_paths.extend([
                "target/integration-custody-artifacts/integration-custody",
                "target/integration-custody-artifacts/preparation.v1",
            ]);
        }
        "health" => artifact_paths.push("target/source-ci-health/governance.json"),
        "standalone-validation" => artifact_paths.extend([
            "target/source-ci-standalone/decapod",
            "target/source-ci-standalone/decapod-release-projection-drift",
            "target/source-ci-standalone/governance.json",
        ]),
        _ => (),
    }
    let receipt = check
        .matrix
        .filter(|_| check.job == "test-integration")
        .map(|shard| format!("target/integration-receipts/{shard}.receipt"));
    if let Some(receipt) = &receipt {
        artifact_paths.push(receipt);
    }
    artifact_paths
        .into_iter()
        .map(str::to_string)
        .chain(native_vega::artifact_paths(check))
        .collect()
}

pub fn run(
    repo: &Path,
    check: &Check,
    paths: &[String],
    base: &str,
    head: &str,
    context: &Context,
    prepare_only: bool,
) -> io::Result<bool> {
    let start = Instant::now();
    let recipe = commands(check, paths)?;
    context.validate(check, base, head, &recipe)?;
    if !prepare_only
        && (!context.expected_toolchain.as_deref().is_some_and(digest)
            || context.preparation_file.is_none())
    {
        return Err(io::Error::other(
            "run-evidence requires independently frozen --expected-toolchain-sha256 and --preparation-file",
        ));
    }
    let executor = context
        .executor
        .as_ref()
        .map(|value| verify_executor_root(repo, value))
        .transpose()?;
    let repo = fs::canonicalize(repo)?;
    let candidate_tree = if workflow_owned::artifact_worker(check) {
        artifact_context(check, base, head, context)?.tree
    } else {
        verify_snapshot(&repo, context, base, head)?
    };
    let executor_root = executor
        .as_ref()
        .map_or(repo.as_path(), |value| value.root.as_path());
    let execution_context = executor
        .as_ref()
        .map(|value| value.context_json(&context.validation_head, &context.tree));
    let context_field = execution_context.as_ref().map_or(String::new(), |value| {
        format!(",\"execution_context\":{value}")
    });
    let profile_executable = env::current_exe()?;
    verify_recipe_files(executor_root, check)?;
    let bindings = context_bindings(&repo, check, base, head, context)?;
    let context_json = bindings_json(&bindings);
    // Unique directories prevent overwriting or confusing an earlier attempt.
    private_directory(&context.evidence_dir, false)?;
    let dir = fs::canonicalize(&context.evidence_dir)?;
    if requires_private_umask(check) {
        private_umask();
    }
    let mut results = Vec::new();
    let mut toolchain_json = "[]".to_string();
    let mut failure = None;
    let mut status = "passed";
    let mut logs_complete = true;
    let preparation = (|| -> io::Result<()> {
        preflight(check)?;
        native_vega::preparation(&repo, executor_root, check)?;
        if check.id.starts_with("golden-vectors-") {
            golden_preflight(&repo)?;
        }
        let measured = measure_tools(check, &recipe, &repo, &dir, "", executor_root)?;
        toolchain_json = measured.0;
        new_file(&dir.join("tool-logs.json"), measured.1.as_bytes())?;
        logs_complete &= measured.2;
        Ok(())
    })();
    if let Err(error) = preparation {
        status = "unavailable";
        failure = Some(error.to_string());
    }
    new_file(&dir.join("toolchain.json"), toolchain_json.as_bytes())?;
    new_file(
        &dir.join("invocations.json"),
        recipe_json(&recipe).as_bytes(),
    )?;
    let toolchain_digest = sha(toolchain_json.as_bytes())?;
    new_file(&dir.join("context.json"), context_json.as_bytes())?;
    if let Some(expected) = &executor {
        let after = verify_executor_root(&repo, context.executor.as_ref().unwrap())?;
        if &after != expected {
            return Err(io::Error::other("executor changed during preparation"));
        }
    }
    let preparation_manifest = format!(
        "{{\"schema\":{},\"authenticated\":false,\"complete_pr_profile\":false,\"preparation_complete\":{},\"workflow\":{},\"job\":{},\"check\":{},\"matrix\":{},\"base\":{},\"head\":{},\"snapshot\":{},\"execution_revision\":{},\"tree\":{},\"provider\":{},\"run_id\":{},\"run_attempt\":{},\"job_id\":{},\"step_key\":{},\"profile_digest\":{},\"invocation_digest\":{},\"toolchain_digest\":{},\"context_digest\":{},\"executor_digest\":{},\"platform\":{}{}}}\n",
        json_string(if executor.is_some() {
            DUAL_ROOT_PREPARATION_SCHEMA
        } else {
            "decapod.source-ci.preparation.v1"
        }),
        status == "passed" && logs_complete,
        json_string(check.workflow),
        json_string(check.job),
        json_string(check.id),
        check.matrix.map_or_else(|| "null".into(), json_string),
        json_string(base),
        json_string(head),
        json_string(&context.snapshot),
        json_string(&context.validation_head),
        json_string(&context.tree),
        json_string(&context.provider),
        json_string(&context.run_id),
        json_string(&context.run_attempt),
        json_string(&context.job_id),
        json_string(&context.step_key),
        json_string(&profile_digest()?),
        json_string(&invocation_digest(&recipe)?),
        json_string(&toolchain_digest),
        json_string(&sha(context_json.as_bytes())?),
        json_string(&file_sha(&profile_executable)?),
        json_string(check.platform.name()),
        context_field
    );
    new_file(
        &dir.join("preparation.json"),
        preparation_manifest.as_bytes(),
    )?;
    if prepare_only {
        print!("{preparation_manifest}");
        if let Some(error) = &failure {
            eprintln!(
                "source-ci-profile: preparation failed for {}: {error}",
                check.id
            );
        } else if !logs_complete {
            eprintln!(
                "source-ci-profile: preparation logs incomplete for {}",
                check.id
            );
        }
        return Ok(status == "passed" && logs_complete);
    }
    // Bind the exact pre-command manifest as the first record in the chain.
    // Its authenticated provider upload and reviewed action ordering, not this
    // local hash alone, establish preparation preceded substantive execution.
    let mut predecessor = Vec::new();
    File::open(context.preparation_file.as_ref().unwrap())?
        .take(64 * 1024 + 1)
        .read_to_end(&mut predecessor)?;
    if predecessor.len() > 64 * 1024 || predecessor != preparation_manifest.as_bytes() {
        return Err(io::Error::other(
            "frozen preparation context/toolchain differs from current measurement",
        ));
    }
    new_file(&dir.join("frozen-preparation.json"), &predecessor)?;
    let preparation_digest = sha(&predecessor)?;
    let mut previous_record = preparation_digest.clone();
    if status == "passed"
        && context.expected_toolchain.as_deref() != Some(toolchain_digest.as_str())
    {
        status = "unavailable";
        failure = Some("measured toolchain differs from independently frozen preparation".into());
    }
    if status == "passed" {
        for (index, invocation) in recipe.iter().enumerate() {
            let stage = Instant::now();
            let outcome = (|| -> io::Result<(PathBuf, String, Execution)> {
                if let Some(expected) = &executor
                    && verify_executor_root(&repo, context.executor.as_ref().unwrap())? != *expected
                {
                    return Err(io::Error::other("executor changed before source command"));
                }
                let executable = resolve(invocation.program, &repo.join(invocation.cwd))?;
                let hash = file_sha(&executable)?;
                let out = execute(
                    configured(
                        invocation,
                        &repo,
                        &executable,
                        &bindings,
                        executor_root,
                        &profile_executable,
                    ),
                    &dir,
                    &format!("command-{index}"),
                    true,
                )?;
                Ok((executable, hash, out))
            })();
            match outcome {
                Ok((executable, hash, out)) => {
                    logs_complete &= out.stdout.total == out.stdout.stored
                        && out.stderr.total == out.stderr.stored;
                    // Record the real exit first, even if semantic postflight fails.
                    let record = format!(
                        "{{\"previous_record_sha256\":{},\"invocation\":{},\"executable\":{},\"executable_sha256\":{},{},\"elapsed_ms\":{},\"stdout\":{},\"stderr\":{}}}",
                        json_string(&previous_record),
                        invocation_json(invocation),
                        json_string(&executable.to_string_lossy()),
                        json_string(&hash),
                        exit_json(out.status),
                        stage.elapsed().as_millis(),
                        out.stdout.json(),
                        out.stderr.json()
                    );
                    previous_record = sha(record.as_bytes())?;
                    results.push(record);
                    if !out.status.success() {
                        status = "failed";
                        break;
                    }
                    if invocation.program == "./target/release/golden_vectors" {
                        let mut bytes = fs::read(dir.join(&out.stdout.path))?;
                        bytes.extend(fs::read(dir.join(&out.stderr.path))?);
                        if let Err(error) = golden_output_check(&bytes) {
                            status = "failed";
                            failure = Some(error.to_string());
                            break;
                        }
                    }
                }
                Err(error) => {
                    status = "failed";
                    failure = Some(error.to_string());
                    break;
                }
            }
        }
        if status == "passed"
            && check.id.starts_with("golden-vectors-")
            && let Err(error) = golden_postflight(&repo)
        {
            status = "failed";
            failure = Some(error.to_string());
        }
    }
    if !results.is_empty() {
        match measure_tools(check, &recipe, &repo, &dir, "after-", executor_root) {
            Ok((after, logs, complete)) => {
                new_file(&dir.join("after-toolchain.json"), after.as_bytes())?;
                new_file(&dir.join("after-tool-logs.json"), logs.as_bytes())?;
                logs_complete &= complete;
                if after != toolchain_json {
                    status = "failed";
                    failure = Some("toolchain changed during source execution".into());
                }
            }
            Err(error) => {
                status = "failed";
                failure = Some(format!("cannot verify post-execution toolchain: {error}"));
            }
        }
    }
    let artifact_paths = required_artifact_paths(check, paths);
    let mut artifacts = Vec::new();
    if status == "passed" {
        for (index, path) in artifact_paths.iter().enumerate() {
            let original = repo.join(path);
            if !original.is_file() {
                status = "failed";
                failure = Some(format!("expected produced artifact missing: {path}"));
                break;
            }
            let destination = format!("artifact-{index}");
            let copy = dir.join(&destination);
            let mut input = File::open(&original)?;
            let mut output = private_file(&copy)?;
            io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            artifacts.push(format!(
                "{{\"source_path\":{},\"path\":{},\"bytes\":{},\"sha256\":{}}}",
                json_string(path),
                json_string(&destination),
                copy.metadata()?.len(),
                json_string(&file_sha(&copy)?)
            ));
        }
    }
    let final_revision = if workflow_owned::artifact_worker(check) {
        None
    } else {
        Some(git_value(&repo, &["rev-parse", "HEAD"])?)
    };
    if status == "passed"
        && let Some(revision) = &final_revision
        && let Err(error) = verify_final_revision(&repo, check, head, context, revision)
    {
        status = "failed";
        failure = Some(error.to_string());
    }
    let mut final_executor_closure = None;
    if let Some(expected) = &executor {
        match verify_executor_root(&repo, context.executor.as_ref().unwrap()) {
            Ok(after) if &after == expected => final_executor_closure = Some(after.closure_digest),
            Ok(_) => {
                status = "failed";
                failure = Some("executor identity changed during source execution".into());
            }
            Err(error) => {
                status = "failed";
                failure = Some(format!(
                    "cannot verify post-execution executor closure: {error}"
                ));
            }
        }
    }
    let final_context_field = if executor.is_some() {
        format!(
            "{context_field},\"final_executor_closure_digest\":{}",
            final_executor_closure
                .as_deref()
                .map_or_else(|| "null".into(), json_string)
        )
    } else {
        String::new()
    };
    let complete = status == "passed" && logs_complete && results.len() == recipe.len();
    let manifest = format!(
        "{{\"schema\":{},\"complete_pr_profile\":false,\"authenticated\":false,\"toolchain_scope\":\"declared executable, compiler, sysroot, configuration and input closure; provider policy remains mandatory\",\"execution_complete\":{},\"status\":{},\"workflow\":{},\"job\":{},\"check\":{},\"matrix\":{},\"base\":{},\"head\":{},\"execution_revision\":{},\"final_execution_revision\":{},\"candidate_tree\":{},\"snapshot\":{},\"snapshot_tree\":{},\"provider\":{},\"run_id\":{},\"run_attempt\":{},\"job_id\":{},\"step_key\":{},\"profile_digest\":{},\"invocation_digest\":{},\"toolchain_digest\":{},\"preparation_digest\":{},\"terminal_record_sha256\":{},\"platform\":{},\"logs_complete\":{},\"context_digest\":{},\"execution_mode\":{},\"results\":[{}],\"artifacts\":[{}],\"error\":{},\"elapsed_ms\":{}{}}}\n",
        json_string(if executor.is_some() {
            DUAL_ROOT_EXECUTION_SCHEMA
        } else {
            SCHEMA
        }),
        complete,
        json_string(status),
        json_string(check.workflow),
        json_string(check.job),
        json_string(check.id),
        check.matrix.map_or_else(|| "null".into(), json_string),
        json_string(base),
        json_string(head),
        json_string(&context.validation_head),
        final_revision
            .as_deref()
            .map_or_else(|| "null".into(), json_string),
        json_string(&candidate_tree),
        json_string(&context.snapshot),
        json_string(&context.tree),
        json_string(&context.provider),
        json_string(&context.run_id),
        json_string(&context.run_attempt),
        json_string(&context.job_id),
        json_string(&context.step_key),
        json_string(&profile_digest()?),
        json_string(&invocation_digest(&recipe)?),
        json_string(&sha(toolchain_json.as_bytes())?),
        json_string(&preparation_digest),
        json_string(&previous_record),
        json_string(check.platform.name()),
        logs_complete,
        json_string(&sha(context_json.as_bytes())?),
        json_string(if workflow_owned::artifact_worker(check) {
            "authenticated_integration_artifacts"
        } else {
            "source_checkout"
        }),
        results.join(","),
        artifacts.join(","),
        failure
            .as_deref()
            .map_or_else(|| "null".into(), json_string),
        start.elapsed().as_millis(),
        final_context_field
    );
    new_file(&dir.join("execution.json"), manifest.as_bytes())?;
    print!("{manifest}");
    Ok(complete)
}

/// Workflow convenience entrypoint: freeze expectations from this reviewed
/// source catalog before preparation or check commands. Provider collection must
/// independently compare this compiled definition to the captured source plan.
// buildkite-gha v0.102.2 internal/runtime/runtime.go RunIdentity.githubValues
// maps native retry N to GITHUB_RUN_ATTEMPT=N+1. No absent/zero/overflow default.
fn imported_native_retry(mapped_attempt: &str) -> io::Result<String> {
    mapped_attempt
        .parse::<u32>()
        .ok()
        .filter(|value| value.to_string() == mapped_attempt)
        .and_then(|value| value.checked_sub(1))
        .map(|value| value.to_string())
        .ok_or_else(|| io::Error::other("invalid imported runtime attempt mapping"))
}

pub fn run_ci(
    repo: &Path,
    check: &Check,
    supplied_base: Option<&str>,
    supplied_head: Option<&str>,
) -> io::Result<bool> {
    run_ci_with_executor(repo, check, supplied_base, supplied_head, None)
}

pub fn run_ci_with_executor(
    repo: &Path,
    check: &Check,
    supplied_base: Option<&str>,
    supplied_head: Option<&str>,
    executor: Option<ExecutorContext>,
) -> io::Result<bool> {
    let variable = |name: &str, fallback: &str| -> io::Result<String> {
        env::var(name)
            .or_else(|_| env::var(fallback))
            .map_err(|_| io::Error::other(format!("missing verified {name}")))
    };
    let base = variable("SOURCE_CI_BASE", "CUSTODY_BASE")?;
    let head = variable("SOURCE_CI_HEAD", "CUSTODY_HEAD")?;
    if supplied_base.is_some_and(|value| value != base)
        || supplied_head.is_some_and(|value| value != head)
    {
        return Err(io::Error::other(
            "caller range differs from verified CI context",
        ));
    }
    // Ordinary manual dispatch is explicitly outside PR readiness. Preserve its
    // existing partial source-check behavior; never issue an execution receipt.
    if base.is_empty() && env::var("SOURCE_CI_EVENT").as_deref() == Ok("workflow_dispatch") {
        if executor.is_some() {
            return Err(io::Error::other(
                "dual-root execution requires immutable subject context",
            ));
        }
        return run_check(repo, check, &[], &base, &head);
    }
    let artifact = workflow_owned::artifact_worker(check);
    let paths = if artifact {
        vec![]
    } else {
        changed_paths(repo, &base, &head)?
    };
    let recipe = commands(check, &paths)?;
    let buildkite = env::var("BUILDKITE_BUILD_ID")
        .ok()
        .filter(|value| !value.is_empty());
    let (provider, run_id, run_attempt, job_id, step_key) = if let Some(build_id) = buildkite {
        (
            "buildkite".into(),
            build_id,
            env::var("BUILDKITE_RETRY_COUNT").unwrap_or_else(|_| "0".into()),
            variable("BUILDKITE_JOB_ID", "BUILDKITE_JOB_ID")?,
            variable("BUILDKITE_STEP_KEY", "BUILDKITE_STEP_KEY")?,
        )
    } else {
        let run_id = variable("GITHUB_RUN_ID", "GITHUB_RUN_ID")?;
        let attempt = variable("GITHUB_RUN_ATTEMPT", "GITHUB_RUN_ATTEMPT")?;
        let logical_job = variable("GITHUB_JOB", "GITHUB_JOB")?;
        if uuid(&run_id) {
            // Shape selects an honest imported-shell claim, never authority.
            // Native UUID/key are authenticated only by the external collector.
            (
                "buildkite_gha".into(),
                run_id,
                imported_native_retry(&attempt)?,
                logical_job,
                String::new(),
            )
        } else {
            ("github".into(), run_id, attempt, logical_job, String::new())
        }
    };
    // Export the actual native-attempt identity, rather than asking imported
    // workflow expression contexts to infer which provider env is available.
    if let Some(path) = env::var_os("GITHUB_ENV") {
        let name = artifact_name(check.job, check.matrix, &run_attempt)?;
        writeln!(
            OpenOptions::new().append(true).open(path)?,
            "SOURCE_CI_CUSTODY_ARTIFACT={name}"
        )?;
    }
    let root = PathBuf::from(variable("RUNNER_TEMP", "RUNNER_TEMP")?).join("source-ci-evidence");
    private_directory(&root, true)?;
    let check_dir = root.join(check.id);
    private_directory(&check_dir, false)?;
    let mut context = Context {
        snapshot: variable("SOURCE_CI_SNAPSHOT", "GITHUB_SHA")?,
        validation_head: variable("SOURCE_CI_VALIDATION_HEAD", "CUSTODY_VALIDATION_HEAD")?,
        tree: variable("SOURCE_CI_TREE", "CUSTODY_TREE")?,
        provider,
        run_id,
        run_attempt,
        job_id,
        step_key,
        expected_profile: profile_digest()?,
        expected_invocation: invocation_digest(&recipe)?,
        expected_toolchain: None,
        preparation_file: None,
        evidence_dir: check_dir.join("preparation"),
        executor,
    };
    if !artifact && !selected(check, &paths, false) {
        let tree = verify_snapshot(repo, &context, &base, &head)?;
        let measured = context
            .executor
            .as_ref()
            .map(|value| verify_executor_root(repo, value))
            .transpose()?;
        let extra = measured.as_ref().map_or(String::new(), |value| {
            format!(
                ",\"execution_context\":{}",
                value.context_json(&context.validation_head, &tree)
            )
        });
        // Context validation deliberately rejects empty required recipes. This
        // separate receipt proves the selector result and claims no execution.
        let selection = format!(
            "{{\"schema\":{},\"status\":\"not_applicable\",\"authenticated\":false,\"complete_pr_profile\":false,\"check\":{},\"workflow\":{},\"job\":{},\"matrix\":{},\"base\":{},\"head\":{},\"snapshot\":{},\"execution_revision\":{},\"tree\":{},\"profile_digest\":{},\"provider\":{},\"run_id\":{},\"run_attempt\":{},\"job_id\":{},\"step_key\":{},\"changed_paths\":{}{}}}\n",
            json_string(if measured.is_some() {
                "decapod.source-ci.selection.v2"
            } else {
                "decapod.source-ci.selection.v1"
            }),
            json_string(check.id),
            json_string(check.workflow),
            json_string(check.job),
            check.matrix.map_or_else(|| "null".into(), json_string),
            json_string(&base),
            json_string(&head),
            json_string(&context.snapshot),
            json_string(&context.validation_head),
            json_string(&tree),
            json_string(&context.expected_profile),
            json_string(&context.provider),
            json_string(&context.run_id),
            json_string(&context.run_attempt),
            json_string(&context.job_id),
            json_string(&context.step_key),
            json_strings(&paths),
            extra
        );
        new_file(&check_dir.join("selection.json"), selection.as_bytes())?;
        print!("{selection}");
        return Ok(true);
    }
    if !run(repo, check, &paths, &base, &head, &context, true)? {
        return Ok(false);
    }
    // This frozen pre-command closure is never selected from green results.
    context.expected_toolchain = Some(file_sha(&context.evidence_dir.join("toolchain.json"))?);
    context.preparation_file = Some(context.evidence_dir.join("preparation.json"));
    context.evidence_dir = check_dir.join("execution");
    run(repo, check, &paths, &base, &head, &context, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    fn temp() -> PathBuf {
        let path = env::temp_dir().join(format!(
            "source-custody-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    fn context(recipe: &[Invocation]) -> Context {
        Context {
            snapshot: "3".repeat(40),
            validation_head: "2".repeat(40),
            tree: "4".repeat(40),
            provider: "github".into(),
            run_id: "12".into(),
            run_attempt: "1".into(),
            job_id: "fmt".into(),
            step_key: String::new(),
            expected_profile: profile_digest().unwrap(),
            expected_invocation: invocation_digest(recipe).unwrap(),
            expected_toolchain: None,
            preparation_file: None,
            evidence_dir: PathBuf::from("unused"),
            executor: None,
        }
    }
    #[test]
    fn reviewed_digests_and_nonempty_recipe_are_mandatory() {
        let check = find_check("fmt").unwrap();
        let recipe = commands(check, &[]).unwrap();
        let mut ctx = context(&recipe);
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_ok()
        );
        ctx.expected_invocation = format!("sha256:{}", "0".repeat(64));
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_err()
        );
        assert!(
            context(&[])
                .validate(check, &"1".repeat(40), &"2".repeat(40), &[])
                .is_err()
        );
    }
    #[test]
    fn provider_namespaces_are_not_interchangeable() {
        let check = find_check("fmt").unwrap();
        let recipe = commands(check, &[]).unwrap();
        let mut ctx = context(&recipe);
        ctx.provider = "buildkite".into();
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_err()
        );
        ctx.run_id = "12345678-abcd-abcd-abcd-123456789abc".into();
        ctx.job_id = "12345678-abcd-abcd-abcd-123456789def".into();
        ctx.step_key = "fmt".into();
        ctx.run_attempt = "0".into();
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_ok()
        );
    }
    #[test]
    fn malformed_provider_identity_names_fields_without_exporting_values() {
        let check = find_check("fmt").unwrap();
        let recipe = commands(check, &[]).unwrap();
        let mut ctx = context(&recipe);
        ctx.provider = "buildkite".into();
        ctx.run_id = "12345678-abcd-abcd-abcd-123456789abc".into();
        ctx.job_id = "12345678-abcd-abcd-abcd-123456789def".into();
        ctx.step_key = "gha-b803fcb7f17ed923-release-metadata".into();
        ctx.run_attempt = "private malformed value".into();
        let error = ctx
            .validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
            .unwrap_err()
            .to_string();
        assert_eq!(
            error,
            "invalid provider-native execution identity: invalid fields=run_attempt"
        );
        assert!(!error.contains("private malformed value"));
        ctx.run_attempt = "0".into();
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_ok()
        );
    }
    #[test]
    fn official_importer_identity_is_logical_and_checked_retry_mapping() {
        let check = find_check("fmt").unwrap();
        let recipe = commands(check, &[]).unwrap();
        let mut ctx = context(&recipe);
        ctx.provider = "buildkite_gha".into();
        ctx.run_id = "12345678-abcd-abcd-abcd-123456789abc".into();
        ctx.run_attempt = imported_native_retry("1").unwrap();
        assert_eq!(ctx.run_attempt, "0");
        assert_eq!(imported_native_retry("4").unwrap(), "3");
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_ok()
        );
        assert_eq!(
            artifact_name(check.job, check.matrix, &ctx.run_attempt).unwrap(),
            "source-ci-custody-fmt-singleton-attempt-0"
        );
        for bad in ["", "0", "01", "-1", "4294967296", "unknown"] {
            assert!(imported_native_retry(bad).is_err(), "{bad}");
        }
        ctx.job_id = "12345678-abcd-abcd-abcd-123456789def".into();
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_err()
        );
        ctx.job_id = check.job.into();
        ctx.step_key = "invented-native-key".into();
        assert!(
            ctx.validate(check, &"1".repeat(40), &"2".repeat(40), &recipe)
                .is_err()
        );
    }
    #[test]
    fn bounded_logs_hash_full_stream_but_mark_truncation() {
        let dir = temp();
        let log = drain(&b"abcdefgh"[..], &dir, "test.log", 3, false).unwrap();
        assert_eq!(fs::read(dir.join("test.log")).unwrap(), b"abc");
        assert_eq!(log.total, 8);
        assert_eq!(log.stored, 3);
        assert_eq!(log.full_sha256, sha(b"abcdefgh").unwrap());
        assert_eq!(log.stored_sha256, sha(b"abc").unwrap());
        assert!(drain(&b"x"[..], &dir, "test.log", 3, false).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn real_exit_and_both_streams_are_preserved() {
        let dir = temp();
        let mut command = Command::new("sh");
        command.args(["-c", "printf out; printf err >&2; exit 7"]);
        let out = execute(command, &dir, "command-0", false).unwrap();
        assert_eq!(out.status.code(), Some(7));
        assert_eq!(fs::read(dir.join(out.stdout.path)).unwrap(), b"out");
        assert_eq!(fs::read(dir.join(out.stderr.path)).unwrap(), b"err");
        fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn isolated_private_creation_child() {
        let Some(root) = env::var_os("SOURCE_CI_PERMISSIONS_CHILD") else {
            return;
        };
        use std::os::unix::fs::PermissionsExt;
        unsafe extern "C" {
            fn umask(mask: u32) -> u32;
        }
        unsafe {
            umask(0);
        }
        let root = PathBuf::from(root);
        private_directory(&root.join("evidence"), false).unwrap();
        new_file(&root.join("evidence/result.json"), b"proof").unwrap();
        let log = drain(&b"log"[..], &root.join("evidence"), "stdout.log", 8, false).unwrap();
        assert_eq!(
            root.join("evidence")
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        for path in ["result.json", &log.path] {
            assert_eq!(
                root.join("evidence")
                    .join(path)
                    .metadata()
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert!(private_directory(&root.join("shared"), true).is_err());
        assert_eq!(
            root.join("shared").metadata().unwrap().permissions().mode() & 0o777,
            0o777
        );
    }
    #[cfg(unix)]
    #[test]
    fn evidence_is_private_at_creation_under_permissive_umask() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp();
        fs::create_dir(dir.join("shared")).unwrap();
        fs::set_permissions(dir.join("shared"), fs::Permissions::from_mode(0o777)).unwrap();
        let listed = Command::new(env::current_exe().unwrap())
            .arg("--list")
            .output()
            .unwrap();
        let listed = String::from_utf8(listed.stdout).unwrap();
        let name = listed
            .lines()
            .find(|line| line.ends_with("custody::tests::isolated_private_creation_child: test"))
            .unwrap()
            .strip_suffix(": test")
            .unwrap();
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("SOURCE_CI_PERMISSIONS_CHILD", &dir)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(
            dir.join("evidence/result.json").is_file(),
            "isolated test was not selected"
        );
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn standard_sha256_known_vector() {
        assert_eq!(
            sha(b"abc").unwrap(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn invocation_changes_are_detected() {
        let mut recipe = commands(find_check("fmt").unwrap(), &[]).unwrap();
        let old = invocation_digest(&recipe).unwrap();
        recipe[0].args.push("--bad");
        assert_ne!(old, invocation_digest(&recipe).unwrap());
    }
    #[test]
    fn ordinary_advanced_base_merge_and_docs_head_preserve_source_semantics() {
        let repo = temp();
        let git = |args: &[&str]| -> String {
            let out = Command::new("git")
                .args([
                    "-c",
                    "user.name=Custody fixture",
                    "-c",
                    "user.email=custody@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "-c",
                    "core.hooksPath=/dev/null",
                ])
                .args(args)
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_COUNT", "0")
                .env_remove("GIT_CONFIG_PARAMETERS")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        git(&["init", "-q", "-b", "master"]);
        fs::write(repo.join("common"), "original\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "common"]);
        let common = git(&["rev-parse", "HEAD"]);
        git(&["checkout", "-qb", "candidate"]);
        fs::write(repo.join("authored"), "candidate change\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "candidate"]);
        let head = git(&["rev-parse", "HEAD"]);
        let head_tree = git(&["rev-parse", "HEAD^{tree}"]);
        git(&["checkout", "-q", "master"]);
        fs::write(
            repo.join("accepted-base-change"),
            "independent base advance\n",
        )
        .unwrap();
        git(&["add", "."]);
        git(&["commit", "-qm", "advanced accepted base"]);
        let base = git(&["rev-parse", "HEAD"]);
        git(&[
            "merge",
            "--no-ff",
            "-m",
            "ordered base/head PR merge",
            "candidate",
        ]);
        let validation = git(&["rev-parse", "HEAD"]);
        let merge_tree = git(&["rev-parse", "HEAD^{tree}"]);
        assert_ne!(merge_tree, head_tree);
        let check = find_check("fmt").unwrap();
        let mut ctx = context(&commands(check, &[]).unwrap());
        ctx.snapshot = validation.clone();
        ctx.validation_head = validation.clone();
        ctx.tree = merge_tree.clone();
        assert_eq!(
            verify_snapshot(&repo, &ctx, &base, &head).unwrap(),
            merge_tree
        );
        verify_final_revision(&repo, check, &head, &ctx, &validation).unwrap();
        fs::write(repo.join("common"), "changed during execution\n").unwrap();
        assert!(verify_final_revision(&repo, check, &head, &ctx, &validation).is_err());
        git(&["add", "common"]);
        fs::write(repo.join("common"), "original\n").unwrap();
        assert!(
            verify_final_revision(&repo, check, &head, &ctx, &validation).is_err(),
            "a staged change cannot hide behind a restored worktree"
        );
        git(&["reset", "-q", "HEAD", "--", "common"]);
        fs::write(repo.join("undeclared-source"), "new input\n").unwrap();
        assert!(verify_final_revision(&repo, check, &head, &ctx, &validation).is_err());
        fs::remove_file(repo.join("undeclared-source")).unwrap();
        verify_final_revision(&repo, check, &head, &ctx, &validation).unwrap();
        // Only the reviewed docs recipe intentionally selects original H.
        git(&["checkout", "-q", "--detach", &head]);
        verify_final_revision(&repo, find_check("docs-drift").unwrap(), &head, &ctx, &head)
            .unwrap();
        assert!(verify_final_revision(&repo, check, &head, &ctx, &head).is_err());
        assert!(verify_snapshot(&repo, &ctx, &base, &head).is_err());
        // Transport mode remains strict even though ordinary merge mode differs.
        let metadata = ci_context::SnapshotMetadata {
            request: "11111111-1111-1111-1111-111111111111".into(),
            source: head.clone(),
            base: common.clone(),
            source_ref: "refs/heads/candidate".into(),
        }
        .encode()
        .unwrap();
        let snapshot = git(&["commit-tree", &head_tree, "-p", &head, "-m", &metadata]);
        ctx.snapshot = snapshot;
        ctx.validation_head = head.clone();
        ctx.tree = head_tree.clone();
        assert_eq!(
            verify_snapshot(&repo, &ctx, &common, &head).unwrap(),
            head_tree
        );
        ctx.snapshot = git(&["commit-tree", &merge_tree, "-p", &head, "-m", &metadata]);
        assert!(
            verify_snapshot(&repo, &ctx, &common, &head).is_err(),
            "transport changed-tree must not use ordinary merge allowance"
        );
        ctx.snapshot = git(&["commit-tree", &head_tree, "-p", &common, "-m", &metadata]);
        assert!(
            verify_snapshot(&repo, &ctx, &common, &head).is_err(),
            "transport must have exactly H as its parent"
        );
        ctx.snapshot = git(&[
            "commit-tree",
            &head_tree,
            "-p",
            &head,
            "-m",
            "not snapshot metadata",
        ]);
        assert!(
            verify_snapshot(&repo, &ctx, &common, &head).is_err(),
            "same tree without exact transport metadata is insufficient"
        );
        fs::remove_dir_all(repo).unwrap();
    }
}

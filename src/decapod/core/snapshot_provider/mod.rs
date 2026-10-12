//! Narrow snapshot transport and authenticated CI status collection.
//! No source-workflow interpreter, authorization policy, artifact attestor or
//! readiness verdict lives here. The independently reviewed plan is mandatory.
//! Results are fresh in-process observations; serialized imports cannot mint them.
pub mod artifact_archive;
pub mod automatic;
#[cfg(test)]
pub(crate) mod fixture_config;
pub mod github_artifacts;
pub mod provider;
pub mod public_buildkite;
pub mod read_authorization;
pub mod snapshot;
pub mod source_execution;
use super::readiness_evidence;

use super::bounded_process::{BoundedCommand, Cancellation, ProcessLimits, Termination};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub type Result<T> = io::Result<T>;
pub(crate) fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub(crate) fn ensure(ok: bool, message: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(invalid(message)) }
}
pub(crate) fn oid(v: &str) -> bool {
    matches!(v.len(), 40 | 64)
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && v.bytes().any(|b| b != b'0')
}
pub(crate) fn uuid(v: &str) -> bool {
    v.len() == 36
        && v.bytes().enumerate().all(|(i, b)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
        && v.bytes().any(|b| matches!(b,b'1'..=b'9'|b'a'..=b'f'))
}
pub(crate) fn slug(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 100
        && v != "."
        && v != ".."
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !v.starts_with('-')
}
pub(crate) fn branch(v: &str) -> bool {
    v.starts_with("refs/heads/")
        && v.len() <= 512
        && !v.ends_with('.')
        && !v.contains("..")
        && !v.contains("@{")
        && v.bytes().all(|b| {
            b.is_ascii_graphic() && !matches!(b, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
        && v.split('/')
            .all(|p| !p.is_empty() && !p.starts_with('.') && !p.ends_with(".lock"))
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub(crate) fn encode_path(v: &str) -> String {
    v.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Repository {
    owner: String,
    name: String,
}
impl Repository {
    pub fn new(owner: &str, name: &str) -> Result<Self> {
        ensure(slug(owner) && slug(name), "invalid GitHub repository")?;
        Ok(Self {
            owner: owner.into(),
            name: name.into(),
        })
    }
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name)
    }
    pub fn https_url(&self) -> String {
        format!("https://github.com/{}.git", self.full_name())
    }
    pub(crate) fn api(&self) -> String {
        format!("repos/{}", self.full_name())
    }
}

/// One deadline owns every subprocess and polling sleep. No background thread.
pub struct Budget<'a> {
    deadline: Instant,
    cancel: &'a Cancellation,
    pub(crate) max_bytes: usize,
}
impl<'a> Budget<'a> {
    pub fn new(timeout: Duration, max_bytes: usize, cancel: &'a Cancellation) -> Result<Self> {
        ensure(
            !timeout.is_zero()
                && timeout <= Duration::from_secs(7200)
                && max_bytes > 0
                && max_bytes <= 16 * 1024 * 1024,
            "invalid collection limits",
        )?;
        Ok(Self {
            deadline: Instant::now() + timeout,
            cancel,
            max_bytes,
        })
    }
    pub(crate) fn remaining(&self) -> Result<Duration> {
        if self.cancel.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "CI operation cancelled",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|v| !v.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "CI operation deadline exceeded")
            })
    }
    pub(crate) fn wait(&self, duration: Duration) -> Result<()> {
        let until = Instant::now() + duration;
        while Instant::now() < until {
            let remaining = self.remaining()?;
            std::thread::sleep(
                Duration::from_millis(100)
                    .min(remaining)
                    .min(until.saturating_duration_since(Instant::now())),
            );
        }
        self.remaining()?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Program {
    GitLocal,
    GitTransport,
    Github,
    Buildkite,
}
/// Opaque to external callers. There is deliberately no arbitrary argv constructor.
#[derive(Debug)]
pub enum ProviderRequest {
    GithubGet {
        path: String,
    },
    GithubArtifactZip {
        repository: Repository,
        artifact_id: u64,
        expected_bytes: usize,
    },
    GithubJobLog {
        repository: Repository,
        job_id: u64,
    },
    BuildkiteGet {
        organization: String,
        path: String,
    },
    BuildkiteCreate {
        organization: String,
        pipeline: String,
        request: Box<snapshot::SnapshotRequest>,
        transport_head: String,
    },
}

#[derive(Debug)]
pub struct CommandSpec {
    pub(crate) program: Program,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) organization: Option<String>,
    pub(crate) provider_request: Option<ProviderRequest>,
}
impl CommandSpec {
    pub fn provider_request(&self) -> Option<&ProviderRequest> {
        self.provider_request.as_ref()
    }
    pub fn program(&self) -> Program {
        self.program
    }
    pub fn arguments(&self) -> &[String] {
        &self.args
    }
}
#[derive(Debug)]
pub struct Reply {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
}
/// Test doubles implement this seam; runtime must use OfficialCli, never import JSON as custody.
pub trait Runner {
    fn execute(&mut self, command: &CommandSpec, budget: &Budget<'_>) -> Result<Reply>;
}
/// Paths must identify already installed, trusted official binaries. No discovery,
/// installation, login, token extraction or persistent settings changes occur here.
type AuthorizationCheck = Box<dyn FnMut(&CommandSpec, &Path) -> Result<()>>;

pub struct OfficialCli {
    git: PathBuf,
    gh: Option<PathBuf>,
    bk: Option<PathBuf>,
    authorize: AuthorizationCheck,
}
impl OfficialCli {
    /// Git-only transport paired with a sealed public reader or a trusted host.
    /// It cannot silently fall back to an authenticated provider CLI.
    pub fn git_only<A>(git: PathBuf, authorize: A) -> Result<Self>
    where
        A: FnMut(&CommandSpec, &Path) -> Result<()> + 'static,
    {
        ensure(
            git.is_absolute() && git.is_file(),
            "installed absolute Git required",
        )?;
        Ok(Self {
            git,
            gh: None,
            bk: None,
            authorize: Box::new(authorize),
        })
    }
    pub fn new<A>(git: PathBuf, gh: PathBuf, bk: Option<PathBuf>, authorize: A) -> Result<Self>
    where
        A: FnMut(&CommandSpec, &Path) -> Result<()> + 'static,
    {
        for path in [&git, &gh].into_iter().chain(bk.as_ref()) {
            ensure(
                path.is_absolute() && path.is_file(),
                "official CLI must be an existing absolute executable path",
            )?;
        }
        Ok(Self {
            git,
            gh: Some(gh),
            bk,
            authorize: Box::new(authorize),
        })
    }
}
impl Runner for OfficialCli {
    fn execute(&mut self, spec: &CommandSpec, budget: &Budget<'_>) -> Result<Reply> {
        budget.remaining()?;
        let program = match spec.program {
            Program::GitLocal | Program::GitTransport => &self.git,
            Program::Github => self.gh.as_ref().ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "configured GitHub CLI unavailable")
            })?,
            Program::Buildkite => self.bk.as_ref().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "configured Buildkite CLI unavailable",
                )
            })?,
        };
        if matches!(
            spec.provider_request(),
            Some(
                ProviderRequest::GithubGet { .. }
                    | ProviderRequest::GithubArtifactZip { .. }
                    | ProviderRequest::GithubJobLog { .. }
                    | ProviderRequest::BuildkiteGet { .. }
            )
        ) {
            spec.authorize_provider_read()?;
        }
        (self.authorize)(spec, program)?;
        let mut c = Command::new(program);
        c.current_dir(&spec.cwd);
        // Preserve already configured authentication under the official CLI. We
        // never inspect secret values. Disable output/debug/pager/browser hooks.
        c.env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .env("PAGER", "cat")
            .env_remove("GH_DEBUG")
            .env_remove("DEBUG")
            .env_remove("GH_FORCE_TTY");
        if matches!(spec.program, Program::GitLocal | Program::GitTransport) {
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("GIT_") {
                    c.env_remove(key);
                }
            }
            c.env("GIT_ALLOW_PROTOCOL", "https")
                .env("GIT_TERMINAL_PROMPT", "0")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .env("GIT_NO_LAZY_FETCH", "1");
            if spec.program == Program::GitLocal {
                c.env("GIT_CONFIG_NOSYSTEM", "1")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null");
            }
            c.env("GIT_AUTHOR_NAME", "Decapod CI snapshot")
                .env("GIT_AUTHOR_EMAIL", "ci-snapshot@decapod.invalid")
                .env("GIT_COMMITTER_NAME", "Decapod CI snapshot")
                .env("GIT_COMMITTER_EMAIL", "ci-snapshot@decapod.invalid");
        }
        if let Some(org) = &spec.organization {
            c.env("BUILDKITE_ORGANIZATION_SLUG", org)
                .env("BUILDKITE_REST_API_ENDPOINT", "https://api.buildkite.com/")
                .env(
                    "BUILDKITE_GRAPHQL_ENDPOINT",
                    "https://graphql.buildkite.com/v1",
                )
                .env("BUILDKITE_NO_PAGER", "true")
                .env("BUILDKITE_NO_INPUT", "true")
                .env("BK_TELEMETRY", "false")
                .env_remove("BUILDKITE_DEBUG");
        }
        c.args(&spec.args);
        let captured = if let Some(ProviderRequest::GithubArtifactZip { expected_bytes, .. }) =
            spec.provider_request()
        {
            // Metadata-verified declared body, separate from the JSON/log cap.
            c.bounded_artifact_capture(
                ProcessLimits {
                    timeout: budget.remaining()?.min(Duration::from_secs(300)),
                    max_output_bytes: expected_bytes + 64 * 1024,
                },
                budget.cancel,
            )?
        } else {
            c.bounded_capture(
                ProcessLimits {
                    timeout: budget.remaining()?.min(Duration::from_secs(30)),
                    max_output_bytes: budget.max_bytes,
                },
                budget.cancel,
            )?
        };
        match captured.termination {
            Termination::Exited => {}
            Termination::Cancelled => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "official CLI cancelled",
                ));
            }
            Termination::TimedOut => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "official CLI timed out",
                ));
            }
            Termination::OutputLimit => return Err(invalid("official CLI output bound exceeded")),
        }
        Ok(Reply {
            exit_code: captured.output.status.code().unwrap_or(-1),
            stdout: captured.output.stdout,
        })
    }
}
pub(crate) fn captured(
    runner: &mut impl Runner,
    spec: &CommandSpec,
    budget: &Budget<'_>,
) -> Result<Reply> {
    budget.remaining()?;
    let result = runner.execute(spec, budget)?;
    ensure(
        result.stdout.len() <= budget.max_bytes,
        "provider output exceeded bound",
    )?;
    Ok(result)
}
pub(crate) fn checked(
    runner: &mut impl Runner,
    spec: CommandSpec,
    budget: &Budget<'_>,
) -> Result<Vec<u8>> {
    budget.remaining()?;
    let r = captured(runner, &spec, budget)?;
    ensure(
        r.stdout.len() <= budget.max_bytes,
        "provider output exceeded bound",
    )?;
    if r.exit_code != 0 {
        // Only the typed read seal can supply an endpoint. Never echo raw argv,
        // stdout, stderr, environment, unvalidated URLs or signed query values.
        let operation = match spec.authorize_provider_read() {
            Ok(read) => format!(
                "operation={:?} method={} endpoint={}",
                read.kind(),
                read.method(),
                read.initial_url()
            ),
            Err(_) => format!("operation={:?}", spec.program()),
        };
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "official CLI request failed: {operation} exit_code={}; authentication or provider availability must be resolved without extracting tokens",
                r.exit_code
            ),
        ));
    }
    Ok(r.stdout)
}
pub(crate) fn text(bytes: Vec<u8>) -> Result<String> {
    String::from_utf8(bytes).map_err(|_| invalid("non-UTF8 provider response"))
}
pub(crate) fn git_spec(root: &Path, args: &[&str], transport: bool) -> CommandSpec {
    let mut a = vec![
        "--no-replace-objects",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgSign=false",
        "-c",
        "push.followTags=false",
        "-c",
        "push.recurseSubmodules=no",
        "-c",
        "http.sslVerify=true",
        "-c",
        "http.followRedirects=false",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    a.extend(args.iter().map(|v| v.to_string()));
    CommandSpec {
        program: if transport {
            Program::GitTransport
        } else {
            Program::GitLocal
        },
        args: a,
        cwd: root.into(),
        organization: None,
        provider_request: None,
    }
}

/// Keep only the host's existing network route, opaquely. No values are logged,
/// persisted, interpreted as service credentials, or transferred to API headers.
/// Removing this route could bypass configured network policy or break DNS.
pub(crate) fn public_network_environment(command: &mut Command) {
    command.env_clear();
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
}

/// Explicit public-read mode, chosen before making requests. It is not an auth
/// error fallback and never accesses private data or Buildkite APIs anonymously.
/// TLS authenticates api.github.com; no account credential is sent. Curl must
/// already be installed/trusted. User curl config, environment, cookies, netrc,
/// external auth helpers are unavailable to this narrow client. Only the exact
/// job-log endpoint may follow up to three provider-issued HTTPS redirects.
pub struct PublicGithubReads<R> {
    curl: PathBuf,
    repository: Repository,
    other: R,
    authorize: AuthorizationCheck,
}
impl<R: Runner> PublicGithubReads<R> {
    pub fn new<A>(curl: PathBuf, repository: Repository, other: R, authorize: A) -> Result<Self>
    where
        A: FnMut(&CommandSpec, &Path) -> Result<()> + 'static,
    {
        ensure(
            curl.is_absolute() && curl.is_file(),
            "public API reader requires installed absolute curl path",
        )?;
        Ok(Self {
            curl,
            repository,
            other,
            authorize: Box::new(authorize),
        })
    }
}
impl<R: Runner> Runner for PublicGithubReads<R> {
    fn execute(&mut self, spec: &CommandSpec, budget: &Budget<'_>) -> Result<Reply> {
        let (path, logs) = match spec.provider_request() {
            Some(ProviderRequest::GithubGet { path }) => (path.clone(), false),
            Some(ProviderRequest::GithubJobLog { repository, job_id }) => {
                ensure(
                    repository == &self.repository && *job_id > 0,
                    "public log repository mismatch",
                )?;
                (
                    format!("{}/actions/jobs/{job_id}/logs", repository.api()),
                    true,
                )
            }
            _ => return self.other.execute(spec, budget),
        };
        spec.authorize_provider_read()?;
        // An observed, host-pinned GitHub log-storage route is not yet available.
        // Never follow arbitrary HTTPS redirects under a strict public read grant.
        ensure(
            !logs,
            "public GitHub job-log storage redirect is unsupported; use an authorized configured official CLI or supported host integration",
        )?;
        ensure(
            path.starts_with(&format!("{}/", self.repository.api())),
            "public reader repository mismatch",
        )?;
        (self.authorize)(spec, &self.curl)?;
        let mut command = Command::new(&self.curl);
        public_network_environment(&mut command);
        command.current_dir(&spec.cwd).args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--connect-timeout",
            "10",
            "--request",
            "GET",
            "--header",
            "Accept: application/vnd.github+json",
        ]);
        command.args([
            "--max-redirs",
            "0",
            "--dump-header",
            "-",
            "--suppress-connect-headers",
        ]);
        command.args(["--url", &format!("https://api.github.com/{path}")]);
        let output = command.bounded_capture(
            ProcessLimits {
                timeout: budget.remaining()?.min(Duration::from_secs(30)),
                max_output_bytes: budget.max_bytes,
            },
            budget.cancel,
        )?;
        match output.termination {
            Termination::Exited => {
                let read = spec.authorize_provider_read()?;
                if !output.output.status.success() {
                    return Err(public_http_failure(
                        &format!("{:?}", read.kind()),
                        read.initial_url(),
                        output.output.status.code().unwrap_or(-1),
                        &output.output.stdout,
                    ));
                }
                let (status, _, body) = public_buildkite::http_response(&output.output.stdout)?;
                ensure(
                    status == 200,
                    "public GitHub endpoint did not return HTTP 200",
                )?;
                Ok(Reply {
                    exit_code: 0,
                    stdout: body.to_vec(),
                })
            }
            Termination::Cancelled => Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "public API read cancelled",
            )),
            Termination::TimedOut => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "public API read timed out",
            )),
            Termination::OutputLimit => Err(invalid("public API read exceeded output bound")),
        }
    }
}

/// Error-only diagnostics from the original bounded response. Endpoint must come
/// from the typed seal, never a redirect Location or curl stderr. GitHub's quota
/// headers use decimal seconds/epoch; unsupported or duplicate values are omitted.
fn public_http_failure(operation: &str, endpoint: &str, exit_code: i32, bytes: &[u8]) -> io::Error {
    public_http_failure_after_attempts(operation, endpoint, exit_code, bytes, 1)
}
fn public_http_failure_after_attempts(
    operation: &str,
    endpoint: &str,
    exit_code: i32,
    bytes: &[u8],
    attempts: u8,
) -> io::Error {
    let status = public_buildkite::http_response(bytes)
        .ok()
        .map(|(status, _, _)| status);
    let mut hints = std::collections::BTreeMap::<&str, Option<u64>>::new();
    if let Some(end) = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .filter(|end| *end <= 64 * 1024)
        && let Ok(headers) = std::str::from_utf8(&bytes[..end])
    {
        for line in headers.split("\r\n").skip(1) {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let key = if name.eq_ignore_ascii_case("retry-after") {
                "retry_after_seconds"
            } else if name.eq_ignore_ascii_case("x-ratelimit-reset") {
                "rate_limit_reset_unix"
            } else {
                continue;
            };
            let value = value.trim();
            let number = (!value.is_empty()
                && value.len() <= 20
                && value.bytes().all(|b| b.is_ascii_digit()))
            .then(|| value.parse::<u64>().ok())
            .flatten();
            hints
                .entry(key)
                .and_modify(|value| *value = None)
                .or_insert(number);
        }
    }
    let mut detail = format!(
        "public provider request failed: operation={operation} method=GET endpoint={endpoint} exit_code={exit_code} http_status={}",
        status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unavailable".into())
    );
    for (name, value) in hints {
        if let Some(value) = value {
            detail.push_str(&format!(" {name}={value}"));
        }
    }
    if attempts == 1 {
        detail.push_str("; no automatic retry or alternate authentication route was attempted");
    } else {
        detail.push_str(&format!(
            "; bounded_timeout_attempts={attempts}; no alternate authentication route was attempted"
        ));
    }
    io::Error::other(detail)
}

#[cfg(test)]
mod request_failure_tests {
    use super::*;
    struct Failed(usize);
    impl Runner for Failed {
        fn execute(&mut self, _: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
            self.0 += 1;
            Ok(Reply {
                exit_code: 22,
                stdout: b"untrusted-response-private-value".to_vec(),
            })
        }
    }
    #[test]
    fn failed_request_identifies_only_sealed_public_endpoint_and_exit() {
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_secs(2), 4096, &cancel).unwrap();
        let path = format!(
            "repos/org/repo/commits/{}/check-runs?filter=all&per_page=100&page=1",
            "a".repeat(40)
        );
        let mut runner = Failed(0);
        let error = checked(
            &mut runner,
            provider::gh_spec(Path::new("/tmp"), &path),
            &budget,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        let message = error.to_string();
        assert!(message.contains("operation=GithubCommitChecks method=GET"));
        assert!(message.contains(&format!("endpoint=https://api.github.com/{path}")));
        assert!(message.contains("exit_code=22"));
        assert!(!message.contains("untrusted-response-private-value"));
        assert_eq!(runner.0, 1);
    }
    #[test]
    fn unsealed_url_or_local_argv_never_enters_failure_diagnostic() {
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_secs(2), 4096, &cancel).unwrap();
        for spec in [
            provider::gh_spec(
                Path::new("/tmp"),
                "https://user:private-value@evil.invalid/private-path?token=private-value",
            ),
            git_spec(Path::new("/tmp"), &["show", "private-value"], false),
        ] {
            let mut runner = Failed(0);
            let message = checked(&mut runner, spec, &budget).unwrap_err().to_string();
            assert!(message.contains("exit_code=22"));
            for hidden in [
                "private-value",
                "evil.invalid",
                "private-path",
                "untrusted-response",
                "endpoint=",
            ] {
                assert!(!message.contains(hidden));
            }
            assert_eq!(runner.0, 1);
        }
    }
    #[test]
    fn public_http_quota_failure_preserves_only_status_and_numeric_retry_hints() {
        let bytes = b"HTTP/2 403 Forbidden\r\nRetry-After: 60\r\nX-RateLimit-Reset: 1791787200\r\nSet-Cookie: private-cookie\r\nLocation: https://evil.invalid/?signature=private-value\r\nX-Other: private-header\r\n\r\nprivate-body";
        let error = public_http_failure(
            "GithubCommitChecks",
            "https://api.github.com/repos/org/repo/commits/exact/check-runs",
            22,
            bytes,
        )
        .to_string();
        for wanted in [
            "http_status=403",
            "exit_code=22",
            "retry_after_seconds=60",
            "rate_limit_reset_unix=1791787200",
        ] {
            assert!(error.contains(wanted));
        }
        for hidden in [
            "private",
            "evil.invalid",
            "signature",
            "Set-Cookie",
            "Forbidden",
        ] {
            assert!(!error.contains(hidden));
        }
    }
    #[test]
    fn malformed_duplicate_or_absent_rate_headers_never_escape() {
        for bytes in [
            b"HTTP/1.1 429 Rate Limited\r\nRetry-After: private-value\r\nX-RateLimit-Reset: 18446744073709551616\r\n\r\n".as_slice(),
            b"HTTP/1.1 429 Rate Limited\r\nRetry-After: 60\r\nRetry-After: 90\r\n\r\n".as_slice(),
        ] {
            let error = public_http_failure("GithubCommitChecks", "https://api.github.com/repos/org/repo/commits/exact/check-runs", 22, bytes).to_string();
            assert!(error.contains("http_status=429"));
            assert!(!error.contains("private-value"));
            assert!(!error.contains("retry_after_seconds="));
            assert!(!error.contains("rate_limit_reset_unix="));
        }
        let error = public_http_failure(
            "GithubCommitChecks",
            "https://api.github.com/repos/org/repo/commits/exact/check-runs",
            6,
            b"private transport output",
        )
        .to_string();
        assert!(error.contains("http_status=unavailable"));
        assert!(!error.contains("private transport output"));
    }
    #[cfg(unix)]
    #[test]
    fn public_curl_adapter_strips_success_headers_and_reports_quota_without_body_or_stderr() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let curl = temp.path().join("synthetic-curl");
        let cancel = Cancellation::default();
        let budget = Budget::new(Duration::from_secs(3), 65536, &cancel).unwrap();
        for failed in [false, true] {
            let status = if failed {
                "429 Too Many Requests"
            } else {
                "200 OK"
            };
            let exit = if failed { 22 } else { 0 };
            std::fs::write(&curl, format!("#!/bin/sh\ncase \"$*\" in *'--dump-header - --suppress-connect-headers'*) ;; *) exit 99;; esac\nprintf 'HTTP/2 {status}\\r\\nRetry-After: 60\\r\\nX-RateLimit-Reset: 1791787200\\r\\nSet-Cookie: private-cookie\\r\\n\\r\\n{{}}'\nprintf 'private-stderr' >&2\nexit {exit}\n")).unwrap();
            std::fs::set_permissions(&curl, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut runner = PublicGithubReads::new(
                curl.clone(),
                Repository::new("org", "repo").unwrap(),
                Failed(0),
                |spec, _| {
                    spec.authorize_provider_read()?;
                    Ok(())
                },
            )
            .unwrap();
            let result = runner.execute(
                &provider::gh_spec(temp.path(), "repos/org/repo/actions/runs/7"),
                &budget,
            );
            if failed {
                let message = result.unwrap_err().to_string();
                assert!(message.contains("http_status=429"));
                assert!(message.contains("retry_after_seconds=60"));
                assert!(message.contains("rate_limit_reset_unix=1791787200"));
                assert!(!message.contains("private"));
                assert!(!message.contains("{}"));
            } else {
                let reply = result.unwrap();
                assert_eq!(reply.exit_code, 0);
                assert_eq!(reply.stdout, b"{}");
            }
        }
    }
}

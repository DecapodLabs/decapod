//! Dedicated, anonymous public-site reader. This is not the REST API schema.
//! Complete native job inventory and content-addressed importer provenance do
//! not prove substantive source checks. Only measured execution custody may do
//! that; this module deliberately cannot mint a readiness Observation.
use super::super::bounded_process::ProcessCapture;
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

mod collection;
mod custody;
mod program;
pub use collection::CollectionReader;
pub use custody::{
    CollectedCustody, CustodyRead, CustodyRequest, CustodyResponse, collect_custody,
    prepare_custody,
};

/// Metadata-only batches share the caller's deadline/cancellation. Custody
/// transport remains separately limited to four reads and 128 MiB aggregate.
pub const PUBLIC_READ_CONCURRENCY: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicBuild {
    organization: String,
    pipeline: String,
    number: u64,
}
impl PublicBuild {
    pub fn new(organization: &str, pipeline: &str, number: u64) -> Result<Self> {
        ensure(
            slug(organization) && slug(pipeline) && number > 0,
            "invalid public build",
        )?;
        Ok(Self {
            organization: organization.into(),
            pipeline: pipeline.into(),
            number,
        })
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    fn base(&self) -> String {
        format!(
            "/{}/{}/builds/{}",
            self.organization, self.pipeline, self.number
        )
    }
    fn job_base(&self, job: &str) -> Result<String> {
        ensure(uuid(job), "invalid public job UUID")?;
        Ok(format!(
            "/organizations/{}/pipelines/{}/builds/{}/jobs/{job}",
            self.organization, self.pipeline, self.number
        ))
    }
}
/// An exact GET destination sealed by this module; never a caller-selected URL.
#[derive(Clone, Debug)]
pub struct PublicRequest {
    path: String,
    artifact_redirect: bool,
    log_redirect: Option<LogRedirectIdentity>,
}
#[derive(Clone, Debug)]
struct LogRedirectIdentity {
    pipeline: String,
    build: String,
    job: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicReadKind {
    BuildEnvelope,
    FullSteps,
    FullJobs,
    JobArtifacts,
    ArtifactContent,
    JobLog,
}
/// Unforgeable outside this module. Application authorization accepts this seal,
/// never a raw URL/capability name or shell command supplied by a caller.
#[derive(Debug)]
pub struct AuthorizedPublicRead {
    kind: PublicReadKind,
    url: String,
    scope: String,
    redirects: u8,
}
impl AuthorizedPublicRead {
    pub fn kind(&self) -> PublicReadKind {
        self.kind
    }
    pub fn method(&self) -> &'static str {
        "GET"
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn scope(&self) -> &str {
        &self.scope
    }
    pub fn max_https_redirects(&self) -> u8 {
        self.redirects
    }
}
impl PublicRequest {
    pub fn authorize_read(&self) -> Result<AuthorizedPublicRead> {
        let parts: Vec<_> = self.path.split('/').collect();
        let decimal = |value: &str| {
            value.bytes().all(|b| b.is_ascii_digit())
                && value.parse::<u64>().is_ok_and(|n| n > 0)
                && !value.starts_with('0')
        };
        let (kind, organization, pipeline, number) = match parts.as_slice() {
            ["", org, pipe, "builds", file]
                if slug(org) && slug(pipe) && file.strip_suffix(".json").is_some_and(decimal) =>
            {
                (
                    PublicReadKind::BuildEnvelope,
                    *org,
                    *pipe,
                    file.strip_suffix(".json").unwrap(),
                )
            }
            ["", org, pipe, "builds", number, "data", endpoint]
                if slug(org) && slug(pipe) && decimal(number) =>
            {
                let kind = match *endpoint {
                    "steps" => PublicReadKind::FullSteps,
                    "jobs?include_retried_jobs=true&paginate=false" => PublicReadKind::FullJobs,
                    _ => return Err(invalid("unsupported public inventory route")),
                };
                (kind, *org, *pipe, *number)
            }
            [
                "",
                "organizations",
                org,
                "pipelines",
                pipe,
                "builds",
                number,
                "jobs",
                job,
                endpoint,
            ] if slug(org) && slug(pipe) && decimal(number) && uuid(job) => {
                let kind = match *endpoint {
                    "artifacts" => PublicReadKind::JobArtifacts,
                    "download.txt" => PublicReadKind::JobLog,
                    _ => return Err(invalid("unsupported public job route")),
                };
                (kind, *org, *pipe, *number)
            }
            [
                "",
                "organizations",
                org,
                "pipelines",
                pipe,
                "builds",
                number,
                "jobs",
                job,
                "artifacts",
                artifact,
            ] if slug(org) && slug(pipe) && decimal(number) && uuid(job) && uuid(artifact) => {
                (PublicReadKind::ArtifactContent, *org, *pipe, *number)
            }
            _ => return Err(invalid("public request escaped fixed GET routes")),
        };
        ensure(
            self.artifact_redirect == (kind == PublicReadKind::ArtifactContent),
            "public redirect policy mismatch",
        )?;
        match (&self.log_redirect, kind) {
            (Some(identity), PublicReadKind::JobLog) => ensure(
                uuid(&identity.pipeline)
                    && uuid(&identity.build)
                    && uuid(&identity.job)
                    && parts[8] == identity.job,
                "public log redirect identity mismatch",
            )?,
            (None, PublicReadKind::JobLog) => {
                return Err(invalid("public log requires verified inventory identity"));
            }
            (Some(_), _) => return Err(invalid("log redirect on non-log route")),
            (None, _) => (),
        }
        Ok(AuthorizedPublicRead {
            kind,
            url: self.url(),
            scope: format!("buildkite:{organization}/{pipeline}/builds/{number}"),
            redirects: match kind {
                PublicReadKind::ArtifactContent => 3,
                PublicReadKind::JobLog => 1,
                _ => 0,
            },
        })
    }
    pub fn url(&self) -> String {
        format!("https://buildkite.com{}", self.path)
    }
    pub fn follows_artifact_redirect(&self) -> bool {
        self.artifact_redirect
    }
}
pub trait PublicReader {
    fn get(&mut self, request: &PublicRequest, budget: &Budget<'_>) -> Result<Vec<u8>>;

    /// Serial fallback for readers without concurrent transport. Implementations
    /// must retain order and join every owned read before returning an error.
    fn get_many(
        &mut self,
        requests: &[PublicRequest],
        budget: &Budget<'_>,
    ) -> Result<Vec<Vec<u8>>> {
        ensure(
            budget.max_bytes <= 16 * 1024 * 1024,
            "metadata batch output allowance exceeds bound",
        )?;
        ensure(
            requests.len() <= PUBLIC_READ_CONCURRENCY,
            "public read batch exceeds bound",
        )?;
        requests
            .iter()
            .map(|request| {
                budget.remaining()?;
                request.authorize_read()?;
                let bytes = self.get(request, budget)?;
                ensure(
                    bytes.len() <= budget.max_bytes,
                    "public response exceeded bound",
                )?;
                Ok(bytes)
            })
            .collect()
    }
    /// Typed custody requests carry individual output limits; the ordinary
    /// metadata budget is never globally enlarged for an archive.
    fn get_custody_many(
        &mut self,
        requests: &[CustodyRead],
        budget: &Budget<'_>,
    ) -> Result<Vec<CustodyResponse>> {
        custody::validate_reads(requests, budget)?;
        requests
            .iter()
            .map(|read| {
                let started = Instant::now();
                let bytes = read_request(self, &read.request, &read.budget(budget))?;
                Ok(CustodyResponse {
                    bytes,
                    elapsed: started.elapsed(),
                })
            })
            .collect()
    }
}
type PublicAuthorization = Box<dyn FnMut(&PublicRequest, &Path) -> Result<()>>;
/// Choose this mode explicitly for a public pipeline before any auth attempt.
/// Existing curl only; no config, environment, credentials, cookies or login.
pub struct CurlPublicReader {
    curl: PathBuf,
    authorize: PublicAuthorization,
}
impl CurlPublicReader {
    pub fn new<A>(curl: PathBuf, authorize: A) -> Result<Self>
    where
        A: FnMut(&PublicRequest, &Path) -> Result<()> + 'static,
    {
        ensure(
            curl.is_absolute() && curl.is_file(),
            "installed absolute curl required",
        )?;
        Ok(Self {
            curl,
            authorize: Box::new(authorize),
        })
    }
}
/// Only the observed Buildkite-owned S3 bucket layout is supported. Validation
/// runs before issuing the redirected GET. Query signatures remain opaque and
/// are never included in errors, logs, authorization scopes or persisted state.
fn validate_storage_redirect(request: &PublicRequest, location: &str) -> Result<()> {
    ensure(
        request.authorize_read()?.kind == PublicReadKind::ArtifactContent,
        "redirect on a non-artifact read",
    )?;
    ensure(
        location.len() <= 16 * 1024
            && location.bytes().all(|b| b.is_ascii_graphic())
            && !location.contains('#'),
        "invalid artifact redirect encoding",
    )?;
    let relative = location
        .strip_prefix("https://s3.amazonaws.com/buildkiteartifacts.com/")
        .ok_or_else(|| invalid("unsupported artifact storage destination"))?;
    let path = relative.split('?').next().unwrap();
    let parts: Vec<_> = path.split('/').collect();
    let request_parts: Vec<_> = request.path.split('/').collect();
    ensure(
        parts.len() >= 5 && parts[..4].iter().all(|p| uuid(p)) && parts[3] == request_parts[8],
        "artifact storage producer/path mismatch",
    )?;
    ensure(
        parts[4..].iter().all(|p| {
            !p.is_empty()
                && *p != "."
                && *p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        }),
        "unsupported artifact object path",
    )
}
/// One redirect from the exact provider job-log route, bound to all three
/// UUIDs already authenticated in PublicInventory. Signed query bytes stay
/// opaque and are never emitted in diagnostics or persisted as evidence.
fn validate_log_redirect(request: &PublicRequest, location: &str) -> Result<()> {
    ensure(
        request.authorize_read()?.kind == PublicReadKind::JobLog,
        "log redirect on a non-log read",
    )?;
    let identity = request
        .log_redirect
        .as_ref()
        .ok_or_else(|| invalid("log redirect lacks inventory identity"))?;
    ensure(
        location.len() <= 16 * 1024
            && location.bytes().all(|b| b.is_ascii_graphic())
            && !location.contains('#'),
        "invalid log redirect encoding",
    )?;
    let path = location.split('?').next().unwrap_or(location);
    ensure(
        path == format!(
            "https://buildkite-cloud.s3.amazonaws.com/logs-by-pipeline/{}/{}/{}.log",
            identity.pipeline, identity.build, identity.job
        ),
        "log storage destination or producer identity mismatch",
    )
}
pub(super) fn http_response(bytes: &[u8]) -> Result<(u16, Option<String>, &[u8])> {
    let end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| invalid("provider response lacks HTTP headers"))?;
    ensure(end <= 64 * 1024, "provider HTTP headers exceed bound")?;
    let headers =
        std::str::from_utf8(&bytes[..end]).map_err(|_| invalid("invalid provider HTTP headers"))?;
    let mut lines = headers.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| invalid("missing provider HTTP status"))?;
    let mut status_parts = status_line.split_whitespace();
    ensure(
        matches!(
            status_parts.next(),
            Some("HTTP/1.0" | "HTTP/1.1" | "HTTP/2" | "HTTP/3")
        ),
        "unsupported provider HTTP protocol",
    )?;
    let status = status_parts
        .next()
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| invalid("invalid provider HTTP status"))?;
    let mut location = None;
    for line in lines {
        let (key, value) = line
            .split_once(':')
            .ok_or_else(|| invalid("malformed provider HTTP header"))?;
        if key.eq_ignore_ascii_case("location") {
            ensure(location.is_none(), "ambiguous provider redirect")?;
            location = Some(value.trim().to_owned());
        }
    }
    Ok((status, location, &bytes[end + 4..]))
}
impl PublicReader for CurlPublicReader {
    fn get(&mut self, request: &PublicRequest, budget: &Budget<'_>) -> Result<Vec<u8>> {
        budget.remaining()?;
        request.authorize_read()?;
        (self.authorize)(request, &self.curl)?;
        authorized_curl_get(&self.curl, request, budget)
    }

    fn get_many(
        &mut self,
        requests: &[PublicRequest],
        budget: &Budget<'_>,
    ) -> Result<Vec<Vec<u8>>> {
        ensure(
            budget.max_bytes <= 16 * 1024 * 1024,
            "metadata batch output allowance exceeds bound",
        )?;
        ensure(
            requests.len() <= PUBLIC_READ_CONCURRENCY,
            "public read batch exceeds bound",
        )?;
        // Authorization stays on the owner thread. No read starts unless every
        // exact request is authorized; no FnMut policy is sent to a worker.
        for request in requests {
            budget.remaining()?;
            request.authorize_read()?;
            (self.authorize)(request, &self.curl)?;
        }
        let curl = &self.curl;
        bounded_parallel_reads(requests, |request| {
            authorized_curl_get(curl, request, budget)
        })
    }
    fn get_custody_many(
        &mut self,
        requests: &[CustodyRead],
        budget: &Budget<'_>,
    ) -> Result<Vec<CustodyResponse>> {
        custody::validate_reads(requests, budget)?;
        for read in requests {
            budget.remaining()?;
            read.request.authorize_read()?;
            (self.authorize)(&read.request, &self.curl)?;
        }
        let curl = &self.curl;
        bounded_parallel_reads(requests, |read| {
            let started = Instant::now();
            let bytes = authorized_curl_get(curl, &read.request, &read.budget(budget))?;
            Ok(CustodyResponse {
                bytes,
                elapsed: started.elapsed(),
            })
        })
    }
}

fn bounded_parallel_reads<T: Sync, U: Send>(
    requests: &[T],
    read: impl Fn(&T) -> Result<U> + Sync,
) -> Result<Vec<U>> {
    ensure(
        requests.len() <= PUBLIC_READ_CONCURRENCY,
        "public read batch exceeds bound",
    )?;
    std::thread::scope(|scope| {
        let handles: Vec<_> = requests
            .iter()
            .map(|request| {
                let read = &read;
                scope.spawn(move || read(request))
            })
            .collect();
        // Join all workers, including after an earlier failure. No background
        // work survives this call and ordered errors cannot yield partial proof.
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err(io::Error::other("public read worker panicked")))
            })
            .collect();
        results.into_iter().collect()
    })
}

/// Execute one already-authorized hop. Capture and wait stay bound to the
/// caller's original Budget; separate closures let policy tests model timeout,
/// cancellation and deadline exhaustion without racing the CI wall clock.
fn read_with_timeout_retries(
    kind: PublicReadKind,
    endpoint: &str,
    timeout_retries: &mut u8,
    mut capture: impl FnMut() -> Result<ProcessCapture>,
    mut wait: impl FnMut(Duration) -> Result<()>,
) -> Result<ProcessCapture> {
    loop {
        let out = capture()?;
        let timed_out = matches!(out.termination, Termination::TimedOut)
            || (matches!(out.termination, Termination::Exited)
                && out.output.status.code() == Some(28));
        // Retry only a timeout of this identical, already-authorized GET.
        // Never retry denied authorization, HTTP failures, TLS failures,
        // cancellation, malformed responses or output-limit violations.
        // Earlier verified downloads remain in this collection; only the
        // current incomplete response is discarded before its retry.
        if timed_out && *timeout_retries < 2 {
            *timeout_retries += 1;
            wait(Duration::from_secs(u64::from(*timeout_retries)))?;
            continue;
        }
        match out.termination {
            Termination::Cancelled => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "public read cancelled",
                ));
            }
            Termination::TimedOut => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!(
                        "public read timed out after {} bounded attempt(s)",
                        *timeout_retries + 1
                    ),
                ));
            }
            Termination::OutputLimit => return Err(invalid("public read exceeded bound")),
            Termination::Exited => (),
        }
        if !out.output.status.success() {
            return Err(public_http_failure_after_attempts(
                &format!("{kind:?}"),
                endpoint,
                out.output.status.code().unwrap_or(-1),
                &out.output.stdout,
                *timeout_retries + 1,
            ));
        }
        return Ok(out);
    }
}

fn authorized_curl_get(
    curl: &Path,
    request: &PublicRequest,
    budget: &Budget<'_>,
) -> Result<Vec<u8>> {
    budget.remaining()?;
    let validated = request.authorize_read()?;
    let mut destination = request.url();
    let mut timeout_retries = 0_u8;
    for hop in 0..=validated.max_https_redirects() {
        let mut cmd = Command::new(curl);
        public_network_environment(&mut cmd);
        // No -L: each Location is checked before the next process exists.
        cmd.current_dir("/").args([
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
            "--dump-header",
            "-",
            "--suppress-connect-headers",
            "--max-redirs",
            "0",
            "--url",
            &destination,
        ]);
        let out = read_with_timeout_retries(
            validated.kind(),
            &request.url(),
            &mut timeout_retries,
            || {
                let limits = ProcessLimits {
                    timeout: budget.remaining()?.min(Duration::from_secs(30)),
                    max_output_bytes: budget.max_bytes,
                };
                if validated.kind() == PublicReadKind::ArtifactContent {
                    cmd.bounded_artifact_capture(limits, budget.cancel)
                } else {
                    cmd.bounded_capture(limits, budget.cancel)
                }
            },
            |duration| budget.wait(duration),
        )?;
        let (status, location, body) = http_response(&out.output.stdout)?;
        if status == 200 {
            return Ok(body.to_vec());
        }
        if !matches!(status, 301 | 302 | 303 | 307 | 308) || hop >= validated.max_https_redirects()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "unsupported public {:?} HTTP status {status} at redirect hop {hop} (limit {})",
                    validated.kind(),
                    validated.max_https_redirects()
                ),
            ));
        }
        let location = location.ok_or_else(|| invalid("provider redirect location absent"))?;
        match validated.kind() {
            PublicReadKind::ArtifactContent => validate_storage_redirect(request, &location)?,
            PublicReadKind::JobLog => validate_log_redirect(request, &location)?,
            _ => return Err(invalid("redirect on non-content public read")),
        }
        destination = location;
    }
    Err(invalid("public artifact redirect limit"))
}

fn read(
    reader: &mut impl PublicReader,
    path: String,
    redirect: bool,
    budget: &Budget<'_>,
) -> Result<Vec<u8>> {
    budget.remaining()?;
    let request = PublicRequest {
        path,
        artifact_redirect: redirect,
        log_redirect: None,
    };
    read_request(reader, &request, budget)
}
fn read_request(
    reader: &mut (impl PublicReader + ?Sized),
    request: &PublicRequest,
    budget: &Budget<'_>,
) -> Result<Vec<u8>> {
    budget.remaining()?;
    request.authorize_read()?;
    let bytes = reader.get(request, budget)?;
    ensure(
        bytes.len() <= budget.max_bytes,
        "public response exceeded bound",
    )?;
    Ok(bytes)
}
/// Fresh ordered metadata reads. Validate the entire response batch before any
/// caller can interpret an envelope; partial results never become observations.
fn read_many(
    reader: &mut impl PublicReader,
    requests: &[PublicRequest],
    budget: &Budget<'_>,
) -> Result<Vec<Vec<u8>>> {
    budget.remaining()?;
    ensure(
        requests.len() <= PUBLIC_READ_CONCURRENCY && budget.max_bytes <= 16 * 1024 * 1024,
        "public metadata batch exceeds bound",
    )?;
    for request in requests {
        request.authorize_read()?;
    }
    let bytes = reader.get_many(requests, budget)?;
    ensure(
        bytes.len() == requests.len(),
        "public batch response count mismatch",
    )?;
    for body in &bytes {
        ensure(
            body.len() <= budget.max_bytes,
            "public response exceeded bound",
        )?;
    }
    budget.remaining()?;
    Ok(bytes)
}
fn metadata_request(path: String) -> PublicRequest {
    PublicRequest {
        path,
        artifact_redirect: false,
        log_redirect: None,
    }
}
fn json(bytes: &[u8]) -> Result<Value> {
    serde_json::from_slice(bytes).map_err(|_| invalid("invalid public JSON"))
}
fn field<'a>(v: &'a Value, key: &str) -> Result<&'a Value> {
    v.get(key)
        .ok_or_else(|| invalid("required public field missing"))
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    field(v, key)?
        .as_str()
        .ok_or_else(|| invalid("public field is not a string"))
}
fn count(v: &Value, key: &str) -> Result<u64> {
    field(v, key)?
        .as_u64()
        .ok_or_else(|| invalid("public count missing or malformed"))
}
fn boolean(v: &Value, key: &str) -> Result<bool> {
    field(v, key)?
        .as_bool()
        .ok_or_else(|| invalid("public boolean missing or malformed"))
}
fn array<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    field(v, key)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("public array missing"))
}
fn digest(v: &str) -> bool {
    v.strip_prefix("sha256:").is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[derive(Debug)]
pub struct InventoryExpectation {
    pub repository: Repository,
    pub pipeline_id: String,
    pub commit: String,
    pub branch: String,
    pub workflow_path: String,
    pub event: String,
}
#[derive(Debug)]
pub struct PublicJob {
    raw: Value,
}
impl PublicJob {
    pub fn id(&self) -> &str {
        self.raw["id"].as_str().unwrap()
    }
    pub fn step_key(&self) -> Option<&str> {
        self.raw["step_key"].as_str()
    }
    pub fn exit_status(&self) -> Option<i64> {
        self.raw["exit_status"].as_i64()
    }
    pub fn raw(&self) -> &Value {
        &self.raw
    }
}
#[derive(Debug)]
pub struct PublicInventory {
    build: PublicBuild,
    envelope: Value,
    steps: BTreeMap<String, Value>,
    jobs: BTreeMap<String, PublicJob>,
    response_digests: [String; 3],
}
impl PublicInventory {
    pub fn build_id(&self) -> &str {
        self.envelope["id"].as_str().unwrap()
    }
    pub fn jobs(&self) -> &BTreeMap<String, PublicJob> {
        &self.jobs
    }
    pub fn response_digests(&self) -> &[String; 3] {
        &self.response_digests
    }
    pub fn execution_fingerprint(&self) -> String {
        let project = |value: &Value, keys: &[&str]| -> Value {
            Value::Object(
                keys.iter()
                    .map(|key| ((*key).into(), value[*key].clone()))
                    .collect(),
            )
        };
        let jobs: Vec<_> = self
            .jobs
            .values()
            .map(|job| {
                project(
                    &job.raw,
                    &[
                        "id",
                        "step_uuid",
                        "step_key",
                        "type",
                        "state",
                        "exit_status",
                        "passed",
                        "soft_failed",
                        "stack_errored",
                        "will_fail",
                        "command",
                        "retried_at",
                        "retried_in_job_uuid",
                        "retry_pending",
                        "matrix",
                        "parallel_group_index",
                        "parallel_group_total",
                    ],
                )
            })
            .collect();
        let steps: Vec<_> = self.steps.values().map(|step| serde_json::json!({"identity":project(step,&["uuid","key","type","state","outcome","command_script","uploaded_by_step_uuid"]), "statistics":project(&step["statistics"],&["retries_count","jobs_count","jobs_finished_count","step_children_count","latest_job_id"])})).collect();
        hash(&serde_json::to_vec(&serde_json::json!({"build_id":self.build_id(),"state":self.envelope["state"],"blocked_state":self.envelope["blocked_state"],"steps":steps,"jobs":jobs})).expect("JSON values serialize"))
    }
    /// This explicitly does not say whether required imported logical jobs ran.
    pub fn all_native_scripts_passed(&self) -> bool {
        self.envelope["state"] == "passed"
            && self.envelope["blocked_state"].is_null()
            && self.jobs.values().all(|j| {
                j.raw["state"] == "finished"
                    && j.raw["passed"] == true
                    && j.raw["soft_failed"] == false
                    && j.raw["stack_errored"] == false
                    && j.raw["will_fail"] == false
                    && j.exit_status() == Some(0)
            })
    }
}
fn verify_envelope(
    envelope: &Value,
    build: &PublicBuild,
    expected: &InventoryExpectation,
) -> Result<()> {
    ensure(
        uuid(string(envelope, "id")?)
            && count(envelope, "number")? == build.number
            && string(envelope, "path")? == build.base()
            && string(envelope, "commit_id")? == expected.commit
            && string(envelope, "branch_name")? == expected.branch
            && string(envelope, "github_workflow_path")? == expected.workflow_path
            && string(envelope, "github_webhook_event")? == expected.event,
        "public build identity mismatch",
    )?;
    ensure(
        string(field(envelope, "account")?, "slug")? == build.organization,
        "public organization mismatch",
    )?;
    let project = field(envelope, "project")?;
    ensure(
        string(project, "id")? == expected.pipeline_id
            && string(project, "slug")? == build.pipeline
            && boolean(project, "public")?
            && string(field(project, "provider")?, "id")? == "github"
            && string(field(project, "provider")?, "url")?
                == format!("https://github.com/{}", expected.repository.full_name()),
        "public pipeline/repository mismatch",
    )?;
    // Require explicit blocked state presence even though null denotes unblocked.
    field(envelope, "blocked_state")?;
    Ok(())
}
/// Identity-authenticated build metadata only. Native/logical jobs and artifacts
/// are deliberately deferred until the whole source run set is terminal.
#[derive(Debug)]
pub struct PublicBuildStatus {
    build: PublicBuild,
    id: String,
    workflow: String,
    trigger_event: String,
    state: String,
    blocked: bool,
}
impl PublicBuildStatus {
    pub fn build(&self) -> &PublicBuild {
        &self.build
    }
    pub fn build_id(&self) -> &str {
        &self.id
    }
    pub fn workflow_path(&self) -> &str {
        &self.workflow
    }
    pub fn trigger_event(&self) -> &str {
        &self.trigger_event
    }
    pub fn state(&self) -> &str {
        &self.state
    }
    pub fn blocked(&self) -> bool {
        self.blocked
    }
}
pub fn discover_build_status(
    reader: &mut impl PublicReader,
    build: PublicBuild,
    snapshot: &snapshot::PreparedSnapshot,
    pipeline_id: &str,
    budget: &Budget<'_>,
) -> Result<PublicBuildStatus> {
    ensure(uuid(pipeline_id), "invalid reviewed pipeline UUID")?;
    let bytes = read(reader, format!("{}.json", build.base()), false, budget)?;
    verify_build_status(build, &bytes, snapshot, pipeline_id)
}
/// Authenticate every discovered build in a finite fresh batch. The provider
/// set remains independently discovered; batching never selects a winning run.
pub fn discover_build_statuses(
    reader: &mut impl PublicReader,
    builds: &[PublicBuild],
    snapshot: &snapshot::PreparedSnapshot,
    pipeline_id: &str,
    budget: &Budget<'_>,
) -> Result<Vec<PublicBuildStatus>> {
    ensure(uuid(pipeline_id), "invalid reviewed pipeline UUID")?;
    let requests: Vec<_> = builds
        .iter()
        .map(|build| metadata_request(format!("{}.json", build.base())))
        .collect();
    let bodies = read_many(reader, &requests, budget)?;
    builds
        .iter()
        .cloned()
        .zip(bodies)
        .map(|(build, body)| verify_build_status(build, &body, snapshot, pipeline_id))
        .collect()
}
fn verify_build_status(
    build: PublicBuild,
    bytes: &[u8],
    snapshot: &snapshot::PreparedSnapshot,
    pipeline_id: &str,
) -> Result<PublicBuildStatus> {
    let envelope = json(bytes)?;
    let workflow = string(&envelope, "github_workflow_path")?.to_owned();
    super::github_artifacts::workflow_name(&workflow)?;
    let trigger_event = string(&envelope, "github_webhook_event")?.to_owned();
    ensure(
        matches!(
            trigger_event.as_str(),
            "push" | "pull_request" | "workflow_dispatch"
        ),
        "unsupported public Buildkite trigger event",
    )?;
    let expected = InventoryExpectation {
        repository: snapshot.request().repository.clone(),
        pipeline_id: pipeline_id.into(),
        commit: snapshot.transport_head().into(),
        branch: snapshot
            .request()
            .transport_ref()
            .trim_start_matches("refs/heads/")
            .into(),
        workflow_path: workflow.clone(),
        // Authenticate the observed event alongside the immutable source before
        // the caller selects the independently required push contract.
        event: trigger_event.clone(),
    };
    verify_envelope(&envelope, &build, &expected)?;
    let state = string(&envelope, "state")?.to_owned();
    if !matches!(
        state.as_str(),
        "creating"
            | "scheduled"
            | "running"
            | "started"
            | "passed"
            | "failing"
            | "failed"
            | "canceling"
            | "canceled"
            | "blocked"
            | "not_run"
            | "skipped"
            | "broken"
    ) {
        // Unknown provider values remain errors. Bound and escape the value so
        // diagnostics cannot inject terminal controls or unbounded log content.
        let state_label: String = state.chars().take(64).collect();
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "unknown public Buildkite state: state={state_label:?} build={} uuid={} url=https://buildkite.com{}",
                build.number(),
                string(&envelope, "id")?,
                build.base(),
            ),
        ));
    }
    Ok(PublicBuildStatus {
        build,
        id: string(&envelope, "id")?.into(),
        workflow,
        trigger_event,
        state,
        blocked: !envelope["blocked_state"].is_null(),
    })
}
/// A complete unfiltered inventory. Any retries, non-script jobs, live mutation,
/// missing fields or ambiguous pagination are unsupported, never default green.
pub fn collect_inventory(
    reader: &mut impl PublicReader,
    build: PublicBuild,
    expected: &InventoryExpectation,
    budget: &Budget<'_>,
) -> Result<PublicInventory> {
    ensure(
        uuid(&expected.pipeline_id)
            && oid(&expected.commit)
            && matches!(
                expected.event.as_str(),
                "push" | "pull_request" | "workflow_dispatch"
            ),
        "invalid expected public identity",
    )?;
    let requests = [
        format!("{}.json", build.base()),
        format!("{}/data/steps", build.base()),
        format!(
            "{}/data/jobs?include_retried_jobs=true&paginate=false",
            build.base()
        ),
    ]
    .map(metadata_request);
    let [envelope_bytes, steps_bytes, jobs_bytes]: [Vec<u8>; 3] =
        read_many(reader, &requests, budget)?
            .try_into()
            .map_err(|_| invalid("public inventory response count mismatch"))?;
    let envelope = json(&envelope_bytes)?;
    verify_envelope(&envelope, &build, expected)?;
    let statistics = field(&envelope, "statistics")?;
    let job_count = count(statistics, "jobs_count")?;
    let step_count = count(statistics, "steps_count")?;
    ensure(
        job_count > 0 && job_count <= 2000 && step_count > 0 && step_count <= 2000,
        "public inventory exceeds supported bounds",
    )?;
    let step_json = json(&steps_bytes)?;
    let rows = step_json
        .as_array()
        .ok_or_else(|| invalid("unfiltered steps must be an array"))?;
    ensure(
        rows.len() as u64 == step_count,
        "incomplete public steps inventory",
    )?;
    let mut steps = BTreeMap::new();
    for step in rows {
        let id = string(step, "uuid")?;
        ensure(
            uuid(id)
                && string(step, "type")? == "command"
                && steps.insert(id.to_owned(), step.clone()).is_none(),
            "unsupported or duplicate public step",
        )?;
        let stats = field(step, "statistics")?;
        ensure(
            count(stats, "retries_count")? == 0
                && count(stats, "jobs_count")? == 1
                && count(stats, "step_children_count")? == 0,
            "public retries/groups/parallelism require a dedicated adapter",
        )?;
    }
    let job_json = json(&jobs_bytes)?;
    ensure(
        !boolean(&job_json, "has_next_page")? && !boolean(&job_json, "has_previous_page")?,
        "public jobs pagination incomplete",
    )?;
    let rows = array(&job_json, "records")?;
    ensure(
        rows.len() as u64 == job_count && job_count == step_count,
        "incomplete public job inventory",
    )?;
    let mut jobs = BTreeMap::new();
    let mut seen_steps = BTreeSet::new();
    for job in rows {
        let id = string(job, "id")?;
        let step_id = string(job, "step_uuid")?;
        ensure(
            uuid(id)
                && count(job, "build_number")? == build.number
                && string(job, "base_path")? == build.job_base(id)?
                && string(job, "type")? == "script"
                && seen_steps.insert(step_id.to_owned()),
            "public job identity mismatch",
        )?;
        let step = steps
            .get(step_id)
            .ok_or_else(|| invalid("public job refers to missing step"))?;
        ensure(
            field(job, "command")? == field(step, "command_script")?,
            "public command/step mismatch",
        )?;
        field(job, "matrix")?;
        field(job, "parallel_group_index")?;
        field(job, "parallel_group_total")?;
        ensure(
            field(job, "step_key")? == field(step, "key")?
                && string(field(step, "statistics")?, "latest_job_id")? == id,
            "public latest job/step mismatch",
        )?;
        ensure(
            field(job, "retried_at")?.is_null()
                && field(job, "retried_in_job_uuid")?.is_null()
                && !boolean(job, "retry_pending")?,
            "public retried jobs unsupported",
        )?;
        for key in ["passed", "soft_failed", "stack_errored", "will_fail"] {
            boolean(job, key)?;
        }
        ensure(
            field(job, "exit_status")?.is_null() || job["exit_status"].as_i64().is_some(),
            "invalid public numeric exit",
        )?;
        ensure(
            jobs.insert(id.to_owned(), PublicJob { raw: job.clone() })
                .is_none(),
            "duplicate public job",
        )?;
    }
    ensure(seen_steps.len() == steps.len(), "unmapped public steps")?;
    Ok(PublicInventory {
        build,
        envelope,
        steps,
        jobs,
        response_digests: [hash(&envelope_bytes), hash(&steps_bytes), hash(&jobs_bytes)],
    })
}

#[derive(Debug)]
pub struct CollectedArtifact {
    job_id: String,
    path: String,
    digest: String,
    metadata_digest: String,
    bytes: Vec<u8>,
}
impl CollectedArtifact {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn metadata_digest(&self) -> &str {
        &self.metadata_digest
    }
    pub fn job_id(&self) -> &str {
        &self.job_id
    }
}
fn artifact_rows(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<Vec<Value>> {
    ensure(
        inventory.jobs.contains_key(job),
        "artifact producer absent from complete inventory",
    )?;
    let bytes = read(
        reader,
        format!("{}/artifacts", inventory.build.job_base(job)?),
        false,
        budget,
    )?;
    let rows = json(&bytes)?
        .as_array()
        .cloned()
        .ok_or_else(|| invalid("public artifacts must be an array"))?;
    ensure(rows.len() <= 2000, "too many public artifacts")?;
    let mut ids = BTreeSet::new();
    for row in &rows {
        let id = string(row, "id")?;
        ensure(
            uuid(id)
                && ids.insert(id)
                && string(row, "url")?
                    == format!("{}/artifacts/{id}", inventory.build.job_base(job)?),
            "public artifact destination mismatch",
        )?;
    }
    Ok(rows)
}
fn download(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    row: &Value,
    budget: &Budget<'_>,
) -> Result<CollectedArtifact> {
    ensure(
        string(row, "state")? == "finished" && !boolean(row, "self_hosted")?,
        "unfinished/self-hosted artifact unsupported",
    )?;
    let expected = format!("sha256:{}", string(row, "sha256sum")?);
    ensure(digest(&expected), "public artifact lacks SHA256")?;
    let bytes = read(reader, string(row, "url")?.to_owned(), true, budget)?;
    ensure(
        hash(&bytes) == expected,
        "public artifact bytes differ from provider hash",
    )?;
    ensure(
        inventory.jobs.contains_key(job),
        "artifact producer missing",
    )?;
    Ok(CollectedArtifact {
        job_id: job.into(),
        path: string(row, "path")?.into(),
        digest: expected,
        metadata_digest: hash(
            &serde_json::to_vec(row)
                .map_err(|_| invalid("artifact metadata serialization failed"))?,
        ),
        bytes,
    })
}
/// Download one exact provider-owned job artifact and verify actual bytes. No
/// filesystem extraction, ZIP interpretation, execution or success import.
pub fn collect_artifact(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    path: &str,
    budget: &Budget<'_>,
) -> Result<CollectedArtifact> {
    ensure(
        !path.is_empty()
            && path.len() <= 1024
            && !path.starts_with('/')
            && !path.split('/').any(|p| p == ".." || p.is_empty()),
        "invalid artifact path",
    )?;
    let rows = artifact_rows(reader, inventory, job, budget)?;
    let selected: Vec<_> = rows.iter().filter(|r| r["path"] == path).collect();
    ensure(selected.len() == 1, "artifact missing or ambiguous")?;
    download(reader, inventory, job, selected[0], budget)
}
#[derive(Debug)]
pub struct RecipeExpectation {
    pub workflow_path: String,
    pub workflow_digest: String,
    pub logical_job: String,
    pub compiler_version: String,
    pub compiler_digest: String,
    pub runtime_digest: String,
    pub matrix: BTreeMap<String, Value>,
    pub job_index: u64,
    pub job_total: u64,
}
#[derive(Debug)]
pub struct ImportedRecipe {
    result: CollectedArtifact,
    plan: CollectedArtifact,
    event: CollectedArtifact,
    logical_result: String,
    plan_json: Value,
}
impl ImportedRecipe {
    pub fn result(&self) -> &CollectedArtifact {
        &self.result
    }
    pub fn plan(&self) -> &CollectedArtifact {
        &self.plan
    }
    pub fn event(&self) -> &CollectedArtifact {
        &self.event
    }
    pub fn logical_result(&self) -> &str {
        &self.logical_result
    }
    pub fn plan_json(&self) -> &Value {
        &self.plan_json
    }
}
/// Bind importer result -> actual producer -> plan -> raw source workflow and
/// event bytes. The provider-owned importer is the trusted compilation producer;
/// authenticated compiler digest declarations are not cryptographic compiler
/// attestations. The controller must independently pin the complete workflow
/// and local-action carrier closure. This is NOT per-step execution proof.
pub fn locate_recipe(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<Option<RecipeLocator>> {
    locate_recipe_result(reader, inventory, job, budget)?
        .map(|partial| finish_recipe_locator(reader, inventory, partial, budget))
        .transpose()
}
fn recipe_result_row(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<Option<Value>> {
    let job_record = inventory
        .jobs
        .get(job)
        .ok_or_else(|| invalid("recipe job absent"))?;
    let Some(step_key) = job_record.step_key() else {
        return Ok(None);
    };
    let rows = artifact_rows(reader, inventory, job, budget)?;
    let prefix = format!("buildkite-gha/v1/results/{step_key}/");
    let results: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["path"]
                .as_str()
                .is_some_and(|s| s.starts_with(&prefix) && s.ends_with(".json"))
        })
        .collect();
    if results.is_empty() {
        return Ok(None);
    }
    ensure(results.len() == 1, "importer result missing or ambiguous")?;
    Ok(Some(results[0].clone()))
}
fn locate_recipe_result(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<Option<RecipeResult>> {
    let Some(row) = recipe_result_row(reader, inventory, job, budget)? else {
        return Ok(None);
    };
    let job_record = &inventory.jobs[job];
    let step_key = job_record
        .step_key()
        .ok_or_else(|| invalid("recipe step key absent"))?;
    let prefix = format!("buildkite-gha/v1/results/{step_key}/");
    let result = download(reader, inventory, job, &row, budget)?;
    let manifest = json(result.bytes())?;
    ensure(
        string(&manifest, "schema")? == "buildkite-gha/result-manifest/v2",
        "unsupported importer result schema",
    )?;
    let producer = field(&manifest, "producer")?;
    ensure(
        string(producer, "build_id")? == inventory.build_id()
            && string(producer, "job_id")? == job
            && string(producer, "step_key")? == step_key,
        "importer result producer mismatch",
    )?;
    let plan_digest = string(&manifest, "plan_digest")?;
    ensure(
        digest(plan_digest) && result.path() == format!("{prefix}{}.json", &plan_digest[7..]),
        "importer plan result path mismatch",
    )?;
    let step = &inventory.steps[string(&job_record.raw, "step_uuid")?];
    let producer_step = string(step, "uploaded_by_step_uuid")?;
    let importer_jobs: Vec<_> = inventory
        .jobs
        .values()
        .filter(|j| j.raw["step_uuid"] == producer_step)
        .collect();
    ensure(
        importer_jobs.len() == 1,
        "importer upload producer missing or ambiguous",
    )?;
    let importer = importer_jobs[0].id();
    ensure(
        importer != job
            && field(&inventory.steps[producer_step], "uploaded_by_step_uuid")?.is_null(),
        "importer must be an independent root producer; deferred import stages unsupported",
    )?;
    let plan_digest = plan_digest.to_owned();
    Ok(Some(RecipeResult {
        result,
        importer: importer.into(),
        plan_digest,
    }))
}
struct RecipeResult {
    result: CollectedArtifact,
    importer: String,
    plan_digest: String,
}
fn finish_recipe_locator(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    partial: RecipeResult,
    budget: &Budget<'_>,
) -> Result<RecipeLocator> {
    let RecipeResult {
        result,
        importer,
        plan_digest,
    } = partial;
    let plan = collect_artifact(
        reader,
        inventory,
        &importer,
        &format!(".buildkite-gha/plans/{}.json", &plan_digest[7..]),
        budget,
    )?;
    ensure(
        plan.digest() == plan_digest,
        "importer plan digest mismatch",
    )?;
    let plan_json = json(plan.bytes())?;
    ensure(
        string(&plan_json, "schema")?
            == "https://buildkite.com/schemas/buildkite-gha/job-plan-v2.schema.json",
        "unsupported importer plan schema",
    )?;
    program::verify(&plan_json)?;
    let workflow = field(&plan_json, "workflow")?;
    let logical_job = string(workflow, "logical_job_id")?.to_owned();
    let matrix = plan_json
        .get("matrix")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let matrix =
        serde_json::from_value(matrix).map_err(|_| invalid("invalid importer matrix locator"))?;
    let strategy = field(&plan_json, "strategy")?;
    let job_index = count(strategy, "job-index")?;
    let job_total = count(strategy, "job-total")?;
    ensure(
        !logical_job.is_empty() && job_total > 0 && job_index < job_total,
        "invalid importer logical locator",
    )?;
    Ok(RecipeLocator {
        result,
        plan,
        plan_json,
        importer,
        logical_job,
        matrix,
        job_index,
        job_total,
    })
}
/// Verified provider result/plan custody used only to locate an independently
/// expected logical job. No caller JSON constructor or readiness conversion.
#[derive(Debug)]
pub struct RecipeLocator {
    result: CollectedArtifact,
    plan: CollectedArtifact,
    plan_json: Value,
    importer: String,
    logical_job: String,
    matrix: BTreeMap<String, Value>,
    job_index: u64,
    job_total: u64,
}
impl RecipeLocator {
    /// Reject an unsupported reviewed compiler before collecting other jobs.
    /// This checks policy against authenticated bytes; it never derives policy.
    pub fn verify_compiler(&self, version: &str, distribution_digest: &str) -> Result<()> {
        verify_compiler(&self.plan_json, version, distribution_digest)
    }
    pub fn verify_runtime(&self, distribution_digest: &str) -> Result<()> {
        ensure(
            digest(distribution_digest),
            "invalid expected runtime digest",
        )?;
        ensure(
            string(field(&self.plan_json, "runtime")?, "distribution_digest")?
                == distribution_digest,
            "importer runtime platform digest mismatch",
        )
    }
    pub fn producer_job_id(&self) -> &str {
        self.result.job_id()
    }
    pub fn logical_job(&self) -> &str {
        &self.logical_job
    }
    pub fn matrix(&self) -> &BTreeMap<String, Value> {
        &self.matrix
    }
    pub fn job_index(&self) -> u64 {
        self.job_index
    }
    pub fn job_total(&self) -> u64 {
        self.job_total
    }
}
fn verify_compiler(plan: &Value, version: &str, distribution_digest: &str) -> Result<()> {
    ensure(
        digest(distribution_digest),
        "invalid expected compiler digest",
    )?;
    ensure(
        string(field(plan, "compiler")?, "version")? == version,
        "importer compiler version mismatch",
    )?;
    ensure(
        string(field(plan, "compiler")?, "distribution_digest")? == distribution_digest,
        "importer compiler digest mismatch",
    )
}
pub fn collect_recipe(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    expected: &RecipeExpectation,
    budget: &Budget<'_>,
) -> Result<ImportedRecipe> {
    let locator = locate_recipe(reader, inventory, job, budget)?
        .ok_or_else(|| invalid("importer result missing"))?;
    collect_located_recipe(reader, inventory, locator, expected, budget)
}
/// Finish a previously located recipe against the frozen source contract,
/// without downloading its result or plan again.
pub fn collect_located_recipe(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    locator: RecipeLocator,
    expected: &RecipeExpectation,
    budget: &Budget<'_>,
) -> Result<ImportedRecipe> {
    ensure(
        digest(&expected.workflow_digest)
            && digest(&expected.compiler_digest)
            && digest(&expected.runtime_digest),
        "invalid expected recipe digests",
    )?;
    let RecipeLocator {
        result,
        plan,
        plan_json,
        importer,
        ..
    } = locator;
    let manifest = json(result.bytes())?;
    let job = result.job_id();
    let record = inventory
        .jobs
        .get(job)
        .ok_or_else(|| invalid("located producer absent from inventory"))?;
    let step_key = record
        .step_key()
        .ok_or_else(|| invalid("located producer lacks step key"))?;
    let producer = field(&manifest, "producer")?;
    ensure(
        string(producer, "build_id")? == inventory.build_id()
            && string(producer, "job_id")? == job
            && string(producer, "step_key")? == step_key,
        "located recipe does not belong to current inventory",
    )?;
    let workflow = field(&plan_json, "workflow")?;
    ensure(
        expected.workflow_path == string(&inventory.envelope, "github_workflow_path")?,
        "expected recipe workflow differs from provider workflow",
    )?;
    ensure(
        string(workflow, "path")?
            .strip_prefix("./")
            .unwrap_or(string(workflow, "path")?)
            == expected.workflow_path
            && string(workflow, "digest")? == expected.workflow_digest
            && string(workflow, "logical_job_id")? == expected.logical_job
            && string(field(&plan_json, "target")?, "step_key")? == step_key,
        "importer logical source mismatch",
    )?;
    verify_compiler(
        &plan_json,
        &expected.compiler_version,
        &expected.compiler_digest,
    )?;
    ensure(
        string(field(&plan_json, "runtime")?, "distribution_digest")? == expected.runtime_digest,
        "importer runtime platform digest mismatch",
    )?;
    let matrix = plan_json
        .get("matrix")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    ensure(
        matrix
            == serde_json::to_value(&expected.matrix)
                .map_err(|_| invalid("invalid expected matrix"))?,
        "importer logical matrix mismatch",
    )?;
    let strategy = field(&plan_json, "strategy")?;
    ensure(
        expected.job_total > 0
            && expected.job_index < expected.job_total
            && count(strategy, "job-index")? == expected.job_index
            && count(strategy, "job-total")? == expected.job_total,
        "importer logical parallel identity mismatch",
    )?;
    let event_description = field(&plan_json, "event")?;
    ensure(
        string(event_description, "provider")? == "github"
            && string(event_description, "name")?
                == string(&inventory.envelope, "github_webhook_event")?
            && string(event_description, "sha")? == string(&inventory.envelope, "commit_id")?
            && string(event_description, "repository")?
                == string(
                    field(field(&inventory.envelope, "project")?, "provider")?,
                    "url",
                )?
                .strip_prefix("https://github.com/")
                .ok_or_else(|| invalid("invalid repository URL"))?,
        "importer event identity mismatch",
    )?;
    let event_digest = string(event_description, "payload_digest")?;
    ensure(
        digest(event_digest) && boolean(event_description, "payload_artifact")?,
        "importer event payload absent",
    )?;
    let event = collect_artifact(
        reader,
        inventory,
        &importer,
        &format!(".buildkite-gha/events/{}.json", &event_digest[7..]),
        budget,
    )?;
    ensure(
        event.digest() == event_digest,
        "importer event digest mismatch",
    )?;
    let logical_result = string(&manifest, "result")?.to_owned();
    ensure(
        matches!(
            logical_result.as_str(),
            "success" | "failure" | "skipped" | "cancelled"
        ),
        "unknown importer logical result",
    )?;
    // Program shape is checked, not recompiled. The provider-owned importer is
    // trusted to compile the independently pinned workflow/action carrier. The
    // digest fields alone are not a compiler attestation or per-step proof.
    // Caller must still verify actual executor custody and setup/toolchain
    // closure before accepting any substantive row.
    Ok(ImportedRecipe {
        result,
        plan,
        event,
        logical_result,
        plan_json,
    })
}

/// Resolve a deterministic uploaded artifact name from the exact producer's
/// importer result, then authenticate the ZIP against both result and site hashes.
pub fn collect_named_archive(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    recipe: &ImportedRecipe,
    expected_name: &str,
    budget: &Budget<'_>,
) -> Result<super::artifact_archive::ArtifactFiles> {
    let selected = select_named_archive(reader, inventory, recipe, expected_name, budget)?;
    selected.collect(reader, budget)
}

#[derive(Clone, Debug)]
struct NamedArchiveSelection {
    request: PublicRequest,
    row: Value,
    job: String,
    origin: String,
    digest: String,
    size: usize,
    file_count: u64,
}
fn select_named_archive(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    recipe: &ImportedRecipe,
    expected_name: &str,
    budget: &Budget<'_>,
) -> Result<NamedArchiveSelection> {
    ensure(
        !expected_name.is_empty()
            && expected_name.len() <= 512
            && !expected_name.chars().any(char::is_control),
        "invalid expected artifact name",
    )?;
    let producer = inventory
        .jobs
        .get(recipe.result.job_id())
        .ok_or_else(|| invalid("source artifact producer missing"))?;
    ensure(
        recipe.logical_result() == "success"
            && producer.raw["state"] == "finished"
            && producer.raw["passed"] == true
            && producer.raw["soft_failed"] == false
            && producer.exit_status() == Some(0),
        "skipped/failed importer job cannot authenticate successful source execution",
    )?;
    let result = json(recipe.result.bytes())?;
    let entries: Vec<_> = array(&result, "artifacts")?
        .iter()
        .filter(|a| a["name"] == expected_name)
        .collect();
    ensure(
        entries.len() == 1,
        "named source artifact missing or ambiguous",
    )?;
    let entry = entries[0];
    let size = count(entry, "size")?;
    ensure(
        size > 0 && size <= 256 * 1024 * 1024,
        "named artifact exceeds archive bound",
    )?;
    let declared_digest = string(entry, "digest")?;
    ensure(digest(declared_digest), "named artifact digest invalid")?;
    // Metadata retains the ordinary API budget; only the exact selected blob
    // receives the independently declared artifact allowance.
    let producer_id = recipe.result.job_id();
    let rows = artifact_rows(reader, inventory, producer_id, budget)?;
    let expected_path = string(entry, "path")?;
    let selected: Vec<_> = rows.iter().filter(|r| r["path"] == expected_path).collect();
    ensure(
        selected.len() == 1,
        "named artifact provider path missing or ambiguous",
    )?;

    let row = selected[0];
    ensure(
        string(row, "state")? == "finished" && !boolean(row, "self_hosted")?,
        "unfinished/self-hosted artifact unsupported",
    )?;
    let provider_digest = format!("sha256:{}", string(row, "sha256sum")?);
    ensure(digest(&provider_digest), "public artifact lacks SHA256")?;
    ensure(
        inventory.jobs.contains_key(producer_id),
        "artifact producer missing",
    )?;
    let request = PublicRequest {
        path: string(row, "url")?.into(),
        artifact_redirect: true,
        log_redirect: None,
    };
    request.authorize_read()?;
    Ok(NamedArchiveSelection {
        request,
        row: row.clone(),
        job: producer_id.into(),
        origin: format!(
            "buildkite:{}/jobs/{}/{}",
            inventory.build_id(),
            producer_id,
            expected_name
        ),
        digest: declared_digest.into(),
        size: size as usize,
        file_count: count(entry, "file_count")?,
    })
}
impl NamedArchiveSelection {
    fn capture_limit(&self) -> usize {
        self.size + 64 * 1024
    }
    fn collect(
        &self,
        reader: &mut impl PublicReader,
        budget: &Budget<'_>,
    ) -> Result<super::artifact_archive::ArtifactFiles> {
        let artifact_budget = Budget {
            deadline: budget.deadline,
            cancel: budget.cancel,
            max_bytes: self.capture_limit(),
        };
        let bytes = read_request(reader, &self.request, &artifact_budget)?;
        self.verify(bytes, budget)
    }
    fn verify(
        &self,
        bytes: Vec<u8>,
        budget: &Budget<'_>,
    ) -> Result<super::artifact_archive::ArtifactFiles> {
        budget.remaining()?;
        let provider_digest = format!("sha256:{}", string(&self.row, "sha256sum")?);
        ensure(
            hash(&bytes) == provider_digest,
            "public artifact bytes differ from provider hash",
        )?;
        ensure(
            provider_digest == self.digest && bytes.len() == self.size,
            "named artifact bytes differ from importer result",
        )?;
        let files = super::artifact_archive::ArtifactFiles::verified(
            self.origin.clone(),
            &self.digest,
            &bytes,
            budget,
        )?;
        ensure(
            files.names().count() as u64 == self.file_count,
            "named artifact file inventory mismatch",
        )?;
        Ok(files)
    }
}

/// Actual raw provider log bytes for a job in the complete inventory. A log
/// heading is not a step completion or permission to synthesize an exit code.
#[derive(Debug)]
pub struct CollectedLog {
    job_id: String,
    digest: String,
    bytes: Vec<u8>,
}
impl CollectedLog {
    pub fn job_id(&self) -> &str {
        &self.job_id
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
fn log_request(inventory: &PublicInventory, job: &str) -> Result<PublicRequest> {
    ensure(
        inventory.jobs.contains_key(job),
        "log job absent from complete inventory",
    )?;
    Ok(PublicRequest {
        path: format!("{}/download.txt", inventory.build.job_base(job)?),
        artifact_redirect: false,
        log_redirect: Some(LogRedirectIdentity {
            pipeline: string(field(&inventory.envelope, "project")?, "id")?.into(),
            build: inventory.build_id().into(),
            job: job.into(),
        }),
    })
}
pub fn collect_log(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<CollectedLog> {
    let request = log_request(inventory, job)?;
    let bytes = read_request(reader, &request, budget)?;
    ensure(!bytes.is_empty(), "provider job log empty or unavailable")?;
    Ok(CollectedLog {
        job_id: job.into(),
        digest: hash(&bytes),
        bytes,
    })
}

/// Re-read the complete inventory immediately before accepting execution custody.
/// Compares execution-relevant fields, excluding changing presentation metadata
/// such as global hosted concurrency. Every fresh API body is separately hashed.
pub fn recheck_inventory(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    expected: &InventoryExpectation,
    budget: &Budget<'_>,
) -> Result<()> {
    let current = collect_inventory(reader, inventory.build.clone(), expected, budget)?;
    ensure(
        current.build_id() == inventory.build_id()
            && current.execution_fingerprint() == inventory.execution_fingerprint(),
        "public inventory changed during collection",
    )
}
#[cfg(test)]
pub(crate) mod tests;

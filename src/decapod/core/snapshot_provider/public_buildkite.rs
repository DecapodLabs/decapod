//! Dedicated, anonymous public-site reader. This is not the REST API schema.
//! Complete native job inventory and content-addressed importer provenance do
//! not prove substantive source checks. Only measured execution custody may do
//! that; this module deliberately cannot mint a readiness Observation.
use super::*;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

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
#[derive(Debug)]
pub struct PublicRequest {
    path: String,
    artifact_redirect: bool,
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
        Ok(AuthorizedPublicRead {
            kind,
            url: self.url(),
            scope: format!("buildkite:{organization}/{pipeline}/builds/{number}"),
            redirects: if self.artifact_redirect { 3 } else { 0 },
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
fn http_response(bytes: &[u8]) -> Result<(u16, Option<String>, &[u8])> {
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
        let validated = request.authorize_read()?;
        (self.authorize)(request, &self.curl)?;
        let mut destination = request.url();
        for hop in 0..=validated.max_https_redirects() {
            let mut cmd = Command::new(&self.curl);
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
                "--include",
                "--suppress-connect-headers",
                "--max-redirs",
                "0",
                "--url",
                &destination,
            ]);
            let limits = ProcessLimits {
                timeout: budget.remaining()?.min(Duration::from_secs(30)),
                max_output_bytes: budget.max_bytes,
            };
            let out = if validated.kind() == PublicReadKind::ArtifactContent {
                cmd.bounded_artifact_capture(limits, budget.cancel)?
            } else {
                cmd.bounded_capture(limits, budget.cancel)?
            };
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
                        "public read timed out",
                    ));
                }
                Termination::OutputLimit => return Err(invalid("public read exceeded bound")),
                Termination::Exited => (),
            }
            if !out.output.status.success() {
                return Err(io::Error::other(format!(
                    "public endpoint unavailable (curl exit {:?}); do not attempt login or bypass",
                    out.output.status.code()
                )));
            }
            let (status, location, body) = http_response(&out.output.stdout)?;
            if status == 200 {
                return Ok(body.to_vec());
            }
            ensure(
                matches!(status, 301 | 302 | 303 | 307 | 308)
                    && hop < validated.max_https_redirects(),
                "unsupported provider HTTP status or redirect limit",
            )?;
            let location = location.ok_or_else(|| invalid("provider redirect location absent"))?;
            validate_storage_redirect(request, &location)?;
            destination = location;
        }
        Err(invalid("public artifact redirect limit"))
    }
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
    };
    request.authorize_read()?;
    let bytes = reader.get(&request, budget)?;
    ensure(
        bytes.len() <= budget.max_bytes,
        "public response exceeded bound",
    )?;
    Ok(bytes)
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
    let envelope_bytes = read(reader, format!("{}.json", build.base()), false, budget)?;
    let steps_bytes = read(
        reader,
        format!("{}/data/steps", build.base()),
        false,
        budget,
    )?;
    let jobs_bytes = read(
        reader,
        format!(
            "{}/data/jobs?include_retried_jobs=true&paginate=false",
            build.base()
        ),
        false,
        budget,
    )?;
    let envelope = json(&envelope_bytes)?;
    ensure(
        uuid(string(&envelope, "id")?)
            && count(&envelope, "number")? == build.number
            && string(&envelope, "path")? == build.base()
            && string(&envelope, "commit_id")? == expected.commit
            && string(&envelope, "branch_name")? == expected.branch
            && string(&envelope, "github_workflow_path")? == expected.workflow_path
            && string(&envelope, "github_webhook_event")? == expected.event,
        "public build identity mismatch",
    )?;
    ensure(
        string(field(&envelope, "account")?, "slug")? == build.organization,
        "public organization mismatch",
    )?;
    let project = field(&envelope, "project")?;
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
    field(&envelope, "blocked_state")?;
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
/// event bytes. A successful result here is still NOT per-step execution proof.
pub fn locate_recipe(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<Option<RecipeLocator>> {
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
    let result = download(reader, inventory, job, results[0], budget)?;
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
    let plan = collect_artifact(
        reader,
        inventory,
        importer,
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
    Ok(Some(RecipeLocator {
        result,
        plan,
        plan_json,
        importer: importer.into(),
        logical_job,
        matrix,
        job_index,
        job_total,
    }))
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
        digest(&expected.workflow_digest) && digest(&expected.runtime_digest),
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
    ensure(
        string(field(&plan_json, "compiler")?, "version")? == expected.compiler_version
            && string(field(&plan_json, "compiler")?, "distribution_digest")?
                == expected.runtime_digest
            && string(field(&plan_json, "runtime")?, "distribution_digest")?
                == expected.runtime_digest,
        "importer compiler/runtime mismatch",
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
    // Program/actions/strategy remain measured data, never copied into an
    // independently expected contract. Caller must validate actual executor
    // attestations and setup/action/toolchain closure before accepting any row.
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
    // Shared cancellation/deadline; only this declared artifact capture receives
    // its finite size allowance. API response bounds are not raised globally.
    let artifact_budget = Budget {
        deadline: budget.deadline,
        cancel: budget.cancel,
        max_bytes: size as usize + 64 * 1024,
    };
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
    let artifact = download(
        reader,
        inventory,
        producer_id,
        selected[0],
        &artifact_budget,
    )?;
    ensure(
        artifact.digest() == declared_digest && artifact.bytes().len() as u64 == size,
        "named artifact bytes differ from importer result",
    )?;
    let files = super::artifact_archive::ArtifactFiles::verified(
        format!(
            "buildkite:{}/jobs/{}/{}",
            inventory.build_id(),
            recipe.result.job_id(),
            expected_name
        ),
        declared_digest,
        artifact.bytes(),
        budget,
    )?;
    ensure(
        files.names().count() as u64 == count(entry, "file_count")?,
        "named artifact file inventory mismatch",
    )?;
    Ok(files)
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
pub fn collect_log(
    reader: &mut impl PublicReader,
    inventory: &PublicInventory,
    job: &str,
    budget: &Budget<'_>,
) -> Result<CollectedLog> {
    ensure(
        inventory.jobs.contains_key(job),
        "log job absent from complete inventory",
    )?;
    let bytes = read(
        reader,
        format!("{}/download.txt", inventory.build.job_base(job)?),
        false,
        budget,
    )?;
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
mod tests;

//! Fresh authenticated provider records, never user-supplied green receipts.
//! The trusted planner defines every expected workflow/job/step before polling.
//! CLI transport does not prove workflow equivalence or artifact contents.
use super::snapshot::PreparedSnapshot;
use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

pub use super::readiness_evidence::Conclusion as State;
use super::readiness_evidence::{
    GITHUB_EXECUTION_SCHEMA_VERSION, GithubExecutionAttestation, GithubExecutionContract,
    GithubFailureMode, GithubStepObservation,
};
#[derive(Clone, Debug)]
pub struct GithubJob {
    pub key: String,
    /// Trusted independently reviewed contract; never obtained from observations.
    pub contract: GithubExecutionContract,
}
#[derive(Clone, Debug)]
pub struct GithubWorkflow {
    pub id: u64,
    pub path: String,
    pub jobs: Vec<GithubJob>,
}
#[derive(Clone, Debug)]
pub struct BuildkiteJob {
    pub key: String,
    pub step_key: String,
    pub parallel_index: Option<u64>,
    pub parallel_total: Option<u64>,
    pub matrix: Option<BTreeMap<String, String>>,
    /// Hash of the independently expected exact provider script; never a label.
    pub command_digest: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildkiteSource {
    Api,
    Webhook,
}
impl BuildkiteSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Webhook => "webhook",
        }
    }
}
#[derive(Clone, Debug)]
pub struct BuildkitePipeline {
    pub organization: String,
    pub slug: String,
    pub id: String,
    pub source: BuildkiteSource,
    pub jobs: Vec<BuildkiteJob>,
}
/// Only executing remote obligations belong here; native/nonapplicable rows
/// remain with the independently complete readiness planner. Key strings are
/// opaque planner identifiers, not shell fragments or provider-derived names.
#[derive(Clone, Debug)]
pub struct RemotePlan {
    pub github: Vec<GithubWorkflow>,
    pub buildkite: Vec<BuildkitePipeline>,
}
impl RemotePlan {
    pub fn validate(&self) -> Result<()> {
        ensure(
            !self.github.is_empty() || !self.buildkite.is_empty(),
            "empty remote plan",
        )?;
        ensure(
            self.github.len() + self.buildkite.len() <= 64,
            "too many remote workflows",
        )?;
        let mut ids = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut job_count = 0;
        for w in &self.github {
            ensure(
                w.id > 0
                    && w.path.starts_with(".github/workflows/")
                    && (w.path.ends_with(".yml") || w.path.ends_with(".yaml"))
                    && w.path.len() <= 256
                    && !w.path.contains("..")
                    && w.path.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.')
                    }),
                "invalid workflow identity",
            )?;
            ensure(
                ids.insert(format!("gh-id:{}", w.id))
                    && ids.insert(format!("gh-path:{}", w.path))
                    && !w.jobs.is_empty(),
                "duplicate or empty workflow",
            )?;
            let mut names = BTreeSet::new();
            for j in &w.jobs {
                job_count += 1;
                let c = &j.contract;
                ensure(
                    label(&j.key)
                        && keys.insert(&j.key)
                        && label(&c.provider_job_name)
                        && names.insert(&c.provider_job_name),
                    "invalid duplicate GitHub obligation",
                )?;
                ensure(
                    c.schema_version == GITHUB_EXECUTION_SCHEMA_VERSION
                        && c.workflow == w.path
                        && label(&c.repository)
                        && label(&c.job)
                        && label(&c.profile)
                        && hex_digest(&c.source_definition_digest),
                    "invalid reviewed GitHub contract",
                )?;
                ensure(
                    oid(&c.source_head)
                        && oid(&c.accepted_base_revision)
                        && oid(&c.transport_revision)
                        && oid(&c.source_tree),
                    "invalid contract Git object binding",
                )?;
                ensure(
                    !c.required_steps.is_empty() && c.required_steps.len() <= 1000,
                    "empty or oversized required-step contract",
                )?;
                let mut steps = BTreeSet::new();
                for step in &c.required_steps {
                    ensure(
                        label(&step.name)
                            && steps.insert(&step.name)
                            && hex_digest(&step.definition_digest)
                            && step.failure_mode == GithubFailureMode::RequiredTerminalSuccess,
                        "masked/unknown/duplicate required step unsupported",
                    )?;
                }
            }
        }
        for p in &self.buildkite {
            ensure(
                slug(&p.organization)
                    && slug(&p.slug)
                    && uuid(&p.id)
                    && !p.jobs.is_empty()
                    && ids.insert(format!("bk:{}/{}", p.organization, p.slug)),
                "invalid pipeline identity",
            )?;
            let mut selectors = BTreeSet::new();
            for j in &p.jobs {
                job_count += 1;
                ensure(
                    label(&j.key)
                        && label(&j.step_key)
                        && keys.insert(&j.key)
                        && hex_digest(&j.command_digest),
                    "invalid or duplicate Buildkite job",
                )?;
                ensure(
                    matches!((j.parallel_index, j.parallel_total), (None, None))
                        || matches!((j.parallel_index,j.parallel_total),(Some(i),Some(n)) if n>0 && i<n),
                    "invalid parallel identity",
                )?;
                let s = serde_json::to_string(&(
                    &j.step_key,
                    j.parallel_index,
                    j.parallel_total,
                    &j.matrix,
                ))
                .map_err(|_| invalid("invalid job selector"))?;
                ensure(selectors.insert(s), "duplicate Buildkite selector")?;
            }
        }
        ensure(job_count <= 2000, "remote job budget exceeded")
    }
}
fn label(v: &str) -> bool {
    !v.is_empty() && v.len() <= 512 && !v.chars().any(char::is_control)
}
fn hex_digest(v: &str) -> bool {
    v.strip_prefix("sha256:").is_some_and(|v| {
        v.len() == 64
            && v.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// No Deserialize or public fields: only this collector can construct records.
/// API response hashes are evidence of the observed status payload, not log or
/// built-artifact hashes. In particular this cannot satisfy CandidateBuild by itself.
#[derive(Debug, Clone, Serialize)]
pub struct AuthenticatedRow {
    key: String,
    execution_id: String,
    state: State,
    exit_code: Option<i32>,
    response_digest: String,
    log_digest: Option<String>,
    github_attestation: Option<GithubExecutionAttestation>,
}
impl AuthenticatedRow {
    /// This is an execution observation, not a complete check result or readiness
    /// verdict. The caller still binds independently measured current identity,
    /// actual toolchain, plan digests and any required artifact bytes.
    pub fn observation(&self) -> Result<readiness_evidence::Observation> {
        ensure(
            self.state == State::Success,
            "failed or incomplete provider observation",
        )?;
        if let Some(attestation) = &self.github_attestation {
            return Ok(readiness_evidence::Observation::GithubActions {
                attestation: Box::new(attestation.clone()),
            });
        }
        ensure(
            self.exit_code == Some(0),
            "numeric provider exit unavailable",
        )?;
        let log = self
            .log_digest
            .as_ref()
            .ok_or_else(|| invalid("measured exact-job logs unavailable"))?;
        Ok(readiness_evidence::Observation::Executed {
            execution_id: self.execution_id.clone(),
            conclusion: self.state,
            exit_code: self.exit_code,
            output_digest: log.clone(),
            artifact_digest: None,
        })
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn execution_id(&self) -> &str {
        &self.execution_id
    }
    pub fn response_digest(&self) -> &str {
        &self.response_digest
    }
    pub fn github_attestation(&self) -> Option<&GithubExecutionAttestation> {
        self.github_attestation.as_ref()
    }
    pub fn log_digest(&self) -> Option<&str> {
        self.log_digest.as_deref()
    }
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub rows: Vec<AuthenticatedRow>,
    pub complete: bool,
    pub all_passed: bool,
}

#[derive(Debug, Deserialize)]
struct Repo {
    full_name: String,
}
#[derive(Debug, Deserialize)]
struct GhRun {
    id: u64,
    run_attempt: u64,
    workflow_id: u64,
    path: String,
    head_sha: String,
    head_branch: String,
    event: String,
    status: String,
    conclusion: Option<String>,
    repository: Repo,
    head_repository: Repo,
    #[serde(skip)]
    api_digest: String,
}
#[derive(Debug, Deserialize)]
struct GhRuns {
    total_count: usize,
    workflow_runs: Vec<GhRun>,
}
#[derive(Debug, Deserialize)]
struct GhStep {
    name: String,
    status: String,
    conclusion: Option<String>,
    number: u64,
}
#[derive(Debug, Deserialize)]
struct GhJob {
    id: u64,
    run_id: u64,
    run_attempt: Option<u64>,
    head_sha: String,
    name: String,
    status: String,
    conclusion: Option<String>,
    steps: Vec<GhStep>,
}
#[derive(Debug, Deserialize)]
struct GhJobs {
    total_count: usize,
    jobs: Vec<GhJob>,
}
#[derive(Debug, Deserialize)]
struct GitObject {
    sha: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}
#[derive(Debug, Deserialize)]
struct GitCommit {
    sha: String,
    tree: GitObject,
    parents: Vec<GitObject>,
    message: String,
}
#[derive(Debug, Deserialize)]
struct GitRef {
    #[serde(rename = "ref")]
    reference: String,
    object: GitObject,
}
#[derive(Debug, Deserialize)]
struct BkPipeline {
    id: String,
    slug: String,
    repository: String,
    url: String,
}
#[derive(Debug, Deserialize)]
struct BkBuild {
    id: String,
    number: u64,
    commit: String,
    branch: String,
    source: String,
    state: String,
    blocked: bool,
    pipeline: BkPipeline,
    #[serde(default)]
    jobs: Vec<BkJob>,
    job_state_counts: Option<BkCounts>,
    #[serde(default)]
    meta_data: BTreeMap<String, serde_json::Value>,
}
#[derive(Debug, Deserialize)]
struct BkCounts {
    total: usize,
    states: BTreeMap<String, usize>,
}
#[derive(Debug, Deserialize)]
struct BkJob {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    step_key: Option<String>,
    state: String,
    exit_status: Option<i32>,
    soft_failed: Option<bool>,
    command: Option<String>,
    parallel_group_index: Option<u64>,
    parallel_group_total: Option<u64>,
    matrix: Option<BTreeMap<String, String>>,
    #[serde(default)]
    retried: bool,
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|_| invalid("provider JSON schema mismatch"))
}
pub(crate) fn gh_spec(root: &Path, endpoint: &str) -> CommandSpec {
    CommandSpec {
        program: Program::Github,
        args: vec![
            "api".into(),
            "--hostname".into(),
            "github.com".into(),
            "--method".into(),
            "GET".into(),
            "--header".into(),
            "Accept: application/vnd.github+json".into(),
            endpoint.into(),
        ],
        cwd: root.into(),
        organization: None,
        provider_request: Some(ProviderRequest::GithubGet {
            path: endpoint.into(),
        }),
    }
}
fn gh<T: serde::de::DeserializeOwned>(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    endpoint: &str,
    b: &Budget<'_>,
) -> Result<(T, String)> {
    let bytes = checked(runner, gh_spec(s.root(), endpoint), b)?;
    Ok((decode(&bytes)?, hash(&bytes)))
}
fn github_log(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    job_id: u64,
    b: &Budget<'_>,
) -> Result<String> {
    ensure(job_id > 0, "invalid GitHub job log identity")?;
    let mut spec = gh_spec(
        s.root(),
        &format!(
            "{}/actions/jobs/{job_id}/logs",
            s.request().repository.api()
        ),
    );
    spec.provider_request = Some(ProviderRequest::GithubJobLog {
        repository: s.request().repository.clone(),
        job_id,
    });
    let bytes = checked(runner, spec, b)?;
    ensure(
        !bytes.is_empty(),
        "successful job has no measured log content",
    )?;
    Ok(hash(&bytes))
}
fn bk_spec(root: &Path, p: &BuildkitePipeline, endpoint: &str) -> CommandSpec {
    CommandSpec {
        program: Program::Buildkite,
        args: vec![
            "api".into(),
            "--no-input".into(),
            "--method".into(),
            "GET".into(),
            endpoint.into(),
        ],
        cwd: root.into(),
        organization: Some(p.organization.clone()),
        provider_request: Some(ProviderRequest::BuildkiteGet {
            organization: p.organization.clone(),
            path: endpoint.into(),
        }),
    }
}
fn bk<T: serde::de::DeserializeOwned>(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    p: &BuildkitePipeline,
    endpoint: &str,
    b: &Budget<'_>,
) -> Result<(T, String)> {
    let bytes = checked(runner, bk_spec(s.root(), p, endpoint), b)?;
    Ok((decode(&bytes)?, hash(&bytes)))
}

/// Check both immutable Git objects and current provider refs. GitHub's message
/// projection may omit the final LF; the locally checked exact S hash remains
/// authoritative for the raw encoding. No other normalization is accepted.
pub fn verify_remote_binding(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    b: &Budget<'_>,
) -> Result<()> {
    super::snapshot::verify_remote_tags(runner, s, b)?;
    let r = s.request();
    let api = r.repository.api();
    let (transport, _): (GitCommit, _) = gh(
        runner,
        s,
        &format!("{api}/git/commits/{}", s.transport_head()),
        b,
    )?;
    let message = r.message()?;
    ensure(
        transport.sha == s.transport_head()
            && transport.tree.sha == r.source_tree
            && transport.parents.len() == 1
            && transport.parents[0].sha == r.source_head
            && (transport.message == message
                || transport.message == message.strip_suffix('\n').unwrap()),
        "remote transport binding mismatch",
    )?;
    let (source, _): (GitCommit, _) = gh(
        runner,
        s,
        &format!("{api}/git/commits/{}", r.source_head),
        b,
    )?;
    ensure(
        source.sha == r.source_head && source.tree.sha == r.source_tree,
        "remote source tree mismatch",
    )?;
    for (reference, expected) in [
        (r.transport_ref(), s.transport_head()),
        (r.accepted_base_ref.clone(), r.accepted_base.as_str()),
    ] {
        let suffix = reference
            .strip_prefix("refs/")
            .ok_or_else(|| invalid("invalid ref"))?;
        let endpoint = format!(
            "{api}/git/ref/{}",
            suffix
                .split('/')
                .map(encode_path)
                .collect::<Vec<_>>()
                .join("/")
        );
        let (actual, _): (GitRef, _) = gh(runner, s, &endpoint, b)?;
        ensure(
            actual.reference == reference
                && actual.object.sha == expected
                && actual.object.kind.as_deref() == Some("commit"),
            "accepted base or transport ref changed",
        )?;
    }
    Ok(())
}
fn gh_state(status: &str, conclusion: Option<&str>) -> Result<State> {
    match (status, conclusion) {
        ("queued" | "requested" | "waiting" | "pending", None) => Ok(State::Pending),
        ("in_progress", None) => Ok(State::Running),
        ("completed", Some("success")) => Ok(State::Success),
        ("completed", Some("skipped" | "neutral")) => Ok(State::Skipped),
        ("completed", Some("cancelled")) => Ok(State::Cancelled),
        ("completed", Some("timed_out")) => Ok(State::TimedOut),
        ("completed", Some("failure" | "action_required" | "stale" | "startup_failure")) => {
            Ok(State::Failure)
        }
        _ => Err(invalid("unknown GitHub execution state")),
    }
}
fn bk_state(status: &str) -> Result<State> {
    match status {
        "creating" | "scheduled" | "waiting" | "pending" | "assigned" | "accepted" | "blocked" => {
            Ok(State::Pending)
        }
        "running" | "limiting" | "waiting_failed" | "canceling" | "failing" => Ok(State::Running),
        "passed" => Ok(State::Success),
        "failed" | "broken" => Ok(State::Failure),
        "canceled" => Ok(State::Cancelled),
        "skipped" | "not_run" => Ok(State::Skipped),
        _ => Err(invalid("unknown Buildkite execution state")),
    }
}
fn terminal(s: &State) -> bool {
    !matches!(s, State::Pending | State::Running)
}
#[derive(Deserialize)]
struct BkLog {
    url: String,
    content: String,
    size: usize,
}
fn buildkite_logs(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    p: &BuildkitePipeline,
    build: &BkBuild,
    rows: &mut [AuthenticatedRow],
    b: &Budget<'_>,
) -> Result<()> {
    for row in rows.iter_mut().filter(|row| row.state == State::Success) {
        let job_id = row
            .execution_id
            .rsplit('/')
            .next()
            .ok_or_else(|| invalid("job log identity missing"))?;
        ensure(uuid(job_id), "invalid exact-job log ID")?;
        let endpoint = format!(
            "/pipelines/{}/builds/{}/jobs/{job_id}/log",
            p.slug, build.number
        );
        let (log, _): (BkLog, _) = bk(runner, s, p, &endpoint, b)?;
        ensure(
            log.url
                == format!(
                    "https://api.buildkite.com/v2/organizations/{}{}",
                    p.organization, endpoint
                )
                && log.content.len() == log.size,
            "Buildkite log identity or completeness mismatch",
        )?;
        row.log_digest = Some(hash(log.content.as_bytes()));
    }
    Ok(())
}
fn check_run(run: &GhRun, s: &PreparedSnapshot, w: &GithubWorkflow) -> Result<()> {
    ensure(
        run.id > 0
            && run.run_attempt > 0
            && run.workflow_id == w.id
            && run.path == w.path
            && run.head_sha == s.transport_head()
            && run.head_branch
                == s.request()
                    .transport_ref()
                    .strip_prefix("refs/heads/")
                    .unwrap()
            && run.event == "push"
            && run.repository.full_name == s.request().repository.full_name()
            && run.head_repository.full_name == s.request().repository.full_name(),
        "GitHub run identity mismatch",
    )
}
fn discover_gh(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    w: &GithubWorkflow,
    b: &Budget<'_>,
) -> Result<Option<GhRun>> {
    let mut all = vec![];
    let mut expected_total = None;
    for page in 1..=10 {
        let endpoint = format!(
            "{}/actions/workflows/{}/runs?head_sha={}&event=push&branch={}&per_page=100&page={page}",
            s.request().repository.api(),
            w.id,
            s.transport_head(),
            encode_path(
                s.request()
                    .transport_ref()
                    .strip_prefix("refs/heads/")
                    .unwrap()
            )
        );
        let (mut response, response_digest): (GhRuns, _) = gh(runner, s, &endpoint, b)?;
        for run in &mut response.workflow_runs {
            run.api_digest = response_digest.clone();
        }
        ensure(
            response.total_count <= 1000
                && expected_total.is_none_or(|n| n == response.total_count),
            "GitHub run pagination changed or exceeded bound",
        )?;
        expected_total = Some(response.total_count);
        ensure(response.workflow_runs.len() <= 100, "invalid GitHub page")?;
        all.extend(response.workflow_runs);
        if all.len() >= response.total_count {
            ensure(all.len() == response.total_count, "run page count mismatch")?;
            break;
        }
        ensure(page < 10, "GitHub run pagination incomplete")?;
    }
    ensure(
        all.len() <= 1,
        "multiple runs for one snapshot workflow; refuse ambiguous custody",
    )?;
    let run = all.pop();
    if let Some(r) = &run {
        check_run(r, s, w)?;
    }
    Ok(run)
}
fn collect_gh(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    w: &GithubWorkflow,
    run: &GhRun,
    b: &Budget<'_>,
) -> Result<(Vec<AuthenticatedRow>, bool)> {
    check_run(run, s, w)?;
    let state = gh_state(&run.status, run.conclusion.as_deref())?;
    let mut jobs = vec![];
    let mut digest_material = run.api_digest.as_bytes().to_vec();
    let mut expected_total = None;
    for page in 1..=20 {
        let endpoint = format!(
            "{}/actions/runs/{}/attempts/{}/jobs?per_page=100&page={page}",
            s.request().repository.api(),
            run.id,
            run.run_attempt
        );
        let (response, digest): (GhJobs, _) = gh(runner, s, &endpoint, b)?;
        ensure(
            response.total_count <= 2000
                && expected_total.is_none_or(|n| n == response.total_count),
            "job pagination changed or exceeded bound",
        )?;
        expected_total = Some(response.total_count);
        digest_material.extend_from_slice(digest.as_bytes());
        ensure(response.jobs.len() <= 100, "invalid job page")?;
        jobs.extend(response.jobs);
        if jobs.len() >= response.total_count {
            ensure(
                jobs.len() == response.total_count,
                "job page count mismatch",
            )?;
            break;
        }
        ensure(page < 20, "job pagination incomplete")?;
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    for j in &jobs {
        ensure(
            j.id > 0
                && ids.insert(j.id)
                && names.insert(&j.name)
                && j.run_id == run.id
                && j.run_attempt
                    .is_none_or(|attempt| attempt == run.run_attempt)
                && j.head_sha == s.transport_head(),
            "duplicate or mismatched GitHub job",
        )?;
    }
    let mut rows = vec![];
    for expected in &w.jobs {
        let Some(j) = jobs
            .iter()
            .find(|j| j.name == expected.contract.provider_job_name)
        else {
            if terminal(&state) {
                return Err(invalid("terminal workflow omitted required job"));
            }
            continue;
        };
        let mut job_state = gh_state(&j.status, j.conclusion.as_deref())?;
        let mut observed_steps = vec![];
        if job_state == State::Success {
            let mut names = BTreeSet::new();
            let mut last_number = 0;
            for step in &j.steps {
                ensure(
                    names.insert(&step.name) && step.number > last_number,
                    "ambiguous or unordered GitHub steps",
                )?;
                last_number = step.number;
            }
            for step in &j.steps {
                if expected
                    .contract
                    .required_steps
                    .iter()
                    .any(|required| required.name == step.name)
                {
                    observed_steps.push(GithubStepObservation {
                        name: step.name.clone(),
                        conclusion: gh_state(&step.status, step.conclusion.as_deref())?,
                    });
                }
            }
            ensure(
                observed_steps.len() == expected.contract.required_steps.len()
                    && observed_steps
                        .iter()
                        .zip(&expected.contract.required_steps)
                        .all(|(actual, required)| actual.name == required.name),
                "successful job omitted or reordered substantive required steps",
            )?;
            if let Some(step) = observed_steps
                .iter()
                .find(|step| step.conclusion != State::Success)
            {
                job_state = if terminal(&step.conclusion) {
                    step.conclusion
                } else {
                    State::Failure
                };
            }
        }
        let response_digest = hash(&digest_material);
        let log_digest = if state == State::Success && job_state == State::Success {
            Some(github_log(runner, s, j.id, b)?)
        } else {
            None
        };
        let github_attestation = if let Some(log) = &log_digest {
            ensure(
                hex_digest(&run.api_digest),
                "run API custody digest unavailable",
            )?;
            Some(GithubExecutionAttestation {
                contract: expected.contract.clone(),
                run_id: run.id,
                run_attempt: u32::try_from(run.run_attempt)
                    .map_err(|_| invalid("GitHub attempt exceeds bound"))?,
                job_id: j.id,
                observed_run_head: run.head_sha.clone(),
                observed_job_name: j.name.clone(),
                run_conclusion: state,
                job_conclusion: job_state,
                steps: observed_steps,
                response_digest: response_digest.clone(),
                log_digest: log.clone(),
                artifact_digest: None,
            })
        } else {
            None
        };
        rows.push(AuthenticatedRow {
            key: expected.key.clone(),
            execution_id: format!(
                "github:{}/{}/{}/{}",
                s.request().repository.full_name(),
                run.id,
                run.run_attempt,
                j.id
            ),
            state: job_state,
            exit_code: None,
            response_digest,
            log_digest,
            github_attestation,
        });
    }
    if terminal(&state) && state != State::Success {
        return Err(invalid("GitHub workflow ended without success"));
    }
    Ok((rows, terminal(&state)))
}
fn check_build(build: &BkBuild, s: &PreparedSnapshot, p: &BuildkitePipeline) -> Result<()> {
    let name = s.request().repository.full_name();
    let pipeline_url = format!(
        "https://api.buildkite.com/v2/organizations/{}/pipelines/{}",
        p.organization, p.slug
    );
    let repos = [
        format!("https://github.com/{name}.git"),
        format!("https://github.com/{name}"),
        format!("git@github.com:{name}.git"),
        format!("git@github.com:{name}"),
    ];
    ensure(
        uuid(&build.id)
            && build.number > 0
            && build.commit == s.transport_head()
            && build.branch
                == s.request()
                    .transport_ref()
                    .strip_prefix("refs/heads/")
                    .unwrap()
            && build.source == p.source.as_str()
            && build.pipeline.id == p.id
            && build.pipeline.slug == p.slug
            && build.pipeline.url == pipeline_url
            && repos.contains(&build.pipeline.repository),
        "Buildkite build identity mismatch",
    )?;
    if p.source == BuildkiteSource::Api {
        for (key, value) in [
            ("decapod-request", s.request().request_id.as_str()),
            ("decapod-source", s.request().source_head.as_str()),
            ("decapod-base", s.request().accepted_base.as_str()),
            ("decapod-tree", s.request().source_tree.as_str()),
        ] {
            ensure(
                build.meta_data.get(key).and_then(|v| v.as_str()) == Some(value),
                "Buildkite API build request metadata mismatch",
            )?;
        }
    }
    Ok(())
}
fn discover_bk(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    p: &BuildkitePipeline,
    b: &Budget<'_>,
) -> Result<Option<BkBuild>> {
    let mut all = vec![];
    for page in 1..=10 {
        let endpoint = format!(
            "/pipelines/{}/builds?commit={}&branch={}&per_page=100&page={page}",
            p.slug,
            s.transport_head(),
            encode_path(
                s.request()
                    .transport_ref()
                    .strip_prefix("refs/heads/")
                    .unwrap()
            )
        );
        let (response, _): (Vec<BkBuild>, _) = bk(runner, s, p, &endpoint, b)?;
        let n = response.len();
        ensure(n <= 100, "invalid Buildkite page")?;
        all.extend(response);
        if n < 100 {
            break;
        }
        ensure(page < 10, "Buildkite build pagination incomplete")?;
    }
    ensure(
        all.len() <= 1,
        "multiple builds for one snapshot pipeline; refuse ambiguous custody",
    )?;
    let build = all.pop();
    if let Some(v) = &build {
        check_build(v, s, p)?;
    }
    Ok(build)
}
fn collect_bk(
    build: &BkBuild,
    digest: String,
    s: &PreparedSnapshot,
    p: &BuildkitePipeline,
) -> Result<(Vec<AuthenticatedRow>, bool)> {
    check_build(build, s, p)?;
    let state = bk_state(&build.state)?;
    // API may retain passed while blocked=true. It is never green in that case.
    ensure(
        !build.blocked,
        "Buildkite build is blocked, including passed-but-blocked states",
    )?;
    let done = terminal(&state);
    let counts = build
        .job_state_counts
        .as_ref()
        .ok_or_else(|| invalid("Buildkite job inventory counts absent"))?;
    ensure(
        counts.total <= 2000
            && counts.total == build.jobs.len()
            && counts
                .states
                .values()
                .try_fold(0usize, |sum, value| sum.checked_add(*value))
                == Some(counts.total),
        "Buildkite returned partial job inventory",
    )?;
    let mut observed_counts = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for j in &build.jobs {
        ensure(
            uuid(&j.id) && ids.insert(&j.id) && !j.retried,
            "duplicate or retired job execution",
        )?;
        *observed_counts.entry(j.state.clone()).or_insert(0) += 1;
    }
    ensure(
        observed_counts == counts.states,
        "Buildkite job state inventory mismatch",
    )?;
    let mut rows = vec![];
    for expected in &p.jobs {
        let matching = build
            .jobs
            .iter()
            .filter(|j| {
                j.step_key.as_deref() == Some(expected.step_key.as_str())
                    && j.parallel_group_index == expected.parallel_index
                    && j.parallel_group_total == expected.parallel_total
                    && j.matrix == expected.matrix
            })
            .collect::<Vec<_>>();
        ensure(matching.len() <= 1, "ambiguous Buildkite step execution")?;
        let Some(job) = matching.first() else {
            if done {
                return Err(invalid("terminal build omitted required script job"));
            }
            continue;
        };
        ensure(
            job.kind == "script",
            "Buildkite wrapper cannot replace substantive script",
        )?;
        ensure(
            job.command
                .as_ref()
                .is_some_and(|c| hash(c.as_bytes()) == expected.command_digest),
            "Buildkite script differs from independently reviewed invocation",
        )?;
        let mut actual = bk_state(&job.state)?;
        if actual == State::Success
            && (job.exit_status != Some(0) || job.soft_failed != Some(false))
        {
            actual = State::Failure;
        }
        rows.push(AuthenticatedRow {
            key: expected.key.clone(),
            execution_id: format!(
                "buildkite:{}/{}/{}/{}/{}",
                p.organization, p.slug, build.number, build.id, job.id
            ),
            state: actual,
            exit_code: job.exit_status,
            response_digest: digest.clone(),
            log_digest: None,
            github_attestation: None,
        });
    }
    if done && state != State::Success {
        return Err(invalid("Buildkite build ended without success"));
    }
    Ok((rows, done))
}

/// Polls until every expected remote workflow is terminal. Missing required jobs,
/// skipped steps, authentication errors, unknown states and changing run attempts
/// cannot become green. A cancellation/deadline returns error and leaves the
/// exact snapshot lease with the caller for explicit recovery/cleanup.
pub fn collect(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    plan: &RemotePlan,
    interval: Duration,
    b: &Budget<'_>,
) -> Result<Collection> {
    plan.validate()?;
    for workflow in &plan.github {
        for job in &workflow.jobs {
            let contract = &job.contract;
            let source = s.request();
            ensure(
                contract.repository == source.repository.full_name()
                    && contract.source_head == source.source_head
                    && contract.accepted_base_revision == source.accepted_base
                    && contract.transport_revision == s.transport_head()
                    && contract.source_tree == source.source_tree,
                "reviewed GitHub contract does not bind this exact snapshot",
            )?;
        }
    }
    ensure(
        interval >= Duration::from_secs(1) && interval <= Duration::from_secs(60),
        "invalid poll interval",
    )?;
    verify_remote_binding(runner, s, b)?;
    let mut gh_ids: BTreeMap<u64, (u64, u64)> = BTreeMap::new();
    let mut bk_ids: BTreeMap<String, (String, u64)> = BTreeMap::new();
    loop {
        b.remaining()?;
        let mut rows = vec![];
        let mut complete = true;
        for w in &plan.github {
            let run = if let Some((id, attempt)) = gh_ids.get(&w.id) {
                let (mut run, response_digest): (GhRun, _) = gh(
                    runner,
                    s,
                    &format!("{}/actions/runs/{id}", s.request().repository.api()),
                    b,
                )?;
                run.api_digest = response_digest;
                ensure(
                    run.id == *id && run.run_attempt == *attempt,
                    "GitHub run or attempt changed while collecting",
                )?;
                run
            } else {
                let Some(run) = discover_gh(runner, s, w, b)? else {
                    complete = false;
                    continue;
                };
                gh_ids.insert(w.id, (run.id, run.run_attempt));
                run
            };
            let (mut observed, done) = collect_gh(runner, s, w, &run, b)?;
            rows.append(&mut observed);
            complete &= done;
        }
        for p in &plan.buildkite {
            let identity = format!("{}/{}", p.organization, p.slug);
            let (id, number) = if let Some(pair) = bk_ids.get(&identity) {
                pair.clone()
            } else {
                let Some(build) = discover_bk(runner, s, p, b)? else {
                    complete = false;
                    continue;
                };
                let pair = (build.id, build.number);
                bk_ids.insert(identity, pair.clone());
                pair
            };
            let (build, digest): (BkBuild, _) = bk(
                runner,
                s,
                p,
                &format!(
                    "/pipelines/{}/builds/{number}?include_retried_jobs=false",
                    p.slug
                ),
                b,
            )?;
            ensure(
                build.id == id && build.number == number,
                "Buildkite execution identity changed",
            )?;
            let (mut observed, done) = collect_bk(&build, digest, s, p)?;
            if done {
                buildkite_logs(runner, s, p, &build, &mut observed, b)?;
            }
            rows.append(&mut observed);
            complete &= done;
        }
        if complete {
            let expected = plan.github.iter().map(|w| w.jobs.len()).sum::<usize>()
                + plan.buildkite.iter().map(|p| p.jobs.len()).sum::<usize>();
            ensure(
                rows.len() == expected && rows.iter().all(|r| terminal(&r.state)),
                "terminal collection is incomplete",
            )?;
            for w in &plan.github {
                let (id, attempt) = gh_ids[&w.id];
                let (latest, _): (GhRun, _) = gh(
                    runner,
                    s,
                    &format!("{}/actions/runs/{id}", s.request().repository.api()),
                    b,
                )?;
                check_run(&latest, s, w)?;
                ensure(
                    latest.id == id
                        && latest.run_attempt == attempt
                        && gh_state(&latest.status, latest.conclusion.as_deref())?
                            == State::Success,
                    "GitHub run changed after job collection",
                )?;
            }
            for p in &plan.buildkite {
                let (id, number) = &bk_ids[&format!("{}/{}", p.organization, p.slug)];
                let (latest, digest): (BkBuild, _) = bk(
                    runner,
                    s,
                    p,
                    &format!(
                        "/pipelines/{}/builds/{number}?include_retried_jobs=false",
                        p.slug
                    ),
                    b,
                )?;
                ensure(
                    latest.id == *id && latest.number == *number,
                    "Buildkite build changed after collection",
                )?;
                let (fresh, done) = collect_bk(&latest, digest, s, p)?;
                ensure(
                    done && fresh.iter().all(|new| {
                        rows.iter().any(|old| {
                            old.key == new.key
                                && old.execution_id == new.execution_id
                                && old.state == new.state
                                && old.exit_code == new.exit_code
                        })
                    }),
                    "Buildkite retry changed after job collection",
                )?;
            }
            verify_remote_binding(runner, s, b)?;
            return Ok(Collection {
                all_passed: rows.iter().all(|r| r.state == State::Success),
                rows,
                complete: true,
            });
        }
        b.wait(interval)?;
    }
}

/// Optional explicit API trigger. Parent must first establish that this pipeline
/// is not also auto-triggered by the temp-ref push, authorize the CI spend and
/// verify the repository-owned pre-PR entrypoint. No automatic retry on unknown
/// POST outcome: discover by unique commit/ref/request before deciding next action.
pub fn submit_buildkite(
    runner: &mut impl Runner,
    s: &PreparedSnapshot,
    p: &BuildkitePipeline,
    b: &Budget<'_>,
) -> Result<u64> {
    RemotePlan {
        github: vec![],
        buildkite: vec![p.clone()],
    }
    .validate()?;
    ensure(
        p.source == BuildkiteSource::Api,
        "explicit submit requires API source",
    )?;
    ensure(
        discover_bk(runner, s, p, b)?.is_none(),
        "existing snapshot build found; do not resubmit",
    )?;
    let r = s.request();
    let body=serde_json::json!({"commit":s.transport_head(),"branch":r.transport_ref().strip_prefix("refs/heads/").unwrap(),"message":format!("Decapod CI preflight {}",r.request_id),"meta_data":{"decapod-request":r.request_id,"decapod-source":r.source_head,"decapod-base":r.accepted_base,"decapod-tree":r.source_tree}}).to_string();
    let spec = CommandSpec {
        program: Program::Buildkite,
        args: vec![
            "api".into(),
            "--no-input".into(),
            "--method".into(),
            "POST".into(),
            format!("/pipelines/{}/builds", p.slug),
            "--data".into(),
            body,
        ],
        cwd: s.root().into(),
        organization: Some(p.organization.clone()),
        provider_request: Some(ProviderRequest::BuildkiteCreate {
            organization: p.organization.clone(),
            pipeline: p.slug.clone(),
            request: Box::new(r.clone()),
            transport_head: s.transport_head().into(),
        }),
    };
    let response = checked(runner, spec, b)?;
    let build: BkBuild = decode(&response)?;
    check_build(&build, s, p)?;
    Ok(build.number)
}
#[cfg(test)]
mod tests;

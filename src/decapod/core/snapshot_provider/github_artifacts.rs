//! Authenticated raw-file handoff: public provider metadata authenticates ZIP
//! bytes materialized by a supported host integration. No token exchange and no
//! caller-supplied success JSON. Exact producer-job mapping is separately proven
//! by the frozen upload-step contract and source-execution manifest.
use super::artifact_archive::ArtifactFiles;
use super::*;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::Read;
const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
#[derive(Debug)]
pub struct GithubArtifactSelection {
    pub repository: Repository,
    pub repository_id: u64,
    pub run_id: u64,
    pub run_attempt: u32,
    pub head_sha: String,
    pub branch: String,
    pub artifact_id: u64,
    pub artifact_name: String,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Repo {
    id: u64,
    full_name: String,
}
#[derive(Debug, Deserialize)]
struct Run {
    id: u64,
    run_attempt: u32,
    head_sha: String,
    head_branch: String,
    repository: Repo,
    head_repository: Repo,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct ArtifactRun {
    id: u64,
    repository_id: u64,
    head_repository_id: u64,
    head_branch: String,
    head_sha: String,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Metadata {
    id: u64,
    node_id: String,
    name: String,
    size_in_bytes: u64,
    url: String,
    archive_download_url: String,
    expired: bool,
    digest: String,
    workflow_run: ArtifactRun,
}
fn get<T: serde::de::DeserializeOwned>(
    runner: &mut impl Runner,
    root: &Path,
    path: &str,
    budget: &Budget<'_>,
) -> Result<T> {
    let bytes = checked(runner, super::provider::gh_spec(root, path), budget)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("GitHub artifact API schema mismatch"))
}
fn verify_run(
    runner: &mut impl Runner,
    root: &Path,
    s: &GithubArtifactSelection,
    budget: &Budget<'_>,
) -> Result<()> {
    let r: Run = get(
        runner,
        root,
        &format!("{}/actions/runs/{}", s.repository.api(), s.run_id),
        budget,
    )?;
    ensure(
        r.id == s.run_id
            && r.run_attempt == s.run_attempt
            && r.head_sha == s.head_sha
            && r.head_branch == s.branch
            && r.repository.id == s.repository_id
            && r.head_repository.id == s.repository_id
            && r.repository.full_name == s.repository.full_name()
            && r.head_repository.full_name == s.repository.full_name(),
        "GitHub artifact current run/attempt/source mismatch",
    )
}
fn metadata(
    runner: &mut impl Runner,
    root: &Path,
    s: &GithubArtifactSelection,
    budget: &Budget<'_>,
) -> Result<Metadata> {
    let path = format!("{}/actions/artifacts/{}", s.repository.api(), s.artifact_id);
    let m: Metadata = get(runner, root, &path, budget)?;
    validate_metadata(&m, s)?;
    Ok(m)
}
fn validate_metadata(m: &Metadata, s: &GithubArtifactSelection) -> Result<()> {
    let path = format!("{}/actions/artifacts/{}", s.repository.api(), s.artifact_id);
    ensure(
        m.id == s.artifact_id
            && m.name == s.artifact_name
            && !m.node_id.is_empty()
            && !m.expired
            && m.size_in_bytes > 0
            && m.size_in_bytes <= MAX_ARCHIVE
            && m.url == format!("https://api.github.com/{path}")
            && m.archive_download_url == format!("https://api.github.com/{path}/zip"),
        "GitHub artifact identity/availability mismatch",
    )?;
    let r = &m.workflow_run;
    ensure(
        r.id == s.run_id
            && r.repository_id == s.repository_id
            && r.head_repository_id == s.repository_id
            && r.head_sha == s.head_sha
            && r.head_branch == s.branch,
        "GitHub artifact source-run mismatch",
    )?;
    ensure(
        m.digest.strip_prefix("sha256:").is_some_and(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }),
        "GitHub artifact lacks authenticated SHA256",
    )?;
    Ok(())
}
/// Download a single metadata-authenticated raw ZIP using the configured official
/// GitHub CLI/host runner. No extraction, caller-chosen URL, credential handoff,
/// or fallback after authentication failure. The CLI owns its documented redirect
/// behavior; its authorization seal explicitly does not claim host-pinned redirects.
pub fn download(
    runner: &mut impl Runner,
    root: &Path,
    selection: &GithubArtifactSelection,
    budget: &Budget<'_>,
) -> Result<ArtifactFiles> {
    validate_selection(selection)?;
    verify_run(runner, root, selection, budget)?;
    let before = metadata(runner, root, selection, budget)?;
    let mut spec = super::provider::gh_spec(
        root,
        &format!(
            "{}/actions/artifacts/{}/zip",
            selection.repository.api(),
            selection.artifact_id
        ),
    );
    spec.provider_request = Some(ProviderRequest::GithubArtifactZip {
        repository: selection.repository.clone(),
        artifact_id: selection.artifact_id,
        expected_bytes: before.size_in_bytes as usize,
    });
    spec.authorize_provider_read()?;
    budget.remaining()?;
    let reply = runner.execute(&spec, budget)?;
    if reply.exit_code != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "configured GitHub artifact download failed; authorization or provider availability must be resolved without extracting tokens",
        ));
    }
    ensure(
        reply.stdout.len() as u64 == before.size_in_bytes,
        "downloaded archive size mismatch",
    )?;
    let files = ArtifactFiles::verified(
        format!(
            "github:{}/runs/{}/attempts/{}/artifacts/{}",
            selection.repository.full_name(),
            selection.run_id,
            selection.run_attempt,
            selection.artifact_id
        ),
        &before.digest,
        &reply.stdout,
        budget,
    )?;
    ensure(
        metadata(runner, root, selection, budget)? == before,
        "GitHub artifact changed during download",
    )?;
    verify_run(runner, root, selection, budget)?;
    Ok(files)
}
fn validate_selection(selection: &GithubArtifactSelection) -> Result<()> {
    ensure(
        selection.repository_id > 0
            && selection.run_id > 0
            && selection.run_attempt > 0
            && selection.artifact_id > 0
            && oid(&selection.head_sha)
            && branch(&format!("refs/heads/{}", selection.branch))
            && !selection.artifact_name.is_empty()
            && selection.artifact_name.len() <= 512
            && !selection.artifact_name.chars().any(char::is_control),
        "invalid expected GitHub artifact binding",
    )
}
/// Reads one ordinary local file without following a final symlink. The file's
/// exact compressed bytes must match independently fetched provider metadata,
/// before bounded in-memory ZIP parsing. The current run attempt is rechecked.
pub fn verify_handoff(
    runner: &mut impl Runner,
    root: &Path,
    selection: &GithubArtifactSelection,
    archive_path: &Path,
    budget: &Budget<'_>,
) -> Result<ArtifactFiles> {
    validate_selection(selection)?;
    verify_run(runner, root, selection, budget)?;
    let before = metadata(runner, root, selection, budget)?;
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = opts.open(archive_path)?;
    let meta = file.metadata()?;
    ensure(
        meta.is_file() && meta.len() == before.size_in_bytes,
        "handoff is not exact-sized regular archive",
    )?;
    let mut bytes = Vec::with_capacity((meta.len() as usize).min(1024 * 1024));
    let mut chunk = [0u8; 65536];
    loop {
        budget.remaining()?;
        let n = file.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        ensure(
            bytes.len() as u64 + n as u64 <= before.size_in_bytes,
            "handoff archive grew during read",
        )?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    ensure(
        bytes.len() as u64 == before.size_in_bytes,
        "handoff archive truncated",
    )?;
    let files = ArtifactFiles::verified(
        format!(
            "github:{}/runs/{}/attempts/{}/artifacts/{}",
            selection.repository.full_name(),
            selection.run_id,
            selection.run_attempt,
            selection.artifact_id
        ),
        &before.digest,
        &bytes,
        budget,
    )?;
    ensure(
        metadata(runner, root, selection, budget)? == before,
        "GitHub artifact changed during import",
    )?;
    verify_run(runner, root, selection, budget)?;
    Ok(files)
}

/// Observed job metadata inside a provider-fetched, complete workflow inventory.
/// This is a locator/context view, not execution evidence or an exit status.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct GithubObservedStep {
    name: String,
    number: u64,
    status: String,
    conclusion: Option<String>,
}
impl GithubObservedStep {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn conclusion(&self) -> Option<&str> {
        self.conclusion.as_deref()
    }
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct GithubObservedJob {
    id: u64,
    run_id: u64,
    run_attempt: u32,
    head_sha: String,
    name: String,
    status: String,
    conclusion: Option<String>,
    steps: Vec<GithubObservedStep>,
}
impl GithubObservedJob {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn status(&self) -> &str {
        &self.status
    }
    pub fn conclusion(&self) -> Option<&str> {
        self.conclusion.as_deref()
    }
    pub fn steps(&self) -> &[GithubObservedStep] {
        &self.steps
    }
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct WorkflowMetadata {
    id: u64,
    path: String,
    state: String,
    url: String,
}
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct DiscoveryRun {
    id: u64,
    run_attempt: u32,
    workflow_id: u64,
    path: String,
    head_sha: String,
    head_branch: String,
    event: String,
    status: String,
    conclusion: Option<String>,
    repository: Repo,
    head_repository: Repo,
}
#[derive(Deserialize)]
struct DiscoveryRuns {
    total_count: usize,
    workflow_runs: Vec<DiscoveryRun>,
}
#[derive(Deserialize)]
struct DiscoveryJobs {
    total_count: usize,
    jobs: Vec<GithubObservedJob>,
}
#[derive(Deserialize)]
struct DiscoveryArtifacts {
    total_count: usize,
    artifacts: Vec<Metadata>,
}
/// Minted only by bounded provider reads for an independently prepared snapshot.
#[derive(Debug)]
pub struct GithubWorkflowInventory {
    repository: Repository,
    run: DiscoveryRun,
    jobs: Vec<GithubObservedJob>,
    artifacts: Vec<Metadata>,
}
impl GithubWorkflowInventory {
    pub fn repository(&self) -> &Repository {
        &self.repository
    }
    pub fn repository_id(&self) -> u64 {
        self.run.repository.id
    }
    pub fn run_id(&self) -> u64 {
        self.run.id
    }
    pub fn run_attempt(&self) -> u32 {
        self.run.run_attempt
    }
    pub fn workflow_id(&self) -> u64 {
        self.run.workflow_id
    }
    pub fn workflow_path(&self) -> &str {
        &self.run.path
    }
    pub fn status(&self) -> &str {
        &self.run.status
    }
    pub fn conclusion(&self) -> Option<&str> {
        self.run.conclusion.as_deref()
    }
    pub fn jobs(&self) -> &[GithubObservedJob] {
        &self.jobs
    }
    pub fn artifact_names(&self) -> impl Iterator<Item = &str> {
        self.artifacts.iter().map(|a| a.name.as_str())
    }
    /// Artifact metadata does not carry job ID/attempt. Reviewed unique upload
    /// naming, actual required upload step, and inner producer manifests must
    /// bind the selection to the exact job and current attempt before use.
    pub fn artifact_selection(&self, name: &str) -> Result<Option<GithubArtifactSelection>> {
        let rows: Vec<_> = self.artifacts.iter().filter(|a| a.name == name).collect();
        ensure(rows.len() <= 1, "ambiguous GitHub artifact name")?;
        let Some(m) = rows.first() else {
            return Ok(None);
        };
        let selection = GithubArtifactSelection {
            repository: self.repository.clone(),
            repository_id: self.run.repository.id,
            run_id: self.run.id,
            run_attempt: self.run.run_attempt,
            head_sha: self.run.head_sha.clone(),
            branch: self.run.head_branch.clone(),
            artifact_id: m.id,
            artifact_name: m.name.clone(),
        };
        validate_metadata(m, &selection)?;
        Ok(Some(selection))
    }
}
fn terminal_shape(status: &str, conclusion: Option<&str>) -> Result<()> {
    ensure(
        matches!(
            status,
            "queued" | "in_progress" | "completed" | "waiting" | "requested" | "pending"
        ),
        "unknown GitHub execution status",
    )?;
    ensure(
        if status == "completed" {
            conclusion.is_some_and(|c| {
                matches!(
                    c,
                    "success"
                        | "failure"
                        | "neutral"
                        | "cancelled"
                        | "skipped"
                        | "timed_out"
                        | "action_required"
                        | "stale"
                        | "startup_failure"
                )
            })
        } else {
            conclusion.is_none()
        },
        "GitHub terminal status/conclusion mismatch",
    )
}
pub(super) fn workflow_name(path: &str) -> Result<&str> {
    let name = path
        .strip_prefix(".github/workflows/")
        .ok_or_else(|| invalid("source workflow path required"))?;
    ensure(
        !name.is_empty()
            && name.len() <= 200
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && (name.ends_with(".yml") || name.ends_with(".yaml"))
            && !name.contains(".."),
        "invalid authored workflow filename",
    )?;
    Ok(name)
}
fn check_discovered_run(
    run: &DiscoveryRun,
    s: &snapshot::PreparedSnapshot,
    w: &WorkflowMetadata,
) -> Result<()> {
    ensure(
        run.id > 0
            && run.run_attempt > 0
            && run.workflow_id == w.id
            && run.path == w.path
            && run.head_sha == s.transport_head()
            && run.head_branch
                == s.request()
                    .transport_ref()
                    .trim_start_matches("refs/heads/")
            && run.event == "push"
            && run.repository.id > 0
            && run.repository == run.head_repository
            && run.repository.full_name == s.request().repository.full_name(),
        "discovered GitHub run does not bind snapshot/workflow/repository",
    )?;
    terminal_shape(&run.status, run.conclusion.as_deref())
}
fn page_total(expected: &mut Option<usize>, actual: usize, len: usize, max: usize) -> Result<()> {
    ensure(
        actual <= max && expected.is_none_or(|n| n == actual) && len <= 100,
        "provider inventory pagination changed or exceeded bound",
    )?;
    *expected = Some(actual);
    Ok(())
}
/// Official GitHub workflow-file lookup, exact-S push-run discovery, full current
/// attempt job list and full run artifact inventory. No guessed IDs and no caller
/// success JSON. A missing run returns None so the caller can poll its deadline.
/// Caller separately invokes verify_remote_binding before/finally for S/H/T/B/tags.
/// Run-only metadata for polling. It proves identity/status, never source execution.
#[derive(Debug)]
pub struct GithubWorkflowStatus {
    run: DiscoveryRun,
    workflow: WorkflowMetadata,
}
impl GithubWorkflowStatus {
    pub fn run_id(&self) -> u64 {
        self.run.id
    }
    pub fn run_attempt(&self) -> u32 {
        self.run.run_attempt
    }
    pub fn workflow_path(&self) -> &str {
        &self.run.path
    }
    pub fn status(&self) -> &str {
        &self.run.status
    }
    pub fn conclusion(&self) -> Option<&str> {
        self.run.conclusion.as_deref()
    }
}
pub fn discover_workflow_status(
    runner: &mut impl Runner,
    snapshot: &snapshot::PreparedSnapshot,
    workflow_path: &str,
    budget: &Budget<'_>,
) -> Result<Option<GithubWorkflowStatus>> {
    let filename = workflow_name(workflow_path)?;
    let root = snapshot.root();
    let repo = &snapshot.request().repository;
    let workflow: WorkflowMetadata = get(
        runner,
        root,
        &format!("{}/actions/workflows/{filename}", repo.api()),
        budget,
    )?;
    ensure(
        workflow.id > 0
            && workflow.path == workflow_path
            && workflow.state == "active"
            && workflow.url
                == format!(
                    "https://api.github.com/{}/actions/workflows/{}",
                    repo.api(),
                    workflow.id
                ),
        "workflow filename resolution mismatch/inactive",
    )?;
    let branch = snapshot
        .request()
        .transport_ref()
        .trim_start_matches("refs/heads/")
        .to_string();
    let mut runs = Vec::new();
    let mut total = None;
    for page in 1..=10 {
        let result: DiscoveryRuns = get(
            runner,
            root,
            &format!(
                "{}/actions/workflows/{}/runs?head_sha={}&event=push&branch={}&per_page=100&page={page}",
                repo.api(),
                workflow.id,
                snapshot.transport_head(),
                encode_path(&branch)
            ),
            budget,
        )?;
        page_total(
            &mut total,
            result.total_count,
            result.workflow_runs.len(),
            1000,
        )?;
        runs.extend(result.workflow_runs);
        ensure(
            runs.len() <= result.total_count,
            "run inventory exceeds total",
        )?;
        if runs.len() == result.total_count {
            break;
        }
        ensure(page < 10, "run inventory incomplete")?;
    }
    ensure(
        runs.len() <= 1,
        "multiple exact snapshot workflow runs are ambiguous",
    )?;
    let Some(run) = runs.pop() else {
        return Ok(None);
    };
    check_discovered_run(&run, snapshot, &workflow)?;
    Ok(Some(GithubWorkflowStatus { run, workflow }))
}
/// Collect one full inventory after polling. Never writes evidence directories.
pub fn discover_workflow(
    runner: &mut impl Runner,
    snapshot: &snapshot::PreparedSnapshot,
    workflow_path: &str,
    budget: &Budget<'_>,
) -> Result<Option<GithubWorkflowInventory>> {
    let Some(GithubWorkflowStatus { run, workflow }) =
        discover_workflow_status(runner, snapshot, workflow_path, budget)?
    else {
        return Ok(None);
    };
    let root = snapshot.root();
    let repo = &snapshot.request().repository;
    let mut jobs = Vec::new();
    let mut total = None;
    for page in 1..=20 {
        let result: DiscoveryJobs = get(
            runner,
            root,
            &format!(
                "{}/actions/runs/{}/attempts/{}/jobs?per_page=100&page={page}",
                repo.api(),
                run.id,
                run.run_attempt
            ),
            budget,
        )?;
        page_total(&mut total, result.total_count, result.jobs.len(), 2000)?;
        jobs.extend(result.jobs);
        ensure(
            jobs.len() <= result.total_count,
            "job inventory exceeds total",
        )?;
        if jobs.len() == result.total_count {
            break;
        }
        ensure(page < 20, "job inventory incomplete")?;
    }
    let mut ids = std::collections::BTreeSet::new();
    for job in &jobs {
        ensure(
            job.id > 0
                && ids.insert(job.id)
                && job.run_id == run.id
                && job.run_attempt == run.run_attempt
                && job.head_sha == run.head_sha
                && !job.name.is_empty(),
            "job identity does not bind exact run attempt",
        )?;
        terminal_shape(&job.status, job.conclusion.as_deref())?;
        let mut number = 0;
        for step in &job.steps {
            ensure(
                step.number > number && !step.name.is_empty(),
                "unordered/duplicate/missing step identity",
            )?;
            number = step.number;
            terminal_shape(&step.status, step.conclusion.as_deref())?;
        }
    }
    let mut artifacts = Vec::new();
    let mut total = None;
    for page in 1..=20 {
        let result: DiscoveryArtifacts = get(
            runner,
            root,
            &format!(
                "{}/actions/runs/{}/artifacts?per_page=100&page={page}",
                repo.api(),
                run.id
            ),
            budget,
        )?;
        page_total(&mut total, result.total_count, result.artifacts.len(), 2000)?;
        artifacts.extend(result.artifacts);
        ensure(
            artifacts.len() <= result.total_count,
            "artifact inventory exceeds total",
        )?;
        if artifacts.len() == result.total_count {
            break;
        }
        ensure(page < 20, "artifact inventory incomplete")?;
    }
    let mut ids = std::collections::BTreeSet::new();
    for artifact in &artifacts {
        ensure(
            artifact.id > 0
                && ids.insert(artifact.id)
                && artifact.workflow_run.id == run.id
                && artifact.workflow_run.repository_id == run.repository.id
                && artifact.workflow_run.head_repository_id == run.repository.id
                && artifact.workflow_run.head_sha == run.head_sha
                && artifact.workflow_run.head_branch == run.head_branch,
            "artifact inventory contains foreign/duplicate producer",
        )?;
    }
    let fresh: DiscoveryRun = get(
        runner,
        root,
        &format!("{}/actions/runs/{}", repo.api(), run.id),
        budget,
    )?;
    check_discovered_run(&fresh, snapshot, &workflow)?;
    ensure(
        fresh == run,
        "GitHub run/attempt changed during inventory collection",
    )?;
    Ok(Some(GithubWorkflowInventory {
        repository: repo.clone(),
        run,
        jobs,
        artifacts,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    struct Mock {
        run: Value,
        metadata: Value,
        rerun: bool,
        reads: usize,
    }
    impl Runner for Mock {
        fn execute(&mut self, s: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
            s.authorize_provider_read()?;
            let body = if s.arguments().last().unwrap().contains("/actions/runs/") {
                self.reads += 1;
                let mut r = self.run.clone();
                if self.rerun && self.reads > 1 {
                    r["run_attempt"] = 2.into();
                }
                r
            } else {
                self.metadata.clone()
            };
            Ok(Reply {
                exit_code: 0,
                stdout: serde_json::to_vec(&body).unwrap(),
            })
        }
    }
    fn selection() -> GithubArtifactSelection {
        GithubArtifactSelection {
            repository: Repository::new("org", "repo").unwrap(),
            repository_id: 1,
            run_id: 7,
            run_attempt: 1,
            head_sha: "a".repeat(40),
            branch: "work".into(),
            artifact_id: 9,
            artifact_name: "source-proof".into(),
        }
    }
    fn fixture() -> (tempfile::TempDir, Mock) {
        let s = selection();
        let bytes = super::super::artifact_archive::tests::zip(
            "fmt/preparation/preparation.json",
            b"{}",
            0x8000,
        );
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("handoff.zip"), &bytes).unwrap();
        let r = json!({"id":7,"run_attempt":1,"head_sha":s.head_sha,"head_branch":"work","repository":{"id":1,"full_name":"org/repo"},"head_repository":{"id":1,"full_name":"org/repo"}});
        let a = json!({"id":9,"node_id":"opaque","name":"source-proof","size_in_bytes":bytes.len(),"url":"https://api.github.com/repos/org/repo/actions/artifacts/9","archive_download_url":"https://api.github.com/repos/org/repo/actions/artifacts/9/zip","expired":false,"digest":hash(&bytes),"workflow_run":{"id":7,"repository_id":1,"head_repository_id":1,"head_branch":"work","head_sha":s.head_sha}});
        (
            dir,
            Mock {
                run: r,
                metadata: a,
                rerun: false,
                reads: 0,
            },
        )
    }
    #[test]
    fn raw_handoff_needs_independent_public_metadata() {
        let (d, mut r) = fixture();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
        let f = verify_handoff(
            &mut r,
            d.path(),
            &selection(),
            &d.path().join("handoff.zip"),
            &b,
        )
        .unwrap();
        assert_eq!(f.file("fmt/preparation/preparation.json").unwrap(), b"{}");
        assert_eq!(r.reads, 2);
    }
    #[test]
    fn wrong_attempt_digest_name_and_foreign_download_endpoint_reject() {
        for n in 0..4 {
            let (d, mut r) = fixture();
            match n {
                0 => r.rerun = true,
                1 => r.metadata["digest"] = hash(b"fake").into(),
                2 => r.metadata["name"] = "different".into(),
                _ => r.metadata["archive_download_url"] = "https://evil.invalid/zip".into(),
            };
            let c = Cancellation::default();
            let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
            assert!(
                verify_handoff(
                    &mut r,
                    d.path(),
                    &selection(),
                    &d.path().join("handoff.zip"),
                    &b
                )
                .is_err()
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn symlink_handoff_rejected() {
        let (d, mut r) = fixture();
        std::os::unix::fs::symlink(d.path().join("handoff.zip"), d.path().join("link.zip"))
            .unwrap();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
        assert!(
            verify_handoff(
                &mut r,
                d.path(),
                &selection(),
                &d.path().join("link.zip"),
                &b
            )
            .is_err()
        );
    }
    fn prepared() -> snapshot::PreparedSnapshot {
        snapshot::PreparedSnapshot::fixture(
            snapshot::SnapshotRequest {
                repository: Repository::new("org", "repo").unwrap(),
                request_id: "11111111-2222-3333-4444-555555555555".into(),
                source_ref: "refs/heads/work".into(),
                source_head: "a".repeat(40),
                source_tree: "c".repeat(40),
                accepted_base: "d".repeat(40),
                accepted_base_ref: "refs/heads/master".into(),
                tags: Default::default(),
            },
            "a".repeat(40),
            "/tmp".into(),
        )
    }
    struct DiscoveryMock {
        bodies: std::collections::BTreeMap<String, Value>,
    }
    impl Runner for DiscoveryMock {
        fn execute(&mut self, s: &CommandSpec, _: &Budget<'_>) -> Result<Reply> {
            s.authorize_provider_read()?;
            let path = s.arguments().last().unwrap();
            let v = self
                .bodies
                .get(path)
                .ok_or_else(|| invalid("unexpected discovery request"))?;
            Ok(Reply {
                exit_code: 0,
                stdout: serde_json::to_vec(v).unwrap(),
            })
        }
    }
    fn discovery_fixture() -> DiscoveryMock {
        let s = prepared();
        let branch = s
            .request()
            .transport_ref()
            .trim_start_matches("refs/heads/")
            .to_string();
        let (_, r) = fixture();
        let mut a = r.metadata;
        a["workflow_run"]["head_branch"] = branch.clone().into();
        let run = json!({"id":7,"run_attempt":1,"workflow_id":99,"path":".github/workflows/ci.yml","event":"push","head_sha":"a".repeat(40),"head_branch":branch,"status":"completed","conclusion":"success","repository":{"id":1,"full_name":"org/repo"},"head_repository":{"id":1,"full_name":"org/repo"}});
        let job = json!({"id":10,"run_id":7,"run_attempt":1,"head_sha":"a".repeat(40),"name":"fmt","status":"completed","conclusion":"success","steps":[{"name":"Run fmt","number":1,"status":"completed","conclusion":"success"},{"name":"Upload source custody","number":2,"status":"completed","conclusion":"success"}]});
        DiscoveryMock {
            bodies: std::collections::BTreeMap::from([
                (
                    "repos/org/repo/actions/workflows/ci.yml".into(),
                    json!({"id":99,"path":".github/workflows/ci.yml","state":"active","url":"https://api.github.com/repos/org/repo/actions/workflows/99"}),
                ),
                (
                    format!(
                        "repos/org/repo/actions/workflows/99/runs?head_sha={}&event=push&branch={}&per_page=100&page=1",
                        s.transport_head(),
                        encode_path(&branch)
                    ),
                    json!({"total_count":1,"workflow_runs":[run.clone()]}),
                ),
                ("repos/org/repo/actions/runs/7".into(), run),
                (
                    "repos/org/repo/actions/runs/7/attempts/1/jobs?per_page=100&page=1".into(),
                    json!({"total_count":1,"jobs":[job]}),
                ),
                (
                    "repos/org/repo/actions/runs/7/artifacts?per_page=100&page=1".into(),
                    json!({"total_count":1,"artifacts":[a]}),
                ),
            ]),
        }
    }
    #[test]
    fn exact_workflow_inventory_mints_actual_locators_and_steps() {
        let mut r = discovery_fixture();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
        let i = discover_workflow(&mut r, &prepared(), ".github/workflows/ci.yml", &b)
            .unwrap()
            .unwrap();
        assert_eq!((i.run_id(), i.run_attempt(), i.repository_id()), (7, 1, 1));
        assert_eq!(i.jobs()[0].id(), 10);
        assert_eq!(i.jobs()[0].steps()[1].name(), "Upload source custody");
        assert_eq!(
            i.artifact_selection("source-proof")
                .unwrap()
                .unwrap()
                .artifact_id,
            9
        );
        assert!(i.artifact_selection("missing").unwrap().is_none());
    }
    #[test]
    fn workflow_inventory_rejects_foreign_source_reruns_duplicates_and_missing_pages() {
        for n in 0..8 {
            let mut r = discovery_fixture();
            match n {
                0 => {
                    r.bodies
                        .get_mut("repos/org/repo/actions/workflows/ci.yml")
                        .unwrap()["path"] = ".github/workflows/evil.yml".into()
                }
                1 => {
                    r.bodies.get_mut("repos/org/repo/actions/runs/7").unwrap()["run_attempt"] =
                        2.into()
                }
                2 => {
                    r.bodies
                        .get_mut(
                            "repos/org/repo/actions/runs/7/attempts/1/jobs?per_page=100&page=1",
                        )
                        .unwrap()["jobs"][0]["head_sha"] = "b".repeat(40).into()
                }
                3 => {
                    r.bodies
                        .get_mut(
                            "repos/org/repo/actions/runs/7/attempts/1/jobs?per_page=100&page=1",
                        )
                        .unwrap()["jobs"][0]["steps"][1]["number"] = 1.into()
                }
                4 => {
                    r.bodies
                        .get_mut("repos/org/repo/actions/runs/7/artifacts?per_page=100&page=1")
                        .unwrap()["total_count"] = 2.into()
                }
                5 => {
                    r.bodies
                        .get_mut("repos/org/repo/actions/runs/7/artifacts?per_page=100&page=1")
                        .unwrap()["artifacts"][0]["workflow_run"]["id"] = 8.into()
                }
                6 => {
                    r.bodies
                        .get_mut(
                            "repos/org/repo/actions/runs/7/attempts/1/jobs?per_page=100&page=1",
                        )
                        .unwrap()["jobs"][0]["run_attempt"] = 2.into()
                }
                _ => {
                    let p = r
                        .bodies
                        .keys()
                        .find(|p| p.contains("/runs?"))
                        .unwrap()
                        .clone();
                    let v = r.bodies.get_mut(&p).unwrap();
                    v["total_count"] = 2.into();
                    let run = v["workflow_runs"][0].clone();
                    v["workflow_runs"].as_array_mut().unwrap().push(run);
                }
            }
            let c = Cancellation::default();
            let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
            assert!(
                discover_workflow(&mut r, &prepared(), ".github/workflows/ci.yml", &b).is_err(),
                "mutation {n}"
            );
        }
    }
    #[test]
    fn absent_workflow_run_returns_pending_not_success() {
        let mut r = discovery_fixture();
        let p = r
            .bodies
            .keys()
            .find(|p| p.contains("/runs?"))
            .unwrap()
            .clone();
        r.bodies
            .insert(p, json!({"total_count":0,"workflow_runs":[]}));
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(3), 1024 * 1024, &c).unwrap();
        assert!(
            discover_workflow(&mut r, &prepared(), ".github/workflows/ci.yml", &b)
                .unwrap()
                .is_none()
        );
    }
    struct DownloadMock {
        inner: Mock,
        bytes: Vec<u8>,
        fail: bool,
        downloads: usize,
    }
    impl Runner for DownloadMock {
        fn execute(&mut self, spec: &CommandSpec, b: &Budget<'_>) -> Result<Reply> {
            if let Some(ProviderRequest::GithubArtifactZip { expected_bytes, .. }) =
                spec.provider_request()
            {
                spec.authorize_provider_read()?;
                assert_eq!(
                    *expected_bytes as u64,
                    self.inner.metadata["size_in_bytes"].as_u64().unwrap()
                );
                self.downloads += 1;
                return Ok(Reply {
                    exit_code: if self.fail { 4 } else { 0 },
                    stdout: self.bytes.clone(),
                });
            }
            self.inner.execute(spec, b)
        }
    }
    fn download_fixture() -> (tempfile::TempDir, DownloadMock) {
        let (dir, inner) = fixture();
        let bytes = std::fs::read(dir.path().join("handoff.zip")).unwrap();
        (
            dir,
            DownloadMock {
                inner,
                bytes,
                fail: false,
                downloads: 0,
            },
        )
    }
    #[test]
    fn configured_runner_downloads_raw_zip_and_rechecks_current_attempt() {
        let (dir, mut runner) = download_fixture();
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(3), 4096, &c).unwrap();
        let files = download(&mut runner, dir.path(), &selection(), &b).unwrap();
        assert_eq!(
            files.file("fmt/preparation/preparation.json").unwrap(),
            b"{}"
        );
        assert_eq!(runner.downloads, 1);
        assert_eq!(runner.inner.reads, 2);
    }
    #[test]
    fn download_rejects_denial_wrong_size_digest_attempt_and_cancel() {
        for mode in 0..6 {
            let (dir, mut runner) = download_fixture();
            let c = Cancellation::default();
            match mode {
                0 => runner.fail = true,
                1 => runner.bytes.push(1),
                2 => runner.bytes[0] ^= 1,
                3 => runner.inner.rerun = true,
                4 => runner.inner.metadata["digest"] = hash(b"false").into(),
                _ => c.cancel(),
            }
            let b = Budget::new(Duration::from_secs(3), 4096, &c).unwrap();
            let error = download(&mut runner, dir.path(), &selection(), &b).unwrap_err();
            if mode == 0 {
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
                assert_eq!(runner.downloads, 1);
            }
            if mode == 5 {
                assert_eq!(error.kind(), io::ErrorKind::Interrupted);
                assert_eq!(runner.downloads, 0);
            }
        }
    }
    #[test]
    fn declared_archive_budget_is_separate_from_metadata_budget() {
        let (dir, mut runner) = download_fixture();
        let c = Cancellation::default();
        runner.bytes = super::super::artifact_archive::tests::zip(
            "synthetic-payload",
            &vec![b'x'; 1024 * 1024],
            0x8000,
        );
        runner.inner.metadata["size_in_bytes"] = runner.bytes.len().into();
        runner.inner.metadata["digest"] = hash(&runner.bytes).into();
        let b = Budget::new(Duration::from_secs(3), 4096, &c).unwrap();
        assert_eq!(
            download(&mut runner, dir.path(), &selection(), &b)
                .unwrap()
                .file("synthetic-payload")
                .unwrap()
                .len(),
            1024 * 1024
        );
    }
}

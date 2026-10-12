//! Metadata-only barrier for the complete independently planned source suite.
//! Status/check URLs are untrusted locators, never success evidence. Every
//! imported Buildkite build is independently rebound to its exact workflow,
//! pipeline, repository, event and snapshot. Collect execution custody once only
//! after this barrier; a successful barrier is not a readiness verdict.
use super::github_artifacts as github;
use super::public_buildkite as buildkite;
use super::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct ImportedPipeline {
    pub organization: String,
    pub pipeline: String,
    pub pipeline_id: String,
    pub workflows: Vec<String>,
}
#[derive(Clone, Debug)]
pub struct SourceRunPlan {
    pub github_workflows: Vec<String>,
    pub buildkite_pipelines: Vec<ImportedPipeline>,
}
impl SourceRunPlan {
    fn validate(&self) -> Result<()> {
        let mut paths = BTreeSet::new();
        ensure(
            self.github_workflows.len() <= 64,
            "source GitHub workflow plan oversized",
        )?;
        for path in &self.github_workflows {
            github::workflow_name(path)?;
            ensure(paths.insert(path), "duplicate source workflow")?;
        }
        let mut pipelines = BTreeSet::new();
        ensure(
            !self.buildkite_pipelines.is_empty() && self.buildkite_pipelines.len() <= 64,
            "source Buildkite workflow plan missing or oversized",
        )?;
        let mut total = 0;
        for pipeline in &self.buildkite_pipelines {
            ensure(
                slug(&pipeline.organization)
                    && slug(&pipeline.pipeline)
                    && uuid(&pipeline.pipeline_id)
                    && !pipeline.workflows.is_empty()
                    && pipelines.insert((&pipeline.organization, &pipeline.pipeline)),
                "invalid duplicate imported pipeline",
            )?;
            let mut workflows = BTreeSet::new();
            for path in &pipeline.workflows {
                github::workflow_name(path)?;
                ensure(workflows.insert(path), "duplicate imported source workflow")?;
                total += 1;
            }
        }
        ensure(total <= 64, "imported workflow plan oversized")
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct GithubRunIdentity {
    run_id: u64,
    attempt: u32,
}
impl GithubRunIdentity {
    pub fn run_id(&self) -> u64 {
        self.run_id
    }
    pub fn run_attempt(&self) -> u32 {
        self.attempt
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ImportedBuildIdentity {
    build: buildkite::PublicBuild,
    id: String,
}
impl ImportedBuildIdentity {
    pub fn build(&self) -> &buildkite::PublicBuild {
        &self.build
    }
    pub fn build_id(&self) -> &str {
        &self.id
    }
}
/// Opaque in-process terminal metadata. It cannot be deserialized or converted
/// into an execution receipt. The source importer still verifies every job.
#[derive(Debug)]
pub struct TerminalSourceRuns {
    source_binding: String,
    plan_binding: String,
    github: BTreeMap<String, GithubRunIdentity>,
    buildkite: BTreeMap<(String, String, String), ImportedBuildIdentity>,
}
impl TerminalSourceRuns {
    pub fn github(&self, workflow: &str) -> Option<&GithubRunIdentity> {
        self.github.get(workflow)
    }
    pub fn buildkite(
        &self,
        organization: &str,
        pipeline: &str,
        workflow: &str,
    ) -> Option<&ImportedBuildIdentity> {
        self.buildkite
            .get(&(organization.into(), pipeline.into(), workflow.into()))
    }
    /// Re-discover the complete exact-S set to detect reruns, replacement builds,
    /// changed workflow bindings, and provider/base/tag/ref movement.
    pub fn recheck(
        &self,
        runner: &mut impl Runner,
        reader: &mut impl buildkite::PublicReader,
        snapshot: &snapshot::PreparedSnapshot,
        plan: &SourceRunPlan,
        budget: &Budget<'_>,
    ) -> Result<()> {
        plan.validate()?;
        ensure(
            self.source_binding == source_binding(snapshot)?
                && self.plan_binding == plan_binding(plan),
            "terminal source set belongs to another plan/snapshot",
        )?;
        let mut state = PollState::default();
        ensure(
            poll(runner, reader, snapshot, plan, &mut state, budget)?,
            "source suite no longer terminal",
        )?;
        ensure(
            state.github == self.github && state.buildkite == self.buildkite,
            "source run/attempt/build changed after collection",
        )?;
        provider::verify_remote_binding(runner, snapshot, budget)
    }
}
fn source_binding(snapshot: &snapshot::PreparedSnapshot) -> Result<String> {
    let request =
        serde_json::to_vec(snapshot.request()).map_err(|_| invalid("invalid snapshot request"))?;
    Ok(hash(
        &[snapshot.transport_head().as_bytes(), &request].concat(),
    ))
}
fn plan_binding(plan: &SourceRunPlan) -> String {
    let rows: Vec<_> = plan
        .buildkite_pipelines
        .iter()
        .map(|p| (&p.organization, &p.pipeline, &p.pipeline_id, &p.workflows))
        .collect();
    hash(&serde_json::to_vec(&(&plan.github_workflows, rows)).expect("string plan serializes"))
}
#[derive(Default)]
struct PollState {
    github: BTreeMap<String, GithubRunIdentity>,
    buildkite: BTreeMap<(String, String, String), ImportedBuildIdentity>,
}
#[derive(Deserialize)]
struct CommitStatus {
    id: u64,
    target_url: Option<String>,
}
#[derive(Deserialize)]
struct CommitCheck {
    id: u64,
    head_sha: String,
    details_url: Option<String>,
}
#[derive(Deserialize)]
struct CommitChecks {
    total_count: usize,
    check_runs: Vec<CommitCheck>,
}
fn get<T: serde::de::DeserializeOwned>(
    runner: &mut impl Runner,
    snapshot: &snapshot::PreparedSnapshot,
    path: &str,
    budget: &Budget<'_>,
) -> Result<T> {
    let bytes = checked(runner, provider::gh_spec(snapshot.root(), path), budget)?;
    serde_json::from_slice(&bytes).map_err(|_| invalid("commit build discovery schema mismatch"))
}
/// No URL from status content is requested. Only exact fixed-host route numbers
/// are retained, then the existing sealed public reader constructs its own URL.
fn build_number(url: &str, pipeline: &ImportedPipeline) -> Option<u64> {
    let prefix = format!(
        "https://buildkite.com/{}/{}/builds/",
        pipeline.organization, pipeline.pipeline
    );
    let suffix = url.strip_prefix(&prefix)?;
    // GitHub's native Buildkite checks link to a job within its build page.
    // Retain only the build locator; job identity is authenticated separately.
    let value = match suffix.split_once('#') {
        Some((number, job)) if uuid(job) => number,
        Some(_) => return None,
        None => suffix,
    };
    if value.is_empty() || value.starts_with('0') || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse::<u64>().ok().filter(|n| *n > 0)
}
fn locators(
    runner: &mut impl Runner,
    snapshot: &snapshot::PreparedSnapshot,
    budget: &Budget<'_>,
) -> Result<BTreeSet<String>> {
    let mut urls = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for page in 1..=20 {
        let rows: Vec<CommitStatus> = get(
            runner,
            snapshot,
            &format!(
                "{}/commits/{}/statuses?per_page=100&page={page}",
                snapshot.request().repository.api(),
                snapshot.transport_head()
            ),
            budget,
        )?;
        ensure(rows.len() <= 100, "oversized commit status page")?;
        let count = rows.len();
        for row in rows {
            ensure(
                row.id > 0 && ids.insert(row.id),
                "duplicate commit status pagination",
            )?;
            if let Some(url) = row.target_url {
                ensure(url.len() <= 2048, "oversized commit status locator")?;
                urls.insert(url);
            }
        }
        if count < 100 {
            break;
        }
        ensure(page < 20, "commit status pagination incomplete")?;
    }
    let mut ids = BTreeSet::new();
    let mut total = None;
    for page in 1..=20 {
        let rows: CommitChecks = get(
            runner,
            snapshot,
            &format!(
                "{}/commits/{}/check-runs?filter=all&per_page=100&page={page}",
                snapshot.request().repository.api(),
                snapshot.transport_head()
            ),
            budget,
        )?;
        ensure(
            rows.total_count <= 2000
                && rows.check_runs.len() <= 100
                && total.is_none_or(|n| n == rows.total_count),
            "commit check pagination changed or oversized",
        )?;
        total = Some(rows.total_count);
        for row in rows.check_runs {
            ensure(
                row.id > 0 && ids.insert(row.id) && row.head_sha == snapshot.transport_head(),
                "commit check foreign source or duplicate pagination",
            )?;
            if let Some(url) = row.details_url {
                ensure(url.len() <= 2048, "oversized commit check locator")?;
                urls.insert(url);
            }
        }
        ensure(ids.len() <= rows.total_count, "commit check count mismatch")?;
        if ids.len() == rows.total_count {
            break;
        }
        ensure(page < 20, "commit check pagination incomplete")?;
    }
    Ok(urls)
}
fn poll(
    runner: &mut impl Runner,
    reader: &mut impl buildkite::PublicReader,
    snapshot: &snapshot::PreparedSnapshot,
    plan: &SourceRunPlan,
    state: &mut PollState,
    budget: &Budget<'_>,
) -> Result<bool> {
    budget.remaining()?;
    let mut complete = true;
    for path in &plan.github_workflows {
        let Some(run) = github::discover_workflow_status(runner, snapshot, path, budget)? else {
            ensure(
                !state.github.contains_key(path),
                "previously discovered GitHub run disappeared",
            )?;
            complete = false;
            continue;
        };
        let identity = GithubRunIdentity {
            run_id: run.run_id(),
            attempt: run.run_attempt(),
        };
        ensure(
            state.github.get(path).is_none_or(|old| old == &identity),
            "GitHub run/attempt changed while polling",
        )?;
        state.github.insert(path.clone(), identity);
        if run.status() == "completed" {
            if run.conclusion() != Some("success") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "source GitHub workflow completed without success: workflow={path} run={} attempt={} status={} conclusion={} url=https://github.com/{}/actions/runs/{}/attempts/{}",
                        run.run_id(),
                        run.run_attempt(),
                        run.status(),
                        run.conclusion().unwrap_or("absent"),
                        snapshot.request().repository.full_name(),
                        run.run_id(),
                        run.run_attempt(),
                    ),
                ));
            }
        } else {
            complete = false;
        }
    }
    // Once every required workflow has an independently bound native build,
    // poll those public envelopes directly. These are locators, never cached
    // execution states. TerminalSourceRuns::recheck uses a fresh PollState, so
    // the final barrier always rediscovers the exact-S registry and rejects
    // replaced, missing or additional builds before accepting this set.
    let discovered = plan.buildkite_pipelines.iter().all(|pipeline| {
        pipeline.workflows.iter().all(|workflow| {
            state.buildkite.contains_key(&(
                pipeline.organization.clone(),
                pipeline.pipeline.clone(),
                workflow.clone(),
            ))
        })
    });
    let urls = if discovered {
        None
    } else {
        Some(locators(runner, snapshot, budget)?)
    };
    for pipeline in &plan.buildkite_pipelines {
        let numbers: BTreeSet<_> = if let Some(urls) = &urls {
            urls.iter()
                .filter_map(|url| build_number(url, pipeline))
                .collect()
        } else {
            pipeline
                .workflows
                .iter()
                .map(|workflow| {
                    state.buildkite[&(
                        pipeline.organization.clone(),
                        pipeline.pipeline.clone(),
                        workflow.clone(),
                    )]
                        .build
                        .number()
                })
                .collect()
        };
        let mut found = BTreeMap::new();
        for number in numbers {
            let build =
                buildkite::PublicBuild::new(&pipeline.organization, &pipeline.pipeline, number)?;
            let status = buildkite::discover_build_status(
                reader,
                build,
                snapshot,
                &pipeline.pipeline_id,
                budget,
            )?;
            if !pipeline
                .workflows
                .iter()
                .any(|w| w == status.workflow_path())
            {
                continue;
            }
            ensure(
                found
                    .insert(status.workflow_path().to_owned(), status)
                    .is_none(),
                "multiple imported builds for one exact snapshot workflow",
            )?;
        }
        for workflow in &pipeline.workflows {
            let key = (
                pipeline.organization.clone(),
                pipeline.pipeline.clone(),
                workflow.clone(),
            );
            let Some(build) = found.remove(workflow) else {
                ensure(
                    !state.buildkite.contains_key(&key),
                    "previously discovered imported build disappeared",
                )?;
                complete = false;
                continue;
            };
            let identity = ImportedBuildIdentity {
                build: build.build().clone(),
                id: build.build_id().into(),
            };
            ensure(
                state.buildkite.get(&key).is_none_or(|old| old == &identity),
                "imported workflow build identity changed while polling",
            )?;
            state.buildkite.insert(key, identity);
            let target = format!(
                "workflow={workflow} build={} uuid={} state={} blocked={} url=https://buildkite.com/{}/{}/builds/{}",
                build.build().number(),
                build.build_id(),
                build.state(),
                build.blocked(),
                pipeline.organization,
                pipeline.pipeline,
                build.build().number(),
            );
            if build.blocked() || build.state() == "blocked" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("source Buildkite build is blocked: {target}"),
                ));
            }
            match build.state() {
                "passed" => {}
                "creating" | "scheduled" | "running" | "started" | "failing" | "canceling" => {
                    complete = false
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("source Buildkite workflow completed without success: {target}"),
                    ));
                }
            }
        }
    }
    Ok(complete)
}
/// Wait without materializing artifacts/evidence or mutating any provider state.
/// One deadline covers all workflows, calls and sleeps. Authorization errors are
/// returned immediately; there is no fallback, credential extraction or retry.
pub fn wait_for_source_runs(
    runner: &mut impl Runner,
    reader: &mut impl buildkite::PublicReader,
    snapshot: &snapshot::PreparedSnapshot,
    plan: &SourceRunPlan,
    interval: Duration,
    budget: &Budget<'_>,
) -> Result<TerminalSourceRuns> {
    plan.validate()?;
    ensure(
        (Duration::from_secs(1)..=Duration::from_secs(60)).contains(&interval),
        "invalid metadata poll interval",
    )?;
    provider::verify_remote_binding(runner, snapshot, budget)?;
    let mut state = PollState::default();
    while !poll(runner, reader, snapshot, plan, &mut state, budget)? {
        budget.wait(interval)?;
    }
    provider::verify_remote_binding(runner, snapshot, budget)?;
    let result = TerminalSourceRuns {
        source_binding: source_binding(snapshot)?,
        plan_binding: plan_binding(plan),
        github: state.github,
        buildkite: state.buildkite,
    };
    result.recheck(runner, reader, snapshot, plan, budget)?;
    Ok(result)
}

#[cfg(test)]
mod tests;

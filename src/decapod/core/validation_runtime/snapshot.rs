//! Current-workbranch preparation, host handoff, publication and collection.
//! A single bounded invocation owns submit, terminal polling and authenticated
//! source custody. Missing obligations remain non-ready. No caller green is read.
use super::*;
use crate::core::snapshot_provider::{self as provider, Repository, Runner};
use provider::{automatic, provider as collect, snapshot as transport};
// Preserved predictive release adapter for #1399; core readiness never calls it.
#[cfg(test)]
mod automatic_tests;
pub mod ci_attested;
#[allow(dead_code)]
mod derived_import;
mod integration;
mod source_import;
#[allow(dead_code)]
mod source_policy;

#[derive(Debug, Clone, Copy)]
pub enum SnapshotMode {
    /// Persist exact H/T/B/current-ref/tags for an authorized same-branch push.
    PrepareForHost,
    /// Existing official authentication or a trusted compiled host Runner.
    PublishAndCollect,
    /// Verify the externally pushed current branch, then collect exact-H proof.
    /// The real workspace branch is retained and must never be cleaned up.
    ResumeAndCollect,
}

/// A fixed importer setup reviewed independently before candidate outcomes.
/// No Deserialize: observed plan metadata cannot authorize its own runtime.
#[derive(Clone, Debug)]
pub struct ReviewedBuildkiteImporter {
    pub compiler_version: String,
    pub compiler_digest: String,
    pub linux_runtime_digest: String,
    pub macos_runtime_digest: String,
}
impl ReviewedBuildkiteImporter {
    /// Independently verified official buildkite/buildkite-gha v0.102.7 assets:
    /// Linux x86_64 asset642891563 (archive900a060862cb...) and Darwin arm64
    /// asset642891564 (archive2ae5b7c4218e...). Digests bind executable members,
    /// not observed job outcomes. The Linux compiler emits both runtime plans.
    pub fn reviewed() -> Self {
        Self {
            compiler_version: "0.102.7".into(),
            compiler_digest:
                "sha256:0e67be8be661f75d4abd69b115a746905664f54560c9b27aab74b4c1cdcc8692".into(),
            linux_runtime_digest:
                "sha256:0e67be8be661f75d4abd69b115a746905664f54560c9b27aab74b4c1cdcc8692".into(),
            macos_runtime_digest:
                "sha256:a887b7bd1453715cfb3bccaeab5c063c12e54f446f281d0d1837b12be0bdf328".into(),
        }
    }
    fn validate(&self) -> io::Result<()> {
        let reviewed = Self::reviewed();
        if self.compiler_version != reviewed.compiler_version
            || self.compiler_digest != reviewed.compiler_digest
            || self.linux_runtime_digest != reviewed.linux_runtime_digest
            || self.macos_runtime_digest != reviewed.macos_runtime_digest
        {
            return Err(invalid("unsupported unreviewed Buildkite importer setup"));
        }
        Ok(())
    }
    /// Select from the independently authored matrix, never the observed plan.
    fn runtime_digest_for(&self, matrix: &BTreeMap<String, String>) -> io::Result<&str> {
        match matrix.get("os").map(String::as_str) {
            None | Some("ubuntu-latest") => Ok(&self.linux_runtime_digest),
            Some("macos-latest") => Ok(&self.macos_runtime_digest),
            Some(_) => Err(invalid("unsupported unreviewed Buildkite runtime platform")),
        }
    }
}
struct PublicBuildkiteReader<'a>(Box<dyn provider::public_buildkite::PublicReader + 'a>);
impl provider::public_buildkite::PublicReader for PublicBuildkiteReader<'_> {
    fn get(
        &mut self,
        request: &provider::public_buildkite::PublicRequest,
        budget: &provider::Budget<'_>,
    ) -> io::Result<Vec<u8>> {
        self.0.get(request, budget)
    }
    fn get_many(
        &mut self,
        requests: &[provider::public_buildkite::PublicRequest],
        budget: &provider::Budget<'_>,
    ) -> io::Result<Vec<Vec<u8>>> {
        self.0.get_many(requests, budget)
    }
    fn get_custody_many(
        &mut self,
        requests: &[provider::public_buildkite::CustodyRead],
        budget: &provider::Budget<'_>,
    ) -> io::Result<Vec<provider::public_buildkite::CustodyResponse>> {
        self.0.get_custody_many(requests, budget)
    }
}

/// Real transport host. The complete source catalog selects the provider runs;
/// terminal provider metadata is only a collection barrier, never readiness.
/// Every required key still needs actual authenticated preparation/execution.
/// The optional legacy `remote_plan` is compiled policy, not caller job JSON.
pub struct SnapshotHost<'a, R: Runner> {
    options: Options,
    repository: Repository,
    runner: R,
    project_check: PrerequisiteCallback<'a>,
    mode: SnapshotMode,
    resume_file: Option<PathBuf>,
    remote_plan: Option<collect::RemotePlan>,
    verified_checks: BTreeMap<CheckKey, source_import::VerifiedCheck>,
    prepared_snapshot: Option<transport::PreparedSnapshot>,
    terminal_sources: Option<(automatic::SourceRunPlan, automatic::TerminalSourceRuns)>,
    public_buildkite: Option<PublicBuildkiteReader<'a>>,
    buildkite_contract: Option<ReviewedBuildkiteImporter>,
    evidence_directory: Option<PathBuf>,
}
impl<'a, R: Runner> SnapshotHost<'a, R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new<P>(
        options: Options,
        repository: Repository,
        runner: R,
        mode: SnapshotMode,
        resume_file: Option<PathBuf>,
        remote_plan: Option<collect::RemotePlan>,
        project_check: P,
    ) -> io::Result<Self>
    where
        P: FnMut(&RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> + 'a,
    {
        if ![repository.full_name(), repository.https_url()].contains(&options.repository) {
            return Err(invalid(
                "snapshot repository differs from canonical project identity",
            ));
        }
        if matches!(mode, SnapshotMode::ResumeAndCollect) != resume_file.is_some() {
            return Err(invalid(
                "resume requires exactly the existing private handoff file",
            ));
        }
        if let Some(plan) = &remote_plan {
            plan.validate()?;
        }
        Ok(Self {
            options,
            repository,
            runner,
            mode,
            resume_file,
            remote_plan,
            verified_checks: BTreeMap::new(),
            prepared_snapshot: None,
            terminal_sources: None,
            public_buildkite: None,
            buildkite_contract: None,
            project_check: Box::new(project_check),
            evidence_directory: None,
        })
    }
    /// GitHub raw ZIP handoff is outside Buildkite-only preflight. Buildkite
    /// artifacts are collected automatically from their authenticated producer.
    pub fn with_artifact_directory(self, _directory: PathBuf) -> io::Result<Self> {
        Err(invalid(
            "GITHUB_ARTIFACT_HANDOFF_NOT_APPLICABLE: preflight collects Buildkite artifacts automatically",
        ))
    }
    /// The importer setup is reviewed compiled policy, not caller JSON. The
    /// public reader preserves the host's typed read authorization callback.
    pub fn with_public_buildkite_reader(
        mut self,
        reader: impl provider::public_buildkite::PublicReader + 'a,
        contract: ReviewedBuildkiteImporter,
    ) -> io::Result<Self> {
        contract.validate()?;
        self.public_buildkite = Some(PublicBuildkiteReader(Box::new(reader)));
        self.buildkite_contract = Some(contract);
        Ok(self)
    }
    pub fn evidence_directory(&self) -> Option<&Path> {
        self.evidence_directory.as_deref()
    }
    fn stage(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<Vec<Blocker>> {
        if !project.configuration.validation_buildkite_preflights {
            return Err(invalid(
                "snapshot host requires the canonical true remote-preflight policy",
            ));
        }
        let provider_budget = provider::Budget::new(
            budget.remaining()?,
            budget.output_limit(),
            budget.cancellation(),
        )?;
        let state = native_proof::capture_current_identity(
            &project.identity_inputs,
            &BTreeMap::new(),
            suite_limits(&self.options, budget)?,
            budget.cancel,
        )?;
        let actual_target = transport::accepted_target(
            &mut self.runner,
            &self.options.root,
            &self.repository,
            &format!("refs/heads/{}", self.options.base_branch),
            &provider_budget,
        )?;
        if actual_target.as_deref() != Some(state.accepted_base_revision.as_str()) {
            return Err(invalid(
                "STALE_ACCEPTED_BASE: target changed before snapshot preparation",
            ));
        }
        let mut request = transport::SnapshotRequest {
            repository: self.repository.clone(),
            request_id: new_uuid()?,
            source_ref: format!("refs/heads/{}", state.work_branch),
            source_head: state.head_revision.clone(),
            source_tree: text(git(&self.options, budget, &["rev-parse", "HEAD^{tree}"])?)?,
            accepted_base: state.accepted_base_revision.clone(),
            accepted_base_ref: format!("refs/heads/{}", self.options.base_branch),
            tags: transport::capture_local_tags(
                &mut self.runner,
                &self.options.root,
                &provider_budget,
            )?,
        };
        let prepared = if let Some(path) = &self.resume_file {
            let record = read_handoff(path)?;
            request.request_id = record
                .pointer("/request/request_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("handoff lacks UUID locator"))?
                .into();
            let head = record
                .get("transport_head")
                .and_then(|v| v.as_str())
                .ok_or_else(|| invalid("handoff lacks S locator"))?;
            require_workbranch_head(&request, head)?;
            // All repository/branch/H/T/B/tag fields above were independently
            // remeasured. The file only locates the same committed workbranch.
            let prepared = transport::resume(
                &mut self.runner,
                &self.options.root,
                request,
                head,
                &provider_budget,
            )?;
            if serde_json::to_value(&prepared).map_err(io::Error::other)? != record {
                return Err(invalid(
                    "resume handoff differs from independently rebuilt source identity",
                ));
            }
            collect::verify_remote_binding(&mut self.runner, &prepared, &provider_budget)?;
            prepared
        } else {
            transport::prepare(
                &mut self.runner,
                &self.options.root,
                request,
                &provider_budget,
            )?
        };
        let directory = private_tempdir(&self.options.evidence_parent, "decapod-snapshot-")?.keep();
        self.evidence_directory = Some(directory.clone());
        write_new(
            &directory.join("snapshot-handoff.json"),
            &serde_json::to_vec_pretty(&prepared).map_err(io::Error::other)?,
        )?;
        write_new(
            &directory.join("required-catalog.json"),
            &serde_json::to_vec_pretty(&project.catalog).map_err(io::Error::other)?,
        )?;
        let mut blockers:Vec<_>=project.catalog.obligations().iter().map(|obligation|Blocker{
            code:"REQUIRED_PROOF_NOT_AUTHENTICATED".into(),detail:"required current execution has not yet supplied complete authenticated measurements".into(),check:Some(obligation.key.clone()),
        }).collect();
        if matches!(self.mode, SnapshotMode::PrepareForHost) {
            blockers.insert(0,Blocker::new("SNAPSHOT_UPLOAD_PENDING",format!(
                "authorized host must push the existing workspace branch {} at exact source {}; resume with {} after exact readback. Only same-name non-forced creation/fast-forward is allowed. This does not establish readiness or open a PR",
                prepared.request().transport_ref(),prepared.transport_head(),directory.join("snapshot-handoff.json").display())));
            return Ok(blockers);
        }
        if let Some(plan) = &self.remote_plan {
            verify_partial_plan(project, plan)?;
        }
        let (snapshot, publish_error) = if matches!(self.mode, SnapshotMode::PublishAndCollect) {
            let attempt = transport::publish(&mut self.runner, prepared, &provider_budget);
            (attempt.snapshot, attempt.outcome.err())
        } else {
            (prepared, None)
        };
        if let Some(error) = publish_error {
            blockers.insert(
                0,
                Blocker::new("SNAPSHOT_UPLOAD_INCOMPLETE", error.to_string()),
            );
        } else {
            let plan = source_import::source_run_plan(project)?;
            let reader = self.public_buildkite.as_mut().ok_or_else(|| invalid(
                "BUILDKITE_PUBLIC_READER_REQUIRED: configure the reviewed typed public reader before collecting source proof"
            ))?;
            let contract = self
                .buildkite_contract
                .as_ref()
                .ok_or_else(|| invalid("reviewed Buildkite importer contract is missing"))?;
            // All provider calls and sleeps share the original deadline. The
            // barrier fetches metadata only; custody is collected exactly once.
            eprintln!("Buildkite validation: stage=provider_terminal_wait");
            let provider_started = Instant::now();
            let terminal = automatic::wait_for_source_runs(
                &mut self.runner,
                reader,
                &snapshot,
                &plan,
                Duration::from_secs(5),
                &provider_budget,
            );
            let provider_terminal_wait_ms = provider_started.elapsed().as_millis();
            eprintln!(
                "Buildkite validation: stage=provider_terminal_wait elapsed_ms={provider_terminal_wait_ms}"
            );
            let terminal = terminal?;
            let collection_started = Instant::now();
            let buildkite = if let Some(policy) = &project.configuration.ci_attested {
                ci_attested::collect(
                    reader,
                    &snapshot,
                    project,
                    &terminal,
                    &state,
                    policy,
                    &provider_budget,
                )
            } else {
                source_import::buildkite_sources(
                    reader,
                    contract,
                    &snapshot,
                    project,
                    &terminal,
                    &directory,
                    &provider_budget,
                )
            };
            let custody_collection_ms = collection_started.elapsed().as_millis();
            eprintln!(
                "Buildkite validation: stage=custody_collection elapsed_ms={custody_collection_ms}"
            );
            let buildkite = buildkite?;
            self.verified_checks = buildkite.checks;
            blockers.extend(buildkite.blockers);
            blockers.retain(|blocker| {
                blocker
                    .check
                    .as_ref()
                    .is_none_or(|key| !self.verified_checks.contains_key(key))
            });
            let recheck_started = Instant::now();
            terminal.recheck(&mut self.runner, reader, &snapshot, &plan, &provider_budget)?;
            write_new(
                &directory.join("buildkite-stage-timing.json"),
                &serde_json::to_vec_pretty(&serde_json::json!({
                    "schema": "decapod.buildkite-stage-timing.v1",
                    "provider_terminal_wait_ms": provider_terminal_wait_ms,
                    "custody_collection_ms": custody_collection_ms,
                    "terminal_recheck_ms": recheck_started.elapsed().as_millis(),
                    "source_head": snapshot.transport_head(),
                    "measurement_only": true
                }))
                .map_err(io::Error::other)?,
            )?;
            self.terminal_sources = Some((plan, terminal));
        }
        // Repeated validation reuses the real workspace branch and independently
        // recollects current proof. A local locator never authorizes readiness or
        // deletion of the workbranch after publication or abandonment.
        write_new(&directory.join("cleanup-instructions.txt"), format!(
            "Retained real workspace branch {} at {}. Reuse snapshot-handoff.json for fresh proof. Cleanup is read-only and never deletes this branch. Old temporary refs require separate explicit cleanup authorization.\n",
            snapshot.request().transport_ref(), snapshot.transport_head()).as_bytes())?;
        let after = native_proof::capture_current_identity(
            &project.identity_inputs,
            &BTreeMap::new(),
            suite_limits(&self.options, budget)?,
            budget.cancel,
        )?;
        if after != state {
            return Err(invalid("SOURCE_CHANGED_DURING_SNAPSHOT_PROOF"));
        }
        let current = transport::accepted_target(
            &mut self.runner,
            &self.options.root,
            &self.repository,
            &format!("refs/heads/{}", self.options.base_branch),
            &provider_budget,
        )?;
        if current.as_deref() != Some(state.accepted_base_revision.as_str()) {
            return Err(invalid(
                "STALE_ACCEPTED_BASE: target changed during snapshot proof",
            ));
        }
        self.prepared_snapshot = Some(snapshot);
        Ok(blockers)
    }
}
impl<R: Runner> Host for SnapshotHost<'_, R> {
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
            return Err(invalid("snapshot target query identity changed"));
        }
        transport::accepted_target(
            &mut self.runner,
            &self.options.root,
            &self.repository,
            &format!("refs/heads/{branch}"),
            &provider::Budget::new(
                budget.remaining()?,
                budget.output_limit(),
                budget.cancellation(),
            )?,
        )?
        .ok_or_else(|| invalid("accepted target branch is absent"))
    }
    fn select(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>> {
        match self.stage(project, budget) {
            Ok(blockers) if blockers.is_empty() => {
                if self.verified_checks.len() != project.catalog.obligations().len() {
                    return Err(vec![Blocker::new(
                        "INCOMPLETE_AUTHENTICATED_CATALOG",
                        "required proof inventory incomplete",
                    )]);
                }
                Ok(ExecutionSelection {
                    policy: ExecutionPolicy {
                        buildkite_preflights: true,
                        executors: self
                            .verified_checks
                            .iter()
                            .map(|(key, check)| (key.clone(), check.binding.executor))
                            .collect(),
                    },
                    bindings: self
                        .verified_checks
                        .iter()
                        .map(|(key, check)| (key.clone(), check.binding.clone()))
                        .collect(),
                    native_invocations: BTreeMap::new(),
                })
            }
            Ok(blockers) => Err(blockers),
            Err(error) => Err(vec![Blocker::new("SNAPSHOT_NOT_READY", error.to_string())]),
        }
    }
    fn execute(
        &mut self,
        project: &CurrentProject,
        selection: &ExecutionSelection,
        plan: &evidence::ExpectedPlan,
        identity: &evidence::MaterialIdentity,
        _: &Path,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite> {
        let snapshot = self
            .prepared_snapshot
            .as_ref()
            .ok_or_else(|| invalid("prepared transport custody missing"))?;
        if self.verified_checks.len() != plan.checks.len() {
            return Err(invalid(
                "complete authenticated execution inventory required",
            ));
        }
        let provider_budget = provider::Budget::new(
            budget.remaining()?,
            budget.output_limit(),
            budget.cancellation(),
        )?;
        collect::verify_remote_binding(&mut self.runner, snapshot, &provider_budget)?;
        let mut results = Vec::new();
        for expected in &plan.checks {
            let actual = self
                .verified_checks
                .get(&expected.key)
                .ok_or_else(|| invalid("required authenticated execution missing"))?;
            if actual.binding.definition_digest != expected.definition_digest
                || actual.binding.invocation_digest != expected.invocation_digest
                || actual.binding.toolchain_digest != expected.toolchain_digest
                || actual.binding.executor != expected.executor
            {
                return Err(invalid("authenticated execution differs from frozen plan"));
            }
            results.push(evidence::CheckResult {
                key: expected.key.clone(),
                definition_digest: actual.binding.definition_digest.clone(),
                invocation_digest: actual.binding.invocation_digest.clone(),
                toolchain_digest: actual.binding.toolchain_digest.clone(),
                executor: actual.binding.executor,
                identity: identity.clone(),
                observation: actual.observation.clone(),
            });
        }
        let mut after = native_proof::capture_current_identity(
            &project.identity_inputs,
            &selection.native_invocations,
            suite_limits(&self.options, budget)?,
            budget.cancellation(),
        )?;
        bind_remote_toolchains(&mut after, selection)?;
        if &after != identity {
            return Err(invalid(
                "source changed after authenticated snapshot execution",
            ));
        }
        let (source_plan, terminal) = self
            .terminal_sources
            .as_ref()
            .ok_or_else(|| invalid("complete terminal provider identity is missing"))?;
        let reader = self
            .public_buildkite
            .as_mut()
            .ok_or_else(|| invalid("public Buildkite reader is missing"))?;
        terminal.recheck(
            &mut self.runner,
            reader,
            snapshot,
            source_plan,
            &provider_budget,
        )?;
        Ok(ExecutedSuite {
            evidence: SuiteEvidence {
                schema_version: evidence::SCHEMA_VERSION.into(),
                expected_plan_digest: plan.digest().map_err(io::Error::other)?,
                before: identity.clone(),
                after,
                facts: project.facts.clone(),
                results,
            },
            failure: None,
        })
    }
    fn evidence_directory(&self) -> Option<&Path> {
        self.evidence_directory.as_deref()
    }
}
fn require_workbranch_head(request: &transport::SnapshotRequest, head: &str) -> io::Result<()> {
    if head != request.source_head {
        return Err(invalid(
            "LEGACY_TEMPORARY_SNAPSHOT_UNSUPPORTED: validation must use the exact current workspace branch commit; prepare a new same-branch handoff",
        ));
    }
    Ok(())
}

fn read_handoff(path: &Path) -> io::Result<serde_json::Value> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 1024 * 1024 {
        return Err(invalid(
            "resume handoff must be a bounded regular local file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(invalid("resume handoff is not private"));
        }
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("handoff changed type"));
        }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(invalid("handoff exceeds bound"));
        }
        serde_json::from_slice(&bytes).map_err(io::Error::other)
    }
    #[cfg(not(unix))]
    serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)
}

/// Verify an authorized host has pushed the exact current workbranch commit.
/// The original handoff is only a locator: rebuild current committed source and
/// require H=S before the provider independently checks the same branch.
/// Returns a NEW private handoff file, never readiness or a publication permit.
pub fn adopt_host_handoff(
    options: &Options,
    repository: &Repository,
    runner: &mut impl Runner,
    prior_handoff: &Path,
    observed_s: &str,
    cancellation: &Cancellation,
) -> io::Result<PathBuf> {
    if ![repository.full_name(), repository.https_url()].contains(&options.repository) {
        return Err(invalid(
            "adoption repository differs from canonical project",
        ));
    }
    let budget = RuntimeBudget {
        deadline: Instant::now() + options.timeout.min(Duration::from_secs(120)),
        cancel: cancellation,
        max_output_bytes: options.max_output_bytes,
        bytes_left: options.max_input_bytes,
    };
    validate_options(options, &budget)?;
    clean(options, &budget)?;
    let record = read_handoff(prior_handoff)?;
    let locator = |pointer: &str| {
        record
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("handoff lacks immutable locator"))
    };
    let provider_budget =
        provider::Budget::new(budget.remaining()?, budget.output_limit(), cancellation)?;
    let request = transport::SnapshotRequest {
        repository: repository.clone(),
        request_id: locator("/request/request_id")?.into(),
        source_ref: text(git(options, &budget, &["symbolic-ref", "HEAD"])?)?,
        source_head: text(git(
            options,
            &budget,
            &["rev-parse", "--verify", "HEAD^{commit}"],
        )?)?,
        source_tree: text(git(
            options,
            &budget,
            &["rev-parse", "--verify", "HEAD^{tree}"],
        )?)?,
        accepted_base: text(git(
            options,
            &budget,
            &[
                "rev-parse",
                "--verify",
                &format!("{}^{{commit}}", options.accepted_base_ref),
            ],
        )?)?,
        accepted_base_ref: format!("refs/heads/{}", options.base_branch),
        tags: transport::capture_local_tags(runner, &options.root, &provider_budget)?,
    };
    require_workbranch_head(&request, locator("/transport_head")?)?;
    require_workbranch_head(&request, observed_s)?;
    let prior = transport::resume(
        runner,
        &options.root,
        request,
        locator("/transport_head")?,
        &provider_budget,
    )?;
    if serde_json::to_value(&prior).map_err(io::Error::other)? != record {
        return Err(invalid(
            "adoption handoff differs from independently rebuilt source identity",
        ));
    }
    let adopted = transport::adopt_host_snapshot(runner, &prior, observed_s, &provider_budget)?;
    budget.remaining()?;
    if adopted.transport_head() != observed_s
        || adopted.root() != prior.root()
        || adopted.request() != prior.request()
    {
        return Err(invalid("provider adoption changed owned source request"));
    }
    // Recheck current local source after the final provider read as well.
    // Adoption produces only a locator, but even that locator must stay bound.
    clean(options, &budget)?;
    let adopted = transport::resume(
        runner,
        &options.root,
        adopted.request().clone(),
        observed_s,
        &provider_budget,
    )?;
    // A concurrently replaced locator is not silently overwritten or reused.
    if read_handoff(prior_handoff)? != record {
        return Err(invalid("original handoff changed during adoption"));
    }
    persist_adopted_handoff(options, &prior, &adopted)
}

fn persist_adopted_handoff(
    options: &Options,
    prior: &transport::PreparedSnapshot,
    adopted: &transport::PreparedSnapshot,
) -> io::Result<PathBuf> {
    let directory = private_tempdir(&options.evidence_parent, "decapod-snapshot-adopted-")?;
    let path = directory.path().join("snapshot-handoff.json");
    write_new(
        &path,
        &serde_json::to_vec_pretty(adopted).map_err(io::Error::other)?,
    )?;
    write_new(&directory.path().join("host-adoption.json"), &serde_json::to_vec_pretty(&serde_json::json!({
        "schema": "decapod.snapshot-host-adoption.v1",
        "ready": false,
        "scope": READINESS_SCOPE,
        "request_id": adopted.request().request_id,
        "prior_transport_head": prior.transport_head(),
        "adopted_transport_head": adopted.transport_head(),
        "next_action": "Run full validate with this snapshot-handoff.json; the same workbranch H still requires fresh complete provider evidence"
    })).map_err(io::Error::other)?)?;
    let _ = directory.keep();
    Ok(path)
}

/// Compatibility cleanup after publication or abandonment: verify the locator,
/// then retain the real workspace branch. This API never deletes a branch and
/// cannot be used to clean historical temporary refs.
pub fn cleanup_handoff(
    options: &Options,
    repository: &Repository,
    runner: &mut impl Runner,
    handoff: &Path,
    cancellation: &Cancellation,
) -> io::Result<transport::Cleanup> {
    if ![repository.full_name(), repository.https_url()].contains(&options.repository) {
        return Err(invalid("cleanup repository differs from canonical project"));
    }
    let budget = RuntimeBudget {
        deadline: Instant::now() + options.timeout.min(Duration::from_secs(30)),
        cancel: cancellation,
        max_output_bytes: options.max_output_bytes,
        bytes_left: options.max_input_bytes,
    };
    clean(options, &budget)?;
    let record = read_handoff(handoff)?;
    let locator = |pointer: &str| {
        record
            .pointer(pointer)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("handoff lacks immutable locator"))
    };
    let provider_budget =
        provider::Budget::new(budget.remaining()?, budget.output_limit(), cancellation)?;
    let request = transport::SnapshotRequest {
        repository: repository.clone(),
        request_id: locator("/request/request_id")?.into(),
        source_ref: text(git(options, &budget, &["symbolic-ref", "HEAD"])?)?,
        source_head: text(git(options, &budget, &["rev-parse", "HEAD"])?)?,
        source_tree: text(git(options, &budget, &["rev-parse", "HEAD^{tree}"])?)?,
        accepted_base: text(git(
            options,
            &budget,
            &["rev-parse", &options.accepted_base_ref],
        )?)?,
        accepted_base_ref: format!("refs/heads/{}", options.base_branch),
        tags: transport::capture_local_tags(runner, &options.root, &provider_budget)?,
    };
    require_workbranch_head(&request, locator("/transport_head")?)?;
    let snapshot = transport::resume(
        runner,
        &options.root,
        request,
        locator("/transport_head")?,
        &provider_budget,
    )?;
    if serde_json::to_value(&snapshot).map_err(io::Error::other)? != record {
        return Err(invalid(
            "cleanup locator differs from independently revalidated owned snapshot",
        ));
    }
    transport::cleanup(runner, &snapshot, &provider_budget)
}

fn verify_partial_plan(project: &CurrentProject, plan: &collect::RemotePlan) -> io::Result<()> {
    plan.validate()?;
    let expected: BTreeMap<_, _> = project
        .catalog
        .obligations()
        .iter()
        .map(|obligation| serde_json::to_string(&obligation.key).map(|key| (key, obligation)))
        .collect::<Result<_, _>>()
        .map_err(io::Error::other)?;
    for workflow in &plan.github {
        for job in &workflow.jobs {
            let obligation = expected
                .get(&job.key)
                .ok_or_else(|| invalid("remote plan has a noncatalog check key"))?;
            if obligation.key.provider != evidence::Provider::GithubActions
                || obligation.key.workflow != workflow.path
                || job.contract.source_definition_digest != obligation.definition_digest
                || job.contract.profile != obligation.key.profile
                || job.contract.job != obligation.key.job
                || job.contract.matrix != obligation.key.matrix
            {
                return Err(invalid(
                    "GitHub mapping differs from independent source catalog",
                ));
            }
        }
    }
    if !plan.buildkite.is_empty() {
        return Err(invalid(
            "BUILDKITE_IMPORTED_SOURCE_ADAPTER_REQUIRED: native wrapper success is not imported source execution",
        ));
    }
    Ok(())
}
fn new_uuid() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use provider::{Budget, CommandSpec, OfficialCli, Program, Reply};
    struct LocalOnly {
        official: OfficialCli,
        base: String,
        remote_reads: usize,
    }
    impl Runner for LocalOnly {
        fn execute(&mut self, command: &CommandSpec, budget: &Budget<'_>) -> io::Result<Reply> {
            if command.program() == Program::GitTransport {
                if command.arguments().iter().any(|arg| arg == "config") {
                    return provider::fixture_config::execute(&mut self.official, command, budget);
                }
                assert!(
                    command.arguments().iter().any(|arg| arg == "ls-remote"),
                    "no remote mutation in this test"
                );
                self.remote_reads += 1;
                return Ok(Reply {
                    exit_code: 0,
                    stdout: if command
                        .arguments()
                        .last()
                        .is_some_and(|arg| arg == "refs/heads/master")
                    {
                        format!("{}\trefs/heads/master\n", self.base).into_bytes()
                    } else {
                        Vec::new()
                    },
                });
            }
            assert_eq!(
                command.program(),
                Program::GitLocal,
                "no provider request in prepare-only mode"
            );
            self.official.execute(command, budget)
        }
    }
    pub(super) fn command(root: &Path, args: &[&str]) -> String {
        let output = Command::new("/usr/bin/git")
            .current_dir(root)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().into()
    }
    // The adopter's provider binding remains real production code. This fixture
    // substitutes only public metadata/transport reads; the exact workbranch
    // commit/tree verification uses actual local Git bytes. No synthetic object
    // or auxiliary branch is created or adopted.
    struct AdoptionReader {
        local: LocalOnly,
        root: PathBuf,
        request: transport::SnapshotRequest,
        server_s: String,
        fetches: usize,
    }
    impl Runner for AdoptionReader {
        fn execute(&mut self, spec: &CommandSpec, budget: &Budget<'_>) -> io::Result<Reply> {
            if let Some(provider::ProviderRequest::GithubGet { path }) = spec.provider_request() {
                let prefix = format!("repos/{}/git/", self.request.repository.full_name());
                let endpoint = path
                    .strip_prefix(&prefix)
                    .ok_or_else(|| invalid("unexpected test endpoint"))?;
                let value = if let Some(id) = endpoint.strip_prefix("commits/") {
                    assert!(
                        [self.server_s.as_str(), self.request.source_head.as_str()].contains(&id)
                    );
                    let raw = command(&self.root, &["cat-file", "commit", id]);
                    let (headers, message) = raw.split_once("\n\n").unwrap();
                    serde_json::json!({"sha": id,
                        "tree":{"sha": headers.lines().find_map(|line| line.strip_prefix("tree ")).unwrap()},
                        "parents": headers.lines().filter_map(|line| line.strip_prefix("parent ")).map(|id|serde_json::json!({"sha":id})).collect::<Vec<_>>(),
                        "message": message})
                } else {
                    let reference = format!("refs/{}", endpoint.strip_prefix("ref/").unwrap());
                    let id = if reference == self.request.accepted_base_ref {
                        &self.request.accepted_base
                    } else {
                        assert_eq!(reference, self.request.transport_ref());
                        &self.server_s
                    };
                    serde_json::json!({"ref":reference,"object":{"sha":id,"type":"commit"}})
                };
                return Ok(Reply {
                    exit_code: 0,
                    stdout: serde_json::to_vec(&value).unwrap(),
                });
            }
            if spec.program() == Program::GitTransport {
                if spec.arguments().iter().any(|arg| arg == "fetch") {
                    self.fetches += 1;
                    assert_eq!(spec.arguments().last().unwrap(), &self.server_s);
                    assert!(
                        spec.arguments()
                            .iter()
                            .any(|arg| arg == "--no-write-fetch-head")
                    );
                    return Ok(Reply {
                        exit_code: 0,
                        stdout: Vec::new(),
                    });
                }
                if spec
                    .arguments()
                    .last()
                    .is_some_and(|arg| arg == &self.request.transport_ref())
                {
                    return Ok(Reply {
                        exit_code: 0,
                        stdout: format!("{}\t{}\n", self.server_s, self.request.transport_ref())
                            .into_bytes(),
                    });
                }
            }
            self.local.execute(spec, budget)
        }
    }
    fn check_adoption(
        options: &Options,
        repository: &Repository,
        handoff: &Path,
        record: &serde_json::Value,
    ) {
        let original = fs::read(handoff).unwrap();
        let before_index = fs::read(options.root.join(".git/index")).unwrap();
        let before_refs = command(
            &options.root,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
        );
        let request = transport::SnapshotRequest {
            repository: repository.clone(),
            request_id: record["request"]["request_id"].as_str().unwrap().into(),
            source_ref: command(&options.root, &["symbolic-ref", "HEAD"]),
            source_head: command(&options.root, &["rev-parse", "HEAD"]),
            source_tree: command(&options.root, &["rev-parse", "HEAD^{tree}"]),
            accepted_base: command(&options.root, &["rev-parse", &options.accepted_base_ref]),
            accepted_base_ref: format!("refs/heads/{}", options.base_branch),
            tags: BTreeMap::new(),
        };
        let server_s = request.source_head.clone();
        assert_eq!(server_s, record["transport_head"].as_str().unwrap());
        let mut reader = AdoptionReader {
            local: LocalOnly {
                official: OfficialCli::git_only(options.git.clone(), |_, _| Ok(())).unwrap(),
                base: request.accepted_base.clone(),
                remote_reads: 0,
            },
            root: options.root.clone(),
            request: request.clone(),
            server_s: server_s.clone(),
            fetches: 0,
        };
        let cancel = Cancellation::default();
        let adopted = adopt_host_handoff(
            options,
            repository,
            &mut reader,
            handoff,
            &server_s,
            &cancel,
        )
        .unwrap();
        assert_ne!(adopted, handoff);
        assert_eq!(reader.fetches, 0);
        let new_record = read_handoff(&adopted).unwrap();
        let mut expected = record.clone();
        expected["transport_head"] = serde_json::Value::String(server_s.clone());
        assert_eq!(new_record, expected);
        assert_eq!(fs::read(handoff).unwrap(), original);
        assert_eq!(
            fs::read(options.root.join(".git/index")).unwrap(),
            before_index
        );
        assert_eq!(
            command(
                &options.root,
                &["for-each-ref", "--format=%(refname) %(objectname)"]
            ),
            before_refs
        );
        assert!(!options.root.join(".git/FETCH_HEAD").exists());
        assert_eq!(command(&options.root, &["status", "--porcelain"]), "");
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(adopted.with_file_name("host-adoption.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(report["ready"], false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                adopted.metadata().unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                adopted
                    .parent()
                    .unwrap()
                    .metadata()
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        let mut legacy = record.clone();
        legacy["transport_head"] = serde_json::Value::String("e".repeat(40));
        let legacy_path = handoff.with_file_name("legacy-transport-handoff.json");
        write_new(&legacy_path, &serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert!(
            adopt_host_handoff(
                options,
                repository,
                &mut reader,
                &legacy_path,
                &server_s,
                &cancel
            )
            .unwrap_err()
            .to_string()
            .contains("LEGACY_TEMPORARY_SNAPSHOT_UNSUPPORTED")
        );
        let mut wrong = record.clone();
        wrong["request"]["source_head"] = serde_json::Value::String("f".repeat(40));
        let bad = handoff.with_file_name("adoption-wrong-handoff.json");
        write_new(&bad, &serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(
            adopt_host_handoff(options, repository, &mut reader, &bad, &server_s, &cancel).is_err()
        );
        let saved = fs::read(options.root.join("source")).unwrap();
        fs::write(options.root.join("source"), "dirty").unwrap();
        assert!(
            adopt_host_handoff(
                options,
                repository,
                &mut reader,
                handoff,
                &server_s,
                &cancel
            )
            .is_err()
        );
        fs::write(options.root.join("source"), saved).unwrap();
        command(
            &options.root,
            &["commit", "--allow-empty", "-m", "new committed source"],
        );
        assert!(
            adopt_host_handoff(
                options,
                repository,
                &mut reader,
                handoff,
                &server_s,
                &cancel
            )
            .is_err()
        );
        assert_eq!(
            reader.fetches, 0,
            "same-branch adoption never fetches a synthetic transport object"
        );
        assert_eq!(fs::read(handoff).unwrap(), original);
    }
    // Synthetic catalog exercises controller behavior only. Snapshot H/T/base,
    // Git bytes/ref/index preservation and handoff material are real local Git.
    pub(super) fn project(root: &Path) -> CurrentProject {
        let mut files: BTreeMap<String, String> = profile::catalog_source_paths()
            .iter()
            .map(|p| ((*p).into(), hash(p.as_bytes())))
            .collect();
        for (path, digest) in profile::reviewed_workflow_digests() {
            files.insert((*path).into(), (*digest).into());
        }
        for (path, bytes) in profile::definition_files()
            .into_iter()
            .chain(profile::carrier_files())
        {
            files.insert(path.into(), hash(bytes));
        }
        let sources = SourceInventory { files };
        let declaration = ProfileDeclaration {
            id: profile::REQUIRED_PROFILE.into(),
            reviewed_sources_digest: sources.digest().unwrap(),
        };
        let facts = ApplicabilityFacts {
            event: "pull_request".into(),
            target_branch: "master".into(),
            changed_paths: BTreeSet::from(["source".into()]),
        };
        let catalog =
            profile::required_source_catalog(Some(&declaration), &sources, &facts).unwrap();
        CurrentProject {
            configuration: ProofConfiguration {
                validation_buildkite_preflights: true,
                required_profile: Some(declaration),
                ..ProofConfiguration::default()
            },
            sources,
            executor: None,
            facts,
            release: None,
            catalog,
            identity_inputs: IdentityInputs {
                root: root.into(),
                git_program: PathBuf::from("/usr/bin/git").canonicalize().unwrap(),
                repository: "fixture/decapod".into(),
                base_branch: "master".into(),
                accepted_base_ref: "refs/remotes/origin/master".into(),
                declared_untracked: BTreeSet::new(),
                configuration: BTreeSet::from([CONFIG.into()]),
                workflows: BTreeSet::from(["source".into()]),
                candidate_inputs: BTreeSet::from(["source".into()]),
            },
        }
    }
    #[test]
    fn prepare_only_records_current_branch_and_private_handoff_without_new_commit_or_push() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".decapod")).unwrap();
        let external = temp.path().join("external");
        fs::create_dir(&external).unwrap();
        command(&root, &["init", "-b", "work"]);
        command(&root, &["config", "user.name", "Snapshot Test"]);
        command(&root, &["config", "user.email", "snapshot@example.invalid"]);
        fs::write(root.join(CONFIG), "[proof]\n").unwrap();
        fs::write(root.join(GOVERNANCE), "{}\n").unwrap();
        fs::write(root.join("source"), "base").unwrap();
        command(&root, &["add", "."]);
        command(&root, &["commit", "-m", "base"]);
        let base = command(&root, &["rev-parse", "HEAD"]);
        command(&root, &["update-ref", "refs/remotes/origin/master", &base]);
        fs::write(root.join("source"), "candidate").unwrap();
        command(&root, &["add", "source"]);
        command(&root, &["commit", "-m", "candidate"]);
        let head = command(&root, &["rev-parse", "HEAD"]);
        let index = fs::read(root.join(".git/index")).unwrap();
        let objects = command(&root, &["count-objects", "-v"]);
        let options = Options {
            root: root.canonicalize().unwrap(),
            git: PathBuf::from("/usr/bin/git").canonicalize().unwrap(),
            repository: "fixture/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: external,
            timeout: Duration::from_secs(30),
            max_input_bytes: 128 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
        };
        let runner = LocalOnly {
            official: OfficialCli::new(
                options.git.clone(),
                PathBuf::from("/usr/bin/true"),
                None,
                |_, _| Ok(()),
            )
            .unwrap(),
            base,
            remote_reads: 0,
        };
        let mut host = SnapshotHost::new(
            options.clone(),
            Repository::new("fixture", "decapod").unwrap(),
            runner,
            SnapshotMode::PrepareForHost,
            None,
            None,
            |_| Ok(ProjectPrerequisites::Passed),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(30),
            cancel: &cancel,
            max_output_bytes: 1024 * 1024,
            bytes_left: 128 * 1024 * 1024,
        };
        let blockers = host.stage(&project(&root), &budget).unwrap();
        assert_eq!(blockers[0].code, "SNAPSHOT_UPLOAD_PENDING");
        assert!(blockers.len() > 1);
        assert_eq!(head, command(&root, &["rev-parse", "HEAD"]));
        assert_eq!(index, fs::read(root.join(".git/index")).unwrap());
        assert_eq!(objects, command(&root, &["count-objects", "-v"]));
        assert!(!root.join(".git/refs/heads/decapod").exists());
        assert!(command(&root, &["status", "--porcelain"]).is_empty());
        let path = host
            .evidence_directory()
            .unwrap()
            .join("snapshot-handoff.json");
        let record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let snapshot = record["transport_head"].as_str().unwrap();
        assert_eq!(snapshot, head);
        assert_eq!(record["request"]["source_ref"], "refs/heads/work");
        assert_eq!(
            cleanup_handoff(&options, &host.repository, &mut host.runner, &path, &cancel).unwrap(),
            transport::Cleanup::RetainedWorkbranch
        );
        let mut changed = record.clone();
        changed["request"]["source_head"] = serde_json::Value::String("f".repeat(40));
        let bad = path.with_file_name("changed-handoff.json");
        write_new(&bad, &serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            cleanup_handoff(&options, &host.repository, &mut host.runner, &bad, &cancel).is_err()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        }
        check_adoption(&options, &host.repository, &path, &record);
    }
}

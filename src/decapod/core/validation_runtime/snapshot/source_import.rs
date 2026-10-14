//! Live provider inventory -> authenticated raw ZIP -> preparation policy ->
//! measured source execution. Caller files provide bytes only, never authority.
use super::*;
use provider::{github_artifacts as github, source_execution as custody};
#[derive(Default)]
pub(super) struct SourceCollection {
    pub checks: BTreeMap<CheckKey, VerifiedCheck>,
    pub blockers: Vec<Blocker>,
}
pub(super) struct VerifiedCheck {
    pub binding: ExecutionBinding,
    pub observation: evidence::Observation,
}
impl SourceCollection {
    fn failure(&mut self, key: &CheckKey, code: &str, error: impl ToString) {
        self.blockers.push(Blocker {
            code: code.into(),
            detail: error.to_string(),
            check: Some(key.clone()),
        });
    }
}

/// Derive transport discovery independently from the complete source catalog.
/// CandidateBuild is proven by its required actual setup-source check, never by
/// adding an invented provider workflow or dropping a source obligation.
pub(super) fn source_run_plan(project: &CurrentProject) -> io::Result<automatic::SourceRunPlan> {
    let mut buildkite = BTreeSet::new();
    for obligation in project.catalog.obligations() {
        if obligation.kind != evidence::CheckKind::SourceCi {
            continue;
        }
        match obligation.key.provider {
            evidence::Provider::Buildkite => {
                buildkite.insert(obligation.key.workflow.clone());
            }
            _ => return Err(invalid("unsupported provider in required source catalog")),
        }
    }
    if buildkite.is_empty() {
        return Err(invalid(
            "complete Buildkite source workflow catalog is required",
        ));
    }
    Ok(automatic::SourceRunPlan {
        github_workflows: Vec::new(),
        buildkite_pipelines: vec![automatic::ImportedPipeline {
            organization: "decapodlabs".into(),
            pipeline: "decapod".into(),
            pipeline_id: "01a0bcf1-5e16-44ae-8457-04495fae8ff5".into(),
            workflows: buildkite.into_iter().collect(),
        }],
    })
}

// Retained adapter for ordinary GitHub CI tooling; Buildkite-only preflight
// never calls it or accepts its results as required preflight proof.
#[allow(dead_code, clippy::too_many_arguments)]
pub(super) fn github_sources(
    options: &Options,
    runner: &mut impl Runner,
    snapshot: &transport::PreparedSnapshot,
    project: &CurrentProject,
    terminal: &automatic::TerminalSourceRuns,
    directory: Option<&Path>,
    evidence_directory: &Path,
    budget: &provider::Budget<'_>,
) -> io::Result<SourceCollection> {
    let mut collection = SourceCollection::default();
    let obligations: Vec<_> = project
        .catalog
        .obligations()
        .iter()
        .filter(|o| {
            o.kind == evidence::CheckKind::SourceCi
                && o.key.provider == evidence::Provider::GithubActions
        })
        .collect();
    let workflows: BTreeSet<_> = obligations
        .iter()
        .map(|o| o.key.workflow.as_str())
        .collect();
    let mut known_inputs = integration::Dependencies::default();
    for workflow in workflows {
        let Some(inventory) = github::discover_workflow(runner, snapshot, workflow, budget)? else {
            for obligation in obligations.iter().filter(|o| o.key.workflow == workflow) {
                collection.failure(
                    &obligation.key,
                    "GITHUB_WORKFLOW_PENDING",
                    format!("no exact snapshot run yet for {workflow}"),
                );
            }
            continue;
        };
        let pinned = terminal
            .github(workflow)
            .ok_or_else(|| invalid("required GitHub workflow absent from terminal source set"))?;
        if inventory.run_id() != pinned.run_id() || inventory.run_attempt() != pinned.run_attempt()
        {
            return Err(invalid("GitHub run/attempt changed after terminal barrier"));
        }
        if inventory.status() != "completed" || inventory.conclusion() != Some("success") {
            for obligation in obligations.iter().filter(|o| o.key.workflow == workflow) {
                collection.failure(
                    &obligation.key,
                    "GITHUB_WORKFLOW_NOT_SUCCESSFUL",
                    format!(
                        "run {} attempt {}: {}/{:?}",
                        inventory.run_id(),
                        inventory.run_attempt(),
                        inventory.status(),
                        inventory.conclusion()
                    ),
                );
            }
            continue;
        }
        let mut selected_artifacts = BTreeMap::<String, u64>::new();
        let mut jobs: BTreeMap<(String, Option<String>), Vec<&profile::Obligation>> =
            BTreeMap::new();
        for obligation in obligations.iter().filter(|o| o.key.workflow == workflow) {
            let check_id = obligation
                .key
                .profile
                .strip_prefix(&format!("{}/", profile::REQUIRED_PROFILE))
                .ok_or_else(|| invalid("source catalog profile mismatch"))?;
            match source::find_check(check_id) {
                Ok(check) => {
                    jobs.entry((check.job.into(), check.matrix.map(str::to_string)))
                        .or_default()
                        .push(obligation);
                }
                Err(error) => collection.failure(&obligation.key, "MISSING_SOURCE_EXECUTOR", error),
            }
        }
        // Authenticated shared-binary producers are consumed before dependent
        // health/governance jobs. Ordering never changes catalog obligations.
        let mut jobs: Vec<_> = jobs.into_iter().collect();
        jobs.sort_by_key(|((job, _), _)| match job.as_str() {
            "setup" => 0,
            "integration-prepare" => 1,
            "integration-status" => 3,
            _ => 2,
        });
        for ((logical, matrix), obligations) in jobs {
            let mut transport_failed = false;
            let imported = (|| -> io::Result<()> {
                let name = job_name(workflow, &logical, matrix.as_deref())?;
                let jobs: Vec<_> = inventory
                    .jobs()
                    .iter()
                    .filter(|j| j.name() == name)
                    .collect();
                if jobs.len() != 1 {
                    return Err(invalid("actual source job missing or ambiguous"));
                }
                let job = jobs[0];
                if job.status() != "completed" || job.conclusion() != Some("success") {
                    return Err(invalid(
                        "required actual source job did not complete successfully",
                    ));
                }
                let uploads: Vec<_> = job
                    .steps()
                    .iter()
                    .filter(|s| s.name() == source::custody::UPLOAD_STEP_NAME)
                    .collect();
                if uploads.len() != 1
                    || uploads[0].status() != "completed"
                    || uploads[0].conclusion() != Some("success")
                {
                    return Err(invalid(
                        "reviewed measured-custody upload step missing/skipped/failed",
                    ));
                }
                let artifact_name = source::custody::artifact_name(
                    &logical,
                    matrix.as_deref(),
                    &inventory.run_attempt().to_string(),
                )?;
                let selection = inventory
                    .artifact_selection(&artifact_name)?
                    .ok_or_else(|| {
                        invalid(format!("required uploaded archive absent: {artifact_name}"))
                    })?;
                let files = if let Some(directory) = directory {
                    // Explicit bootstrap transport supplies bytes only. IDs,
                    // digests and attempts are independently reauthenticated.
                    let path =
                        directory.join(format!("github-artifact-{}.zip", selection.artifact_id));
                    github::verify_handoff(runner, &options.root, &selection, &path, budget)
                } else {
                    // Normal validate owns the exact authenticated download;
                    // an authorization failure is terminal, never a fallback.
                    github::download(runner, &options.root, &selection, budget)
                };
                let files = files.map_err(|error| {
                    transport_failed = true;
                    io::Error::new(
                        error.kind(),
                        format!(
                        "GitHub artifact {} for run {} attempt {} could not be verified: {error}",
                        selection.artifact_id, selection.run_id, selection.run_attempt
                    ),
                    )
                })?;
                if selected_artifacts
                    .insert(artifact_name, selection.artifact_id)
                    .is_some()
                {
                    return Err(invalid("duplicate selected source artifact"));
                }
                let context = ProducerContext {
                    provider: "github",
                    run_id: &inventory.run_id().to_string(),
                    run_attempt: &inventory.run_attempt().to_string(),
                    native_job: None,
                    step_key: "",
                    imported_recipe: None,
                };
                for obligation in &obligations {
                    if let Err(error) = verify_check(
                        project,
                        snapshot,
                        &files,
                        obligation,
                        &context,
                        &mut known_inputs,
                        evidence_directory,
                        &mut collection,
                    ) {
                        collection.failure(
                            &obligation.key,
                            "SOURCE_EXECUTION_CUSTODY_BLOCKED",
                            error,
                        );
                    }
                }
                Ok(())
            })();
            if let Err(error) = imported {
                // Transport/authentication failures stop this invocation. Do not
                // retry another artifact or silently switch acquisition routes.
                if transport_failed {
                    return Err(error);
                }
                for obligation in &obligations {
                    collection.failure(&obligation.key, "SOURCE_EXECUTION_CUSTODY_BLOCKED", &error);
                }
            }
        }
        let current = github::discover_workflow(runner, snapshot, workflow, budget)?
            .ok_or_else(|| invalid("provider workflow disappeared during custody collection"))?;
        for (name, id) in selected_artifacts {
            if current
                .artifact_selection(&name)?
                .is_none_or(|selection| selection.artifact_id != id)
            {
                return Err(invalid(
                    "selected source artifact changed during collection",
                ));
            }
        }
        if current.run_id() != inventory.run_id()
            || current.run_attempt() != inventory.run_attempt()
            || current.status() != inventory.status()
            || current.conclusion() != inventory.conclusion()
            || current.jobs() != inventory.jobs()
        {
            return Err(invalid(
                "provider run/attempt/jobs changed during artifact verification",
            ));
        }
    }
    Ok(collection)
}

struct ProducerContext<'a> {
    provider: &'a str,
    run_id: &'a str,
    run_attempt: &'a str,
    native_job: Option<&'a str>,
    step_key: &'a str,
    imported_recipe: Option<&'a provider::public_buildkite::ImportedRecipe>,
}
#[allow(clippy::too_many_arguments)]
fn verify_check(
    project: &CurrentProject,
    snapshot: &transport::PreparedSnapshot,
    files: &provider::artifact_archive::ArtifactFiles,
    obligation: &profile::Obligation,
    context: &ProducerContext<'_>,
    known_inputs: &mut integration::Dependencies,
    evidence_directory: &Path,
    collection: &mut SourceCollection,
) -> io::Result<()> {
    let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
    let id = obligation
        .key
        .profile
        .strip_prefix(&format!("{}/", profile::REQUIRED_PROFILE))
        .ok_or_else(|| invalid("source profile mismatch"))?;
    let check = source::find_check(id)?;
    let expected = custody::SourceExpectation {
        workflow: check.workflow.into(),
        job: check.job.into(),
        check: check.id.into(),
        matrix: check.matrix.map(str::to_string),
        platform: check.platform.name().into(),
        base: snapshot.request().accepted_base.clone(),
        head: snapshot.request().source_head.clone(),
        snapshot: snapshot.transport_head().into(),
        execution_revision: snapshot.request().source_head.clone(),
        tree: snapshot.request().source_tree.clone(),
        provider: context.provider.into(),
        run_id: context.run_id.into(),
        run_attempt: context.run_attempt.into(),
        job_id: context
            .native_job
            .map(str::to_string)
            .unwrap_or_else(|| check.job.into()),
        step_key: context.step_key.into(),
        profile_digest: source::custody::profile_digest()?,
        executor: if source::workflow_owned::artifact_worker(check) {
            // The authenticated producer supplies the exact R binary/recipes;
            // workers do not checkout or recompile either source root.
            None
        } else {
            project
                .executor
                .as_ref()
                .map(|executor| custody::ExecutorExpectation {
                    revision: executor.revision().into(),
                    tree: executor.tree().into(),
                    profile_digest: executor.profile_digest().into(),
                })
        },
        invocations: source::custody::recipe_json(&source::commands(check, &paths)?).into_bytes(),
        artifact_source_paths: source::custody::required_artifact_paths(check, &paths)
            .into_iter()
            .collect(),
        execution_mode: if source::workflow_owned::artifact_worker(check) {
            "authenticated_integration_artifacts"
        } else {
            "source_checkout"
        }
        .into(),
    };
    // The expected binding is frozen from authenticated,
    // independently policy-checked preparation BEFORE accepting
    // substantive results. No green row chooses its own tools.
    let preparation = if let Some(recipe) = context.imported_recipe {
        custody::verify_imported_preparation(files, &expected, recipe, |measured| {
            source_policy::verify(project, check, &expected, measured, known_inputs)
        })?
    } else {
        custody::verify_preparation(files, &expected, |measured| {
            source_policy::verify(project, check, &expected, measured, known_inputs)
        })?
    };
    let binding = ExecutionBinding {
        role: obligation.role,
        definition_digest: obligation.definition_digest.clone(),
        invocation_digest: hash(&expected.invocations),
        toolchain_digest: preparation.toolchain_digest(),
        executor: obligation.key.provider,
        dependencies: source_dependencies(project, check, obligation.key.provider)?,
    };
    let execution = custody::verify_execution(files, &preparation)?;
    if execution.toolchain_digest() != binding.toolchain_digest
        || execution.invocation_digest() != binding.invocation_digest
    {
        return Err(invalid(
            "measured execution differs from frozen preparation",
        ));
    }
    for artifact in execution.artifacts() {
        // These exact renames are the reviewed source download
        // contract, not arbitrary receipt-requested aliases.
        let consumer = match (check.id, artifact.source_path.as_str()) {
            ("setup-bazel", "bazel-bin/decapod") => Some("target/debug/decapod"),
            ("setup-bazel", "bazel-bin/release_projection_drift") => {
                Some("target/debug/decapod-release-projection-drift")
            }
            _ => None,
        };
        if let Some(path) = consumer {
            known_inputs
                .inputs
                .insert(path.into(), artifact.digest.clone());
        }
    }
    integration::record(project, check, &expected, files, known_inputs)?;
    let observation = execution.observation();
    if check.id == "setup-bazel" && obligation.key.provider == evidence::Provider::Buildkite {
        let candidate = project
            .catalog
            .obligations()
            .iter()
            .find(|o| o.kind == evidence::CheckKind::CandidateBuild)
            .ok_or_else(|| invalid("candidate build obligation absent"))?;
        if candidate.key.provider != evidence::Provider::Buildkite
            || binding.executor != evidence::Provider::Buildkite
        {
            return Err(invalid(
                "candidate build must bind authenticated Buildkite setup",
            ));
        }
        let artifact = execution
            .artifacts()
            .iter()
            .find(|a| a.source_path == "bazel-bin/decapod" && a.bytes > 0)
            .ok_or_else(|| invalid("actual candidate executable absent from setup build"))?;
        let mut candidate_observation = observation.clone();
        if let evidence::Observation::Executed {
            artifact_digest, ..
        } = &mut candidate_observation
        {
            *artifact_digest = Some(artifact.digest.clone());
        } else {
            return Err(invalid(
                "candidate source build lacks actual command execution",
            ));
        }
        let mut candidate_binding = binding.clone();
        candidate_binding.role = candidate.role;
        candidate_binding.definition_digest = candidate.definition_digest.clone();
        candidate_binding.dependencies = vec![evidence::Dependency::Check(obligation.key.clone())];
        collection.checks.insert(
            candidate.key.clone(),
            VerifiedCheck {
                binding: candidate_binding,
                observation: candidate_observation,
            },
        );
    }
    let name = hash(&serde_json::to_vec(&obligation.key).map_err(io::Error::other)?);
    write_new(&evidence_directory.join(format!("source-{}.json",&name[7..])),&serde_json::to_vec_pretty(&serde_json::json!({"key":obligation.key,"binding":binding,"archive_origin":files.origin(),"archive_digest":files.archive_digest(),"preparation_digest":preparation.preparation_digest(),"execution_digest":execution.execution_digest(),"observation":observation})).map_err(io::Error::other)?)?;
    if collection
        .checks
        .insert(
            obligation.key.clone(),
            VerifiedCheck {
                binding,
                observation,
            },
        )
        .is_some()
    {
        return Err(invalid("duplicate authenticated source result"));
    }
    Ok(())
}

fn source_dependencies(
    project: &CurrentProject,
    check: &source::Check,
    provider: evidence::Provider,
) -> io::Result<Vec<evidence::Dependency>> {
    let jobs: &[&str] = match check.id {
        "health" | "governance-artifacts" => &["setup"],
        "integration-status" => &["integration-prepare", "test-integration"],
        _ if check.job == "test-integration" => &["integration-prepare"],
        _ => &[],
    };
    let dependencies: Vec<_> = project
        .catalog
        .obligations()
        .iter()
        .filter(|o| {
            o.kind == evidence::CheckKind::SourceCi
                && o.key.provider == provider
                && o.key.workflow == format!(".github/workflows/{}", check.workflow)
                && jobs.contains(&o.key.job.as_str())
        })
        .map(|o| evidence::Dependency::Check(o.key.clone()))
        .collect();
    if !jobs.is_empty() && dependencies.is_empty() {
        return Err(invalid(
            "required source producer dependency absent from catalog",
        ));
    }
    Ok(dependencies)
}

/// Names are finite reviewed workflow contracts; no expression/YAML interpreter
/// and no observed label is accepted as its own expected mapping.
fn job_name(workflow: &str, job: &str, matrix: Option<&str>) -> io::Result<String> {
    let simple = match (workflow, job) {
        (".github/workflows/ci.yml", "release-metadata") => "Release Metadata Classification",
        (".github/workflows/ci.yml", "commit-lint") => "Commit Lint",
        (".github/workflows/ci.yml", "governance-artifacts") => {
            "Governance Checkpoints (every authored commit)"
        }
        (".github/workflows/ci.yml", "material-specs") => {
            "Material Living Specs (beyond fingerprints)"
        }
        (".github/workflows/ci.yml", "detect-migration-script-changes") => {
            "Detect Migration Script Changes"
        }
        (".github/workflows/ci.yml", "supabase-consumer") => "Supabase consumer (offline fixtures)",
        (".github/workflows/ci.yml", "workspace-lifecycle-docker") => {
            "Workspace lifecycle (actual Docker)"
        }
        (".github/workflows/ci.yml", "fmt") => "Lint (fmt)",
        (".github/workflows/ci.yml", "clippy") => "Lint (clippy)",
        (".github/workflows/ci.yml", "deps-lint") => "Lint (dependencies)",
        (".github/workflows/ci.yml", "health") => "Decapod Validate",
        (".github/workflows/ci.yml", "setup") => "Setup & Build Shared Binary",
        (".github/workflows/ci.yml", "integration-prepare") => "Prepare Integration Tests",
        (".github/workflows/ci.yml", "integration-status") => "Tests (integration)",
        (".github/workflows/ci.yml", "test-rpc-suite") => "Tests (RPC Suite)",
        (".github/workflows/ci.yml", "contracts") => "Contract Conformance",
        (".github/workflows/ci.yml", "migration-script-tests") => "Migration Script Tests",
        (".github/workflows/constitution-sync.yml", "validate-constitution") => {
            "validate-constitution"
        }
        (".github/workflows/decapod-validate.yml", "validate") => "validate",
        (".github/workflows/docs.yml", "build") => "build",
        (".github/workflows/docs_sync.yml", "sync-docs") => "sync-docs",
        (".github/workflows/ci.yml", "musl-publication") => return match matrix {
            Some("x86_64-unknown-linux-musl") => {
                Ok("Musl build and atomic publication (x86_64-unknown-linux-musl, native)".into())
            }
            Some("aarch64-unknown-linux-musl") => Ok(
                "Musl build and atomic publication (aarch64-unknown-linux-musl, QEMU user-mode)"
                    .into(),
            ),
            _ => Err(invalid("unknown musl source matrix")),
        },
        (".github/workflows/ci.yml", "state-commit-golden-vectors") => {
            return matrix
                .filter(|m| ["ubuntu-latest", "macos-latest"].contains(m))
                .map(|m| format!("Golden Vectors ({m})"))
                .ok_or_else(|| invalid("unknown golden source matrix"));
        }
        (".github/workflows/ci.yml", "test") => {
            return matrix
                .filter(|m| ["core", "plugins", "cli"].contains(m))
                .map(|m| format!("Tests ({m})"))
                .ok_or_else(|| invalid("unknown test source matrix"));
        }
        (".github/workflows/ci.yml", "test-integration") => {
            return matrix
                .filter(|m| {
                    source::CHECKS
                        .iter()
                        .any(|c| c.job == job && c.matrix == Some(m))
                })
                .map(|m| format!("Tests (integration {m})"))
                .ok_or_else(|| invalid("unknown integration source matrix"));
        }
        _ => return Err(invalid("unreviewed provider job name mapping")),
    };
    if matrix.is_some() {
        return Err(invalid("unexpected provider source matrix"));
    }
    Ok(simple.into())
}

/// The metadata barrier discovers and authenticates build locators. Each full
/// envelope must retain the pinned UUID, repository, S/ref and source workflow.
#[allow(clippy::too_many_arguments)]
pub(super) fn buildkite_sources(
    reader: &mut impl provider::public_buildkite::PublicReader,
    contract: &ReviewedBuildkiteImporter,
    snapshot: &transport::PreparedSnapshot,
    project: &CurrentProject,
    terminal: &automatic::TerminalSourceRuns,
    evidence_directory: &Path,
    budget: &provider::Budget<'_>,
) -> io::Result<SourceCollection> {
    use provider::public_buildkite as bk;
    contract.validate()?;
    let mut cached_reader = bk::CollectionReader::new(reader);
    let reader = &mut cached_reader;
    let mut workflow_timings = Vec::new();
    let mut collection = SourceCollection::default();
    let obligations: Vec<_> = project
        .catalog
        .obligations()
        .iter()
        .filter(|o| {
            o.kind == evidence::CheckKind::SourceCi
                && o.key.provider == evidence::Provider::Buildkite
        })
        .collect();
    let workflows: BTreeSet<_> = obligations
        .iter()
        .map(|o| o.key.workflow.as_str())
        .collect();
    let paths: Vec<_> = project.facts.changed_paths.iter().cloned().collect();
    let source_jobs = source::inventory(&paths, false);
    for workflow in workflows {
        let workflow_started = Instant::now();
        eprintln!("Buildkite collection: stage=metadata_discovery workflow={workflow}");
        let pinned = terminal
            .buildkite("decapodlabs", "decapod", workflow)
            .ok_or_else(|| {
                invalid("required Buildkite workflow absent from terminal source set")
            })?;
        let expected_inventory = bk::InventoryExpectation {
            repository: snapshot.request().repository.clone(),
            pipeline_id: "01a0bcf1-5e16-44ae-8457-04495fae8ff5".into(),
            commit: snapshot.transport_head().into(),
            branch: snapshot
                .request()
                .transport_ref()
                .strip_prefix("refs/heads/")
                .unwrap()
                .into(),
            workflow_path: workflow.into(),
            event: "push".into(),
        };
        let inventory =
            bk::collect_inventory(reader, pinned.build().clone(), &expected_inventory, budget)?;
        if inventory.build_id() != pinned.build_id() {
            return Err(invalid("Buildkite build changed after terminal barrier"));
        }
        let metadata_ms = workflow_started.elapsed().as_millis();
        type JobKey = (String, String);
        let mut expected = BTreeMap::<
            JobKey,
            (
                bk::RecipeExpectation,
                Vec<&profile::Obligation>,
                Option<String>,
            ),
        >::new();
        for obligation in obligations.iter().filter(|o| o.key.workflow == workflow) {
            let id = obligation
                .key
                .profile
                .strip_prefix(&format!("{}/", profile::REQUIRED_PROFILE))
                .ok_or_else(|| invalid("source profile mismatch"))?;
            let check = source::find_check(id)?;
            let (index, total) = authored_matrix_position(&source_jobs, check)?;
            let matrix = obligation.key.matrix.clone();
            let key = (
                check.job.to_string(),
                serde_json::to_string(&matrix).map_err(io::Error::other)?,
            );
            let runtime_digest = contract.runtime_digest_for(&matrix)?.to_string();
            let row = expected.entry(key).or_insert_with(|| {
                (
                    bk::RecipeExpectation {
                        workflow_path: workflow.into(),
                        workflow_digest: project.sources.files[workflow].clone(),
                        logical_job: check.job.into(),
                        compiler_version: contract.compiler_version.clone(),
                        compiler_digest: contract.compiler_digest.clone(),
                        runtime_digest,
                        matrix: matrix
                            .iter()
                            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                            .collect(),
                        job_index: index as u64,
                        job_total: total as u64,
                    },
                    Vec::new(),
                    check.matrix.map(str::to_string),
                )
            });
            row.1.push(obligation);
        }
        let discovery_started = Instant::now();
        eprintln!("Buildkite collection: stage=recipe_discovery workflow={workflow}");
        let mut matched = BTreeMap::new();
        reader.locate_recipes(&inventory, budget, |locator| {
            // The reviewed global compiler identity is independent of measured
            // plans. Reject before the next batch, not after the full inventory.
            locator.verify_compiler(&contract.compiler_version, &contract.compiler_digest)?;
            let key = (
                locator.logical_job().to_string(),
                serde_json::to_string(locator.matrix()).map_err(io::Error::other)?,
            );
            if let Some((expected, _, _)) = expected.get(&key) {
                locator.verify_runtime(&expected.runtime_digest)?;
                if matched.insert(key, locator).is_some() {
                    return Err(invalid(
                        "multiple imported native jobs claim the same required source matrix",
                    ));
                }
            }
            Ok(())
        })?;
        let recipe_discovery_ms = discovery_started.elapsed().as_millis();
        let custody_started = Instant::now();
        eprintln!("Buildkite collection: stage=custody_verification workflow={workflow}");
        let mut expected: Vec<_> = expected.into_iter().collect();
        expected.sort_by_key(|((job, _), _)| match job.as_str() {
            "setup" => 0,
            "integration-prepare" => 1,
            "integration-status" => 3,
            _ => 2,
        });
        let mut known_inputs = integration::Dependencies::default();
        let mut recipe_validation_ms = 0;
        let mut archive_download_verify_ms = 0;
        let mut job_log_ms = 0;
        let mut source_custody_verify_ms = 0;
        let mut custody_transfer_wall_ms = 0;
        let mut archive_decode_ms = 0;
        let mut parallel_custody_batches = 0;
        let mut serial_custody_groups = 0;
        let expected_count = expected.len();
        let mut pending = std::collections::VecDeque::new();
        // Recipe and artifact selections need no source dependency outputs.
        // Prepare them first; source verification remains in the exact sorted
        // setup/integration/dependent/status order below.
        for (index, (key, (expected, obligations, matrix))) in expected.into_iter().enumerate() {
            let prepared = (|| -> io::Result<_> {
                let locator = matched
                    .remove(&key)
                    .ok_or_else(|| invalid("required actual imported source job absent"))?;
                let native_job = locator.producer_job_id().to_string();
                let started = Instant::now();
                let recipe =
                    bk::collect_located_recipe(reader, &inventory, locator, &expected, budget);
                recipe_validation_ms += started.elapsed().as_millis();
                let recipe = recipe?;
                if recipe.logical_result() != "success" {
                    return Err(invalid(format!(
                        "required logical source result is {}",
                        recipe.logical_result()
                    )));
                }
                let job = inventory
                    .jobs()
                    .get(&native_job)
                    .ok_or_else(|| invalid("native producer missing"))?;
                job.step_key()
                    .ok_or_else(|| invalid("native producer step key missing"))?;
                // Inventory rejects retried jobs/steps; supported native attempt0.
                let archive_name =
                    source::custody::artifact_name(&expected.logical_job, matrix.as_deref(), "0")?;
                let custody =
                    bk::prepare_custody(reader, &inventory, &recipe, &archive_name, budget)?;
                Ok((native_job, recipe, custody))
            })();
            match prepared {
                Ok((native_job, recipe, custody)) => {
                    pending.push_back((index, obligations, native_job, recipe, custody))
                }
                Err(error) => {
                    for obligation in &obligations {
                        collection.failure(
                            &obligation.key,
                            "BUILDKITE_SOURCE_CUSTODY_BLOCKED",
                            &error,
                        );
                    }
                }
            }
        }
        process_custody_batches(
            pending,
            |first, next| first.4.can_batch_with(&next.4),
            |batch| {
                if batch[0].4.parallel_bytes().is_none() {
                    serial_custody_groups += 1;
                } else {
                    parallel_custody_batches += 1;
                }
                let requests: Vec<_> = batch.iter().map(|job| &job.4).collect();
                let started = Instant::now();
                let transferred = bk::collect_custody(reader, &requests, budget);
                custody_transfer_wall_ms += started.elapsed().as_millis();
                // Returning the transport error stops the entire collection;
                // the helper does not verify this batch or dispatch a later one.
                transferred
            },
            |(index, obligations, native_job, recipe, _), custody| {
                let (archive_request_ms, log_request_ms) = custody.request_milliseconds();
                archive_download_verify_ms += archive_request_ms;
                job_log_ms += log_request_ms;
                let imported = (|| -> io::Result<()> {
                    let started = Instant::now();
                    let verified = custody.into_verified(budget);
                    let decode_ms = started.elapsed().as_millis();
                    archive_decode_ms += decode_ms;
                    archive_download_verify_ms += decode_ms;
                    let (files, log) = verified?;
                    write_new(
                        &evidence_directory.join(format!("buildkite-{native_job}.log")),
                        log.bytes(),
                    )?;
                    let job = inventory
                        .jobs()
                        .get(&native_job)
                        .ok_or_else(|| invalid("native producer missing"))?;
                    let step_key = job
                        .step_key()
                        .ok_or_else(|| invalid("native producer step key missing"))?;
                    let context = ProducerContext {
                        provider: "buildkite_gha",
                        run_id: inventory.build_id(),
                        run_attempt: "0",
                        native_job: Some(&native_job),
                        step_key,
                        imported_recipe: Some(&recipe),
                    };
                    let started = Instant::now();
                    for obligation in &obligations {
                        if let Err(error) = verify_check(
                            project,
                            snapshot,
                            &files,
                            obligation,
                            &context,
                            &mut known_inputs,
                            evidence_directory,
                            &mut collection,
                        ) {
                            collection.failure(
                                &obligation.key,
                                "SOURCE_EXECUTION_CUSTODY_BLOCKED",
                                error,
                            );
                        }
                    }
                    source_custody_verify_ms += started.elapsed().as_millis();
                    Ok(())
                })();
                if let Err(error) = imported {
                    for obligation in &obligations {
                        collection.failure(
                            &obligation.key,
                            "BUILDKITE_SOURCE_CUSTODY_BLOCKED",
                            &error,
                        );
                    }
                }
                eprintln!(
                    "Buildkite collection: stage=custody_verification workflow={workflow} completed={}/{} elapsed_ms={}",
                    index + 1,
                    expected_count,
                    custody_started.elapsed().as_millis(),
                );
                Ok(())
            },
        )?;
        let custody_verification_ms = custody_started.elapsed().as_millis();
        let recheck_started = Instant::now();
        bk::recheck_inventory(reader, &inventory, &expected_inventory, budget)?;
        workflow_timings.push(serde_json::json!({
            "workflow": workflow,
            "build_number": pinned.build().number(),
            "metadata_discovery_ms": metadata_ms,
            "recipe_discovery_ms": recipe_discovery_ms,
            "custody_verification_ms": custody_verification_ms,
            "recipe_validation_ms": recipe_validation_ms,
            "archive_download_verify_ms": archive_download_verify_ms,
            "job_log_ms": job_log_ms,
            "source_custody_verify_ms": source_custody_verify_ms,
            "custody_transfer_wall_ms": custody_transfer_wall_ms,
            "deferred_archive_decode_ms": archive_decode_ms,
            "parallel_custody_batches": parallel_custody_batches,
            "serial_custody_groups": serial_custody_groups,
            "custody_timing_scope": "archive/log durations sum individual work and may overlap; custody_transfer_wall_ms is batch wall time; serial oversized archives decode within transfer",
            "custody_parallel_allowance_sum_max_bytes": 128 * 1024 * 1024,
            "custody_max_parallel_reads": 4,
            "inventory_recheck_ms": recheck_started.elapsed().as_millis(),
            "workflow_wall_ms": workflow_started.elapsed().as_millis()
        }));
    }
    write_new(
        &evidence_directory.join("buildkite-collection-timing.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "decapod.buildkite-collection-timing.v2",
            "source_head": snapshot.transport_head(),
            "measurement_only": true,
            "transport": reader.measurements(),
            "workflows": workflow_timings
        }))
        .map_err(io::Error::other)?,
    )?;
    Ok(collection)
}
/// Own the whole source-import batch loop. A transfer error is deliberately
/// fatal to this collection: no member of that batch reaches verification, no
/// later batch starts, and no partially built SourceCollection is returned.
/// Previously imported groups may have diagnostic files, never full readiness.
fn process_custody_batches<T, U>(
    mut pending: std::collections::VecDeque<T>,
    mut compatible: impl FnMut(&T, &T) -> bool,
    mut transfer: impl FnMut(&[T]) -> io::Result<Vec<U>>,
    mut verify: impl FnMut(T, U) -> io::Result<()>,
) -> io::Result<()> {
    while let Some(first) = pending.pop_front() {
        let mut batch = vec![first];
        if pending
            .front()
            .is_some_and(|next| compatible(&batch[0], next))
        {
            batch.push(
                pending
                    .pop_front()
                    .ok_or_else(|| invalid("custody queue changed"))?,
            );
        }
        let transferred = transfer(&batch)?;
        if transferred.len() != batch.len() {
            return Err(invalid("custody group result count mismatch"));
        }
        for (job, custody) in batch.into_iter().zip(transferred) {
            verify(job, custody)?;
        }
    }
    Ok(())
}

fn authored_matrix_position(
    source_jobs: &[source::JobInstance],
    check: &source::Check,
) -> io::Result<(usize, usize)> {
    // The importer expands the whole authored static matrix before applying
    // row conditions. Applicability selects proof obligations, not row indices.
    let matrices: Vec<_> = source_jobs
        .iter()
        .filter(|job| job.workflow == check.workflow && job.job == check.job)
        .collect();
    let index = matrices
        .iter()
        .position(|job| job.matrix == check.matrix)
        .ok_or_else(|| invalid("source matrix missing from authored expansion"))?;
    Ok((index, matrices.len()))
}
#[cfg(test)]
fn read_buildkite_locators(path: &Path) -> io::Result<BTreeMap<String, u64>> {
    use serde::de::{MapAccess, Visitor};
    struct Locators(BTreeMap<String, u64>);
    impl<'de> Deserialize<'de> for Locators {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Unique;
            impl<'de> Visitor<'de> for Unique {
                type Value = Locators;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("unique workflow/build-number locators")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Locators, M::Error> {
                    let mut result = BTreeMap::new();
                    while let Some((key, value)) = map.next_entry::<String, u64>()? {
                        if result.len() >= 64
                            || value == 0
                            || !profile::reviewed_workflow_digests()
                                .iter()
                                .any(|(name, _)| *name == key)
                            || result.insert(key, value).is_some()
                        {
                            return Err(serde::de::Error::custom(
                                "invalid/duplicate/unreviewed workflow build locator",
                            ));
                        }
                    }
                    Ok(Locators(result))
                }
            }
            d.deserialize_map(Unique)
        }
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 65536 {
        return Err(invalid("build locator must be a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(invalid("build locator grew beyond bound"));
    }
    serde_json::from_slice::<Locators>(&bytes)
        .map(|v| v.0)
        .map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_group_maps_one_native_job_to_three_distinct_source_checks() {
        let jobs = source::inventory(&[], false);
        let grouped: Vec<_> = jobs
            .iter()
            .filter(|job| job.job == "test-rpc-suite")
            .collect();
        assert_eq!(grouped.len(), 1);
        assert_eq!(
            grouped[0].checks,
            ["rpc-suite", "chaos-replay", "daemonless-lifecycle"]
        );
        assert_eq!(
            job_name(".github/workflows/ci.yml", "test-rpc-suite", None).unwrap(),
            "Tests (RPC Suite)"
        );
        assert!(job_name(".github/workflows/ci.yml", "chaos-replay", None).is_err());
        assert!(job_name(".github/workflows/ci.yml", "daemonless-lifecycle", None).is_err());
        let mut invocations = BTreeSet::new();
        for id in &grouped[0].checks {
            let check = source::find_check(id).unwrap();
            assert_eq!(check.job, grouped[0].job);
            assert_eq!(check.matrix, None);
            assert!(invocations.insert(
                source::custody::invocation_digest(&source::commands(check, &[]).unwrap()).unwrap()
            ));
        }
        assert_eq!(invocations.len(), 3);
    }
    #[test]
    fn every_shared_check_has_reviewed_provider_job_name() {
        for check in source::CHECKS {
            let workflow = format!(".github/workflows/{}", check.workflow);
            assert!(
                job_name(&workflow, check.job, check.matrix).is_ok(),
                "{}",
                check.id
            );
        }
        assert!(job_name(".github/workflows/ci.yml", "fmt", Some("foreign")).is_err());
        assert!(job_name(".github/workflows/ci.yml", "new-green-job", None).is_err());
    }
    #[test]
    fn static_integration_matrix_keeps_inactive_row_indices() {
        let jobs = source::inventory(&["docs/example.md".into()], false);
        let integration: Vec<_> = jobs
            .iter()
            .filter(|job| job.job == "test-integration")
            .collect();
        assert_eq!(integration.len(), 9);
        assert!(integration.iter().any(|job| !job.applicable));
        for (index, job) in integration.iter().enumerate() {
            let check = source::CHECKS
                .iter()
                .find(|check| check.job == job.job && check.matrix == job.matrix)
                .unwrap();
            assert_eq!(authored_matrix_position(&jobs, check).unwrap(), (index, 9));
        }
    }
    #[test]
    fn build_number_handoff_is_only_unique_bounded_reviewed_locator() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("locators.json");
        fs::write(&path, br#"{".github/workflows/ci.yml":876}"#).unwrap();
        assert_eq!(
            read_buildkite_locators(&path).unwrap()[".github/workflows/ci.yml"],
            876
        );
        for invalid in [
            r#"{".github/workflows/ci.yml":876,".github/workflows/ci.yml":877}"#,
            r#"{".github/workflows/ci.yml":"876"}"#,
            r#"{".github/workflows/ci.yml":0}"#,
            r#"{".github/workflows/unknown.yml":876}"#,
            r#"{".github/workflows/ci.yml":876,"ready":true}"#,
        ] {
            fs::write(&path, invalid).unwrap();
            assert!(read_buildkite_locators(&path).is_err(), "{invalid}");
        }
        #[cfg(unix)]
        {
            let link = temp.path().join("link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(read_buildkite_locators(&link).is_err());
        }
    }
    #[test]
    fn importer_identity_is_reviewed_release_member_not_observed_row() {
        let contract = ReviewedBuildkiteImporter::reviewed();
        assert!(contract.validate().is_ok());
        let mut altered = contract.clone();
        altered.linux_runtime_digest = hash(b"some observed green row");
        assert!(altered.validate().is_err());
        altered = contract.clone();
        altered.compiler_version = "0.102.4".into();
        assert!(altered.validate().is_err());
        altered = contract.clone();
        altered.compiler_digest = hash(b"different compiler");
        assert!(altered.validate().is_err());
        altered = contract;
        altered.macos_runtime_digest = hash(b"different platform runtime");
        assert!(altered.validate().is_err());
    }
    #[test]
    fn reviewed_importer_rejects_old_mixed_and_unknown_release_identities() {
        let reviewed = ReviewedBuildkiteImporter::reviewed();
        let old_linux = "sha256:0803b5258df3929ab975bcb00b5e49eea028c910f6c59f84c03067ce19dc32fe";
        let old_macos = "sha256:274edf3e0086df695cf23ad1713adaa25a5ecdd1decafb16480e222e0e9aa9fa";
        let old = ReviewedBuildkiteImporter {
            compiler_version: "0.102.3".into(),
            compiler_digest: old_linux.into(),
            linux_runtime_digest: old_linux.into(),
            macos_runtime_digest: old_macos.into(),
        };
        assert!(old.validate().is_err());
        for version in ["0.102.3", "0.102.4", "0.102.6", "observed-green"] {
            let mut mixed = reviewed.clone();
            mixed.compiler_version = version.into();
            assert!(mixed.validate().is_err(), "{version}");
        }
        for field in 0..3 {
            let mut mixed = reviewed.clone();
            match field {
                0 => mixed.compiler_digest = old_linux.into(),
                1 => mixed.linux_runtime_digest = old_linux.into(),
                _ => mixed.macos_runtime_digest = old_macos.into(),
            }
            assert!(mixed.validate().is_err(), "old digest field {field}");
        }
        let mut swapped = reviewed.clone();
        std::mem::swap(
            &mut swapped.linux_runtime_digest,
            &mut swapped.macos_runtime_digest,
        );
        assert!(swapped.validate().is_err());
    }
    #[test]
    fn reviewed_01025_runtime_selection_uses_authored_platform() {
        let contract = ReviewedBuildkiteImporter::reviewed();
        assert_eq!(contract.compiler_version, "0.102.5");
        assert_eq!(
            contract.compiler_digest,
            "sha256:0e33c9189e80ec1e776a53f837f61b26e8f7f2b26671a6df2c0b4d23c09a4b02"
        );
        assert_eq!(
            contract.runtime_digest_for(&BTreeMap::new()).unwrap(),
            contract.compiler_digest
        );
        let mut matrix = BTreeMap::from([("os".into(), "macos-latest".into())]);
        assert_eq!(
            contract.runtime_digest_for(&matrix).unwrap(),
            "sha256:7af087fef3a258ed093c1812c18eb0832e44b48047cf3ab1b2adf0836fc2ad86"
        );
        matrix.insert("os".into(), "ubuntu-latest".into());
        assert_eq!(
            contract.runtime_digest_for(&matrix).unwrap(),
            contract.compiler_digest
        );
        matrix.insert("os".into(), "unreviewed-platform".into());
        assert!(contract.runtime_digest_for(&matrix).is_err());
    }
    #[test]
    fn source_collection_stops_on_denied_or_cancelled_batch_without_partial_proof() {
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::Interrupted] {
            for failing_batch in [0, 1] {
                let jobs = (0..6).collect();
                let mut dispatched = Vec::new();
                let mut verified = Vec::new();
                let error = process_custody_batches(
                    jobs,
                    |_, _| true,
                    |batch| {
                        let number = dispatched.len();
                        dispatched.push(batch.to_vec());
                        if number == failing_batch {
                            // Model a successful first read followed by denial or
                            // cancellation: no partial group result may escape.
                            let _completed_transport = batch[0];
                            return Err(io::Error::new(kind, "fixture transport failure"));
                        }
                        Ok(batch.to_vec())
                    },
                    |job, result| {
                        assert_eq!(job, result);
                        verified.push(job);
                        Ok(())
                    },
                )
                .unwrap_err();
                assert_eq!(error.kind(), kind);
                assert_eq!(
                    dispatched.len(),
                    failing_batch + 1,
                    "no later batch may start"
                );
                assert_eq!(
                    verified,
                    (0..failing_batch * 2).collect::<Vec<_>>(),
                    "no proof callback for any member of a failed batch"
                );
            }
        }
    }
    #[test]
    fn source_collection_rejects_partial_batch_before_any_proof_callback() {
        let mut dispatched = 0;
        let mut verified = 0;
        let error = process_custody_batches(
            (0..4).collect(),
            |_, _| true,
            |batch| {
                dispatched += 1;
                Ok(vec![batch[0]])
            },
            |_, _| {
                verified += 1;
                Ok(())
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("custody group result count mismatch")
        );
        assert_eq!(dispatched, 1);
        assert_eq!(verified, 0);
    }
    #[test]
    fn source_collection_verifies_setup_before_dependents_despite_batched_transport() {
        let jobs = [
            "setup",
            "integration-prepare",
            "worker",
            "integration-status",
        ];
        let mut verified = Vec::new();
        process_custody_batches(
            jobs.into_iter().collect(),
            |_, _| true,
            |batch| Ok(batch.to_vec()),
            |job, result| {
                assert_eq!(job, result);
                match job {
                    "integration-prepare" => assert_eq!(verified, ["setup"]),
                    "worker" => assert_eq!(verified, ["setup", "integration-prepare"]),
                    "integration-status" => {
                        assert_eq!(verified, ["setup", "integration-prepare", "worker"])
                    }
                    _ => (),
                }
                verified.push(job);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(verified, jobs);
    }
}

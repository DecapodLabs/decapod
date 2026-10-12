//! GitHub's complete derivative graph, authenticated ZIP members and frozen H
//! are the only inputs to this importer. An aggregate success JSON is not proof.
use super::source_import::{SourceCollection, VerifiedCheck};
use super::*;
use crate::core::derived_release_driver as driver;
use derived_release_profile::{FileMaterial, Snapshot, Tool, ToolResolution};
use provider::{artifact_archive::ArtifactFiles, github_artifacts as github};
use serde::{Deserialize, Serialize};

const WORKFLOW: &str = ".github/workflows/preflight-release.yml";
const HELPER: &str = ".github/scripts/preflight-release.py";
const WORKFLOW_SHA: &str = "28e60d4c8691b9d179fb1680cd9bcd45e3246ae42c2048bf651f64cf8579e686";
const HELPER_SHA: &str = "eba11211d59db93512c463f501d878ee18aa4578eeaaacae7a7df3b3a19a8743";
const UPLOAD: &str = "Upload authenticated derived-release artifact";
const SETUP_SOURCES: &[&str] = &[
    ".github/release-tools.toml",
    ".github/release.toml",
    "dist-workspace.toml",
    "dist.toml",
    ".github/workflows/release.yml",
    ".github/actions/setup-release-proof/action.yml",
    ".github/scripts/release-tool-versions.py",
    ".github/scripts/install-release-dist.sh",
];

/// Expected coverage comes only from CurrentProject, never from imported rows.
#[allow(clippy::too_many_arguments)]
pub(super) fn github_derived_releases(
    options: &Options,
    runner: &mut impl Runner,
    snapshot: &transport::PreparedSnapshot,
    project: &CurrentProject,
    directory: Option<&Path>,
    evidence_directory: &Path,
    runtime_budget: &RuntimeBudget<'_>,
    budget: &provider::Budget<'_>,
) -> io::Result<SourceCollection> {
    let obligations: Vec<_> = project
        .catalog
        .obligations()
        .iter()
        .filter(|o| o.kind == evidence::CheckKind::DerivedRelease)
        .collect();
    if obligations.is_empty() {
        return Ok(SourceCollection {
            checks: BTreeMap::new(),
            blockers: vec![Blocker::new(
                "DERIVED_RELEASE_CATALOG_MISSING",
                "required derivative release catalog is empty",
            )],
        });
    }
    let imported = collect(
        options,
        runner,
        snapshot,
        project,
        &obligations,
        directory,
        evidence_directory,
        runtime_budget,
        budget,
    );
    match imported {
        Ok(checks) => Ok(SourceCollection {
            checks,
            blockers: Vec::new(),
        }),
        Err(error) => Ok(SourceCollection {
            checks: BTreeMap::new(),
            blockers: obligations
                .iter()
                .map(|o| Blocker {
                    code: "DERIVED_RELEASE_CUSTODY_BLOCKED".into(),
                    detail: error.to_string(),
                    check: Some(o.key.clone()),
                })
                .collect(),
        }),
    }
}

#[allow(clippy::too_many_arguments)]
fn collect(
    options: &Options,
    runner: &mut impl Runner,
    snapshot: &transport::PreparedSnapshot,
    project: &CurrentProject,
    obligations: &[&profile::Obligation],
    directory: Option<&Path>,
    evidence_directory: &Path,
    runtime_budget: &RuntimeBudget<'_>,
    budget: &provider::Budget<'_>,
) -> io::Result<BTreeMap<CheckKey, VerifiedCheck>> {
    validate_catalog(project, obligations)?;
    let release = project.release.as_ref().ok_or_else(|| {
        invalid("predictive release profile is not enabled for core PR readiness")
    })?;
    // Read committed objects, not caller-supplied files or an editable checkout.
    let before = committed_snapshot(options, snapshot, runtime_budget)?;
    let read = |path: &str| -> io::Result<&str> {
        std::str::from_utf8(source(&before, path)?)
            .map_err(|_| invalid("release source is not UTF-8"))
    };
    let actual_profile = derived_release_profile::profile(
        read("Cargo.toml")?,
        read(".github/release.toml")?,
        read("dist-workspace.toml")?,
        read("dist.toml")?,
    )
    .map_err(invalid)?;
    if &actual_profile != release {
        return Err(invalid("committed H release profile changed"));
    }
    for (path, expected) in [(WORKFLOW, WORKFLOW_SHA), (HELPER, HELPER_SHA)] {
        if raw_hash(source(&before, path)?) != expected {
            return Err(invalid(format!(
                "reviewed derivative graph changed: {path}"
            )));
        }
    }
    let release_date = snapshot_date(options, snapshot, runtime_budget)?;
    let inventory = github::discover_workflow(runner, snapshot, WORKFLOW, budget)?
        .ok_or_else(|| invalid("exact snapshot derived-release workflow is pending"))?;
    if inventory.status() != "completed" || inventory.conclusion() != Some("success") {
        return Err(invalid(format!(
            "derived-release run {} attempt {} is {}/{:?}",
            inventory.run_id(),
            inventory.run_attempt(),
            inventory.status(),
            inventory.conclusion()
        )));
    }
    let attempt = inventory.run_attempt();
    let mut workers = vec![Worker::prepare(attempt), Worker::foundation(attempt)];
    for target in &release.targets {
        workers.push(Worker::local(target, &release.runners[target], attempt)?);
    }
    validate_inventory(&inventory, &workers)?;
    let pins: Pins =
        toml::from_str(read(".github/release-tools.toml")?).map_err(io::Error::other)?;
    let mut authenticated = BTreeMap::new();
    // Preparation is policy checked before any success proof is decoded.
    for worker in &workers {
        let files = archive(
            options,
            runner,
            &inventory,
            &worker.artifact,
            directory,
            budget,
        )?;
        let setup = verify_setup(
            &before,
            &pins,
            &files,
            snapshot,
            &inventory,
            worker,
            &release_date,
        )?;
        authenticated.insert(worker.artifact.clone(), (files, setup));
    }
    let (prepare_files, prepare_setup) =
        &authenticated[&format!("derived-release-prepare-{attempt}")];
    let expected_transport = driver::ExpectedTransport {
        source_commit: &snapshot.request().source_head,
        source_git_tree: &snapshot.request().source_tree,
        accepted_base: &snapshot.request().accepted_base,
        release_date: &release_date,
        setup_contract_sha256: &prepare_setup.contract_sha256,
    };
    let mut bindings = BTreeMap::new();
    for obligation in obligations
        .iter()
        .filter(|o| matches!(o.key.job.as_str(), "generate" | "verify-delta"))
    {
        bindings.insert(
            obligation.key.clone(),
            binding(
                obligation,
                snapshot,
                &release_date,
                release,
                prepare_setup,
                None,
            )?,
        );
    }
    let (transport, generated) =
        driver::verify_exported_transport(release, &before, &expected_transport, |name| {
            member(prepare_files, "prepare/transport", name)
        })?;
    verify_tool_measurements(&transport.generation_tools, prepare_setup)?;
    let cargo: toml::Value = toml::from_str(read("Cargo.toml")?).map_err(io::Error::other)?;
    let repository = cargo
        .get("package")
        .and_then(|p| p.get("repository"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| invalid("missing committed repository URL"))?;
    let mut expected_update = release
        .generation_command(&release_date)
        .map_err(invalid)?
        .args;
    expected_update.extend(["--repo-url".into(), repository.into()]);
    if !transport.preparation_records.iter().any(|r| {
        r.program_sha256 == prepare_setup.tools["release_plz"].binary_sha256
            && r.arguments == expected_update
            && r.exit_code == 0
            && r.output_policy == "raw"
    }) {
        return Err(invalid(
            "source-pinned generator binary/argv was not actually executed",
        ));
    }
    if transport.installer_definition_sha256 != prepare_setup.installer_definition_sha256
        || transport.semver_checks_version != prepare_setup.tools["cargo_semver_checks"].version
        || transport.semver_checks_sha256
            != prepare_setup.tools["cargo_semver_checks"].binary_sha256
    {
        return Err(invalid(
            "generation differs from independently frozen installer/semver setup",
        ));
    }
    verify_semver_log(&transport.preparation_records, prepare_setup, |name| {
        member(prepare_files, "prepare/transport", name)
    })?;
    let mut reports = Vec::new();
    let mut proof_outputs = BTreeMap::new();
    for worker in workers.iter().filter(|w| w.job != "prepare") {
        let (files, setup) = &authenticated[&worker.artifact];
        if setup.contract_sha256 != prepare_setup.contract_sha256 {
            return Err(invalid(
                "worker source/setup closure differs from preparation",
            ));
        }
        for (id, prefix, stage) in worker.stages() {
            let obligation = obligations
                .iter()
                .find(|o| o.key.job == id && o.key.matrix.get("target") == worker.target.as_ref())
                .ok_or_else(|| invalid("stage has no independently required catalog row"))?;
            // Freeze expected commands and measured preparation BEFORE the proof.
            bindings.insert(
                obligation.key.clone(),
                binding(
                    obligation,
                    snapshot,
                    &release_date,
                    release,
                    setup,
                    Some((&transport, &generated)),
                )?,
            );
            let proof = files.file(&format!("{prefix}/derived-release-proof.json"))?;
            let report =
                driver::verify_exported_stage(release, &transport, &generated, proof, |name| {
                    member(files, prefix, name)
                })?;
            if report.stage != stage {
                return Err(invalid("proof stage differs from provider producer"));
            }
            verify_tool_measurements(&report.worker_setup.tools, setup)?;
            if report.worker_setup.semver_checks_version
                != setup.tools["cargo_semver_checks"].version
                || report.worker_setup.semver_checks_sha256
                    != setup.tools["cargo_semver_checks"].binary_sha256
                || report.worker_setup.installer_definition_sha256
                    != setup.installer_definition_sha256
                || report.worker_setup.build_environment != setup.build_environment
            {
                return Err(invalid(
                    "stage setup differs from independently frozen measured preparation",
                ));
            }
            proof_outputs.insert(obligation.key.clone(), output_digest(files, proof)?);
            reports.push(report);
        }
    }
    let (foundation, foundation_setup) =
        &authenticated[&format!("derived-release-foundation-{attempt}")];
    let dist = derived_release_profile::verify_dist_plan(
        release,
        &generated,
        foundation.file("plan/evidence/plan-dist-manifest.json")?,
    )
    .map_err(invalid)?;
    let aggregate = obligations
        .iter()
        .find(|o| o.key.job == "verify-artifacts")
        .ok_or_else(|| invalid("missing complete-artifact obligation"))?;
    bindings.insert(
        aggregate.key.clone(),
        binding(
            aggregate,
            snapshot,
            &release_date,
            release,
            foundation_setup,
            Some((&transport, &generated)),
        )?,
    );
    let artifacts = driver::verify_complete_reports(
        release,
        &generated,
        &dist,
        &snapshot.request().source_head,
        &transport.binding.derived_commit,
        &reports,
    )?;
    proof_outputs.insert(
        aggregate.key.clone(),
        hash(&serde_json::to_vec(&artifacts)?),
    );
    for obligation in obligations
        .iter()
        .filter(|o| matches!(o.key.job.as_str(), "generate" | "verify-delta"))
    {
        proof_outputs.insert(
            obligation.key.clone(),
            output_digest(
                prepare_files,
                prepare_files.file("prepare/transport/transport.json")?,
            )?,
        );
    }
    // No rows are admitted if producer identity changed while bytes were decoded.
    let current = github::discover_workflow(runner, snapshot, WORKFLOW, budget)?
        .ok_or_else(|| invalid("derived-release workflow disappeared during import"))?;
    if current.run_id() != inventory.run_id()
        || current.run_attempt() != inventory.run_attempt()
        || current.workflow_id() != inventory.workflow_id()
        || current.status() != inventory.status()
        || current.conclusion() != inventory.conclusion()
        || current.jobs() != inventory.jobs()
        || current.artifact_names().collect::<BTreeSet<_>>()
            != inventory.artifact_names().collect::<BTreeSet<_>>()
    {
        return Err(invalid(
            "derived-release run/attempt/jobs changed during import",
        ));
    }
    let selected_names = workers
        .iter()
        .map(|w| w.artifact.clone())
        .chain([format!("derived-release-aggregate-{attempt}")]);
    for name in selected_names {
        let old = inventory
            .artifact_selection(&name)?
            .ok_or_else(|| invalid("original artifact disappeared"))?;
        let new = current
            .artifact_selection(&name)?
            .ok_or_else(|| invalid("current artifact disappeared"))?;
        if old.artifact_id != new.artifact_id
            || old.run_id != new.run_id
            || old.run_attempt != new.run_attempt
        {
            return Err(invalid(
                "derivative artifact identity changed during import",
            ));
        }
    }
    let mut checks = BTreeMap::new();
    for obligation in obligations {
        let binding = bindings
            .remove(&obligation.key)
            .ok_or_else(|| invalid("missing frozen binding"))?;
        let output_digest = proof_outputs
            .remove(&obligation.key)
            .ok_or_else(|| invalid("missing verified derivative output"))?;
        let observation = evidence::Observation::Executed {
            execution_id: format!(
                "github:{}/runs/{}/attempts/{}/derived-release/{}/{}",
                inventory.repository().full_name(),
                inventory.run_id(),
                inventory.run_attempt(),
                obligation.key.job,
                obligation
                    .key
                    .matrix
                    .get("target")
                    .map(String::as_str)
                    .unwrap_or("global")
            ),
            conclusion: evidence::Conclusion::Success,
            exit_code: Some(0),
            output_digest,
            artifact_digest: if obligation.key.job == "verify-artifacts" {
                Some(hash(&serde_json::to_vec(&artifacts)?))
            } else {
                None
            },
        };
        let name = hash(&serde_json::to_vec(&obligation.key)?);
        write_new(
            &evidence_directory.join(format!("derived-{}.json", &name[7..])),
            &serde_json::to_vec_pretty(&serde_json::json!({"key": obligation.key,
                "binding": binding, "observation": observation, "derived_commit": transport.binding.derived_commit,
                "generation_binding_sha256": generated.binding_sha256}))?,
        )?;
        checks.insert(
            obligation.key.clone(),
            VerifiedCheck {
                binding,
                observation,
            },
        );
    }
    Ok(checks)
}

fn validate_catalog(
    project: &CurrentProject,
    obligations: &[&profile::Obligation],
) -> io::Result<()> {
    let release = project.release.as_ref().ok_or_else(|| {
        invalid("predictive release profile is not enabled for core PR readiness")
    })?;
    let expected: BTreeSet<_> = release
        .required_checks()
        .into_iter()
        .map(|c| {
            let mut matrix = BTreeMap::new();
            if let Some(t) = c.target {
                matrix.insert("target".into(), t);
            }
            if let Some(r) = c.runner {
                matrix.insert("runner".into(), r);
            }
            CheckKey {
                provider: evidence::Provider::GithubActions,
                workflow: ".github/workflows/release.yml".into(),
                job: c.id.into(),
                matrix,
                profile: format!("{}/derived-release", profile::REQUIRED_PROFILE),
            }
        })
        .collect();
    let actual: BTreeSet<_> = obligations.iter().map(|o| o.key.clone()).collect();
    if actual != expected
        || obligations.len() != expected.len()
        || obligations
            .iter()
            .any(|o| o.role != InvocationRole::DerivedRelease)
    {
        return Err(invalid(
            "unknown, missing, duplicate or misrouted derived-release catalog row",
        ));
    }
    for target in &release.targets {
        let expected = if target.ends_with("apple-darwin") {
            "macos-14"
        } else {
            "ubuntu-24.04"
        };
        if release.runners.get(target).map(String::as_str) != Some(expected) {
            return Err(invalid("unreviewed derivative runner"));
        }
    }
    if release.runners.get("global").map(String::as_str) != Some("ubuntu-24.04") {
        return Err(invalid("unreviewed global derivative runner"));
    }
    Ok(())
}

struct Worker {
    job: String,
    display: String,
    runner: String,
    target: Option<String>,
    artifact: String,
}
impl Worker {
    fn prepare(attempt: u32) -> Self {
        Self {
            job: "prepare".into(),
            display: "prepare".into(),
            runner: "ubuntu-24.04".into(),
            target: None,
            artifact: format!("derived-release-prepare-{attempt}"),
        }
    }
    fn foundation(attempt: u32) -> Self {
        Self {
            job: "foundation".into(),
            display: "foundation".into(),
            runner: "ubuntu-24.04".into(),
            target: None,
            artifact: format!("derived-release-foundation-{attempt}"),
        }
    }
    fn local(target: &str, runner: &str, attempt: u32) -> io::Result<Self> {
        let job = match (target, runner) {
            ("x86_64-unknown-linux-gnu", "ubuntu-24.04") => "local_linux_x86_64",
            ("aarch64-unknown-linux-gnu", "ubuntu-24.04") => "local_linux_aarch64",
            ("x86_64-apple-darwin", "macos-14") => "local_macos_x86_64",
            ("aarch64-apple-darwin", "macos-14") => "local_macos_aarch64",
            _ => return Err(invalid("unsupported explicit derivative target/runner job")),
        };
        Ok(Self {
            job: job.into(),
            display: format!("local ({target}, {runner})"),
            runner: runner.into(),
            target: Some(target.into()),
            artifact: format!("derived-release-{target}-{attempt}"),
        })
    }
    fn stages(&self) -> Vec<(&'static str, &'static str, driver::Stage)> {
        match self.target.as_ref() {
            Some(target) => vec![(
                "dist-local",
                "local",
                driver::Stage::Local {
                    target: target.clone(),
                },
            )],
            None => vec![
                ("dist-plan", "plan", driver::Stage::Plan),
                ("cargo-package", "package", driver::Stage::Package),
                ("dist-global", "global", driver::Stage::Global),
                ("candidate-health", "candidate", driver::Stage::Candidate),
            ],
        }
    }
}
fn validate_inventory(
    inventory: &github::GithubWorkflowInventory,
    workers: &[Worker],
) -> io::Result<()> {
    let names: BTreeSet<_> = workers
        .iter()
        .map(|w| w.display.as_str())
        .chain(["aggregate"])
        .collect();
    let actual: BTreeSet<_> = inventory.jobs().iter().map(|j| j.name()).collect();
    if actual != names || actual.len() != inventory.jobs().len() {
        return Err(invalid(
            "unknown, missing or duplicate actual derivative job",
        ));
    }
    let artifacts: BTreeSet<_> = workers
        .iter()
        .map(|w| w.artifact.clone())
        .chain([format!(
            "derived-release-aggregate-{}",
            inventory.run_attempt()
        )])
        .collect();
    validate_artifact_names(
        inventory.run_attempt(),
        &artifacts,
        inventory.artifact_names(),
    )?;
    for job in inventory.jobs() {
        if job.status() != "completed" || job.conclusion() != Some("success") {
            return Err(invalid(
                "required derivative job skipped, pending or failed",
            ));
        }
        let required = match job.name() {
            "prepare" => "Generate and freeze one derivative",
            "foundation" => "Plan, verify package, build global/source archive and validate D",
            "aggregate" => "Verify raw byte custody and complete derived-release obligations",
            _ => "Import frozen derivative and build configured local artifact",
        };
        for name in [required, UPLOAD] {
            let steps: Vec<_> = job.steps().iter().filter(|s| s.name() == name).collect();
            if steps.len() != 1
                || steps[0].status() != "completed"
                || steps[0].conclusion() != Some("success")
            {
                return Err(invalid(format!(
                    "required measured derivative step absent/skipped/failed: {name}"
                )));
            }
        }
    }
    Ok(())
}
fn validate_artifact_names<'a>(
    attempt: u32,
    expected: &BTreeSet<String>,
    actual: impl Iterator<Item = &'a str>,
) -> io::Result<()> {
    let mut current = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for name in actual {
        if !seen.insert(name) {
            return Err(invalid("duplicate uploaded derivative artifact name"));
        }
        if expected.contains(name) {
            current.insert(name.to_owned());
            continue;
        }
        let (prefix, suffix) = name
            .rsplit_once('-')
            .ok_or_else(|| invalid("unknown derivative artifact"))?;
        let old: u32 = suffix
            .parse()
            .map_err(|_| invalid("unknown derivative artifact"))?;
        if old == 0
            || old >= attempt
            || suffix != old.to_string()
            || !expected.contains(&format!("{prefix}-{attempt}"))
        {
            return Err(invalid("unknown or future-attempt derivative artifact"));
        }
    }
    if current != *expected {
        return Err(invalid("missing current-attempt derivative artifact"));
    }
    Ok(())
}

fn archive(
    options: &Options,
    runner: &mut impl Runner,
    inventory: &github::GithubWorkflowInventory,
    name: &str,
    directory: Option<&Path>,
    budget: &provider::Budget<'_>,
) -> io::Result<ArtifactFiles> {
    let selection = inventory
        .artifact_selection(name)?
        .ok_or_else(|| invalid("missing derivative archive"))?;
    let directory = directory.ok_or_else(|| invalid(format!(
        "RAW_PROVIDER_ARTIFACT_REQUIRED: retrieve GitHub artifact {} from run {} attempt {} as github-artifact-{}.zip",
        selection.artifact_id, selection.run_id, selection.run_attempt, selection.artifact_id)))?;
    github::verify_handoff(
        runner,
        &options.root,
        &selection,
        &directory.join(format!("github-artifact-{}.zip", selection.artifact_id)),
        budget,
    )
}
fn source<'a>(before: &'a Snapshot, name: &str) -> io::Result<&'a [u8]> {
    let material = before
        .get(name)
        .ok_or_else(|| invalid(format!("missing committed release input: {name}")))?;
    if !matches!(material.mode, 0o100644 | 0o100755) {
        return Err(invalid("release input is not a regular blob"));
    }
    Ok(&material.bytes)
}
fn member(files: &ArtifactFiles, prefix: &str, name: &str) -> io::Result<Vec<u8>> {
    if !safe_relative(name) {
        return Err(invalid("unsafe release evidence member"));
    }
    Ok(files.file(&format!("{prefix}/{name}"))?.to_vec())
}
fn raw_hash(bytes: &[u8]) -> String {
    hash(bytes)[7..].into()
}
fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn output_digest(files: &ArtifactFiles, bytes: &[u8]) -> io::Result<String> {
    Ok(hash(&serde_json::to_vec(&(
        files.origin(),
        files.archive_digest(),
        raw_hash(bytes),
    ))?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pins {
    rust: String,
    release_plz: String,
    cargo_dist: String,
    release_plz_action: String,
    semver_checks_constraint: String,
    semver_checks_installer: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MeasuredTool {
    version: String,
    binary_sha256: String,
    argv: Vec<String>,
    stdout: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Setup {
    schema: String,
    contract_sha256: String,
    source_files: BTreeMap<String, String>,
    installer_definition_sha256: String,
    installer: String,
    runner: String,
    runner_os: String,
    runner_arch: String,
    tools: BTreeMap<String, MeasuredTool>,
    build_environment: BTreeMap<String, String>,
    authority: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Context {
    #[serde(rename = "SOURCE_HEAD")]
    source: String,
    #[serde(rename = "ACCEPTED_BASE")]
    base: String,
    #[serde(rename = "TRANSPORT_HEAD")]
    transport: String,
    #[serde(rename = "SOURCE_TREE")]
    tree: String,
    #[serde(rename = "SNAPSHOT_REQUEST")]
    request: String,
    #[serde(rename = "SOURCE_REF")]
    source_ref: String,
    release_date: String,
    run_id: String,
    run_attempt: String,
    job: String,
    runner: String,
    runner_os: String,
    runner_arch: String,
}
#[allow(clippy::too_many_arguments)]
fn verify_setup(
    before: &Snapshot,
    pins: &Pins,
    files: &ArtifactFiles,
    snapshot: &transport::PreparedSnapshot,
    inventory: &github::GithubWorkflowInventory,
    worker: &Worker,
    date: &str,
) -> io::Result<Setup> {
    let context: Context = parse(files.file("context.json")?)?;
    let setup: Setup = parse(files.file("setup.json")?)?;
    let request = snapshot.request();
    let (os, arch) = if worker.runner == "macos-14" {
        ("macOS", "ARM64")
    } else {
        ("Linux", "X64")
    };
    if context.source != request.source_head
        || context.base != request.accepted_base
        || context.transport != snapshot.transport_head()
        || context.tree != request.source_tree
        || context.request != request.request_id
        || context.source_ref != request.source_ref
        || context.release_date != date
        || context.run_id != inventory.run_id().to_string()
        || context.run_attempt != inventory.run_attempt().to_string()
        || context.job != worker.job
        || context.runner != worker.runner
        || context.runner_os != os
        || context.runner_arch != arch
        || setup.runner != context.runner
        || setup.runner_os != os
        || setup.runner_arch != arch
    {
        return Err(invalid(
            "derivative artifact producer/source/runner context mismatch",
        ));
    }
    let expected_sources: BTreeMap<_, _> = SETUP_SOURCES
        .iter()
        .map(|path| Ok(((*path).to_owned(), raw_hash(source(before, path)?))))
        .collect::<io::Result<_>>()?;
    let installer = files.file("setup-inputs/semver-checks-installer-action.yml")?;
    if installer.is_empty() || installer.len() > 1024 * 1024 {
        return Err(invalid("invalid actual installer definition"));
    }
    let installer_hash = raw_hash(installer);
    let expected_contract = raw_hash(&serde_json::to_vec(&serde_json::json!({
        "source_files": expected_sources, "installer_definition_sha256": installer_hash }))?);
    let reference = pins
        .semver_checks_installer
        .rsplit_once('@')
        .ok_or_else(|| invalid("installer must be immutable"))?;
    let action = std::str::from_utf8(source(
        before,
        ".github/actions/setup-release-proof/action.yml",
    )?)
    .map_err(io::Error::other)?;
    if reference.0 != "taiki-e/install-action"
        || reference.1.len() != 40
        || !reference.1.bytes().all(|b| b.is_ascii_hexdigit())
        || !action
            .lines()
            .any(|line| line.trim() == format!("- uses: {}", pins.semver_checks_installer))
        || !pins.release_plz_action.starts_with("release-plz/action@")
        || setup.schema != "derived-release-shared-setup/1"
        || setup.source_files != expected_sources
        || setup.contract_sha256 != expected_contract
        || setup.installer_definition_sha256 != installer_hash
        || setup.installer != pins.semver_checks_installer
        || setup.authority
            != "same reviewed production setup; measured here, no historical production binary claim"
    {
        return Err(invalid(
            "shared production installer/source contract mismatch",
        ));
    }
    let expected_tools = [
        "git",
        "cargo",
        "rustc",
        "release_plz",
        "cargo_dist",
        "cargo_semver_checks",
    ];
    if setup
        .tools
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_tools.into_iter().collect()
    {
        return Err(invalid("missing or unknown measured release tool"));
    }
    for (key, tool) in &setup.tools {
        let suffix = if key == "cargo_dist" {
            vec!["dist", "--version"]
        } else {
            vec!["--version"]
        };
        if !is_hash(&tool.binary_sha256)
            || tool.argv.len() != suffix.len() + 1
            || !Path::new(&tool.argv[0]).is_absolute()
            || tool.argv[1..]
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != suffix
            || tool
                .stdout
                .split_whitespace()
                .nth(if key == "git" { 2 } else { 1 })
                != Some(tool.version.as_str())
        {
            return Err(invalid("unmeasured or incomplete release tool preparation"));
        }
    }
    for (key, expected) in [
        ("cargo", &pins.rust),
        ("rustc", &pins.rust),
        ("release_plz", &pins.release_plz),
        ("cargo_dist", &pins.cargo_dist),
    ] {
        if setup.tools[key].version != *expected {
            return Err(invalid("measured tool differs from committed pin"));
        }
    }
    let semver = &setup.tools["cargo_semver_checks"].version;
    if !semver
        .strip_prefix(&format!("{}.", pins.semver_checks_constraint))
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid(
            "measured semver-checks differs from committed constraint",
        ));
    }
    let expected_env: BTreeSet<&str> = if os == "macOS" {
        ["SDKROOT", "DEVELOPER_DIR"].into_iter().collect()
    } else if worker.target.as_deref() == Some("aarch64-unknown-linux-gnu") {
        ["CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER"]
            .into_iter()
            .collect()
    } else {
        BTreeSet::new()
    };
    if setup
        .build_environment
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_env
        || setup
            .build_environment
            .values()
            .any(|v| !Path::new(v).is_absolute())
    {
        return Err(invalid("unreviewed release compiler/SDK environment"));
    }
    Ok(setup)
}
fn verify_tool_measurements(
    tools: &BTreeMap<Tool, ToolResolution>,
    setup: &Setup,
) -> io::Result<()> {
    for (key, name) in [
        (Tool::Git, "git"),
        (Tool::Cargo, "cargo"),
        (Tool::Rustc, "rustc"),
        (Tool::ReleasePlz, "release_plz"),
        (Tool::CargoDist, "cargo_dist"),
    ] {
        let actual = tools
            .get(&key)
            .ok_or_else(|| invalid("missing executed tool measurement"))?;
        let frozen = &setup.tools[name];
        if actual.actual_version != frozen.version
            || actual.executable_sha256 != frozen.binary_sha256
            || actual.production_resolution_sha256 != setup.contract_sha256
            || actual.production_versions
                != BTreeMap::from([(
                    format!("shared-reviewed-setup:{}", setup.runner),
                    frozen.version.clone(),
                )])
        {
            return Err(invalid(
                "executed tool differs from independent preparation",
            ));
        }
    }
    if tools.len() != 5 {
        return Err(invalid("unknown executed tool measurement"));
    }
    Ok(())
}
fn verify_semver_log(
    records: &[driver::CommandRecord],
    setup: &Setup,
    mut get: impl FnMut(&str) -> io::Result<Vec<u8>>,
) -> io::Result<()> {
    let tool = &setup.tools["cargo_semver_checks"];
    let row = records
        .iter()
        .find(|r| {
            r.program_sha256 == tool.binary_sha256
                && r.arguments == ["--version"]
                && r.exit_code == 0
                && r.output_policy == "raw"
        })
        .ok_or_else(|| invalid("semver-checks actual version log missing"))?;
    let raw = get(&format!("evidence/logs/{}.stdout", row.stdout_sha256))?;
    if raw_hash(&raw) != row.stdout_sha256
        || std::str::from_utf8(&raw)
            .map_err(io::Error::other)?
            .split_whitespace()
            .nth(1)
            != Some(tool.version.as_str())
    {
        return Err(invalid(
            "actual semver-checks version differs from preparation",
        ));
    }
    Ok(())
}
fn binding(
    obligation: &profile::Obligation,
    snapshot: &transport::PreparedSnapshot,
    date: &str,
    release: &ReleaseProfile,
    setup: &Setup,
    derived: Option<(
        &driver::ReleaseTransport,
        &derived_release_profile::GeneratedRelease,
    )>,
) -> io::Result<ExecutionBinding> {
    let stage_commands = derived
        .map(|(_, generated)| {
            release
                .verification_commands(&generated.version, &release.targets)
                .map_err(invalid)
        })
        .transpose()?;
    let expected = serde_json::json!({"schema":"derived-release-import-binding/1", "key": obligation.key,
        "source": snapshot.request().source_head, "tree": snapshot.request().source_tree,
        "base": snapshot.request().accepted_base, "snapshot": snapshot.transport_head(), "release_date":date,
        "configuration": release.configuration_digest, "setup_contract":setup.contract_sha256,
        "generation_command": release.generation_command(date).map_err(invalid)?,
        "source_bound_stage_commands": stage_commands,
        "derived": derived.map(|(t,g)| (&t.binding.derived_commit, &g.binding_sha256, &g.tag)),
        "adapter_workflow_sha256":WORKFLOW_SHA, "adapter_helper_sha256":HELPER_SHA});
    Ok(ExecutionBinding {
        role: InvocationRole::DerivedRelease,
        definition_digest: obligation.definition_digest.clone(),
        invocation_digest: hash(&serde_json::to_vec(&expected)?),
        toolchain_digest: hash(&serde_json::to_vec(setup)?),
        executor: evidence::Provider::GithubActions,
        dependencies: Vec::new(),
    })
}

fn committed_snapshot(
    options: &Options,
    snapshot: &transport::PreparedSnapshot,
    budget: &RuntimeBudget<'_>,
) -> io::Result<Snapshot> {
    let head = &snapshot.request().source_head;
    if text(git(
        options,
        budget,
        &["rev-parse", &format!("{head}^{{tree}}")],
    )?)? != snapshot.request().source_tree
    {
        return Err(invalid("committed source tree changed"));
    }
    let listing = git(options, budget, &["ls-tree", "-rz", "--full-tree", head])?;
    if listing.is_empty() || listing.last() != Some(&0) {
        return Err(invalid("incomplete committed source inventory"));
    }
    let mut result = Snapshot::new();
    let mut total = 0usize;
    for entry in listing.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        budget.remaining()?;
        let entry = std::str::from_utf8(entry).map_err(io::Error::other)?;
        let (fields, path) = entry
            .split_once('\t')
            .ok_or_else(|| invalid("invalid committed tree entry"))?;
        let fields: Vec<_> = fields.split(' ').collect();
        if fields.len() != 3
            || fields[1] != "blob"
            || !safe_relative(path)
            || result.len() >= options.max_files.min(100_000)
        {
            return Err(invalid("unsupported committed tree member"));
        }
        let mode = u32::from_str_radix(fields[0], 8).map_err(io::Error::other)?;
        if !matches!(mode, 0o100644 | 0o100755 | 0o120000) {
            return Err(invalid("unsupported committed mode"));
        }
        let bytes = git(options, budget, &["cat-file", "blob", fields[2]])?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| invalid("source material overflow"))?;
        if total as u64 > options.max_input_bytes.min(512 * 1024 * 1024)
            || result
                .insert(path.into(), FileMaterial { mode, bytes })
                .is_some()
        {
            return Err(invalid("source snapshot oversized or duplicated"));
        }
    }
    Ok(result)
}
fn snapshot_date(
    options: &Options,
    snapshot: &transport::PreparedSnapshot,
    budget: &RuntimeBudget<'_>,
) -> io::Result<String> {
    let seconds: i64 = text(git(
        options,
        budget,
        &["show", "-s", "--format=%ct", snapshot.transport_head()],
    )?)?
    .parse()
    .map_err(io::Error::other)?;
    utc_date(seconds)
}
fn utc_date(seconds: i64) -> io::Result<String> {
    // Gregorian civil-from-days, independent of process/commit timezone.
    if !(0..=253_402_300_799).contains(&seconds) {
        return Err(invalid("snapshot timestamp outside reviewed date range"));
    }
    let z = seconds / 86_400 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}

// Reject duplicate object keys before decoding nested maps or typed structures.
fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> io::Result<T> {
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("oversized preparation JSON"));
    }
    let unique: Unique = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    serde_json::from_value(unique.0).map_err(io::Error::other)
}
struct Unique(serde_json::Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(n.into()))
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::Null))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = Vec::new();
                while let Some(Unique(x)) = a.next_element()? {
                    v.push(x);
                }
                Ok(Unique(v.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut v = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if v.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    let Unique(x) = a.next_value()?;
                    v.insert(k, x);
                }
                Ok(Unique(v.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_are_utc_and_bounded() {
        assert_eq!(utc_date(0).unwrap(), "1970-01-01");
        assert_eq!(utc_date(951_782_400).unwrap(), "2000-02-29");
        assert_eq!(utc_date(253_402_300_799).unwrap(), "9999-12-31");
        assert!(utc_date(-1).is_err());
    }
    #[test]
    fn duplicate_nested_preparation_keys_are_rejected() {
        assert!(parse::<serde_json::Value>(br#"{"tools":{"cargo":1,"cargo":2}}"#).is_err());
        assert!(parse::<serde_json::Value>(br#"{"tools":{"cargo":1},"tools":{}}"#).is_err());
        assert!(parse::<serde_json::Value>(br#"{"tools":{"cargo":1}}"#).is_ok());
    }
    fn run_git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("/usr/bin/git")
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    #[test]
    fn committed_source_ignores_worktree_changes_and_preserves_modes() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        run_git(root, &["init", "-b", "work"]);
        run_git(root, &["config", "user.email", "test@example.invalid"]);
        run_git(root, &["config", "user.name", "Test"]);
        fs::write(root.join("ordinary"), b"committed\0bytes").unwrap();
        fs::write(root.join("executable"), b"#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(root.join("executable"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("ordinary", root.join("link")).unwrap();
        run_git(root, &["add", "."]);
        run_git(root, &["commit", "-m", "fixture"]);
        let head = run_git(root, &["rev-parse", "HEAD"]);
        let tree = run_git(root, &["rev-parse", "HEAD^{tree}"]);
        let snapshot = transport::PreparedSnapshot::fixture(
            transport::SnapshotRequest {
                repository: Repository::new("fixture", "decapod").unwrap(),
                request_id: "12345678-1234-4234-9234-123456789abc".into(),
                source_ref: "refs/heads/work".into(),
                source_head: head.clone(),
                source_tree: tree,
                accepted_base: "a".repeat(40),
                accepted_base_ref: "refs/heads/master".into(),
                tags: BTreeMap::new(),
            },
            head,
            root.into(),
        );
        let options = Options {
            root: root.into(),
            git: PathBuf::from("/usr/bin/git"),
            repository: "fixture/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: root.into(),
            timeout: Duration::from_secs(10),
            max_input_bytes: 1024 * 1024,
            max_files: 10,
            max_output_bytes: 1024 * 1024,
        };
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(10),
            cancel: &cancel,
            max_output_bytes: 1024 * 1024,
            bytes_left: 1024 * 1024,
        };
        fs::write(root.join("ordinary"), "caller replacement").unwrap();
        fs::write(root.join("untracked"), "must not enter H").unwrap();
        let actual = committed_snapshot(&options, &snapshot, &budget).unwrap();
        assert_eq!(actual.len(), 3);
        assert_eq!(actual["ordinary"].bytes, b"committed\0bytes");
        assert_eq!(actual["ordinary"].mode, 0o100644);
        assert_eq!(actual["executable"].mode, 0o100755);
        assert_eq!(actual["link"].mode, 0o120000);
        assert_eq!(actual["link"].bytes, b"ordinary");
        let mut limited = options.clone();
        limited.max_files = 2;
        assert!(committed_snapshot(&limited, &snapshot, &budget).is_err());
        limited.max_files = 10;
        limited.max_input_bytes = 1;
        assert!(committed_snapshot(&limited, &snapshot, &budget).is_err());
    }
    #[test]
    fn finite_workers_do_not_merge_platform_rows() {
        let a = Worker::local("aarch64-apple-darwin", "macos-14", 1).unwrap();
        let b = Worker::local("x86_64-apple-darwin", "macos-14", 1).unwrap();
        assert_eq!(a.job, "local_macos_aarch64");
        assert_eq!(b.job, "local_macos_x86_64");
        assert!(Worker::local("x86_64-apple-darwin", "ubuntu-24.04", 1).is_err());
        assert!(Worker::local("foreign-target", "macos-14", 1).is_err());
        assert_ne!(a.display, b.display);
        assert_ne!(a.artifact, b.artifact);
        assert_ne!(a.stages()[0].2, b.stages()[0].2);
        assert_eq!(Worker::foundation(1).stages().len(), 4);
    }
    #[test]
    fn setup_structs_reject_unknown_fields_and_bad_hashes() {
        assert!(
            parse::<MeasuredTool>(
                br#"{"version":"1","binary_sha256":"a","argv":[],"stdout":"","success":true}"#
            )
            .is_err()
        );
        assert!(!is_hash("sha256:0123"));
        assert!(!is_hash(&"A".repeat(64)));
        assert!(is_hash(&"a".repeat(64)));
    }
    #[test]
    fn older_artifacts_cannot_supply_current_attempt_coverage() {
        let expected = BTreeSet::from([
            "derived-release-prepare-2".to_owned(),
            "derived-release-foundation-2".to_owned(),
        ]);
        assert!(
            validate_artifact_names(
                2,
                &expected,
                [
                    "derived-release-prepare-2",
                    "derived-release-foundation-2",
                    "derived-release-prepare-1"
                ]
                .into_iter()
            )
            .is_ok()
        );
        for rows in [
            vec!["derived-release-prepare-1", "derived-release-foundation-2"],
            vec![
                "derived-release-prepare-2",
                "derived-release-foundation-2",
                "unknown-1",
            ],
            vec![
                "derived-release-prepare-2",
                "derived-release-foundation-2",
                "derived-release-prepare-3",
            ],
            vec![
                "derived-release-prepare-2",
                "derived-release-foundation-2",
                "derived-release-prepare-02",
            ],
            vec![
                "derived-release-prepare-2",
                "derived-release-foundation-2",
                "derived-release-prepare-2",
            ],
        ] {
            assert!(validate_artifact_names(2, &expected, rows.into_iter()).is_err());
        }
    }
}

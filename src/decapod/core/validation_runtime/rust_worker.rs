//! Supplemental Rust-worker checks after complete strict source proof.
//! This reuses the existing immutable native runner. It is not a second CI
//! profile, an evidence importer, a model oracle or a publication exception.
use super::super::native_proof::{ExecutionPurpose, NativePlatform, ReviewedCapabilities, Value};
use super::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub arguments: Vec<String>,
    /// Request the worker's existing --proof-output protocol. The controller
    /// supplies a fresh private destination and retains the bounded JSON bytes.
    #[serde(default)]
    pub proof_output: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Worker {
    pub name: String,
    /// A separately locked Cargo workspace inside the committed project.
    pub manifest: String,
    #[serde(default)]
    pub features: Vec<String>,
    /// On macOS, measure selected Metal tools and SDK metadata. Compiler custody
    /// does not establish that GPU dispatch or model inference happened.
    #[serde(default)]
    pub metal_toolchain: bool,
    pub runs: Vec<Run>,
    pub timeout_seconds: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub reviewed_recipes_digest: String,
    pub workers: Vec<Worker>,
}
pub fn recipe_digest(workers: &[Worker]) -> io::Result<String> {
    evidence::digest(&("decapod.rust-worker-recipe.v1", workers)).map_err(io::Error::other)
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn manifest_path(value: &str) -> bool {
    let path = Path::new(value);
    value.len() <= 512
        && !value.contains(['\\', '\0'])
        && value
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && path.components().count() >= 2
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path.file_name().is_some_and(|part| part == "Cargo.toml")
}
pub fn validate(configuration: &Configuration) -> io::Result<()> {
    if configuration.workers.is_empty()
        || configuration.workers.len() > 4
        || recipe_digest(&configuration.workers)? != configuration.reviewed_recipes_digest
    {
        return Err(invalid(
            "RUST_WORKER_CONTRACT_INVALID: review the bounded nonempty recipe before pinning its digest",
        ));
    }
    let mut names = BTreeSet::new();
    let mut variants = BTreeSet::new();
    for worker in &configuration.workers {
        let features = worker.features.iter().collect::<BTreeSet<_>>();
        if !name(&worker.name)
            || !names.insert(&worker.name)
            || !manifest_path(&worker.manifest)
            || !variants.insert((&worker.manifest, features.clone()))
            || worker.features.len() > 16
            || features.len() != worker.features.len()
            || worker.features.iter().any(|f| !name(f))
            || worker.runs.is_empty()
            || worker.runs.len() > 8
            || !(1..=1800).contains(&worker.timeout_seconds)
            || worker.runs.iter().any(|run| {
                run.arguments.len() > 64
                    || run.arguments.iter().any(|arg| {
                        arg.len() > 4096
                            || arg.contains('\0')
                            || arg == "--proof-output"
                            || arg.starts_with("--proof-output=")
                    })
                    || run.arguments.iter().map(String::len).sum::<usize>() > 16 * 1024
            })
        {
            return Err(invalid(
                "RUST_WORKER_CONTRACT_INVALID: duplicate, malformed or oversized recipe",
            ));
        }
    }
    Ok(())
}
/// Existing accepted obligations cannot be removed or reinterpreted by the
/// candidate. Additional workers only add required proof; they do not waive CI.
pub fn preserve_accepted(
    candidate: Option<&Configuration>,
    accepted: Option<&Configuration>,
) -> io::Result<()> {
    if let Some(candidate) = candidate {
        validate(candidate)?;
    }
    if let Some(accepted) = accepted {
        validate(accepted)?;
        let candidate = candidate.ok_or_else(|| {
            invalid("RUST_WORKER_POLICY_DOWNGRADE: accepted recipes cannot be omitted")
        })?;
        for worker in &accepted.workers {
            if !candidate.workers.contains(worker) {
                return Err(invalid(
                    "RUST_WORKER_POLICY_DOWNGRADE: accepted recipes must remain unchanged",
                ));
            }
        }
    }
    Ok(())
}

fn source_matches(
    expected: &evidence::MaterialIdentity,
    observed: &evidence::MaterialIdentity,
) -> bool {
    // Primary provider and supplemental native closures are measured separately.
    let mut expected = expected.clone();
    expected.toolchain_digest = observed.toolchain_digest.clone();
    expected == *observed
}
fn base_arguments(worker: &Worker, operation: &str) -> Vec<Value> {
    let mut args = vec![
        operation,
        "--release",
        "--locked",
        "--manifest-path",
        &worker.manifest,
    ]
    .into_iter()
    .map(|s| Value::Literal(s.to_owned()))
    .collect::<Vec<_>>();
    if !worker.features.is_empty() {
        args.extend([
            Value::Literal("--features".into()),
            Value::Literal(worker.features.join(",")),
        ]);
    }
    args
}
fn manifest_version(
    root: &Path,
    worker: &Worker,
    budget: &mut RuntimeBudget<'_>,
) -> io::Result<String> {
    let manifest = file(root, &worker.manifest)?;
    let raw = budget.read(&manifest)?;
    if raw.len() > 64 * 1024 {
        return Err(invalid("RUST_WORKER_MANIFEST_TOO_LARGE"));
    }
    let value: toml::Value = toml::from_str(std::str::from_utf8(&raw).map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    if value
        .get("workspace")
        .and_then(toml::Value::as_table)
        .is_none()
    {
        return Err(invalid(
            "RUST_WORKER_WORKSPACE_REQUIRED: use a separately locked workspace",
        ));
    }
    let version = value
        .get("package")
        .and_then(|p| p.get("rust-version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| invalid("RUST_WORKER_TOOLCHAIN_REQUIRED: explicit package.rust-version"))?;
    let lock = Path::new(&worker.manifest)
        .parent()
        .unwrap()
        .join("Cargo.lock");
    let lock = lock
        .to_str()
        .ok_or_else(|| invalid("non-UTF-8 worker lock path"))?;
    let lock_bytes = budget.read(&file(root, lock)?)?;
    if lock_bytes.is_empty() || lock_bytes.len() > 1024 * 1024 {
        return Err(invalid("RUST_WORKER_LOCK_INVALID"));
    }
    Ok(version.into())
}

fn metal_tools(
    options: &Options,
    budget: &RuntimeBudget<'_>,
    tools: &mut PreparedNativeTools,
    authorize: &mut dyn FnMut(NativeAction<'_>) -> io::Result<()>,
) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let xcrun = PathBuf::from("/usr/bin/xcrun").canonicalize()?;
    let select = PathBuf::from("/usr/bin/xcode-select").canonicalize()?;
    let mut probe = |program: &Path, arguments: &[&str]| -> io::Result<String> {
        if program.starts_with(&options.root) || !fs::symlink_metadata(program)?.is_file() {
            return Err(invalid("RUST_WORKER_METAL_TOOL_INVALID"));
        }
        let arguments = arguments.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        authorize(NativeAction::ToolProbe {
            program,
            arguments: &arguments,
        })?;
        let capture = Command::new(program)
            .args(&arguments)
            .env_clear()
            .bounded_capture(
                ProcessLimits {
                    timeout: budget.remaining()?.min(Duration::from_secs(15)),
                    max_output_bytes: 16 * 1024,
                },
                budget.cancel,
            )?;
        if capture.termination != Termination::Exited || !capture.output.status.success() {
            return Err(invalid("RUST_WORKER_METAL_TOOL_UNAVAILABLE"));
        }
        let value = text(capture.output.stdout)?;
        if value.len() > 4096 || !Path::new(&value).is_absolute() {
            return Err(invalid("RUST_WORKER_METAL_PATH_INVALID"));
        }
        Ok(value)
    };
    let developer = PathBuf::from(probe(&select, &["--print-path"])?).canonicalize()?;
    if developer.starts_with(&options.root) {
        return Err(invalid("RUST_WORKER_METAL_DEVELOPER_PATH_INVALID"));
    }
    tools.tools.programs.insert("xcrun".into(), xcrun.clone());
    tools.tools.programs.insert("xcode-select".into(), select);
    for name in ["metal", "metallib"] {
        let path = PathBuf::from(probe(&xcrun, &["--find", name])?).canonicalize()?;
        if path.starts_with(&options.root) || !fs::symlink_metadata(&path)?.is_file() {
            return Err(invalid("RUST_WORKER_METAL_COMPILER_INVALID"));
        }
        tools.tools.programs.insert(name.into(), path);
    }
    for sdk in ["macosx", "iphoneos"] {
        let root =
            PathBuf::from(probe(&xcrun, &["--sdk", sdk, "--show-sdk-path"])?).canonicalize()?;
        if root.starts_with(&options.root) {
            return Err(invalid("RUST_WORKER_METAL_SDK_INVALID"));
        }
        for name in ["SDKSettings.json", "SDKSettings.plist"] {
            let path = root.join(name).canonicalize()?;
            if !fs::symlink_metadata(&path)?.is_file() {
                return Err(invalid("RUST_WORKER_METAL_SDK_METADATA_INVALID"));
            }
            tools
                .tools
                .closure
                .insert(format!("sdk:{sdk}:{name}"), path);
        }
        if sdk == "macosx" {
            tools.tools.environment.insert(
                "SDKROOT".into(),
                Value::Literal(root.to_string_lossy().into_owned()),
            );
        }
    }
    tools.tools.environment.insert(
        "DEVELOPER_DIR".into(),
        Value::Literal(developer.to_string_lossy().into_owned()),
    );
    Ok(())
}

/// Caller has already verified the complete primary source verdict. The return
/// value never replaces that verdict; errors keep the outer report non-ready.
pub(super) fn execute(
    options: &Options,
    project: &CurrentProject,
    primary_identity: &evidence::MaterialIdentity,
    directory: &Path,
    budget: &mut RuntimeBudget<'_>,
    authorize: &mut dyn FnMut(NativeAction<'_>) -> io::Result<()>,
) -> io::Result<()> {
    let Some(configuration) = &project.configuration.rust_workers else {
        return Ok(());
    };
    validate(configuration)?;
    if proof_configuration(&budget.read(&file(&options.root, CONFIG)?)?)?
        .rust_workers
        .as_ref()
        != Some(configuration)
    {
        return Err(invalid(
            "RUST_WORKER_POLICY_STALE: canonical recipe differs",
        ));
    }
    let directory = directory.join("rust-workers");
    private_directory(&directory)?;
    let mut versions = BTreeSet::new();
    for worker in &configuration.workers {
        versions.insert(manifest_version(&options.root, worker, budget)?);
        let lock = Path::new(&worker.manifest)
            .parent()
            .unwrap()
            .join("Cargo.lock");
        if !project
            .identity_inputs
            .candidate_inputs
            .contains(&worker.manifest)
            || !project
                .identity_inputs
                .candidate_inputs
                .contains(lock.to_str().unwrap())
        {
            return Err(invalid(
                "RUST_WORKER_INPUT_UNTRACKED: commit the manifest and lock",
            ));
        }
    }
    if versions.len() != 1 {
        return Err(invalid(
            "RUST_WORKER_TOOLCHAIN_CONFLICT: one explicit Rust toolchain per recipe set",
        ));
    }
    let mut tools = discover_rust_tools_until(
        options,
        versions.first().unwrap(),
        budget.deadline,
        budget.cancel,
        authorize,
    )?;
    if configuration
        .workers
        .iter()
        .any(|worker| worker.metal_toolchain)
    {
        metal_tools(options, budget, &mut tools, authorize)?;
    }
    let mut invocation_map = BTreeMap::new();
    let mut checks = Vec::new();
    let mut proof_files = Vec::new();
    for (worker_number, worker) in configuration.workers.iter().enumerate() {
        // Build products/cache are owned scratch, not permanent failure logs.
        let target = tools.storage.path().join(format!("target-{worker_number}"));
        private_directory(&target)?;
        let mut environment = tools.tools.environment.clone();
        environment.insert(
            "CARGO_TARGET_DIR".into(),
            Value::Literal(target.to_string_lossy().into_owned()),
        );
        environment.insert(
            "RUSTC".into(),
            Value::Literal(tools.tools.programs["rustc"].to_string_lossy().into_owned()),
        );
        let mut operations = vec![base_arguments(worker, "test")];
        for (run_number, run) in worker.runs.iter().enumerate() {
            let mut args = base_arguments(worker, "run");
            args.push(Value::Literal("--".into()));
            args.extend(run.arguments.iter().cloned().map(Value::Literal));
            if run.proof_output {
                let proof = directory.join(format!("worker-{worker_number}-run-{run_number}.json"));
                args.extend([
                    Value::Literal("--proof-output".into()),
                    Value::Literal(proof.to_string_lossy().into_owned()),
                ]);
                proof_files.push(proof);
            }
            operations.push(args);
        }
        let mut previous = None;
        for (number, arguments) in operations.into_iter().enumerate() {
            let key = CheckKey {
                provider: evidence::Provider::Native,
                workflow: "decapod.rust-workers.v1".into(),
                job: format!("{}-{number}", worker.name),
                matrix: BTreeMap::from([(
                    "platform".into(),
                    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
                )]),
                // Local-command evidence is supplementary to the strict plan.
                profile: super::super::repository_profile::REQUIRED_PROFILE.into(),
            };
            let invocation = Invocation {
                platform: NativePlatform::current()?,
                program: tools.tools.programs["cargo"].clone(),
                toolchain_files: tools
                    .tools
                    .closure
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .chain(
                        tools
                            .tools
                            .programs
                            .iter()
                            .map(|(k, v)| (format!("installed:{k}"), v.clone())),
                    )
                    .collect(),
                arguments,
                directory: PathBuf::from("."),
                environment: environment.clone(),
                purpose: ExecutionPurpose::SourceCheck,
                capabilities: ReviewedCapabilities::LocalCheckWithOwnedScratch,
                timeout_ms: worker.timeout_seconds * 1000,
                output_limit: budget.output_limit(),
                artifacts: BTreeSet::new(),
            };
            authorize(NativeAction::SourceCheck {
                invocation: &invocation,
            })?;
            checks.push(evidence::ExpectedCheck {
                key: key.clone(),
                kind: evidence::CheckKind::RepositoryLocal,
                definition_digest: evidence::digest(&(worker, number)).map_err(io::Error::other)?,
                invocation_digest: invocation.digest()?,
                toolchain_digest: native_proof::toolchain_digest(&invocation)?,
                executor: evidence::Provider::Native,
                dependencies: previous
                    .into_iter()
                    .map(evidence::Dependency::Check)
                    .collect(),
                requirement: evidence::Requirement::Execute,
            });
            previous = Some(key.clone());
            invocation_map.insert(key, invocation);
        }
    }
    let plan = evidence::ExpectedPlan {
        schema_version: evidence::SCHEMA_VERSION.into(),
        policy_digest: configuration.reviewed_recipes_digest.clone(),
        source_inventory_digest: project.sources.digest().map_err(io::Error::other)?,
        buildkite_enabled: false,
        checks,
    };
    let invocation_bytes = serde_json::to_vec_pretty(&invocation_map.iter().collect::<Vec<_>>())
        .map_err(io::Error::other)?;
    if invocation_bytes.len() > budget.output_limit() {
        return Err(invalid("RUST_WORKER_INVOCATION_RECORD_TOO_LARGE"));
    }
    write_new(&directory.join("invocations.json"), &invocation_bytes)?;
    let mut versions = BTreeMap::new();
    for (name, path) in tools
        .tools
        .closure
        .iter()
        .filter(|(name, _)| name.starts_with("resolution:"))
    {
        let bytes = budget.read(path)?;
        if bytes.len() > 16 * 1024 {
            return Err(invalid("RUST_WORKER_VERSION_RECORD_TOO_LARGE"));
        }
        versions.insert(name, String::from_utf8(bytes).map_err(io::Error::other)?);
    }
    write_new(
        &directory.join("tool-versions.json"),
        &serde_json::to_vec_pretty(&versions).map_err(io::Error::other)?,
    )?;
    write_new(
        &directory.join("expected-plan.json"),
        &serde_json::to_vec_pretty(&plan).map_err(io::Error::other)?,
    )?;
    let identity = native_proof::capture_current_identity(
        &project.identity_inputs,
        &invocation_map,
        suite_limits(options, budget)?,
        budget.cancel,
    )?;
    if !source_matches(primary_identity, &identity) {
        return Err(invalid("RUST_WORKER_SOURCE_STALE_BEFORE_EXECUTION"));
    }
    let snapshot = native_proof::snapshot::prepare_snapshot(
        &project.identity_inputs,
        &native_proof::snapshot::SnapshotReview::frozen(&identity, BTreeSet::new()),
        native_proof::snapshot::SnapshotLimits {
            timeout: budget.remaining()?,
            max_snapshot_bytes: 4 * 1024 * 1024 * 1024,
            max_files: 200_000,
            max_io_bytes: 32 * 1024 * 1024 * 1024,
            git_output_limit: budget.output_limit(),
        },
        &directory,
        budget.cancel,
    )?;
    let run = native_proof::run_native(
        &plan,
        &invocation_map,
        &project.identity_inputs,
        &project.facts,
        SuiteLimits {
            timeout: budget.remaining()?,
            max_checks: 64,
            max_hashed_bytes: 32 * 1024 * 1024 * 1024,
            max_output_bytes: budget.output_limit(),
            git_output_limit: budget.output_limit(),
        },
        &snapshot,
        budget.cancel,
    )?;
    write_new(
        &directory.join("observations.json"),
        &serde_json::to_vec_pretty(&run.evidence).map_err(io::Error::other)?,
    )?;
    for (i, diagnostic) in run.diagnostics.iter().enumerate() {
        write_new(
            &directory.join(format!("check-{i}.stdout")),
            &diagnostic.stdout,
        )?;
        write_new(
            &directory.join(format!("check-{i}.stderr")),
            &diagnostic.stderr,
        )?;
        write_new(&directory.join(format!("check-{i}.json")),&serde_json::to_vec_pretty(&serde_json::json!({
            "key":diagnostic.key,"wall_clock_ms":diagnostic.duration.as_secs_f64()*1000.0,
            "termination":format!("{:?}",diagnostic.termination),"error":diagnostic.error,
            "scope":"supplemental Rust command execution; process success does not establish model parity or calibration",
            "publication_ready":false
        })).map_err(io::Error::other)?)?;
    }
    if let Some(error) = run.failure {
        return Err(invalid(format!("RUST_WORKER_FAILED: {error}")));
    }
    let current = native_proof::capture_current_identity(
        &project.identity_inputs,
        &invocation_map,
        suite_limits(options, budget)?,
        budget.cancel,
    )?;
    if !source_matches(primary_identity, &current)
        || evidence::verify(&plan, &current, &project.facts, &run.evidence).verdict
            != Verdict::Ready
    {
        return Err(invalid(
            "RUST_WORKER_NOT_READY: missing, failed or stale native evidence",
        ));
    }
    let mut proof_digests = BTreeMap::new();
    for path in proof_files {
        proof_digests.insert(
            path.file_name().unwrap().to_string_lossy().to_string(),
            proof_digest(&path, budget)?,
        );
    }
    write_new(
        &directory.join("proof-output-digests.json"),
        &serde_json::to_vec_pretty(&proof_digests).map_err(io::Error::other)?,
    )?;
    Ok(())
}

fn proof_digest(path: &Path, budget: &RuntimeBudget<'_>) -> io::Result<String> {
    budget.remaining()?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 1024 * 1024 {
        return Err(invalid("RUST_WORKER_PROOF_OUTPUT_INVALID"));
    }
    super::super::fs_permissions::check_file(path, false)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        if metadata.mode() & 0o077 != 0 || metadata.nlink() != 1 {
            return Err(invalid("RUST_WORKER_PROOF_OUTPUT_NOT_PRIVATE"));
        }
        // A replaced FIFO must not block validation before its deadline.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut handle = options.open(path)?;
    let opened = handle.metadata()?;
    if !opened.is_file() || opened.len() != metadata.len() {
        return Err(invalid("RUST_WORKER_PROOF_OUTPUT_CHANGED"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != metadata.dev()
            || opened.ino() != metadata.ino()
            || opened.mode() & 0o077 != 0
            || opened.nlink() != 1
        {
            return Err(invalid("RUST_WORKER_PROOF_OUTPUT_CHANGED"));
        }
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut handle)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    budget.remaining()?;
    let after = handle.metadata()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if after.mode() & 0o077 != 0 || after.nlink() != 1 {
            return Err(invalid("RUST_WORKER_PROOF_OUTPUT_CHANGED"));
        }
    }
    if bytes.len() > 1024 * 1024
        || bytes.len() as u64 != opened.len()
        || after.len() != opened.len()
        || after.modified()? != opened.modified()?
    {
        return Err(invalid("RUST_WORKER_PROOF_OUTPUT_CHANGED_OR_TOO_LARGE"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
    if value.as_object().is_none_or(|object| object.is_empty()) {
        return Err(invalid(
            "RUST_WORKER_PROOF_OUTPUT_INVALID: expected a nonempty JSON object",
        ));
    }
    Ok(hash(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn configuration() -> Configuration {
        let workers = vec![Worker {
            name: "probe".into(),
            manifest: "assets/tools/probe/Cargo.toml".into(),
            features: vec![],
            metal_toolchain: false,
            runs: vec![Run {
                arguments: vec!["--probe".into()],
                proof_output: false,
            }],
            timeout_seconds: 60,
        }];
        Configuration {
            reviewed_recipes_digest: recipe_digest(&workers).unwrap(),
            workers,
        }
    }
    fn repin(configuration: &mut Configuration) {
        configuration.reviewed_recipes_digest = recipe_digest(&configuration.workers).unwrap();
    }
    #[test]
    fn recipe_digest_does_not_authorize_malformed_or_unbounded_recipes() {
        assert!(validate(&configuration()).is_ok());
        let mut changed = configuration();
        changed.workers[0].runs[0].arguments.push("changed".into());
        assert!(validate(&changed).is_err());
        for path in [
            "Cargo.toml",
            "/outside/Cargo.toml",
            "a/../Cargo.toml",
            "a//Cargo.toml",
            "a/./Cargo.toml",
            "a\\Cargo.toml",
            "a/not-a-manifest",
        ] {
            let mut changed = configuration();
            changed.workers[0].manifest = path.into();
            repin(&mut changed);
            assert!(validate(&changed).is_err(), "{path}");
        }
        for budget in [0, 1801, u64::MAX] {
            let mut changed = configuration();
            changed.workers[0].timeout_seconds = budget;
            repin(&mut changed);
            assert!(validate(&changed).is_err());
        }
        for argument in [
            "x\0y".to_string(),
            "x".repeat(4097),
            "--proof-output".to_string(),
            "--proof-output=/elsewhere".to_string(),
        ] {
            let mut changed = configuration();
            changed.workers[0].runs[0].arguments = vec![argument];
            repin(&mut changed);
            assert!(validate(&changed).is_err());
        }
        let mut changed = configuration();
        changed.workers.push(changed.workers[0].clone());
        repin(&mut changed);
        assert!(validate(&changed).is_err());
        changed = configuration();
        changed.workers[0].features = vec!["cuda".into(), "cuda".into()];
        repin(&mut changed);
        assert!(validate(&changed).is_err());
        changed = configuration();
        changed.workers[0].runs.clear();
        repin(&mut changed);
        assert!(validate(&changed).is_err());
        changed = configuration();
        changed.workers[0].runs = vec![changed.workers[0].runs[0].clone(); 9];
        repin(&mut changed);
        assert!(validate(&changed).is_err());
    }
    #[test]
    fn accepted_checks_cannot_be_removed_or_changed_by_a_matching_new_digest() {
        let accepted = configuration();
        assert!(preserve_accepted(Some(&accepted), Some(&accepted)).is_ok());
        assert!(preserve_accepted(None, Some(&accepted)).is_err());
        let mut changed = accepted.clone();
        changed.workers[0].runs[0].arguments.clear();
        repin(&mut changed);
        assert!(preserve_accepted(Some(&changed), Some(&accepted)).is_err());
        changed = accepted.clone();
        let mut additional = changed.workers[0].clone();
        additional.name = "second".into();
        additional.manifest = "other/probe/Cargo.toml".into();
        changed.workers.push(additional);
        repin(&mut changed);
        assert!(preserve_accepted(Some(&changed), Some(&accepted)).is_ok());
        assert!(
            serde_json::from_value::<Configuration>(
                serde_json::json!({"reviewed_recipes_digest":"x","workers":[],"ready":true})
            )
            .is_err()
        );
    }
    #[test]
    fn distinct_feature_variants_share_a_manifest_but_duplicates_do_not() {
        let mut config = configuration();
        let mut gpu = config.workers[0].clone();
        gpu.name = "gpu".into();
        gpu.features = vec!["cuda".into(), "diagnostics".into()];
        config.workers.push(gpu.clone());
        repin(&mut config);
        assert!(validate(&config).is_ok());
        let accepted = config.clone();
        let mut duplicate = gpu;
        duplicate.name = "alias".into();
        duplicate.features.reverse();
        config.workers.push(duplicate);
        repin(&mut config);
        assert!(validate(&config).is_err());
        assert!(preserve_accepted(Some(&configuration()), Some(&accepted)).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn private_proof_outputs_reject_links_permissions_malformed_and_oversized_data() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("proof.json");
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancel,
            max_output_bytes: 1024,
            bytes_left: 1024 * 1024,
        };
        fs::write(&path, b"{\"fixture\":true}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(proof_digest(&path, &budget).is_ok());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(proof_digest(&path, &budget).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let alias = temp.path().join("alias");
        symlink(&path, &alias).unwrap();
        assert!(proof_digest(&alias, &budget).is_err());
        fs::remove_file(&alias).unwrap();
        fs::hard_link(&path, &alias).unwrap();
        assert!(proof_digest(&path, &budget).is_err());
        fs::remove_file(&alias).unwrap();
        for contents in ["{", "null", "[]", "{}", ""] {
            fs::write(&path, contents).unwrap();
            assert!(proof_digest(&path, &budget).is_err());
        }
        fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        assert!(proof_digest(&path, &budget).is_err());
        cancel.cancel();
        assert!(proof_digest(&path, &budget).is_err());
    }
}

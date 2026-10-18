//! Repository-owned, local-native readiness adapter.
//!
//! This host supplies invocations to the existing immutable-snapshot runner. It
//! neither interprets workflows nor manufactures provider execution evidence.
//! Tool installations and their declared closure are explicitly reviewed; no
//! Rust, Cargo, Decapod executable, ambient PATH or installation is assumed.
use super::*;
use crate::core::snapshot_provider;
use crate::core::{proof::ProofDef, repository_profile};

pub struct RepositoryHost<'a> {
    options: Options,
    repository: snapshot_provider::Repository,
    remote_url: String,
    project_check: PrerequisiteCallback<'a>,
    authorize: NativeAuthorizer<'a>,
    authorized_commands: BTreeMap<CheckKey, AuthorizedRepositoryProof>,
}

/// In-process authority for one exact, validated repository proof invocation.
/// Only this host can mint the seal, after the complete source/command contract
/// and selection have been checked. Serialized declarations are not seals.
pub struct AuthorizedRepositoryProof {
    invocation_digest: String,
    toolchain_digest: String,
    command_contract_digest: String,
    source_inventory_digest: String,
    source_root: PathBuf,
    repository: String,
}

impl AuthorizedRepositoryProof {
    pub(crate) fn verify(&self, invocation: &Invocation) -> io::Result<()> {
        if invocation.capabilities
            != native_proof::ReviewedCapabilities::RepositoryDeclaredLocalCheck
            || invocation.digest()? != self.invocation_digest
            || native_proof::toolchain_digest(invocation)? != self.toolchain_digest
        {
            return Err(invalid(
                "REPOSITORY_PROOF_SEAL_MISMATCH: invocation or measured tools changed",
            ));
        }
        Ok(())
    }

    fn verify_context(&self, project: &CurrentProject) -> io::Result<()> {
        let declaration = project
            .configuration
            .required_profile
            .as_ref()
            .ok_or_else(|| invalid("repository proof declaration missing"))?;
        let contract = project
            .configuration
            .repository_contract
            .as_ref()
            .ok_or_else(|| invalid("repository proof contract missing"))?;
        repository_profile::validate_contract(
            declaration,
            &project.sources,
            &project.configuration.commands,
            contract,
        )
        .map_err(io::Error::other)?;
        if project.configuration.validation_buildkite_preflights
            || project.identity_inputs.root != self.source_root
            || project.identity_inputs.repository != self.repository
            || project.sources.digest().map_err(io::Error::other)? != self.source_inventory_digest
            || contract.reviewed_commands_digest != self.command_contract_digest
        {
            return Err(invalid(
                "REPOSITORY_PROOF_SEAL_STALE: reviewed source or command contract changed",
            ));
        }
        Ok(())
    }
}

impl<'a> RepositoryHost<'a> {
    pub fn new<P, A>(
        options: Options,
        remote_url: String,
        project_check: P,
        authorize: A,
    ) -> io::Result<Self>
    where
        P: FnMut(&RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> + 'a,
        A: FnMut(NativeAction<'_>) -> io::Result<()> + 'a,
    {
        let name = remote_url
            .strip_prefix("https://github.com/")
            .and_then(|value| value.strip_suffix(".git"))
            .ok_or_else(|| invalid("expected canonical HTTPS GitHub remote"))?;
        let (owner, repository) = name
            .split_once('/')
            .ok_or_else(|| invalid("invalid repository target identity"))?;
        let repository = snapshot_provider::Repository::new(owner, repository)?;
        if repository.https_url() != remote_url
            || ![name, remote_url.as_str()].contains(&options.repository.as_str())
        {
            return Err(invalid(
                "canonical remote differs from independently resolved repository",
            ));
        }
        Ok(Self {
            options,
            repository,
            remote_url,
            project_check: Box::new(project_check),
            authorize: Box::new(authorize),
            authorized_commands: BTreeMap::new(),
        })
    }

    fn query_target(
        &mut self,
        repository: &str,
        branch: &str,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<String> {
        if repository != self.options.repository || branch != self.options.base_branch {
            return Err(invalid("target query identity changed"));
        }
        let reference = format!("refs/heads/{branch}");
        if !snapshot_provider::branch(&reference) {
            return Err(invalid("invalid accepted target branch"));
        }
        (self.authorize)(NativeAction::TargetRead {
            program: &self.options.git,
            remote_url: &self.remote_url,
            full_ref: &reference,
        })?;
        // The operation above authorizes this exact read. Reuse the existing
        // transport's configured authentication and URL-rewrite guard; do not
        // inspect credentials, change configuration or try an auth fallback.
        let mut runner =
            snapshot_provider::OfficialCli::git_only(self.options.git.clone(), |_, _| Ok(()))?;
        let query_budget = snapshot_provider::Budget::new(
            budget.remaining()?.min(Duration::from_secs(30)),
            budget.output_limit().min(16 * 1024 * 1024),
            budget.cancellation(),
        )?;
        snapshot_provider::snapshot::accepted_target(
            &mut runner,
            &self.options.root,
            &self.repository,
            &reference,
            &query_budget,
        )?
        .ok_or_else(|| invalid("ACCEPTED_TARGET_UNAVAILABLE: accepted branch was not found"))
    }
}

impl Host for RepositoryHost<'_> {
    fn project_check(&mut self, budget: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> {
        (self.project_check)(budget)
    }

    fn accepted_target(
        &mut self,
        repository: &str,
        branch: &str,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<String> {
        self.query_target(repository, branch, budget)
    }

    fn select(
        &mut self,
        project: &CurrentProject,
        budget: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>> {
        self.authorized_commands.clear();
        let selection = select_commands(project, budget)?;
        let sources = project
            .sources
            .digest()
            .map_err(|error| vec![Blocker::new("REPOSITORY_PROOF_SEAL", error.to_string())])?;
        let contract = project
            .configuration
            .repository_contract
            .as_ref()
            .ok_or_else(|| {
                vec![Blocker::new(
                    "REPOSITORY_PROOF_SEAL",
                    "validated contract disappeared",
                )]
            })?;
        for (key, binding) in &selection.bindings {
            self.authorized_commands.insert(
                key.clone(),
                AuthorizedRepositoryProof {
                    invocation_digest: binding.invocation_digest.clone(),
                    toolchain_digest: binding.toolchain_digest.clone(),
                    command_contract_digest: contract.reviewed_commands_digest.clone(),
                    source_inventory_digest: sources.clone(),
                    source_root: project.identity_inputs.root.clone(),
                    repository: project.identity_inputs.repository.clone(),
                },
            );
        }
        Ok(selection)
    }

    fn execute(
        &mut self,
        project: &CurrentProject,
        selection: &ExecutionSelection,
        plan: &evidence::ExpectedPlan,
        identity: &evidence::MaterialIdentity,
        evidence_directory: &Path,
        budget: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite> {
        // Authorize the complete frozen selection before executing any check.
        // The common runner retains bounded execution, snapshot custody and
        // independent before/after executable, source and target measurement.
        if selection.native_invocations.keys().collect::<BTreeSet<_>>()
            != self.authorized_commands.keys().collect::<BTreeSet<_>>()
        {
            return Err(invalid(
                "REPOSITORY_PROOF_SEAL_MISSING: select the complete reviewed contract before execution",
            ));
        }
        for (key, invocation) in &selection.native_invocations {
            budget.remaining()?;
            let seal = &self.authorized_commands[key];
            seal.verify_context(project)?;
            seal.verify(invocation)?;
            verify_invocation_path(invocation, &project.identity_inputs.root, budget)?;
            (self.authorize)(NativeAction::RepositoryCheck { invocation, seal })?;
        }
        let result = execute_native(
            project,
            selection,
            plan,
            identity,
            evidence_directory,
            budget,
        )?;
        for invocation in selection.native_invocations.values() {
            verify_invocation_path(invocation, &project.identity_inputs.root, budget)?;
        }
        Ok(result)
    }
}

fn installed_file(path: &str, root: &Path) -> io::Result<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute()
        || path.canonicalize()? != path
        || path.starts_with(root)
        || !fs::symlink_metadata(&path)?.is_file()
    {
        return Err(invalid(
            "repository tools must be canonical installed files outside the source checkout",
        ));
    }
    Ok(path)
}

fn declared_path(
    path: Option<&String>,
    installed: &BTreeSet<PathBuf>,
    root: &Path,
    budget: &RuntimeBudget<'_>,
) -> io::Result<()> {
    let Some(path) = path else {
        return Ok(());
    };
    if path.is_empty() {
        return Ok(());
    }
    let mut entries_left = 4096_usize;
    for directory in std::env::split_paths(path) {
        budget.remaining()?;
        if !directory.is_absolute()
            || directory.canonicalize()? != directory
            || directory.starts_with(root)
            || !directory.is_dir()
        {
            return Err(invalid(
                "repository PATH must name canonical installed tool directories",
            ));
        }
        for entry in fs::read_dir(&directory)? {
            budget.remaining()?;
            entries_left = entries_left
                .checked_sub(1)
                .ok_or_else(|| invalid("repository PATH inventory exceeds its bound"))?;
            let entry = entry?;
            let metadata = fs::metadata(entry.path())?;
            if !metadata.is_file() {
                continue;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            if entry.file_type()?.is_symlink() {
                return Err(invalid(
                    "repository PATH executable aliases are unsupported; declare canonical executable locations",
                ));
            }
            if !installed.contains(&entry.path().canonicalize()?) {
                return Err(invalid(
                    "repository PATH exposes an executable outside the declared toolchain; use a dedicated reviewed tool directory",
                ));
            }
        }
    }
    Ok(())
}

fn verify_invocation_path(
    invocation: &Invocation,
    root: &Path,
    budget: &RuntimeBudget<'_>,
) -> io::Result<()> {
    let installed = invocation
        .toolchain_files
        .values()
        .cloned()
        .chain(std::iter::once(invocation.program.clone()))
        .collect();
    match invocation.environment.get("PATH") {
        Some(native_proof::Value::Literal(path)) => {
            declared_path(Some(path), &installed, root, budget)
        }
        _ => Err(invalid(
            "repository execution requires an explicit reviewed PATH",
        )),
    }
}

fn command_invocation(
    command: &ProofDef,
    execution: &repository_profile::RepositoryExecution,
    root: &Path,
    budget: &RuntimeBudget<'_>,
) -> io::Result<Invocation> {
    budget.remaining()?;
    let program = installed_file(&execution.program, root)?;
    let mut toolchain_files = BTreeMap::new();
    for (name, path) in &execution.toolchain_files {
        budget.remaining()?;
        if name.trim().is_empty() || name == "program" {
            return Err(invalid("invalid declared toolchain label"));
        }
        toolchain_files.insert(name.clone(), installed_file(path, root)?);
    }
    if toolchain_files.is_empty() {
        return Err(invalid(
            "repository command requires its declared runtime/toolchain files",
        ));
    }
    let installed = toolchain_files
        .values()
        .cloned()
        .chain(std::iter::once(program.clone()))
        .collect();
    declared_path(execution.environment.get("PATH"), &installed, root, budget)?;
    if execution.environment.contains_key("HOME") {
        return Err(invalid(
            "repository HOME is reserved for owned execution scratch",
        ));
    }
    let mut environment: BTreeMap<_, _> = execution
        .environment
        .iter()
        .map(|(name, value)| (name.clone(), native_proof::Value::Literal(value.clone())))
        .collect();
    environment.insert(
        "HOME".into(),
        native_proof::Value::ScratchPath("home".into()),
    );
    // An absent PATH can trigger libc's default executable search path. Keep
    // both the caller's PATH and that implicit fallback out of this contract.
    environment
        .entry("PATH".into())
        .or_insert_with(|| native_proof::Value::Literal(String::new()));
    Ok(Invocation {
        platform: native_proof::NativePlatform::current()?,
        program,
        toolchain_files,
        arguments: command
            .args
            .iter()
            .cloned()
            .map(native_proof::Value::Literal)
            .collect(),
        directory: PathBuf::from(&execution.directory),
        environment,
        purpose: native_proof::ExecutionPurpose::SourceCheck,
        capabilities: native_proof::ReviewedCapabilities::RepositoryDeclaredLocalCheck,
        timeout_ms: execution.timeout_seconds.saturating_mul(1000),
        output_limit: budget.output_limit().min(64 * 1024 * 1024),
        artifacts: BTreeSet::new(),
    })
}

/// Select every required repository command only after independently checking
/// the explicit profile, source and command/execution contract. This seam is
/// shared by production and real-run fixtures; it imports no execution receipt.
pub fn select_commands(
    project: &CurrentProject,
    budget: &RuntimeBudget<'_>,
) -> Result<ExecutionSelection, Vec<Blocker>> {
    let contract = (|| -> io::Result<_> {
        budget.remaining()?;
        if project.configuration.validation_buildkite_preflights {
            return Err(invalid(
                "repository local commands cannot supply Buildkite execution evidence",
            ));
        }
        let declaration = project
            .configuration
            .required_profile
            .as_ref()
            .ok_or_else(|| invalid("repository profile is missing"))?;
        let contract = project
            .configuration
            .repository_contract
            .as_ref()
            .ok_or_else(|| invalid("repository command contract is missing"))?;
        let catalog = repository_profile::required_catalog(
            declaration,
            &project.sources,
            &project.facts,
            &project.configuration.commands,
            contract,
        )
        .map_err(io::Error::other)?;
        if catalog.obligations() != project.catalog.obligations() {
            return Err(invalid(
                "repository command catalog differs from the reviewed contract",
            ));
        }
        Ok(contract)
    })()
    .map_err(|error| {
        vec![Blocker::new(
            "REPOSITORY_COMMAND_CONTRACT",
            error.to_string(),
        )]
    })?;

    let mut blockers = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut native_invocations = BTreeMap::new();
    let mut executors = BTreeMap::new();
    for obligation in project.catalog.obligations() {
        let selected = (|| -> io::Result<_> {
            let command = project
                .configuration
                .commands
                .iter()
                .find(|command| command.name == obligation.key.job)
                .ok_or_else(|| invalid("required repository command missing"))?;
            let execution = contract
                .executions
                .get(&command.name)
                .ok_or_else(|| invalid("required execution declaration missing"))?;
            let invocation =
                command_invocation(command, execution, &project.identity_inputs.root, budget)?;
            let invocation_digest = invocation.digest()?;
            let toolchain_digest = native_proof::toolchain_digest(&invocation)?;
            budget.remaining()?;
            Ok((invocation, invocation_digest, toolchain_digest))
        })();
        match selected {
            Ok((invocation, invocation_digest, toolchain_digest)) => {
                bindings.insert(
                    obligation.key.clone(),
                    ExecutionBinding {
                        role: obligation.role,
                        definition_digest: obligation.definition_digest.clone(),
                        invocation_digest,
                        toolchain_digest,
                        executor: evidence::Provider::Native,
                        dependencies: Vec::new(),
                    },
                );
                executors.insert(obligation.key.clone(), evidence::Provider::Native);
                native_invocations.insert(obligation.key.clone(), invocation);
            }
            Err(error) => {
                blockers.push(Blocker::missing(obligation.key.clone(), error.to_string()));
            }
        }
    }
    if blockers.is_empty() {
        Ok(ExecutionSelection {
            policy: ExecutionPolicy {
                buildkite_preflights: false,
                executors,
            },
            bindings,
            native_invocations,
        })
    } else {
        Err(blockers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn options(root: &Path) -> Options {
        Options {
            root: root.into(),
            git: root.join("unavailable-git"),
            repository: "consumer/project".into(),
            base_branch: "main".into(),
            accepted_base_ref: "refs/remotes/origin/main".into(),
            evidence_parent: root.into(),
            timeout: Duration::from_secs(5),
            max_input_bytes: 1024,
            max_files: 100,
            max_output_bytes: 1024,
        }
    }

    #[test]
    fn target_constructor_requires_the_independently_resolved_repository() {
        let root = tempfile::tempdir().unwrap();
        for remote in [
            "https://github.com/other/project.git",
            "https://github.com/consumer/project/extra.git",
            "https://github.com/consumer/../project.git",
            "http://github.com/consumer/project.git",
            "https://token@github.com/consumer/project.git",
        ] {
            assert!(
                RepositoryHost::new(
                    options(root.path()),
                    remote.into(),
                    |_| Ok(ProjectPrerequisites::Passed),
                    |_| Ok(()),
                )
                .is_err()
            );
        }
        assert!(
            RepositoryHost::new(
                options(root.path()),
                "https://github.com/consumer/project.git".into(),
                |_| Ok(ProjectPrerequisites::Passed),
                |_| Ok(()),
            )
            .is_ok()
        );
    }

    #[test]
    fn target_denial_stops_before_git_or_authentication_access() {
        let root = tempfile::tempdir().unwrap();
        let observed = Cell::new(false);
        let mut host = RepositoryHost::new(
            options(root.path()),
            "https://github.com/consumer/project.git".into(),
            |_| Ok(ProjectPrerequisites::Passed),
            |action| {
                if let NativeAction::TargetRead {
                    remote_url,
                    full_ref,
                    ..
                } = action
                {
                    assert_eq!(remote_url, "https://github.com/consumer/project.git");
                    assert_eq!(full_ref, "refs/heads/main");
                    observed.set(true);
                } else {
                    panic!("unexpected authorization request");
                }
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            },
        )
        .unwrap();
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancel,
            max_output_bytes: 1024,
            bytes_left: 1024,
        };
        assert!(
            host.accepted_target("other/project", "main", &budget)
                .is_err()
        );
        assert!(!observed.get());
        let error = host
            .accepted_target("consumer/project", "main", &budget)
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(observed.get());
    }

    #[test]
    fn command_selection_preserves_arguments_and_reserves_owned_home() {
        let storage = tempfile::tempdir().unwrap();
        let root = storage.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        let program = root.join("proof-tool");
        let runtime = root.join("runtime");
        fs::write(&program, b"fixture executable bytes").unwrap();
        fs::write(&runtime, b"fixture runtime bytes").unwrap();
        let command = ProofDef {
            name: "inspect-source".into(),
            command: "proof-tool".into(),
            args: vec!["with spaces".into(), "$HOME".into(), "--exact=value".into()],
            description: String::new(),
            required: true,
        };
        let mut execution = repository_profile::RepositoryExecution {
            program: program.to_string_lossy().into(),
            toolchain_files: BTreeMap::from([("runtime".into(), runtime.to_string_lossy().into())]),
            environment: BTreeMap::from([("LANG".into(), "C".into())]),
            directory: ".".into(),
            timeout_seconds: 5,
        };
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancel,
            max_output_bytes: 1024,
            bytes_left: 1024,
        };
        let invocation = command_invocation(&command, &execution, &source, &budget).unwrap();
        assert_eq!(
            invocation.arguments,
            command
                .args
                .iter()
                .cloned()
                .map(native_proof::Value::Literal)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            invocation.environment["LANG"],
            native_proof::Value::Literal("C".into())
        );
        assert_eq!(
            invocation.environment["PATH"],
            native_proof::Value::Literal(String::new())
        );
        assert_eq!(
            invocation.environment["HOME"],
            native_proof::Value::ScratchPath("home".into())
        );
        assert_eq!(
            invocation.capabilities,
            native_proof::ReviewedCapabilities::RepositoryDeclaredLocalCheck
        );
        execution
            .environment
            .insert("HOME".into(), "/home/user".into());
        assert!(command_invocation(&command, &execution, &source, &budget).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn declared_path_rejects_unmeasured_executables_and_retargetable_aliases() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let storage = tempfile::tempdir().unwrap();
        let root = storage.path().canonicalize().unwrap();
        let bin = root.join("bin");
        fs::create_dir(&bin).unwrap();
        let first = bin.join("first");
        let second = bin.join("second");
        for tool in [&first, &second] {
            fs::write(tool, b"fixture executable bytes").unwrap();
            fs::set_permissions(tool, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = bin.to_string_lossy().into_owned();
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancel,
            max_output_bytes: 1024,
            bytes_left: 1024,
        };
        let source = root.join("source");
        let mut installed = BTreeSet::from([first.clone()]);
        assert!(declared_path(Some(&path), &installed, &source, &budget).is_err());
        installed.insert(second);
        assert!(declared_path(Some(&path), &installed, &source, &budget).is_ok());
        symlink(first, bin.join("alias")).unwrap();
        assert!(declared_path(Some(&path), &installed, &source, &budget).is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn host_seals_reject_tampered_invocations_tools_and_stale_contracts() {
        use crate::core::external_action::authorize_repository_proof;
        let storage = tempfile::tempdir().unwrap();
        let root = storage.path().canonicalize().unwrap();
        let source = root.join("source");
        fs::create_dir(&source).unwrap();
        let program = root.join("proof-tool");
        let runtime = root.join("runtime");
        fs::write(&program, b"measured executable fixture").unwrap();
        fs::write(&runtime, b"measured runtime fixture").unwrap();
        let commands = vec![ProofDef {
            name: "inspect".into(),
            command: "proof-tool".into(),
            args: vec!["input".into()],
            description: String::new(),
            required: true,
        }];
        let mut contract = repository_profile::RepositoryContract {
            reviewed_commands_digest: String::new(),
            source_coverage: Vec::new(),
            executions: BTreeMap::from([(
                "inspect".into(),
                repository_profile::RepositoryExecution {
                    program: program.to_string_lossy().into(),
                    toolchain_files: BTreeMap::from([(
                        "runtime".into(),
                        runtime.to_string_lossy().into(),
                    )]),
                    environment: BTreeMap::new(),
                    directory: ".".into(),
                    timeout_seconds: 5,
                },
            )]),
        };
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&commands, &contract).unwrap();
        let sources = SourceInventory {
            files: BTreeMap::from([("tool.conf".into(), hash(b"reviewed source"))]),
        };
        let declaration = ProfileDeclaration {
            id: repository_profile::REQUIRED_PROFILE.into(),
            reviewed_sources_digest: sources.digest().unwrap(),
        };
        let facts = ApplicabilityFacts {
            event: "pull_request".into(),
            target_branch: "main".into(),
            changed_paths: BTreeSet::new(),
        };
        let catalog = repository_profile::required_catalog(
            &declaration,
            &sources,
            &facts,
            &commands,
            &contract,
        )
        .unwrap();
        let mut project = CurrentProject {
            configuration: ProofConfiguration {
                validation_buildkite_preflights: false,
                required_profile: Some(declaration),
                commands,
                repository_contract: Some(contract),
                ci_attested: None,
                rust_workers: None,
            },
            sources,
            executor: None,
            facts,
            release: None,
            catalog,
            identity_inputs: IdentityInputs {
                root: source.clone(),
                git_program: root.join("unused-git"),
                repository: "consumer/project".into(),
                base_branch: "main".into(),
                accepted_base_ref: "refs/remotes/origin/main".into(),
                declared_untracked: BTreeSet::new(),
                configuration: BTreeSet::new(),
                workflows: BTreeSet::new(),
                candidate_inputs: BTreeSet::new(),
            },
        };
        let mut host = RepositoryHost::new(
            options(&source),
            "https://github.com/consumer/project.git".into(),
            |_| Ok(ProjectPrerequisites::Passed),
            |_| Ok(()),
        )
        .unwrap();
        let cancel = Cancellation::default();
        let budget = RuntimeBudget {
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancel,
            max_output_bytes: 1024,
            bytes_left: 1024,
        };
        let selection = host.select(&project, &budget).unwrap();
        let (key, invocation) = selection.native_invocations.first_key_value().unwrap();
        let seal = &host.authorized_commands[key];
        let store = source.join(".decapod/data");
        authorize_repository_proof(&store, invocation, seal).unwrap();
        seal.verify_context(&project).unwrap();
        let mut tampered = invocation.clone();
        tampered
            .arguments
            .push(native_proof::Value::Literal("extra".into()));
        assert!(authorize_repository_proof(&store, &tampered, seal).is_err());
        fs::write(&runtime, b"replacement runtime fixture").unwrap();
        assert!(authorize_repository_proof(&store, invocation, seal).is_err());
        fs::write(&runtime, b"measured runtime fixture").unwrap();
        project.configuration.commands[0]
            .args
            .push("new argument".into());
        assert!(seal.verify_context(&project).is_err());
        let contract = project.configuration.repository_contract.as_mut().unwrap();
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&project.configuration.commands, contract)
                .unwrap();
        // Even an independently re-reviewed changed declaration needs a fresh
        // selection; an old in-process seal cannot authorize the new contract.
        assert!(seal.verify_context(&project).is_err());
        project.configuration.commands[0].args.pop();
        let contract = project.configuration.repository_contract.as_mut().unwrap();
        contract.reviewed_commands_digest =
            repository_profile::command_contract_digest(&project.configuration.commands, contract)
                .unwrap();
        project
            .sources
            .files
            .insert("tool.conf".into(), hash(b"changed source"));
        project
            .configuration
            .required_profile
            .as_mut()
            .unwrap()
            .reviewed_sources_digest = project.sources.digest().unwrap();
        assert!(seal.verify_context(&project).is_err());
    }
}

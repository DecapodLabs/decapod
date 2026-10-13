use super::*;
use std::process::Command;

fn git_path() -> PathBuf {
    PathBuf::from("/usr/bin/git").canonicalize().unwrap()
}
fn run(root: &Path, args: &[&str]) -> String {
    let out = Command::new(git_path())
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
struct Fixture {
    storage: tempfile::TempDir,
    options: Options,
}
impl Fixture {
    fn new(config: &str) -> Self {
        let storage = tempfile::tempdir().unwrap();
        let root = storage.path().join("source");
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join(".decapod")).unwrap();
        fs::create_dir(storage.path().join("evidence")).unwrap();
        fs::write(root.join(CONFIG), config).unwrap();
        fs::write(root.join(GOVERNANCE), b"{\"sections\":{}}\n").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"decapod\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
        )
        .unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"candidate\"); }\n",
        )
        .unwrap();
        fs::write(
            root.join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"decapod\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        run(&root, &["init", "-b", "work"]);
        run(&root, &["config", "user.name", "Runtime Test"]);
        run(&root, &["config", "user.email", "runtime@example.invalid"]);
        run(&root, &["add", "."]);
        run(&root, &["commit", "-m", "fixture"]);
        run(&root, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
        let options = Options {
            root: root.canonicalize().unwrap(),
            git: git_path(),
            repository: "fixture/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            evidence_parent: storage.path().join("evidence"),
            timeout: Duration::from_secs(60),
            max_input_bytes: 128 * 1024 * 1024,
            max_files: 10_000,
            max_output_bytes: 1024 * 1024,
        };
        Self { storage, options }
    }
    fn inputs(&self) -> IdentityInputs {
        IdentityInputs {
            root: self.options.root.clone(),
            git_program: git_path(),
            repository: "fixture/decapod".into(),
            base_branch: "master".into(),
            accepted_base_ref: "refs/remotes/origin/master".into(),
            declared_untracked: BTreeSet::new(),
            configuration: BTreeSet::from([CONFIG.into()]),
            workflows: BTreeSet::from(["Cargo.toml".into()]),
            candidate_inputs: BTreeSet::from([
                "Cargo.toml".into(),
                "Cargo.lock".into(),
                "src/main.rs".into(),
            ]),
        }
    }
}
struct StopHost {
    prerequisites: ProjectPrerequisites,
    called: usize,
    modify: Option<PathBuf>,
}
impl Host for StopHost {
    fn project_check(&mut self, _: &RuntimeBudget<'_>) -> io::Result<ProjectPrerequisites> {
        self.called += 1;
        if let Some(path) = &self.modify {
            fs::write(path, "changed by prerequisites\n")?;
        }
        Ok(self.prerequisites)
    }
    fn accepted_target(&mut self, _: &str, _: &str, _: &RuntimeBudget<'_>) -> io::Result<String> {
        panic!("must not query target")
    }
    fn select(
        &mut self,
        _: &CurrentProject,
        _: &RuntimeBudget<'_>,
    ) -> Result<ExecutionSelection, Vec<Blocker>> {
        panic!("must not select adapters")
    }
    fn execute(
        &mut self,
        _: &CurrentProject,
        _: &ExecutionSelection,
        _: &evidence::ExpectedPlan,
        _: &evidence::MaterialIdentity,
        _: &Path,
        _: &RuntimeBudget<'_>,
    ) -> io::Result<ExecutedSuite> {
        panic!("must not execute")
    }
}
fn host() -> StopHost {
    StopHost {
        prerequisites: ProjectPrerequisites::Passed,
        called: 0,
        modify: None,
    }
}

#[test]
fn canonical_flag_defaults_false_and_preserves_explicit_value() {
    assert!(
        !proof_configuration(b"[proof]\ncommands=[]\n")
            .unwrap()
            .validation_buildkite_preflights
    );
    assert!(
        proof_configuration(b"[proof]\nvalidation_buildkite_preflights=true\n")
            .unwrap()
            .validation_buildkite_preflights
    );
    assert!(proof_configuration(b"[proof]\nvalidation_buildkite_preflights=\"true\"\n").is_err());
    assert!(proof_configuration(b"[proof]\nvalidation_buildkite_preflights=true\nvalidation_buildkite_preflights=false\n").is_err());
    assert!(
        proof_configuration(
            b"[proof.required_profile]\nid=\"x\"\nreviewed_sources_digest=\"x\"\nextra=true\n"
        )
        .is_err()
    );
}
#[test]
fn failed_partial_prerequisites_never_reach_execution() {
    let f = Fixture::new("[proof]\n");
    let mut h = host();
    h.prerequisites = ProjectPrerequisites::Failed;
    let report = validate(&f.options, &mut h, &Cancellation::default());
    assert!(!report.ready);
    assert_eq!(report.scope, READINESS_SCOPE);
    assert_eq!(report.exit_code(), 1);
    assert_eq!(h.called, 1);
    assert!(
        report.blockers[0]
            .detail
            .contains("PROJECT_PREREQUISITES_FAILED")
    );
}
#[test]
fn preparation_changes_require_commit_and_rerun() {
    let f = Fixture::new("[proof]\n");
    let mut h = host();
    h.modify = Some(f.options.root.join(CONFIG));
    let report = validate(&f.options, &mut h, &Cancellation::default());
    assert!(!report.ready);
    assert!(report.blockers[0].detail.contains("COMMIT_AND_RERUN"));
}
#[test]
fn missing_profile_never_becomes_legacy_receipt_green() {
    let f = Fixture::new("[proof]\n");
    let mut h = host();
    let report = validate(&f.options, &mut h, &Cancellation::default());
    assert!(!report.ready);
    assert!(!report.blockers.is_empty());
    assert_eq!(h.called, 1);
    // No release configuration exists, so either the missing profile or missing
    // real production input blocks. No legacy validation receipt is consulted.
    assert_eq!(report.exit_code(), 1);
}
#[test]
fn evidence_cannot_be_written_into_checkout_or_git_metadata() {
    let mut f = Fixture::new("[proof]\n");
    let mut h = host();
    f.options.evidence_parent = f.options.root.clone();
    let report = validate(&f.options, &mut h, &Cancellation::default());
    assert!(!report.ready);
    assert_eq!(h.called, 0);
    assert!(
        report.blockers[0]
            .detail
            .contains("outside source/Git metadata")
    );
    f.options.evidence_parent = f.options.root.join(".git");
    assert_eq!(
        validate(&f.options, &mut h, &Cancellation::default()).exit_code(),
        1
    );
}
#[test]
fn cancellation_stops_before_prerequisites() {
    let f = Fixture::new("[proof]\n");
    let mut h = host();
    let cancel = Cancellation::default();
    cancel.cancel();
    assert_eq!(validate(&f.options, &mut h, &cancel).exit_code(), 1);
    assert_eq!(h.called, 0);
}
#[test]
fn source_inventory_detects_ignored_new_provider_definition() {
    let f = Fixture::new("[proof]\n");
    let cancel = Cancellation::default();
    fs::create_dir(f.options.root.join(".buildkite")).unwrap();
    fs::write(
        f.options.root.join(".buildkite/pipeline.yml"),
        "steps: []\n",
    )
    .unwrap();
    let discovered = discover_source_inventory(&f.options, &cancel).unwrap();
    assert!(discovered.files.contains_key(".buildkite/pipeline.yml"));
}
#[test]
fn readiness_and_init_share_complete_source_inventory_and_budgets() {
    let mut f = Fixture::new("[proof]\n");
    for (path, contents) in [
        (".cargo/config.toml", "[build]\n"),
        ("assets/build/defs.bzl", "# build contract\n"),
        (".github/workflows/unknown.yml", "jobs: {}\n"),
        ("scripts/check.sh", "exit 1\n"),
    ] {
        fs::create_dir_all(f.options.root.join(path).parent().unwrap()).unwrap();
        fs::write(f.options.root.join(path), contents).unwrap();
    }
    let init =
        crate::core::init_reconciliation::discover_source_inventory(&f.options.root).unwrap();
    let runtime = discover_source_inventory(&f.options, &Cancellation::default()).unwrap();
    assert_eq!(init, runtime);
    assert_eq!(init.digest().unwrap(), runtime.digest().unwrap());
    assert!(runtime.files.contains_key("Cargo.toml"));
    assert!(runtime.files.contains_key("assets/build/defs.bzl"));
    f.options.max_input_bytes = 1;
    assert!(discover_source_inventory(&f.options, &Cancellation::default()).is_err());
    f.options.max_input_bytes = 1024 * 1024;
    f.options.max_files = 1;
    assert!(discover_source_inventory(&f.options, &Cancellation::default()).is_err());
    let cancelled = Cancellation::default();
    cancelled.cancel();
    assert!(discover_source_inventory(&f.options, &cancelled).is_err());
}
#[test]
fn paths_are_nul_complete_and_safe() {
    assert!(paths(b"foo").is_err());
    assert!(paths(b"../outside\0").is_err());
    assert!(paths(b".git/config\0").is_err());
    assert!(paths(b"nested\\path\0").is_err());
    assert_eq!(paths(b"hello world\0src/lib.rs\0").unwrap().len(), 2);
}
#[test]
fn source_selector_retains_foreign_platform_and_sequence_failures() {
    let tools = ReviewedNativeTools {
        programs: BTreeMap::new(),
        closure: BTreeMap::new(),
        environment: BTreeMap::new(),
    };
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: BTreeSet::from(["src/core/a.rs".into()]),
    };
    assert!(
        source_invocation("contracts", &facts, &tools, Duration::from_secs(1), 1024)
            .unwrap_err()
            .to_string()
            .contains("SEQUENCE")
    );
    #[cfg(target_os = "linux")]
    assert!(
        source_invocation(
            "golden-vectors-macos",
            &facts,
            &tools,
            Duration::from_secs(1),
            1024
        )
        .unwrap_err()
        .to_string()
        .contains("PLATFORM")
    );
    assert!(
        source_invocation(
            "workflow-owned",
            &facts,
            &tools,
            Duration::from_secs(1),
            1024
        )
        .is_err()
    );
}

// Actual compiler/candidate smoke test. No fake green command is substituted.
// It is opt-in because ordinary test runners need not have an installed reviewed
// Rust toolchain. Running this requires the explicit absolute toolchain path.
#[test]
#[ignore = "requires DECAPOD_RUNTIME_TEST_TOOLCHAIN pointing to installed Rust toolchain"]
fn actual_native_candidate_build_is_measured_but_remains_partial() {
    let bin = PathBuf::from(
        std::env::var("DECAPOD_RUNTIME_TEST_TOOLCHAIN").expect("explicit toolchain/bin path"),
    );
    let f = Fixture::new("[proof]\n");
    let inputs = f.inputs();
    let mut tools = ReviewedNativeTools {
        programs: BTreeMap::new(),
        closure: BTreeMap::new(),
        environment: BTreeMap::new(),
    };
    tools
        .programs
        .insert("cargo".into(), bin.join("cargo").canonicalize().unwrap());
    tools
        .programs
        .insert("rustc".into(), bin.join("rustc").canonicalize().unwrap());
    // Fixture-specific reviewed closure, not a production toolchain attestation.
    tools.closure.insert(
        "fixture compiler".into(),
        bin.join("rustc").canonicalize().unwrap(),
    );
    tools.environment.insert(
        "PATH".into(),
        native_proof::Value::Literal(format!("{}:/usr/bin:/bin", bin.display())),
    );
    tools.environment.insert(
        "CARGO_HOME".into(),
        native_proof::Value::ScratchPath("cargo-home".into()),
    );
    tools.environment.insert(
        "HOME".into(),
        native_proof::Value::ScratchPath("home".into()),
    );
    let before = run(&f.options.root, &["rev-parse", "HEAD"]);
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: BTreeSet::new(),
    };
    let mut authorized = 0;
    let result = run_native_source_slice(
        &inputs,
        &facts,
        &["candidate-build".into()],
        &tools,
        &f.options.evidence_parent,
        SuiteLimits {
            timeout: Duration::from_secs(90),
            max_checks: 10,
            max_hashed_bytes: 1024 * 1024 * 1024,
            max_output_bytes: 1024 * 1024,
            git_output_limit: 1024 * 1024,
        },
        &Cancellation::default(),
        &mut |_| {
            authorized += 1;
            Ok(())
        },
    )
    .unwrap();
    assert!(!result.ready);
    assert_eq!(authorized, 1);
    assert_eq!(result.checks.len(), 1);
    assert_eq!(result.checks[0].exit_code, Some(0));
    assert!(result.checks[0].artifact_digest.is_some());
    assert_eq!(before, run(&f.options.root, &["rev-parse", "HEAD"]));
    assert!(run(&f.options.root, &["status", "--porcelain"]).is_empty());
    assert!(f.storage.path().join("evidence/slice-0.stderr").is_file());
}

#[test]
fn native_host_requires_matching_canonical_remote() {
    let f = Fixture::new("[proof]\n");
    let tools = ReviewedNativeTools {
        programs: BTreeMap::new(),
        closure: BTreeMap::new(),
        environment: BTreeMap::new(),
    };
    assert!(
        NativeHost::new(
            f.options,
            tools,
            "https://github.com/other/repository.git".into(),
            |_| Ok(ProjectPrerequisites::Passed),
            |_| Ok(())
        )
        .is_err()
    );
}

#[test]
#[ignore = "requires installed project Rust toolchain and reviewed probes"]
fn installed_tool_discovery_measures_real_pinned_rust_without_credentials() {
    let mut f = Fixture::new("[proof]\n");
    f.options.max_input_bytes = 2 * 1024 * 1024 * 1024;
    let mut probes = Vec::new();
    let prepared = discover_rust_tools(
        &f.options,
        "1.96.1",
        &Cancellation::default(),
        &mut |action| {
            if let NativeAction::ToolProbe { program, arguments } = action {
                probes.push((program.to_path_buf(), arguments.to_vec()));
            } else {
                panic!("only finite probes are allowed during discovery")
            }
            Ok(())
        },
    )
    .unwrap();
    assert!(prepared.tools.programs.contains_key("cargo"));
    assert!(prepared.tools.programs.contains_key("cc"));
    assert!(
        prepared
            .tools
            .closure
            .keys()
            .any(|k| k.starts_with("rust-target:"))
    );
    assert!(
        probes
            .iter()
            .any(|(_, args)| args == &["--print".to_string(), "sysroot".to_string()])
    );
    assert!(
        prepared
            .storage
            .path()
            .join("cargo")
            .read_dir()
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        prepared
            .storage
            .path()
            .join("home")
            .read_dir()
            .unwrap()
            .next()
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn private_creation_remains_private_under_permissive_umask() {
    if let Ok(root) = std::env::var("DECAPOD_PRIVATE_RUNTIME_CHILD") {
        use std::os::unix::fs::PermissionsExt;
        let root = PathBuf::from(root);
        // Isolated child process: never change the test runner's global umask.
        unsafe {
            libc::umask(
                std::env::var("DECAPOD_PRIVATE_RUNTIME_UMASK")
                    .unwrap()
                    .parse::<u32>()
                    .unwrap(),
            );
        }
        let temp = private_tempdir(&root, "owned-").unwrap();
        private_directory(&temp.path().join("home")).unwrap();
        write_new(&temp.path().join("evidence"), b"private").unwrap();
        assert_eq!(
            temp.path().metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            temp.path()
                .join("home")
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            temp.path()
                .join("evidence")
                .metadata()
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        native_snapshot_preserves_only_reviewed_toolchain_alias_and_target();
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    for mask in [0, 2] {
        let status=Command::new(std::env::current_exe().unwrap())
        .args(["--exact","core::validation_runtime::tests::private_creation_remains_private_under_permissive_umask","--nocapture"])
        .env("DECAPOD_PRIVATE_RUNTIME_CHILD",temp.path()).env("DECAPOD_PRIVATE_RUNTIME_UMASK",mask.to_string()).status().unwrap();
        assert!(status.success());
    }
}

#[cfg(unix)]
#[test]
fn readiness_and_init_bind_the_same_reviewed_toolchain_alias() {
    let f = Fixture::new("[proof]\n");
    let target = crate::core::init_reconciliation::TOOLCHAIN_TARGET;
    fs::create_dir_all(f.options.root.join(target).parent().unwrap()).unwrap();
    fs::write(
        f.options.root.join(target),
        "[toolchain]\nchannel='1.96.1'\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(target, f.options.root.join("rust-toolchain.toml")).unwrap();
    let init =
        crate::core::init_reconciliation::discover_source_inventory(&f.options.root).unwrap();
    let runtime = discover_source_inventory(&f.options, &Cancellation::default()).unwrap();
    assert_eq!(init, runtime);
    assert_ne!(runtime.files["rust-toolchain.toml"], runtime.files[target]);
}

#[cfg(unix)]
#[test]
fn native_snapshot_preserves_only_reviewed_toolchain_alias_and_target() {
    use crate::core::init_reconciliation::{TOOLCHAIN_ALIAS, TOOLCHAIN_TARGET};
    use crate::core::native_proof::snapshot::{SnapshotLimits, SnapshotReview, prepare_snapshot};
    use std::os::unix::fs::symlink;
    let f = Fixture::new("[proof]\n");
    fs::create_dir_all(f.options.root.join(TOOLCHAIN_TARGET).parent().unwrap()).unwrap();
    fs::write(
        f.options.root.join(TOOLCHAIN_TARGET),
        "[toolchain]\nchannel='1.96.1'\n",
    )
    .unwrap();
    symlink(TOOLCHAIN_TARGET, f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
    run(&f.options.root, &["add", "."]);
    run(
        &f.options.root,
        &["commit", "-m", "reviewed toolchain alias"],
    );
    let mut inputs = f.inputs();
    inputs
        .configuration
        .extend([TOOLCHAIN_ALIAS.into(), TOOLCHAIN_TARGET.into()]);
    inputs
        .workflows
        .extend([TOOLCHAIN_ALIAS.into(), TOOLCHAIN_TARGET.into()]);
    inputs
        .candidate_inputs
        .extend([TOOLCHAIN_ALIAS.into(), TOOLCHAIN_TARGET.into()]);
    let cancel = Cancellation::default();
    let limits = native_proof::SuiteLimits {
        timeout: Duration::from_secs(30),
        max_hashed_bytes: 128 * 1024 * 1024,
        max_checks: 1,
        max_output_bytes: 1024 * 1024,
        git_output_limit: 1024 * 1024,
    };
    let snapshot_limits = SnapshotLimits {
        timeout: Duration::from_secs(30),
        max_snapshot_bytes: 16 * 1024 * 1024,
        max_files: 1000,
        max_io_bytes: 128 * 1024 * 1024,
        git_output_limit: 1024 * 1024,
    };
    let identity =
        native_proof::capture_current_identity(&inputs, &BTreeMap::new(), limits, &cancel).unwrap();
    let review = SnapshotReview::frozen(&identity, BTreeSet::new());
    let snapshot = prepare_snapshot(
        &inputs,
        &review,
        snapshot_limits,
        &f.options.evidence_parent,
        &cancel,
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            snapshot.root().metadata().unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            snapshot
                .root()
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
    assert_eq!(
        fs::read_link(snapshot.root().join(TOOLCHAIN_ALIAS)).unwrap(),
        Path::new(TOOLCHAIN_TARGET)
    );
    assert_eq!(
        fs::read(snapshot.root().join(TOOLCHAIN_TARGET)).unwrap(),
        fs::read(f.options.root.join(TOOLCHAIN_TARGET)).unwrap()
    );
    assert!(
        fs::symlink_metadata(snapshot.root().join(TOOLCHAIN_TARGET))
            .unwrap()
            .is_file()
    );
    for target in ["Cargo.toml", "../outside"] {
        fs::remove_file(f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
        symlink(target, f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
        assert!(
            prepare_snapshot(
                &inputs,
                &review,
                snapshot_limits,
                &f.options.evidence_parent,
                &cancel
            )
            .is_err()
        );
    }
    // A valid working link must not launder a wrong staged/index link.
    fs::remove_file(f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
    symlink("Cargo.toml", f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
    run(&f.options.root, &["add", TOOLCHAIN_ALIAS]);
    fs::remove_file(f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
    symlink(TOOLCHAIN_TARGET, f.options.root.join(TOOLCHAIN_ALIAS)).unwrap();
    let current =
        native_proof::capture_current_identity(&inputs, &BTreeMap::new(), limits, &cancel).unwrap();
    assert!(
        prepare_snapshot(
            &inputs,
            &SnapshotReview::frozen(&current, BTreeSet::new()),
            snapshot_limits,
            &f.options.evidence_parent,
            &cancel
        )
        .is_err()
    );
}

#[test]
fn validation_phase_assigns_missing_configuration_to_agent_and_never_requests_prebuild() {
    for buildkite in [false, true] {
        let f = Fixture::new(&format!(
            "[proof]\nvalidation_buildkite_preflights={buildkite}\n"
        ));
        let mut h = host();
        h.prerequisites = if buildkite {
            ProjectPrerequisites::ProviderRequired
        } else {
            ProjectPrerequisites::Passed
        };
        let report = validate(&f.options, &mut h, &Cancellation::default());
        assert_eq!(report.exit_code(), 1);
        assert!(
            report
                .blockers
                .iter()
                .any(|b| b.detail.contains("MISSING_PROOF_PROFILE"))
        );
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["ready"], false);
        assert_eq!(value["agent_guidance"]["phase"], "validation");
        assert_eq!(value["agent_guidance"]["owner"], "agent");
        assert_eq!(value["agent_guidance"]["entry_command"], "decapod validate");
        assert_eq!(
            value["agent_guidance"]["next_action"],
            "repair_and_revalidate"
        );
        assert!(
            report
                .agent_guidance
                .execution
                .contains("no separate manual build")
        );
        assert!(
            report
                .agent_guidance
                .human_input_when
                .contains("missing authorization")
        );
        assert!(!f.options.root.join("target").exists());
    }
}

#[test]
fn missing_required_build_test_sources_fail_before_provider_execution() {
    let f = Fixture::new("[proof]\n");
    let digest = crate::core::init_reconciliation::discover_source_inventory(&f.options.root)
        .unwrap()
        .digest()
        .unwrap();
    fs::write(
        f.options.root.join(CONFIG),
        format!(
            "[proof.required_profile]\nid={:?}\nreviewed_sources_digest={digest:?}\n",
            profile::REQUIRED_PROFILE
        ),
    )
    .unwrap();
    run(&f.options.root, &["add", CONFIG]);
    run(
        &f.options.root,
        &["commit", "-m", "declare incomplete contract"],
    );
    let mut h = host();
    let report = validate(&f.options, &mut h, &Cancellation::default());
    assert_eq!(report.exit_code(), 1);
    assert!(
        report
            .blockers
            .iter()
            .any(|b| b.detail.contains("MISSING_PROOF_SOURCE"))
    );
    assert_eq!(h.called, 1);
    assert_eq!(report.agent_guidance.next_action, "repair_and_revalidate");
    assert!(report.evidence_path.is_none());
}

#[test]
fn successful_validation_preserves_readiness_scope_and_existing_delivery_authority() {
    let guidance = ValidationGuidance::for_outcome(true);
    assert_eq!(guidance.next_action, "review_delivery_readiness");
    assert!(guidance.instruction.contains("human intent"));
    assert!(guidance.instruction.contains("reported readiness scope"));
    assert!(guidance.instruction.contains("existing authorization"));
}

#[test]
fn reviewed_controller_mismatch_is_actionable_without_provider_dispatch_or_readiness() {
    for buildkite in [false, true] {
        let f = Fixture::new("[proof]\n");
        // Complete but unsupported workflow bytes distinguish an actual
        // controller mismatch from missing sources or an unreconciled pin.
        for path in profile::catalog_source_paths() {
            let file = f.options.root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, b"# changed source contract\n").unwrap();
        }
        for (path, bytes) in profile::definition_files() {
            let file = f.options.root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        let inventory = discover_source_inventory(&f.options, &Cancellation::default()).unwrap();
        let reviewed = inventory.digest().unwrap();
        for (digest, expected_code, expected_action) in [
            (
                hash(b"unreviewed inventory"),
                "READINESS_BLOCKED",
                "repair_and_revalidate",
            ),
            (
                reviewed,
                "CONTROLLER_CONTRACT_UNSUPPORTED",
                "obtain_supported_controller",
            ),
        ] {
            fs::write(
                f.options.root.join(CONFIG),
                format!(
                    "[proof]\nvalidation_buildkite_preflights={buildkite}\n[proof.required_profile]\nid={:?}\nreviewed_sources_digest={digest:?}\n",
                    profile::REQUIRED_PROFILE
                ),
            )
            .unwrap();
            run(&f.options.root, &["add", "."]);
            run(&f.options.root, &["commit", "-m", "review source contract"]);
            // StopHost panics on target queries, selection or execution.
            let mut h = host();
            h.prerequisites = if buildkite {
                ProjectPrerequisites::ProviderRequired
            } else {
                ProjectPrerequisites::Passed
            };
            let report = validate(&f.options, &mut h, &Cancellation::default());
            assert_eq!(h.called, 1);
            assert!(!report.ready);
            assert_eq!(report.exit_code(), 1);
            assert_eq!(report.blockers.len(), 1);
            assert_eq!(report.blockers[0].code, expected_code);
            assert_eq!(report.agent_guidance.next_action, expected_action);
            assert!(report.stage_dispositions.is_empty());
            assert!(report.evidence_path.is_none());
            assert!(!f.options.root.join("target").exists());
            assert_eq!(fs::read_dir(&f.options.evidence_parent).unwrap().count(), 0);
            if expected_code == "CONTROLLER_CONTRACT_UNSUPPORTED" {
                assert!(
                    report.blockers[0]
                        .detail
                        .contains(".github/workflows/ci.yml")
                );
                let instruction = report.agent_guidance.instruction;
                assert!(
                    instruction
                        .contains("controller does not support this reviewed source contract")
                );
                assert!(instruction.contains("Rerunning the same controller, repinning configuration or running a manual build cannot resolve"));
                assert!(instruction.contains("independently supported controller"));
                assert!(instruction.contains("preserve the blocker if none is available"));
                assert!(instruction.contains("No automatic controller bootstrap is implemented"));
                let value = serde_json::to_value(&report).unwrap();
                assert_eq!(value["ready"], false);
                assert_eq!(value["blockers"][0]["code"], expected_code);
            } else {
                assert!(
                    report.blockers[0]
                        .detail
                        .starts_with("PROOF_PROFILE_RECONCILIATION_REQUIRED:")
                );
            }
        }
    }
}

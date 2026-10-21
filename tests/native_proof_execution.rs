// Repository integration tests. Unsupported hosts fail explicitly; they do not
// substitute another platform or report an empty native proof as successful.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn unsupported_native_host_is_a_hard_error() {
    assert!(decapod::core::native_proof::NativePlatform::current().is_err());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod supported {
    use decapod::core::bounded_process::{
        BoundedCommand, Cancellation, ProcessLimits, Termination,
    };
    use decapod::core::native_proof::*;
    use decapod::core::readiness_evidence::*;
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
        process::Command,
        time::{Duration, Instant},
    };

    struct CompiledFixture {
        _owned: Option<tempfile::TempDir>,
        path: PathBuf,
    }
    fn compiled_fixture() -> std::sync::Arc<CompiledFixture> {
        use std::sync::{Arc, Mutex, OnceLock, Weak};
        static CACHE: OnceLock<Mutex<Weak<CompiledFixture>>> = OnceLock::new();
        let mut cached = CACHE
            .get_or_init(|| Mutex::new(Weak::new()))
            .lock()
            .unwrap();
        if let Some(fixture) = cached.upgrade() {
            return fixture;
        }
        if let Some(packaged) =
            option_env!("DECAPOD_NATIVE_PROOF_TEST_FIXTURE").filter(|p| !p.is_empty())
        {
            let mut candidates = vec![PathBuf::from(packaged)];
            for variable in ["RUNFILES_DIR", "TEST_SRCDIR"] {
                if let Some(root) = std::env::var_os(variable) {
                    candidates.push(
                        PathBuf::from(root)
                            .join("_main")
                            .join(packaged.trim_start_matches("./")),
                    );
                }
            }
            let path = candidates
                .into_iter()
                .find_map(|p| p.canonicalize().ok())
                .expect("Bazel must provide its declared test-only fixture in runfiles");
            let fixture = Arc::new(CompiledFixture { _owned: None, path });
            *cached = Arc::downgrade(&fixture);
            return fixture;
        }
        let owned = tempfile::Builder::new()
            .prefix("native-proof-test-tool-")
            .tempdir()
            .unwrap();
        let source = owned.path().join("fixture.rs");
        fs::write(&source, include_str!("fixtures/native_proof_fixture.rs")).unwrap();
        let path = owned.path().join("native-proof-fixture");
        let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
        let output = Command::new(compiler)
            .args(["--edition=2024", "--crate-name", "native_proof_fixture"])
            .arg(&source)
            .arg("-o")
            .arg(&path)
            .bounded_output(Duration::from_secs(30))
            .expect("an installed Rust compiler is required for native proof fixtures");
        assert!(
            output.status.success(),
            "fixture compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let fixture = Arc::new(CompiledFixture {
            path: path.canonicalize().unwrap(),
            _owned: Some(owned),
        });
        *cached = Arc::downgrade(&fixture);
        fixture
    }

    fn git(root: &std::path::Path, args: &[&str]) {
        let output = Command::new("/usr/bin/git")
            .args(args)
            .current_dir(root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .bounded_output(Duration::from_secs(5))
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    struct Fixture {
        _compiled: std::sync::Arc<CompiledFixture>,
        repo: tempfile::TempDir,
        scratch: tempfile::TempDir,
        snapshot: OwnedExecutionSnapshot,
        inputs: IdentityInputs,
        plan: ExpectedPlan,
        invocations: BTreeMap<CheckKey, Invocation>,
        facts: ApplicabilityFacts,
    }
    impl Fixture {
        fn new() -> Self {
            let compiled = compiled_fixture();
            let repo = tempfile::tempdir().unwrap();
            let scratch = tempfile::tempdir().unwrap();
            for (name, content) in [
                ("config", "policy"),
                ("workflow", "source definition"),
                ("source", "candidate source"),
            ] {
                fs::write(repo.path().join(name), content).unwrap();
            }
            git(repo.path(), &["init", "-b", "master"]);
            git(repo.path(), &["add", "."]);
            git(repo.path(), &["commit", "-m", "base fixture"]);
            git(repo.path(), &["checkout", "-b", "work"]);
            let owned = tempfile::tempdir().unwrap();
            let checkout = owned.path().join("checkout");
            git(
                owned.path(),
                &[
                    "clone",
                    "--no-hardlinks",
                    "--no-checkout",
                    repo.path().to_str().unwrap(),
                    checkout.to_str().unwrap(),
                ],
            );
            git(&checkout, &["checkout", "--detach", "HEAD"]);
            let snapshot =
                OwnedExecutionSnapshot::prepared(owned, std::path::Path::new("checkout")).unwrap();
            let inputs = IdentityInputs {
                root: repo.path().canonicalize().unwrap(),
                git_program: PathBuf::from("/usr/bin/git").canonicalize().unwrap(),
                repository: "fixture:local".into(),
                base_branch: "master".into(),
                accepted_base_ref: "refs/heads/master".into(),
                declared_untracked: Default::default(),
                configuration: ["config".into()].into(),
                workflows: ["workflow".into()].into(),
                candidate_inputs: ["source".into()].into(),
            };
            let mut invocations = BTreeMap::new();
            let mut checks = Vec::new();
            for (i, (kind, purpose)) in [
                (CheckKind::SourceCi, ExecutionPurpose::SourceCheck),
                (CheckKind::DerivedRelease, ExecutionPurpose::ReleaseProof),
                (CheckKind::CandidateBuild, ExecutionPurpose::CandidateBuild),
            ]
            .into_iter()
            .enumerate()
            {
                let key = CheckKey {
                    provider: Provider::Buildkite,
                    workflow: "fixture".into(),
                    job: format!("check-{i}"),
                    matrix: Default::default(),
                    profile: "local-fixture".into(),
                };
                let (arguments, artifacts) = if kind == CheckKind::CandidateBuild {
                    (
                        vec![
                            Value::Literal("artifact".into()),
                            Value::ScratchPath("candidate".into()),
                            Value::Literal("actual candidate bytes".into()),
                        ],
                        ["candidate".into()].into(),
                    )
                } else {
                    (vec![Value::Literal("ok".into())], Default::default())
                };
                let invocation = Invocation {
                    platform: NativePlatform::current().unwrap(),
                    program: compiled.path.clone(),
                    toolchain_files: BTreeMap::new(),
                    arguments,
                    directory: ".".into(),
                    environment: BTreeMap::new(),
                    purpose,
                    capabilities: ReviewedCapabilities::LocalCheckWithOwnedScratch,
                    timeout_ms: 2000,
                    output_limit: 65536,
                    artifacts,
                    following: Vec::new(),
                };
                checks.push(ExpectedCheck {
                    key: key.clone(),
                    kind,
                    definition_digest: digest(&format!("definition-{i}")).unwrap(),
                    invocation_digest: invocation.digest().unwrap(),
                    toolchain_digest: toolchain_digest(&invocation).unwrap(),
                    executor: Provider::Native,
                    dependencies: vec![],
                    requirement: Requirement::Execute,
                });
                invocations.insert(key, invocation);
            }
            let plan = ExpectedPlan {
                schema_version: SCHEMA_VERSION.into(),
                policy_digest: digest(&"policy").unwrap(),
                source_inventory_digest: digest(&"source inventory").unwrap(),
                buildkite_enabled: false,
                checks,
            };
            let facts = ApplicabilityFacts {
                event: "pull_request".into(),
                target_branch: "master".into(),
                changed_paths: ["source".into()].into(),
            };
            Self {
                _compiled: compiled,
                repo,
                scratch,
                snapshot,
                inputs,
                plan,
                invocations,
                facts,
            }
        }
        fn set(&mut self, index: usize, mutate: impl FnOnce(&mut Invocation)) {
            let check = &mut self.plan.checks[index];
            let invocation = self.invocations.get_mut(&check.key).unwrap();
            mutate(invocation);
            check.invocation_digest = invocation.digest().unwrap();
            if invocation.program.is_file() {
                check.toolchain_digest = toolchain_digest(invocation).unwrap();
            }
        }
        fn limits() -> SuiteLimits {
            SuiteLimits {
                timeout: Duration::from_secs(120),
                max_checks: 20,
                max_hashed_bytes: 1024 * 1024 * 1024,
                max_output_bytes: 1024 * 1024,
                git_output_limit: 1024 * 1024,
            }
        }
        fn run(&self) -> std::io::Result<NativeRun> {
            self.run_with(Self::limits(), &Cancellation::default())
        }
        fn run_with(
            &self,
            limits: SuiteLimits,
            cancel: &Cancellation,
        ) -> std::io::Result<NativeRun> {
            run_native(
                &self.plan,
                &self.invocations,
                &self.inputs,
                &self.facts,
                limits,
                &self.snapshot,
                cancel,
            )
        }
        fn verdict(&self, run: &NativeRun) -> Verdict {
            let current = capture_current_identity(
                &self.inputs,
                &self.invocations,
                Self::limits(),
                &Cancellation::default(),
            )
            .unwrap();
            verify(&self.plan, &current, &self.facts, &run.evidence).verdict
        }
        fn clean(&self) {
            assert_eq!(
                fs::read_dir(self.snapshot.root().parent().unwrap())
                    .unwrap()
                    .count(),
                1
            );
            assert_eq!(fs::read_dir(self.scratch.path()).unwrap().count(), 0);
        }
    }

    #[test]
    fn sequence_uses_shared_scratch_and_records_each_executed_command() {
        let mut f = Fixture::new();
        f.set(0, |invocation| {
            invocation.arguments = vec![
                Value::Literal("artifact".into()),
                Value::ScratchPath("shared".into()),
                Value::Literal("produced bytes".into()),
            ];
            invocation.artifacts.insert("shared".into());
            invocation.following.push(InvocationStep {
                program: invocation.program.clone(),
                arguments: vec![
                    Value::Literal("read-artifact".into()),
                    Value::ScratchPath("shared".into()),
                    Value::Literal("produced bytes".into()),
                ],
                directory: ".".into(),
                environment: BTreeMap::new(),
            });
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_none(), "{:?}", run.failure);
        assert_eq!(f.verdict(&run), Verdict::Ready);
        assert_eq!(run.diagnostics[0].steps.len(), 2);
        assert_eq!(run.diagnostics[0].steps[1].exit_code, Some(0));
        assert!(
            String::from_utf8_lossy(&run.diagnostics[0].stdout)
                .contains("verified actual artifact")
        );
        f.clean();
    }

    fn step(invocation: &Invocation, args: &[&str]) -> InvocationStep {
        InvocationStep {
            program: invocation.program.clone(),
            arguments: args
                .iter()
                .map(|arg| Value::Literal((*arg).into()))
                .collect(),
            directory: ".".into(),
            environment: BTreeMap::new(),
        }
    }

    #[test]
    fn sequence_later_failure_stops_before_remaining_commands_or_checks() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.following = vec![step(i, &["exit", "9"]), step(i, &["ok"])]
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_some());
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        assert_eq!(run.diagnostics.len(), 1);
        assert_eq!(run.diagnostics[0].steps.len(), 2);
        assert_eq!(run.diagnostics[0].steps[1].exit_code, Some(9));
        f.clean();
    }

    #[test]
    fn sequence_commands_share_the_original_deadline() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            // Include first-use Mach-O loading, but the second slow command
            // must still consume the remaining original allowance.
            i.timeout_ms = 1500;
            i.arguments = vec![Value::Literal("sleep".into()), Value::Literal("700".into())];
            i.following = vec![step(i, &["sleep", "1100"]), step(i, &["ok"])];
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_some());
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        assert_eq!(run.diagnostics[0].termination, Termination::TimedOut);
        assert_eq!(run.diagnostics[0].steps.len(), 2);
        assert!(run.diagnostics[0].duration < Duration::from_secs(3));
        f.clean();
    }

    #[test]
    fn sequence_cannot_reset_its_output_allowance() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.output_limit = 12;
            i.following = vec![step(i, &["ok"]), step(i, &["ok"])];
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_some());
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        assert_eq!(run.diagnostics[0].termination, Termination::OutputLimit);
        assert!(run.diagnostics[0].steps.len() < 3);
        f.clean();
    }

    #[test]
    fn sequence_rejects_unbound_steps_shells_and_oversized_command_lists() {
        let mut f = Fixture::new();
        let key = f.plan.checks[0].key.clone();
        let invocation = f.invocations.get_mut(&key).unwrap();
        invocation.following.push(step(invocation, &["ok"]));
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.set(0, |i| {
            let mut shell = step(i, &["-c", "true"]);
            shell.program = PathBuf::from("/bin/sh").canonicalize().unwrap();
            i.following = vec![shell];
        });
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.set(0, |i| i.following = vec![step(i, &["ok"]); 64]);
        assert!(f.run().is_err());
    }

    #[test]
    fn sequence_retains_prior_execution_when_a_later_tool_changes() {
        let mut f = Fixture::new();
        let later = f.scratch.path().join("later-tool");
        fs::copy(&f._compiled.path, &later).unwrap();
        f.set(0, |i| {
            i.arguments = vec![
                Value::Literal("mutate".into()),
                Value::Literal(later.to_string_lossy().into_owned()),
            ];
            let mut next = step(i, &["ok"]);
            next.program = later.clone();
            i.following = vec![next];
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_some());
        assert_eq!(run.diagnostics.len(), 1);
        assert_eq!(run.diagnostics[0].steps.len(), 2);
        assert_eq!(run.diagnostics[0].steps[0].exit_code, Some(0));
        assert!(
            run.diagnostics[0].steps[1]
                .error
                .as_ref()
                .unwrap()
                .contains("tool changed")
        );
        assert_eq!(f.verdict(&run), Verdict::NotReady);
    }

    #[test]
    fn toolchain_measurement_respects_the_original_budget_and_cancellation() {
        let f = Fixture::new();
        let invocation = &f.invocations[&f.plan.checks[0].key];
        assert_eq!(
            toolchain_digest_bounded(invocation, Duration::ZERO, &Cancellation::default())
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::TimedOut
        );
        let cancel = Cancellation::default();
        cancel.cancel();
        assert_eq!(
            toolchain_digest_bounded(invocation, Duration::from_secs(15), &cancel)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn successful_suite_measures_actual_exit_output_artifact_and_material() {
        let fixture = Fixture::new();
        let run = fixture.run().unwrap();
        assert!(run.failure.is_none(), "{:?}", run.failure);
        assert_eq!(fixture.verdict(&run), Verdict::Ready);
        assert_eq!(run.evidence.before, run.evidence.after);
        assert_eq!(run.evidence.before.work_branch, "work");
        assert_eq!(run.diagnostics.len(), 3);
        assert_eq!(run.diagnostics[0].stdout, b"checked\n");
        assert_eq!(run.diagnostics[0].stderr, b"diagnostic\n");
        let Observation::Executed {
            conclusion,
            exit_code,
            artifact_digest,
            ..
        } = &run.evidence.results[2].observation
        else {
            panic!()
        };
        assert_eq!(*conclusion, Conclusion::Success);
        assert_eq!(*exit_code, Some(0));
        use sha2::{Digest, Sha256};
        let actual = format!("sha256:{:x}", Sha256::digest(b"actual candidate bytes"));
        assert_eq!(
            *artifact_digest,
            Some(digest(&BTreeMap::from([("candidate", actual)])).unwrap())
        );
        assert_ne!(
            artifact_digest.as_ref().unwrap(),
            &run.evidence.before.candidate_digest
        );
        fixture.clean();
    }
    #[test]
    fn empty_incomplete_and_unbound_plans_fail_before_proof_spawn() {
        let mut f = Fixture::new();
        f.plan.checks.clear();
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks.pop();
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks[0].invocation_digest = digest(&"unreviewed").unwrap();
        assert!(f.run().is_err());
        f.clean();
    }
    #[test]
    fn missing_tool_wrong_tool_digest_and_wrong_backend_fail() {
        let mut f = Fixture::new();
        f.set(0, |i| i.program = "/nonexistent/proof-tool".into());
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks[0].toolchain_digest = digest(&"different executable").unwrap();
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks[0].executor = Provider::Buildkite;
        assert!(f.run().is_err());
    }
    #[test]
    fn actual_nonzero_exit_fails_and_stops_dependent_checks() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.arguments = vec![Value::Literal("exit".into()), Value::Literal("7".into())]
        });
        let run = f.run().unwrap();
        assert!(run.failure.is_some());
        assert_eq!(fixture_result(&run, 0), (Conclusion::Failure, Some(7)));
        assert_eq!(run.evidence.results.len(), 1);
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        f.clean();
    }
    fn fixture_result(run: &NativeRun, index: usize) -> (Conclusion, Option<i32>) {
        let Observation::Executed {
            conclusion,
            exit_code,
            ..
        } = &run.evidence.results[index].observation
        else {
            panic!()
        };
        (*conclusion, *exit_code)
    }
    #[test]
    fn missing_candidate_artifact_cannot_turn_zero_exit_into_success() {
        let mut f = Fixture::new();
        f.set(2, |i| i.arguments = vec![Value::Literal("ok".into())]);
        let run = f.run().unwrap();
        assert_eq!(fixture_result(&run, 2), (Conclusion::Failure, Some(0)));
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        f.clean();
    }
    #[test]
    fn symlink_artifact_and_path_traversal_are_rejected() {
        let mut f = Fixture::new();
        let source = f.repo.path().join("source").to_string_lossy().into_owned();
        f.set(2, |i| {
            i.arguments = vec![
                Value::Literal("symlink-artifact".into()),
                Value::ScratchPath("candidate".into()),
                Value::Literal(source),
            ]
        });
        let run = f.run().unwrap();
        assert_eq!(fixture_result(&run, 2).0, Conclusion::Failure);
        f.clean();
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.arguments.push(Value::ScratchPath("../outside".into()))
        });
        assert!(f.run().is_err());
    }
    #[test]
    fn timeouts_and_suite_deadline_propagate_as_not_ready() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.arguments = vec![
                Value::Literal("sleep".into()),
                Value::Literal("30000".into()),
            ];
            i.timeout_ms = 40;
        });
        let started = Instant::now();
        let run = f.run().unwrap();
        assert!(started.elapsed() < Duration::from_secs(120));
        assert!(run.diagnostics[0].duration < Duration::from_secs(1));
        assert_eq!(fixture_result(&run, 0).0, Conclusion::TimedOut);
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        f.clean();
        let mut limits = Fixture::limits();
        limits.timeout = Duration::from_millis(1);
        assert!(f.run_with(limits, &Cancellation::default()).is_err());
    }
    #[test]
    fn cancellation_prevents_spawn_and_cancels_a_running_check() {
        let f = Fixture::new();
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(f.run_with(Fixture::limits(), &cancel).is_err());
        let mut f = Fixture::new();
        let marker = f.scratch.path().join("started");
        let marker_text = marker.to_string_lossy().into_owned();
        f.set(0, |i| {
            let mut waiting = step(i, &["sleep", "30000"]);
            waiting.arguments.push(Value::Literal(marker_text));
            i.following = vec![waiting, step(i, &["ok"])];
        });
        let cancel = Cancellation::default();
        let other = cancel.clone();
        let waiter = std::thread::spawn(move || {
            let start = Instant::now();
            while !marker.exists() && start.elapsed() < Duration::from_secs(120) {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(
                marker.exists(),
                "fixture did not reach execution before cancellation"
            );
            other.cancel();
        });
        let run = f.run_with(Fixture::limits(), &cancel).unwrap();
        waiter.join().unwrap();
        assert_eq!(fixture_result(&run, 0).0, Conclusion::Cancelled);
        assert_eq!(run.diagnostics[0].steps.len(), 2);
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        fs::remove_file(f.scratch.path().join("started")).unwrap();
        f.clean();
    }
    #[test]
    fn noisy_output_is_bounded_and_never_reported_as_success() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.arguments = vec![Value::Literal("noisy".into())];
            i.output_limit = 1024;
        });
        let run = f.run().unwrap();
        assert_eq!(run.diagnostics[0].termination, Termination::OutputLimit);
        assert!(run.diagnostics[0].stdout.len() <= 1024);
        assert!(run.diagnostics[0].stderr.len() <= 1024);
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        f.clean();
    }
    #[test]
    fn material_mutation_is_measured_and_fails_readiness() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.arguments = vec![
                Value::Literal("mutate".into()),
                Value::Literal("source".into()),
            ]
        });
        let run = f.run().unwrap();
        assert_eq!(
            run.evidence.before.tree_digest,
            run.evidence.after.tree_digest
        );
        assert_eq!(
            fs::read_to_string(f.repo.path().join("source")).unwrap(),
            "candidate source"
        );
        assert_eq!(
            fs::read_to_string(f.snapshot.root().join("source")).unwrap(),
            "modified during check"
        );
        assert!(run.failure.is_some());
        assert_eq!(f.verdict(&run), Verdict::NotReady);
    }
    #[test]
    fn shell_recursion_validator_dependency_and_cycles_are_rejected() {
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.program = PathBuf::from("/bin/sh").canonicalize().unwrap();
            i.arguments = vec![Value::Literal("-c".into()), Value::Literal("exit 0".into())];
        });
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.set(0, |i| i.arguments.push(Value::Literal("validate".into())));
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks[0].dependencies = vec![Dependency::ValidationReceipt];
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        f.plan.checks[0].dependencies = vec![Dependency::Check(f.plan.checks[1].key.clone())];
        f.plan.checks[1].dependencies = vec![Dependency::Check(f.plan.checks[0].key.clone())];
        assert!(f.run().is_err());
    }
    #[test]
    fn invocation_arguments_are_literal_and_environment_is_explicit() {
        let mut f = Fixture::new();
        let literal = "; $(touch outside) && echo nope".to_string();
        f.set(0, |i| {
            i.arguments = vec![
                Value::Literal("environment".into()),
                Value::Literal(literal.clone()),
            ]
        });
        let run = f.run().unwrap();
        assert_eq!(run.diagnostics[0].stdout, format!("{literal}\n").as_bytes());
        assert_eq!(f.verdict(&run), Verdict::Ready);
        let mut f = Fixture::new();
        f.set(0, |i| {
            i.environment
                .insert("ACCESS_TOKEN".into(), Value::Literal("fixture".into()));
        });
        assert!(f.run().is_err());
    }
    #[test]
    fn required_order_is_topological_and_nonapplicability_is_proven() {
        let mut f = Fixture::new();
        f.plan.checks[0].dependencies = vec![Dependency::Check(f.plan.checks[1].key.clone())];
        let run = f.run().unwrap();
        assert_eq!(run.evidence.results[0].key, f.plan.checks[1].key);
        assert_eq!(f.verdict(&run), Verdict::Ready);
        let mut f = Fixture::new();
        f.plan.checks[0].requirement = Requirement::NotApplicable(NonApplicability::EventOutside {
            events: ["push".into()].into(),
        });
        f.invocations.remove(&f.plan.checks[0].key);
        let run = f.run().unwrap();
        assert!(matches!(
            run.evidence.results[0].observation,
            Observation::NotApplicable { .. }
        ));
        assert_eq!(f.verdict(&run), Verdict::Ready);
        f.facts.event = "push".into();
        assert!(f.run().is_err());
    }
    #[test]
    fn failed_material_capture_and_hash_budget_are_closed() {
        let f = Fixture::new();
        fs::write(f.repo.path().join("unknown"), "undeclared").unwrap();
        assert!(f.run().is_err());
        let f = Fixture::new();
        let mut limits = Fixture::limits();
        limits.max_hashed_bytes = 1;
        assert!(f.run_with(limits, &Cancellation::default()).is_err());
    }
    #[test]
    fn owned_process_group_is_stopped_on_exit_and_timeout() {
        let compiled = compiled_fixture();
        // First-use Mach-O loading can exceed the deliberately short child
        // cleanup deadline. Exercise that loading separately; the 100 ms
        // descendant deadline below remains unchanged.
        let warmup = Command::new(&compiled.path)
            .arg("ok")
            .bounded_output(Duration::from_secs(5))
            .unwrap();
        assert!(warmup.status.success());
        #[cfg(target_os = "linux")]
        assert_eq!(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) }, 0);
        for wait in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let descendant = dir.path().join("descendant");
            let recorded = dir.path().join("recorded");
            let mut command = Command::new(&compiled.path);
            command.arg("descendant").arg(&descendant).arg(&recorded);
            if wait {
                command.arg("wait");
            }
            let capture = command
                .bounded_capture(
                    ProcessLimits {
                        timeout: Duration::from_millis(100),
                        max_output_bytes: 65536,
                    },
                    &Cancellation::default(),
                )
                .unwrap();
            assert_eq!(
                capture.termination,
                if wait {
                    Termination::TimedOut
                } else {
                    Termination::Exited
                }
            );
            let pid = fs::read_to_string(&recorded).unwrap();
            #[cfg(target_os = "macos")]
            {
                // macOS adopts orphaned grandchildren rather than providing
                // Linux's subreaper API. Verify the recorded owned descendant
                // is gone or a non-running zombie, not merely the parent exit.
                let start = Instant::now();
                loop {
                    let status = Command::new("/bin/ps")
                        .args(["-o", "stat=", "-p", pid.trim()])
                        .bounded_output(Duration::from_secs(1))
                        .unwrap();
                    let state = String::from_utf8(status.stdout).unwrap();
                    if (status.status.code() == Some(1) && state.trim().is_empty())
                        || (status.status.success() && state.trim_start().starts_with('Z'))
                    {
                        break;
                    }
                    assert!(status.status.success(), "cannot inspect owned descendant");
                    assert!(
                        start.elapsed() < Duration::from_secs(2),
                        "descendant still running"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
            #[cfg(target_os = "linux")]
            {
                let start = Instant::now();
                loop {
                    let stat = fs::read_to_string(format!("/proc/{pid}/stat"));
                    if stat.is_err()
                        || stat
                            .as_ref()
                            .unwrap()
                            .split(')')
                            .nth(1)
                            .unwrap()
                            .trim_start()
                            .starts_with('Z')
                    {
                        break;
                    }
                    assert!(
                        start.elapsed() < Duration::from_secs(2),
                        "descendant still running"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                let pid: libc::pid_t = pid.parse().unwrap();
                let reaped = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };
                assert_eq!(reaped, pid);
            }
        }
    }

    #[test]
    fn unsupported_reviewed_platform_and_empty_artifact_fail_closed() {
        let mut f = Fixture::new();
        f.set(0, |i| i.platform = NativePlatform::WindowsX86_64);
        assert_eq!(f.run().unwrap_err().kind(), std::io::ErrorKind::Unsupported);
        let mut f = Fixture::new();
        f.set(2, |i| i.arguments[2] = Value::Literal(String::new()));
        let run = f.run().unwrap();
        assert_eq!(fixture_result(&run, 2).0, Conclusion::Failure);
        assert_eq!(f.verdict(&run), Verdict::NotReady);
        f.clean();
    }

    #[test]
    fn executable_script_and_renamed_self_are_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let mut f = Fixture::new();
        let script = f.scratch.path().join("innocent-name");
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        f.set(0, |i| i.program = script.clone());
        assert!(f.run().is_err());
        let mut f = Fixture::new();
        let copied = f.scratch.path().join("renamed-self");
        fs::copy(std::env::current_exe().unwrap(), &copied).unwrap();
        f.set(0, |i| i.program = copied);
        assert!(f.run().is_err());
    }

    #[test]
    fn changed_reviewed_toolchain_component_and_suite_output_budget_fail() {
        let mut f = Fixture::new();
        let component = f.scratch.path().join("toolchain-component");
        fs::write(&component, "reviewed").unwrap();
        f.set(0, |i| {
            i.toolchain_files.insert("sdk".into(), component.clone());
        });
        fs::write(&component, "changed after review").unwrap();
        assert!(f.run().is_err());
        let f = Fixture::new();
        let mut limits = Fixture::limits();
        limits.max_output_bytes = 8;
        let run = f.run_with(limits, &Cancellation::default()).unwrap();
        assert!(run.failure.is_some());
        assert_eq!(f.verdict(&run), Verdict::NotReady);
    }

    #[test]
    fn execution_snapshot_must_be_detached_and_match_source_material() {
        let f = Fixture::new();
        git(f.snapshot.root(), &["checkout", "-b", "wrong-branch"]);
        assert!(f.run().is_err());
        let f = Fixture::new();
        fs::write(f.snapshot.root().join("source"), "different candidate").unwrap();
        assert!(f.run().is_err());
        let f = Fixture::new();
        fs::write(f.repo.path().join("source"), "new dirty source").unwrap();
        assert!(f.run().is_err());
    }

    #[test]
    fn source_identity_is_recaptured_around_snapshot_execution() {
        let mut f = Fixture::new();
        let marker = f.scratch.path().join("started");
        let marker_arg = marker.to_string_lossy().into_owned();
        let source = f.repo.path().join("source");
        f.set(0, |i| {
            i.arguments = vec![
                Value::Literal("sleep".into()),
                Value::Literal("200".into()),
                Value::Literal(marker_arg),
            ]
        });
        let change = std::thread::spawn(move || {
            let start = Instant::now();
            while !marker.exists() && start.elapsed() < Duration::from_secs(120) {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(marker.exists());
            fs::write(source, "source changed during execution").unwrap();
        });
        let run = f.run().unwrap();
        change.join().unwrap();
        assert_ne!(
            run.evidence.before.tree_digest,
            run.evidence.after.tree_digest
        );
        assert_eq!(f.verdict(&run), Verdict::NotReady);
    }

    #[test]
    fn snapshot_hardlinks_cannot_mutate_original_source() {
        let f = Fixture::new();
        let path = f.snapshot.root().join("source");
        fs::remove_file(&path).unwrap();
        fs::hard_link(f.repo.path().join("source"), &path).unwrap();
        assert!(f.run().unwrap_err().to_string().contains("hardlinked"));
    }

    fn snapshot_limits() -> decapod::core::native_proof::snapshot::SnapshotLimits {
        decapod::core::native_proof::snapshot::SnapshotLimits {
            timeout: Duration::from_secs(30),
            max_snapshot_bytes: 64 * 1024 * 1024,
            max_files: 1000,
            max_io_bytes: 512 * 1024 * 1024,
            git_output_limit: 16 * 1024 * 1024,
        }
    }
    fn freeze(f: &Fixture) -> decapod::core::native_proof::snapshot::SnapshotReview {
        let identity = capture_current_identity(
            &f.inputs,
            &f.invocations,
            Fixture::limits(),
            &Cancellation::default(),
        )
        .unwrap();
        decapod::core::native_proof::snapshot::SnapshotReview::frozen(&identity, Default::default())
    }

    #[test]
    fn preparer_preserves_dirty_index_modes_deletions_and_declared_inputs_without_source_writes() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        use std::os::unix::fs::PermissionsExt;
        let mut f = Fixture::new();
        fs::write(f.repo.path().join("source"), "staged source").unwrap();
        git(f.repo.path(), &["add", "source"]);
        fs::write(f.repo.path().join("source"), "unstaged source").unwrap();
        fs::set_permissions(
            f.repo.path().join("source"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fs::write(f.repo.path().join("added"), "staged addition").unwrap();
        git(f.repo.path(), &["add", "added"]);
        fs::write(f.repo.path().join("added"), "unstaged addition").unwrap();
        git(f.repo.path(), &["rm", "workflow"]);
        f.inputs.workflows = ["config".into()].into();
        fs::write(f.repo.path().join("declared"), "reviewed untracked").unwrap();
        f.inputs.declared_untracked.insert("declared".into());
        let before_index = fs::read(f.repo.path().join(".git/index")).unwrap();
        let before_head = fs::read(f.repo.path().join(".git/HEAD")).unwrap();
        let review = freeze(&f);
        let prepared = prepare_snapshot(
            &f.inputs,
            &review,
            snapshot_limits(),
            f.scratch.path(),
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(prepared.root().join("source")).unwrap(),
            "unstaged source"
        );
        assert_eq!(
            fs::metadata(prepared.root().join("source"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111,
            0o111
        );
        assert_eq!(
            fs::read_to_string(prepared.root().join("added")).unwrap(),
            "unstaged addition"
        );
        assert!(!prepared.root().join("workflow").exists());
        assert_eq!(
            fs::read_to_string(prepared.root().join("declared")).unwrap(),
            "reviewed untracked"
        );
        assert_eq!(
            before_index,
            fs::read(f.repo.path().join(".git/index")).unwrap()
        );
        assert_eq!(
            before_head,
            fs::read(f.repo.path().join(".git/HEAD")).unwrap()
        );
        assert!(!prepared.root().join(".git/hooks").exists());
        assert!(
            !prepared
                .root()
                .join(".git/objects/info/alternates")
                .exists()
        );
        let run = run_native(
            &f.plan,
            &f.invocations,
            &f.inputs,
            &f.facts,
            Fixture::limits(),
            &prepared,
            &Cancellation::default(),
        )
        .unwrap();
        assert!(run.failure.is_none(), "{:?}", run.failure);
        assert_eq!(f.verdict(&run), Verdict::Ready);
        drop(prepared);
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
    }

    #[test]
    fn preparer_rejects_secret_undeclared_and_unsupported_inputs() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        let review = freeze(&f);
        fs::write(f.repo.path().join("unreviewed"), "new file").unwrap();
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .is_err()
        );
        let f = Fixture::new();
        fs::write(f.repo.path().join(".env"), "UNUSED_FIXTURE=true").unwrap();
        git(f.repo.path(), &["add", ".env"]);
        let review = freeze(&f);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("sensitive input path")
        );
        let f = Fixture::new();
        fs::write(
            f.repo.path().join("source"),
            ["-----BEGIN ", "PRIVATE KEY-----\n", "fixture-only\n"].concat(),
        )
        .unwrap();
        let review = freeze(&f);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("credential-like")
        );
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
        let f = Fixture::new();
        std::os::unix::fs::symlink("source", f.repo.path().join("link")).unwrap();
        git(f.repo.path(), &["add", "link"]);
        let review = freeze(&f);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .is_err()
        );
    }

    #[test]
    fn preparer_is_bounded_and_rejects_frozen_identity_drift_and_partial_history() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        let mut review = freeze(&f);
        review.head_revision = "a".repeat(40);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .is_err()
        );
        let f = Fixture::new();
        let review = freeze(&f);
        let mut limits = snapshot_limits();
        limits.max_snapshot_bytes = 1;
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                limits,
                f.scratch.path(),
                &Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
        let cancel = Cancellation::default();
        cancel.cancel();
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &cancel
            )
            .is_err()
        );
        git(
            f.repo.path(),
            &["config", "remote.fixture.promisor", "true"],
        );
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("promisor")
        );
    }

    #[test]
    fn source_index_changes_invalidate_identity_even_when_working_bytes_are_unchanged() {
        let f = Fixture::new();
        fs::write(f.repo.path().join("source"), "working bytes").unwrap();
        let before = capture_current_identity(
            &f.inputs,
            &f.invocations,
            Fixture::limits(),
            &Cancellation::default(),
        )
        .unwrap();
        git(f.repo.path(), &["add", "source"]);
        let after = capture_current_identity(
            &f.inputs,
            &f.invocations,
            Fixture::limits(),
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(before.tree_digest, after.tree_digest);
        assert_ne!(before.history_digest, after.history_digest);
    }
    #[test]
    fn preparer_never_runs_or_copies_source_hooks_and_rejects_submodules() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new();
        let marker = f.scratch.path().join("unexpected-hook");
        let hook = f.repo.path().join(".git/hooks/post-checkout");
        fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        git(
            f.repo.path(),
            &[
                "config",
                "core.hooksPath",
                f.repo.path().join(".git/hooks").to_str().unwrap(),
            ],
        );
        git(
            f.repo.path(),
            &["config", "core.fsmonitor", hook.to_str().unwrap()],
        );
        let review = freeze(&f);
        let snapshot = prepare_snapshot(
            &f.inputs,
            &review,
            snapshot_limits(),
            f.scratch.path(),
            &Cancellation::default(),
        )
        .unwrap();
        assert!(!marker.exists());
        assert!(!snapshot.root().join(".git/hooks").exists());
        drop(snapshot);
        let f = Fixture::new();
        let review = freeze(&f);
        git(
            f.repo.path(),
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{},module", review.head_revision),
            ],
        );
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("submodules")
        );
    }

    #[test]
    fn preparer_rejects_staged_only_credentials_and_explicit_sensitive_paths() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        fs::write(
            f.repo.path().join("source"),
            ["-----BEGIN ", "PRIVATE KEY-----\nfixture-only\n"].concat(),
        )
        .unwrap();
        git(f.repo.path(), &["add", "source"]);
        fs::write(f.repo.path().join("source"), "ordinary working bytes").unwrap();
        let review = freeze(&f);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("credential-like")
        );
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
        let f = Fixture::new();
        let mut review = freeze(&f);
        review.denied_paths.insert("source".into());
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("sensitive input path")
        );
    }

    #[test]
    fn preparer_copies_only_declared_ignored_inputs_and_cleans_on_budget_failure() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let mut f = Fixture::new();
        fs::write(f.repo.path().join(".gitignore"), "ignored/\n").unwrap();
        git(f.repo.path(), &["add", ".gitignore"]);
        fs::create_dir(f.repo.path().join("ignored")).unwrap();
        fs::write(f.repo.path().join("ignored/input"), "declared bytes").unwrap();
        fs::write(
            f.repo.path().join("ignored/not-an-input"),
            "excluded ambient bytes",
        )
        .unwrap();
        f.inputs.declared_untracked.insert("ignored/input".into());
        let review = freeze(&f);
        let snapshot = prepare_snapshot(
            &f.inputs,
            &review,
            snapshot_limits(),
            f.scratch.path(),
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(snapshot.root().join("ignored/input")).unwrap(),
            "declared bytes"
        );
        assert!(!snapshot.root().join("ignored/not-an-input").exists());
        drop(snapshot);
        let mut limits = snapshot_limits();
        limits.max_files = 1;
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                limits,
                f.scratch.path(),
                &Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
    }
    #[test]
    fn preparer_rejects_source_or_git_metadata_as_scratch_destination() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        let review = freeze(&f);
        for destination in [f.repo.path().to_path_buf(), f.repo.path().join(".git")] {
            assert!(
                prepare_snapshot(
                    &f.inputs,
                    &review,
                    snapshot_limits(),
                    &destination,
                    &Cancellation::default()
                )
                .unwrap_err()
                .to_string()
                .contains("scratch must not be inside")
            );
        }
    }
    #[test]
    fn preparer_rejects_unchanged_index_secret_hidden_by_dirty_worktree() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        fs::write(
            f.repo.path().join("source"),
            ["-----BEGIN ", "PRIVATE KEY-----\nfixture-only\n"].concat(),
        )
        .unwrap();
        git(f.repo.path(), &["add", "source"]);
        git(f.repo.path(), &["commit", "-m", "dummy credential fixture"]);
        fs::write(f.repo.path().join("source"), "ordinary dirty working bytes").unwrap();
        let review = freeze(&f);
        assert!(
            prepare_snapshot(
                &f.inputs,
                &review,
                snapshot_limits(),
                f.scratch.path(),
                &Cancellation::default()
            )
            .unwrap_err()
            .to_string()
            .contains("credential-like")
        );
        assert_eq!(fs::read_dir(f.scratch.path()).unwrap().count(), 0);
    }
    #[test]
    fn preparer_preserves_annotated_and_lightweight_tags_and_tags_bind_identity() {
        use decapod::core::native_proof::snapshot::prepare_snapshot;
        let f = Fixture::new();
        let before = capture_current_identity(
            &f.inputs,
            &f.invocations,
            Fixture::limits(),
            &Cancellation::default(),
        )
        .unwrap();
        git(f.repo.path(), &["tag", "lightweight"]);
        git(
            f.repo.path(),
            &["tag", "-a", "v0.1.0", "-m", "fixture release tag"],
        );
        let after = capture_current_identity(
            &f.inputs,
            &f.invocations,
            Fixture::limits(),
            &Cancellation::default(),
        )
        .unwrap();
        assert_eq!(before.tree_digest, after.tree_digest);
        assert_ne!(before.history_digest, after.history_digest);
        let review = freeze(&f);
        let snapshot = prepare_snapshot(
            &f.inputs,
            &review,
            snapshot_limits(),
            f.scratch.path(),
            &Cancellation::default(),
        )
        .unwrap();
        let read = |root: &std::path::Path| {
            Command::new("/usr/bin/git")
                .current_dir(root)
                .args([
                    "for-each-ref",
                    "--format=%(refname) %(objectname)",
                    "refs/tags/",
                ])
                .bounded_output(Duration::from_secs(5))
                .unwrap()
                .stdout
        };
        assert_eq!(read(f.repo.path()), read(snapshot.root()));
        let run = run_native(
            &f.plan,
            &f.invocations,
            &f.inputs,
            &f.facts,
            Fixture::limits(),
            &snapshot,
            &Cancellation::default(),
        )
        .unwrap();
        assert!(run.failure.is_none(), "{:?}", run.failure);
    }
}

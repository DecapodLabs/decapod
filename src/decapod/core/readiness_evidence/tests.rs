use super::*;

fn hash(label: &str) -> String {
    digest(&label).unwrap()
}

fn fixture() -> (
    ExpectedPlan,
    MaterialIdentity,
    ApplicabilityFacts,
    SuiteEvidence,
) {
    let identity = MaterialIdentity {
        repository: "github.com/DecapodLabs/decapod".into(),
        work_branch: "work/readiness".into(),
        head_revision: "a".repeat(40),
        base_branch: "master".into(),
        accepted_base_revision: "b".repeat(40),
        history_digest: hash("history"),
        tree_digest: hash("tree"),
        configuration_digest: hash("config"),
        workflow_digest: hash("workflow"),
        toolchain_digest: hash("tools"),
        candidate_digest: hash("candidate"),
    };
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: ["src/lib.rs".into()].into(),
    };
    let checks = [
        CheckKind::SourceCi,
        CheckKind::DerivedRelease,
        CheckKind::CandidateBuild,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| ExpectedCheck {
        key: CheckKey {
            provider: Provider::GithubActions,
            workflow: ".github/workflows/ci.yml".into(),
            job: format!("check-{index}"),
            matrix: [
                ("os".into(), "ubuntu-24.04".into()),
                ("rust".into(), "1.96.1".into()),
            ]
            .into(),
            profile: "linux-x86_64-debug".into(),
        },
        kind,
        definition_digest: hash(&format!("definition-{index}")),
        invocation_digest: hash(&format!("invocation-{index}")),
        toolchain_digest: hash("linux toolchain"),
        executor: Provider::Native,
        dependencies: vec![],
        requirement: Requirement::Execute,
    })
    .collect::<Vec<_>>();
    let plan = ExpectedPlan {
        schema_version: SCHEMA_VERSION.into(),
        policy_digest: hash("policy"),
        source_inventory_digest: hash("inventory"),
        buildkite_enabled: false,
        checks,
    };
    let evidence = SuiteEvidence {
        schema_version: SCHEMA_VERSION.into(),
        expected_plan_digest: plan.digest().unwrap(),
        before: identity.clone(),
        after: identity.clone(),
        facts: facts.clone(),
        results: plan
            .checks
            .iter()
            .map(|check| CheckResult {
                key: check.key.clone(),
                definition_digest: check.definition_digest.clone(),
                invocation_digest: check.invocation_digest.clone(),
                toolchain_digest: check.toolchain_digest.clone(),
                executor: check.executor,
                identity: identity.clone(),
                observation: Observation::Executed {
                    execution_id: format!("native-run-{}-attempt-1", check.key.job),
                    conclusion: Conclusion::Success,
                    exit_code: Some(0),
                    output_digest: hash("output"),
                    artifact_digest: (check.kind == CheckKind::CandidateBuild)
                        .then(|| hash("built executable")),
                },
            })
            .collect(),
    };
    (plan, identity, facts, evidence)
}

fn assert_not_ready(
    plan: &ExpectedPlan,
    identity: &MaterialIdentity,
    facts: &ApplicabilityFacts,
    evidence: &SuiteEvidence,
) -> Verification {
    let verification = verify(plan, identity, facts, evidence);
    assert_eq!(verification.verdict, Verdict::NotReady, "{verification:?}");
    assert!(!verification.failures.is_empty());
    verification
}

#[test]
fn complete_executed_suite_passes_and_roundtrips() {
    let (plan, identity, facts, evidence) = fixture();
    let json = serde_json::to_string(&evidence).unwrap();
    let roundtrip = serde_json::from_str(&json).unwrap();
    assert_eq!(evidence, roundtrip);
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
    assert_eq!(
        serde_json::to_string(&Verdict::NotReady).unwrap(),
        "\"not_ready\""
    );
}

#[test]
fn empty_plan_and_empty_evidence_never_pass() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    evidence.results.clear();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    plan.checks.clear();
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::EmptyPlan)
    );
}

#[test]
fn missing_every_required_row_is_blocking() {
    for index in 0..3 {
        let (plan, identity, facts, mut evidence) = fixture();
        let removed = evidence.results.remove(index);
        assert!(
            assert_not_ready(&plan, &identity, &facts, &evidence)
                .failures
                .contains(&Failure::Missing(removed.key))
        );
    }
}

#[test]
fn duplicate_expected_and_observed_rows_fail_without_deduplication() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    let key = plan.checks[0].key.clone();
    plan.checks.push(plan.checks[0].clone());
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::DuplicateExpected(key.clone()))
    );
    plan.checks.pop();
    evidence.expected_plan_digest = plan.digest().unwrap();
    evidence.results.push(evidence.results[0].clone());
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::DuplicateResult(key))
    );
}

#[test]
fn extra_provider_workflow_job_matrix_or_profile_cannot_replace_expected_row() {
    let mutations: [fn(&mut CheckKey); 5] = [
        |k| k.provider = Provider::Buildkite,
        |k| k.workflow = "other.yml".into(),
        |k| k.job = "other".into(),
        |k| {
            k.matrix.insert("os".into(), "macos".into());
        },
        |k| k.profile = "release".into(),
    ];
    for mutate in mutations {
        let (plan, identity, facts, mut evidence) = fixture();
        mutate(&mut evidence.results[0].key);
        let failure = assert_not_ready(&plan, &identity, &facts, &evidence);
        assert!(
            failure
                .failures
                .iter()
                .any(|f| matches!(f, Failure::Missing(_)))
        );
        assert!(
            failure
                .failures
                .iter()
                .any(|f| matches!(f, Failure::Extra(_)))
        );
    }
}

#[test]
fn extra_success_does_not_expand_expected_coverage() {
    let (plan, identity, facts, mut evidence) = fixture();
    let mut extra = evidence.results[0].clone();
    extra.key.job = "bonus".into();
    evidence.results.push(extra);
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn all_identity_fields_are_current_and_per_row_bound() {
    let mutations: [fn(&mut MaterialIdentity); 11] = [
        |i| i.repository.push('x'),
        |i| i.work_branch.push('x'),
        |i| i.head_revision = "c".repeat(40),
        |i| i.base_branch.push('x'),
        |i| i.accepted_base_revision = "c".repeat(40),
        |i| i.history_digest = hash("new history"),
        |i| i.tree_digest = hash("new tree"),
        |i| i.configuration_digest = hash("new config"),
        |i| i.workflow_digest = hash("new workflow"),
        |i| i.toolchain_digest = hash("new toolchain"),
        |i| i.candidate_digest = hash("new candidate"),
    ];
    for mutate in mutations {
        let (plan, mut current, facts, evidence) = fixture();
        mutate(&mut current);
        assert!(
            assert_not_ready(&plan, &current, &facts, &evidence)
                .failures
                .contains(&Failure::StaleIdentity)
        );
        let (plan, current, facts, mut evidence) = fixture();
        mutate(&mut evidence.results[0].identity);
        assert!(
            assert_not_ready(&plan, &current, &facts, &evidence)
                .failures
                .iter()
                .any(|f| matches!(f, Failure::InvalidResult(_)))
        );
    }
}

#[test]
fn policy_inventory_and_definition_cannot_change_after_run() {
    let mutations: [fn(&mut ExpectedPlan); 5] = [
        |p| p.policy_digest = hash("new policy"),
        |p| p.source_inventory_digest = hash("new inventory"),
        |p| p.checks[0].definition_digest = hash("new definition"),
        |p| p.checks[0].invocation_digest = hash("new invocation"),
        |p| p.checks[0].toolchain_digest = hash("new tools"),
    ];
    for mutate in mutations {
        let (mut plan, identity, facts, evidence) = fixture();
        mutate(&mut plan);
        assert!(
            assert_not_ready(&plan, &identity, &facts, &evidence)
                .failures
                .contains(&Failure::PlanMismatch)
        );
    }
}

#[test]
fn self_asserted_shrunken_receipt_plan_is_not_authority() {
    let (plan, identity, facts, mut evidence) = fixture();
    let mut invented = plan.clone();
    invented.checks.pop();
    evidence.expected_plan_digest = invented.digest().unwrap();
    evidence.results.pop();
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::PlanMismatch)
    );
}

#[test]
fn running_pending_failed_cancelled_timeout_and_skipped_fail_even_with_zero_exit() {
    for state in [
        Conclusion::Pending,
        Conclusion::Running,
        Conclusion::Failure,
        Conclusion::Skipped,
        Conclusion::Cancelled,
        Conclusion::TimedOut,
    ] {
        let (plan, identity, facts, mut evidence) = fixture();
        if let Observation::Executed { conclusion, .. } = &mut evidence.results[0].observation {
            *conclusion = state;
        }
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn success_requires_real_exit_execution_output_and_candidate_artifact() {
    let mutations: [fn(&mut SuiteEvidence); 6] = [
        |e| {
            if let Observation::Executed { exit_code, .. } = &mut e.results[0].observation {
                *exit_code = None;
            }
        },
        |e| {
            if let Observation::Executed { exit_code, .. } = &mut e.results[0].observation {
                *exit_code = Some(1);
            }
        },
        |e| {
            if let Observation::Executed { execution_id, .. } = &mut e.results[0].observation {
                execution_id.clear();
            }
        },
        |e| {
            if let Observation::Executed { output_digest, .. } = &mut e.results[0].observation {
                output_digest.clear();
            }
        },
        |e| {
            if let Observation::Executed {
                artifact_digest, ..
            } = &mut e.results[2].observation
            {
                *artifact_digest = None;
            }
        },
        |e| {
            if let Observation::Executed {
                artifact_digest, ..
            } = &mut e.results[2].observation
            {
                *artifact_digest = Some("green".into());
            }
        },
    ];
    for mutate in mutations {
        let (plan, identity, facts, mut evidence) = fixture();
        mutate(&mut evidence);
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn worktree_changed_during_execution_is_blocking_even_if_all_results_pass() {
    let (plan, mut current, facts, mut evidence) = fixture();
    current.tree_digest = hash("after edit");
    evidence.after = current.clone();
    assert!(
        assert_not_ready(&plan, &current, &facts, &evidence)
            .failures
            .contains(&Failure::WorktreeChanged)
    );
}

#[test]
fn validator_receipt_and_self_job_dependencies_are_forbidden() {
    for dependency in [Dependency::ValidationReceipt, Dependency::Validator] {
        let (mut plan, identity, facts, mut evidence) = fixture();
        plan.checks[0].dependencies.push(dependency);
        evidence.expected_plan_digest = plan.digest().unwrap();
        assert!(
            assert_not_ready(&plan, &identity, &facts, &evidence)
                .failures
                .iter()
                .any(|f| matches!(f, Failure::ForbiddenDependency(_)))
        );
    }
    let (mut plan, identity, facts, mut evidence) = fixture();
    plan.checks[0].kind = CheckKind::Validator;
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn self_missing_duplicate_and_cyclic_dependencies_fail() {
    for case in 0..4 {
        let (mut plan, identity, facts, mut evidence) = fixture();
        let a = plan.checks[0].key.clone();
        let b = plan.checks[1].key.clone();
        match case {
            0 => plan.checks[0].dependencies = vec![Dependency::Check(a)],
            1 => {
                let mut missing = a;
                missing.job = "absent".into();
                plan.checks[0].dependencies = vec![Dependency::Check(missing)];
            }
            2 => {
                plan.checks[0].dependencies =
                    vec![Dependency::Check(b.clone()), Dependency::Check(b)]
            }
            _ => {
                plan.checks[0].dependencies = vec![Dependency::Check(b)];
                plan.checks[1].dependencies = vec![Dependency::Check(a)];
            }
        }
        evidence.expected_plan_digest = plan.digest().unwrap();
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

fn make_not_applicable(
    plan: &mut ExpectedPlan,
    evidence: &mut SuiteEvidence,
    index: usize,
    predicate: NonApplicability,
) {
    evidence.results[index].observation = Observation::NotApplicable {
        predicate_digest: digest(&predicate).unwrap(),
    };
    plan.checks[index].requirement = Requirement::NotApplicable(predicate);
    evidence.expected_plan_digest = plan.digest().unwrap();
}

#[test]
fn independently_proven_nonapplicability_passes_with_fresh_candidate_build() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    make_not_applicable(
        &mut plan,
        &mut evidence,
        0,
        NonApplicability::EventOutside {
            events: ["schedule".into()].into(),
        },
    );
    make_not_applicable(
        &mut plan,
        &mut evidence,
        1,
        NonApplicability::TargetOutside {
            branches: ["release".into()].into(),
        },
    );
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
}

#[test]
fn all_not_applicable_and_candidate_build_exemption_fail() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    for index in 0..3 {
        make_not_applicable(
            &mut plan,
            &mut evidence,
            index,
            NonApplicability::EventOutside {
                events: ["schedule".into()].into(),
            },
        );
    }
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn nonapplicability_needs_true_predicate_exact_digest_and_independent_facts() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    make_not_applicable(
        &mut plan,
        &mut evidence,
        0,
        NonApplicability::EventOutside {
            events: ["pull_request".into()].into(),
        },
    );
    assert_not_ready(&plan, &identity, &facts, &evidence);
    evidence.facts.event = "schedule".into();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    evidence.facts = facts.clone();
    make_not_applicable(
        &mut plan,
        &mut evidence,
        0,
        NonApplicability::EventOutside {
            events: ["schedule".into()].into(),
        },
    );
    evidence.results[0].observation = Observation::NotApplicable {
        predicate_digest: hash("invented predicate"),
    };
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn successful_skipped_wrapper_is_not_proven_nonapplicability() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    plan.checks[0].requirement = Requirement::NotApplicable(NonApplicability::EventOutside {
        events: ["schedule".into()].into(),
    });
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    let (plan, identity, facts, mut evidence) = fixture();
    evidence.results[0].observation = Observation::NotApplicable {
        predicate_digest: hash("skip"),
    };
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn path_predicates_have_component_boundaries_and_reject_empty_scope() {
    let (mut plan, identity, mut facts, mut evidence) = fixture();
    for (paths, expected) in [
        (vec!["src2/a.rs"], Verdict::Ready),
        (vec!["src/a.rs"], Verdict::NotReady),
        (vec!["Cargo.toml"], Verdict::NotReady),
    ] {
        facts.changed_paths = paths.into_iter().map(String::from).collect();
        evidence.facts = facts.clone();
        make_not_applicable(
            &mut plan,
            &mut evidence,
            0,
            NonApplicability::PathsUntouched {
                exact_paths: ["Cargo.toml".into()].into(),
                directories: ["src".into()].into(),
            },
        );
        assert_eq!(
            verify(&plan, &identity, &facts, &evidence).verdict,
            expected
        );
    }
    make_not_applicable(
        &mut plan,
        &mut evidence,
        0,
        NonApplicability::PathsUntouched {
            exact_paths: BTreeSet::new(),
            directories: BTreeSet::new(),
        },
    );
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn buildkite_false_preserves_obligation_and_requires_native_equivalent() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    plan.checks[0].key.provider = Provider::Buildkite;
    evidence.results[0].key.provider = Provider::Buildkite;
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
    for backend in [Provider::Buildkite, Provider::GithubActions] {
        plan.checks[0].executor = backend;
        evidence.results[0].executor = backend;
        evidence.expected_plan_digest = plan.digest().unwrap();
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
    plan.buildkite_enabled = true;
    plan.checks[0].executor = Provider::Buildkite;
    evidence.results[0].executor = Provider::Buildkite;
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
}

#[test]
fn unimplemented_version_reuse_is_always_rejected() {
    let (plan, identity, facts, mut evidence) = fixture();
    evidence.results[0].observation = Observation::Reused {
        evidence_digest: hash("green base"),
    };
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .iter()
            .any(|f| matches!(f, Failure::ReuseUnsupported(_)))
    );
}

#[test]
fn schemas_unknown_fields_and_malformed_identities_fail_closed() {
    let (mut plan, mut identity, facts, mut evidence) = fixture();
    evidence.schema_version = "2.0.0".into();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    evidence.schema_version = SCHEMA_VERSION.into();
    plan.schema_version = "2.0.0".into();
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    plan.schema_version = SCHEMA_VERSION.into();
    evidence.expected_plan_digest = plan.digest().unwrap();
    identity.head_revision = "HEAD".into();
    evidence.before = identity.clone();
    evidence.after = identity.clone();
    for result in &mut evidence.results {
        result.identity = identity.clone();
    }
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::InvalidIdentity)
    );
    let mut json = serde_json::to_value(&evidence).unwrap();
    json["all_passed"] = true.into();
    assert!(serde_json::from_value::<SuiteEvidence>(json).is_err());
}

#[test]
fn order_is_irrelevant_and_diagnostics_are_deterministic() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    let dependency = plan.checks[1].key.clone();
    plan.checks[0]
        .dependencies
        .push(Dependency::Check(dependency));
    evidence.expected_plan_digest = plan.digest().unwrap();
    let first = verify(&plan, &identity, &facts, &evidence);
    plan.checks.reverse();
    evidence.results.reverse();
    assert_eq!(plan.digest().unwrap(), evidence.expected_plan_digest);
    assert_eq!(verify(&plan, &identity, &facts, &evidence), first);
    assert_eq!(first.verdict, Verdict::Ready);
}

#[cfg(unix)]
mod material_tests {
    use super::*;
    use std::{
        fs,
        os::unix::{fs::PermissionsExt, fs::symlink},
        path::Path,
        process::Command,
    };

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fn write(root: &Path, path: &str, content: &str) {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
    fn repository() -> tempfile::TempDir {
        let temp = tempfile::Builder::new()
            .prefix("material-fixture-")
            .tempdir()
            .unwrap();
        git(temp.path(), &["init", "-b", "work"]);
        for (path, content) in [
            ("src/lib.rs", "fn main() {}"),
            (".gitignore", "target/\nignored-input\n"),
            (".github/workflows/ci.yml", "jobs: []"),
            (".decapod/config.toml", "[proof]"),
            (".config/build/toolchain.toml", "version = 1"),
            (".decapod/governance.json", "{}"),
        ] {
            write(temp.path(), path, content);
        }
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-m", "fixture"]);
        temp
    }
    fn capture(root: &Path) -> material::WorkingMaterial {
        material::capture(root, &BTreeSet::new()).unwrap()
    }

    #[test]
    fn actual_unstaged_bytes_modes_and_deletions_are_bound_without_index_writes() {
        let temp = repository();
        let root = temp.path();
        let before = capture(root);
        let index = fs::read(root.join(".git/index")).unwrap();
        write(root, "src/lib.rs", "fn changed() {}");
        let changed = capture(root);
        assert_ne!(before.digest().unwrap(), changed.digest().unwrap());
        assert_eq!(index, fs::read(root.join(".git/index")).unwrap());
        fs::set_permissions(root.join("src/lib.rs"), fs::Permissions::from_mode(0o755)).unwrap();
        let executable = capture(root);
        assert_ne!(changed, executable);
        assert_eq!(executable.files["src/lib.rs"].mode, "100755");
        fs::remove_file(root.join("src/lib.rs")).unwrap();
        let deleted = capture(root);
        assert_eq!(deleted.files["src/lib.rs"].mode, "000000");
        assert!(deleted.files["src/lib.rs"].content_digest.is_none());
        assert_eq!(index, fs::read(root.join(".git/index")).unwrap());
    }

    #[test]
    fn staged_deletion_cannot_hide_existing_working_bytes() {
        let temp = repository();
        let root = temp.path();
        let before = capture(root);
        git(root, &["rm", "--cached", "src/lib.rs"]);
        assert_eq!(before, capture(root));
        fs::remove_file(root.join("src/lib.rs")).unwrap();
        assert_ne!(before, capture(root));
    }

    #[test]
    fn declared_untracked_and_ignored_inputs_are_bound_and_missing_inputs_fail() {
        let temp = repository();
        let root = temp.path();
        write(root, "new.rs", "new");
        assert!(material::capture(root, &BTreeSet::new()).is_err());
        let mut declared: BTreeSet<String> = ["new.rs".into()].into();
        let before = material::capture(root, &declared).unwrap();
        write(root, "new.rs", "changed");
        assert_ne!(before, material::capture(root, &declared).unwrap());
        write(root, "ignored-input", "input");
        declared.insert("ignored-input".into());
        let before = material::capture(root, &declared).unwrap();
        write(root, "ignored-input", "new input");
        assert_ne!(before, material::capture(root, &declared).unwrap());
        fs::remove_file(root.join("new.rs")).unwrap();
        assert!(material::capture(root, &declared).is_err());
    }

    #[test]
    fn tracked_configuration_workflows_tools_bind_but_receipt_runtime_does_not() {
        let temp = repository();
        let root = temp.path();
        let before = capture(root);
        write(root, ".decapod/governance.json", "{\"validation\":\"new\"}");
        write(root, ".decapod/data/readiness.json", "runtime evidence");
        assert_eq!(before, capture(root));
        for path in [
            ".decapod/config.toml",
            ".github/workflows/ci.yml",
            ".config/build/toolchain.toml",
        ] {
            let before = capture(root);
            write(root, path, "changed");
            assert_ne!(before, capture(root));
        }
        assert!(!material::is_bookkeeping(
            ".decapod/governance-other/src.rs"
        ));
        assert!(!material::is_bookkeeping(
            ".decapod/managed/specs/INTENT.md"
        ));
    }

    #[test]
    fn symlink_targets_bind_and_symlink_ancestors_cannot_escape() {
        let temp = repository();
        let root = temp.path();
        symlink("src/lib.rs", root.join("link")).unwrap();
        let declared: BTreeSet<String> = ["link".into()].into();
        let before = material::capture(root, &declared).unwrap();
        assert_eq!(before.files["link"].mode, "120000");
        fs::remove_file(root.join("link")).unwrap();
        symlink(".gitignore", root.join("link")).unwrap();
        assert_ne!(before, material::capture(root, &declared).unwrap());
        fs::remove_file(root.join("link")).unwrap();
        symlink("src", root.join("link")).unwrap();
        assert!(material::capture(root, &["link".into(), "link/lib.rs".into()].into()).is_err());
    }

    #[test]
    fn declarations_cannot_escape_or_exempt_tracked_inputs() {
        let temp = repository();
        for path in [
            "../secret",
            "/absolute",
            "a/../b",
            ".git/config",
            ".decapod/governance.json",
            "src/lib.rs",
            "a\\b",
        ] {
            assert!(
                material::capture(temp.path(), &[path.into()].into()).is_err(),
                "{path}"
            );
        }
    }

    #[test]
    fn index_conflicts_fail_closed() {
        let temp = repository();
        let root = temp.path();
        git(root, &["checkout", "-b", "other"]);
        write(root, "src/lib.rs", "other");
        git(root, &["commit", "-am", "other"]);
        git(root, &["checkout", "work"]);
        write(root, "src/lib.rs", "work");
        git(root, &["commit", "-am", "work"]);
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "merge",
                "other",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(material::capture(root, &BTreeSet::new()).is_err());
    }
    #[test]
    fn escaping_and_bookkeeping_symlink_targets_fail_closed() {
        let temp = repository();
        let root = temp.path();
        for target in [
            "../outside",
            "/external",
            ".git/config",
            ".decapod/data/runtime-input",
        ] {
            symlink(target, root.join("link")).unwrap();
            assert!(
                material::capture(root, &["link".into()].into()).is_err(),
                "{target}"
            );
            fs::remove_file(root.join("link")).unwrap();
        }
    }
}

fn github_fixture(
    index: usize,
) -> (
    ExpectedPlan,
    MaterialIdentity,
    ApplicabilityFacts,
    SuiteEvidence,
) {
    let (mut plan, identity, facts, mut evidence) = fixture();
    let check = &mut plan.checks[index];
    check.executor = Provider::GithubActions;
    let contract = GithubExecutionContract {
        schema_version: GITHUB_EXECUTION_SCHEMA_VERSION.into(),
        repository: identity.repository.clone(),
        workflow: check.key.workflow.clone(),
        job: check.key.job.clone(),
        provider_job_name: format!("{} (ubuntu-24.04)", check.key.job),
        matrix: check.key.matrix.clone(),
        profile: check.key.profile.clone(),
        source_definition_digest: check.definition_digest.clone(),
        source_head: identity.head_revision.clone(),
        accepted_base_revision: identity.accepted_base_revision.clone(),
        transport_revision: "c".repeat(40),
        source_tree: "d".repeat(40),
        required_steps: vec![
            GithubRequiredStep {
                name: "Verify immutable checkout".into(),
                definition_digest: hash("checkout action"),
                failure_mode: GithubFailureMode::RequiredTerminalSuccess,
            },
            GithubRequiredStep {
                name: "Run substantive check".into(),
                definition_digest: hash("actual command"),
                failure_mode: GithubFailureMode::RequiredTerminalSuccess,
            },
        ],
    };
    check.invocation_digest = contract.digest().unwrap();
    evidence.results[index].executor = Provider::GithubActions;
    evidence.results[index].invocation_digest = check.invocation_digest.clone();
    let attestation = GithubExecutionAttestation {
        observed_run_head: contract.transport_revision.clone(),
        observed_job_name: contract.provider_job_name.clone(),
        steps: contract
            .required_steps
            .iter()
            .map(|s| GithubStepObservation {
                name: s.name.clone(),
                conclusion: Conclusion::Success,
            })
            .collect(),
        contract,
        run_id: 17,
        run_attempt: 2,
        job_id: 99,
        run_conclusion: Conclusion::Success,
        job_conclusion: Conclusion::Success,
        response_digest: hash("actual API response bundle"),
        log_digest: hash("actual exact job log bytes"),
        artifact_digest: (check.kind == CheckKind::CandidateBuild)
            .then(|| hash("actual artifact bytes")),
    };
    evidence.results[index].observation = Observation::GithubActions {
        attestation: Box::new(attestation),
    };
    evidence.expected_plan_digest = plan.digest().unwrap();
    (plan, identity, facts, evidence)
}
fn attestation(evidence: &mut SuiteEvidence, index: usize) -> &mut GithubExecutionAttestation {
    let Observation::GithubActions { attestation } = &mut evidence.results[index].observation
    else {
        panic!("github fixture required");
    };
    attestation
}
fn rebind_contract(plan: &mut ExpectedPlan, evidence: &mut SuiteEvidence, index: usize) {
    let value = attestation(evidence, index).contract.digest().unwrap();
    plan.checks[index].invocation_digest = value.clone();
    evidence.results[index].invocation_digest = value;
    evidence.expected_plan_digest = plan.digest().unwrap();
}

#[test]
fn explicit_github_attestation_is_success_without_invented_numeric_exit() {
    for index in [0, 2] {
        let (plan, identity, facts, evidence) = github_fixture(index);
        assert_eq!(
            verify(&plan, &identity, &facts, &evidence).verdict,
            Verdict::Ready
        );
        let json = serde_json::to_value(&evidence).unwrap();
        assert!(
            json["results"][index]["observation"]
                .get("exit_code")
                .is_none()
        );
        assert_eq!(
            serde_json::from_value::<SuiteEvidence>(json).unwrap(),
            evidence
        );
    }
}

#[test]
fn numeric_observations_still_require_measured_zero_for_every_executor() {
    for executor in [
        Provider::Native,
        Provider::Buildkite,
        Provider::GithubActions,
    ] {
        let (mut plan, identity, facts, mut evidence) = fixture();
        plan.buildkite_enabled = true;
        plan.checks[0].executor = executor;
        evidence.results[0].executor = executor;
        evidence.expected_plan_digest = plan.digest().unwrap();
        let Observation::Executed { exit_code, .. } = &mut evidence.results[0].observation else {
            unreachable!()
        };
        *exit_code = None;
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_attestation_cannot_satisfy_native_or_buildkite_executor() {
    for executor in [Provider::Native, Provider::Buildkite] {
        let (mut plan, identity, facts, mut evidence) = github_fixture(0);
        plan.buildkite_enabled = true;
        plan.checks[0].executor = executor;
        evidence.results[0].executor = executor;
        evidence.expected_plan_digest = plan.digest().unwrap();
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_missing_extra_duplicate_or_reordered_substantive_steps_fail() {
    for case in 0..4 {
        let (plan, identity, facts, mut evidence) = github_fixture(0);
        let a = attestation(&mut evidence, 0);
        match case {
            0 => {
                a.steps.pop();
            }
            1 => a.steps.push(GithubStepObservation {
                name: "other".into(),
                conclusion: Conclusion::Success,
            }),
            2 => a.steps[1] = a.steps[0].clone(),
            _ => a.steps.reverse(),
        }
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_empty_duplicate_or_masked_expected_contract_fails_even_if_rebound() {
    for case in 0..5 {
        let (mut plan, identity, facts, mut evidence) = github_fixture(0);
        let a = attestation(&mut evidence, 0);
        match case {
            0 => {
                a.contract.required_steps.clear();
                a.steps.clear();
            }
            1 => {
                a.contract.required_steps[1] = a.contract.required_steps[0].clone();
                a.steps[1] = a.steps[0].clone();
            }
            2 => a.contract.required_steps[0].failure_mode = GithubFailureMode::ContinueOnError,
            3 => a.contract.required_steps[0].failure_mode = GithubFailureMode::Unknown,
            _ => a.contract.required_steps[0].definition_digest.clear(),
        }
        rebind_contract(&mut plan, &mut evidence, 0);
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_contract_cannot_shrink_to_observed_green_steps() {
    let (plan, identity, facts, mut evidence) = github_fixture(0);
    let a = attestation(&mut evidence, 0);
    a.contract.required_steps.pop();
    a.steps.pop();
    // Expected invocation digest stays independently bound to the complete steps.
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn github_running_skipped_failed_cancelled_timeout_never_mean_executed_success() {
    for state in [
        Conclusion::Pending,
        Conclusion::Running,
        Conclusion::Failure,
        Conclusion::Skipped,
        Conclusion::Cancelled,
        Conclusion::TimedOut,
    ] {
        for subject in 0..3 {
            let (plan, identity, facts, mut evidence) = github_fixture(0);
            let a = attestation(&mut evidence, 0);
            match subject {
                0 => a.run_conclusion = state,
                1 => a.job_conclusion = state,
                _ => a.steps[0].conclusion = state,
            }
            assert_not_ready(&plan, &identity, &facts, &evidence);
        }
    }
}

#[test]
fn github_source_workflow_job_matrix_profile_and_definition_bind_independently() {
    let mutations: [fn(&mut GithubExecutionContract); 8] = [
        |c| c.repository = "other/repo".into(),
        |c| c.workflow = "other.yml".into(),
        |c| c.job = "other".into(),
        |c| c.matrix.clear(),
        |c| c.profile = "other".into(),
        |c| c.source_definition_digest = hash("other"),
        |c| c.source_head = "e".repeat(40),
        |c| c.accepted_base_revision = "f".repeat(40),
    ];
    for mutate in mutations {
        let (mut plan, identity, facts, mut evidence) = github_fixture(0);
        mutate(&mut attestation(&mut evidence, 0).contract);
        rebind_contract(&mut plan, &mut evidence, 0);
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_exact_run_job_transport_and_measured_custody_are_required() {
    let mutations: [fn(&mut GithubExecutionAttestation); 9] = [
        |a| a.run_id = 0,
        |a| a.run_attempt = 0,
        |a| a.job_id = 0,
        |a| a.observed_run_head = "e".repeat(40),
        |a| a.observed_job_name = "different matrix".into(),
        |a| a.response_digest.clear(),
        |a| a.log_digest.clear(),
        |a| a.contract.transport_revision = "0".repeat(40),
        |a| a.contract.source_tree = "0".repeat(40),
    ];
    for mutate in mutations {
        let (mut plan, identity, facts, mut evidence) = github_fixture(0);
        mutate(attestation(&mut evidence, 0));
        rebind_contract(&mut plan, &mut evidence, 0);
        assert_not_ready(&plan, &identity, &facts, &evidence);
    }
}

#[test]
fn github_candidate_build_requires_separate_actual_artifact_evidence() {
    let (plan, identity, facts, mut evidence) = github_fixture(2);
    attestation(&mut evidence, 2).artifact_digest = None;
    assert_not_ready(&plan, &identity, &facts, &evidence);
    attestation(&mut evidence, 2).artifact_digest = Some("success".into());
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn github_schema_does_not_reinterpret_old_or_unknown_receipts() {
    let (plan, identity, facts, mut evidence) = github_fixture(0);
    evidence.schema_version = "1.0.0".into();
    assert_not_ready(&plan, &identity, &facts, &evidence);
    evidence.schema_version = SCHEMA_VERSION.into();
    attestation(&mut evidence, 0).contract.schema_version = "unknown".into();
    assert_not_ready(&plan, &identity, &facts, &evidence);
}

#[test]
fn core_source_plan_needs_no_release_but_never_drops_planned_release_or_candidate() {
    let (mut plan, identity, facts, mut evidence) = fixture();
    plan.checks
        .retain(|check| check.kind != CheckKind::DerivedRelease);
    evidence
        .results
        .retain(|result| plan.checks.iter().any(|check| check.key == result.key));
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert_eq!(
        verify(&plan, &identity, &facts, &evidence).verdict,
        Verdict::Ready
    );
    let candidate = plan
        .checks
        .iter()
        .find(|check| check.kind == CheckKind::CandidateBuild)
        .unwrap()
        .key
        .clone();
    evidence.results.retain(|result| result.key != candidate);
    assert_not_ready(&plan, &identity, &facts, &evidence);
    plan.checks
        .retain(|check| check.kind != CheckKind::CandidateBuild);
    evidence.expected_plan_digest = plan.digest().unwrap();
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::IncompletePlan)
    );
    let (plan, identity, facts, mut evidence) = fixture();
    let release = plan
        .checks
        .iter()
        .find(|check| check.kind == CheckKind::DerivedRelease)
        .unwrap()
        .key
        .clone();
    evidence.results.retain(|result| result.key != release);
    assert!(
        assert_not_ready(&plan, &identity, &facts, &evidence)
            .failures
            .contains(&Failure::Missing(release))
    );
}

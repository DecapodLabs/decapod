use super::*;
use evidence::{CheckResult, Conclusion, Observation};

fn d(value: &str) -> String {
    evidence::digest(&value).unwrap()
}
fn release() -> ReleaseProfile {
    super::super::derived_release_profile::profile(
        "[package]\nname = \"decapod\"\nversion = \"0.107.5\"\nrust-version = \"1.96.1\"\nrepository = \"https://github.com/DecapodLabs/decapod\"\n[[bin]]\nname = \"decapod\"\npath = \"src/main.rs\"\n",
        "[workspace]\nchangelog_update = true\nchangelog_path = \"CHANGELOG.md\"\ngit_tag_name = \"v{{ version }}\"\npr_labels = [\"release\"]\nallow_dirty = true\n",
        "[workspace]\nmembers = [\"cargo:.\"]\n[dist]\ncargo-dist-version = \"0.22.1\"\nci = \"github\"\ninstallers = []\ntargets = [\"x86_64-unknown-linux-gnu\", \"aarch64-apple-darwin\"]\nallow-dirty = [\"ci\"]\n[dist.github-custom-runners]\nglobal = \"ubuntu-24.04\"\nx86_64-unknown-linux-gnu = \"ubuntu-24.04\"\naarch64-apple-darwin = \"macos-14\"\n",
        "[package]\nbinaries = [\"decapod\"]\n",
    ).unwrap()
}
fn inputs() -> (
    ProfileDeclaration,
    SourceInventory,
    ApplicabilityFacts,
    ReleaseProfile,
) {
    let mut discovered = SourceInventory {
        files: catalog_source_paths()
            .iter()
            .map(|p| ((*p).into(), d(p)))
            .collect(),
    };
    for (path, contents) in definition_files() {
        discovered.files.insert(path.into(), bytes_digest(contents));
    }
    for (path, digest) in reviewed_workflow_digests() {
        discovered.files.insert((*path).into(), (*digest).into());
    }
    let declaration = ProfileDeclaration {
        id: REQUIRED_PROFILE.into(),
        reviewed_sources_digest: discovered.digest().unwrap(),
    };
    let facts = ApplicabilityFacts {
        event: "pull_request".into(),
        target_branch: "master".into(),
        changed_paths: [
            "src/decapod/core/todo.rs",
            "src/decapod/core/sql/new.sql",
            "src/decapod/plugins/todo.rs",
            "src/main.rs",
            "Cargo.toml",
            ".github/workflows/ci.yml",
            "tests/gatling.rs",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    };
    (declaration, discovered, facts, release())
}
fn identity() -> MaterialIdentity {
    MaterialIdentity {
        repository: "github.com/DecapodLabs/decapod".into(),
        work_branch: "work/feature".into(),
        head_revision: "a".repeat(40),
        base_branch: "master".into(),
        accepted_base_revision: "b".repeat(40),
        history_digest: d("history"),
        tree_digest: d("tree"),
        configuration_digest: d("config"),
        workflow_digest: d("workflows"),
        toolchain_digest: d("tools"),
        candidate_digest: d("candidate"),
    }
}
fn catalog() -> RequiredCatalog {
    let (decl, files, facts, release) = inputs();
    required_catalog(Some(&decl), &files, &facts, &release).unwrap()
}
fn bindings(catalog: &RequiredCatalog) -> (ExecutionPolicy, BTreeMap<CheckKey, ExecutionBinding>) {
    let map: BTreeMap<_, _> = catalog
        .obligations()
        .iter()
        .map(|o| {
            (
                o.key.clone(),
                ExecutionBinding {
                    role: o.role,
                    definition_digest: o.definition_digest.clone(),
                    invocation_digest: d(&o.key.profile),
                    toolchain_digest: d("tool"),
                    executor: Provider::Native,
                    dependencies: Vec::new(),
                },
            )
        })
        .collect();
    let policy = ExecutionPolicy {
        buildkite_preflights: false,
        executors: map.keys().map(|k| (k.clone(), Provider::Native)).collect(),
    };
    (policy, map)
}
fn compiled() -> CompiledProfile {
    let catalog = catalog();
    let (policy, bindings) = bindings(&catalog);
    compile_plan(
        catalog,
        &policy,
        &bindings,
        &identity(),
        &d("governance policy"),
    )
    .unwrap()
}
fn state() -> PublicationState {
    PublicationState {
        identity: identity(),
        governance_policy_digest: d("governance policy"),
        governance_worktree_digest: d("governance"),
        governance_index_digest: d("governance"),
        governance_commit_digest: d("governance"),
        uncommitted_paths: BTreeSet::new(),
    }
}
fn target() -> TargetObservation {
    TargetObservation {
        invocation_id: "run-1".into(),
        repository: identity().repository,
        branch: "master".into(),
        revision: "b".repeat(40),
    }
}
fn success(profile: &CompiledProfile) -> SuiteEvidence {
    SuiteEvidence {
        schema_version: evidence::SCHEMA_VERSION.into(),
        expected_plan_digest: profile.plan().digest().unwrap(),
        before: identity(),
        after: identity(),
        facts: profile.facts().clone(),
        results: profile
            .plan()
            .checks
            .iter()
            .map(|c| CheckResult {
                key: c.key.clone(),
                definition_digest: c.definition_digest.clone(),
                invocation_digest: c.invocation_digest.clone(),
                toolchain_digest: c.toolchain_digest.clone(),
                executor: c.executor,
                identity: identity(),
                observation: Observation::Executed {
                    execution_id: format!("run/{}", c.key.profile),
                    conclusion: Conclusion::Success,
                    exit_code: Some(0),
                    output_digest: d("output"),
                    artifact_digest: (c.kind == CheckKind::CandidateBuild)
                        .then(|| d("actual candidate")),
                },
            })
            .collect(),
    }
}

#[test]
fn core_catalog_keeps_all_source_rows_platforms_and_candidate_without_release_packaging() {
    let c = catalog();
    assert!(!c.obligations.is_empty());
    assert_eq!(
        c.obligations
            .iter()
            .filter(|o| o.kind == CheckKind::DerivedRelease)
            .count(),
        0
    );
    assert_eq!(
        c.obligations
            .iter()
            .filter(|o| o.kind == CheckKind::CandidateBuild)
            .count(),
        1
    );
    assert!(
        c.obligations
            .iter()
            .any(|o| o.key.matrix.get("os").is_some_and(|v| v == "macos-latest"))
    );
    assert!(c.obligations.iter().any(|o| {
        o.key
            .matrix
            .get("target")
            .is_some_and(|v| v == "aarch64-unknown-linux-musl")
    }));
    for job in ["health", "validate"] {
        assert!(
            c.obligations
                .iter()
                .any(|o| o.key.job == job && o.role == InvocationRole::ProjectPrerequisites)
        );
    }
    assert!(
        c.obligations
            .iter()
            .any(|o| o.key.job == "integration-prepare")
    );
    assert!(
        c.obligations
            .iter()
            .any(|o| o.key.job == "integration-status")
    );
}

#[test]
fn missing_unknown_and_stale_profile_are_actionable_failures() {
    let (mut decl, mut files, facts, release) = inputs();
    assert!(matches!(
        required_catalog(None, &files, &facts, &release),
        Err(PlanningFailure::MissingProfile)
    ));
    decl.id = "unknown".into();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::UnknownProfile(_))
    ));
    decl.id = REQUIRED_PROFILE.into();
    files
        .files
        .insert(".github/workflows/new.yml".into(), d("new workflow"));
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::SourceContractChanged)
    ));
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::UnsupportedSource(_))
    ));
}

#[test]
fn edited_compiled_selection_helper_and_unknown_provider_cannot_be_auto_blessed() {
    let (mut decl, mut files, facts, release) = inputs();
    files.files.insert(
        "assets/tools/source_ci_profile.rs".into(),
        d("modified helper"),
    );
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::SourceContractChanged)
    ));
    let (mut decl, mut files, facts, release) = inputs();
    files
        .files
        .insert(".buildkite/pipeline.yml".into(), d("new provider"));
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::UnsupportedSource(_))
    ));
}

#[test]
fn missing_source_and_self_referential_config_are_rejected() {
    let (mut decl, mut files, facts, release) = inputs();
    files.files.remove(".github/workflows/ci.yml");
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::MissingSource(_))
    ));
    files.files.insert(".decapod/config.toml".into(), d("self"));
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::SourceContractChanged)
    ));
}

#[test]
fn exclusions_are_recomputed_from_source_facts_not_provider_skip_results() {
    let (decl, files, mut facts, release) = inputs();
    let full = required_catalog(Some(&decl), &files, &facts, &release).unwrap();
    facts.changed_paths = ["README.md".into()].into();
    let docs = required_catalog(Some(&decl), &files, &facts, &release).unwrap();
    assert!(!docs.exclusions().is_empty());
    assert_ne!(full.source_inventory_digest, docs.source_inventory_digest);
    assert!(
        docs.obligations
            .iter()
            .any(|o| o.kind == CheckKind::CandidateBuild)
    );
    assert!(
        !docs
            .obligations
            .iter()
            .any(|o| o.kind == CheckKind::DerivedRelease)
    );
    assert!(
        docs.exclusions
            .iter()
            .any(|o| o.key.profile.ends_with("selective-core"))
    );
}

#[test]
fn every_missing_binding_is_blocking_including_workflow_owned_and_foreign_platform() {
    let c = catalog();
    let (policy, all) = bindings(&c);
    for key in all.keys() {
        let mut fewer = all.clone();
        fewer.remove(key);
        assert!(matches!(
            compile_plan(c.clone(), &policy, &fewer, &identity(), &d("policy")),
            Err(PlanningFailure::MissingBinding(_))
        ));
    }
}

#[test]
fn extra_wrong_role_recursive_and_wrong_backend_bindings_fail() {
    for case in 0..4 {
        let c = catalog();
        let (policy, mut bindings) = bindings(&c);
        let key = bindings.keys().next().unwrap().clone();
        match case {
            0 => {
                let mut k = key.clone();
                k.job = "extra".into();
                bindings.insert(k, bindings[&key].clone());
            }
            1 => bindings.get_mut(&key).unwrap().role = InvocationRole::FullValidator,
            2 => bindings.get_mut(&key).unwrap().executor = Provider::Buildkite,
            _ => bindings
                .get_mut(&key)
                .unwrap()
                .dependencies
                .push(Dependency::ValidationReceipt),
        }
        assert!(compile_plan(c, &policy, &bindings, &identity(), &d("policy")).is_err());
    }
}

#[test]
fn native_and_mixed_remote_policies_have_same_required_inventory() {
    let c = catalog();
    let (mut policy, mut bindings) = bindings(&c);
    let native = compile_plan(c.clone(), &policy, &bindings, &identity(), &d("policy")).unwrap();
    policy.buildkite_preflights = true;
    for (key, executor) in &mut policy.executors {
        *executor = key.provider;
        bindings.get_mut(key).unwrap().executor = *executor;
    }
    let remote = compile_plan(c.clone(), &policy, &bindings, &identity(), &d("policy")).unwrap();
    assert_eq!(
        native.plan.source_inventory_digest,
        remote.plan.source_inventory_digest
    );
    assert_eq!(
        native
            .plan
            .checks
            .iter()
            .map(|c| &c.key)
            .collect::<Vec<_>>(),
        remote
            .plan
            .checks
            .iter()
            .map(|c| &c.key)
            .collect::<Vec<_>>()
    );
    policy.buildkite_preflights = false;
    assert!(compile_plan(c, &policy, &bindings, &identity(), &d("policy")).is_err());
}

#[test]
fn prerequisites_are_explicitly_partial_never_public_readiness() {
    assert_eq!(
        ProjectPrerequisites::Passed.readiness_verdict(),
        Verdict::NotReady
    );
    assert_eq!(
        publication_exit_code(&Ok(ProjectPrerequisites::Passed.readiness_verdict())),
        1
    );
    assert!(matches!(
        freeze(
            "run-1",
            ProjectPrerequisites::Failed,
            &compiled(),
            &state(),
            &target()
        ),
        Err(LifecycleFailure::PrerequisitesFailed)
    ));
}

#[test]
fn frozen_exact_head_with_complete_evidence_is_the_only_success() {
    let profile = compiled();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &profile,
        &state(),
        &target(),
    )
    .unwrap();
    let evidence = success(&profile);
    assert_eq!(
        publication_exit_code(&finish(
            &frozen,
            &profile,
            &state(),
            &target(),
            ProviderOutcome::Complete(&evidence)
        )),
        0
    );
    for outcome in [
        ProviderOutcome::Pending,
        ProviderOutcome::Failed("missing adapter"),
    ] {
        assert_eq!(
            publication_exit_code(&finish(&frozen, &profile, &state(), &target(), outcome)),
            1
        );
    }
    let mut missing = evidence;
    missing.results.clear();
    assert_eq!(
        publication_exit_code(&finish(
            &frozen,
            &profile,
            &state(),
            &target(),
            ProviderOutcome::Complete(&missing)
        )),
        1
    );
}

#[test]
fn fresh_target_equality_is_required_before_and_after_run() {
    let profile = compiled();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &profile,
        &state(),
        &target(),
    )
    .unwrap();
    for case in 0..4 {
        let mut target = target();
        match case {
            0 => target.revision = "c".repeat(40),
            1 => target.branch = "other".into(),
            2 => target.repository = "other".into(),
            _ => target.invocation_id = "old-run".into(),
        }
        assert!(matches!(
            freeze(
                "run-1",
                ProjectPrerequisites::Passed,
                &profile,
                &state(),
                &target
            ),
            Err(LifecycleFailure::StaleBase)
        ));
        assert!(matches!(
            finish(
                &frozen,
                &profile,
                &state(),
                &target,
                ProviderOutcome::Complete(&success(&profile))
            ),
            Err(LifecycleFailure::StaleBase)
        ));
    }
}

#[test]
fn all_commits_including_governance_only_and_receipt_writes_invalidate_readiness() {
    let profile = compiled();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &profile,
        &state(),
        &target(),
    )
    .unwrap();
    for case in 0..5 {
        let mut current = state();
        match case {
            0 => current.identity.head_revision = "c".repeat(40),
            1 => current.identity.history_digest = d("new history"),
            2 => current.governance_worktree_digest = d("new receipt"),
            3 => current.governance_index_digest = d("staged receipt"),
            _ => {
                current
                    .uncommitted_paths
                    .insert(".decapod/governance.json".into());
            }
        }
        assert_eq!(
            publication_exit_code(&finish(
                &frozen,
                &profile,
                &current,
                &target(),
                ProviderOutcome::Complete(&success(&profile))
            )),
            1
        );
    }
}

#[test]
fn stale_config_governance_policy_workflow_toolchain_or_tree_never_pass() {
    let profile = compiled();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &profile,
        &state(),
        &target(),
    )
    .unwrap();
    for mutate in [
        (|s: &mut PublicationState| s.identity.configuration_digest = d("new"))
            as fn(&mut PublicationState),
        |s| s.governance_policy_digest = d("new"),
        |s| s.identity.workflow_digest = d("new"),
        |s| s.identity.toolchain_digest = d("new"),
        |s| s.identity.tree_digest = d("new"),
    ] {
        let mut current = state();
        mutate(&mut current);
        assert_eq!(
            publication_exit_code(&finish(
                &frozen,
                &profile,
                &current,
                &target(),
                ProviderOutcome::Complete(&success(&profile))
            )),
            1
        );
    }
}

#[test]
fn replanned_policy_and_changed_predicate_facts_require_new_proof() {
    let old = compiled();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &old,
        &state(),
        &target(),
    )
    .unwrap();
    let (decl, files, mut facts, release) = inputs();
    facts.changed_paths = ["README.md".into()].into();
    let c = required_catalog(Some(&decl), &files, &facts, &release).unwrap();
    let (policy, bindings) = bindings(&c);
    let new = compile_plan(c, &policy, &bindings, &identity(), &d("governance policy")).unwrap();
    assert!(matches!(
        finish(
            &frozen,
            &new,
            &state(),
            &target(),
            ProviderOutcome::Complete(&success(&new))
        ),
        Err(LifecycleFailure::StalePolicy)
    ));
}

#[test]
fn repeated_final_verification_is_stable_and_never_rewrites_governance() {
    let profile = compiled();
    let state = state();
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::Passed,
        &profile,
        &state,
        &target(),
    )
    .unwrap();
    let proof = success(&profile);
    let before = state.governance_worktree_digest.clone();
    for _ in 0..3 {
        assert_eq!(
            finish(
                &frozen,
                &profile,
                &state,
                &target(),
                ProviderOutcome::Complete(&proof)
            ),
            Ok(Verdict::Ready)
        );
    }
    assert_eq!(before, state.governance_worktree_digest);
}

#[test]
fn modified_known_workflow_cannot_be_blessed_by_only_repinning_project_config() {
    let (mut decl, mut files, facts, release) = inputs();
    files.files.insert(
        ".github/workflows/docs_autosync.yml".into(),
        d("new pull_request trigger"),
    );
    decl.reviewed_sources_digest = files.digest().unwrap();
    assert!(matches!(
        required_catalog(Some(&decl), &files, &facts, &release),
        Err(PlanningFailure::SourceContractChanged)
    ));
}

#[test]
fn source_matrix_keys_preserve_actual_workflow_axes() {
    assert_eq!(
        matrix("test", Some("core")).unwrap(),
        [("target".into(), "core".into())].into()
    );
    assert_eq!(
        matrix("musl-publication", Some("x86_64-unknown-linux-musl")).unwrap(),
        [
            ("target".into(), "x86_64-unknown-linux-musl".into()),
            ("execution".into(), "native".into())
        ]
        .into()
    );
    assert_eq!(
        matrix("musl-publication", Some("aarch64-unknown-linux-musl")).unwrap(),
        [
            ("target".into(), "aarch64-unknown-linux-musl".into()),
            ("execution".into(), "QEMU user-mode".into())
        ]
        .into()
    );
}

#[test]
fn unknown_matrix_shape_requires_explicit_adapter_reconciliation() {
    assert_eq!(
        matrix("new-job", Some("foo")),
        Err(PlanningFailure::SourceContractChanged)
    );
    assert_eq!(
        matrix("musl-publication", Some("new-target")),
        Err(PlanningFailure::SourceContractChanged)
    );
}

#[test]
fn source_catalog_keeps_every_logical_row_once_on_buildkite() {
    let c = catalog();
    let paths: Vec<_> = c.facts.changed_paths.iter().cloned().collect();
    let mut expected_required = BTreeSet::new();
    let mut expected_excluded = BTreeSet::new();
    for job in source::inventory(&paths, false) {
        let members = if job.checks.is_empty() {
            vec!["workflow-owned"]
        } else {
            job.checks.clone()
        };
        for id in members {
            let check_key = key(
                job.workflow,
                job.job,
                matrix(job.job, job.matrix).unwrap(),
                id,
            );
            assert_eq!(check_key.provider, Provider::Buildkite);
            let required = job.applicable
                && (id == "workflow-owned"
                    || source::selected(source::find_check(id).unwrap(), &paths, false));
            if required {
                assert!(expected_required.insert(check_key));
            } else {
                assert!(expected_excluded.insert(check_key));
            }
        }
    }
    assert_eq!(
        c.obligations
            .iter()
            .filter(|o| o.kind == CheckKind::SourceCi)
            .map(|o| o.key.clone())
            .collect::<BTreeSet<_>>(),
        expected_required
    );
    assert_eq!(
        c.exclusions
            .iter()
            .map(|o| o.key.clone())
            .collect::<BTreeSet<_>>(),
        expected_excluded
    );
    assert!(
        c.obligations
            .iter()
            .all(|o| o.key.provider == Provider::Buildkite)
    );
    assert_eq!(c.obligations.len(), expected_required.len() + 1);
    assert!(
        c.stage_dispositions()
            .iter()
            .any(|row| row.stage == "github-pull-request-ci"
                && row.disposition == StageDispositionKind::NonApplicable
                && row.evidence_scope == "no_github_actions_execution_or_assurance_claim")
    );
}

#[test]
fn github_success_never_substitutes_for_buildkite_preflight() {
    let c = catalog();
    let (mut policy, mut bindings) = bindings(&c);
    policy.buildkite_preflights = true;
    for (key, executor) in &mut policy.executors {
        *executor = Provider::Buildkite;
        bindings.get_mut(key).unwrap().executor = *executor;
    }
    let profile = compile_plan(c, &policy, &bindings, &identity(), &d("policy")).unwrap();
    let mut proof = success(&profile);
    for row in &mut proof.results {
        row.key.provider = Provider::GithubActions;
        row.executor = Provider::GithubActions;
    }
    assert_eq!(
        evidence::verify(profile.plan(), &identity(), profile.facts(), &proof).verdict,
        Verdict::NotReady
    );
}

#[test]
fn true_mode_routes_all_source_and_candidate_builds_only_to_buildkite() {
    for executor in [
        Provider::GithubActions,
        Provider::Buildkite,
        Provider::Native,
    ] {
        let c = catalog();
        let (mut policy, mut bindings) = bindings(&c);
        policy.buildkite_preflights = true;
        for (key, route) in &mut policy.executors {
            *route = executor;
            bindings.get_mut(key).unwrap().executor = executor;
        }
        let result = compile_plan(c, &policy, &bindings, &identity(), &d("policy"));
        if executor == Provider::Buildkite {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(PlanningFailure::InvalidBinding(_))));
        }
    }
    let c = catalog();
    let (mut policy, mut bindings) = bindings(&c);
    policy.buildkite_preflights = true;
    for (key, route) in &mut policy.executors {
        *route = Provider::Buildkite;
        bindings.get_mut(key).unwrap().executor = Provider::Buildkite;
    }
    let candidate = c
        .obligations()
        .iter()
        .find(|o| o.kind == CheckKind::CandidateBuild)
        .unwrap()
        .key
        .clone();
    policy
        .executors
        .insert(candidate.clone(), Provider::GithubActions);
    bindings.get_mut(&candidate).unwrap().executor = Provider::GithubActions;
    assert!(matches!(
        compile_plan(c, &policy, &bindings, &identity(), &d("policy")),
        Err(PlanningFailure::InvalidBinding(key)) if key == candidate
    ));
}

#[test]
fn native_mode_retains_complete_platform_obligations() {
    let c = catalog();
    let (policy, mut bindings) = bindings(&c);
    let foreign = c
        .obligations()
        .iter()
        .find(|o| {
            o.key
                .matrix
                .get("os")
                .is_some_and(|os| os == "macos-latest")
        })
        .unwrap()
        .key
        .clone();
    bindings.remove(&foreign);
    assert!(matches!(
        compile_plan(c, &policy, &bindings, &identity(), &d("policy")),
        Err(PlanningFailure::MissingBinding(key)) if key == foreign
    ));
}

#[test]
fn core_dispositions_require_real_source_builds_and_claim_no_release_assurance() {
    let (declaration, mut sources, facts, _) = inputs();
    for path in [".github/release.toml", "dist.toml", "dist-workspace.toml"] {
        sources.files.remove(path);
    }
    let declaration = ProfileDeclaration {
        reviewed_sources_digest: sources.digest().unwrap(),
        ..declaration
    };
    let catalog = required_source_catalog(Some(&declaration), &sources, &facts).unwrap();
    for obligation in catalog.obligations() {
        let disposition = catalog
            .stage_dispositions()
            .iter()
            .find(|d| d.key.as_ref() == Some(&obligation.key))
            .unwrap();
        assert_eq!(disposition.disposition, StageDispositionKind::Real);
        assert_eq!(
            disposition.evidence_scope,
            "authenticated_execution_required"
        );
        assert_ne!(obligation.kind, CheckKind::DerivedRelease);
    }
    for stage in ["publish", "deploy"] {
        let row = catalog
            .stage_dispositions()
            .iter()
            .find(|d| d.stage == stage)
            .unwrap();
        assert_eq!(row.disposition, StageDispositionKind::Mocked);
        assert_eq!(
            row.evidence_scope,
            "simulation_only_no_execution_or_assurance_claim"
        );
        assert!(row.key.is_none());
    }
    let release = catalog
        .stage_dispositions()
        .iter()
        .find(|d| d.stage == "derived-release-packaging")
        .unwrap();
    assert_eq!(release.disposition, StageDispositionKind::NonApplicable);
    assert!(release.reason.contains("#1399"));
    assert_eq!(release.evidence_scope, "no_release_packaging_assurance");
}

#[test]
fn provider_owned_prerequisites_require_remote_rows_and_complete_actual_evidence() {
    let c = catalog();
    let (mut policy, mut bindings) = bindings(&c);
    policy.buildkite_preflights = true;
    for (key, binding) in &mut bindings {
        binding.executor = Provider::Buildkite;
        policy.executors.insert(key.clone(), Provider::Buildkite);
    }
    let profile =
        compile_plan(c, &policy, &bindings, &identity(), &d("governance policy")).unwrap();
    assert_eq!(
        ProjectPrerequisites::ProviderRequired.readiness_verdict(),
        Verdict::NotReady
    );
    let frozen = freeze(
        "run-1",
        ProjectPrerequisites::ProviderRequired,
        &profile,
        &state(),
        &target(),
    )
    .unwrap();
    assert!(
        finish(
            &frozen,
            &profile,
            &state(),
            &target(),
            ProviderOutcome::Pending
        )
        .is_err()
    );
    let mut evidence = success(&profile);
    let row = evidence
        .results
        .iter()
        .position(|row| {
            row.key.profile.ends_with("/health")
                || row.key.profile.ends_with("/standalone-validation")
        })
        .unwrap();
    evidence.results.remove(row);
    assert!(
        finish(
            &frozen,
            &profile,
            &state(),
            &target(),
            ProviderOutcome::Complete(&evidence)
        )
        .is_err()
    );
    assert_eq!(
        finish(
            &frozen,
            &profile,
            &state(),
            &target(),
            ProviderOutcome::Complete(&success(&profile))
        )
        .unwrap(),
        Verdict::Ready
    );
    assert!(
        freeze(
            "run-1",
            ProjectPrerequisites::ProviderRequired,
            &compiled(),
            &state(),
            &target()
        )
        .is_err()
    );
    let mut without_project_rows = profile;
    without_project_rows.provider_prerequisites_required = false;
    assert!(
        freeze(
            "run-1",
            ProjectPrerequisites::ProviderRequired,
            &without_project_rows,
            &state(),
            &target()
        )
        .is_err()
    );
}

#[test]
fn missing_required_build_and_test_contracts_never_compile_an_empty_profile() {
    for path in [
        "assets/tools/source_ci_profile.rs",
        ".github/workflows/ci.yml",
    ] {
        let (mut declaration, mut sources, facts, _) = inputs();
        sources.files.remove(path);
        // Even a matching inventory declaration cannot bless missing coverage.
        declaration.reviewed_sources_digest = sources.digest().unwrap();
        let error = required_source_catalog(Some(&declaration), &sources, &facts).unwrap_err();
        assert_eq!(error, PlanningFailure::MissingSource(path.into()));
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("agent must restore or reconcile"));
        assert!(diagnostic.contains("rerun decapod validate"));
    }
}

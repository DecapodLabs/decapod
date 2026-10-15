use std::fs;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use tempfile::{TempDir, tempdir};

// Bazel removes compilation sandboxes before tests run. Resolve declared data
// through runfiles rather than the compile-time CARGO_MANIFEST_DIR in that case.
fn repository_root() -> std::path::PathBuf {
    match (
        std::env::var_os("TEST_SRCDIR"),
        std::env::var_os("TEST_WORKSPACE"),
    ) {
        (Some(runfiles), Some(workspace)) => Path::new(&runfiles).join(workspace),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf(),
    }
}

// Follow the workflow's specific job through the finite catalog before checking
// its recipe. A command, condition or context export in another job is no proof.
fn source_check(workflow_name: &str, job_id: &str, check_id: &str) -> (String, &'static str, bool) {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows").join(workflow_name))
        .expect("read workflow");
    let header = format!("  {job_id}:\n");
    let jobs = workflow.split_once("\njobs:\n").expect("workflow jobs").1;
    let start = jobs
        .match_indices(&header)
        .find(|(offset, _)| *offset == 0 || jobs.as_bytes()[offset - 1] == b'\n')
        .expect("required workflow job")
        .0;
    let job = jobs[start..]
        .lines()
        .skip(1)
        .take_while(|line| {
            !line.starts_with("  ")
                || line.starts_with("   ")
                || !line.ends_with(':')
                || line.trim_start().starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    let invocation = format!("run: '\"$RUNNER_TEMP/source-ci-profile\" run-ci --check {check_id}'");
    for required in [
        "fetch-depth: 0",
        "uses: ./.github/actions/ci-context",
        "uses: ./.github/actions/source-ci-profile",
        &invocation,
    ] {
        assert!(
            job.lines().any(|line| line.trim() == required),
            "{workflow_name}/{job_id} must bind {check_id}: missing {required}"
        );
    }
    assert!(
        job.find("uses: ./.github/actions/ci-context").unwrap() < job.find(&invocation).unwrap()
    );
    assert!(
        job.find("uses: ./.github/actions/source-ci-profile")
            .unwrap()
            < job.find(&invocation).unwrap()
    );
    let preparation = include_str!("../.github/actions/source-ci-profile/action.yml");
    assert!(preparation.lines().any(|line| line.trim()
        == "rustc --edition=2024 -O assets/tools/source_ci_profile.rs -o \"$RUNNER_TEMP/source-ci-profile\""));

    let context = include_str!("../.github/actions/ci-context/action.yml");
    for (key, variable) in [
        ("event", "EVENT"),
        ("base", "BASE"),
        ("head", "HEAD"),
        ("merge_base", "MERGE_BASE"),
    ] {
        let export =
            format!("{key}) printf 'SOURCE_CI_{variable}=%s\\n' \"$value\" >> \"$GITHUB_ENV\" ;;");
        assert!(
            context.lines().any(|line| line.trim() == export),
            "missing verified {key} export"
        );
    }
    for forbidden in [
        "startsWith(github.head_ref",
        "github.event.pull_request.labels",
    ] {
        assert!(
            !workflow.contains(forbidden),
            "workflow must not infer release status from {forbidden}"
        );
    }

    let profile = include_str!("../assets/tools/source_ci_profile.rs");
    let catalog = profile
        .split_once("pub const CHECKS: &[Check] = &[\n")
        .unwrap()
        .1
        .split_once("\n];")
        .unwrap()
        .0;
    let check_header = format!("        id: \"{check_id}\",\n");
    let entries = catalog
        .split("    Check {\n")
        .filter_map(|entry| entry.split_once("\n    },").map(|(entry, _)| entry))
        .filter(|entry| entry.starts_with(&check_header))
        .collect::<Vec<_>>();
    assert_eq!(
        entries.len(),
        1,
        "{check_id} must have one finite catalog entry"
    );
    for field in [
        format!("workflow: \"{workflow_name}\","),
        format!("job: \"{job_id}\","),
    ] {
        assert!(
            entries[0].lines().any(|line| line.trim() == field),
            "wrong {check_id} catalog binding: {field}"
        );
    }
    let skipped = match entries[0]
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("skipped_for_release_only: "))
    {
        Some("true,") => true,
        Some("false,") => false,
        other => panic!("missing explicit release policy for {check_id}: {other:?}"),
    };
    let dispatch = profile
        .split_once("pub fn commands(")
        .unwrap()
        .1
        .split_once("\nfn executable(")
        .unwrap()
        .0;
    assert!(dispatch.contains("let mut result = if let Some(recipe) = native_vega::commands(check) {\n        recipe\n    } else if let Some(script) = workflow_owned::script(check) {\n        vec![Invocation::new(\"bash\", &[script])]"));
    let run_ci = profile
        .split_once("    if mode == \"run-ci\" {\n")
        .unwrap()
        .1
        .split_once("\n    }")
        .unwrap()
        .0;
    assert!(run_ci.contains("return custody::run_ci_with_executor(\n            &repo,\n            selected_check.unwrap(),\n            base.as_deref(),\n            head.as_deref(),\n            custody::ExecutorContext::from_flags(&custody_flags)?,\n        );"));
    let custody = include_str!("../assets/tools/source_ci_custody.rs");
    let runner = custody
        .split_once("pub fn run_ci_with_executor(\n")
        .unwrap()
        .1
        .split_once("\n#[cfg(test)]")
        .unwrap()
        .0;
    for required in [
        "executor: Option<ExecutorContext>,",
        "let base = variable(\"SOURCE_CI_BASE\", \"CUSTODY_BASE\")?;",
        "let head = variable(\"SOURCE_CI_HEAD\", \"CUSTODY_HEAD\")?;",
        "let recipe = commands(check, &paths)?;",
    ] {
        assert!(
            runner.lines().any(|line| line.trim() == required),
            "run-ci must preserve {required}"
        );
    }
    let execution_context = runner
        .split_once("let mut context = Context {\n")
        .unwrap()
        .1
        .split_once("\n    };")
        .unwrap()
        .0;
    assert!(
        execution_context
            .lines()
            .any(|line| line.trim() == "executor,")
    );
    assert!(runner.contains("if !run(repo, check, &paths, &base, &head, &context, true)? {"));
    assert!(runner.contains("run(repo, check, &paths, &base, &head, &context, false)"));
    let bindings = custody
        .split_once("fn context_bindings(\n")
        .unwrap()
        .1
        .split_once("\nfn ")
        .unwrap()
        .0;
    assert!(bindings.lines().any(|line| line.trim() == "let release_only = !artifact && workflow_owned::release_only(repo, &event, base, head);"));
    assert!(bindings.lines().any(
        |line| line.trim() == "(\"SOURCE_CI_RELEASE_ONLY\".into(), release_only.to_string()),"
    ));
    let recipes = include_str!("../assets/tools/source_ci_workflow_owned.rs");
    let script_map = recipes
        .split_once("pub fn script(check: &Check)")
        .unwrap()
        .1
        .split_once("\npub fn artifact_worker(")
        .unwrap()
        .0;
    let path = format!("assets/tools/source_ci/{check_id}.sh");
    let arm = format!("\"{check_id}\" => \"{path}\",");
    assert_eq!(
        script_map.lines().filter(|line| line.trim() == arm).count(),
        1
    );
    let recipe = match check_id {
        "standalone-validation" => {
            include_str!("../assets/tools/source_ci/standalone-validation.sh")
        }
        "governance-artifacts" => include_str!("../assets/tools/source_ci/governance-artifacts.sh"),
        "material-specs" => include_str!("../assets/tools/source_ci/material-specs.sh"),
        "health" => include_str!("../assets/tools/source_ci/health.sh"),
        _ => panic!("unreviewed policy recipe: {check_id}"),
    };
    (job, recipe, skipped)
}

fn assert_verified_change_bindings(recipe: &str) {
    for binding in [
        "export CHANGE_EVENT=\"${SOURCE_CI_EVENT:?}\"",
        "export CHANGE_BASE=\"${SOURCE_CI_BASE?}\"",
        "export CHANGE_HEAD=\"${SOURCE_CI_HEAD:?}\"",
    ] {
        assert!(
            recipe.lines().any(|line| line == binding),
            "missing recipe binding: {binding}"
        );
    }
}

#[test]
fn release_workflow_lets_release_plz_update_the_manifest() {
    let workflow_path = repository_root().join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release workflow");

    assert!(
        workflow.contains("uses: release-plz/action@"),
        "release workflow should use release-plz"
    );
    assert!(
        workflow.contains("config: .github/release.toml"),
        "release workflow should pass the repository release-plz config"
    );
    assert!(
        !workflow.contains("command: release"),
        "release-plz must not be forced into release-only mode; default mode creates the release PR that updates Cargo.toml before publishing"
    );
}

fn release_job(workflow: &str, id: &str) -> String {
    let header = format!("\n  {id}:\n");
    workflow
        .split_once(&header)
        .expect("release job exists")
        .1
        .lines()
        .take_while(|line| {
            !line.starts_with("  ")
                || line.starts_with("   ")
                || !line.ends_with(':')
                || line.trim_start().starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn release_jobs_require_native_ownership_before_resolving_secrets() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let owner = release_job(workflow, "release-owner");
    assert!(owner.contains("permissions: {}"));
    assert!(!owner.contains("secrets."));
    assert!(!owner.contains("uses:"));
    assert!(owner.contains("native-owner: ${{ steps.owner.outputs.native_owner }}"));

    let jobs: Vec<_> = workflow
        .split_once("\njobs:\n")
        .unwrap()
        .1
        .lines()
        .filter_map(|line| {
            line.strip_prefix("  ")
                .filter(|line| !line.starts_with(' ') && !line.starts_with('#'))
                .and_then(|line| line.strip_suffix(':'))
        })
        .collect();
    assert_eq!(
        jobs,
        [
            "release-owner",
            "release-publish",
            "plan",
            "publish-decapod-image",
            "build-local-artifacts",
            "build-global-artifacts",
            "host",
            "announce",
        ],
        "new release jobs need an explicit ownership review"
    );

    // buildkite-gha v0.102.3 evaluates the job condition before fetching
    // RequiredSecrets or a workflow token. Step-only guards cannot provide
    // that boundary. This checks the source contract, not a live provider run:
    // https://github.com/buildkite/buildkite-gha/blob/v0.102.3/internal/runtime/action_execution.go#L196-L275
    for id in jobs.into_iter().filter(|id| *id != "release-owner") {
        let job = release_job(workflow, id);
        assert!(
            job.contains("    needs: [release-owner]\n")
                || job.contains("    needs:\n      - release-owner\n"),
            "{id} must directly depend on the credential-free ownership job"
        );
        let condition = job
            .lines()
            .find_map(|line| line.strip_prefix("    if: "))
            .expect("release job must have a job-level ownership condition");
        let condition = condition
            .strip_prefix("${{ ")
            .and_then(|condition| condition.strip_suffix(" }}"))
            .unwrap_or(condition);
        let remaining = condition
            .strip_prefix("needs.release-owner.outputs.native-owner == 'true'")
            .unwrap_or_else(|| panic!("{id} must require positive native ownership"));
        if !remaining.is_empty() {
            let expression = remaining
                .strip_prefix(" && (")
                .and_then(|rest| rest.strip_suffix(')'))
                .unwrap_or_else(|| panic!("{id} cannot bypass ownership with an alternative"));
            let mut depth = 0_i32;
            for character in expression.chars() {
                match character {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => (),
                }
                assert!(depth >= 0, "{id} closes the ownership guard too early");
            }
            assert_eq!(depth, 0, "{id} must preserve balanced applicability");
        }
    }
    assert!(!workflow.contains("Run release-plz (Buildkite)"));
    assert!(!workflow.contains("Build and push Decapod image (Buildkite)"));
    assert!(!workflow.contains("cargo install --locked release-plz"));
}

#[cfg(unix)]
#[test]
fn release_owner_selector_is_credential_free_and_fails_closed() {
    use std::os::unix::fs::PermissionsExt;

    let workflow = include_str!("../.github/workflows/release.yml");
    let owner = release_job(workflow, "release-owner");
    let script = owner
        .split_once("        run: |\n")
        .unwrap()
        .1
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n");
    // buildkite-gha v0.102.3 exposes GITHUB_ACTIONS=true and the protected
    // RUNNER_ENVIRONMENT=self-hosted, without inheriting BUILDKITE markers.
    // Reproduce that sanitized process, rather than a full Buildkite host env.
    // https://github.com/buildkite/buildkite-gha/blob/v0.102.3/internal/expression/engine.go#L893-L894
    for (case, github_actions, runner, buildkite, build_id, expected) in [
        (
            "native hosted",
            Some("true"),
            Some("github-hosted"),
            None,
            None,
            "true",
        ),
        (
            "sanitized importer",
            Some("true"),
            Some("self-hosted"),
            None,
            None,
            "false",
        ),
        ("missing runner", Some("true"), None, None, None, "false"),
        ("empty runner", Some("true"), Some(""), None, None, "false"),
        (
            "unknown runner",
            Some("true"),
            Some("unknown"),
            None,
            None,
            "false",
        ),
        (
            "missing GitHub",
            None,
            Some("github-hosted"),
            None,
            None,
            "false",
        ),
        (
            "false GitHub",
            Some("false"),
            Some("github-hosted"),
            None,
            None,
            "false",
        ),
        (
            "Buildkite host",
            Some("true"),
            Some("github-hosted"),
            Some("true"),
            Some("build-fixture"),
            "false",
        ),
        (
            "Buildkite build ID",
            Some("true"),
            Some("github-hosted"),
            None,
            Some("build-fixture"),
            "false",
        ),
        (
            "Buildkite marker",
            Some("true"),
            Some("github-hosted"),
            Some("true"),
            None,
            "false",
        ),
    ] {
        let temp = tempdir().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        // Never let this fixture contact providers, install tools, or publish.
        // Any credential access or mutating command would be a test failure.
        for command in ["release-plz", "cargo", "gh", "docker", "buildkite-agent"] {
            let path = bin.join(command);
            fs::write(
                &path,
                "#!/bin/sh\nprintf '%s\\n' \"$0\" >> \"$COMMAND_LOG\"\nexit 97\n",
            )
            .unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let output_file = temp.path().join("github-output");
        let command_log = temp.path().join("commands");
        let mut command = Command::new("/bin/bash");
        command
            .args(["--noprofile", "--norc", "-c", &script])
            .env_clear()
            .env("PATH", &bin)
            .env("GITHUB_OUTPUT", &output_file)
            .env("COMMAND_LOG", &command_log)
            .current_dir(temp.path());
        for (name, value) in [
            ("GITHUB_ACTIONS", github_actions),
            ("RUNNER_ENVIRONMENT", runner),
            ("BUILDKITE", buildkite),
            ("BUILDKITE_BUILD_ID", build_id),
        ] {
            if let Some(value) = value {
                command.env(name, value);
            }
        }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{case}: {output:?}");
        assert_eq!(
            fs::read_to_string(output_file).unwrap(),
            format!("native_owner={expected}\n"),
            "{case}"
        );
        assert!(
            !command_log.exists(),
            "ownership cannot touch credentials or releases"
        );
        if expected == "false" {
            assert!(String::from_utf8_lossy(&output.stdout).contains("not publication proof"));
        }
    }
}

#[test]
fn release_pr_only_recovery_preserves_default_release_ordering() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let dispatch = workflow
        .split_once("  workflow_dispatch:\n")
        .unwrap()
        .1
        .split_once("\njobs:\n")
        .unwrap()
        .0;
    assert!(dispatch.contains("      release_pr_only:\n"));
    assert!(dispatch.contains("        type: boolean\n"));
    assert!(dispatch.contains("        default: false\n"));
    let publish = release_job(workflow, "release-publish");
    assert_eq!(publish.matches("uses: release-plz/action@").count(), 1);
    assert!(publish.contains("command: ${{ github.event_name == 'workflow_dispatch' && inputs.release_pr_only && 'release-pr' || '' }}"));
    assert!(publish.contains("GITHUB_TOKEN: ${{ steps.generate-token.outputs.token }}"));
    assert!(publish.contains("CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}"));
    assert!(publish.contains("config: .github/release.toml"));
    assert!(!publish.contains("continue-on-error:"));
    // Empty command is the native action's ordered release-pr then release
    // mode, with normal failure propagation. PR-only dispatch selects the
    // upstream release-pr command, never --dry-run (which still opens a PR).
    // https://github.com/release-plz/action/blob/f393a7b38aca9d9b4c58447b291dbac963901df6/action.yml
    assert!(!publish.contains("dry_run:"));
}

#[cfg(unix)]
fn run_release_plan_fixture(
    manifest: &str,
    environment: &[(&str, &str)],
    cargo_exit: i32,
) -> (TempDir, std::process::Output) {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().expect("create temporary home");
    let cargo_bin = temp.path().join(".cargo/bin");
    fs::create_dir_all(&cargo_bin).expect("create cargo bin directory");
    let cargo_dist = cargo_bin.join("cargo-dist");
    fs::write(
        &cargo_dist,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$CARGO_HOME/invocation\"\ncat \"$CARGO_HOME/fixture-manifest.json\"\nexit \"$RELEASE_PLAN_FIXTURE_EXIT\"\n",
    )
    .expect("write cargo-dist fixture");
    fs::set_permissions(&cargo_dist, fs::Permissions::from_mode(0o755))
        .expect("make cargo-dist fixture executable");
    // Shadow cargo too: tag cases must never reach a real host/publish command.
    let cargo = cargo_bin.join("cargo");
    fs::write(
        &cargo,
        "#!/bin/sh\nexec \"$CARGO_HOME/bin/cargo-dist\" \"$@\"\n",
    )
    .expect("write cargo fixture");
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o755))
        .expect("make cargo fixture executable");
    fs::write(temp.path().join(".cargo/fixture-manifest.json"), manifest)
        .expect("write manifest fixture");

    let github_output = temp.path().join("github-output");
    let script = repository_root().join(".github/scripts/release-plan.sh");
    let mut command = Command::new("/bin/bash");
    command
        .current_dir(temp.path())
        .env("HOME", temp.path())
        .env("CARGO_HOME", temp.path().join(".cargo"))
        .env_remove("BUILDKITE_TAG")
        .env_remove("BUILDKITE_BRANCH")
        .env_remove("BUILDKITE_GITHUB_EVENT")
        .env("GITHUB_EVENT_NAME", "pull_request")
        .env("GITHUB_REF_NAME", "release-plz-test")
        .env("GITHUB_REF_TYPE", "branch")
        .env("GITHUB_OUTPUT", &github_output)
        .env("RELEASE_PLAN_FIXTURE_EXIT", cargo_exit.to_string())
        .arg(script);
    for (key, value) in environment {
        command.env(key, value);
    }
    let output = command.output().expect("run release plan script");
    (temp, output)
}

#[cfg(unix)]
#[test]
fn release_plan_restores_cargo_dist_path_after_installer_step() {
    let (temp, output) = run_release_plan_fixture(r#"{"ci":{}}"#, &[], 0);

    assert!(
        output.status.success(),
        "release plan should find cargo-dist in CARGO_HOME/bin; stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let outputs =
        fs::read_to_string(temp.path().join("github-output")).expect("read workflow outputs");
    assert!(outputs.contains("publishing=false"));
    assert!(outputs.contains("has_local_artifacts=false"));
    assert!(outputs.contains("pr_upload=false"));
    assert!(!outputs.contains("manifest="));
    assert_eq!(
        fs::read_to_string(temp.path().join(".cargo/invocation")).unwrap(),
        "dist\nplan\n--output-format=json\n"
    );
}

#[cfg(unix)]
#[test]
fn release_plan_preserves_large_manifests_without_exporting_them() {
    let manifest = serde_json::json!({
        "announcement_github_body": "release notes\n".repeat(1000),
        "ci": {"github": {
            "artifacts_matrix": {"include": [{"target": "x86_64-unknown-linux-gnu"}]},
            "pr_run_mode": "upload"
        }}
    })
    .to_string();
    assert!(manifest.len() > 1024);
    let (temp, output) = run_release_plan_fixture(&manifest, &[], 0);
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        fs::read_to_string(temp.path().join("plan-dist-manifest.json")).unwrap(),
        manifest
    );
    let outputs = fs::read_to_string(temp.path().join("github-output")).unwrap();
    assert_eq!(
        outputs,
        "has_local_artifacts=true\npr_upload=true\ntag=\ntag_flag=\npublishing=false\n"
    );
    for line in outputs.lines() {
        let (_, value) = line.split_once('=').expect("key/value output");
        assert!(value.len() <= 1024, "job output exceeds Buildkite limit");
    }
}

#[cfg(unix)]
#[test]
fn release_plan_scheduling_outputs_preserve_cargo_dist_decisions() {
    for (github, has_local_artifacts, pr_upload) in [
        (serde_json::json!({}), false, false),
        (serde_json::json!({"pr_run_mode": "upload"}), false, true),
        (
            serde_json::json!({"artifacts_matrix": {"include": null}}),
            false,
            false,
        ),
        (
            serde_json::json!({"artifacts_matrix": {"include": []}, "pr_run_mode": "plan"}),
            true,
            false,
        ),
        (
            serde_json::json!({"artifacts_matrix": {"include": [{}]}, "pr_run_mode": "upload"}),
            true,
            true,
        ),
        (
            serde_json::json!({"artifacts_matrix": {"include": [{}]}, "pr_run_mode": "skip"}),
            true,
            false,
        ),
        (
            serde_json::json!({"pr_run_mode": "x".repeat(2048)}),
            false,
            false,
        ),
    ] {
        let manifest = serde_json::json!({"ci": {"github": github}}).to_string();
        let (temp, output) = run_release_plan_fixture(&manifest, &[], 0);
        assert!(output.status.success(), "{manifest}: {:?}", output);
        let outputs = fs::read_to_string(temp.path().join("github-output")).unwrap();
        assert!(outputs.contains(&format!("has_local_artifacts={has_local_artifacts}\n")));
        assert!(outputs.contains(&format!("pr_upload={pr_upload}\n")));
        assert!(outputs.len() < 1024);
    }
}

#[cfg(unix)]
#[test]
fn release_plan_keeps_tag_hosting_explicit_on_both_runtimes() {
    for environment in [
        vec![("GITHUB_REF_TYPE", "tag"), ("GITHUB_REF_NAME", "v1.2.3")],
        vec![("BUILDKITE_TAG", "v1.2.3"), ("GITHUB_REF_NAME", "")],
    ] {
        let (temp, output) = run_release_plan_fixture(r#"{"ci":{}}"#, &environment, 0);
        assert!(output.status.success(), "{:?}", output);
        let outputs = fs::read_to_string(temp.path().join("github-output")).unwrap();
        assert!(outputs.contains("tag=v1.2.3\n"));
        assert!(outputs.contains("tag_flag=--tag=v1.2.3\n"));
        assert!(outputs.contains("publishing=true\n"));
        assert_eq!(
            fs::read_to_string(temp.path().join(".cargo/invocation")).unwrap(),
            "dist\nhost\n--steps=create\n--tag=v1.2.3\n--output-format=json\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn release_plan_invalid_manifests_fail_before_exporting_outputs() {
    for manifest in ["", "{", "null", "[]", "{}\n{}", r#"{"ci":"invalid"}"#] {
        let (temp, output) = run_release_plan_fixture(manifest, &[], 0);
        assert!(
            !output.status.success(),
            "accepted invalid manifest: {manifest}"
        );
        assert!(
            !temp.path().join("github-output").exists(),
            "invalid manifest must not publish scheduling or publishing outputs"
        );
    }
}

#[cfg(unix)]
#[test]
fn release_plan_cargo_failure_does_not_export_publishing_outputs() {
    let (temp, output) = run_release_plan_fixture(
        r#"{"ci":{}}"#,
        &[("GITHUB_REF_TYPE", "tag"), ("GITHUB_REF_NAME", "v1.2.3")],
        42,
    );
    assert_eq!(output.status.code(), Some(42));
    assert!(!temp.path().join("github-output").exists());
}

#[test]
fn release_workflow_keeps_manifests_in_artifacts_and_bounded_job_outputs() {
    let workflow =
        fs::read_to_string(repository_root().join(".github/workflows/release.yml")).unwrap();
    for required in [
        "has-local-artifacts: ${{ steps.plan.outputs.has_local_artifacts }}",
        "pr-upload: ${{ steps.plan.outputs.pr_upload }}",
        "needs.plan.outputs.has-local-artifacts == 'true'",
        "needs.plan.outputs.pr-upload == 'true'",
        "needs.plan.outputs.publishing == 'true'",
        "name: artifacts-plan-dist-manifest",
        "path: plan-dist-manifest.json",
        "name: artifacts-dist-manifest",
        "path: dist-manifest.json",
        "fromJson(steps.host.outputs.manifest).announcement_github_body",
    ] {
        assert!(
            workflow.contains(required),
            "missing release contract: {required}"
        );
    }
    assert!(!workflow.contains("val: ${{ steps.plan.outputs.manifest }}"));
    assert!(!workflow.contains("val: ${{ steps.host.outputs.manifest }}"));
    assert!(!workflow.contains("needs.plan.outputs.val"));
    assert!(
        workflow.contains(
            "always() && needs.plan.result == 'success' && needs.plan.outputs.publishing == 'true'"
        ),
        "hosting must not proceed after a failed plan, even if it emitted publishing=true"
    );
}

#[test]
fn release_plan_does_not_request_a_github_workflow_token() {
    let workflow_path = repository_root().join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(workflow_path).expect("read release workflow");
    let plan_start = workflow.find("\n  plan:\n").expect("find release plan job");
    let plan_end = workflow[plan_start + 1..]
        .find("\n  publish-decapod-image:\n")
        .map(|offset| plan_start + 1 + offset)
        .expect("find job after release plan");
    let plan = &workflow[plan_start..plan_end];

    assert!(
        !plan.contains("GH_TOKEN: ${{") && !plan.contains("GITHUB_TOKEN: ${{"),
        "non-publishing release planning must not request a Buildkite workflow token"
    );
    assert!(
        plan.contains("run: bash .github/scripts/release-plan.sh"),
        "release planning must remain delegated to the checked-in helper"
    );
}

#[test]
fn release_workflow_does_not_auto_heal_release_bound_pins() {
    // Master pins stay at the Decapod version that generated master.
    // release-plz may bump Cargo.toml only; the next user/agent PR advances pins.
    // Release Artifact Sync is removed entirely (#1245).
    let workflow_path = repository_root().join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release workflow");
    assert!(
        !workflow.contains("Sync release-bound artifacts after release-plz"),
        "release.yml must not run a post-release-plz pin heal step"
    );
    assert!(
        !workflow.contains("gh pr create"),
        "release.yml must not open automated release-bound-sync PRs"
    );
    assert!(
        workflow.contains("Intentionally NO post-release-plz")
            || workflow.contains("Release Artifact Sync"),
        "release.yml must document that auto pin heal is gone"
    );
}

#[test]
fn release_artifact_sync_workflow_is_removed() {
    let path = repository_root().join(".github/workflows/release-artifact-sync.yml");
    assert!(
        !path.exists(),
        "Release Artifact Sync workflow must be deleted; agents advance pins in PRs via local validate"
    );
}

#[test]
fn post_merge_and_pr_prerequisites_use_verified_immutable_context() {
    let (job, recipe, _) =
        source_check("decapod-validate.yml", "validate", "standalone-validation");
    assert!(
        recipe
            .lines()
            .any(|line| line == "./target/debug/decapod govern project-check --refresh-specs")
    );
    assert_verified_change_bindings(recipe);
    assert!(recipe.contains("export DECAPOD_VALIDATE_REQUIRE_CHANGE_CONTEXT=\"1\""));
    for source in [&job, recipe] {
        assert!(!source.contains("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES"));
        assert!(!source.contains("github.event.pull_request.labels"));
        assert!(!source.contains("startsWith(github.head_ref"));
    }
}

#[test]
fn release_workflow_publishes_decapod_ghcr_image() {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");
    let dockerfile = fs::read_to_string(root.join("assets/Dockerfile.workspace"))
        .expect("read workspace Dockerfile");

    assert!(
        workflow.contains("packages: write"),
        "release workflow needs package publishing permission for GHCR"
    );
    assert!(
        workflow.contains("ghcr.io/decapodlabs/decapod"),
        "release workflow should publish the Decapod image to GHCR"
    );
    assert!(
        workflow.contains("docker/build-push-action@"),
        "release workflow should build and push the Decapod image"
    );
    assert!(
        workflow.contains("sha_short=${GITHUB_SHA::7}"),
        "release workflow should compute the preferred 7-character commit SHA"
    );
    assert!(
        workflow.contains("type=raw,value=${{ github.ref_name }}${{ matrix.tag_suffix }}"),
        "release workflow should publish the version-matching workspace image tag"
    );
    assert!(
        !workflow.contains("type=raw,value=sha-")
            && !workflow.contains("latest_tag:")
            && !workflow.contains("type=raw,value=${{ matrix.latest_tag }}"),
        "release workflow should not retain SHA or floating workspace image tags"
    );
    assert!(
        workflow.contains("tag_suffix: \"-debian\"")
            && workflow.contains("tag_suffix: \"-alpine\""),
        "release workflow should publish debian/glibc and alpine/musl Decapod workspace image variants"
    );
    assert!(
        !workflow.contains("tag_suffix: \"-bookworm\""),
        "the public glibc tag should remain distro-family based; bookworm is an implementation base"
    );
    assert!(
        workflow.contains("file: assets/Dockerfile.workspace"),
        "release workflow should build the committed Decapod workspace image Dockerfile"
    );
    assert!(
        dockerfile.contains("Decapod workspace image shim"),
        "Decapod image Dockerfile should describe its role as the workspace image shim"
    );
    assert!(
        dockerfile.contains("ARG DECAPOD_BUILD_IMAGE=rust:1.96.1-slim-bookworm")
            && dockerfile.contains("ARG DECAPOD_RUNTIME_IMAGE=debian:bookworm-slim")
            && dockerfile.contains("apk add --no-cache")
            && dockerfile.contains("apt-get install -y --no-install-recommends"),
        "Decapod image Dockerfile should default to glibc while supporting alpine variant builds"
    );
    assert!(
        dockerfile.contains("COPY --from=build /opt/decapod/bin/decapod /usr/local/bin/decapod"),
        "Decapod image should publish the compiled decapod binary"
    );
    assert!(
        dockerfile.contains("LABEL org.opencontainers.image.revision=\"$DECAPOD_REVISION\""),
        "Decapod image should carry the release commit revision label"
    );
}

#[test]
fn release_workflow_verifies_anonymous_ghcr_pull() {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");

    assert!(
        workflow.contains("name: Verify anonymous GHCR pull"),
        "release workflow should verify the published image without caller credentials"
    );
    assert!(
        workflow.contains(
            "https://ghcr.io/token?service=ghcr.io&scope=repository:decapodlabs/decapod:pull"
        ),
        "release workflow should request an anonymous pull token"
    );
    assert!(
        workflow.contains("Authorization: Bearer $token"),
        "release workflow should use the anonymous registry token for the manifest request"
    );
    assert!(
        workflow.contains("https://ghcr.io/v2/decapodlabs/decapod/manifests/$IMAGE_TAG"),
        "release workflow should fetch the exact variant tag that it just published"
    );
}

#[test]
fn release_workflow_can_resume_existing_github_release() {
    let root = repository_root();
    let workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("read release workflow");

    assert!(
        workflow.contains("release_tag=\"${{ needs.plan.outputs.tag }}\"")
            && workflow.contains("gh release view \"$release_tag\""),
        "release workflow should check whether the GitHub release already exists"
    );
    assert!(
        workflow.contains("gh release edit \"$release_tag\""),
        "release workflow should update release metadata when rerun after release creation"
    );
    assert!(
        workflow.contains("gh release upload \"$release_tag\" artifacts/* --clobber"),
        "release workflow should replace assets when rerun against an existing release"
    );
    assert!(
        workflow.contains("gh release create \"$release_tag\" --target \"$RELEASE_COMMIT\""),
        "release workflow should still create the GitHub release on the first publish"
    );
}

#[test]
fn public_release_surface_has_no_private_propodus_dependency() {
    let root = repository_root();
    let cargo_toml = fs::read_to_string(root.join("Cargo.toml")).expect("read Cargo.toml");
    let cargo_lock = fs::read_to_string(root.join("Cargo.lock")).expect("read Cargo.lock");

    assert!(
        !cargo_toml.contains("propodus"),
        "Cargo.toml must not require propodus for the publishable Decapod crate"
    );
    assert!(
        !cargo_lock.contains("name = \"propodus\""),
        "Cargo.lock must not include the private propodus package"
    );
    assert!(
        !cargo_lock.contains("DecapodLabs/propodus"),
        "Cargo.lock must not include a private propodus git source"
    );
    assert!(
        cargo_toml.contains("cloud = []"),
        "Cargo.toml should preserve an explicit public cloud feature seam"
    );

    for workflow in [
        ".github/workflows/ci.yml",
        ".github/workflows/decapod-validate.yml",
        ".github/workflows/docs_sync.yml",
        ".github/workflows/release.yml",
    ] {
        let contents = fs::read_to_string(root.join(workflow)).expect("read workflow");
        assert!(
            !contents.contains("PROPODUS_READONLY_PAT")
                && !contents.contains("DecapodLabs/propodus"),
            "{workflow} must not configure private propodus access for public release checks"
        );
    }
}

#[test]
fn governance_artifact_gate_skips_only_proven_release_metadata() {
    let (job, recipe, skipped) =
        source_check("ci.yml", "governance-artifacts", "governance-artifacts");
    assert!(
        job.lines().any(|line| line.trim() == "if: (needs.release-metadata.outputs.pr_like == 'true') && needs.release-metadata.outputs.release_only != 'true'"),
        "only proven version-only PRs retain the checkpoint-policy exemption"
    );
    assert!(skipped);
    assert!(recipe.lines().any(|line| line == "export GOVERNANCE_BASE=\"${SOURCE_CI_BASE?}\" GOVERNANCE_HEAD_REF=\"${SOURCE_CI_HEAD:?}\""));
    assert!(
        recipe.contains("\"$source_root/target/debug/decapod\" govern artifacts verify-checkpoints \\\n  --base-branch \"$GOVERNANCE_BASE\" \\\n  --head-ref \"$GOVERNANCE_HEAD_REF\" --target-base master"),
        "governance recipe must verify the full immutable PR range"
    );
}

#[test]
fn material_specs_gate_skips_only_proven_release_metadata() {
    let (job, recipe, skipped) = source_check("ci.yml", "material-specs", "material-specs");
    assert!(
        job.lines().any(|line| line.trim() == "if: (needs.release-metadata.outputs.pr_like == 'true') && needs.release-metadata.outputs.release_only != 'true'"),
        "only proven version-only PRs skip the material living-specs gate"
    );
    assert!(skipped);
    for binding in [
        "export CHANGE_BASE=\"${SOURCE_CI_MERGE_BASE:?}\"",
        "export CHANGE_HEAD=\"${SOURCE_CI_HEAD:?}\"",
        "base_ref = os.environ[\"CHANGE_BASE\"]",
        "head_ref = os.environ[\"CHANGE_HEAD\"]",
        "    base_mat = material(git_show(base_ref, rel) or \"\")",
        "    head_mat = material(head_text)",
    ] {
        assert!(
            recipe.lines().any(|line| line == binding),
            "missing material proof: {binding}"
        );
    }
    let failure = recipe
        .split_once("if not material_hits:\n")
        .expect("fail-closed material gate")
        .1
        .split_once("\nprint(")
        .expect("end of failure branch")
        .0;
    assert!(failure.contains("FINGERPRINT_ONLY_SPECS"));
    assert!(failure.lines().any(|line| line == "    sys.exit(1)"));
}

#[test]
fn proven_release_prs_verify_projection_drift_instead_of_skipping_the_gate() {
    for (workflow, job_id, check_id, binary) in [
        (
            "decapod-validate.yml",
            "validate",
            "standalone-validation",
            "./target/debug/decapod-release-projection-drift",
        ),
        (
            "ci.yml",
            "health",
            "health",
            "decapod-release-projection-drift",
        ),
    ] {
        let (job, recipe, skipped) = source_check(workflow, job_id, check_id);
        assert!(!skipped);
        assert!(!job.contains("release_only != 'true'"));
        assert_verified_change_bindings(recipe);
        assert!(recipe.lines().any(|line| line
            == "export RELEASE_ONLY=\"${SOURCE_CI_RELEASE_ONLY:-${RELEASE_ONLY:-false}}\""));
        let release_branch = format!(
            "if [ \"$RELEASE_ONLY\" = \"true\" ]; then\n  {binary} \\\n    --repo . --event \"$CHANGE_EVENT\" --base \"$CHANGE_BASE\" --head \"$CHANGE_HEAD\"\n  exit 0\nfi"
        );
        assert!(
            recipe.contains(&release_branch),
            "{check_id} must verify release drift before success"
        );
        assert!(recipe.contains("# Ensure no spec or entrypoint drift"));
        assert!(recipe.contains("set -euo pipefail"));
        for source in [&job, recipe] {
            assert!(!source.contains("startsWith(github.head_ref"));
            assert!(!source.contains("github.event.pull_request.labels"));
        }
    }
}

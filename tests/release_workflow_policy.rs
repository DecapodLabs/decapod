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
fn post_merge_validate_skips_fingerprint_evaluation() {
    // A tree-local post-merge binary cannot match the last published
    // entrypoint fingerprint; enforce pins on PRs, not on push to master.
    let path = repository_root().join(".github/workflows/decapod-validate.yml");
    let workflow = fs::read_to_string(&path).expect("read decapod-validate workflow");
    assert!(
        workflow.contains("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES"),
        "decapod-validate must support skipping fingerprint gates post-merge"
    );
    assert!(
        workflow.contains("github.event_name == 'push'"),
        "fingerprint skip must be limited to push/post-merge events"
    );
    assert!(
        workflow.contains(
            "if: github.event_name == 'pull_request' && !contains(github.event.pull_request.labels.*.name, 'release') && !startsWith(github.head_ref, 'release-plz-')",
        ),
        "entrypoint/spec drift gate must skip release-plz and release-labelled PRs"
    );
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
    let root = repository_root();
    let workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");

    assert!(
        workflow.contains("governance-artifacts:"),
        "CI must retain the governance artifact job"
    );
    assert!(
        workflow.contains("needs.release-metadata.outputs.release_only != 'true'"),
        "only proven version-only PRs retain the checkpoint-policy exemption"
    );
    for required in [
        "govern artifacts verify-checkpoints",
        "--base-branch \"origin/$GOVERNANCE_BASE_BRANCH\"",
        "--head-ref \"$GOVERNANCE_HEAD_REF\"",
        "github.event.pull_request.head.sha",
        "fetch-depth: 0",
    ] {
        assert!(
            workflow.contains(required),
            "governance job must verify the full immutable PR range: missing {required}"
        );
    }
}

#[test]
fn material_specs_gate_skips_only_proven_release_metadata() {
    let root = repository_root();
    let workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");

    assert!(
        workflow.contains("material-specs:"),
        "CI must retain the material living-specs job (#1183)"
    );
    assert!(
        workflow.contains("FINGERPRINT_ONLY_SPECS"),
        "material-specs job must fail closed on fingerprint-only refreshes"
    );
    assert!(
        workflow.contains("needs.release-metadata.outputs.release_only != 'true'"),
        "only proven version-only PRs skip the material living-specs gate"
    );
}

#[test]
fn release_pr_skips_spec_entrypoint_drift_gate() {
    // Regression for release PR #1236 / actions run 31461500432:
    // release-plz bumps Cargo.toml (repo_signal changes) without entrypoint
    // pins; validate --refresh-specs rewrites the manifest and the drift gate
    // must not hard-fail the release PR. Pin advancement is deferred to the next user/agent PR (Release Artifact Sync removed).
    let root = repository_root();
    let validate = fs::read_to_string(root.join(".github/workflows/decapod-validate.yml"))
        .expect("read decapod-validate workflow");
    let ci = fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");

    assert!(
        validate.contains("startsWith(github.head_ref, 'release-plz-')")
            && validate.contains("!contains(github.event.pull_request.labels.*.name, 'release')"),
        "decapod-validate drift gate must skip release-plz / release-labeled PRs"
    );
    assert!(
        ci.contains("RELEASE_ONLY: ${{ needs.release-metadata.outputs.release_only }}")
            && ci.contains("decapod-release-projection-drift")
            && ci.contains("Ensure no spec or entrypoint drift"),
        "CI must verify expected projection drift only for proven version-only releases"
    );
}

use std::fs;
use std::path::Path;

#[test]
fn release_workflow_lets_release_plz_update_the_manifest() {
    let workflow_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
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

#[test]
fn release_workflow_syncs_release_bound_artifacts_after_release_plz() {
    // Regression since #1170 / v0.95.4: release-plz bumps Cargo.toml without
    // regenerating entrypoint pins; CI drift gate then fails on every release.
    let workflow_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let workflow = fs::read_to_string(&workflow_path).expect("read release workflow");
    assert!(
        workflow.contains("Sync release-bound artifacts after release-plz"),
        "release workflow must heal entrypoint/spec pins in the same job as release-plz"
    );
    assert!(
        workflow.contains("validate --refresh-specs"),
        "post-release-plz sync must regenerate via validate --refresh-specs"
    );
    assert!(
        workflow.contains("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES"),
        "release heal must skip fingerprint hard-fails while regenerating pins"
    );
    assert!(
        workflow.contains("chore: sync release-bound entrypoints and living specs"),
        "post-release-plz sync must commit healed release-bound artifacts"
    );
    assert!(
        workflow.contains("chore/release-bound-sync") && workflow.contains("gh pr create"),
        "when no release PR is open, heal must open a PR rather than push master"
    );
}

#[test]
fn release_artifact_sync_heals_master_and_release_prs() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release-artifact-sync.yml");
    let workflow = fs::read_to_string(&path).expect("read release-artifact-sync workflow");
    assert!(
        workflow.contains("branches: [master]") || workflow.contains("branches: [master]"),
        "release-artifact-sync must also run on master pushes to heal post-merge pin lag"
    );
    assert!(
        workflow.contains("startsWith(github.head_ref, 'release-plz-')"),
        "release-artifact-sync must still cover release-plz PR branches"
    );
    assert!(
        workflow.contains("DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES"),
        "sync must skip fingerprint hard-fails while healing release-bound pins"
    );
    assert!(
        workflow.contains("Release pin heal failed"),
        "sync must fail closed when entrypoint pins remain stale after refresh"
    );
    assert!(
        workflow.contains("create-github-app-token@"),
        "sync must use the GitHub App token (same as release.yml)"
    );
    assert!(
        workflow.contains("gh pr create"),
        "master path must open a PR; direct master push is ruleset-blocked"
    );
    assert!(
        !workflow.contains("TARGET_REF: master")
            && !workflow.contains("git push origin \"HEAD:master\""),
        "must not push healed artifacts directly to master"
    );
    assert!(
        workflow.contains("chore/release-bound-sync"),
        "master heal branch name must be stable for PR updates"
    );
}

#[test]
fn post_merge_validate_skips_fingerprint_evaluation() {
    // A tree-local post-merge binary cannot match the last published
    // entrypoint fingerprint; enforce pins on PRs, not on push to master.
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/decapod-validate.yml");
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
        workflow.contains("if: github.event_name == 'pull_request'"),
        "entrypoint/spec drift gate must remain PR-only"
    );
}

#[test]
fn release_workflow_publishes_decapod_ghcr_image() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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
fn governance_artifact_gate_skips_release_labeled_prs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workflow =
        fs::read_to_string(root.join(".github/workflows/ci.yml")).expect("read CI workflow");

    assert!(
        workflow.contains("governance-artifacts:"),
        "CI must retain the governance artifact job"
    );
    assert!(
        workflow.contains(
            "if: github.event_name == 'pull_request' && !contains(github.event.pull_request.labels.*.name, 'release')"
        ),
        "release-labeled PRs must not be forced to carry the four governance artifacts"
    );
    for artifact in [
        ".decapod/governance/claims.json",
        ".decapod/governance/trajectory.json",
        ".decapod/governance/validation.json",
        ".decapod/governance/plan.json",
    ] {
        assert!(
            workflow.contains(artifact),
            "the regular governance gate must continue checking {artifact}"
        );
    }
}

#[test]
fn material_specs_gate_skips_release_labeled_prs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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
        workflow.contains(
            "if: github.event_name == 'pull_request' && !contains(github.event.pull_request.labels.*.name, 'release')"
        ),
        "release-labeled PRs must not be forced through the material living-specs gate"
    );
}

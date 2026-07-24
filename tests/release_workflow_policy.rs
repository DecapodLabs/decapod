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
        workflow.contains("tag_suffix: \"\"") && workflow.contains("tag_suffix: \"-alpine\""),
        "release workflow should publish glibc and alpine Decapod workspace image variants"
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

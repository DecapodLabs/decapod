// Moved from src/decapod/core/entrypoint_integrity.rs
use super::*;

#[test]
fn compiled_manifest_matches_canonical_templates() {
    let manifest = expected_entrypoints();
    assert_eq!(manifest.len(), ENTRYPOINT_FILES.len());
    for surface in ENTRYPOINT_FILES {
        let payload = canonical_template(surface).expect("canonical entrypoint");
        let expected = expected_fingerprint(surface).expect("compiled fingerprint");
        assert_eq!(
            fingerprint_for_payload(surface, RELEASE_VERSION, &payload),
            expected,
            "compiled release manifest drifted from canonical {surface}"
        );
        let manifest_entry = manifest
            .iter()
            .find(|entry| entry.surface == surface)
            .expect("release manifest entry");
        assert_eq!(
            manifest_entry.fingerprint, expected,
            "release manifest SHA drifted from computed {surface}"
        );
    }
}

#[test]
fn computed_manifest_is_filename_and_version_bound() {
    for surface in ENTRYPOINT_FILES {
        let payload = canonical_template(surface).expect("canonical entrypoint");
        let expected = expected_fingerprint(surface).expect("computed fingerprint");
        assert_eq!(
            fingerprint_for_payload(surface, RELEASE_VERSION, &payload),
            expected
        );
        // Changing the release string must change the fingerprint so stale
        // markers cannot silently pass validation after a version bump.
        let other_release = if RELEASE_VERSION == "0.0.0" {
            "0.0.1"
        } else {
            "0.0.0"
        };
        assert_ne!(
            fingerprint_for_payload(surface, other_release, &payload),
            expected,
            "fingerprint must bind release version for {surface}"
        );
    }
}

#[test]
fn refresh_entrypoint_metadata_rewrites_stale_release_pins() {
    // Simulates a release-plz version bump that commits Cargo.toml without
    // regenerating AGENTS.md headers: body stays canonical, release marker is stale.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let payload = canonical_template("AGENTS.md").expect("template");
    let stale_release = if RELEASE_VERSION == "0.0.0" {
        "0.0.1"
    } else {
        "0.0.0"
    };
    let stale_fp = fingerprint_for_payload("AGENTS.md", stale_release, &payload);
    let stale = format!(
        "<!-- decapod-release: {stale_release} -->\n<!-- decapod-fingerprint: {stale_fp} -->\n{payload}"
    );
    std::fs::write(root.join("AGENTS.md"), &stale).expect("write stale entrypoint");

    let updated = refresh_entrypoint_metadata(root).expect("refresh");
    assert_eq!(updated, 1, "stale release pin must be rewritten");
    let healed = std::fs::read_to_string(root.join("AGENTS.md")).expect("read healed");
    let expected = render_entrypoint("AGENTS.md").expect("render");
    assert_eq!(healed, expected, "headers must match the evaluating binary");
    // Idempotent once current.
    assert_eq!(refresh_entrypoint_metadata(root).expect("second"), 0);
}

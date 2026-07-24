// Moved from src/decapod/core/entrypoint_integrity.rs
use super::*;

#[test]
fn compiled_manifest_matches_canonical_templates() {
    assert_eq!(EXPECTED_ENTRYPOINTS.len(), ENTRYPOINT_FILES.len());
    for surface in ENTRYPOINT_FILES {
        let payload = canonical_template(surface).expect("canonical entrypoint");
        assert_eq!(
            fingerprint_for_payload(surface, RELEASE_VERSION, &payload),
            expected_fingerprint(surface).expect("compiled fingerprint"),
            "compiled release manifest drifted from canonical {surface}"
        );
        let manifest_entry = EXPECTED_ENTRYPOINTS
            .iter()
            .find(|entry| entry.surface == surface)
            .expect("release manifest entry");
        assert_eq!(
            manifest_entry.fingerprint,
            expected_fingerprint(surface).expect("computed fingerprint"),
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
    }
}

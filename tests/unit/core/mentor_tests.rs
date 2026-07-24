// Moved from src/decapod/core/mentor.rs
use super::{MentorEngine, is_decapod_workspace_dockerfile};
use std::fs;

#[test]
fn test_container_candidates_decapod_seed_in_root() {
    let root = tempfile::tempdir().expect("temporary repository");
    let dockerfile = root.path().join("Dockerfile");
    fs::write(
            &dockerfile,
            "ARG DECAPOD_IMAGE=ghcr.io/decapodlabs/decapod:v0.72.13\nFROM $DECAPOD_IMAGE\nLABEL org.decapod.managed=\"workspace\"\n",
        )
        .expect("write root Dockerfile");

    let obligations = MentorEngine::new(root.path())
        .get_container_candidates()
        .expect("compute container obligations");
    let obligation = obligations
        .iter()
        .find(|obligation| obligation.ref_path == dockerfile.to_string_lossy())
        .expect("root Dockerfile obligation");

    assert_eq!(obligation.relevance_score, 1.0);
    assert!(obligation.title.contains("Decapod workspace seed"));
    assert!(
        obligation
            .why_short
            .contains("internal workspace container")
    );
    assert!(is_decapod_workspace_dockerfile(
        &fs::read_to_string(dockerfile).unwrap()
    ));
}

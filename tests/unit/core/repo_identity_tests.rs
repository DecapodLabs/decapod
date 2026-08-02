// Moved from src/decapod/core/repo_identity.rs
use super::parse_github_repository;

#[test]
fn parses_supported_github_remote_forms() {
    for remote in [
        "git@github.com:DecapodLabs/decapod.git",
        "ssh://git@github.com/DecapodLabs/decapod.git",
        "https://github.com/DecapodLabs/decapod",
    ] {
        assert_eq!(
            parse_github_repository(remote).as_deref(),
            Some("DecapodLabs/decapod")
        );
    }
}

#[test]
fn rejects_non_github_and_ambiguous_remotes() {
    for remote in [
        "git@gitlab.com:DecapodLabs/decapod.git",
        "https://github.com/DecapodLabs/decapod/issues",
        "https://github.com/DecapodLabs/decapod/fork.git",
    ] {
        assert!(parse_github_repository(remote).is_none());
    }
}

#[test]
fn repository_identity_is_canonical_without_a_local_gate() {
    let identity =
        super::resolve_repository_identity_from_remote("git@github.com:DecapodLabs/decapod.git")
            .expect("repository identity");
    assert_eq!(identity.canonical_name, "DecapodLabs/decapod");
    assert_eq!(identity.owner, "DecapodLabs");
    assert_eq!(identity.repository, "decapod");
}

#[test]
fn repository_identity_preserves_fork_boundaries() {
    let identity = super::resolve_repository_identity_from_remote(
        "https://github.com/example/decapod-fork.git",
    )
    .expect("fork identity");
    assert_eq!(identity.canonical_name, "example/decapod-fork");
    assert_ne!(identity.canonical_name, super::DOGFOOD_REPOSITORY);
}

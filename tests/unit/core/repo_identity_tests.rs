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

#[test]
fn gitlab_source_keys_support_nested_namespaces_without_provider_collisions() {
    for remote in [
        "git@gitlab.com:example/team/project.git",
        "ssh://git@gitlab.com/example/team/project.git",
        "https://gitlab.com/example/team/project",
    ] {
        let identity = super::resolve_repository_identity_from_remote(remote).unwrap();
        assert_eq!(identity.canonical_name, "gitlab:example%2Fteam/project");
        assert_eq!(identity.owner, "example/team");
        assert_eq!(identity.repository, "project");
        assert_eq!(identity.canonical_name.matches('/').count(), 1);
    }
    let github =
        super::resolve_repository_identity_from_remote("https://github.com/example/project.git")
            .unwrap();
    let gitlab =
        super::resolve_repository_identity_from_remote("https://gitlab.com/example/project.git")
            .unwrap();
    assert_ne!(github.canonical_name, gitlab.canonical_name);
    assert_eq!(github.canonical_name, "example/project");
}

#[test]
fn source_keys_reject_ambiguous_urls_without_echoing_credentials() {
    for remote in [
        "https://user:private-secret@gitlab.com/example/project.git",
        "git@github.com:user:private-secret/project.git",
        "https://github.com/example/project.git?token=private-secret",
        "https://gitlab.com/example/project.git?token=private-secret",
        "https://gitlab.com/example//project.git",
        "https://gitlab.com/example/../project.git",
        "https://gitlab.com/example%2Fteam/project.git",
        "https://other-gitlab.example/example/project.git",
    ] {
        let error = super::resolve_repository_identity_from_remote(remote).unwrap_err();
        assert!(!error.to_string().contains("private-secret"));
    }
}

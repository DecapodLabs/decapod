// Moved from src/decapod/cli.rs
use super::{BackendType, RepoContext};

#[test]
fn backend_field_selects_the_repository_backend() {
    let mut context = RepoContext {
        backend: Some(BackendType::Cloud),
        ..RepoContext::default()
    };
    assert_eq!(context.effective_backend(), BackendType::Cloud);

    context.backend = Some(BackendType::Local);
    assert_eq!(context.effective_backend(), BackendType::Local);
}

#[test]
fn setting_backend_selects_the_canonical_config_field() {
    let mut context = RepoContext::default();
    context.set_backend(BackendType::Cloud);
    assert_eq!(context.backend, Some(BackendType::Cloud));
    assert_eq!(context.effective_backend(), BackendType::Cloud);
}

#[test]
fn governance_lifecycle_requires_explicit_boundary_and_falsifiable_claims() {
    use super::{ArtifactsCli, ArtifactsCommand, Cli, Command, GovernCli, GovernCommand};
    use clap::Parser;
    let parsed = Cli::try_parse_from([
        "decapod",
        "govern",
        "artifacts",
        "begin-pr",
        "--id",
        "change-7",
        "--base-branch",
        "master",
    ])
    .unwrap();
    assert!(matches!(parsed.command,
        Command::Govern(GovernCli { command: GovernCommand::Artifacts(ArtifactsCli {
            command: ArtifactsCommand::BeginPr { id, base_branch },
        }) }) if id == "change-7" && base_branch == "master"));
    assert!(
        Cli::try_parse_from([
            "decapod",
            "govern",
            "artifacts",
            "begin-pr",
            "--id",
            "change-7",
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "decapod",
            "govern",
            "artifacts",
            "claim",
            "--id",
            "claim-7",
            "--statement",
            "All authored commits are covered",
            "--status",
            "supported",
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "decapod",
            "govern",
            "artifacts",
            "claim",
            "--id",
            "claim-7",
            "--statement",
            "All authored commits are covered",
            "--falsifier",
            "A missing checkpoint",
            "--status",
            "done",
        ])
        .is_err()
    );
}

#[test]
fn governance_checkpoint_accepts_cumulative_proof_references() {
    use super::{ArtifactsCli, ArtifactsCommand, Cli, Command, GovernCli, GovernCommand};
    use clap::Parser;
    let parsed = Cli::try_parse_from([
        "decapod",
        "govern",
        "artifacts",
        "checkpoint",
        "--id",
        "checkpoint-2",
        "--summary",
        "Publication proof",
        "--proof-ref",
        "test:first",
        "--proof-ref",
        "test:second",
    ])
    .unwrap();
    assert!(matches!(parsed.command,
        Command::Govern(GovernCli { command: GovernCommand::Artifacts(ArtifactsCli {
            command: ArtifactsCommand::Checkpoint { proof_refs, .. },
        }) }) if proof_refs == ["test:first", "test:second"]));
    let verify = Cli::try_parse_from([
        "decapod",
        "govern",
        "artifacts",
        "verify-checkpoints",
        "--base-branch",
        "origin/master",
    ])
    .unwrap();
    assert!(matches!(verify.command,
        Command::Govern(GovernCli { command: GovernCommand::Artifacts(ArtifactsCli {
            command: ArtifactsCommand::VerifyCheckpoints { head_ref, .. },
        }) }) if head_ref == "HEAD"));
}

#[test]
fn governance_obligation_resolution_requires_explicit_proof() {
    use super::{ArtifactsCli, ArtifactsCommand, Cli, Command, GovernCli, GovernCommand};
    use clap::Parser;
    assert!(
        Cli::try_parse_from([
            "decapod",
            "govern",
            "artifacts",
            "resolve-obligation",
            "--id",
            "claim:open-1",
            "--resolution",
            "The falsifier was tested",
        ])
        .is_err()
    );
    let parsed = Cli::try_parse_from([
        "decapod",
        "govern",
        "artifacts",
        "resolve-obligation",
        "--id",
        "claim:open-1",
        "--resolution",
        "The falsifier was tested",
        "--proof-ref",
        "test:first",
        "--proof-ref",
        "test:second",
    ])
    .unwrap();
    assert!(matches!(parsed.command,
        Command::Govern(GovernCli { command: GovernCommand::Artifacts(ArtifactsCli {
            command: ArtifactsCommand::ResolveObligation { id, proof_refs, .. },
        }) }) if id == "claim:open-1" && proof_refs == ["test:first", "test:second"]));
}

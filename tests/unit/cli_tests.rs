// Moved from src/decapod/cli.rs
use super::{BackendType, RepoContext};

#[test]
fn readiness_snapshot_handoff_flags_are_explicit_and_unambiguous() {
    use super::{Cli, Command};
    use clap::Parser;
    assert!(
        matches!(Cli::try_parse_from(["decapod", "validate", "--snapshot-prepare"]).unwrap().command,
        Command::Validate(value) if value.snapshot_prepare && value.snapshot_resume.is_none())
    );
    assert!(matches!(Cli::try_parse_from([
        "decapod", "validate", "--snapshot-resume", "/private/handoff.json",
        "--provider-artifacts", "/private/artifacts"
    ]).unwrap().command, Command::Validate(value)
        if !value.snapshot_prepare && value.snapshot_resume.is_some() && value.provider_artifacts.is_some()));
    assert!(
        Cli::try_parse_from([
            "decapod",
            "validate",
            "--snapshot-prepare",
            "--snapshot-resume",
            "/private/handoff.json"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "decapod",
            "validate",
            "--provider-artifacts",
            "/private/artifacts"
        ])
        .is_err()
    );
}

#[test]
fn snapshot_adoption_is_explicit_and_cannot_mix_with_proof_import() {
    use super::{Cli, Command};
    use clap::Parser;
    let sha = "1234567890123456789012345678901234567890";
    assert!(matches!(Cli::try_parse_from([
        "decapod", "validate", "--snapshot-resume", "/private/prior.json",
        "--snapshot-adopt", sha
    ]).unwrap().command, Command::Validate(value) if value.snapshot_adopt.as_deref() == Some(sha)));
    assert!(Cli::try_parse_from(["decapod", "validate", "--snapshot-adopt", sha]).is_err());
    assert!(
        Cli::try_parse_from([
            "decapod",
            "validate",
            "--snapshot-resume",
            "/private/prior.json",
            "--snapshot-adopt",
            sha,
            "--provider-artifacts",
            "/private/artifacts"
        ])
        .is_err()
    );
}

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

#[test]
fn init_buildkite_preflights_distinguishes_omitted_true_and_false() {
    use super::{Cli, Command, InitCommand};
    use clap::Parser;

    for surface in [vec!["decapod", "init"], vec!["decapod", "init", "with"]] {
        for (option, expected) in [
            (vec![], None),
            (vec!["--validation-buildkite-preflights"], Some(true)),
            (vec!["--validation-buildkite-preflights=true"], Some(true)),
            (vec!["--validation-buildkite-preflights=false"], Some(false)),
            (
                vec!["--validation-buildkite-preflights", "false"],
                Some(false),
            ),
        ] {
            let mut args = surface.clone();
            args.extend(option);
            let parsed = Cli::try_parse_from(args).expect("parse init provider option");
            let Command::Init(init) = parsed.command else {
                panic!("expected init");
            };
            let actual = match init.command {
                Some(InitCommand::With(with)) => with.validation_buildkite_preflights,
                None => init.validation_buildkite_preflights,
                _ => panic!("expected init or init with"),
            };
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn init_buildkite_preflights_rejects_non_boolean_values() {
    use super::Cli;
    use clap::Parser;

    for surface in [vec!["decapod", "init"], vec!["decapod", "init", "with"]] {
        let mut args = surface;
        args.push("--validation-buildkite-preflights=disabled");
        assert!(Cli::try_parse_from(args).is_err());
    }
}

#[test]
fn project_check_is_explicit_and_does_not_change_validate_scope() {
    use super::{Cli, Command, GovernCli, GovernCommand};
    use clap::{CommandFactory, Parser};
    let parsed = Cli::try_parse_from([
        "decapod",
        "govern",
        "project-check",
        "--refresh-specs",
        "--format",
        "json",
    ])
    .unwrap();
    assert!(
        matches!(parsed.command, Command::Govern(GovernCli { command: GovernCommand::ProjectCheck(check) }) if check.refresh_specs && check.format == "json")
    );
    assert!(matches!(
        Cli::try_parse_from(["decapod", "validate"])
            .unwrap()
            .command,
        Command::Validate(_)
    ));
    let help = Cli::command().render_long_help().to_string();
    assert!(!help.contains("__project-check-worker"));
}

#[test]
fn immutable_checkpoint_range_can_keep_its_named_target_binding() {
    use super::{ArtifactsCli, ArtifactsCommand, Cli, Command, GovernCli, GovernCommand};
    use clap::Parser;
    let parsed = Cli::try_parse_from([
        "decapod",
        "govern",
        "artifacts",
        "verify-checkpoints",
        "--base-branch",
        "0123456789012345678901234567890123456789",
        "--head-ref",
        "1123456789012345678901234567890123456789",
        "--target-base",
        "master",
    ])
    .unwrap();
    assert!(
        matches!(parsed.command, Command::Govern(GovernCli { command: GovernCommand::Artifacts(ArtifactsCli { command: ArtifactsCommand::VerifyCheckpoints { target_base: Some(target), .. } }) }) if target == "master")
    );
}

#[test]
fn prerequisite_bootstrap_guidance_holds_publication_for_full_validation() {
    let source = include_str!("../../src/decapod/core/assets.rs");
    assert!(source.contains(
        "decapod docs ingest && decapod govern project-check && decapod session acquire"
    ));
    assert!(!source.contains("decapod docs ingest && decapod validate && decapod session acquire"));
    assert!(source.contains("including a draft"));
    assert!(source.contains("its success is never permission to create a PR"));
    assert!(source.contains("Work branch pushes and iteration are allowed before preflight"));
    assert!(
        source.contains("intent is converged, the branch is mergeable into the accepted target")
    );
    assert!(source.contains("accepted-target change invalidates earlier readiness"));
}

#[test]
fn workspace_publish_rpc_rejects_snapshot_adoption_instead_of_ignoring_it() {
    let params = serde_json::json!({
        "snapshot_resume": "/private/handoff.json",
        "snapshot_adopt": "1234567890123456789012345678901234567890"
    });
    assert!(serde_json::from_value::<crate::core::rpc::WorkspacePublishParams>(params).is_err());
}

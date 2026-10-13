use crate::core::error;
use crate::core::events;
use crate::core::store::{Store, StoreKind};
use crate::plugins::policy;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Output};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalCapability {
    VcsRead,
    VcsWrite,
    ProofExec,
    VerificationExec,
    SystemInspect,
    PreflightProbe,
    PreflightRead,
}

impl ExternalCapability {
    fn as_str(self) -> &'static str {
        match self {
            ExternalCapability::VcsRead => "vcs_read",
            ExternalCapability::VcsWrite => "vcs_write",
            ExternalCapability::ProofExec => "proof_exec",
            ExternalCapability::VerificationExec => "verification_exec",
            ExternalCapability::SystemInspect => "system_inspect",
            ExternalCapability::PreflightProbe => "preflight_probe",
            ExternalCapability::PreflightRead => "preflight_read",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionRule {
    capability: String,
    allowed_bins: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionConfig {
    rules: Vec<ExternalActionRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExternalActionEvent {
    ts: String,
    event_id: String,
    capability: String,
    scope: String,
    command: String,
    args: Vec<String>,
    cwd: String,
    status: String,
    exit_code: Option<i32>,
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

fn default_config() -> ExternalActionConfig {
    ExternalActionConfig {
        rules: vec![
            ExternalActionRule {
                capability: "vcs_read".to_string(),
                allowed_bins: vec!["git".to_string()],
            },
            ExternalActionRule {
                capability: "vcs_write".to_string(),
                allowed_bins: vec!["git".to_string()],
            },
            ExternalActionRule {
                capability: "proof_exec".to_string(),
                allowed_bins: vec![
                    "cargo".to_string(),
                    "decapod".to_string(),
                    "git".to_string(),
                    "bash".to_string(),
                    "sh".to_string(),
                ],
            },
            ExternalActionRule {
                capability: "verification_exec".to_string(),
                allowed_bins: vec!["decapod".to_string()],
            },
            ExternalActionRule {
                capability: "system_inspect".to_string(),
                allowed_bins: vec!["lsof".to_string()],
            },
        ],
    }
}

fn maybe_load_config(store_root: &Path) -> ExternalActionConfig {
    let repo_root = store_root.parent().and_then(|p| p.parent());
    let Some(repo_root) = repo_root else {
        return default_config();
    };
    let path = repo_root.join(".decapod").join("EXTERNAL_ACTIONS.json");
    if !path.exists() {
        return default_config();
    }
    let Ok(content) = std::fs::read_to_string(path) else {
        return default_config();
    };
    serde_json::from_str(&content).unwrap_or_else(|_| default_config())
}

fn allowed_for_capability(
    config: &ExternalActionConfig,
    capability: ExternalCapability,
) -> Vec<String> {
    config
        .rules
        .iter()
        .find(|r| r.capability == capability.as_str())
        .map(|r| r.allowed_bins.clone())
        .unwrap_or_default()
}

fn command_bin(command: &str) -> String {
    Path::new(command)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| command.to_string())
}

fn require_external_approval(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
) -> Result<(), error::DecapodError> {
    // Only write-like external capabilities require approval.
    if capability != ExternalCapability::VcsWrite {
        return Ok(());
    }
    // Scoped low-risk internal reconciliation path.
    if scope == "todo.handoff.reconcile" {
        return Ok(());
    }
    let store = Store {
        kind: StoreKind::Repo,
        root: store_root.to_path_buf(),
    };
    let approval_scope = format!("external:{}:{}", capability.as_str(), scope);
    let risk = policy::RiskLevel::HIGH;
    let requires_human = policy::human_in_loop_required(&store, &approval_scope, risk, true);
    if !requires_human {
        return Ok(());
    }
    policy::initialize_policy_db(store_root)?;
    if !policy::check_approval(&store, &approval_scope, Some(&approval_scope), "global")? {
        return Err(error::DecapodError::ValidationError(format!(
            "External action denied: capability '{}' scope '{}' requires approval. Run `decapod govern policy eval --command '{}' --path '{}'` to get the approval command.",
            capability.as_str(),
            scope,
            approval_scope,
            approval_scope
        )));
    }
    Ok(())
}

fn log_event(store_root: &Path, event: &ExternalActionEvent) -> Result<(), error::DecapodError> {
    events::append(
        store_root,
        events::EXTERNAL_ACTIONS,
        &serde_json::to_value(event)
            .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
    )?;
    Ok(())
}

/// Check the existing external-action policy before a separately bounded runner
/// starts an already reviewed invocation. This does not execute, grant approval,
/// change configuration, or accept arbitrary imported command definitions.
pub fn authorize(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
    command: &str,
) -> Result<(), error::DecapodError> {
    if matches!(
        capability,
        ExternalCapability::PreflightProbe | ExternalCapability::PreflightRead
    ) {
        return Err(error::DecapodError::ValidationError(
            "Preflight operations require the sealed argument-aware authorization path".into(),
        ));
    }
    authorize_checked(store_root, capability, scope, command)
}

fn authorize_checked(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
    command: &str,
) -> Result<(), error::DecapodError> {
    authorize_binary(store_root, capability, command)?;
    require_external_approval(store_root, capability, scope)
}

fn authorize_binary(
    store_root: &Path,
    capability: ExternalCapability,
    command: &str,
) -> Result<(), error::DecapodError> {
    let config = maybe_load_config(store_root);
    let allowed_bins = allowed_for_capability(&config, capability);
    let bin = command_bin(command);
    let explicitly_configured = config
        .rules
        .iter()
        .any(|r| r.capability == capability.as_str());
    let sealed_default = !explicitly_configured
        && matches!(
            capability,
            ExternalCapability::PreflightProbe | ExternalCapability::PreflightRead
        );
    let is_allowed = sealed_default
        || allowed_bins.iter().any(|b| b == &bin)
        || (capability == ExternalCapability::VerificationExec && bin.starts_with("decapod"));
    if !is_allowed {
        return Err(error::DecapodError::ValidationError(format!(
            "External action denied: capability '{}' does not allow binary '{}'",
            capability.as_str(),
            bin
        )));
    }

    Ok(())
}

/// Configured validation owns its exact workspace-branch submission. Only the
/// publisher can mint this seal, after source, target, tags and ancestry checks.
/// Keep the existing write executable policy; no operator approval receipt is
/// needed for this bounded validation step. Generic writes retain their policy.
pub fn authorize_workspace_branch_push(
    store_root: &Path,
    program: &Path,
    _request: &crate::core::snapshot_provider::snapshot::AuthorizedWorkspaceBranchPush,
) -> Result<(), error::DecapodError> {
    if !program.is_absolute() || program.file_name().and_then(|p| p.to_str()) != Some("git") {
        return Err(error::DecapodError::ValidationError(
            "WORKSPACE_PUSH_REJECTED: installed absolute Git required".into(),
        ));
    }
    authorize_binary(
        store_root,
        ExternalCapability::VcsWrite,
        &program.to_string_lossy(),
    )
}

/// Authorize only a fixed read-only installed-tool probe. This cannot execute
/// commands, install tools, evaluate shell text, or authorize provider writes.
/// Generic `authorize`/`execute` reject this capability; the typed caller still
/// owns the bounded process and installed-file identity checks.
pub fn authorize_preflight_probe(
    store_root: &Path,
    program: &Path,
    arguments: &[&str],
) -> Result<(), error::DecapodError> {
    let name = program.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let versioned = |prefix: &str| {
        name == prefix
            || name
                .strip_prefix(&format!("{prefix}-"))
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit() || c == b'.')
                })
    };
    let compiler = [
        "cc",
        "gcc",
        "clang",
        "x86_64-linux-gnu-gcc",
        "aarch64-linux-gnu-gcc",
    ]
    .iter()
    .any(|prefix| versioned(prefix));
    let allowed = program.is_absolute()
        && ((name == "rustc"
            && matches!(
                arguments,
                ["--version", "--verbose"] | ["--print", "sysroot"]
            ))
            || (name == "cargo" && arguments == ["--version", "--verbose"])
            || (compiler && arguments == ["--version"]));
    if !allowed {
        return Err(error::DecapodError::ValidationError(
            "PREFLIGHT_PROBE_REJECTED: unsupported executable or probe arguments".into(),
        ));
    }
    authorize_checked(
        store_root,
        ExternalCapability::PreflightProbe,
        "readiness.toolchain",
        &program.to_string_lossy(),
    )
}

/// Only the public-site reader may mint this route seal. Its bounded HTTP
/// engine validates each artifact redirect before following it. No auth token,
/// arbitrary URL, POST, cookie jar or shell command is accepted here.
pub fn authorize_public_buildkite_read(
    store_root: &Path,
    program: &Path,
    request: &crate::core::snapshot_provider::public_buildkite::AuthorizedPublicRead,
) -> Result<(), error::DecapodError> {
    if !program.is_absolute()
        || program.file_name().and_then(|p| p.to_str()) != Some("curl")
        || request.method() != "GET"
        || !request.url().starts_with("https://buildkite.com/")
        || request.max_https_redirects() > 3
    {
        return Err(error::DecapodError::ValidationError(
            "PREFLIGHT_READ_REJECTED".into(),
        ));
    }
    authorize_checked(
        store_root,
        ExternalCapability::PreflightRead,
        request.scope(),
        &program.to_string_lossy(),
    )
}

/// Authorize the explicit zero-redirect anonymous GitHub JSON reader. Protected
/// raw logs require a supported authenticated host; this route never upgrades
/// to an official CLI or follows an unvalidated storage redirect.
pub fn authorize_public_github_json_read(
    store_root: &Path,
    program: &Path,
    request: &crate::core::snapshot_provider::read_authorization::AuthorizedProviderRead,
) -> Result<(), error::DecapodError> {
    if !program.is_absolute()
        || program.file_name().and_then(|p| p.to_str()) != Some("curl")
        || request.method() != "GET"
        || !request
            .initial_url()
            .starts_with("https://api.github.com/repos/")
        || request
            .initial_url()
            .split('?')
            .next()
            .is_some_and(|p| p.ends_with("/logs") || p.ends_with("/zip"))
    {
        return Err(error::DecapodError::ValidationError(
            "PREFLIGHT_READ_REJECTED".into(),
        ));
    }
    authorize_checked(
        store_root,
        ExternalCapability::PreflightRead,
        request.scope(),
        &program.to_string_lossy(),
    )
}

/// Authorize only a sealed fixed GET through an already configured official gh.
/// The seal checks exact argv, repository, endpoint and archive-size bounds.
/// gh owns authenticated redirects; this does not grant arbitrary URLs, methods,
/// login, token access, or permission to change persistent authentication.
pub fn authorize_official_github_read(
    store_root: &Path,
    program: &Path,
    request: &crate::core::snapshot_provider::read_authorization::AuthorizedProviderRead,
) -> Result<(), error::DecapodError> {
    use crate::core::snapshot_provider::read_authorization::ProviderReadKind;
    if !program.is_absolute()
        || program.file_name().and_then(|p| p.to_str()) != Some("gh")
        || request.method() != "GET"
        || !request
            .initial_url()
            .starts_with("https://api.github.com/repos/")
        || !matches!(
            request.kind(),
            ProviderReadKind::GithubObjects
                | ProviderReadKind::GithubArtifactMetadata
                | ProviderReadKind::GithubArtifactZip
                | ProviderReadKind::GithubCommitChecks
                | ProviderReadKind::GithubRuns
                | ProviderReadKind::GithubJobs
                | ProviderReadKind::GithubJobLog
        )
    {
        return Err(error::DecapodError::ValidationError(
            "PREFLIGHT_READ_REJECTED".into(),
        ));
    }
    authorize_checked(
        store_root,
        ExternalCapability::PreflightRead,
        request.scope(),
        &program.to_string_lossy(),
    )
}

pub fn execute(
    store_root: &Path,
    capability: ExternalCapability,
    scope: &str,
    command: &str,
    args: &[&str],
    cwd: &Path,
) -> Result<Output, error::DecapodError> {
    authorize(store_root, capability, scope, command)?;

    let output = Command::new(command)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(error::DecapodError::IoError)?;

    let event = ExternalActionEvent {
        ts: now_iso(),
        event_id: crate::core::ulid::new_ulid(),
        capability: capability.as_str().to_string(),
        scope: scope.to_string(),
        command: command.to_string(),
        args: args.iter().map(|s| s.to_string()).collect(),
        cwd: cwd.to_string_lossy().to_string(),
        status: if output.status.success() {
            "success".to_string()
        } else {
            "error".to_string()
        },
        exit_code: output.status.code(),
    };
    let _ = log_event(store_root, &event);

    Ok(output)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "external_action",
        "version": "0.1.0",
        "description": "Capability-scoped broker for external commands with default-deny allowlists",
        "capabilities": [
            "vcs_read",
            "vcs_write",
            "proof_exec",
            "verification_exec",
            "system_inspect"
        ],
        "config": ".decapod/EXTERNAL_ACTIONS.json",
        "storage": ["decapod.db:external_actions_events"]
    })
}

#[cfg(test)]
mod preflight_authorization_tests {
    use super::*;
    #[test]
    fn official_github_read_requires_exact_sealed_get_and_installed_cli() {
        use crate::core::snapshot_provider::{CommandSpec, Program, ProviderRequest, Repository};
        let root = tempfile::tempdir().unwrap();
        let repository = Repository::new("owner", "repo").unwrap();
        let path = "repos/owner/repo/actions/artifacts/42/zip";
        let mut spec = CommandSpec {
            program: Program::Github,
            args: [
                "api",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "--header",
                "Accept: application/vnd.github+json",
                path,
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            cwd: root.path().into(),
            organization: None,
            provider_request: Some(ProviderRequest::GithubArtifactZip {
                repository,
                artifact_id: 42,
                expected_bytes: 1024,
            }),
        };
        let sealed = spec.authorize_provider_read().unwrap();
        authorize_official_github_read(root.path(), Path::new("/usr/bin/gh"), &sealed).unwrap();
        for program in ["gh", "/usr/bin/curl", "/usr/bin/sh"] {
            assert!(
                authorize_official_github_read(root.path(), Path::new(program), &sealed).is_err()
            );
        }
        assert!(
            authorize_public_github_json_read(root.path(), Path::new("/usr/bin/curl"), &sealed)
                .is_err()
        );
        spec.args[4] = "POST".into();
        assert!(spec.authorize_provider_read().is_err());
        assert!(
            authorize(
                root.path(),
                ExternalCapability::PreflightRead,
                "github:owner/repo",
                "/usr/bin/gh"
            )
            .is_err()
        );
    }
    #[test]
    fn probes_are_finite_and_do_not_grant_generic_execution() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for (program, args) in [
            ("/usr/bin/rustc", vec!["--version", "--verbose"]),
            ("/usr/bin/rustc", vec!["--print", "sysroot"]),
            ("/usr/bin/cargo", vec!["--version", "--verbose"]),
            ("/usr/bin/x86_64-linux-gnu-gcc-12", vec!["--version"]),
        ] {
            authorize_preflight_probe(root, Path::new(program), &args).unwrap();
        }
        for (program, args) in [
            ("rustc", vec!["--version", "--verbose"]),
            ("/usr/bin/rustc", vec!["--print", "sysroot", "source.rs"]),
            ("/usr/bin/cargo", vec!["install", "unknown"]),
            ("/usr/bin/sh", vec!["--version"]),
            ("/usr/bin/gcc-shell", vec!["--version"]),
            ("/usr/bin/cc", vec!["--version", "@args"]),
        ] {
            assert!(authorize_preflight_probe(root, Path::new(program), &args).is_err());
        }
        for capability in [
            ExternalCapability::PreflightProbe,
            ExternalCapability::PreflightRead,
        ] {
            assert!(authorize(root, capability, "readiness", "cargo").is_err());
            assert!(execute(root, capability, "readiness", "cargo", &["--version"], root).is_err());
        }
    }
}

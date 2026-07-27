//! CLI struct definitions for the Decapod command-line interface.
//!
//! All clap-derived types live here. Dispatch logic lives in `dispatch/`.

use crate::core::{constitution_cli, docs_cli, flight_recorder, obligation, todo, workunit};
use crate::plan_governance;
use crate::plugins::{
    aptitude, container, cron, decide, doctor, eval, federation, health, internalize, lcm, map_ops,
    policy, primitives, reflex, selective_test, verify, workflow,
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(
    name = "decapod",
    version = env!("CARGO_PKG_VERSION"),
    about = "Decapod is the daemonless, local-first control plane that agents call on demand to turn intent into context, then context into explicit specifications before inference, enforce boundaries, and produce proof-backed completion across concurrent multi-agent work. 🦀",
    disable_version_flag = true
)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ValidateCli {
    /// Store to validate: 'user' (blank-slate semantics) or 'repo' (dogfood backlog).
    #[clap(long, default_value = "repo")]
    pub store: String,
    /// Output format: 'text' or 'json'.
    #[clap(long, default_value = "text")]
    pub format: String,
    /// Print per-gate timing information.
    #[clap(long, short = 'v')]
    pub verbose: bool,
    /// Automatically refresh specs when staleness is detected
    #[clap(long)]
    pub refresh_specs: bool,
    /// Enable projection-consistency validation for Decapod-managed governance surfaces
    #[clap(long)]
    pub projections: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct AgentEvalCli {
    /// Prompt text to evaluate. Prefer --stdin when the prompt is untrusted.
    #[clap(long, conflicts_with = "stdin")]
    pub prompt: Option<String>,
    /// Read the untrusted prompt from stdin without interpreting it as shell input.
    #[clap(long, conflicts_with = "prompt")]
    pub stdin: bool,
    /// Output format: 'json' or 'text'.
    #[clap(long, default_value = "json", value_parser = ["json", "text"])]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct CapabilitiesCli {
    /// Output format: 'json' or 'text'.
    #[clap(long, default_value = "text")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct WorkspaceCli {
    #[clap(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WorkspaceCommand {
    /// Ensure an isolated workspace exists (create if needed)
    Ensure {
        /// Branch name. Mandatory: MUST contain the claimed Todo ID (e.g. agent/unknown/feat_01ks...) or hash.
        #[clap(long)]
        branch: Option<String>,
        /// Use a container for the workspace
        #[clap(long)]
        container: bool,
    },
    /// Show current workspace status
    Status,
    /// Publish workspace changes as a patch/PR bundle
    Publish {
        /// Title for the change
        #[clap(long)]
        title: Option<String>,
        /// Description for the change
        #[clap(long)]
        description: Option<String>,
    },
    /// Prune stale/unused agent workspaces
    Prune {
        /// Force removal of worktrees with local changes
        #[clap(long)]
        force: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct RpcCli {
    /// Operation to perform
    #[clap(long)]
    pub op: Option<String>,
    /// JSON parameters
    #[clap(long)]
    pub params: Option<String>,
    /// Read request from stdin instead of command line
    #[clap(long)]
    pub stdin: bool,
    /// Serve the transport-neutral RPC profile over authenticated HTTP.
    #[clap(long)]
    pub http_server: bool,
    /// HTTP bind address. Loopback is required unless --allow-remote is set.
    #[clap(long, default_value = "127.0.0.1:7331")]
    pub listen: String,
    /// Bearer token for the optional HTTP transport. May also be supplied by DECAPOD_HTTP_TOKEN.
    #[clap(long)]
    pub auth_token: Option<String>,
    /// Explicitly allow binding beyond loopback.
    #[clap(long)]
    pub allow_remote: bool,
    /// Maximum HTTP request body size.
    #[clap(long, default_value_t = 1_048_576)]
    pub max_body_bytes: usize,
}

// ===== Grouped Command Structures =====

#[derive(clap::Args, Debug)]
pub(crate) struct InitGroupCli {
    #[clap(subcommand)]
    pub command: Option<InitCommand>,
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    pub dir: Option<PathBuf>,
    /// Create this project directory if needed, enter it for initialization, and scaffold there.
    #[clap(long)]
    pub project_dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    pub force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    pub dry_run: bool,
    /// Generate project specs docs scaffolding under `.decapod/managed/specs/` (enabled by default).
    #[clap(long = "no-specs", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub specs: bool,
    /// Generate GitHub Action workflow for project validation (enabled by default).
    #[clap(long = "no-ci", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub ci: bool,
    /// Preferred diagram notation for generated `.decapod/managed/specs/ARCHITECTURE.md`.

    #[clap(long, value_enum, default_value_t = InitDiagramStyle::Ascii)]
    pub diagram_style: InitDiagramStyle,
    /// Force creation of all 4 entrypoint files (AGENTS.md, CLAUDE.md, GEMINI.md, CODEX.md).
    #[clap(long)]
    pub all: bool,
    /// Support non-interactive agent initialization for proof-gated flows.
    #[clap(long)]
    pub proof: bool,
    /// Create only CLAUDE.md entrypoint file.
    #[clap(long)]
    pub claude: bool,
    /// Create only GEMINI.md entrypoint file.
    #[clap(long)]
    pub gemini: bool,
    /// Create only CODEX.md entrypoint file.
    #[clap(long)]
    pub cdx_ep: bool,
    /// Create only AGENTS.md entrypoint file.
    #[clap(long)]
    pub agents: bool,
    /// Seed product name for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_name: Option<String>,
    /// Seed product summary/outcome for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_summary: Option<String>,
    /// Seed architecture direction for generated specs (non-interactive safe).
    #[clap(long)]
    pub architecture_direction: Option<String>,
    /// Seed product type for generated specs (e.g. service_or_library/application).
    #[clap(long)]
    pub product_type: Option<String>,
    /// Seed done criteria for generated specs (non-interactive safe).
    #[clap(long)]
    pub done_criteria: Option<String>,
    /// Seed primary languages (repeatable and/or comma-separated).
    #[clap(long = "primary-language", value_delimiter = ',')]
    pub primary_languages: Vec<String>,
    /// Seed detected surfaces (repeatable and/or comma-separated).
    #[clap(long = "surface", value_delimiter = ',')]
    pub detected_surfaces: Vec<String>,
    /// Seed declared capabilities (repeatable and/or comma-separated).
    #[clap(
        long = "declared-capability",
        alias = "capability",
        value_delimiter = ','
    )]
    pub declared_capabilities: Vec<String>,
    /// Enable container workspaces (enabled by default).
    ///
    /// WARNING: Disabling container workspaces is only safe for single-agent workflows.
    /// Multi-agent concurrent runs require container isolation to prevent environment
    /// corruption and race conditions. Only disable if you are the only agent working
    /// in this repository.
    #[clap(long = "no-container-workspaces", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub container_workspaces: bool,
    /// Protect repository-relative paths from agent mutations (repeatable/comma-separated).
    #[clap(long = "protected-path", value_delimiter = ',')]
    pub protected_paths: Vec<String>,
    /// Approval classifier names to enable (repeatable/comma-separated).
    #[clap(long = "approval-category", value_delimiter = ',')]
    pub approval_categories: Vec<String>,
    /// Configure the local isolation mode: worktree or container.
    #[clap(long, value_enum)]
    pub isolation_mode: Option<IsolationMode>,
    /// External tracker provider metadata (for example, beads).
    #[clap(long)]
    pub tracker_provider: Option<String>,
    /// External tracker project metadata (non-secret).
    #[clap(long)]
    pub tracker_project: Option<String>,
    /// External tracker URL metadata (non-secret).
    #[clap(long)]
    pub tracker_url: Option<String>,
    /// Repository-relative context source (repeatable/comma-separated).
    #[clap(long = "declared-context-source", value_delimiter = ',')]
    pub declared_context_sources: Vec<String>,
    /// Proof command in `name=command` form (repeatable/comma-separated).
    #[clap(long = "proof-command", value_delimiter = ',')]
    pub proof_commands: Vec<String>,
    /// Init storage backend: 'local' (default) or 'cloud' (experimental, account required).
    ///
    /// Cloud backend records non-secret Decapod Cloud intent in `.decapod/config.toml`.
    /// It does not perform login, provisioning, or sync during init.
    #[clap(long, value_enum, default_value_t = BackendType::Local)]
    pub backend: BackendType,
    /// Explicitly request local Git repository initialization (the default unless --no-git is set).
    #[clap(long = "git", action = clap::ArgAction::SetTrue)]
    pub git: bool,
    /// Skip local Git repository initialization.
    #[clap(long = "no-git", action = clap::ArgAction::SetTrue)]
    pub no_git: bool,
}

#[derive(Subcommand, Debug)]
pub(crate) enum InitCommand {
    /// Remove all Decapod files from repository
    Clean {
        /// Directory to clean (defaults to current working directory).
        #[clap(short, long)]
        dir: Option<PathBuf>,
    },
    /// Apply explicit init options (non-interactive).
    #[clap(alias = "wtih")]
    With(Box<InitWithCli>),
}

#[derive(clap::Args, Debug, Clone)]
pub(crate) struct InitWithCli {
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    pub dir: Option<PathBuf>,
    /// Create this project directory if needed, enter it for initialization, and scaffold there.
    #[clap(long)]
    pub project_dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    pub force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    pub dry_run: bool,
    /// Force creation of all 4 entrypoint files (AGENTS.md, CLAUDE.md, GEMINI.md, CODEX.md).
    #[clap(long)]
    pub all: bool,
    /// Support non-interactive agent initialization for proof-gated flows.
    #[clap(long)]
    pub proof: bool,
    /// Create only CLAUDE.md entrypoint file.
    #[clap(long)]
    pub claude: bool,
    /// Create only GEMINI.md entrypoint file.
    #[clap(long)]
    pub gemini: bool,
    /// Create only CODEX.md entrypoint file.
    #[clap(long)]
    pub cdx_ep: bool,
    /// Create only AGENTS.md entrypoint file.
    #[clap(long)]
    pub agents: bool,
    /// Generate project specs docs scaffolding under `.decapod/managed/specs/` (enabled by default).
    #[clap(long = "no-specs", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub specs: bool,
    /// Generate GitHub Action workflow for project validation (enabled by default).
    #[clap(long = "no-ci", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub ci: bool,
    /// Preferred diagram notation for generated `.decapod/managed/specs/ARCHITECTURE.md`.

    #[clap(long, value_enum, default_value_t = InitDiagramStyle::Ascii)]
    pub diagram_style: InitDiagramStyle,
    /// Seed product name for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_name: Option<String>,
    /// Seed product summary/outcome for generated specs (non-interactive safe).
    #[clap(long)]
    pub product_summary: Option<String>,
    /// Seed architecture direction for generated specs (non-interactive safe).
    #[clap(long)]
    pub architecture_direction: Option<String>,
    /// Seed product type for generated specs (e.g. service_or_library/application).
    #[clap(long)]
    pub product_type: Option<String>,
    /// Seed done criteria for generated specs (non-interactive safe).
    #[clap(long)]
    pub done_criteria: Option<String>,
    /// Seed primary languages (repeatable and/or comma-separated).
    #[clap(long = "primary-language", value_delimiter = ',')]
    pub primary_languages: Vec<String>,
    /// Seed detected surfaces (repeatable and/or comma-separated).
    #[clap(long = "surface", value_delimiter = ',')]
    pub detected_surfaces: Vec<String>,
    /// Seed declared capabilities (repeatable and/or comma-separated).
    #[clap(
        long = "declared-capability",
        alias = "capability",
        value_delimiter = ','
    )]
    pub declared_capabilities: Vec<String>,
    /// Enable container workspaces (enabled by default).
    ///
    /// WARNING: Disabling container workspaces is only safe for single-agent workflows.
    /// Multi-agent concurrent runs require container isolation to prevent environment
    /// corruption and race conditions. Only disable if you are the only agent working
    /// in this repository.
    #[clap(long = "no-container-workspaces", action = clap::ArgAction::SetFalse, default_value_t = true)]
    pub container_workspaces: bool,
    /// Protect repository-relative paths from agent mutations (repeatable/comma-separated).
    #[clap(long = "protected-path", value_delimiter = ',')]
    pub protected_paths: Vec<String>,
    /// Approval classifier names to enable (repeatable/comma-separated).
    #[clap(long = "approval-category", value_delimiter = ',')]
    pub approval_categories: Vec<String>,
    /// Configure the local isolation mode: worktree or container.
    #[clap(long, value_enum)]
    pub isolation_mode: Option<IsolationMode>,
    /// External tracker provider metadata (for example, beads).
    #[clap(long)]
    pub tracker_provider: Option<String>,
    /// External tracker project metadata (non-secret).
    #[clap(long)]
    pub tracker_project: Option<String>,
    /// External tracker URL metadata (non-secret).
    #[clap(long)]
    pub tracker_url: Option<String>,
    /// Repository-relative context source (repeatable/comma-separated).
    #[clap(long = "declared-context-source", value_delimiter = ',')]
    pub declared_context_sources: Vec<String>,
    /// Proof command in `name=command` form (repeatable/comma-separated).
    #[clap(long = "proof-command", value_delimiter = ',')]
    pub proof_commands: Vec<String>,
    /// Init storage backend: 'local' (default) or 'cloud' (experimental, account required).
    ///
    /// Cloud backend records non-secret Decapod Cloud intent in `.decapod/config.toml`.
    /// It does not perform login, provisioning, or sync during init.
    #[clap(long, value_enum, default_value_t = BackendType::Local)]
    pub backend: BackendType,
    /// Explicitly request local Git repository initialization (the default unless --no-git is set).
    #[clap(long = "git", action = clap::ArgAction::SetTrue)]
    pub git: bool,
    /// Skip local Git repository initialization.
    #[clap(long = "no-git", action = clap::ArgAction::SetTrue)]
    pub no_git: bool,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InitDiagramStyle {
    Ascii,
    Mermaid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecapodProjectConfig {
    pub schema_version: String,
    pub init: InitConfigSection,
    pub repo: RepoContext,
    #[serde(default)]
    pub governance: GovernanceConfig,
    #[serde(default)]
    pub proof: crate::core::proof::ProjectProofConfig,
    #[serde(default)]
    pub custody: CustodyConfig,
    #[serde(default)]
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub context: DeclaredContextConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitConfigSection {
    #[serde(default = "default_true")]
    pub specs: bool,
    #[serde(default = "default_true")]
    pub ci: bool,
    pub diagram_style: InitDiagramStyle,
    pub entrypoints: Vec<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    #[default]
    Local,
    Cloud,
}

impl BackendType {
    pub fn is_cloud(self) -> bool {
        matches!(self, Self::Cloud)
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum IsolationMode {
    Worktree,
    #[default]
    Container,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct GovernanceConfig {
    #[serde(default)]
    pub protected_paths: Vec<String>,
    #[serde(default)]
    pub approval_categories: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CustodyConfig {
    #[serde(default)]
    pub isolation: IsolationMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackerConfig {
    #[serde(default = "default_tracker_provider")]
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

fn default_tracker_provider() -> String {
    "decapod".to_string()
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            provider: default_tracker_provider(),
            project: None,
            url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeclaredContextConfig {
    #[serde(default)]
    pub declared_sources: Vec<String>,
}

/// Canonicalize a repository-relative path used by governance configuration.
/// Absolute paths and traversal are rejected before any consumer sees them.
pub fn canonical_repo_relative_path(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Err("path must not be empty".to_string());
    }
    let path = Path::new(&normalized);
    if path.is_absolute() {
        return Err(format!("path must be repository-relative: {raw}"));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("path must not escape the repository: {raw}"));
            }
        }
    }
    if parts.is_empty() {
        return Err(format!(
            "path must name a repository file or directory: {raw}"
        ));
    }
    Ok(parts.join("/"))
}

pub fn canonical_repo_relative_paths(raw: &[String]) -> Result<Vec<String>, String> {
    let mut paths = raw
        .iter()
        .map(|path| canonical_repo_relative_path(path))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    for pair in paths.windows(2) {
        if pair[0] == pair[1] {
            return Err(format!("duplicate repository-relative path: {}", pair[0]));
        }
    }
    Ok(paths)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudRuntimeConfig {
    #[serde(default = "default_cloud_provider")]
    pub provider: String,
    #[serde(default = "default_cloud_api_url")]
    pub api_url: String,
}

fn default_cloud_provider() -> String {
    "vercel".to_string()
}

fn default_cloud_api_url() -> String {
    std::env::var("DECAPOD_PROPODUS_API_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://project-oqn7i.vercel.app".to_string())
}

impl Default for CloudRuntimeConfig {
    fn default() -> Self {
        Self {
            provider: default_cloud_provider(),
            api_url: default_cloud_api_url(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "architecture_direction", alias = "architecture_intent")]
    pub architecture_direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub done_criteria: Option<String>,
    /// Repository base branch used for isolated workspaces and publication.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_languages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected_surfaces: Vec<String>,
    #[serde(default)]
    pub external_tracker: bool,
    #[serde(default = "default_container_workspaces_true")]
    pub container_workspaces: bool,
    /// Canonical repository backend selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendType>,
    #[serde(
        rename = "declared_capabilities",
        alias = "capabilities",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_validation: Option<MigrationValidationConfig>,
}

impl RepoContext {
    /// Resolve the configured repository backend.
    pub fn effective_backend(&self) -> BackendType {
        self.backend.unwrap_or_default()
    }

    /// Set the canonical repository backend.
    pub fn set_backend(&mut self, backend: BackendType) {
        self.backend = Some(backend);
    }
}

/// Human-governed executable proof for the persistent-state capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MigrationValidationConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub working_directory: Option<String>,
    #[serde(default = "default_migration_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub expected_exit_code: i32,
    #[serde(default)]
    pub evidence_path: Option<String>,
}

fn default_migration_timeout_seconds() -> u64 {
    60
}

fn default_container_workspaces_true() -> bool {
    true
}

impl DecapodProjectConfig {
    pub fn load(repo_root: &std::path::Path) -> Result<Self, crate::error::DecapodError> {
        let config_path = repo_root.join(".decapod").join("config.toml");
        if !config_path.exists() {
            return Err(crate::error::DecapodError::NotFound(
                "config.toml not found".to_string(),
            ));
        }
        let content = std::fs::read_to_string(config_path)?;
        let config: Self = toml::from_str(&content).map_err(|e| {
            crate::error::DecapodError::Config(format!("Failed to parse config.toml: {e}"))
        })?;
        Ok(config)
    }
}

impl Default for DecapodProjectConfig {
    fn default() -> Self {
        Self {
            schema_version: "1.0.0".to_string(),
            init: InitConfigSection {
                specs: true,
                ci: true,
                diagram_style: InitDiagramStyle::Ascii,
                entrypoints: vec![
                    "AGENTS.md".to_string(),
                    "CLAUDE.md".to_string(),
                    "GEMINI.md".to_string(),
                    "CODEX.md".to_string(),
                ],
            },
            repo: RepoContext::default(),
            governance: GovernanceConfig::default(),
            proof: crate::core::proof::ProjectProofConfig::default(),
            custody: CustodyConfig::default(),
            tracker: TrackerConfig::default(),
            context: DeclaredContextConfig::default(),
        }
    }
}

#[derive(clap::Args, Debug)]
pub(crate) struct SessionCli {
    #[clap(subcommand)]
    pub command: SessionCommand,
}

#[derive(clap::Args, Debug)]
pub(crate) struct CloudCli {
    /// Output format: text or json.
    #[clap(long, global = true, default_value = "text")]
    pub format: String,
    #[clap(subcommand)]
    pub command: CloudCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CloudCommand {
    /// Start or resume repository-bound GitHub authorization and save the machine session outside the repository.
    Login,
    /// Report whether a local or environment credential is available without printing it.
    Status,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SessionCommand {
    /// Acquire a new session token (required before using other commands)
    Acquire,
    /// Show current session status
    Status,
    /// Release the current session token
    Release,
    /// Bootstrap a governed work session with stubs and handshake artifact
    Init {
        /// Intended scope for this work session
        #[clap(long, default_value = "governed-work-session")]
        scope: String,
        /// Proof commands this session commits to run
        #[clap(long = "proof")]
        proofs: Vec<String>,
        /// Overwrite existing stubs if they already exist
        #[clap(long)]
        force: bool,
    },
    /// Deterministic agent handshake artifact (repo-native)
    Handshake(HandshakeCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct SetupCli {
    #[clap(subcommand)]
    pub command: SetupCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SetupCommand {
    /// Install or uninstall repository git hooks
    Hook {
        /// Install conventional commit message validation hook
        #[clap(long)]
        commit_msg: bool,
        /// Install Rust pre-commit hook (fmt + clippy)
        #[clap(long)]
        pre_commit: bool,
        /// Remove installed hooks
        #[clap(long)]
        uninstall: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct GovernCli {
    #[clap(subcommand)]
    pub command: GovernCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum GovernCommand {
    /// Risk classification and approvals
    Policy(policy::PolicyCli),

    /// Claims, proofs, and system health
    Health(health::HealthCli),

    /// Execute verification proofs
    Proof(ProofCommandCli),

    /// Run integrity watchlist checks
    Watcher(WatcherCli),

    /// Operator feedback and preferences
    Feedback(FeedbackCli),

    /// Workspace safety gates: path blocklist, diff size, secret scan, dangerous patterns
    Gatekeeper(GatekeeperCli),

    /// Inspect and repair the required publication governance artifacts
    Artifacts(ArtifactsCli),

    /// Plan-governed execution artifacts and gates
    Plan(PlanCli),

    /// Work unit manifest artifacts (intent/spec/state/proof chain)
    Workunit(WorkunitCli),

    /// Inspectable local-first agent run trajectory artifacts
    Trajectory(TrajectoryCli),

    /// Deterministic context capsule query over embedded constitution docs
    Capsule(CapsuleCli),

    /// STATE_COMMIT: prove and verify cryptographic state commitments
    StateCommit(StateCommitCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct ArtifactsCli {
    #[clap(subcommand)]
    pub command: ArtifactsCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ArtifactsCommand {
    /// Inventory plan, claims, trajectory, and validation artifacts
    Inventory {
        /// Base branch used to verify the PR diff (defaults to master, then main)
        #[clap(long = "base-branch")]
        base_branch: Option<String>,
        /// Create the claims ledger template when claims.json is absent
        #[clap(long)]
        repair: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct PlanCli {
    #[clap(subcommand)]
    pub command: PlanCommand,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum PlanStateArg {
    Draft,
    Annotating,
    Approved,
    Executing,
    Done,
}

impl From<PlanStateArg> for plan_governance::PlanState {
    fn from(value: PlanStateArg) -> Self {
        match value {
            PlanStateArg::Draft => Self::Draft,
            PlanStateArg::Annotating => Self::Annotating,
            PlanStateArg::Approved => Self::Approved,
            PlanStateArg::Executing => Self::Executing,
            PlanStateArg::Done => Self::Done,
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum PlanCommand {
    /// Initialize governed PLAN artifact
    Init {
        #[clap(long)]
        title: String,
        #[clap(long)]
        intent: String,
        #[clap(long = "todo-id")]
        todo_ids: Vec<String>,
        #[clap(long = "proof-hook")]
        proof_hooks: Vec<String>,
        #[clap(long = "unknown")]
        unknowns: Vec<String>,
        #[clap(long = "question")]
        human_questions: Vec<String>,
        #[clap(long = "stop-condition")]
        stop_conditions: Vec<String>,
        #[clap(long = "contradiction")]
        unresolved_contradictions: Vec<String>,
        #[clap(long = "deferred-question")]
        deferred_questions: Vec<String>,
        #[clap(long = "forbidden-path")]
        forbidden_paths: Vec<String>,
        #[clap(long)]
        file_touch_budget: Option<usize>,
    },
    /// Patch governed PLAN artifact
    Update {
        #[clap(long)]
        title: Option<String>,
        #[clap(long)]
        intent: Option<String>,
        #[clap(long = "todo-id")]
        todo_ids: Vec<String>,
        #[clap(long = "proof-hook")]
        proof_hooks: Vec<String>,
        #[clap(long = "unknown")]
        unknowns: Vec<String>,
        #[clap(long = "question")]
        human_questions: Vec<String>,
        #[clap(long = "stop-condition")]
        stop_conditions: Vec<String>,
        #[clap(long = "contradiction")]
        unresolved_contradictions: Vec<String>,
        #[clap(long = "deferred-question")]
        deferred_questions: Vec<String>,
        #[clap(long, default_value_t = false)]
        clear_unknowns: bool,
        #[clap(long, default_value_t = false)]
        clear_questions: bool,
        #[clap(long, default_value_t = false)]
        clear_stop_conditions: bool,
        #[clap(long, default_value_t = false)]
        clear_contradictions: bool,
        #[clap(long, default_value_t = false)]
        clear_deferred_questions: bool,
        #[clap(long = "forbidden-path")]
        forbidden_paths: Vec<String>,
        #[clap(long)]
        file_touch_budget: Option<usize>,
    },
    /// Set plan state
    SetState {
        #[clap(long, value_enum)]
        state: PlanStateArg,
    },
    /// Shortcut for setting plan state to APPROVED
    Approve,
    /// Display current plan artifact
    Status,
    /// Execute readiness check with typed pushback markers
    CheckExecute {
        #[clap(long)]
        todo_id: Option<String>,
    },
    /// Add a new phase to the plan
    PhaseAdd {
        #[clap(long)]
        id: String,
        #[clap(long)]
        name: String,
        #[clap(long)]
        description: String,
    },
    /// Update an existing phase
    PhaseUpdate {
        #[clap(long)]
        id: String,
        #[clap(long)]
        name: Option<String>,
        #[clap(long)]
        description: Option<String>,
    },
    /// List all phases in the plan
    PhaseList,
    /// Show details of a specific phase
    PhaseShow {
        #[clap(long)]
        id: String,
    },
    /// Add an entry gate to a phase
    AddEntryGate {
        #[clap(long)]
        phase_id: String,
        #[clap(long)]
        description: String,
    },
    /// Add an exit gate to a phase
    AddExitGate {
        #[clap(long)]
        phase_id: String,
        #[clap(long)]
        description: String,
    },
    /// Update a gate's description
    UpdateGate {
        #[clap(long)]
        phase_id: String,
        #[clap(long)]
        gate_index: usize,
        #[clap(long)]
        is_entry_gate: bool,
        #[clap(long)]
        description: String,
    },
    /// Mark a gate as satisfied
    SatisfyGate {
        #[clap(long)]
        phase_id: String,
        #[clap(long)]
        gate_index: usize,
        #[clap(long)]
        is_entry_gate: bool,
    },
    /// Enter a phase (verify entry gates are satisfied)
    EnterPhase {
        #[clap(long)]
        phase_id: String,
    },
    /// Exit a phase (verify exit gates are satisfied)
    ExitPhase {
        #[clap(long)]
        phase_id: String,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct WorkunitCli {
    #[clap(subcommand)]
    pub command: WorkunitCommand,
}

#[derive(clap::Args, Debug)]
pub(crate) struct TrajectoryCli {
    #[clap(subcommand)]
    pub command: TrajectoryCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TrajectoryCommand {
    /// Create a trajectory artifact for an agent run
    Init {
        #[clap(long)]
        run_id: String,
        #[clap(long)]
        task_id: Option<String>,
        /// Stable intent boundary shared by loop records; defaults to intent:<run-id>
        #[clap(long)]
        intent_id: Option<String>,
        #[clap(long)]
        original_intent: String,
        #[clap(long)]
        derived_intent: String,
        #[clap(long)]
        destination: Option<String>,
        #[clap(long = "phase")]
        current_phase: Option<String>,
        #[clap(long = "next-transition")]
        next_transitions: Vec<String>,
        #[clap(long = "blocker")]
        blockers: Vec<String>,
        #[clap(long = "boundary")]
        active_boundaries: Vec<String>,
        #[clap(long = "scope")]
        repo_scope: Vec<String>,
    },
    /// Record inspected files, actions, checks, assumptions, and completion claims
    Record {
        #[clap(long)]
        run_id: String,
        #[clap(long)]
        task_id: Option<String>,
        #[clap(long)]
        destination: Option<String>,
        #[clap(long = "phase")]
        current_phase: Option<String>,
        #[clap(long = "next-transition")]
        next_transitions: Vec<String>,
        #[clap(long = "blocker")]
        blockers: Vec<String>,
        #[clap(long, default_value_t = false)]
        clear_blockers: bool,
        #[clap(long = "boundary")]
        active_boundaries: Vec<String>,
        #[clap(long = "scope")]
        repo_scope: Vec<String>,
        #[clap(long = "inspected-file")]
        inspected_files: Vec<String>,
        #[clap(long = "modified-file")]
        modified_files: Vec<String>,
        #[clap(long = "command")]
        declared_commands: Vec<String>,
        #[clap(long = "tool-call")]
        tool_calls: Vec<String>,
        /// Typed trajectory loop record as a JSON object; repeat for attempts or nested loops
        #[clap(long = "loop-json")]
        loops: Vec<String>,
        /// Check in name=status form: passed, failed, partial, or unavailable
        #[clap(long = "check")]
        checks: Vec<String>,
        #[clap(long = "evidence")]
        evidence: Vec<String>,
        #[clap(long = "shortcut-signal")]
        shortcut_risk_signals: Vec<String>,
        #[clap(long = "assumption")]
        unresolved_assumptions: Vec<String>,
        #[clap(long)]
        completion_claim: Option<String>,
    },
    /// Inspect a trajectory artifact and its computed proof/verdict status
    Get {
        #[clap(long)]
        run_id: String,
    },
    /// Show compact trajectory proof status
    Status {
        #[clap(long)]
        run_id: String,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum WorkunitStatusArg {
    Draft,
    Executing,
    Claimed,
    Verified,
}

impl From<WorkunitStatusArg> for workunit::WorkUnitStatus {
    fn from(value: WorkunitStatusArg) -> Self {
        match value {
            WorkunitStatusArg::Draft => Self::Draft,
            WorkunitStatusArg::Executing => Self::Executing,
            WorkunitStatusArg::Claimed => Self::Claimed,
            WorkunitStatusArg::Verified => Self::Verified,
        }
    }
}

#[derive(Subcommand, Debug)]
pub(crate) enum WorkunitCommand {
    /// Initialize a work unit manifest for a task
    Init {
        #[clap(long)]
        task_id: String,
        #[clap(long)]
        intent_ref: String,
    },
    /// Get full work unit manifest JSON
    Get {
        #[clap(long)]
        task_id: String,
    },
    /// Show compact work unit status
    Status {
        #[clap(long)]
        task_id: String,
    },
    /// Attach a spec reference to a work unit
    AttachSpec {
        #[clap(long)]
        task_id: String,
        #[clap(long = "ref")]
        reference: String,
    },
    /// Attach a state reference to a work unit
    AttachState {
        #[clap(long)]
        task_id: String,
        #[clap(long = "ref")]
        reference: String,
    },
    /// Replace proof plan gates for a work unit
    SetProofPlan {
        #[clap(long)]
        task_id: String,
        #[clap(long = "gate")]
        gates: Vec<String>,
    },
    /// Record proof result for a gate
    RecordProof {
        #[clap(long)]
        task_id: String,
        #[clap(long)]
        gate: String,
        #[clap(long)]
        status: String,
        #[clap(long)]
        artifact: Option<String>,
    },
    /// Transition workunit status through governed state machine
    Transition {
        #[clap(long)]
        task_id: String,
        #[clap(long, value_enum)]
        to: WorkunitStatusArg,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct CapsuleCli {
    #[clap(subcommand)]
    pub command: CapsuleCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum CapsuleCommand {
    /// Query a deterministic context capsule from embedded docs
    Query {
        #[clap(long)]
        topic: String,
        #[clap(long)]
        scope: String,
        #[clap(long)]
        risk_tier: Option<String>,
        #[clap(long)]
        task_id: Option<String>,
        #[clap(long)]
        workunit_id: Option<String>,
        #[clap(long, default_value_t = 6)]
        limit: usize,
        #[clap(long, default_value_t = false)]
        write: bool,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct DataCli {
    #[clap(subcommand)]
    pub command: DataCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum DataCommand {
    /// Session archives (MOVE-not-TRIM)
    Archive(ArchiveCli),

    /// Repository knowledge base
    Knowledge(KnowledgeCli),

    /// Token budgets and context packing
    Context(ContextCli),

    /// Subsystem schemas and discovery
    Schema(SchemaCli),

    /// Repository structure and dependencies
    Repo(RepoCli),

    /// Audit log access (The Thin Waist)
    Broker(BrokerCli),

    /// Aptitude memory and preferences
    #[clap(aliases = ["memory"])]
    Aptitude(aptitude::AptitudeCli),

    /// Governed agent memory — typed knowledge graph
    Federation(federation::FederationCli),

    /// Markdown-native primitive layer
    Primitives(primitives::PrimitivesCli),

    /// Deterministic map operators — structured parallel processing
    Map(map_ops::MapCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct AutoCli {
    #[clap(subcommand)]
    pub command: AutoCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum AutoCommand {
    /// Scheduled tasks (time-based)
    Cron(cron::CronCli),

    /// Event-driven automation
    Reflex(reflex::ReflexCli),

    /// Workflow automation and discovery
    Workflow(workflow::WorkflowCli),

    /// Ephemeral isolated container execution
    Container(container::ContainerCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct QaCli {
    #[clap(subcommand)]
    pub command: QaCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum QaCommand {
    /// Inspect validation, proof, task, and workspace recovery state without mutating it.
    Diagnose(crate::plugins::verify::DiagnoseCli),

    /// Verify previously completed work (proof replay + drift checks)
    Verify(verify::VerifyCli),

    /// CI validation checks
    Check {
        /// Check crate description matches expected
        #[clap(long)]
        crate_description: bool,
        /// Smoke-check all discoverable command help surfaces
        #[clap(long)]
        commands: bool,
        /// Run all checks
        #[clap(long)]
        all: bool,
    },

    /// Run gatling regression test across all CLI code paths
    Gatling(crate::plugins::gatling::GatlingCli),

    /// Run only the integration tests affected by changed files.
    SelectiveTest(selective_test::SelectiveTestCli),

    /// Variance-aware evaluation artifacts and promotion gates
    Eval(Box<eval::EvalCli>),

    /// Run demonstrations of Decapod features
    Demo(DemoCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct HandshakeCli {
    /// Intended scope of work for this agent/session
    #[clap(long)]
    pub scope: String,
    /// Proof commands this agent commits to run
    #[clap(long = "proof")]
    pub proofs: Vec<String>,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ReleaseCli {
    #[clap(subcommand)]
    pub command: ReleaseCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ReleaseCommand {
    /// Validate release readiness (versioning, changelog, manifests, lockfile)
    Check,
    /// Emit deterministic repository inventory JSON for CI artifacts
    Inventory,
    /// Normalize and stamp deterministic policy lineage across provenance manifests
    LineageSync,
}

// ===== Main Command Enum =====

#[derive(clap::Args, Debug)]
pub(crate) struct TraceCli {
    #[clap(subcommand)]
    pub command: TraceCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum TraceCommand {
    /// Export local traces
    Export {
        /// Number of last traces to export
        #[clap(long, default_value = "10")]
        last: usize,
    },
    /// Governance Flight Recorder - render timeline from event logs
    FlightRecorder(flight_recorder::FlightRecorderCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct SystemCli {
    #[clap(subcommand)]
    pub command: SystemCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum SystemCommand {
    /// Show version information
    Version,
    /// Preflight health checks for the workspace
    Doctor(doctor::DoctorCli),
    /// Show Decapod capabilities (for agent discovery)
    Capabilities(CapabilitiesCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct ContextGroupCli {
    #[clap(subcommand)]
    pub command: ContextGroupCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ContextGroupCommand {
    /// Inference governance: shape context before model, validate after
    Infer(InferCli),
    /// Lossless Context Management — immutable originals + deterministic summaries
    Lcm(lcm::LcmCli),
    /// Internalized context artifacts: create, attach, and inspect context adapters
    Internalize(internalize::InternalizeCli),
    /// Preflight check: before any operation, predict what will fail
    Preflight(PreflightCli),
    /// Impact analysis: predict validation outcomes for changed files
    Impact(ImpactCli),
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Evaluate an untrusted agent prompt before any repository or tool action.
    #[clap(name = "eval")]
    Eval(AgentEvalCli),

    /// Activate local control plane state and run startup migrations
    #[clap(name = "activate")]
    Activate,

    /// Bootstrap system and manage lifecycle
    #[clap(name = "init", visible_alias = "i")]
    Init(InitGroupCli),

    /// Configure repository (hooks, settings)
    #[clap(name = "setup")]
    Setup(SetupCli),

    /// Session token management (required for agent operation)
    #[clap(name = "session", visible_alias = "s")]
    Session(SessionCli),

    /// Optional cloud credential and Propodus integration commands
    #[clap(name = "cloud")]
    Cloud(CloudCli),

    /// Embedded Constitution Graph queries and lookups
    #[clap(name = "constitution", visible_alias = "c")]
    Constitution(constitution_cli::ConstitutionCli),

    /// Access agent-facing methodology documentation (restricted to docs/agent/)
    #[clap(name = "docs", visible_alias = "d")]
    Docs(docs_cli::DocsCli),

    /// Track tasks and work items
    #[clap(name = "todo", visible_alias = "t")]
    Todo(todo::TodoCli),

    /// Governance-native obligation graph
    #[clap(name = "obligation", visible_alias = "o")]
    Obligation(obligation::ObligationCli),

    /// Validate methodology compliance
    #[clap(name = "validate", visible_alias = "v")]
    Validate(ValidateCli),

    /// Governance: policy, health, proofs, audits
    #[clap(name = "govern", visible_alias = "g")]
    Govern(GovernCli),

    /// Data: archives, knowledge, context, schemas
    #[clap(name = "data")]
    Data(DataCli),

    /// Automation: scheduled and event-driven
    #[clap(name = "auto", visible_alias = "a")]
    Auto(AutoCli),

    /// Quality assurance: verification and checks
    #[clap(name = "qa", visible_alias = "q")]
    Qa(QaCli),

    /// Architecture decision prompting
    #[clap(name = "decide")]
    Decide(decide::DecideCli),

    /// Agent workspace management
    #[clap(name = "workspace", visible_alias = "w")]
    Workspace(WorkspaceCli),

    /// Decapod-specific structured RPC interface for agents
    #[clap(name = "rpc")]
    Rpc(RpcCli),

    /// Release lifecycle checks and guards
    #[clap(name = "release")]
    Release(ReleaseCli),

    /// Show Decapod capabilities (for agent discovery)
    #[clap(name = "capabilities")]
    Capabilities(CapabilitiesCli),

    /// Inference governance: shape context before model, validate after
    #[clap(name = "infer")]
    Infer(InferCli),

    /// Local trace management
    #[clap(name = "trace")]
    Trace(TraceCli),

    /// System: capabilities, version, doctor
    #[clap(name = "system")]
    System(SystemCli),

    /// Context: infer, lcm, internalize, preflight, impact
    #[clap(name = "context")]
    Context(ContextGroupCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct BrokerCli {
    #[clap(subcommand)]
    pub command: BrokerCommand,
}

#[derive(clap::Args, Debug)]
pub(crate) struct StateCommitCli {
    #[clap(subcommand)]
    pub command: StateCommitCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum StateCommitCommand {
    /// Compute STATE_COMMIT for the current workspace
    Prove {
        /// Base commit SHA (required)
        #[clap(long)]
        base: String,
        /// Head commit SHA (defaults to current HEAD)
        #[clap(long)]
        head: Option<String>,
        /// Output file for scope_record.cbor
        #[clap(long, default_value = "scope_record.cbor")]
        output: PathBuf,
    },
    /// Verify a STATE_COMMIT matches current workspace
    Verify {
        /// Path to scope_record.cbor
        #[clap(long)]
        scope_record: PathBuf,
        /// Expected state_commit_root
        #[clap(long)]
        expected_root: Option<String>,
    },
    /// Explain the contents of a scope_record.cbor file
    Explain {
        /// Path to scope_record.cbor
        #[clap(long)]
        scope_record: PathBuf,
    },
}

#[derive(Subcommand, Debug)]
pub(crate) enum BrokerCommand {
    /// Show the audit log of brokered mutations.
    Audit,
    /// Verify audit log integrity and detect crash-induced divergence.
    Verify,
}

#[derive(clap::Args, Debug)]
pub(crate) struct KnowledgeCli {
    #[clap(subcommand)]
    pub command: KnowledgeCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum KnowledgeCommand {
    /// Add an entry to project knowledge
    Add {
        #[clap(long)]
        id: String,
        #[clap(long)]
        title: String,
        #[clap(long)]
        text: String,
        #[clap(long)]
        provenance: String,
        #[clap(long)]
        claim_id: Option<String>,
    },
    /// Search project knowledge
    Search {
        #[clap(long)]
        query: String,
    },
    /// Record explicit promotion of advisory/episodic knowledge into procedural class
    Promote {
        #[clap(long)]
        source_entry_id: String,
        #[clap(long = "evidence-ref")]
        evidence_refs: Vec<String>,
        #[clap(long)]
        approved_by: String,
        #[clap(long)]
        reason: String,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct RepoCli {
    #[clap(subcommand)]
    pub command: RepoCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum RepoCommand {
    /// Generate a deterministic summary of the repo
    Map,
    /// Generate a Markdown dependency graph (Mermaid format)
    Graph,
}

#[derive(clap::Args, Debug)]
pub(crate) struct WatcherCli {
    #[clap(subcommand)]
    pub command: WatcherCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum WatcherCommand {
    /// Run all checks in the watchlist
    Run,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ArchiveCli {
    #[clap(subcommand)]
    pub command: ArchiveCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ArchiveCommand {
    /// List all session archives
    List,
    /// Verify archive integrity (hashes and presence)
    Verify,
}

#[derive(clap::Args, Debug)]
pub(crate) struct FeedbackCli {
    #[clap(subcommand)]
    pub command: FeedbackCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum FeedbackCommand {
    /// Add operator feedback to the ledger
    Add {
        #[clap(long)]
        source: String,
        #[clap(long)]
        text: String,
        #[clap(long)]
        links: Option<String>,
    },
    /// Propose preference updates based on feedback
    Propose,
}

#[derive(clap::Args, Debug)]
pub(crate) struct GatekeeperCli {
    #[clap(subcommand)]
    pub command: GatekeeperCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum GatekeeperCommand {
    /// Check staged/changed files against safety gates
    Check {
        /// Paths to check (defaults to git staged files)
        #[clap(long)]
        paths: Option<Vec<String>>,
        /// Maximum diff size in bytes (default 10MB)
        #[clap(long)]
        max_diff_bytes: Option<u64>,
        /// Disable secret scanning
        #[clap(long)]
        no_secrets: bool,
        /// Disable dangerous pattern scanning
        #[clap(long)]
        no_dangerous: bool,
    },
}

#[derive(clap::Args, Debug)]
pub struct ProofCommandCli {
    #[clap(subcommand)]
    pub command: ProofSubCommand,
}

#[derive(Subcommand, Debug)]
pub enum ProofSubCommand {
    /// Run all configured proofs
    Run,
    /// Run a specific proof by name
    Test {
        #[clap(long)]
        name: String,
    },
    /// Show proof configuration and results
    List,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ContextCli {
    #[clap(subcommand)]
    pub command: ContextCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum ContextCommand {
    /// Audit current session token usage against profiles.
    Audit {
        #[clap(long)]
        profile: String,
        #[clap(long)]
        files: Vec<PathBuf>,
    },
    /// Perform MOVE-not-TRIM archival of a session file.
    Pack {
        #[clap(long)]
        path: PathBuf,
        #[clap(long)]
        summary: String,
    },
    /// Restore content from an archive (budget-gated)
    Restore {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "main")]
        profile: String,
        #[clap(long)]
        current_files: Vec<PathBuf>,
    },
}

#[derive(clap::Args, Debug)]
pub(crate) struct SchemaCli {
    /// Format: json | md
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Optional: filter by subsystem name
    #[clap(long)]
    pub subsystem: Option<String>,
    /// Force deterministic output (removes volatile timestamps)
    #[clap(long)]
    pub deterministic: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct PreflightCli {
    /// Operation to preflight (e.g., todo.add, validate, workspace.ensure)
    #[clap(long)]
    pub op: Option<String>,
    /// Output format: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Session ID to preflight for
    #[clap(long)]
    pub session: Option<String>,
}

#[derive(clap::Args, Debug)]
pub(crate) struct ImpactCli {
    /// Comma-separated list of changed files
    #[clap(long)]
    pub changed_files: Option<String>,
    /// Output format: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
    /// Predict mode: don't actually run gates, just predict
    #[clap(long)]
    pub predict: bool,
}

#[derive(clap::Args, Debug)]
pub(crate) struct InferCli {
    /// Subcommand: init, validate, or budget
    #[clap(subcommand)]
    pub command: InferCommand,
}

#[derive(Subcommand, Debug)]
pub(crate) enum InferCommand {
    /// Initialize inference context: returns selected_context, excluded_context, token_budget
    Init(InferInitCli),
    /// Return a precise orientation packet before agent inference
    Orientation(InferOrientationCli),
    /// Validate inference result against intent and proof expectations
    Validate(InferValidateCli),
    /// Estimate token budget for a given intent and context
    Budget(InferBudgetCli),
}

#[derive(clap::Args, Debug)]
pub(crate) struct InferOrientationCli {
    /// Task intent (what the human asked for)
    #[clap(long)]
    pub intent: Option<String>,
    /// Optional specific task ID
    #[clap(long)]
    pub task_id: Option<String>,
    /// Format output: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct InferInitCli {
    /// Task intent (what the human asked for)
    #[clap(long)]
    pub intent: String,
    /// Comma-separated list of relevant files/directories to consider
    #[clap(long)]
    pub context: Option<String>,
    /// Format output: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct InferValidateCli {
    /// The model output to validate
    #[clap(long)]
    pub result: String,
    /// Original intent
    #[clap(long)]
    pub intent: String,
    /// Format output: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct InferBudgetCli {
    /// Task intent
    #[clap(long)]
    pub intent: String,
    /// Files to include in context
    #[clap(long)]
    pub context: Option<String>,
    /// Format output: json | text
    #[clap(long, default_value = "json")]
    pub format: String,
}

#[derive(clap::Args, Debug)]
pub(crate) struct DemoCli {
    /// Demo to run: interlock
    #[clap(long, default_value = "interlock")]
    pub demo: String,
}

#[cfg(test)]
mod tests {
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
}

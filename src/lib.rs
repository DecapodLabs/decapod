//! Decapod: A Project OS for AI Agents
//!
//! **Decapod is a local-first control plane for agentic software engineering.**
//!
//! This is NOT a tool for humans to orchestrate. This IS a tool for AI agents to coordinate.
//! Humans steer via intent; agents execute via this orchestration layer.
//!
//! # Core Principles
//!
//! - **Local-first**: All state is local, versioned, and auditable
//! - **Deterministic**: Event-sourced stores enable reproducible replay
//! - **Agent-first**: Designed for machine consumption, not human UX
//! - **Constitution-driven**: Embedded methodology enforces contracts
//! - **Proof-gated**: Validation harness ensures methodology adherence
//!
//! # For AI Agents
//!
//! **You MUST:**
//! 1. Read the constitution first: `decapod docs show core/DECAPOD.md`
//! 2. Use the CLI exclusively: Never bypass `decapod` commands
//! 3. Validate before completion: `decapod validate` must pass
//! 4. Record proofs: `decapod proof run` for executable claims
//! 5. Track work: `decapod todo add` before multi-step tasks
//!
//! # Architecture
//!
//! ## Dual-Store Model
//!
//! - **User Store** (`~/.decapod/data/`): Agent-local, blank-slate semantics
//! - **Repo Store** (`<repo>/.decapod/data/`): Project-scoped, event-sourced, deterministic
//!
//! ## The Thin Waist
//!
//! All state mutations route through `DbBroker` for:
//! - Serialization (in-process lock)
//! - Audit logging (`broker.events.jsonl`)
//! - Intent tracking
//!
//! ## Subsystems (Plugins)
//!
//! - `todo`: Task tracking with event sourcing
//! - `health`: Proof-based claim status tracking
//! - `knowledge`: Structured knowledge with provenance
//! - `policy`: Approval gates for high-risk operations
//! - `watcher`: Read-only constitution compliance monitoring
//! - `archive`: Session archival with hash verification
//! - `context`: Multi-modal context packing for agents
//! - `cron`: Scheduled recurring tasks
//! - `reflex`: Event-triggered automation
//! - `feedback`: Agent-to-human proposal system
//! - `trust`: Trust score tracking for agents
//! - `heartbeat`: Liveness monitoring
//!
//! # Examples
//!
//! ```bash
//! # Initialize a Decapod project
//! decapod init
//!
//! # Read the methodology
//! decapod docs show core/DECAPOD.md
//!
//! # Add a task
//! decapod todo add "Implement feature X"
//!
//! # Run validation harness
//! decapod validate
//!
//! # Run proof checks
//! decapod proof run
//! ```
//!
//! # Crate Structure
//!
//! - [`core`]: Fundamental types and control plane (store, broker, proof, validate)
//! - [`plugins`]: Subsystem implementations (TODO, health, knowledge, etc.)

pub mod constitution;
pub mod core;
pub mod plugins;

use core::{
    db, docs, docs_cli, error, flight_recorder, migration, obligation, proof, repomap, scaffold,
    state_commit,
    store::{Store, StoreKind},
    todo, trace, validate,
};
use plugins::{
    archive, container, context, cron, decide, doctor, federation, feedback, health, knowledge,
    policy, primitives, reflex, teammate, verify, watcher, workflow,
};

use clap::{CommandFactory, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser, Debug)]
#[clap(
    name = "decapod",
    version = env!("CARGO_PKG_VERSION"),
    about = "The Intent-Driven Engineering System",
    disable_version_flag = true
)]
struct Cli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::Args, Debug)]
struct ValidateCli {
    /// Store to validate: 'user' (blank-slate semantics) or 'repo' (dogfood backlog).
    #[clap(long, default_value = "repo")]
    store: String,
    /// Output format: 'text' or 'json'.
    #[clap(long, default_value = "text")]
    format: String,
    /// Print per-gate timing information.
    #[clap(long, short = 'v')]
    verbose: bool,
}

#[derive(clap::Args, Debug)]
struct CapabilitiesCli {
    /// Output format: 'json' or 'text'.
    #[clap(long, default_value = "text")]
    format: String,
}

#[derive(clap::Args, Debug)]
struct WorkspaceCli {
    #[clap(subcommand)]
    command: WorkspaceCommand,
}

#[derive(Subcommand, Debug)]
enum WorkspaceCommand {
    /// Ensure an isolated workspace exists (create if needed)
    Ensure {
        /// Branch name (auto-generated if not provided)
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
}

#[derive(clap::Args, Debug)]
struct RpcCli {
    /// Operation to perform
    #[clap(long)]
    op: Option<String>,
    /// JSON parameters
    #[clap(long)]
    params: Option<String>,
    /// Read request from stdin instead of command line
    #[clap(long)]
    stdin: bool,
}

// ===== Grouped Command Structures =====

#[derive(clap::Args, Debug)]
struct InitGroupCli {
    #[clap(subcommand)]
    command: Option<InitCommand>,
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    dry_run: bool,
    /// Force creation of all 3 entrypoint files (GEMINI.md, AGENTS.md, CLAUDE.md).
    #[clap(long)]
    all: bool,
    /// Create only CLAUDE.md entrypoint file.
    #[clap(long)]
    claude: bool,
    /// Create only GEMINI.md entrypoint file.
    #[clap(long)]
    gemini: bool,
    /// Create only AGENTS.md entrypoint file.
    #[clap(long)]
    agents: bool,
}

#[derive(Subcommand, Debug)]
enum InitCommand {
    /// Remove all Decapod files from repository
    Clean {
        /// Directory to clean (defaults to current working directory).
        #[clap(short, long)]
        dir: Option<PathBuf>,
    },
}

#[derive(clap::Args, Debug)]
struct SessionCli {
    #[clap(subcommand)]
    command: SessionCommand,
}

#[derive(Subcommand, Debug)]
enum SessionCommand {
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
}

#[derive(clap::Args, Debug)]
struct SetupCli {
    #[clap(subcommand)]
    command: SetupCommand,
}

#[derive(Subcommand, Debug)]
enum SetupCommand {
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
struct GovernCli {
    #[clap(subcommand)]
    command: GovernCommand,
}

#[derive(Subcommand, Debug)]
enum GovernCommand {
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
}

#[derive(clap::Args, Debug)]
struct DataCli {
    #[clap(subcommand)]
    command: DataCommand,
}

#[derive(Subcommand, Debug)]
enum DataCommand {
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

    /// Teammate preferences and patterns
    Teammate(teammate::TeammateCli),

    /// Governed agent memory — typed knowledge graph
    Federation(federation::FederationCli),

    /// Markdown-native primitive layer
    Primitives(primitives::PrimitivesCli),
}

#[derive(clap::Args, Debug)]
struct AutoCli {
    #[clap(subcommand)]
    command: AutoCommand,
}

#[derive(Subcommand, Debug)]
enum AutoCommand {
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
struct QaCli {
    #[clap(subcommand)]
    command: QaCommand,
}

#[derive(Subcommand, Debug)]
enum QaCommand {
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
    Gatling(plugins::gatling::GatlingCli),
}

#[derive(clap::Args, Debug)]
struct HandshakeCli {
    /// Intended scope of work for this agent/session
    #[clap(long)]
    scope: String,
    /// Proof commands this agent commits to run
    #[clap(long = "proof")]
    proofs: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct ReleaseCli {
    #[clap(subcommand)]
    command: ReleaseCommand,
}

#[derive(Subcommand, Debug)]
enum ReleaseCommand {
    /// Validate release readiness (versioning, changelog, manifests, lockfile)
    Check,
}

// ===== Main Command Enum =====

#[derive(clap::Args, Debug)]
struct TraceCli {
    #[clap(subcommand)]
    command: TraceCommand,
}

#[derive(Subcommand, Debug)]
enum TraceCommand {
    /// Export local traces
    Export {
        /// Number of last traces to export
        #[clap(long, default_value = "10")]
        last: usize,
    },
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Bootstrap system and manage lifecycle
    #[clap(name = "init", visible_alias = "i")]
    Init(InitGroupCli),

    /// Configure repository (hooks, settings)
    #[clap(name = "setup")]
    Setup(SetupCli),

    /// Session token management (required for agent operation)
    #[clap(name = "session", visible_alias = "s")]
    Session(SessionCli),

    /// Access methodology documentation
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

    /// Show version information
    #[clap(name = "version")]
    Version,

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

    /// Structured JSON-RPC interface for agents
    #[clap(name = "rpc")]
    Rpc(RpcCli),

    /// Deterministic agent handshake artifact (repo-native)
    #[clap(name = "handshake")]
    Handshake(HandshakeCli),

    /// Release lifecycle checks and guards
    #[clap(name = "release")]
    Release(ReleaseCli),

    /// Show Decapod capabilities (for agent discovery)
    #[clap(name = "capabilities")]
    Capabilities(CapabilitiesCli),

    /// Local trace management
    #[clap(name = "trace")]
    Trace(TraceCli),

    /// Governance Flight Recorder - render timeline from event logs
    #[clap(name = "flight-recorder")]
    FlightRecorder(flight_recorder::FlightRecorderCli),

    /// STATE_COMMIT: prove and verify cryptographic state commitments
    #[clap(name = "state-commit")]
    StateCommit(StateCommitCli),

    /// Preflight health checks for the workspace
    #[clap(name = "doctor")]
    Doctor(doctor::DoctorCli),
}

#[derive(clap::Args, Debug)]
struct BrokerCli {
    #[clap(subcommand)]
    command: BrokerCommand,
}

#[derive(clap::Args, Debug)]
struct StateCommitCli {
    #[clap(subcommand)]
    command: StateCommitCommand,
}

#[derive(Subcommand, Debug)]
enum StateCommitCommand {
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
enum BrokerCommand {
    /// Show the audit log of brokered mutations.
    Audit,
    /// Verify audit log integrity and detect crash-induced divergence.
    Verify,
}

#[derive(clap::Args, Debug)]
struct KnowledgeCli {
    #[clap(subcommand)]
    command: KnowledgeCommand,
}

#[derive(Subcommand, Debug)]
enum KnowledgeCommand {
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
}

#[derive(clap::Args, Debug)]
struct RepoCli {
    #[clap(subcommand)]
    command: RepoCommand,
}

#[derive(Subcommand, Debug)]
enum RepoCommand {
    /// Generate a deterministic summary of the repo
    Map,
    /// Generate a Markdown dependency graph (Mermaid format)
    Graph,
}

#[derive(clap::Args, Debug)]
struct WatcherCli {
    #[clap(subcommand)]
    command: WatcherCommand,
}

#[derive(Subcommand, Debug)]
enum WatcherCommand {
    /// Run all checks in the watchlist
    Run,
}

#[derive(clap::Args, Debug)]
struct ArchiveCli {
    #[clap(subcommand)]
    command: ArchiveCommand,
}

#[derive(Subcommand, Debug)]
enum ArchiveCommand {
    /// List all session archives
    List,
    /// Verify archive integrity (hashes and presence)
    Verify,
}

#[derive(clap::Args, Debug)]
struct FeedbackCli {
    #[clap(subcommand)]
    command: FeedbackCommand,
}

#[derive(Subcommand, Debug)]
enum FeedbackCommand {
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
struct GatekeeperCli {
    #[clap(subcommand)]
    command: GatekeeperCommand,
}

#[derive(Subcommand, Debug)]
enum GatekeeperCommand {
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
struct ContextCli {
    #[clap(subcommand)]
    command: ContextCommand,
}

#[derive(Subcommand, Debug)]
enum ContextCommand {
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
struct SchemaCli {
    /// Format: json | md
    #[clap(long, default_value = "json")]
    format: String,
    /// Optional: filter by subsystem name
    #[clap(long)]
    subsystem: Option<String>,
    /// Force deterministic output (removes volatile timestamps)
    #[clap(long)]
    deterministic: bool,
}

fn find_decapod_project_root(start_dir: &Path) -> Result<PathBuf, error::DecapodError> {
    let mut current_dir = PathBuf::from(start_dir);
    loop {
        if current_dir.join(".decapod").exists() {
            return Ok(current_dir);
        }
        if !current_dir.pop() {
            return Err(error::DecapodError::NotFound(
                "'.decapod' directory not found in current or parent directories. Run `decapod init` first.".to_string(),
            ));
        }
    }
}

fn clean_project(dir: Option<PathBuf>) -> Result<(), error::DecapodError> {
    let raw_dir = match dir {
        Some(d) => d,
        None => std::env::current_dir()?,
    };
    let target_dir = std::fs::canonicalize(&raw_dir).map_err(error::DecapodError::IoError)?;

    let decapod_root = target_dir.join(".decapod");
    if decapod_root.exists() {
        println!("Removing directory: {}", decapod_root.display());
        fs::remove_dir_all(&decapod_root).map_err(error::DecapodError::IoError)?;
    }

    for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
        let path = target_dir.join(file);
        if path.exists() {
            println!("Removing file: {}", path.display());
            fs::remove_file(&path).map_err(error::DecapodError::IoError)?;
        }
    }
    println!("Decapod files cleaned from {}", target_dir.display());
    Ok(())
}

pub fn run() -> Result<(), error::DecapodError> {
    let cli = Cli::parse();
    let current_dir = std::env::current_dir()?;
    let decapod_root_option = find_decapod_project_root(&current_dir);
    let store_root: PathBuf;

    match cli.command {
        Command::Version => {
            // Version command - simple output for scripts/parsing
            println!("v{}", migration::DECAPOD_VERSION);
            return Ok(());
        }
        Command::Init(init_group) => {
            // Handle subcommands (clean)
            if let Some(subcmd) = init_group.command {
                match subcmd {
                    InitCommand::Clean { dir } => {
                        clean_project(dir)?;
                        return Ok(());
                    }
                }
            }

            // Base init command

            let target_dir = match init_group.dir {
                Some(d) => d,
                None => current_dir.clone(),
            };
            let target_dir =
                std::fs::canonicalize(&target_dir).map_err(error::DecapodError::IoError)?;

            // Check if .decapod exists and skip if it does, unless --force
            let setup_decapod_root = target_dir.join(".decapod");
            if setup_decapod_root.exists() && !init_group.force {
                println!(
                    "init: already initialized (.decapod exists); rerun with --force to overwrite"
                );
                return Ok(());
            }

            // Check which agent files exist and track which ones to generate
            use sha2::{Digest, Sha256};
            let mut existing_agent_files = vec![];
            for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md", "CODEX.md"] {
                if target_dir.join(file).exists() {
                    existing_agent_files.push(file);
                }
            }

            // Safely backup root agent entrypoint files if they exist and differ from templates
            let mut created_backups = false;
            let mut backup_count = 0usize;
            if !init_group.dry_run {
                for file in &existing_agent_files {
                    let path = target_dir.join(file);

                    // Get template content for this file
                    let template_content = core::assets::get_template(file).unwrap_or_default();

                    // Compute template checksum
                    let mut hasher = Sha256::new();
                    hasher.update(template_content.as_bytes());
                    let template_hash = format!("{:x}", hasher.finalize());

                    // Compute existing file checksum
                    let existing_content = fs::read_to_string(&path).unwrap_or_default();
                    let mut hasher = Sha256::new();
                    hasher.update(existing_content.as_bytes());
                    let existing_hash = format!("{:x}", hasher.finalize());

                    // Only backup if checksums differ
                    if template_hash != existing_hash {
                        created_backups = true;
                        backup_count += 1;
                        let backup_path = target_dir.join(format!("{}.bak", file));
                        fs::rename(&path, &backup_path).map_err(error::DecapodError::IoError)?;
                    }
                }
            }

            // Blend legacy agent entrypoints into OVERRIDE.md
            if !init_group.dry_run {
                scaffold::blend_legacy_entrypoints(&target_dir)?;
            }

            // Databases are created lazily on first use by runtime commands.
            // Init only generates the project structure files for speed.

            // Determine which agent files to generate based on flags
            // Individual flags override existing files list
            let mut agent_files_to_generate =
                if init_group.claude || init_group.gemini || init_group.agents {
                    let mut files = vec![];
                    if init_group.claude {
                        files.push("CLAUDE.md".to_string());
                    }
                    if init_group.gemini {
                        files.push("GEMINI.md".to_string());
                    }
                    if init_group.agents {
                        files.push("AGENTS.md".to_string());
                    }
                    files
                } else {
                    existing_agent_files
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect()
                };

            // AGENTS.md is mandatory whenever we are doing selective entrypoint generation.
            // Keep empty list semantics intact so scaffold can generate the full default set.
            if !agent_files_to_generate.is_empty()
                && !agent_files_to_generate.iter().any(|f| f == "AGENTS.md")
            {
                agent_files_to_generate.push("AGENTS.md".to_string());
            }

            let scaffold_summary =
                scaffold::scaffold_project_entrypoints(&scaffold::ScaffoldOptions {
                    target_dir,
                    force: init_group.force,
                    dry_run: init_group.dry_run,
                    agent_files: agent_files_to_generate,
                    created_backups,
                    all: init_group.all,
                })?;

            let target_display = setup_decapod_root
                .parent()
                .unwrap_or(current_dir.as_path())
                .display()
                .to_string();
            println!(
                "init: ok target={} mode={}",
                target_display,
                if init_group.dry_run {
                    "dry-run"
                } else {
                    "apply"
                }
            );
            println!(
                "init: files entry+{}={}~{} cfg+{}={}~{} backups={}",
                scaffold_summary.entrypoints_created,
                scaffold_summary.entrypoints_unchanged,
                scaffold_summary.entrypoints_preserved,
                scaffold_summary.config_created,
                scaffold_summary.config_unchanged,
                scaffold_summary.config_preserved,
                backup_count
            );
            println!("init: status=ready");
        }
        Command::Session(session_cli) => {
            run_session_command(session_cli)?;
        }
        Command::Release(release_cli) => {
            let project_root = decapod_root_option?;
            run_release_command(release_cli, &project_root)?;
        }
        Command::Setup(setup_cli) => match setup_cli.command {
            SetupCommand::Hook {
                commit_msg,
                pre_commit,
                uninstall,
            } => {
                run_hook_install(commit_msg, pre_commit, uninstall)?;
            }
        },
        _ => {
            let project_root = decapod_root_option?;
            let is_validate_cmd = matches!(&cli.command, Command::Validate(_));
            if requires_session_token(&cli.command) {
                ensure_session_valid()?;
            }
            enforce_worktree_requirement(&cli.command, &project_root)?;

            // For other commands, ensure .decapod exists
            let decapod_root_path = project_root.join(".decapod");
            store_root = decapod_root_path.join("data");
            std::fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;

            // Check for version/schema changes and run protected migrations if needed.
            // Backups are auto-created in .decapod/data only when schema upgrades are pending.
            let migration_result =
                migration::check_and_migrate_with_backup(&decapod_root_path, |data_root| {
                    use std::sync::Mutex;
                    let init_errors: Mutex<Vec<error::DecapodError>> = Mutex::new(Vec::new());
                    rayon::scope(|s| {
                        let errs = &init_errors;
                        // Bin 4: Transactional (TODO)
                        s.spawn(|_| {
                            if let Err(e) = todo::initialize_todo_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        // Bin 1: Governance
                        s.spawn(|_| {
                            if let Err(e) = health::initialize_health_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = policy::initialize_policy_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = feedback::initialize_feedback_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = archive::initialize_archive_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        // Bin 2: Memory
                        s.spawn(|_| {
                            if let Err(e) = db::initialize_knowledge_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = teammate::initialize_teammate_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = federation::initialize_federation_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = decide::initialize_decide_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        // Bin 3: Automation
                        s.spawn(|_| {
                            if let Err(e) = cron::initialize_cron_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                        s.spawn(|_| {
                            if let Err(e) = reflex::initialize_reflex_db(data_root) {
                                errs.lock().unwrap().push(e);
                            }
                        });
                    });
                    let errs = init_errors.into_inner().unwrap();
                    if let Some(e) = errs.into_iter().next() {
                        return Err(e);
                    }
                    Ok(())
                });
            match migration_result {
                Ok(()) => {}
                Err(e) if is_validate_cmd => return Err(normalize_validate_error(e)),
                Err(e) => return Err(e),
            }

            let project_store = Store {
                kind: StoreKind::Repo,
                root: store_root.clone(),
            };

            if should_auto_clock_in(&cli.command) {
                todo::clock_in_agent_presence(&project_store)?;
            }

            match cli.command {
                Command::Validate(validate_cli) => {
                    run_validate_command(validate_cli, &project_root, &project_store)?;
                }
                Command::Version => show_version_info()?,
                Command::Docs(docs_cli) => {
                    let result = docs_cli::run_docs_cli(docs_cli)?;
                    if result.ingested_core_constitution {
                        mark_core_constitution_ingested(&project_root)?;
                    }
                }
                Command::Todo(todo_cli) => todo::run_todo_cli(&project_store, todo_cli)?,
                Command::Obligation(obligation_cli) => {
                    obligation::run_obligation_cli(&project_store, obligation_cli)?
                }
                Command::Govern(govern_cli) => {
                    run_govern_command(govern_cli, &project_store, &store_root)?;
                }
                Command::Data(data_cli) => {
                    run_data_command(data_cli, &project_store, &project_root, &store_root)?;
                }
                Command::Auto(auto_cli) => run_auto_command(auto_cli, &project_store)?,
                Command::Qa(qa_cli) => run_qa_command(qa_cli, &project_store, &project_root)?,
                Command::Decide(decide_cli) => decide::run_decide_cli(&project_store, decide_cli)?,
                Command::Workspace(workspace_cli) => {
                    run_workspace_command(workspace_cli, &project_root)?;
                }
                Command::Rpc(rpc_cli) => {
                    run_rpc_command(rpc_cli, &project_root)?;
                }
                Command::Handshake(handshake_cli) => {
                    run_handshake_command(handshake_cli, &project_root)?;
                }
                Command::Release(release_cli) => {
                    run_release_command(release_cli, &project_root)?;
                }
                Command::Capabilities(cap_cli) => {
                    run_capabilities_command(cap_cli)?;
                }
                Command::Trace(trace_cli) => {
                    run_trace_command(trace_cli, &project_root)?;
                }
                Command::FlightRecorder(fr_cli) => {
                    flight_recorder::run_flight_recorder_cli(&project_store, fr_cli)?;
                }
                Command::StateCommit(sc_cli) => {
                    run_state_commit_command(sc_cli, &project_root)?;
                }
                Command::Doctor(doctor_cli) => {
                    doctor::run_doctor_cli(&project_store, &project_root, doctor_cli)?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

fn should_auto_clock_in(command: &Command) -> bool {
    match command {
        Command::Todo(todo_cli) => !todo::is_heartbeat_command(todo_cli),
        Command::Version
        | Command::Init(_)
        | Command::Setup(_)
        | Command::Session(_)
        | Command::Release(_)
        | Command::StateCommit(_)
        | Command::Doctor(_) => false,
        _ => true,
    }
}

fn command_requires_worktree(command: &Command) -> bool {
    match command {
        Command::Init(_)
        | Command::Setup(_)
        | Command::Session(_)
        | Command::Version
        | Command::Workspace(_)
        | Command::Capabilities(_)
        | Command::Trace(_)
        | Command::FlightRecorder(_)
        | Command::Docs(_)
        | Command::Handshake(_)
        | Command::Release(_)
        | Command::Todo(_)
        | Command::StateCommit(_)
        | Command::Doctor(_) => false,
        Command::Data(data_cli) => !matches!(data_cli.command, DataCommand::Schema(_)),
        Command::Rpc(_) => false,
        _ => true,
    }
}

fn enforce_worktree_requirement(
    command: &Command,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        return Ok(());
    }
    if !command_requires_worktree(command) {
        return Ok(());
    }

    let status = crate::core::workspace::get_workspace_status(project_root)?;
    if status.git.in_worktree {
        return Ok(());
    }

    Err(error::DecapodError::ValidationError(format!(
        "Command requires isolated git worktree; current checkout is not a worktree (branch='{}'). Run `decapod workspace ensure --branch agent/<id>/<topic>` and execute from the reported worktree path.",
        status.git.current_branch
    )))
}

fn rpc_op_requires_worktree(op: &str) -> bool {
    !matches!(
        op,
        "agent.init"
            | "workspace.status"
            | "workspace.ensure"
            | "assurance.evaluate"
            | "mentor.obligations"
            | "context.resolve"
            | "context.bindings"
            | "schema.get"
            | "store.upsert"
            | "store.query"
            | "validate.run"
            | "standards.resolve"
    )
}

fn enforce_worktree_requirement_for_rpc(
    op: &str,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        return Ok(());
    }
    if !rpc_op_requires_worktree(op) {
        return Ok(());
    }

    let status = crate::core::workspace::get_workspace_status(project_root)?;
    if status.git.in_worktree {
        return Ok(());
    }

    Err(error::DecapodError::ValidationError(format!(
        "RPC op '{}' requires isolated git worktree; current checkout is not a worktree (branch='{}'). Run `decapod workspace ensure --branch agent/<id>/<topic>` and execute from the reported worktree path.",
        op, status.git.current_branch
    )))
}

fn rpc_op_bypasses_session(op: &str) -> bool {
    matches!(
        op,
        "agent.init"
            | "context.resolve"
            | "context.bindings"
            | "schema.get"
            | "store.upsert"
            | "store.query"
            | "validate.run"
            | "workspace.status"
            | "workspace.ensure"
            | "standards.resolve"
    )
}

fn requires_session_token(command: &Command) -> bool {
    match command {
        // Bootstrap/session lifecycle + version + capabilities are sessionless.
        Command::Init(_)
        | Command::Session(_)
        | Command::Version
        | Command::Docs(_)
        | Command::Capabilities(_)
        | Command::Release(_)
        | Command::Trace(_)
        | Command::FlightRecorder(_)
        | Command::StateCommit(_)
        | Command::Doctor(_) => false,
        Command::Data(DataCli {
            command: DataCommand::Schema(_),
        }) => false,
        Command::Rpc(rpc_cli) => {
            if let Some(ref op) = rpc_cli.op {
                !rpc_op_bypasses_session(op)
            } else {
                // If op is not provided via flag, we'll check it after parsing JSON in run_rpc_command
                false
            }
        }
        _ => true,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentSessionRecord {
    agent_id: String,
    token: String,
    password_hash: String,
    issued_at_epoch_secs: u64,
    expires_at_epoch_secs: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ConstitutionalAwarenessRecord {
    agent_id: String,
    session_token: Option<String>,
    initialized_at_epoch_secs: u64,
    validated_at_epoch_secs: Option<u64>,
    core_constitution_ingested_at_epoch_secs: Option<u64>,
    context_resolved_at_epoch_secs: Option<u64>,
    source_ops: Vec<String>,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn session_ttl_secs() -> u64 {
    std::env::var("DECAPOD_SESSION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(3600)
}

fn current_agent_id() -> String {
    std::env::var("DECAPOD_AGENT_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn sanitize_agent_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn sessions_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".decapod")
        .join("generated")
        .join("sessions")
}

fn session_file_for_agent(project_root: &Path, agent_id: &str) -> PathBuf {
    sessions_dir(project_root).join(format!("{}.json", sanitize_agent_component(agent_id)))
}

fn awareness_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(".decapod")
        .join("generated")
        .join("awareness")
}

fn awareness_file_for_agent(project_root: &Path, agent_id: &str) -> PathBuf {
    awareness_dir(project_root).join(format!("{}.json", sanitize_agent_component(agent_id)))
}

fn hash_password(password: &str, token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

fn generate_ephemeral_password() -> Result<String, error::DecapodError> {
    let mut buf = vec![0u8; 24];
    let mut urandom = fs::File::open("/dev/urandom").map_err(error::DecapodError::IoError)?;
    urandom
        .read_exact(&mut buf)
        .map_err(error::DecapodError::IoError)?;
    let mut out = String::with_capacity(buf.len() * 2);
    for b in buf {
        out.push_str(&format!("{:02x}", b));
    }
    Ok(out)
}

fn read_agent_session(
    project_root: &Path,
    agent_id: &str,
) -> Result<Option<AgentSessionRecord>, error::DecapodError> {
    let path = session_file_for_agent(project_root, agent_id);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let rec: AgentSessionRecord = serde_json::from_str(&raw)
        .map_err(|e| error::DecapodError::SessionError(format!("invalid session file: {}", e)))?;
    Ok(Some(rec))
}

fn write_agent_session(
    project_root: &Path,
    rec: &AgentSessionRecord,
) -> Result<(), error::DecapodError> {
    let dir = sessions_dir(project_root);
    fs::create_dir_all(&dir).map_err(error::DecapodError::IoError)?;
    let path = session_file_for_agent(project_root, &rec.agent_id);
    let body = serde_json::to_string_pretty(rec)
        .map_err(|e| error::DecapodError::SessionError(format!("session encode error: {}", e)))?;
    fs::write(&path, body).map_err(error::DecapodError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .map_err(error::DecapodError::IoError)?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn clear_agent_awareness(project_root: &Path, agent_id: &str) -> Result<(), error::DecapodError> {
    let path = awareness_file_for_agent(project_root, agent_id);
    if path.exists() {
        fs::remove_file(path).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn read_awareness_record(
    project_root: &Path,
    agent_id: &str,
) -> Result<Option<ConstitutionalAwarenessRecord>, error::DecapodError> {
    let path = awareness_file_for_agent(project_root, agent_id);
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
    let rec: ConstitutionalAwarenessRecord = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid constitutional awareness record: {}",
            e
        ))
    })?;
    Ok(Some(rec))
}

fn write_awareness_record(
    project_root: &Path,
    rec: &ConstitutionalAwarenessRecord,
) -> Result<(), error::DecapodError> {
    let dir = awareness_dir(project_root);
    fs::create_dir_all(&dir).map_err(error::DecapodError::IoError)?;
    let path = awareness_file_for_agent(project_root, &rec.agent_id);
    let body = serde_json::to_string_pretty(rec).map_err(|e| {
        error::DecapodError::ValidationError(format!("awareness encode error: {}", e))
    })?;
    fs::write(&path, body).map_err(error::DecapodError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&path)
            .map_err(error::DecapodError::IoError)?
            .permissions();
        perms.set_mode(0o600);
        fs::set_permissions(&path, perms).map_err(error::DecapodError::IoError)?;
    }
    Ok(())
}

fn mark_constitution_initialized(project_root: &Path) -> Result<(), error::DecapodError> {
    let agent_id = current_agent_id();
    let session_token = read_agent_session(project_root, &agent_id)?.map(|s| s.token);
    let now = now_epoch_secs();
    let existing = read_awareness_record(project_root, &agent_id)?;
    let mut source_ops = existing
        .as_ref()
        .map(|r| r.source_ops.clone())
        .unwrap_or_default();
    if !source_ops.iter().any(|op| op == "agent.init") {
        source_ops.push("agent.init".to_string());
    }
    let rec = ConstitutionalAwarenessRecord {
        agent_id,
        session_token,
        initialized_at_epoch_secs: now,
        validated_at_epoch_secs: existing.as_ref().and_then(|r| r.validated_at_epoch_secs),
        core_constitution_ingested_at_epoch_secs: existing
            .as_ref()
            .and_then(|r| r.core_constitution_ingested_at_epoch_secs),
        context_resolved_at_epoch_secs: existing.and_then(|r| r.context_resolved_at_epoch_secs),
        source_ops,
    };
    write_awareness_record(project_root, &rec)
}

fn mark_constitution_context_resolved(project_root: &Path) -> Result<(), error::DecapodError> {
    let agent_id = current_agent_id();
    let mut rec =
        read_awareness_record(project_root, &agent_id)?.unwrap_or(ConstitutionalAwarenessRecord {
            agent_id: agent_id.clone(),
            session_token: read_agent_session(project_root, &agent_id)?.map(|s| s.token),
            initialized_at_epoch_secs: now_epoch_secs(),
            validated_at_epoch_secs: None,
            core_constitution_ingested_at_epoch_secs: None,
            context_resolved_at_epoch_secs: None,
            source_ops: Vec::new(),
        });
    rec.context_resolved_at_epoch_secs = Some(now_epoch_secs());
    if !rec.source_ops.iter().any(|op| op == "context.resolve") {
        rec.source_ops.push("context.resolve".to_string());
    }
    write_awareness_record(project_root, &rec)
}

fn mark_validation_completed(project_root: &Path) -> Result<(), error::DecapodError> {
    let agent_id = current_agent_id();
    let mut rec =
        read_awareness_record(project_root, &agent_id)?.unwrap_or(ConstitutionalAwarenessRecord {
            agent_id: agent_id.clone(),
            session_token: read_agent_session(project_root, &agent_id)?.map(|s| s.token),
            initialized_at_epoch_secs: now_epoch_secs(),
            validated_at_epoch_secs: None,
            core_constitution_ingested_at_epoch_secs: None,
            context_resolved_at_epoch_secs: None,
            source_ops: Vec::new(),
        });
    rec.validated_at_epoch_secs = Some(now_epoch_secs());
    if !rec.source_ops.iter().any(|op| op == "validate") {
        rec.source_ops.push("validate".to_string());
    }
    write_awareness_record(project_root, &rec)
}

fn mark_core_constitution_ingested(project_root: &Path) -> Result<(), error::DecapodError> {
    let agent_id = current_agent_id();
    let mut rec =
        read_awareness_record(project_root, &agent_id)?.unwrap_or(ConstitutionalAwarenessRecord {
            agent_id: agent_id.clone(),
            session_token: read_agent_session(project_root, &agent_id)?.map(|s| s.token),
            initialized_at_epoch_secs: now_epoch_secs(),
            validated_at_epoch_secs: None,
            core_constitution_ingested_at_epoch_secs: None,
            context_resolved_at_epoch_secs: None,
            source_ops: Vec::new(),
        });
    rec.core_constitution_ingested_at_epoch_secs = Some(now_epoch_secs());
    if !rec.source_ops.iter().any(|op| op == "docs.ingest") {
        rec.source_ops.push("docs.ingest".to_string());
    }
    write_awareness_record(project_root, &rec)
}

fn cleanup_expired_sessions(
    project_root: &Path,
    store_root: &Path,
) -> Result<Vec<String>, error::DecapodError> {
    let dir = sessions_dir(project_root);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let now = now_epoch_secs();
    let mut expired_agents = Vec::new();
    for entry in fs::read_dir(&dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(v) => v,
            Err(_) => {
                let _ = fs::remove_file(&path);
                continue;
            }
        };
        let rec: AgentSessionRecord = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => {
                let _ = fs::remove_file(&path);
                continue;
            }
        };
        if rec.expires_at_epoch_secs <= now {
            let _ = fs::remove_file(&path);
            expired_agents.push(rec.agent_id);
        }
    }

    if !expired_agents.is_empty() {
        todo::cleanup_stale_agent_assignments(store_root, &expired_agents, "session.expired")?;
        for agent_id in &expired_agents {
            let _ = clear_agent_awareness(project_root, agent_id);
        }
    }

    Ok(expired_agents)
}

fn ensure_session_valid() -> Result<(), error::DecapodError> {
    let current_dir = std::env::current_dir()?;
    let project_root = find_decapod_project_root(&current_dir)?;
    let store_root = project_root.join(".decapod").join("data");
    fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;
    let _ = cleanup_expired_sessions(&project_root, &store_root)?;

    let agent_id = current_agent_id();
    let session = read_agent_session(&project_root, &agent_id)?;
    let Some(session) = session else {
        return Err(error::DecapodError::SessionError(format!(
            "No active session for agent '{}'. Run 'decapod session acquire' first. Reminder: this CLI/API is not for humans.",
            agent_id
        )));
    };

    if session.expires_at_epoch_secs <= now_epoch_secs() {
        let _ = fs::remove_file(session_file_for_agent(&project_root, &agent_id));
        let _ = todo::cleanup_stale_agent_assignments(
            &store_root,
            std::slice::from_ref(&agent_id),
            "session.expired",
        );
        return Err(error::DecapodError::SessionError(format!(
            "Session expired for agent '{}'. Run 'decapod session acquire' to rotate credentials.",
            agent_id
        )));
    }

    if agent_id == "unknown" {
        return Ok(());
    }

    let supplied_password = std::env::var("DECAPOD_SESSION_PASSWORD").map_err(|_| {
        error::DecapodError::SessionError(
            "Missing DECAPOD_SESSION_PASSWORD. Agent+password is required for session access."
                .to_string(),
        )
    })?;
    let supplied_hash = hash_password(&supplied_password, &session.token);
    if supplied_hash != session.password_hash {
        return Err(error::DecapodError::SessionError(
            "Invalid DECAPOD_SESSION_PASSWORD for current agent session.".to_string(),
        ));
    }
    Ok(())
}

fn run_session_command(session_cli: SessionCli) -> Result<(), error::DecapodError> {
    let current_dir = std::env::current_dir()?;
    let project_root = find_decapod_project_root(&current_dir)?;
    let store_root = project_root.join(".decapod").join("data");
    fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;
    let _ = cleanup_expired_sessions(&project_root, &store_root)?;

    match session_cli.command {
        SessionCommand::Acquire => {
            let agent_id = current_agent_id();
            if let Some(existing) = read_agent_session(&project_root, &agent_id)?
                && existing.expires_at_epoch_secs > now_epoch_secs()
            {
                println!(
                    "Session already active for agent '{}'. Use 'decapod session status' for details.",
                    agent_id
                );
                return Ok(());
            }

            let issued = now_epoch_secs();
            let expires = issued.saturating_add(session_ttl_secs());
            let token = ulid::Ulid::to_string(&ulid::Ulid::new());
            let password = generate_ephemeral_password()?;
            let rec = AgentSessionRecord {
                agent_id: agent_id.clone(),
                token: token.clone(),
                password_hash: hash_password(&password, &token),
                issued_at_epoch_secs: issued,
                expires_at_epoch_secs: expires,
            };
            write_agent_session(&project_root, &rec)?;
            clear_agent_awareness(&project_root, &agent_id)?;

            println!("Session acquired successfully.");
            println!("Agent: {}", agent_id);
            println!("Token: {}", token);
            println!("Password: {}", password);
            println!("ExpiresAtEpoch: {}", expires);
            println!(
                "Export before running other commands: DECAPOD_AGENT_ID='{}' and DECAPOD_SESSION_PASSWORD='<password>'",
                rec.agent_id
            );
            println!("\nYou may now use other decapod commands.");
            Ok(())
        }
        SessionCommand::Status => {
            let agent_id = current_agent_id();
            if let Some(session) = read_agent_session(&project_root, &agent_id)? {
                println!("Session active");
                println!("Agent: {}", session.agent_id);
                println!("Token: {}", session.token);
                println!("IssuedAtEpoch: {}", session.issued_at_epoch_secs);
                println!("ExpiresAtEpoch: {}", session.expires_at_epoch_secs);
            } else {
                println!("No active session");
                println!("Run 'decapod session acquire' to start a session");
            }
            Ok(())
        }
        SessionCommand::Release => {
            let agent_id = current_agent_id();
            let session_path = session_file_for_agent(&project_root, &agent_id);
            if session_path.exists() {
                std::fs::remove_file(&session_path).map_err(error::DecapodError::IoError)?;
                clear_agent_awareness(&project_root, &agent_id)?;
                let _ = todo::cleanup_stale_agent_assignments(
                    &store_root,
                    std::slice::from_ref(&agent_id),
                    "session.release",
                );
                println!("Session released");
            } else {
                println!("No active session to release");
            }
            Ok(())
        }
        SessionCommand::Init {
            scope,
            mut proofs,
            force,
        } => {
            if proofs.is_empty() {
                proofs.push("decapod validate".to_string());
            }
            run_session_init(&project_root, &scope, &proofs, force)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandshakeArtifact {
    schema_version: String,
    request_id: String,
    agent_id: String,
    repo_version: String,
    scope: String,
    proofs: Vec<String>,
    declared_docs: Vec<String>,
    doc_hashes: serde_json::Value,
    artifact_hash: String,
}

fn hash_bytes_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    format!("{:x}", hasher.finalize())
}

fn required_handshake_docs() -> Vec<&'static str> {
    vec![
        "CLAUDE.md",
        "AGENTS.md",
        "constitution/core/DECAPOD.md",
        "constitution/interfaces/CONTROL_PLANE.md",
    ]
}

fn build_handshake_artifact(
    project_root: &Path,
    scope: &str,
    proofs: &[String],
) -> Result<HandshakeArtifact, error::DecapodError> {
    let mut doc_hashes = serde_json::Map::new();
    let required_docs = required_handshake_docs();
    for rel in &required_docs {
        let abs = project_root.join(rel);
        if !abs.exists() {
            return Err(error::DecapodError::ValidationError(format!(
                "Handshake requires `{}` to exist.",
                rel
            )));
        }
        let bytes = fs::read(&abs).map_err(error::DecapodError::IoError)?;
        doc_hashes.insert(
            (*rel).to_string(),
            serde_json::json!(hash_bytes_hex(&bytes)),
        );
    }

    let request_id = ulid::Ulid::new().to_string();
    let mut unsigned = serde_json::json!({
        "schema_version": "1.0.0",
        "request_id": request_id,
        "agent_id": current_agent_id(),
        "repo_version": migration::DECAPOD_VERSION,
        "scope": scope,
        "proofs": proofs,
        "declared_docs": required_docs,
        "doc_hashes": doc_hashes,
    });
    let canonical = serde_json::to_vec(&unsigned).map_err(|e| {
        error::DecapodError::ValidationError(format!("Failed to encode handshake artifact: {e}"))
    })?;
    let artifact_hash = hash_bytes_hex(&canonical);
    unsigned["artifact_hash"] = serde_json::json!(artifact_hash);

    serde_json::from_value(unsigned).map_err(|e| {
        error::DecapodError::ValidationError(format!("Failed to finalize handshake artifact: {e}"))
    })
}

fn write_handshake_artifact(
    project_root: &Path,
    artifact: &HandshakeArtifact,
) -> Result<PathBuf, error::DecapodError> {
    let dir = project_root
        .join(".decapod")
        .join("records")
        .join("handshakes");
    fs::create_dir_all(&dir).map_err(error::DecapodError::IoError)?;
    let file = format!(
        "{}-{}.json",
        crate::core::time::now_epoch_z(),
        artifact.agent_id.replace('/', "_")
    );
    let path = dir.join(file);
    let pretty = serde_json::to_vec_pretty(artifact).map_err(|e| {
        error::DecapodError::ValidationError(format!("Failed to serialize handshake record: {e}"))
    })?;
    fs::write(&path, pretty).map_err(error::DecapodError::IoError)?;
    Ok(path)
}

fn run_handshake_command(
    cli: HandshakeCli,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    if cli.proofs.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "Handshake requires at least one `--proof` declaration.".to_string(),
        ));
    }
    let artifact = build_handshake_artifact(project_root, &cli.scope, &cli.proofs)?;
    let path = write_handshake_artifact(project_root, &artifact)?;
    println!(
        "{}",
        serde_json::json!({
            "cmd": "handshake",
            "status": "ok",
            "path": path,
            "artifact_hash": artifact.artifact_hash,
            "repo_version": artifact.repo_version,
            "scope": artifact.scope,
            "proofs": artifact.proofs,
        })
    );
    Ok(())
}

fn run_session_init(
    project_root: &Path,
    scope: &str,
    proofs: &[String],
    force: bool,
) -> Result<(), error::DecapodError> {
    let mut created = Vec::new();
    let mut skipped = Vec::new();

    let tasks_dir = project_root.join("tasks");
    fs::create_dir_all(&tasks_dir).map_err(error::DecapodError::IoError)?;

    let todo_path = tasks_dir.join("todo.md");
    let todo_stub = "\
# Work Session Plan

- Task: <replace-with-task-id-and-title>
- Scope: <replace-with-scope>
- Constraints: keep daemonless, repo-native, proof-gated

## Required Constitution Links
- constitution/core/DECAPOD.md
- constitution/interfaces/CONTROL_PLANE.md
- constitution/specs/SECURITY.md

## Proof Plan
- decapod validate
";
    write_stub(&todo_path, todo_stub, force, &mut created, &mut skipped)?;

    let intent_path = project_root.join("INTENT.md");
    let intent_stub = "\
# INTENT

## Problem
<what outcome is required>

## Constraints
- daemonless
- repo-native canonical state
- deterministic reducers and proof gates

## Acceptance Proofs
- decapod validate
";
    write_stub(&intent_path, intent_stub, force, &mut created, &mut skipped)?;

    let handshake_path = project_root.join("HANDSHAKE.md");
    let handshake_stub = "\
# HANDSHAKE

- Agent: <agent-id>
- Scope: <scope>
- Proofs: <proof-list>
- Record: `.decapod/records/handshakes/<latest>.json`
";
    write_stub(
        &handshake_path,
        handshake_stub,
        force,
        &mut created,
        &mut skipped,
    )?;

    let artifact = build_handshake_artifact(project_root, scope, proofs)?;
    let artifact_path = write_handshake_artifact(project_root, &artifact)?;

    println!(
        "{}",
        serde_json::json!({
            "cmd": "session.init",
            "status": "ok",
            "created": created,
            "skipped": skipped,
            "handshake_record": artifact_path,
            "template_refs": [
                "templates/INTENT.md",
                "templates/SPEC.md",
                "templates/ADR.md",
                "templates/CLAIM_NODE.md",
                "templates/DRIFT_ROW.md"
            ]
        })
    );
    Ok(())
}

fn write_stub(
    path: &Path,
    content: &str,
    force: bool,
    created: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<(), error::DecapodError> {
    if path.exists() && !force {
        skipped.push(path.display().to_string());
        return Ok(());
    }
    fs::write(path, content).map_err(error::DecapodError::IoError)?;
    created.push(path.display().to_string());
    Ok(())
}

fn run_release_command(cli: ReleaseCli, project_root: &Path) -> Result<(), error::DecapodError> {
    match cli.command {
        ReleaseCommand::Check => run_release_check(project_root),
    }
}

fn run_release_check(project_root: &Path) -> Result<(), error::DecapodError> {
    let mut failures = Vec::new();
    let changelog = project_root.join("CHANGELOG.md");
    let migrations = project_root.join("docs").join("MIGRATIONS.md");
    let cargo_lock = project_root.join("Cargo.lock");
    let cargo_toml = project_root.join("Cargo.toml");
    let rpc_golden_req = project_root.join("tests/golden/rpc/v1/agent_init.request.json");
    let rpc_golden_res = project_root.join("tests/golden/rpc/v1/agent_init.response.json");
    let artifact_manifest = project_root.join("artifacts/provenance/artifact_manifest.json");
    let proof_manifest = project_root.join("artifacts/provenance/proof_manifest.json");

    if !changelog.exists() {
        failures.push("CHANGELOG.md missing".to_string());
    } else {
        let raw = fs::read_to_string(&changelog).map_err(error::DecapodError::IoError)?;
        if !raw.contains("## [Unreleased]") {
            failures.push("CHANGELOG.md missing `## [Unreleased]` section".to_string());
        }
    }
    if !migrations.exists() {
        failures.push("docs/MIGRATIONS.md missing".to_string());
    }
    if !cargo_lock.exists() {
        failures.push("Cargo.lock missing (locked builds required)".to_string());
    }
    if !cargo_toml.exists() {
        failures.push("Cargo.toml missing".to_string());
    }
    if !rpc_golden_req.exists() || !rpc_golden_res.exists() {
        failures.push("RPC golden vectors missing under tests/golden/rpc/v1".to_string());
    }
    if !artifact_manifest.exists() {
        failures.push(
            "artifact provenance manifest missing: artifacts/provenance/artifact_manifest.json"
                .to_string(),
        );
    }
    if !proof_manifest.exists() {
        failures.push(
            "proof provenance manifest missing: artifacts/provenance/proof_manifest.json"
                .to_string(),
        );
    }
    if artifact_manifest.exists()
        && let Err(e) = validate_artifact_manifest(project_root, &artifact_manifest)
    {
        failures.push(format!("artifact manifest invalid: {}", e));
    }
    if proof_manifest.exists()
        && let Err(e) = validate_proof_manifest(&proof_manifest)
    {
        failures.push(format!("proof manifest invalid: {}", e));
    }

    if !failures.is_empty() {
        return Err(error::DecapodError::ValidationError(format!(
            "release.check failed:\n- {}",
            failures.join("\n- ")
        )));
    }

    println!(
        "{}",
        serde_json::json!({
            "cmd": "release.check",
            "status": "ok",
            "checks": [
                "changelog.unreleased",
                "migrations.doc",
                "cargo.lock.present",
                "rpc.golden_vectors.present",
                "provenance.manifests.verified"
            ]
        })
    );
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, error::DecapodError> {
    let bytes = fs::read(path).map_err(error::DecapodError::IoError)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_artifact_manifest(
    project_root: &Path,
    manifest_path: &Path,
) -> Result<(), error::DecapodError> {
    let raw = fs::read_to_string(manifest_path).map_err(error::DecapodError::IoError)?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!("artifact manifest is not valid JSON: {e}"))
    })?;
    if v.get("schema_version").and_then(|x| x.as_str()) != Some("1.0.0") {
        return Err(error::DecapodError::ValidationError(
            "artifact manifest schema_version must be 1.0.0".to_string(),
        ));
    }
    if v.get("kind").and_then(|x| x.as_str()) != Some("artifact_manifest") {
        return Err(error::DecapodError::ValidationError(
            "artifact manifest kind must be artifact_manifest".to_string(),
        ));
    }

    let artifacts = v
        .get("artifacts")
        .and_then(|x| x.as_array())
        .ok_or_else(|| {
            error::DecapodError::ValidationError(
                "artifact manifest artifacts[] required".to_string(),
            )
        })?;
    if artifacts.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "artifact manifest artifacts[] must not be empty".to_string(),
        ));
    }

    for entry in artifacts {
        let path = entry.get("path").and_then(|x| x.as_str()).ok_or_else(|| {
            error::DecapodError::ValidationError("artifact entry missing path".to_string())
        })?;
        let sha = entry
            .get("sha256")
            .and_then(|x| x.as_str())
            .ok_or_else(|| {
                error::DecapodError::ValidationError("artifact entry missing sha256".to_string())
            })?;
        if sha.is_empty() || sha.contains("TO_BE_FILLED") {
            return Err(error::DecapodError::ValidationError(format!(
                "artifact entry '{}' has placeholder sha256",
                path
            )));
        }
        let abs = project_root.join(path);
        if !abs.exists() {
            return Err(error::DecapodError::ValidationError(format!(
                "artifact entry '{}' does not exist",
                path
            )));
        }
        let actual = sha256_file(&abs)?;
        if actual != sha {
            return Err(error::DecapodError::ValidationError(format!(
                "artifact entry '{}' sha256 mismatch",
                path
            )));
        }
    }
    Ok(())
}

fn validate_proof_manifest(manifest_path: &Path) -> Result<(), error::DecapodError> {
    let raw = fs::read_to_string(manifest_path).map_err(error::DecapodError::IoError)?;
    let v: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!("proof manifest is not valid JSON: {e}"))
    })?;
    if v.get("schema_version").and_then(|x| x.as_str()) != Some("1.0.0") {
        return Err(error::DecapodError::ValidationError(
            "proof manifest schema_version must be 1.0.0".to_string(),
        ));
    }
    if v.get("kind").and_then(|x| x.as_str()) != Some("proof_manifest") {
        return Err(error::DecapodError::ValidationError(
            "proof manifest kind must be proof_manifest".to_string(),
        ));
    }
    let proofs = v.get("proofs").and_then(|x| x.as_array()).ok_or_else(|| {
        error::DecapodError::ValidationError("proof manifest proofs[] required".to_string())
    })?;
    if proofs.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "proof manifest proofs[] must not be empty".to_string(),
        ));
    }
    for p in proofs {
        let command = p.get("command").and_then(|x| x.as_str()).unwrap_or("");
        let result = p.get("result").and_then(|x| x.as_str()).unwrap_or("");
        if command.is_empty() || command.contains("TO_BE_FILLED") {
            return Err(error::DecapodError::ValidationError(
                "proof manifest command must be non-empty and non-placeholder".to_string(),
            ));
        }
        if result.is_empty() || result.contains("TO_BE_FILLED") {
            return Err(error::DecapodError::ValidationError(
                "proof manifest result must be non-empty and non-placeholder".to_string(),
            ));
        }
    }
    let env = v
        .get("environment")
        .and_then(|x| x.as_object())
        .ok_or_else(|| {
            error::DecapodError::ValidationError("proof manifest environment required".to_string())
        })?;
    for key in ["os", "rust"] {
        let value = env.get(key).and_then(|x| x.as_str()).unwrap_or("");
        if value.is_empty() || value.contains("TO_BE_FILLED") {
            return Err(error::DecapodError::ValidationError(format!(
                "proof manifest environment.{} must be non-empty and non-placeholder",
                key
            )));
        }
    }
    Ok(())
}

fn run_validate_command(
    validate_cli: ValidateCli,
    project_root: &Path,
    project_store: &Store,
) -> Result<(), error::DecapodError> {
    use crate::core::workspace;

    if std::env::var("DECAPOD_VALIDATE_SKIP_GIT_GATES").is_ok() {
        // Skip workspace check if gates are explicitly skipped
    } else {
        // FIRST: Check workspace enforcement (non-negotiable)
        let workspace_status = workspace::get_workspace_status(project_root)?;

        if !workspace_status.can_work {
            let blocker = workspace_status
                .blockers
                .first()
                .expect("Workspace should have a blocker if can_work is false");

            let response = serde_json::json!({
                "success": false,
                "gate": "workspace_protection",
                "error": blocker.message,
                "resolve_hint": blocker.resolve_hint,
                "branch": workspace_status.git.current_branch,
                "is_protected": workspace_status.git.is_protected,
                "in_container": workspace_status.container.in_container,
            });

            if validate_cli.format == "json" {
                println!("{}", serde_json::to_string_pretty(&response).unwrap());
            } else {
                eprintln!("❌ VALIDATION FAILED: Workspace Protection Gate");
                eprintln!("   Error: {}", blocker.message);
                eprintln!("   Hint: {}", blocker.resolve_hint);
            }

            return Err(error::DecapodError::ValidationError(
                "Workspace protection gate failed".to_string(),
            ));
        }
    }

    let decapod_root = project_root.to_path_buf();
    let store = match validate_cli.store.as_str() {
        "user" => {
            // User store uses a temp directory for blank-slate validation
            let tmp_root =
                std::env::temp_dir().join(format!("decapod_validate_user_{}", ulid::Ulid::new()));
            std::fs::create_dir_all(&tmp_root).map_err(error::DecapodError::IoError)?;
            Store {
                kind: StoreKind::User,
                root: tmp_root,
            }
        }
        _ => project_store.clone(),
    };

    run_validation_bounded(&store, &decapod_root, validate_cli.verbose)?;
    mark_validation_completed(project_root)?;
    Ok(())
}

fn validate_timeout_secs() -> u64 {
    std::env::var("DECAPOD_VALIDATE_TIMEOUT_SECS")
        .ok()
        .or_else(|| std::env::var("DECAPOD_VALIDATE_TIMEOUT_SECONDS").ok())
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(30)
}

fn normalize_validate_error(err: error::DecapodError) -> error::DecapodError {
    match err {
        error::DecapodError::RusqliteError(rusqlite::Error::SqliteFailure(code, msg)) => {
            let is_lock = code.code == rusqlite::ErrorCode::DatabaseBusy
                || code.extended_code == 522
                || msg
                    .as_deref()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("locked");
            if is_lock {
                return error::DecapodError::ValidationError(
                    "VALIDATE_TIMEOUT_OR_LOCK: SQLite contention detected. Retry with backoff or inspect concurrent decapod processes.".to_string(),
                );
            }
            error::DecapodError::RusqliteError(rusqlite::Error::SqliteFailure(code, msg))
        }
        error::DecapodError::ValidationError(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("database is locked")
                || lower.contains("databasebusy")
                || lower.contains("sqlite_code=databasebusy")
            {
                return error::DecapodError::ValidationError(
                    "VALIDATE_TIMEOUT_OR_LOCK: SQLite contention detected. Retry with backoff or inspect concurrent decapod processes.".to_string(),
                );
            }
            error::DecapodError::ValidationError(message)
        }
        other => other,
    }
}

fn run_validation_bounded(
    store: &Store,
    project_root: &Path,
    verbose: bool,
) -> Result<(), error::DecapodError> {
    let timeout_secs = validate_timeout_secs();
    let (tx, rx) = mpsc::channel();
    let store_cloned = store.clone();
    let root = project_root.to_path_buf();

    std::thread::spawn(move || {
        let result = validate::run_validation(&store_cloned, &root, &root, verbose);
        let _ = tx.send(result);
    });

    match rx.recv_timeout(std::time::Duration::from_secs(timeout_secs)) {
        Ok(result) => result.map_err(normalize_validate_error),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(error::DecapodError::ValidationError(format!(
            "VALIDATE_TIMEOUT_OR_LOCK: validate exceeded timeout ({}s). Terminated to preserve proof-gate liveness.",
            timeout_secs
        ))),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(error::DecapodError::ValidationError(
            "VALIDATE_TIMEOUT_OR_LOCK: validate worker disconnected unexpectedly.".to_string(),
        )),
    }
}

fn rpc_op_requires_constitutional_awareness(op: &str) -> bool {
    matches!(
        op,
        "workspace.publish"
            | "store.upsert"
            | "scaffold.apply_answer"
            | "scaffold.generate_artifacts"
    )
}

fn enforce_constitutional_awareness_for_rpc(
    op: &str,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    if !rpc_op_requires_constitutional_awareness(op) {
        return Ok(());
    }

    let agent_id = current_agent_id();
    let rec = read_awareness_record(project_root, &agent_id)?;
    let Some(rec) = rec else {
        return Err(error::DecapodError::ValidationError(
            "Constitutional awareness required before mutating operations. Run `decapod validate`, then `decapod docs ingest`, then `decapod session acquire`, `decapod rpc --op agent.init`, and `decapod rpc --op context.resolve`."
                .to_string(),
        ));
    };

    if rec.validated_at_epoch_secs.is_none() {
        return Err(error::DecapodError::ValidationError(
            "Constitutional awareness incomplete: `decapod validate` has not completed for this agent context. Run `decapod validate` first."
                .to_string(),
        ));
    }

    if rec.core_constitution_ingested_at_epoch_secs.is_none() {
        return Err(error::DecapodError::ValidationError(
            "Constitutional awareness incomplete: core constitution ingestion missing. Run `decapod docs ingest` to ingest `constitution/core/*.md` before mutating operations."
                .to_string(),
        ));
    }

    if rec.context_resolved_at_epoch_secs.is_none() {
        return Err(error::DecapodError::ValidationError(
            "Constitutional awareness incomplete: `context.resolve` has not been executed after initialization. Run `decapod rpc --op context.resolve`."
                .to_string(),
        ));
    }

    if let Some(session) = read_agent_session(project_root, &agent_id)?
        && rec.session_token.as_deref() != Some(session.token.as_str())
    {
        return Err(error::DecapodError::ValidationError(
            "Constitutional awareness is stale for the active session. Re-run `decapod rpc --op agent.init` and `decapod rpc --op context.resolve`."
                .to_string(),
        ));
    }

    Ok(())
}

fn run_govern_command(
    govern_cli: GovernCli,
    project_store: &Store,
    store_root: &Path,
) -> Result<(), error::DecapodError> {
    match govern_cli.command {
        GovernCommand::Policy(policy_cli) => policy::run_policy_cli(project_store, policy_cli)?,
        GovernCommand::Health(health_cli) => health::run_health_cli(project_store, health_cli)?,
        GovernCommand::Proof(proof_cli) => proof::execute_proof_cli(&proof_cli, store_root)?,
        GovernCommand::Watcher(watcher_cli) => match watcher_cli.command {
            WatcherCommand::Run => {
                let report = watcher::run_watcher(project_store)?;
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            }
        },
        GovernCommand::Feedback(feedback_cli) => {
            feedback::initialize_feedback_db(store_root)?;
            match feedback_cli.command {
                FeedbackCommand::Add {
                    source,
                    text,
                    links,
                } => {
                    let id =
                        feedback::add_feedback(project_store, &source, &text, links.as_deref())?;
                    println!("Feedback recorded: {}", id);
                }
                FeedbackCommand::Propose => {
                    let proposal = feedback::propose_prefs(project_store)?;
                    println!("{}", proposal);
                }
            }
        }
        GovernCommand::Gatekeeper(gk_cli) => match gk_cli.command {
            GatekeeperCommand::Check {
                paths,
                max_diff_bytes,
                no_secrets,
                no_dangerous,
            } => {
                use crate::core::gatekeeper;

                let repo_root = project_store
                    .root
                    .parent()
                    .and_then(|p| p.parent())
                    .unwrap_or(&project_store.root);

                // Collect paths: explicit or git staged files
                let check_paths: Vec<std::path::PathBuf> = if let Some(explicit) = paths {
                    explicit.into_iter().map(std::path::PathBuf::from).collect()
                } else {
                    // Get staged files from git
                    let output = std::process::Command::new("git")
                        .args(["diff", "--cached", "--name-only"])
                        .current_dir(repo_root)
                        .output()
                        .map_err(error::DecapodError::IoError)?;
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(std::path::PathBuf::from)
                        .collect()
                };

                // Get diff size
                let diff_output = std::process::Command::new("git")
                    .args(["diff", "--cached", "--stat"])
                    .current_dir(repo_root)
                    .output()
                    .map_err(error::DecapodError::IoError)?;
                let diff_bytes = diff_output.stdout.len() as u64;

                let mut config = gatekeeper::GatekeeperConfig::default();
                if let Some(max) = max_diff_bytes {
                    config.max_diff_bytes = max;
                }
                config.scan_secrets = !no_secrets;
                config.scan_dangerous_patterns = !no_dangerous;

                let result =
                    gatekeeper::run_gatekeeper(repo_root, &check_paths, diff_bytes, &config)?;

                if result.passed {
                    println!(
                        "Gatekeeper: all checks passed ({} files scanned)",
                        check_paths.len()
                    );
                } else {
                    println!(
                        "Gatekeeper: {} violation(s) found:",
                        result.violations.len()
                    );
                    for v in &result.violations {
                        let loc = v.line.map(|l| format!(":{}", l)).unwrap_or_default();
                        println!("  [{}] {}{}: {}", v.kind, v.path.display(), loc, v.message);
                    }
                    return Err(error::DecapodError::ValidationError(format!(
                        "Gatekeeper: {} violation(s)",
                        result.violations.len()
                    )));
                }
            }
        },
    }

    Ok(())
}

fn run_data_command(
    data_cli: DataCli,
    project_store: &Store,
    project_root: &Path,
    store_root: &Path,
) -> Result<(), error::DecapodError> {
    match data_cli.command {
        DataCommand::Archive(archive_cli) => {
            archive::initialize_archive_db(store_root)?;
            match archive_cli.command {
                ArchiveCommand::List => {
                    let items = archive::list_archives(project_store)?;
                    println!("{}", serde_json::to_string_pretty(&items).unwrap());
                }
                ArchiveCommand::Verify => {
                    let failures = archive::verify_archives(project_store)?;
                    if failures.is_empty() {
                        println!("All archives verified successfully.");
                    } else {
                        println!("Archive verification failed:");
                        for f in failures {
                            println!("- {}", f);
                        }
                    }
                }
            }
        }
        DataCommand::Knowledge(knowledge_cli) => {
            db::initialize_knowledge_db(store_root)?;
            match knowledge_cli.command {
                KnowledgeCommand::Add {
                    id,
                    title,
                    text,
                    provenance,
                    claim_id,
                } => {
                    let result = knowledge::add_knowledge(
                        project_store,
                        knowledge::AddKnowledgeParams {
                            id: &id,
                            title: &title,
                            content: &text,
                            provenance: &provenance,
                            claim_id: claim_id.as_deref(),
                            merge_key: None,
                            conflict_policy: knowledge::KnowledgeConflictPolicy::Merge,
                            status: "active",
                            ttl_policy: "persistent",
                            expires_ts: None,
                        },
                    )?;
                    println!(
                        "Knowledge entry {}: {} (action: {})",
                        result.id, id, result.action
                    );
                }
                KnowledgeCommand::Search { query } => {
                    let results = knowledge::search_knowledge(
                        project_store,
                        &query,
                        knowledge::SearchOptions {
                            as_of: None,
                            window_days: None,
                            rank: "relevance",
                        },
                    )?;
                    println!("{}", serde_json::to_string_pretty(&results).unwrap());
                }
            }
        }
        DataCommand::Context(context_cli) => {
            let manager = context::ContextManager::new(store_root)?;
            match context_cli.command {
                ContextCommand::Audit { profile, files } => {
                    let total = manager.audit_session(&files)?;
                    match manager.get_profile(&profile) {
                        Some(p) => {
                            println!(
                                "Total tokens for profile '{}': {} / {} (budget)",
                                profile, total, p.budget_tokens
                            );
                            if total > p.budget_tokens {
                                println!("⚠ OVER BUDGET");
                            }
                        }
                        None => {
                            println!("Total tokens: {} (Profile '{}' not found)", total, profile);
                        }
                    }
                }
                ContextCommand::Pack { path, summary } => {
                    let archive_path = manager
                        .pack_and_archive(project_store, &path, &summary)
                        .map_err(|err| match err {
                            error::DecapodError::ContextPackError(msg) => {
                                error::DecapodError::ContextPackError(format!(
                                    "Context pack failed: {}",
                                    msg
                                ))
                            }
                            other => other,
                        })?;
                    println!("Session archived to: {}", archive_path.display());
                }
                ContextCommand::Restore {
                    id,
                    profile,
                    current_files,
                } => {
                    let content = manager.restore_archive(&id, &profile, &current_files)?;
                    println!(
                        "--- RESTORED CONTENT (Archive: {}) ---\n{}\n--- END RESTORED ---",
                        id, content
                    );
                }
            }
        }
        DataCommand::Schema(schema_cli) => {
            let schemas = schema_catalog();

            let output = if let Some(sub) = schema_cli.subsystem {
                schemas
                    .get(sub.as_str())
                    .cloned()
                    .unwrap_or(serde_json::json!({ "error": "subsystem not found" }))
            } else {
                let mut envelope = serde_json::json!({
                    "schema_version": "1.0.0",
                    "subsystems": schemas,
                    "deprecations": deprecation_metadata(),
                    "command_registry": cli_command_registry()
                });
                if !schema_cli.deterministic {
                    envelope.as_object_mut().unwrap().insert(
                        "generated_at".to_string(),
                        serde_json::json!(format!("{:?}", std::time::SystemTime::now())),
                    );
                }
                envelope
            };

            match schema_cli.format.as_str() {
                "json" => println!("{}", serde_json::to_string_pretty(&output).unwrap()),
                "md" => {
                    println!("{}", schema_to_markdown(&output));
                }
                other => {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Unsupported schema format '{}'. Use 'json' or 'md'.",
                        other
                    )));
                }
            }
        }
        DataCommand::Repo(repo_cli) => match repo_cli.command {
            RepoCommand::Map => {
                let map = repomap::generate_map(project_root);
                println!("{}", serde_json::to_string_pretty(&map).unwrap());
            }
            RepoCommand::Graph => {
                let graph = repomap::generate_doc_graph(project_root);
                println!("{}", graph.mermaid);
            }
        },
        DataCommand::Broker(broker_cli) => match broker_cli.command {
            BrokerCommand::Audit => {
                let audit_log = store_root.join("broker.events.jsonl");
                if audit_log.exists() {
                    let content = std::fs::read_to_string(audit_log)?;
                    println!("{}", content);
                } else {
                    println!("No audit log found.");
                }
            }
            BrokerCommand::Verify => {
                let broker = core::broker::DbBroker::new(store_root);
                let report = broker.verify_replay()?;
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
                if !report.divergences.is_empty() {
                    return Err(error::DecapodError::ValidationError(format!(
                        "Audit log integrity check failed: {} divergence(s) detected",
                        report.divergences.len()
                    )));
                }
            }
        },
        DataCommand::Teammate(teammate_cli) => {
            teammate::run_teammate_cli(project_store, teammate_cli)?;
        }
        DataCommand::Federation(federation_cli) => {
            federation::run_federation_cli(project_store, federation_cli)?;
        }
        DataCommand::Primitives(primitives_cli) => {
            primitives::run_primitives_cli(project_store, primitives_cli)?;
        }
    }

    Ok(())
}

fn schema_to_markdown(schema: &serde_json::Value) -> String {
    fn render_value(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort();
                let mut out = String::new();
                for key in keys {
                    let value = &map[&key];
                    match value {
                        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                            out.push_str(&format!("- **{}**:\n", key));
                            for line in render_value(value).lines() {
                                out.push_str(&format!("  {}\n", line));
                            }
                        }
                        _ => out.push_str(&format!("- **{}**: `{}`\n", key, value)),
                    }
                }
                out
            }
            serde_json::Value::Array(items) => {
                let mut out = String::new();
                for item in items {
                    match item {
                        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
                            out.push_str("- item:\n");
                            for line in render_value(item).lines() {
                                out.push_str(&format!("  {}\n", line));
                            }
                        }
                        _ => out.push_str(&format!("- `{}`\n", item)),
                    }
                }
                out
            }
            _ => format!("- `{}`\n", v),
        }
    }

    let mut out = String::from("# Decapod Schema\n\n");
    out.push_str(&render_value(schema));
    out
}

fn schema_catalog() -> std::collections::BTreeMap<&'static str, serde_json::Value> {
    let mut schemas = std::collections::BTreeMap::new();
    schemas.insert("todo", todo::schema());
    schemas.insert("cron", cron::schema());
    schemas.insert("reflex", reflex::schema());
    schemas.insert("workflow", workflow::schema());
    schemas.insert("container", container::schema());
    schemas.insert("health", health::health_schema());
    schemas.insert("broker", core::broker::schema());
    schemas.insert("external_action", core::external_action::schema());
    schemas.insert("context", context::schema());
    schemas.insert("policy", policy::schema());
    schemas.insert("knowledge", knowledge::schema());
    schemas.insert("repomap", repomap::schema());
    schemas.insert("watcher", watcher::schema());
    schemas.insert("archive", archive::schema());
    schemas.insert("feedback", feedback::schema());
    schemas.insert("teammate", teammate::schema());
    schemas.insert("federation", federation::schema());
    schemas.insert("primitives", primitives::schema());
    schemas.insert("decide", decide::schema());
    schemas.insert("docs", docs_cli::schema());
    schemas.insert("deprecations", deprecation_metadata());
    schemas.insert(
        "command_registry",
        serde_json::json!({
            "name": "command_registry",
            "version": "0.1.0",
            "description": "Machine-readable CLI command registry generated from clap command definitions",
            "root": cli_command_registry()
        }),
    );
    schemas
}

fn deprecation_metadata() -> serde_json::Value {
    serde_json::json!({
        "name": "deprecations",
        "version": "0.1.0",
        "description": "Deprecated command surfaces and replacement pointers",
        "entries": [
            {
                "surface": "command",
                "path": "decapod heartbeat",
                "status": "deprecated",
                "replacement": "decapod govern health summary",
                "notes": "Heartbeat command family was consolidated into govern health"
            },
            {
                "surface": "command",
                "path": "decapod trust",
                "status": "deprecated",
                "replacement": "decapod govern health autonomy",
                "notes": "Trust command family was consolidated into govern health"
            },
            {
                "surface": "module",
                "path": "src/plugins/heartbeat.rs",
                "status": "deprecated",
                "replacement": "src/plugins/health.rs"
            }
        ]
    })
}

fn cli_command_registry() -> serde_json::Value {
    let command = Cli::command();
    command_to_registry(&command)
}

fn command_to_registry(command: &clap::Command) -> serde_json::Value {
    let mut subcommands: Vec<serde_json::Value> = command
        .get_subcommands()
        .filter(|sub| !sub.is_hide_set())
        .map(command_to_registry)
        .collect();
    subcommands.sort_by(|a, b| {
        let a_name = a
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let b_name = b
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        a_name.cmp(b_name)
    });

    let mut options: Vec<serde_json::Value> = command
        .get_arguments()
        .filter(|arg| !arg.is_hide_set())
        .map(|arg| {
            let mut flags = Vec::new();
            if let Some(long) = arg.get_long() {
                flags.push(format!("--{}", long));
            }
            if let Some(short) = arg.get_short() {
                flags.push(format!("-{}", short));
            }
            if flags.is_empty() {
                flags.push(arg.get_id().to_string());
            }

            let value_names = arg
                .get_value_names()
                .map(|values| values.iter().map(|v| v.to_string()).collect::<Vec<_>>())
                .unwrap_or_default();

            serde_json::json!({
                "id": arg.get_id().to_string(),
                "flags": flags,
                "required": arg.is_required_set(),
                "help": arg.get_help().map(|help| help.to_string()),
                "value_names": value_names
            })
        })
        .collect();

    options.sort_by(|a, b| {
        let a_id = a
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let b_id = b
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        a_id.cmp(b_id)
    });

    let aliases: Vec<String> = command.get_all_aliases().map(str::to_string).collect();

    serde_json::json!({
        "name": command.get_name(),
        "about": command.get_about().map(|about| about.to_string()),
        "aliases": aliases,
        "options": options,
        "subcommands": subcommands
    })
}

fn run_auto_command(auto_cli: AutoCli, project_store: &Store) -> Result<(), error::DecapodError> {
    match auto_cli.command {
        AutoCommand::Cron(cron_cli) => cron::run_cron_cli(project_store, cron_cli)?,
        AutoCommand::Reflex(reflex_cli) => reflex::run_reflex_cli(project_store, reflex_cli),
        AutoCommand::Workflow(workflow_cli) => {
            workflow::run_workflow_cli(project_store, workflow_cli)?
        }
        AutoCommand::Container(container_cli) => {
            container::run_container_cli(project_store, container_cli)?
        }
    }

    Ok(())
}

fn run_qa_command(
    qa_cli: QaCli,
    project_store: &Store,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    match qa_cli.command {
        QaCommand::Verify(verify_cli) => {
            verify::run_verify_cli(project_store, project_root, verify_cli)?
        }
        QaCommand::Check {
            crate_description,
            commands,
            all,
        } => run_check(crate_description, commands, all)?,
        QaCommand::Gatling(ref gatling_cli) => plugins::gatling::run_gatling_cli(gatling_cli)?,
    }

    Ok(())
}

fn run_hook_install(
    commit_msg: bool,
    pre_commit: bool,
    uninstall: bool,
) -> Result<(), error::DecapodError> {
    let git_dir_output = std::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map_err(error::DecapodError::IoError)?;

    if !git_dir_output.status.success() {
        return Err(error::DecapodError::ValidationError(
            "Not in a git repository".to_string(),
        ));
    }

    let git_dir = String::from_utf8_lossy(&git_dir_output.stdout)
        .trim()
        .to_string();
    let hooks_dir = PathBuf::from(git_dir).join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(error::DecapodError::IoError)?;

    if uninstall {
        let commit_msg_path = hooks_dir.join("commit-msg");
        let pre_commit_path = hooks_dir.join("pre-commit");
        let mut removed_any = false;

        if commit_msg_path.exists() {
            fs::remove_file(&commit_msg_path).map_err(error::DecapodError::IoError)?;
            println!("✓ Removed commit-msg hook");
            removed_any = true;
        }
        if pre_commit_path.exists() {
            fs::remove_file(&pre_commit_path).map_err(error::DecapodError::IoError)?;
            println!("✓ Removed pre-commit hook");
            removed_any = true;
        }
        if !removed_any {
            println!("No hooks found to remove");
        }
        return Ok(());
    }

    if commit_msg {
        let hook_content = r#"#!/bin/sh
MSG_FILE="$1"
SUBJECT="$(head -n1 "$MSG_FILE")"
if printf '%s' "$SUBJECT" | grep -Eq '^(feat|fix|docs|style|refactor|test|chore|ci|build|perf|revert)(\([^)]+\))?: .+'; then
  exit 0
fi
echo "commit-msg hook: expected conventional commit subject"
echo "got: $SUBJECT"
exit 1
"#;
        let hook_path = hooks_dir.join("commit-msg");
        let mut file = fs::File::create(&hook_path).map_err(error::DecapodError::IoError)?;
        file.write_all(hook_content.as_bytes())
            .map_err(error::DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)
                .map_err(error::DecapodError::IoError)?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms).map_err(error::DecapodError::IoError)?;
        }
        println!("✓ Installed commit-msg hook for conventional commits");
    }

    if pre_commit {
        let hook_content = r#"#!/bin/sh
set -e
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
"#;
        let hook_path = hooks_dir.join("pre-commit");
        let mut file = fs::File::create(&hook_path).map_err(error::DecapodError::IoError)?;
        file.write_all(hook_content.as_bytes())
            .map_err(error::DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)
                .map_err(error::DecapodError::IoError)?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms).map_err(error::DecapodError::IoError)?;
        }
        println!("✓ Installed pre-commit hook (fmt + clippy)");
    }

    if !commit_msg && !pre_commit {
        println!("No hooks specified. Use --commit-msg and/or --pre-commit");
    }

    Ok(())
}

fn run_check(
    crate_description: bool,
    commands: bool,
    all: bool,
) -> Result<(), error::DecapodError> {
    if crate_description || all {
        let expected = "Decapod is a Rust-built governance runtime for AI agents: repo-native state, enforced workflow, proof gates, safe coordination.";

        let output = std::process::Command::new("cargo")
            .args(["metadata", "--no-deps", "--format-version", "1"])
            .output()
            .map_err(|e| error::DecapodError::IoError(std::io::Error::other(e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(error::DecapodError::ValidationError(format!(
                "cargo metadata failed: {}",
                stderr.trim()
            )));
        }

        let json_str = String::from_utf8_lossy(&output.stdout);

        if json_str.contains(expected) {
            println!("✓ Crate description matches");
        } else {
            println!("✗ Crate description mismatch!");
            println!("  Expected: {}", expected);
            return Err(error::DecapodError::ValidationError(
                "Crate description check failed".into(),
            ));
        }
    }

    if commands || all {
        run_command_help_smoke()?;
        println!("✓ Command help surfaces are valid");
    }

    if all && !(crate_description || commands) {
        println!("Note: --all enables all checks");
    }

    Ok(())
}

fn run_command_help_smoke() -> Result<(), error::DecapodError> {
    fn walk(cmd: &clap::Command, prefix: Vec<String>, all_paths: &mut Vec<Vec<String>>) {
        if cmd.get_name() != "help" {
            all_paths.push(prefix.clone());
        }
        for sub in cmd.get_subcommands().filter(|sub| !sub.is_hide_set()) {
            let mut next = prefix.clone();
            next.push(sub.get_name().to_string());
            walk(sub, next, all_paths);
        }
    }

    let exe = std::env::current_exe().map_err(error::DecapodError::IoError)?;
    let mut command_paths = Vec::new();
    walk(&Cli::command(), Vec::new(), &mut command_paths);
    command_paths.sort();
    command_paths.dedup();

    use rayon::prelude::*;
    command_paths.par_iter().try_for_each(|path| {
        let mut args = path.clone();
        args.push("--help".to_string());
        let output = std::process::Command::new(&exe)
            .args(&args)
            .output()
            .map_err(error::DecapodError::IoError)?;
        if !output.status.success() {
            return Err(error::DecapodError::ValidationError(format!(
                "help smoke failed for `decapod {}`: {}",
                path.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(())
    })?;
    Ok(())
}

/// Show version information
fn show_version_info() -> Result<(), error::DecapodError> {
    use colored::Colorize;

    println!(
        "{} {}",
        "Decapod version:".bright_white(),
        migration::DECAPOD_VERSION.bright_green()
    );
    println!(
        "  {} {}",
        "Update:".bright_white(),
        "cargo install decapod".bright_cyan()
    );

    Ok(())
}

/// Run workspace command
fn run_workspace_command(
    cli: WorkspaceCli,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    use crate::core::workspace;

    match cli.command {
        WorkspaceCommand::Ensure { branch, container } => {
            let agent_id =
                std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let config = branch.map(|b| workspace::WorkspaceConfig {
                branch: b,
                use_container: container,
                base_image: if container {
                    Some("rust:1.75-slim".to_string())
                } else {
                    None
                },
            });
            let status = workspace::ensure_workspace(project_root, config, &agent_id)?;

            println!(
                "{}",
                serde_json::json!({
                    "status": if status.can_work { "ok" } else { "pending" },
                    "branch": status.git.current_branch,
                    "is_protected": status.git.is_protected,
                    "can_work": status.can_work,
                    "in_container": status.container.in_container,
                    "docker_available": status.container.docker_available,
                    "worktree_path": status.git.worktree_path,
                    "required_actions": status.required_actions,
                })
            );
        }
        WorkspaceCommand::Status => {
            let status = workspace::get_workspace_status(project_root)?;

            println!(
                "{}",
                serde_json::json!({
                    "can_work": status.can_work,
                    "git_branch": status.git.current_branch,
                    "git_is_protected": status.git.is_protected,
                    "git_has_local_mods": status.git.has_local_mods,
                    "in_container": status.container.in_container,
                    "container_image": status.container.image,
                    "docker_available": status.container.docker_available,
                    "blockers": status.blockers.len(),
                    "required_actions": status.required_actions,
                })
            );
        }
        WorkspaceCommand::Publish { title, description } => {
            let project_store = Store {
                kind: StoreKind::Repo,
                root: project_root.join(".decapod").join("data"),
            };
            run_validation_bounded(&project_store, project_root, false)?;
            let result = workspace::publish_workspace(project_root, title, description)?;
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "branch": result.branch,
                    "commit_hash": result.commit_hash,
                    "remote_url": result.remote_url,
                    "pr_url": result.pr_url,
                })
            );
        }
    }

    Ok(())
}

/// Run STATE_COMMIT commands (prove/verify)
fn run_state_commit_command(
    cli: StateCommitCli,
    project_root: &Path,
) -> Result<(), error::DecapodError> {
    match cli.command {
        StateCommitCommand::Prove { base, head, output } => {
            let head = head.unwrap_or_else(|| {
                state_commit::run_git(project_root, &["rev-parse", "HEAD"])
                    .unwrap_or_else(|_| "HEAD".to_string())
            });

            println!("Computing STATE_COMMIT:");
            println!("  base: {}", base);
            println!("  head: {}", head);

            // Use library function
            let input = state_commit::StateCommitInput {
                base_sha: base,
                head_sha: head.clone(),
                ignore_policy_hash: "da39a3ee5e6b4b0d3255bfef95601890afd80709".to_string(), // empty
            };

            let result = state_commit::prove(&input, project_root)
                .map_err(error::DecapodError::ValidationError)?;

            println!("  files: {}", result.entries.len());

            // Write output
            std::fs::write(&output, &result.scope_record_bytes)
                .map_err(error::DecapodError::IoError)?;

            println!("  scope_record_hash: {}", result.scope_record_hash);
            println!("  state_commit_root: {}", result.state_commit_root);
            println!("  output: {}", output.display());

            Ok(())
        }
        StateCommitCommand::Verify {
            scope_record,
            expected_root,
        } => {
            // Read scope record
            let cbor_bytes = std::fs::read(&scope_record).map_err(error::DecapodError::IoError)?;

            // Use library function for verification
            let record_hash = if let Some(ref exp) = expected_root {
                match state_commit::verify(&cbor_bytes, exp) {
                    Ok(h) => h,
                    Err(e) => {
                        println!("STATE_COMMIT verification:");
                        println!("  scope_record: {}", scope_record.display());
                        println!("  ❌ MISMATCH: {}", e);
                        return Err(error::DecapodError::ValidationError(e));
                    }
                }
            } else {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(&cbor_bytes);
                format!("{:x}", hasher.finalize())
            };

            println!("STATE_COMMIT verification:");
            println!("  scope_record: {}", scope_record.display());
            println!("  scope_record_hash: {}", record_hash);
            println!("  ✅ VERIFIED");

            Ok(())
        }
        StateCommitCommand::Explain { scope_record } => {
            // Read and parse scope_record
            let cbor_bytes = std::fs::read(&scope_record).map_err(error::DecapodError::IoError)?;

            // Compute hashes
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&cbor_bytes);
            let scope_record_hash = format!("{:x}", hasher.finalize());

            // Parse basic structure (simplified - looks for embedded strings)
            let content = String::from_utf8_lossy(&cbor_bytes);

            println!("STATE_COMMIT Explanation:");
            println!("  File: {}", scope_record.display());
            println!("  Size: {} bytes", cbor_bytes.len());
            println!("  scope_record_hash: {}", scope_record_hash);
            println!();

            // Try to extract version and SHAs from the CBOR structure
            if let Some(version_pos) = content.find("state_commit.") {
                if let Some(end_pos) = content[version_pos..].find('\0') {
                    println!(
                        "  algo_version: {}",
                        &content[version_pos..version_pos + end_pos]
                    );
                }
            }

            // Count entries (looking for patterns in the binary data)
            let entry_count = content.matches("kind=").count();
            println!("  Estimated entries: {}", entry_count);
            println!();

            println!("Note: scope_record_hash is sha256(scope_record_bytes)");
            println!("      state_commit_root is the Merkle root of entry hashes");

            Ok(())
        }
    }
}

/// Run RPC command
fn run_rpc_command(cli: RpcCli, project_root: &Path) -> Result<(), error::DecapodError> {
    use crate::core::assurance::{AssuranceEngine, AssuranceEvaluateInput};
    use crate::core::interview;
    use crate::core::mentor;
    use crate::core::rpc::*;
    use crate::core::standards;
    use crate::core::workspace;

    let request: RpcRequest = if cli.stdin {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(error::DecapodError::IoError)?;
        serde_json::from_str(&buffer)
            .map_err(|e| error::DecapodError::ValidationError(format!("Invalid JSON: {}", e)))?
    } else {
        let op = cli.op.ok_or_else(|| {
            error::DecapodError::ValidationError("Operation required".to_string())
        })?;
        let params = cli
            .params
            .as_ref()
            .and_then(|p| serde_json::from_str(p).ok())
            .unwrap_or(serde_json::json!({}));

        RpcRequest {
            op,
            params,
            id: default_request_id(),
            session: None,
        }
    };

    enforce_worktree_requirement_for_rpc(&request.op, project_root)?;

    if !rpc_op_bypasses_session(&request.op) {
        ensure_session_valid()?;
    }
    enforce_constitutional_awareness_for_rpc(&request.op, project_root)?;

    let project_store = Store {
        kind: StoreKind::Repo,
        root: project_root.join(".decapod").join("data"),
    };

    let mandates = docs::resolve_mandates(project_root, &request.op);
    let mandate_blockers = validate::evaluate_mandates(project_root, &project_store, &mandates);

    // If any mandate is blocked, we fail the operation
    let blocked_mandate = mandates.iter().find(|m| {
        mandate_blockers
            .iter()
            .any(|b| b.message.contains(&m.fragment.title))
    });

    if let Some(mandate) = blocked_mandate {
        let blocker = mandate_blockers
            .iter()
            .find(|b| b.message.contains(&mandate.fragment.title))
            .unwrap();
        let response = error_response(
            request.id.clone(),
            request.op.clone(),
            request.params.clone(),
            "mandate_violation".to_string(),
            blocker.message.clone(),
            Some(blocker.clone()),
            mandates,
        );
        println!("{}", serde_json::to_string_pretty(&response).unwrap());
        return Ok(());
    }

    let response = match request.op.as_str() {
        "agent.init" => {
            // Session initialization with receipt
            let workspace_status = workspace::get_workspace_status(project_root)?;
            let mut allowed_ops = workspace::get_allowed_ops(&workspace_status);

            // Add mandatory todo ops if no active tasks
            let agent_id = current_agent_id();
            if agent_id != "unknown" {
                if let Ok(mut tasks) = todo::list_tasks(
                    &project_store.root,
                    Some("open".to_string()),
                    None,
                    None,
                    None,
                    None,
                ) {
                    tasks.retain(|t| t.assigned_to == agent_id);
                    if tasks.is_empty() {
                        allowed_ops.insert(
                            0,
                            AllowedOp {
                                op: "todo.add".to_string(),
                                reason: "MANDATORY: Create a task for your work".to_string(),
                                required_params: vec!["title".to_string()],
                            },
                        );
                    } else if tasks.iter().any(|t| t.assigned_to.is_empty()) {
                        allowed_ops.insert(
                            0,
                            AllowedOp {
                                op: "todo.claim".to_string(),
                                reason: "MANDATORY: Claim your assigned task".to_string(),
                                required_params: vec!["id".to_string()],
                            },
                        );
                    }
                }
            }

            let context_capsule = if workspace_status.can_work {
                Some(ContextCapsule {
                    fragments: vec![],
                    spec: Some("Agent initialized successfully".to_string()),
                    architecture: None,
                    security: None,
                    standards: Some({
                        let resolved = standards::resolve_standards(project_root)?;
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            "project_name".to_string(),
                            serde_json::json!(resolved.project_name),
                        );
                        map
                    }),
                })
            } else {
                None
            };

            let _blocked_by = if !workspace_status.can_work {
                workspace_status.blockers.clone()
            } else {
                vec![]
            };

            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                context_capsule,
                allowed_ops,
                mandates.clone(),
            );
            response.result = Some(serde_json::json!({
                "environment_context": {
                    "repo_root": project_root.to_string_lossy(),
                    "workspace_path": project_root.to_string_lossy(),
                    "tool_summary": {
                        "docker_available": workspace_status.container.docker_available,
                        "in_container": workspace_status.container.in_container,
                    },
                    "done_means": "decapod validate passes"
                }
            }));
            mark_constitution_initialized(project_root)?;
            response
        }
        "workspace.status" => {
            let status = workspace::get_workspace_status(project_root)?;
            let blocked_by = status.blockers.clone();
            let allowed_ops = workspace::get_allowed_ops(&status);

            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                None,
                allowed_ops,
                mandates.clone(),
            );
            response.result = Some(serde_json::json!({
                "git_branch": status.git.current_branch,
                "git_is_protected": status.git.is_protected,
                "in_container": status.container.in_container,
                "can_work": status.can_work,
            }));
            response.blocked_by = blocked_by;
            response
        }
        "workspace.ensure" => {
            let agent_id =
                std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let branch = request
                .params
                .get("branch")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let config = branch.map(|b| workspace::WorkspaceConfig {
                branch: b,
                use_container: false,
                base_image: None,
            });

            let status = workspace::ensure_workspace(project_root, config, &agent_id)?;
            let allowed_ops = workspace::get_allowed_ops(&status);

            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![format!(".git/refs/heads/{}", status.git.current_branch)],
                None,
                allowed_ops,
                mandates.clone(),
            )
        }
        "workspace.publish" => {
            let title = request
                .params
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let description = request
                .params
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let result = workspace::publish_workspace(project_root, title, description)?;

            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                Some(serde_json::json!({
                    "branch": result.branch,
                    "commit_hash": result.commit_hash,
                    "remote_url": result.remote_url,
                    "pr_url": result.pr_url,
                })),
                vec![format!(".git/refs/heads/{}", result.branch)],
                None,
                vec![AllowedOp {
                    op: "validate".to_string(),
                    reason: "Publish complete - run validation".to_string(),
                    required_params: vec![],
                }],
                mandates.clone(),
            )
        }
        "context.resolve" => {
            let params = &request.params;
            let op = params.get("op").and_then(|v| v.as_str());
            let touched_paths = params.get("touched_paths").and_then(|v| v.as_array());
            let intent_tags = params.get("intent_tags").and_then(|v| v.as_array());
            let _limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);

            let mut fragments = Vec::new();
            let bindings = docs::get_bindings(project_root);

            // Deterministic relevance mapping
            if let Some(o) = op {
                if let Some(doc_ref) = bindings.ops.get(o) {
                    let parts: Vec<&str> = doc_ref.split('#').collect();
                    let path = parts[0];
                    let anchor = parts.get(1).copied();
                    if let Some(f) = docs::get_fragment(project_root, path, anchor) {
                        fragments.push(f);
                    }
                }
            }

            if let Some(paths) = touched_paths {
                for p in paths.iter().filter_map(|v| v.as_str()) {
                    for (prefix, doc_ref) in &bindings.paths {
                        if p.contains(prefix) {
                            let parts: Vec<&str> = doc_ref.split('#').collect();
                            let path = parts[0];
                            let anchor = parts.get(1).copied();
                            if let Some(f) = docs::get_fragment(project_root, path, anchor) {
                                fragments.push(f);
                            }
                        }
                    }
                }
            }

            if let Some(tags) = intent_tags {
                for t in tags.iter().filter_map(|v| v.as_str()) {
                    if let Some(doc_ref) = bindings.tags.get(t) {
                        let parts: Vec<&str> = doc_ref.split('#').collect();
                        let path = parts[0];
                        let anchor = parts.get(1).copied();
                        if let Some(f) = docs::get_fragment(project_root, path, anchor) {
                            fragments.push(f);
                        }
                    }
                }
            }

            fragments.sort_by(|a, b| a.r#ref.cmp(&b.r#ref));
            fragments.dedup_by(|a, b| a.r#ref == b.r#ref);
            fragments.truncate(5);

            let result = serde_json::json!({
                "fragments": fragments
            });
            mark_constitution_context_resolved(project_root)?;

            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                Some(result),
                vec![],
                Some(ContextCapsule {
                    fragments,
                    spec: None,
                    architecture: None,
                    security: None,
                    standards: None,
                }),
                vec![],
                mandates.clone(),
            )
        }
        "context.bindings" => {
            let bindings = docs::get_bindings(project_root);
            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                Some(serde_json::to_value(bindings).unwrap()),
                vec![],
                None,
                vec![],
                mandates.clone(),
            )
        }
        "schema.get" => {
            let params = &request.params;
            let entity = params.get("entity").and_then(|v| v.as_str());
            match entity {
                Some("todo") => success_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    Some(serde_json::json!({
                        "schema_version": "v1",
                        "json_schema": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "description": { "type": "string" },
                                "priority": { "type": "string", "enum": ["low", "medium", "high", "critical"] },
                                "tags": { "type": "string" }
                            },
                            "required": ["title"]
                        }
                    })),
                    vec![],
                    None,
                    vec![],
                    mandates.clone(),
                ),
                Some("knowledge") => success_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    Some(serde_json::json!({
                        "schema_version": "v1",
                        "json_schema": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "title": { "type": "string" },
                                "text": { "type": "string" },
                                "provenance": { "type": "string" }
                            },
                            "required": ["id", "title", "text", "provenance"]
                        }
                    })),
                    vec![],
                    None,
                    vec![],
                    mandates.clone(),
                ),
                Some("decision") => success_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    Some(serde_json::json!({
                        "schema_version": "v1",
                        "json_schema": {
                            "type": "object",
                            "properties": {
                                "title": { "type": "string" },
                                "rationale": { "type": "string" },
                                "options": { "type": "array", "items": { "type": "string" } },
                                "chosen": { "type": "string" }
                            },
                            "required": ["title", "rationale", "chosen"]
                        }
                    })),
                    vec![],
                    None,
                    vec![],
                    mandates.clone(),
                ),
                _ => error_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    "invalid_entity".to_string(),
                    format!("Invalid or missing entity: {:?}", entity),
                    None,
                    mandates.clone(),
                ),
            }
        }
        "store.upsert" => {
            let params = &request.params;
            let entity = params.get("entity").and_then(|v| v.as_str());
            let payload = params.get("payload");
            let _provenance = params.get("provenance");

            match entity {
                Some("todo") => {
                    let title = payload
                        .and_then(|p| p.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let description = payload
                        .and_then(|p| p.get("description"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let priority = payload
                        .and_then(|p| p.get("priority"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("medium")
                        .to_string();
                    let tags = payload
                        .and_then(|p| p.get("tags"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let args = todo::TodoCommand::Add {
                        title,
                        description,
                        priority,
                        tags,
                        owner: "".to_string(),
                        due: None,
                        r#ref: "".to_string(),
                        dir: None,
                        depends_on: "".to_string(),
                        blocks: "".to_string(),
                        parent: None,
                    };
                    let res = todo::add_task(&project_store.root, &args)?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "id": res.get("id"),
                            "stored": true
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                Some("knowledge") => {
                    let id = payload
                        .and_then(|p| p.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = payload
                        .and_then(|p| p.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let text = payload
                        .and_then(|p| p.get("text"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let provenance = payload
                        .and_then(|p| p.get("provenance"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    db::initialize_knowledge_db(&project_store.root)?;
                    let result = knowledge::add_knowledge(
                        &project_store,
                        knowledge::AddKnowledgeParams {
                            id: &id,
                            title: &title,
                            content: &text,
                            provenance: &provenance,
                            claim_id: None,
                            merge_key: None,
                            conflict_policy: knowledge::KnowledgeConflictPolicy::Merge,
                            status: "active",
                            ttl_policy: "persistent",
                            expires_ts: None,
                        },
                    )?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "id": result.id,
                            "stored": true,
                            "action": result.action
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                Some("decision") => {
                    // Decisions land in federation for now as a common store
                    let title = payload
                        .and_then(|p| p.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let rationale = payload
                        .and_then(|p| p.get("rationale"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let chosen = payload
                        .and_then(|p| p.get("chosen"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let content = format!("Decision: {}\nRationale: {}", chosen, rationale);
                    let node_id = federation::add_node(
                        &project_store,
                        &title,
                        "decision",
                        "notable",
                        "agent_inferred",
                        &content,
                        "rpc:store.upsert",
                        "",
                        "repo",
                        None,
                        "agent",
                    )?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "id": node_id,
                            "stored": true
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                _ => error_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    "invalid_entity".to_string(),
                    format!("Invalid or missing entity: {:?}", entity),
                    None,
                    mandates.clone(),
                ),
            }
        }
        "store.query" => {
            let params = &request.params;
            let entity = params.get("entity").and_then(|v| v.as_str());
            let query = params.get("query");

            match entity {
                Some("todo") => {
                    let status = query
                        .and_then(|q| q.get("status"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    let tasks =
                        todo::list_tasks(&project_store.root, status, None, None, None, None)?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "items": tasks,
                            "next_page": null
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                Some("knowledge") => {
                    let text = query
                        .and_then(|q| q.get("text"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    db::initialize_knowledge_db(&project_store.root)?;
                    let entries = knowledge::search_knowledge(
                        &project_store,
                        text,
                        knowledge::SearchOptions {
                            as_of: None,
                            window_days: None,
                            rank: "relevance",
                        },
                    )?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "items": entries,
                            "next_page": null
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                Some("decision") => {
                    let nodes = plugins::federation_ext::list_nodes(
                        &project_store.root,
                        Some("decision".to_string()),
                        None,
                        None,
                        None,
                    )?;
                    success_response(
                        request.id.clone(),
                        request.op.clone(),
                        request.params.clone(),
                        Some(serde_json::json!({
                            "items": nodes,
                            "next_page": null
                        })),
                        vec![],
                        None,
                        vec![],
                        mandates.clone(),
                    )
                }
                _ => error_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    "invalid_entity".to_string(),
                    format!("Invalid or missing entity: {:?}", entity),
                    None,
                    mandates.clone(),
                ),
            }
        }
        "validate.run" => {
            let project_store = Store {
                kind: StoreKind::Repo,
                root: project_root.join(".decapod").join("data"),
            };

            let res = run_validation_bounded(&project_store, project_root, false);

            match res {
                Ok(_) => success_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    Some(serde_json::json!({ "success": true })),
                    vec![],
                    None,
                    vec![],
                    mandates.clone(),
                ),
                Err(e) => error_response(
                    request.id.clone(),
                    request.op.clone(),
                    request.params.clone(),
                    "validation_failed".to_string(),
                    e.to_string(),
                    None,
                    mandates.clone(),
                ),
            }
        }
        "scaffold.next_question" => {
            let project_name = request
                .params
                .get("project_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Untitled")
                .to_string();

            let interview = interview::init_interview(project_name);
            let question = interview::next_question(&interview);

            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                None,
                vec![AllowedOp {
                    op: "scaffold.apply_answer".to_string(),
                    reason: "Provide answer to continue interview".to_string(),
                    required_params: vec!["question_id".to_string(), "value".to_string()],
                }],
                mandates.clone(),
            );

            if let Some(q) = question {
                response.result = Some(serde_json::json!({
                    "interview_id": interview.id,
                    "question": q,
                }));
            } else {
                response.result = Some(serde_json::json!({
                    "interview_id": interview.id,
                    "complete": true,
                }));
            }

            response
        }
        "scaffold.apply_answer" => {
            let question_id = request
                .params
                .get("question_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    error::DecapodError::ValidationError("question_id required".to_string())
                })?;
            let value = request
                .params
                .clone()
                .get("value")
                .cloned()
                .ok_or_else(|| {
                    error::DecapodError::ValidationError("value required".to_string())
                })?;

            let mut interview = interview::init_interview("project".to_string());
            interview::apply_answer(&mut interview, question_id, value)?;

            let next_q = interview::next_question(&interview);

            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                None,
                vec![AllowedOp {
                    op: if next_q.is_some() {
                        "scaffold.next_question".to_string()
                    } else {
                        "scaffold.generate_artifacts".to_string()
                    },
                    reason: if next_q.is_some() {
                        "Continue interview".to_string()
                    } else {
                        "Interview complete - generate artifacts".to_string()
                    },
                    required_params: vec![],
                }],
                mandates.clone(),
            );

            response.result = Some(serde_json::json!({
                "answers_count": interview.answers.len(),
                "is_complete": interview.is_complete,
            }));

            response
        }
        "scaffold.generate_artifacts" => {
            let interview = interview::init_interview("project".to_string());
            let output_dir = project_root.to_path_buf();

            let artifacts = interview::generate_artifacts(&interview, &output_dir)?;

            let touched_paths: Vec<String> = artifacts
                .iter()
                .map(|a| a.path.to_string_lossy().to_string())
                .collect();

            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                touched_paths,
                None,
                vec![AllowedOp {
                    op: "validate".to_string(),
                    reason: "Artifacts generated - validate before claiming done".to_string(),
                    required_params: vec![],
                }],
                mandates.clone(),
            )
        }
        "standards.resolve" => {
            let resolved = standards::resolve_standards(project_root)?;

            let mut standards_map = std::collections::HashMap::new();
            standards_map.insert(
                "project_name".to_string(),
                serde_json::json!(resolved.project_name),
            );
            for (k, v) in &resolved.standards {
                standards_map.insert(k.clone(), v.clone());
            }

            let context_capsule = ContextCapsule {
                fragments: vec![],
                spec: None,
                architecture: None,
                security: None,
                standards: Some(standards_map),
            };

            success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                Some(context_capsule),
                vec![],
                mandates.clone(),
            )
        }
        "mentor.obligations" => {
            use crate::core::mentor::{MentorEngine, ObligationsContext};

            let engine = MentorEngine::new(project_root);
            let ctx = ObligationsContext {
                op: request
                    .params
                    .get("op")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                params: request
                    .params
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
                touched_paths: request
                    .params
                    .get("touched_paths")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                diff_summary: request
                    .params
                    .get("diff_summary")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                project_profile_id: request
                    .params
                    .get("project_profile_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                session_id: request
                    .params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                high_risk: request
                    .params
                    .get("high_risk")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            };

            let obligations = engine.compute_obligations(&ctx)?;

            let context_capsule = ContextCapsule {
                fragments: vec![],
                spec: None,
                architecture: None,
                security: None,
                standards: None,
            };

            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                vec![],
                Some(context_capsule),
                vec![AllowedOp {
                    op: "mentor.obligations".to_string(),
                    reason: "Obligations computed - review must list before proceeding".to_string(),
                    required_params: vec![],
                }],
                mandates.clone(),
            );

            response.result = Some(serde_json::json!({
                "obligations": obligations,
            }));

            // Add blockers for contradictions
            if !obligations.contradictions.is_empty() {
                response.blocked_by =
                    mentor::contradictions_to_blockers(&obligations.contradictions);
            }

            response
        }
        "assurance.evaluate" => {
            let input = AssuranceEvaluateInput {
                op: request
                    .params
                    .get("op")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
                params: request
                    .params
                    .get("params")
                    .cloned()
                    .unwrap_or(serde_json::json!({})),
                touched_paths: request
                    .params
                    .get("touched_paths")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default(),
                diff_summary: request
                    .params
                    .get("diff_summary")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                session_id: request
                    .params
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                phase: request
                    .params
                    .get("phase")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok()),
                time_budget_s: request
                    .params
                    .clone()
                    .get("time_budget_s")
                    .and_then(|v| v.as_u64()),
            };

            let engine = AssuranceEngine::new(project_root);
            let evaluated = engine.evaluate(&input)?;
            let mut response = success_response(
                request.id.clone(),
                request.op.clone(),
                request.params.clone(),
                None,
                input.touched_paths.clone(),
                None,
                if let Some(interlock) = &evaluated.interlock {
                    interlock
                        .unblock_ops
                        .iter()
                        .map(|op| AllowedOp {
                            op: op.clone(),
                            reason: format!("Unblock path for {}", interlock.code),
                            required_params: vec![],
                        })
                        .collect()
                } else {
                    vec![AllowedOp {
                        op: "assurance.evaluate".to_string(),
                        reason: "Re-evaluate after meaningful context changes".to_string(),
                        required_params: vec![],
                    }]
                },
                mandates.clone(),
            );
            response.interlock = evaluated.interlock.clone();
            response.advisory = Some(evaluated.advisory.clone());
            response.attestation = Some(evaluated.attestation.clone());
            response.result = Some(serde_json::json!({
                "assurance_evaluated": true,
                "interlock_code": evaluated.interlock.as_ref().map(|i| i.code.clone()),
            }));
            if let Some(interlock) = evaluated.interlock {
                response.blocked_by = vec![Blocker {
                    kind: match interlock.code.as_str() {
                        "workspace_required" => BlockerKind::WorkspaceRequired,
                        "verification_required" => BlockerKind::MissingProof,
                        "store_boundary_violation" => BlockerKind::Unauthorized,
                        "decision_required" => BlockerKind::MissingAnswer,
                        _ => BlockerKind::ValidationFailed,
                    },
                    message: interlock.code,
                    resolve_hint: interlock.message,
                }];
            }
            response
        }
        _ => error_response(
            request.id.clone(),
            request.op.clone(),
            request.params.clone(),
            "unknown_op".to_string(),
            format!("Unknown operation: {}", request.op),
            None,
            mandates.clone(),
        ),
    };

    // Trace the RPC call
    let trace_event = trace::TraceEvent {
        trace_id: request.id.clone(),
        ts: chrono::Utc::now().to_rfc3339(),
        actor: current_agent_id(),
        op: request.op.clone(),
        request: serde_json::to_value(&request).unwrap_or(serde_json::Value::Null),
        response: serde_json::to_value(&response).unwrap_or(serde_json::Value::Null),
    };
    let _ = trace::append_trace(project_root, trace_event);

    println!("{}", serde_json::to_string_pretty(&response).unwrap());
    Ok(())
}

/// Run capabilities command
fn run_capabilities_command(cli: CapabilitiesCli) -> Result<(), error::DecapodError> {
    use crate::core::rpc::generate_capabilities;

    let report = generate_capabilities();

    match cli.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        }
        _ => {
            println!("Decapod {}", report.version);
            println!("==================\n");

            println!("Capabilities:");
            for cap in &report.capabilities {
                println!("  {} [{}] - {}", cap.name, cap.stability, cap.description);
            }

            println!("\nSubsystems:");
            for sub in &report.subsystems {
                println!("  {} [{}]", sub.name, sub.status);
                println!("    Ops: {}", sub.ops.join(", "));
            }

            println!("\nWorkspace:");
            println!(
                "  Enforcement: {}",
                if report.workspace.enforcement_available {
                    "available"
                } else {
                    "unavailable"
                }
            );
            println!(
                "  Docker: {}",
                if report.workspace.docker_available {
                    "available"
                } else {
                    "unavailable"
                }
            );
            println!(
                "  Protected: {}",
                report.workspace.protected_patterns.join(", ")
            );

            println!("\nInterview:");
            println!(
                "  Available: {}",
                if report.interview.available {
                    "yes"
                } else {
                    "no"
                }
            );
            println!(
                "  Artifacts: {}",
                report.interview.artifact_types.join(", ")
            );
            println!("\nInterlocks:");
            println!("  Codes: {}", report.interlock_codes.join(", "));
        }
    }

    Ok(())
}

fn run_trace_command(cli: TraceCli, project_root: &Path) -> Result<(), error::DecapodError> {
    match cli.command {
        TraceCommand::Export { last } => {
            let traces = trace::get_last_traces(project_root, last)?;
            for t in traces {
                println!("{}", t);
            }
        }
    }
    Ok(())
}

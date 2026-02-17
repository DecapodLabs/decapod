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
    db, docs_cli, error, migration, output, proof, repomap, scaffold,
    store::{Store, StoreKind},
    validate,
};
use plugins::{
    archive, container, context, cron, decide, federation, feedback, health, knowledge, policy,
    primitives, reflex, teammate, todo, verify, watcher, workflow,
};

use clap::{CommandFactory, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
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

// ===== Main Command Enum =====

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

    /// Agent orchestration and lifecycle
    #[clap(name = "agent")]
    Agent(AgentCli),

    /// Execute arbitrary commands under governance
    #[clap(name = "exec")]
    Exec(ExecCli),

    /// Governed filesystem operations
    #[clap(name = "fs")]
    Fs(core::fs_cli::FsCli),
}

#[derive(clap::Args, Debug)]
struct AgentCli {
    #[clap(subcommand)]
    command: AgentCommand,
}

#[derive(Subcommand, Debug)]
enum AgentCommand {
    /// Flash the agent with the constitution and produce a verifiable startup receipt.
    Init,
    /// Show the current agent startup status and receipt.
    Status,
}

#[derive(clap::Args, Debug)]
struct ExecCli {
    /// The intent/reason for running this command.
    #[clap(long, short)]
    intent: String,
    /// The command and arguments to execute.
    #[clap(trailing_var_arg = true)]
    command: Vec<String>,
}

#[derive(clap::Args, Debug)]
struct BrokerCli {
    #[clap(subcommand)]
    command: BrokerCommand,
}

#[derive(Subcommand, Debug)]
enum BrokerCommand {
    /// Show the audit log of brokered mutations.
    Audit,
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
        #[clap(long)]
        merge_key: Option<String>,
        #[clap(long, default_value = "reject", value_parser = ["merge", "supersede", "reject"])]
        on_conflict: String,
        #[clap(long, default_value = "active", value_parser = ["active", "superseded", "deprecated", "stale"])]
        status: String,
        #[clap(long, default_value = "persistent", value_parser = ["ephemeral", "decay", "persistent"])]
        ttl_policy: String,
        #[clap(long)]
        expires_ts: Option<String>,
    },
    /// Search project knowledge
    Search {
        #[clap(long)]
        query: String,
        #[clap(long)]
        as_of: Option<String>,
        #[clap(long)]
        window_days: Option<u32>,
        #[clap(long, default_value = "recency_desc", value_parser = ["recency_desc", "recency_decay"])]
        rank: String,
    },
    /// Log retrieval feedback outcome for ROI tracking
    #[clap(name = "retrieval-log")]
    RetrievalLog {
        #[clap(long)]
        query: String,
        #[clap(long, value_delimiter = ',')]
        returned_ids: Vec<String>,
        #[clap(long, value_delimiter = ',')]
        used_ids: Vec<String>,
        #[clap(long, value_parser = ["helped", "neutral", "hurt", "unknown"])]
        outcome: String,
    },
    /// Apply deterministic TTL decay policy
    Decay {
        #[clap(long, default_value = "default")]
        policy: String,
        #[clap(long)]
        as_of: Option<String>,
        #[clap(long)]
        dry_run: bool,
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

            // Create .decapod/data for init
            let setup_store_root = setup_decapod_root.join("data");
            if !init_group.dry_run {
                std::fs::create_dir_all(&setup_store_root).map_err(error::DecapodError::IoError)?;
            }

            // `--dry-run` should not perform any mutations.
            let mut db_created = 0usize;
            let mut db_preserved = 0usize;
            let mut events_created = 0usize;
            let mut events_preserved = 0usize;
            let mut generated_created = false;
            let mut generated_preserved = false;
            let mut init_warnings: Vec<String> = Vec::new();
            if !init_group.dry_run {
                // Initialize all store DBs in the resolved store root (preserve existing)
                let dbs = [
                    ("todo.db", setup_store_root.join("todo.db")),
                    ("knowledge.db", setup_store_root.join("knowledge.db")),
                    ("cron.db", setup_store_root.join("cron.db")),
                    ("reflex.db", setup_store_root.join("reflex.db")),
                    ("health.db", setup_store_root.join("health.db")),
                    ("policy.db", setup_store_root.join("policy.db")),
                    ("archive.db", setup_store_root.join("archive.db")),
                    ("feedback.db", setup_store_root.join("feedback.db")),
                    ("teammate.db", setup_store_root.join("teammate.db")),
                    ("federation.db", setup_store_root.join("federation.db")),
                    ("decisions.db", setup_store_root.join("decisions.db")),
                ];

                for (db_name, db_path) in dbs {
                    if db_path.exists() {
                        db_preserved += 1;
                    } else {
                        match db_name {
                            "todo.db" => todo::initialize_todo_db(&setup_store_root)?,
                            "knowledge.db" => db::initialize_knowledge_db(&setup_store_root)?,
                            "cron.db" => cron::initialize_cron_db(&setup_store_root)?,
                            "reflex.db" => reflex::initialize_reflex_db(&setup_store_root)?,
                            "health.db" => health::initialize_health_db(&setup_store_root)?,
                            "policy.db" => policy::initialize_policy_db(&setup_store_root)?,
                            "archive.db" => archive::initialize_archive_db(&setup_store_root)?,
                            "feedback.db" => feedback::initialize_feedback_db(&setup_store_root)?,
                            "teammate.db" => teammate::initialize_teammate_db(&setup_store_root)?,
                            "federation.db" => {
                                federation::initialize_federation_db(&setup_store_root)?
                            }
                            "decisions.db" => decide::initialize_decide_db(&setup_store_root)?,
                            _ => unreachable!(),
                        }
                        db_created += 1;
                    }
                }

                // Create empty todo events file for validation (preserve existing)
                let events_path = setup_store_root.join("todo.events.jsonl");
                if events_path.exists() {
                    events_preserved += 1;
                } else {
                    std::fs::write(&events_path, "").map_err(error::DecapodError::IoError)?;
                    events_created += 1;
                }

                // Create empty federation events file (preserve existing)
                let fed_events_path = setup_store_root.join("federation.events.jsonl");
                if fed_events_path.exists() {
                    events_preserved += 1;
                } else {
                    std::fs::write(&fed_events_path, "").map_err(error::DecapodError::IoError)?;
                    events_created += 1;
                }

                // Create generated directory for derived files (checksums, caches, etc.)
                let generated_dir = setup_decapod_root.join("generated");
                if generated_dir.exists() {
                    generated_preserved = true;
                } else {
                    std::fs::create_dir_all(&generated_dir)
                        .map_err(error::DecapodError::IoError)?;
                    generated_created = true;
                }

                write_init_container_ssh_key_path(&target_dir)?;
                let has_runtime = init_has_container_runtime();
                let dedicated_key_hint = init_container_ssh_key_hint(&target_dir);
                let has_dedicated_key = init_has_dedicated_agent_key(&target_dir);

                if !has_runtime {
                    init_warnings.push(
                        "container runtime missing (install Docker or Podman for isolated execution)"
                            .to_string(),
                    );
                }

                if !has_dedicated_key {
                    init_warnings.push(format!(
                        "missing dedicated SSH key ({})",
                        dedicated_key_hint
                    ));
                }

                if !has_runtime || !has_dedicated_key {
                    let mut reasons = Vec::new();
                    if !has_runtime {
                        reasons.push("missing docker/podman");
                    }
                    if !has_dedicated_key {
                        reasons.push("missing dedicated ssh key (see .decapod/generated/container_ssh_key_path)");
                    }
                    ensure_init_container_disable_override(&target_dir, &reasons.join(", "))?;
                    init_warnings.push(
                        "container subsystem disabled until prerequisites are met".to_string(),
                    );
                }
            }

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
            if !init_group.dry_run {
                println!(
                    "init: store db+{}={} events+{}={} generated+{}={}",
                    db_created,
                    db_preserved,
                    events_created,
                    events_preserved,
                    usize::from(generated_created),
                    usize::from(generated_preserved)
                );
            }
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
            if !init_warnings.is_empty() {
                println!(
                    "init: warnings {}: {}",
                    init_warnings.len(),
                    output::preview_messages(&init_warnings, 2, 120)
                );
            }
            println!("init: status=ready");
        }
        Command::Session(session_cli) => {
            run_session_command(session_cli)?;
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
            if requires_session_token(&cli.command) {
                ensure_session_valid()?;
            }

            // For other commands, ensure .decapod exists
            let project_root = decapod_root_option?;
            let decapod_root_path = project_root.join(".decapod");
            store_root = decapod_root_path.join("data");
            std::fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;

            // Check for version/schema changes and run protected migrations if needed.
            // Backups are auto-created in .decapod/data only when schema upgrades are pending.
            migration::check_and_migrate_with_backup(&decapod_root_path, |data_root| {
                todo::initialize_todo_db(data_root)?;
                db::initialize_knowledge_db(data_root)?;
                cron::initialize_cron_db(data_root)?;
                reflex::initialize_reflex_db(data_root)?;
                health::initialize_health_db(data_root)?;
                policy::initialize_policy_db(data_root)?;
                archive::initialize_archive_db(data_root)?;
                feedback::initialize_feedback_db(data_root)?;
                teammate::initialize_teammate_db(data_root)?;
                federation::initialize_federation_db(data_root)?;
                decide::initialize_decide_db(data_root)?;
                Ok(())
            })?;

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
                Command::Docs(docs_cli) => docs_cli::run_docs_cli(docs_cli)?,
                Command::Todo(todo_cli) => todo::run_todo_cli(&project_store, todo_cli)?,
                Command::Govern(govern_cli) => {
                    run_govern_command(govern_cli, &project_store, &store_root)?;
                }
                Command::Data(data_cli) => {
                    run_data_command(data_cli, &project_store, &project_root, &store_root)?;
                }
                Command::Auto(auto_cli) => run_auto_command(auto_cli, &project_store)?,
                Command::Qa(qa_cli) => run_qa_command(qa_cli, &project_store, &project_root)?,
                Command::Decide(decide_cli) => decide::run_decide_cli(&project_store, decide_cli)?,
                Command::Agent(agent_cli) => run_agent_command(agent_cli, &project_root)?,
                Command::Exec(exec_cli) => run_exec_command(exec_cli, &project_store)?,
                Command::Fs(fs_cli) => core::fs_cli::run_fs_cli(fs_cli, &project_store, &project_root)?,
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

fn run_agent_command(cli: AgentCli, project_root: &Path) -> Result<(), error::DecapodError> {
    match cli.command {
        AgentCommand::Init => {
            let agent_id = current_agent_id();
            let receipt_path = project_root.join(".decapod").join("generated").join("agent_init.json");
            
            // 1. Print Flash Primer
            println!("\n=== DECAPOD AGENT FLASH ===");
            println!("Identity: {}", agent_id);
            println!("Router:   core/DECAPOD.md");
            println!("Law:      constitution/");
            println!("\nCORE CONTRACT:");
            println!("1. Start at router: `decapod docs show core/DECAPOD.md` before acting.");
            println!("2. Use control plane: Mutations must happen via `decapod` commands.");
            println!("3. Pass validation: `decapod validate` MUST pass before claiming 'done'.");
            println!("4. Record proofs: Record executable evidence of successful work.");
            println!("\nBypassing the control plane results in unverified, unsafe work.");
            println!("============================\n");

            // 2. Generate Receipt
            let head = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(project_root)
                .output()
                .ok()
                .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
                .unwrap_or_else(|| "unknown".to_string());

            let branch = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(project_root)
                .output()
                .ok()
                .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None })
                .unwrap_or_else(|| "unknown".to_string());

            let receipt = serde_json::json!({
                "agent_id": agent_id,
                "ts": now_epoch_secs(),
                "git_head": head,
                "git_branch": branch,
                "decapod_version": migration::DECAPOD_VERSION,
                "router_path": "core/DECAPOD.md",
                "status": "flashed"
            });

            fs::create_dir_all(receipt_path.parent().unwrap()).map_err(error::DecapodError::IoError)?;
            fs::write(&receipt_path, serde_json::to_string_pretty(&receipt).unwrap()).map_err(error::DecapodError::IoError)?;
            
            println!("✓ Agent init receipt written to .decapod/generated/agent_init.json");
            Ok(())
        }
        AgentCommand::Status => {
            let receipt_path = project_root.join(".decapod").join("generated").join("agent_init.json");
            if receipt_path.exists() {
                let content = fs::read_to_string(receipt_path).map_err(error::DecapodError::IoError)?;
                println!("{}", content);
            } else {
                println!("No agent init receipt found. Run `decapod agent init` first.");
            }
            Ok(())
        }
    }
}

fn run_exec_command(cli: ExecCli, store: &Store) -> Result<(), error::DecapodError> {
    if cli.command.is_empty() {
        return Err(error::DecapodError::ValidationError("No command provided to exec".to_string()));
    }

    let agent_id = current_agent_id();
    println!("exec: agent={} intent='{}' cmd='{}'", agent_id, cli.intent, cli.command.join(" "));

    let start_ts = now_epoch_secs();
    let status = std::process::Command::new(&cli.command[0])
        .args(&cli.command[1..])
        .status()
        .map_err(error::DecapodError::IoError)?;
    let end_ts = now_epoch_secs();

    // Record to broker
    let event = serde_json::json!({
        "op": "exec",
        "actor": agent_id,
        "intent": cli.intent,
        "command": cli.command,
        "exit_code": status.code(),
        "ts_start": start_ts,
        "ts_end": end_ts,
        "elapsed": end_ts - start_ts,
    });

    let broker_log = store.root.join("broker.events.jsonl");
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(broker_log)
        .map_err(error::DecapodError::IoError)?;
    
    let mut line = serde_json::to_string(&event).unwrap();
    line.push('\n');
    file.write_all(line.as_bytes()).map_err(error::DecapodError::IoError)?;

    if !status.success() {
        return Err(error::DecapodError::ValidationError(format!("Command failed with exit code: {:?}", status.code())));
    }

    Ok(())
}

fn should_auto_clock_in(command: &Command) -> bool {
    match command {
        Command::Todo(todo_cli) => !todo::is_heartbeat_command(todo_cli),
        Command::Version | Command::Init(_) | Command::Setup(_) | Command::Session(_) => false,
        _ => true,
    }
}

fn requires_session_token(command: &Command) -> bool {
    match command {
        // Only bootstrap/session lifecycle + version are sessionless.
        Command::Init(_) | Command::Session(_) | Command::Version => false,
        Command::Data(DataCli {
            command: DataCommand::Schema(_),
        }) => false,
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
    }
}

fn run_validate_command(
    validate_cli: ValidateCli,
    project_root: &Path,
    project_store: &Store,
) -> Result<(), error::DecapodError> {
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
    validate::run_validation(&store, &decapod_root, &decapod_root)
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
                    merge_key,
                    on_conflict,
                    status,
                    ttl_policy,
                    expires_ts,
                } => {
                    let conflict_policy = knowledge::parse_conflict_policy(&on_conflict)?;
                    let result = knowledge::add_knowledge(
                        project_store,
                        knowledge::AddKnowledgeParams {
                            id: &id,
                            title: &title,
                            content: &text,
                            provenance: &provenance,
                            claim_id: claim_id.as_deref(),
                            merge_key: merge_key.as_deref(),
                            conflict_policy,
                            status: &status,
                            ttl_policy: &ttl_policy,
                            expires_ts: expires_ts.as_deref(),
                        },
                    )?;
                    println!(
                        "Knowledge entry action={} id={} superseded={:?}",
                        result.action, result.id, result.superseded_ids
                    );
                }
                KnowledgeCommand::Search {
                    query,
                    as_of,
                    window_days,
                    rank,
                } => {
                    let results = knowledge::search_knowledge(
                        project_store,
                        &query,
                        knowledge::SearchOptions {
                            as_of: as_of.as_deref(),
                            window_days,
                            rank: &rank,
                        },
                    )?;
                    println!("{}", serde_json::to_string_pretty(&results).unwrap());
                }
                KnowledgeCommand::RetrievalLog {
                    query,
                    returned_ids,
                    used_ids,
                    outcome,
                } => {
                    let actor =
                        std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
                    let result = knowledge::log_retrieval_feedback(
                        project_store,
                        &actor,
                        &query,
                        &returned_ids,
                        &used_ids,
                        &outcome,
                    )?;
                    println!(
                        "Retrieval feedback logged: {} ({})",
                        result.event_id, result.file
                    );
                }
                KnowledgeCommand::Decay {
                    policy,
                    as_of,
                    dry_run,
                } => {
                    let result = knowledge::decay_knowledge(
                        project_store,
                        &policy,
                        as_of.as_deref(),
                        dry_run,
                    )?;
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
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
            },
            {
                "surface": "module",
                "path": "src/plugins/trust.rs",
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

    for path in command_paths {
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
    }
    Ok(())
}

fn init_has_container_runtime() -> bool {
    command_exists("docker") || command_exists("podman")
}

fn init_container_ssh_key_hint(target_dir: &Path) -> String {
    let path_file = target_dir
        .join(".decapod")
        .join("generated")
        .join("container_ssh_key_path");
    fs::read_to_string(path_file)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "~/.ssh/decapod_agent_ed25519".to_string())
}

fn init_has_dedicated_agent_key(target_dir: &Path) -> bool {
    let hint = init_container_ssh_key_hint(target_dir);
    let key = if let Some(home) = std::env::var_os("HOME") {
        if hint.starts_with("~/") {
            PathBuf::from(home).join(hint.trim_start_matches("~/"))
        } else {
            PathBuf::from(hint)
        }
    } else {
        PathBuf::from(hint)
    };
    key.is_file() && key.with_extension("pub").is_file()
}

fn command_exists(name: &str) -> bool {
    ProcessCommand::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ensure_init_container_disable_override(
    target_dir: &Path,
    reason: &str,
) -> Result<(), error::DecapodError> {
    let override_path = target_dir.join(".decapod").join("OVERRIDE.md");
    let existing = if override_path.exists() {
        fs::read_to_string(&override_path).map_err(error::DecapodError::IoError)?
    } else {
        String::new()
    };
    if existing.contains(container::CONTAINER_DISABLE_MARKER) {
        return Ok(());
    }
    if let Some(parent) = override_path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&override_path)
        .map_err(error::DecapodError::IoError)?;
    if !existing.ends_with('\n') && !existing.is_empty() {
        writeln!(file).map_err(error::DecapodError::IoError)?;
    }
    writeln!(file, "### plugins/CONTAINER.md").map_err(error::DecapodError::IoError)?;
    writeln!(file, "## Runtime Guard Override (auto-generated)")
        .map_err(error::DecapodError::IoError)?;
    writeln!(file, "{}", container::CONTAINER_DISABLE_MARKER)
        .map_err(error::DecapodError::IoError)?;
    writeln!(file, "reason: {}", reason).map_err(error::DecapodError::IoError)?;
    writeln!(
        file,
        "warning: disabling isolated containers increases risk of concurrent agents stepping on each other."
    )
    .map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn write_init_container_ssh_key_path(target_dir: &Path) -> Result<(), error::DecapodError> {
    let generated_dir = target_dir.join(".decapod").join("generated");
    fs::create_dir_all(&generated_dir).map_err(error::DecapodError::IoError)?;
    let path_file = generated_dir.join("container_ssh_key_path");
    if !path_file.exists() {
        fs::write(&path_file, "~/.ssh/decapod_agent_ed25519\n")
            .map_err(error::DecapodError::IoError)?;
    }
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

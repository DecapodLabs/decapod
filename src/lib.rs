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
    db, docs_cli, error, migration, proof, repomap, scaffold,
    store::{Store, StoreKind},
    tui, validate,
};
use plugins::{
    archive, context, cron, decide, federation, feedback, health, knowledge, policy, reflex,
    teammate, todo, verify, watcher,
};

use clap::{CommandFactory, Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[clap(
    name = "decapod",
    version = env!("CARGO_PKG_VERSION"),
    about = "The Intent-Driven Engineering System"
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
struct SetupCli {
    #[clap(subcommand)]
    command: SetupCommand,
}

#[derive(Subcommand, Debug)]
enum SetupCommand {
    /// Git hooks for commit validation
    Hook {
        /// Install commit-msg hook for conventional commits
        #[clap(long, default_value = "true")]
        commit_msg: bool,
        /// Install pre-commit hook (fmt, clippy)
        #[clap(long)]
        pre_commit: bool,
        /// Uninstall hooks
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
            use colored::Colorize;

            // Clear screen and position cursor for pristine alien output
            print!("\x1B[2J\x1B[1;1H");

            let _width = tui::terminal_width();

            // 🛸 ALIEN SPACESHIP BANNER 🛸
            println!();
            println!();
            println!(
                "{}",
                "              ▗▄▄▄▄▖  ▗▄▄▄▄▄▄▄▄▄▄▄▄▖  ▗▄▄▄▄▖"
                    .bright_magenta()
                    .bold()
            );
            println!(
                "{}",
                "            ▗▀▀      ▝▀              ▀▘      ▀▀▖"
                    .bright_magenta()
                    .bold()
            );
            println!(
                "          {}   {}   {}",
                "▗▀".bright_magenta().bold(),
                "🦀 D E C A P O D 🦀".bright_white().bold().underline(),
                "▀▖".bright_magenta().bold()
            );
            println!(
                "{}",
                "         ▐                                        ▌"
                    .bright_cyan()
                    .bold()
            );
            println!(
                "         {} {} {}",
                "▐".bright_cyan().bold(),
                "C O N T R O L   P L A N E".bright_cyan().bold(),
                "▌".bright_cyan().bold()
            );
            println!(
                "{}",
                "         ▐                                        ▌"
                    .bright_cyan()
                    .bold()
            );
            println!(
                "{}",
                "          ▝▖                                    ▗▘"
                    .bright_magenta()
                    .bold()
            );
            println!(
                "{}",
                "            ▝▄▄                              ▄▄▘"
                    .bright_magenta()
                    .bold()
            );
            println!(
                "{}",
                "              ▝▀▀▀▀▖  ▝▀▀▀▀▀▀▀▀▀▀▀▀▘  ▗▀▀▀▀▘"
                    .bright_magenta()
                    .bold()
            );
            println!();
            tui::render_box(
                "⛔ DESIGNED FROM INCEPTION FOR AGENTS",
                "AI TOOL FOR AGENTS. NOT A HUMAN TOOL FOR AI.",
                tui::BoxStyle::Error,
            );
            tui::render_box(
                "⚠ DO NOT DISRUPT AGENT FLOW",
                "YOU WILL DEGRADE SYSTEM OUTPUTS",
                tui::BoxStyle::Warning,
            );
            println!(
                "  {} {}",
                "OVERRIDES ONLY:".bright_red().bold(),
                ".decapod/OVERRIDE.md".bright_cyan().bold().underline()
            );
            println!();

            let target_dir = match init_group.dir {
                Some(d) => d,
                None => current_dir.clone(),
            };
            let target_dir =
                std::fs::canonicalize(&target_dir).map_err(error::DecapodError::IoError)?;

            // Check if .decapod exists and skip if it does, unless --force
            let setup_decapod_root = target_dir.join(".decapod");
            if setup_decapod_root.exists() && !init_group.force {
                tui::render_box(
                    "⚠  SYSTEM ALREADY INITIALIZED",
                    "Use --force to override",
                    tui::BoxStyle::Warning,
                );
                println!();
                println!("  {} Detected existing control plane", "▸".bright_yellow());
                println!(
                    "  {} Use {} flag to override",
                    "▸".bright_yellow(),
                    "--force".bright_cyan().bold()
                );
                println!();
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
            if !init_group.dry_run {
                let mut backed_up = false;
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
                        if !backed_up {
                            println!(
                                "        {}",
                                "▼▼▼ PRESERVATION PROTOCOL ACTIVATED ▼▼▼"
                                    .bright_yellow()
                                    .bold()
                            );
                            println!();
                            backed_up = true;
                            created_backups = true;
                        }
                        let backup_path = target_dir.join(format!("{}.bak", file));
                        fs::rename(&path, &backup_path).map_err(error::DecapodError::IoError)?;
                        println!(
                            "          {} {} {} {}",
                            "◆".bright_cyan(),
                            file.bright_white().bold(),
                            "⟿".bright_yellow(),
                            format!("{}.bak", file.strip_suffix(".md").unwrap_or(file))
                                .bright_black()
                        );
                    }
                }
                if backed_up {
                    println!();
                }
            }

            // Create .decapod/data for init
            let setup_store_root = setup_decapod_root.join("data");
            if !init_group.dry_run {
                std::fs::create_dir_all(&setup_store_root).map_err(error::DecapodError::IoError)?;
            }

            // `--dry-run` should not perform any mutations.
            if !init_group.dry_run {
                // Databases setup section - TUI styled box
                tui::render_box(
                    "⚡ SUBSYSTEM INITIALIZATION",
                    "Database & State Management",
                    tui::BoxStyle::Cyan,
                );
                println!();

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
                        println!(
                            "    {} {} {}",
                            "✓".bright_green(),
                            db_name.bright_white(),
                            "(preserved - existing data kept)".bright_black()
                        );
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
                        println!("    {} {}", "●".bright_green(), db_name.bright_white());
                    }
                }

                println!();

                // Create empty todo events file for validation (preserve existing)
                let events_path = setup_store_root.join("todo.events.jsonl");
                if events_path.exists() {
                    println!(
                        "    {} {} {}",
                        "✓".bright_green(),
                        "todo.events.jsonl".bright_white(),
                        "(preserved - event history kept)".bright_black()
                    );
                } else {
                    std::fs::write(&events_path, "").map_err(error::DecapodError::IoError)?;
                    println!(
                        "    {} {}",
                        "●".bright_green(),
                        "todo.events.jsonl".bright_white()
                    );
                }

                // Create empty federation events file (preserve existing)
                let fed_events_path = setup_store_root.join("federation.events.jsonl");
                if fed_events_path.exists() {
                    println!(
                        "    {} {} {}",
                        "✓".bright_green(),
                        "federation.events.jsonl".bright_white(),
                        "(preserved - event history kept)".bright_black()
                    );
                } else {
                    std::fs::write(&fed_events_path, "").map_err(error::DecapodError::IoError)?;
                    println!(
                        "    {} {}",
                        "●".bright_green(),
                        "federation.events.jsonl".bright_white()
                    );
                }

                // Create generated directory for derived files (checksums, caches, etc.)
                let generated_dir = setup_decapod_root.join("generated");
                if generated_dir.exists() {
                    println!(
                        "    {} {} {}",
                        "✓".bright_green(),
                        "generated/".bright_white(),
                        "(preserved - existing files kept)".bright_black()
                    );
                } else {
                    std::fs::create_dir_all(&generated_dir)
                        .map_err(error::DecapodError::IoError)?;
                    println!("    {} {}", "●".bright_green(), "generated/".bright_white());
                }

                println!();
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

            scaffold::scaffold_project_entrypoints(&scaffold::ScaffoldOptions {
                target_dir,
                force: init_group.force,
                dry_run: init_group.dry_run,
                agent_files: agent_files_to_generate,
                created_backups,
                all: init_group.all,
            })?;
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
            // For other commands, ensure .decapod exists
            let project_root = decapod_root_option?;
            let decapod_root_path = project_root.join(".decapod");
            store_root = decapod_root_path.join("data");
            std::fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;

            // Check for version changes and run migrations if needed
            migration::check_and_migrate(&decapod_root_path)?;

            let project_store = Store {
                kind: StoreKind::Repo,
                root: store_root.clone(),
            };

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
                _ => unreachable!(),
            }
        }
    }
    Ok(())
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
                } => {
                    knowledge::add_knowledge(
                        project_store,
                        &id,
                        &title,
                        &text,
                        &provenance,
                        claim_id.as_deref(),
                    )?;
                    println!("Knowledge entry added: {}", id);
                }
                KnowledgeCommand::Search { query } => {
                    let results = knowledge::search_knowledge(project_store, &query)?;
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
                    println!("Markdown schema format not yet implemented. Defaulting to JSON.");
                    println!("{}", serde_json::to_string_pretty(&output).unwrap());
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
    }

    Ok(())
}

fn schema_catalog() -> std::collections::BTreeMap<&'static str, serde_json::Value> {
    let mut schemas = std::collections::BTreeMap::new();
    schemas.insert("todo", todo::schema());
    schemas.insert("cron", cron::schema());
    schemas.insert("reflex", reflex::schema());
    schemas.insert("health", health::health_schema());
    schemas.insert("broker", core::broker::schema());
    schemas.insert("context", context::schema());
    schemas.insert("policy", policy::schema());
    schemas.insert("knowledge", knowledge::schema());
    schemas.insert("repomap", repomap::schema());
    schemas.insert("watcher", watcher::schema());
    schemas.insert("archive", archive::schema());
    schemas.insert("feedback", feedback::schema());
    schemas.insert("teammate", teammate::schema());
    schemas.insert("federation", federation::schema());
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
            all,
        } => run_check(crate_description, all)?,
        QaCommand::Gatling(ref gatling_cli) => plugins::gatling::run_gatling_cli(gatling_cli)?,
    }

    Ok(())
}

fn run_hook_install(
    commit_msg: bool,
    pre_commit: bool,
    uninstall: bool,
) -> Result<(), error::DecapodError> {
    use std::fs;
    use std::io::Write;

    let git_dir = Path::new(".git");
    if !git_dir.exists() {
        return Err(error::DecapodError::ValidationError(
            ".git directory not found. Are you in the root of the project?".into(),
        ));
    }

    let hooks_dir = git_dir.join("hooks");
    fs::create_dir_all(&hooks_dir).map_err(error::DecapodError::IoError)?;

    if uninstall {
        let commit_msg_path = hooks_dir.join("commit-msg");
        let pre_commit_path = hooks_dir.join("pre-commit");

        let mut removed = false;
        if commit_msg_path.exists() {
            fs::remove_file(&commit_msg_path)?;
            println!("✓ Removed commit-msg hook");
            removed = true;
        }
        if pre_commit_path.exists() {
            fs::remove_file(&pre_commit_path)?;
            println!("✓ Removed pre-commit hook");
            removed = true;
        }
        if !removed {
            println!("No hooks found to remove");
        }
        return Ok(());
    }

    // Install commit-msg hook
    if commit_msg {
        let hook_content = r#"#!/bin/sh
# Conventional commit validation hook
# Installed by Decapod

MSG=$(cat "$1")
REGEX="^(feat|fix|chore|ci|docs|style|refactor|perf|test)(\(.*\))?!?: .+"

if ! echo "$MSG" | grep -qE "$REGEX"; then
    echo "Error: Invalid commit message format."
    echo "  Commit messages must follow the Conventional Commits format."
    echo "  Example: 'feat: add login functionality'"
    echo "  Allowed prefixes: feat, fix, chore, ci, docs, style, refactor, perf, test"
    exit 1
fi
"#;

        let hook_path = hooks_dir.join("commit-msg");
        let mut file = fs::File::create(&hook_path).map_err(error::DecapodError::IoError)?;
        file.write_all(hook_content.as_bytes())
            .map_err(error::DecapodError::IoError)?;
        drop(file);

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

    // Install pre-commit hook (pure Rust - runs fmt and clippy)
    if pre_commit {
        // Use a simple shell wrapper that calls cargo
        let hook_content = r#"#!/bin/sh
# Pre-commit hook - runs cargo fmt and clippy
# Installed by Decapod

echo "Running pre-commit checks..."

# Run cargo fmt
if ! cargo fmt --all -- --check 2>/dev/null; then
    echo "Formatting check failed. Run 'cargo fmt --all' to fix."
    exit 1
fi

# Run cargo clippy
if ! cargo clippy --all-targets --all-features -- -D warnings 2>/dev/null; then
    echo "Clippy check failed."
    exit 1
fi

echo "Pre-commit checks passed!"
exit 0
"#;

        let hook_path = hooks_dir.join("pre-commit");
        let mut file = fs::File::create(&hook_path).map_err(error::DecapodError::IoError)?;
        file.write_all(hook_content.as_bytes())
            .map_err(error::DecapodError::IoError)?;
        drop(file);

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

fn run_check(crate_description: bool, all: bool) -> Result<(), error::DecapodError> {
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

    if all && !crate_description {
        println!("Note: --all requires --crate-description");
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

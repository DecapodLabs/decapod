pub mod core;
pub mod plugins;

use core::{
    db, docs_cli, error, proof, repomap, scaffold,
    store::{Store, StoreKind},
    validate,
};
use plugins::{
    archive, context, cron, feedback, health, heartbeat, knowledge, policy, reflex, todo, trust,
    watcher,
};

use clap::{Parser, Subcommand};
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
struct InitCli {
    /// Directory to initialize (defaults to current working directory).
    #[clap(short, long)]
    dir: Option<PathBuf>,
    /// Overwrite existing files by archiving them under `<dir>/.decapod_archive/`.
    #[clap(long)]
    force: bool,
    /// Show what would change without writing files.
    #[clap(long)]
    dry_run: bool,
}

#[derive(clap::Args, Debug)]
struct CleanCli {
    /// Directory to clean (defaults to current working directory).
    #[clap(short, long)]
    dir: Option<PathBuf>,
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

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialize the Decapod system (user store + project entrypoints)
    Init(InitCli),
    /// Remove all Decapod-related files from the current repository.
    Clean(CleanCli),
    /// Validate the Decapod methodology against the documentation
    Validate(ValidateCli),
    /// Access embedded Decapod methodology documentation
    Docs(docs_cli::DocsCli),
    /// Manage cron jobs
    Cron(cron::CronCli),
    /// Manage automated responses (reflexes)
    Reflex(reflex::ReflexCli),
    /// Manage TODO tasks (repo dogfooding + end-user tasks)
    Todo(todo::TodoCli),
    /// Manage brokered state access (The Thin Waist)
    Broker(BrokerCli),
    /// Manage agent context budgets and archival
    Context(ContextCli),
    /// Discover schemas for all subsystems
    Schema(SchemaCli),
    /// Manage the Health Engine (claims and proofs)
    Health(health::HealthCli),
    /// Evaluate risk classification and policy approvals
    Policy(policy::PolicyCli),
    /// Manage repository knowledge
    Knowledge(KnowledgeCli),
    /// Output a deterministic repository map
    Repo(RepoCli),
    /// Execute read-only watchlist checks
    Watcher(WatcherCli),
    /// Show computed system health overview
    Heartbeat,
    /// Show computed agent autonomy status
    Trust(TrustCli),
    /// Manage session archives (MOVE-not-TRIM)
    Archive(ArchiveCli),
    /// Manage operator feedback and preference refinement
    Feedback(FeedbackCli),
    /// Run configurable proofs with audit trail
    Proof(ProofCommandCli),
    /// Run an end-to-end usability verification (simulates fresh install)
    Verify,
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
struct TrustCli {
    /// Actor ID to check trust status for
    #[clap(long, default_value = "decapod")]
    id: String,
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

fn clean_project(opts: &CleanCli) -> Result<(), error::DecapodError> {
    let raw_dir = match opts.dir {
        Some(ref d) => d.clone(),
        None => std::env::current_dir()?,
    };
    let target_dir = std::fs::canonicalize(&raw_dir).map_err(error::DecapodError::IoError)?;

    let decapod_root = target_dir.join(".decapod");
    if decapod_root.exists() {
        println!("Removing directory: {}", decapod_root.display());
        fs::remove_dir_all(&decapod_root).map_err(error::DecapodError::IoError)?;
    }

    for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
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
        Command::Init(init_cli) => {
            let target_dir = match init_cli.dir {
                Some(d) => d,
                None => current_dir.clone(),
            };
            let target_dir =
                std::fs::canonicalize(&target_dir).map_err(error::DecapodError::IoError)?;

            // Check if .decapod exists and skip if it does, unless --force
            let setup_decapod_root = target_dir.join(".decapod");
            if setup_decapod_root.exists() && !init_cli.force {
                println!(
                    "'.decapod' directory already exists in {}. Skipping init. Use --force to re-initialize and overwrite.",
                    target_dir.display()
                );
                return Ok(());
            }

            // Safely backup root AGENTS.md, CLAUDE.md, GEMINI.md if they exist
            if !init_cli.dry_run {
                for file in ["AGENTS.md", "CLAUDE.md", "GEMINI.md"] {
                    let path = target_dir.join(file);
                    if path.exists() {
                        let backup_path = target_dir.join(format!("{}.bak", file));
                        fs::rename(&path, &backup_path).map_err(error::DecapodError::IoError)?;
                        println!(
                            "Backed up existing {} to {}",
                            path.display(),
                            backup_path.display()
                        );
                    }
                }
            }

            // Create .decapod/data for init
            let setup_store_root = setup_decapod_root.join("data");
            if !init_cli.dry_run {
                std::fs::create_dir_all(&setup_store_root).map_err(error::DecapodError::IoError)?;
            }

            // `--dry-run` should not perform any mutations.
            if !init_cli.dry_run {
                // Initialize all store DBs in the resolved store root
                todo::initialize_todo_db(&setup_store_root)?;
                db::initialize_knowledge_db(&setup_store_root)?;
                cron::initialize_cron_db(&setup_store_root)?;
                reflex::initialize_reflex_db(&setup_store_root)?;
                health::initialize_health_db(&setup_store_root)?;
                policy::initialize_policy_db(&setup_store_root)?;
                archive::initialize_archive_db(&setup_store_root)?;
                feedback::initialize_feedback_db(&setup_store_root)?;

                // Create repo store sentinel file for validation
                let sentinel_path = setup_decapod_root.join("DECAPOD_REPO_STORE");
                std::fs::write(&sentinel_path, "").map_err(error::DecapodError::IoError)?;

                // Create empty todo events file for validation
                let events_path = setup_store_root.join("todo.events.jsonl");
                std::fs::write(&events_path, "").map_err(error::DecapodError::IoError)?;
            }

            scaffold::scaffold_project_entrypoints(&scaffold::ScaffoldOptions {
                target_dir,
                force: init_cli.force,
                dry_run: init_cli.dry_run,
            })?;
        }
        Command::Clean(clean_cli) => {
            clean_project(&clean_cli)?;
        }
        _ => {
            // For other commands, ensure .decapod exists
            let project_root = decapod_root_option?;
            store_root = project_root.join(".decapod").join("data");
            std::fs::create_dir_all(&store_root).map_err(error::DecapodError::IoError)?;
            let project_store = Store {
                kind: StoreKind::Repo,
                root: store_root.clone(),
            };

            match cli.command {
                Command::Validate(validate_cli) => {
                    let decapod_root = project_root.clone();
                    let store = match validate_cli.store.as_str() {
                        "user" => {
                            // User store uses a temp directory for blank-slate validation
                            let tmp_root = std::env::temp_dir()
                                .join(format!("decapod_validate_user_{}", ulid::Ulid::new()));
                            std::fs::create_dir_all(&tmp_root)
                                .map_err(error::DecapodError::IoError)?;
                            Store {
                                kind: StoreKind::User,
                                root: tmp_root,
                            }
                        }
                        _ => project_store,
                    };
                    validate::run_validation(&store, &decapod_root, &decapod_root)?;
                }
                Command::Docs(docs_cli) => {
                    docs_cli::run_docs_cli(docs_cli)?;
                }
                Command::Proof(proof_cli) => {
                    proof::execute_proof_cli(&proof_cli, &store_root)?;
                }
                Command::Cron(cron_cli) => {
                    cron::run_cron_cli(&project_store, cron_cli)?;
                }
                Command::Reflex(reflex_cli) => {
                    reflex::run_reflex_cli(&project_store, reflex_cli);
                }
                Command::Todo(todo_cli) => {
                    todo::run_todo_cli(&project_store, todo_cli)?;
                }
                Command::Broker(broker_cli) => match broker_cli.command {
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
                Command::Context(context_cli) => {
                    let manager = context::ContextManager::new(&store_root)?;
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
                                    println!(
                                        "Total tokens: {} (Profile '{}' not found)",
                                        total, profile
                                    );
                                }
                            }
                        }
                        ContextCommand::Pack { path, summary } => {
                            let archive_path =
                                manager.pack_and_archive(&project_store, &path, &summary)?;
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
                Command::Schema(schema_cli) => {
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
                    schemas.insert("heartbeat", heartbeat::schema());
                    schemas.insert("trust", trust::schema());
                    schemas.insert("archive", archive::schema());
                    schemas.insert("feedback", feedback::schema());
                    schemas.insert("docs", docs_cli::schema());

                    let output = if let Some(sub) = schema_cli.subsystem {
                        schemas
                            .get(sub.as_str())
                            .cloned()
                            .unwrap_or(serde_json::json!({ "error": "subsystem not found" }))
                    } else {
                        let mut envelope = serde_json::json!({
                            "schema_version": "1.0.0",
                            "subsystems": schemas
                        });
                        if !schema_cli.deterministic {
                            envelope.as_object_mut().unwrap().insert(
                                "generated_at".to_string(),
                                serde_json::json!(format!("{:?}", std::time::SystemTime::now())),
                            );
                        }
                        envelope
                    };

                    if schema_cli.format == "json" {
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    } else {
                        println!("Markdown schema format not yet implemented. Defaulting to JSON.");
                        println!("{}", serde_json::to_string_pretty(&output).unwrap());
                    }
                }
                Command::Health(health_cli) => {
                    health::run_health_cli(&project_store, health_cli)?;
                }
                Command::Policy(policy_cli) => {
                    policy::run_policy_cli(&project_store, policy_cli)?;
                }
                Command::Knowledge(k_cli) => {
                    db::initialize_knowledge_db(&store_root)?;
                    match k_cli.command {
                        KnowledgeCommand::Add {
                            id,
                            title,
                            text,
                            provenance,
                            claim_id,
                        } => {
                            knowledge::add_knowledge(
                                &project_store,
                                &id,
                                &title,
                                &text,
                                &provenance,
                                claim_id.as_deref(),
                            )?;
                            println!("Knowledge entry added: {}", id);
                        }
                        KnowledgeCommand::Search { query } => {
                            let results = knowledge::search_knowledge(&project_store, &query)?;
                            println!("{}", serde_json::to_string_pretty(&results).unwrap());
                        }
                    }
                }
                Command::Repo(r_cli) => match r_cli.command {
                    RepoCommand::Map => {
                        let decapod_root = project_root;
                        let map = repomap::generate_map(&decapod_root);
                        println!("{}", serde_json::to_string_pretty(&map).unwrap());
                    }
                    RepoCommand::Graph => {
                        let decapod_root = project_root;
                        let graph = repomap::generate_doc_graph(&decapod_root);
                        println!("{}", graph.mermaid);
                    }
                },
                Command::Watcher(w_cli) => match w_cli.command {
                    WatcherCommand::Run => {
                        let report = watcher::run_watcher(&project_store)?;
                        println!("{}", serde_json::to_string_pretty(&report).unwrap());
                    }
                },
                Command::Heartbeat => {
                    let status = heartbeat::get_status(&project_store)?;
                    println!("{}", serde_json::to_string_pretty(&status).unwrap());
                }
                Command::Trust(t_cli) => {
                    let status = trust::get_trust_status(&project_store, &t_cli.id)?;
                    println!("{}", serde_json::to_string_pretty(&status).unwrap());
                }
                Command::Archive(arc_cli) => {
                    archive::initialize_archive_db(&store_root)?;
                    match arc_cli.command {
                        ArchiveCommand::List => {
                            let items = archive::list_archives(&project_store)?;
                            println!("{}", serde_json::to_string_pretty(&items).unwrap());
                        }
                        ArchiveCommand::Verify => {
                            let failures = archive::verify_archives(&project_store)?;
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
                Command::Feedback(f_cli) => {
                    feedback::initialize_feedback_db(&store_root)?;
                    match f_cli.command {
                        FeedbackCommand::Add {
                            source,
                            text,
                            links,
                        } => {
                            let id = feedback::add_feedback(
                                &project_store,
                                &source,
                                &text,
                                links.as_deref(),
                            )?;
                            println!("Feedback recorded: {}", id);
                        }
                        FeedbackCommand::Propose => {
                            let proposal = feedback::propose_prefs(&project_store)?;
                            println!("{}", proposal);
                        }
                    }
                }
                Command::Verify => {
                    run_verification()?;
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}

fn run_verification() -> Result<(), error::DecapodError> {
    use std::process::Command;
    let temp_dir = std::env::temp_dir().join(format!("decapod_verify_{}", ulid::Ulid::new()));
    std::fs::create_dir_all(&temp_dir)?;
    println!("Testing in {}", temp_dir.display());

    let status = Command::new("git")
        .arg("init")
        .arg("-q")
        .current_dir(&temp_dir)
        .status()?;
    if !status.success() {
        return Err(error::DecapodError::ValidationError(
            "git init failed".into(),
        ));
    }

    let exe = std::env::current_exe()?;

    let status = Command::new(&exe)
        .arg("init")
        .arg("--dir")
        .arg(".")
        .current_dir(&temp_dir)
        .status()?;
    if !status.success() {
        return Err(error::DecapodError::ValidationError(
            "decapod init failed".into(),
        ));
    }

    println!("Checking directory structure...");
    let m_dir = temp_dir.join(".decapod");
    if !m_dir.is_dir() {
        return Err(error::DecapodError::NotFound(".decapod/ missing".into()));
    }
    if !temp_dir.join("AGENTS.md").is_file() {
        return Err(error::DecapodError::ValidationError(
            "AGENTS.md missing".into(),
        ));
    }

    if m_dir.join("projects").exists() {
        return Err(error::DecapodError::ValidationError(
            "Legacy projects dir found".into(),
        ));
    }
    if m_dir.join("docs").exists() {
        return Err(error::DecapodError::ValidationError(
            ".decapod/docs should not exist".into(),
        ));
    }

    println!("Checking embedded docs access...");
    let out = Command::new(&exe).arg("docs").arg("list").output()?;
    if !String::from_utf8_lossy(&out.stdout).contains("embedded/core/DECAPOD.md") {
        return Err(error::DecapodError::ValidationError(
            "docs list missing embedded/core/DECAPOD.md".into(),
        ));
    }
    println!("Checking validate...");
    let status = Command::new(&exe)
        .arg("validate")
        .current_dir(&temp_dir)
        .status()?;
    if !status.success() {
        return Err(error::DecapodError::ValidationError(
            "decapod validate failed".into(),
        ));
    }

    println!("Checking state scoping...");
    let data_dir = m_dir.join("data");
    if !data_dir.is_dir() {
        return Err(error::DecapodError::NotFound(
            ".decapod/data/ missing".into(),
        ));
    }
    if !data_dir.join("todo.db").is_file() {
        return Err(error::DecapodError::NotFound(
            "todo.db missing from .decapod/data/".into(),
        ));
    }

    println!("SUCCESS: Project adopted and scoped correctly.");
    std::fs::remove_dir_all(&temp_dir)?;
    Ok(())
}

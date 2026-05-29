//! Documentation CLI for accessing embedded constitution.
//!
//! This module implements the `decapod docs` command family for querying
//! Decapod's embedded methodology documents.

use crate::core::{assets, docs, error};
use clap::Subcommand;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// CLI structure for `decapod docs` command
#[derive(clap::Args, Debug)]
pub struct DocsCli {
    #[clap(subcommand)]
    pub command: DocsCommand,
}

/// Document source selector for viewing constitution docs
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum DocumentSource {
    /// Show only the embedded content (from the binary)
    Embedded,
    /// Show only the override content (from .decapod/OVERRIDE.md sections)
    Override,
    /// Show merged content (embedded base + project override appended)
    Merged,
}

/// Subcommands for the `decapod docs` CLI
#[derive(Subcommand, Debug)]
pub enum DocsCommand {
    /// List all embedded Decapod methodology documents.
    List,
    /// Display the content of a specific embedded document.
    Show {
        #[clap(value_parser)]
        path: String,
        /// Source to display: embedded (binary), override (.decapod), or merged (default)
        #[clap(long, short, value_enum, default_value = "merged")]
        source: DocumentSource,
    },
    /// Dump all embedded constitution for agentic ingestion.
    Ingest,
    /// Return scoped constitution fragments relevant to a concrete query.
    Search {
        /// Problem/query text to scope against constitution docs.
        #[clap(long)]
        query: String,
        /// Optional operation context (e.g. workspace.ensure, store.upsert).
        #[clap(long)]
        op: Option<String>,
        /// Optional touched paths (repeatable).
        #[clap(long = "path")]
        path: Vec<String>,
        /// Optional intent tags (repeatable).
        #[clap(long = "tag")]
        tag: Vec<String>,
        /// Max fragments to return.
        #[clap(long, default_value_t = 5)]
        limit: usize,
        /// Output format: text or json.
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Validate and cache OVERRIDE.md checksum.
    Override {
        /// Force re-cache even if unchanged
        #[clap(long, short)]
        force: bool,
    },
    /// Autogenerate/Sync documentation from code implementation.
    Build {
        /// Only update docs for specific files that were touched.
        #[clap(long, num_args(1..), value_delimiter = ' ')]
        touched: Option<Vec<PathBuf>>,
    },
}

#[derive(Debug, Default)]
pub struct DocsRunResult {
    pub ingested_core_constitution: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideChecksumStatus {
    MissingOverride,
    Cached,
    Updated,
    Unchanged,
}

pub fn sync_override_checksum(
    repo_root: &Path,
    force: bool,
) -> Result<OverrideChecksumStatus, error::DecapodError> {
    let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

    if !override_path.exists() {
        return Ok(OverrideChecksumStatus::MissingOverride);
    }

    let current_checksum = calculate_sha256(&override_path)?;
    if force {
        cache_checksum(repo_root, &current_checksum)?;
        return Ok(OverrideChecksumStatus::Cached);
    }

    match get_cached_checksum(repo_root) {
        Some(cached_checksum) if cached_checksum == current_checksum => {
            Ok(OverrideChecksumStatus::Unchanged)
        }
        Some(_) => {
            cache_checksum(repo_root, &current_checksum)?;
            Ok(OverrideChecksumStatus::Updated)
        }
        None => {
            cache_checksum(repo_root, &current_checksum)?;
            Ok(OverrideChecksumStatus::Cached)
        }
    }
}

pub fn run_docs_cli(cli: DocsCli) -> Result<DocsRunResult, error::DecapodError> {
    match cli.command {
        DocsCommand::List => {
            let all_docs = assets::list_docs();
            let docs: Vec<_> = all_docs
                .into_iter()
                .filter(|d| d.starts_with("docs/agent/"))
                .collect();

            if !docs.is_empty() {
                println!("Decapod Agent Documentation:");
                for doc in docs {
                    println!("- embedded/{doc}");
                }
            } else {
                println!("No agent documentation found.");
            }
            Ok(DocsRunResult::default())
        }
        DocsCommand::Show { path, source } => {
            let normalized_path = path.strip_prefix("embedded/").unwrap_or(&path).to_string();

            if !normalized_path.starts_with("docs/agent/") {
                return Err(error::DecapodError::ValidationError(format!(
                    "Access denied: 'decapod docs' is restricted to 'docs/agent/' paths. Use 'decapod constitution' for other sections. Invalid path: {path}"
                )));
            }

            // It's a direct document, not a constitution section
            let content = match source {
                DocumentSource::Embedded => assets::get_embedded_doc(&normalized_path),
                DocumentSource::Override => {
                    let current_dir =
                        std::env::current_dir().map_err(error::DecapodError::IoError)?;
                    let repo_root = find_repo_root(&current_dir)?;
                    assets::get_override_doc(&repo_root, &normalized_path)
                }
                DocumentSource::Merged => {
                    let current_dir =
                        std::env::current_dir().map_err(error::DecapodError::IoError)?;
                    let repo_root = find_repo_root(&current_dir)?;
                    assets::get_merged_doc(&repo_root, &normalized_path)
                }
            };

            match content {
                Some(content) => {
                    println!("{content}");
                    Ok(DocsRunResult::default())
                }
                None => Err(error::DecapodError::NotFound(format!(
                    "Document not found: {path}"
                ))),
            }
        }
        DocsCommand::Build { touched } => {
            build_docs(touched.unwrap_or_default())?;
            Ok(DocsRunResult::default())
        }

        DocsCommand::Ingest => {
            let docs = assets::list_docs();
            // Determine repo root for override merging
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let mut ingested_core_constitution = false;

            for doc_path in docs {
                if !doc_path.starts_with("docs/agent/") {
                    continue;
                }

                // Convert embedded path to relative path for override merging
                let relative_path = doc_path.strip_prefix("embedded/").unwrap_or(&doc_path);
                if relative_path.starts_with("core/") {
                    ingested_core_constitution = true;
                }

                if let Some(content) = assets::get_merged_doc(&repo_root, relative_path) {
                    println!("--- BEGIN embedded/constitution.json#{doc_path} ---");
                    println!("{content}");
                    println!("--- END embedded/constitution.json#{doc_path} ---");
                }
            }
            Ok(DocsRunResult {
                ingested_core_constitution,
            })
        }
        DocsCommand::Search {
            query,
            op,
            path,
            tag,
            limit,
            format,
        } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let all_fragments = docs::resolve_scoped_fragments(
                &repo_root,
                Some(&query),
                op.as_deref(),
                &path,
                &tag,
                limit * 2, // Get more to allow for filtering
            );

            // Filter for only docs/agent/
            let fragments: Vec<_> = all_fragments
                .into_iter()
                .filter(|f| f.r#ref.contains("docs/agent/"))
                .take(limit)
                .collect();

            if format.eq_ignore_ascii_case("json") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "query": query,
                        "op": op,
                        "paths": path,
                        "tags": tag,
                        "fragments": fragments,
                    }))
                    .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?
                );
            } else {
                println!("Scoped agent documentation context:");
                if fragments.is_empty() {
                    println!("No matching agent documentation found.");
                }
                for (idx, fragment) in fragments.iter().enumerate() {
                    println!("\n{}. {} ({})", idx + 1, fragment.title, fragment.r#ref);
                    println!("{}", fragment.excerpt);
                }
            }
            Ok(DocsRunResult::default())
        }
        DocsCommand::Override { force } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let override_path = repo_root.join(".decapod").join("OVERRIDE.md");
            match sync_override_checksum(&repo_root, force)? {
                OverrideChecksumStatus::MissingOverride => {
                    println!("ℹ No OVERRIDE.md found at {}", override_path.display());
                    println!("  Run `decapod init` to create one.");
                }
                OverrideChecksumStatus::Cached => {
                    println!("✓ OVERRIDE.md checksum cached");
                }
                OverrideChecksumStatus::Updated => {
                    println!("📝 OVERRIDE.md checksum refreshed");
                }
                OverrideChecksumStatus::Unchanged => {
                    println!("✓ OVERRIDE.md unchanged");
                }
            }

            Ok(DocsRunResult::default())
        }
    }
}

/// Helper function to find the .decapod repo root
/// (This is a simplified version; a real implementation might be more robust)
fn find_repo_root(start_dir: &Path) -> Result<PathBuf, error::DecapodError> {
    // Check for developer override first
    let override_root = std::env::var("DECAPOD_DEV_OVERRIDE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| start_dir.to_path_buf());

    let mut current_dir = override_root;
    loop {
        if current_dir.join(".decapod").exists() {
            return Ok(current_dir);
        }
        if !current_dir.pop() {
            return Err(error::DecapodError::NotFound(
                "'.decapod' directory not found in current or parent directories.".to_string(),
            ));
        }
    }
}

/// Calculate SHA256 checksum of a file
use clap::CommandFactory;
use std::io::Write;

fn build_docs(touched: Vec<PathBuf>) -> Result<(), error::DecapodError> {
    let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
    let repo_root = find_repo_root(&current_dir)?;

    let contracts_path = repo_root.join("docs/agent/command-contracts.md");
    let _schema_path = repo_root.join("docs/agent/config-schema.md");

    // Only update if relevant files are touched, or if touched is empty (full build)
    let update_contracts = touched.is_empty()
        || touched.iter().any(|p| {
            let p_str = p.to_string_lossy();
            p_str.contains("src/cli.rs") || p_str.contains("src/core/docs_cli.rs")
        });

    let update_schema = touched.is_empty()
        || touched.iter().any(|p| {
            let p_str = p.to_string_lossy();
            p_str.contains("src/cli.rs")
        });

    if update_contracts {
        println!("Updating docs/agent/command-contracts.md...");
        let mut file = std::fs::File::create(&contracts_path)?;
        writeln!(file, "# Command Contracts\n")?;
        writeln!(
            file,
            "This document defines the normative operational contracts for the Decapod CLI.\n"
        )?;

        let cmd = crate::cli::Cli::command();
        for sub in cmd.get_subcommands() {
            if sub.is_hide_set() {
                continue;
            }
            let name = sub.get_name();
            let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();

            writeln!(file, "## `decapod {name}`")?;
            writeln!(file, "- **Intent:** {about}")?;

            // Extract more info if it's a known core command
            match name {
                "docs" => {
                    writeln!(
                        file,
                        "- **Restriction:** Only handles documents under `docs/agent/`."
                    )?;
                }
                "todo" => {
                    writeln!(
                        file,
                        "- **Preconditions:** Agent must have an active session."
                    )?;
                    writeln!(file, "- **State Transition:** Managed via `todo.db`.")?;
                }
                "workspace" => {
                    writeln!(file, "- **Preconditions:** Task must be claimed.")?;
                    writeln!(
                        file,
                        "- **State Transition:** Creates git worktrees/containers."
                    )?;
                }
                "validate" => {
                    writeln!(file, "- **Intent:** Verify methodology compliance.")?;
                    writeln!(file, "- **Outcome:** Exit code 0 on success, 1 on failure.")?;
                }
                _ => {}
            }
            writeln!(file)?;
        }

        // Auto-scan for RPCs
        writeln!(file, "# RPC Operations (Auto-generated)\n")?;
        let rpc_src =
            std::fs::read_to_string(repo_root.join("src/core/rpc.rs")).unwrap_or_default();
        for line in rpc_src.lines() {
            if line.contains("pub struct ") && line.contains("Params {") {
                let rpc_name = line
                    .split("pub struct ")
                    .nth(1)
                    .unwrap_or("")
                    .split("Params")
                    .next()
                    .unwrap_or("");
                if !rpc_name.is_empty() {
                    writeln!(file, "### Operation: `{rpc_name}`")?;
                }
            }
        }
    }

    if update_schema {
        println!("Updating docs/agent/config-schema.md...");
        let schema_path = repo_root.join("docs/agent/config-schema.md");
        let mut file = std::fs::File::create(&schema_path)?;
        writeln!(file, "# Configuration Schema (Auto-generated)\n")?;

        let config_src = std::fs::read_to_string(repo_root.join("src/cli.rs")).unwrap_or_default();
        if let Some(start) = config_src.find("pub struct DecapodProjectConfig {") {
            let end = config_src[start..].find('}').unwrap_or(0);
            let fields = &config_src[start..start + end + 1];
            writeln!(file, "```rust\n{fields}\n```")?;
        }
    }

    Ok(())
}

fn calculate_sha256(path: &Path) -> Result<String, error::DecapodError> {
    let content = std::fs::read(path).map_err(error::DecapodError::IoError)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{hash:x}"))
}

/// Get cached checksum for OVERRIDE.md
fn get_cached_checksum(repo_root: &Path) -> Option<String> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("generated")
        .join("override.checksum");
    std::fs::read_to_string(checksum_path).ok()
}

/// Cache checksum for OVERRIDE.md
fn cache_checksum(repo_root: &Path, checksum: &str) -> Result<(), error::DecapodError> {
    let checksum_path = repo_root
        .join(".decapod")
        .join("generated")
        .join("override.checksum");
    // Ensure generated directory exists
    if let Some(parent) = checksum_path.parent() {
        std::fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    std::fs::write(checksum_path, checksum).map_err(error::DecapodError::IoError)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "docs",
        "type": "object",
        "properties": {
            "list": {
                "type": "null",
                "description": "List all embedded Decapod methodology documents"
            },
            "show": {
                "type": "string",
                "description": "Display a specific embedded document"
            },
            "ingest": {
                "type": "null",
                "description": "Dump all embedded constitution for agentic ingestion"
            },
            "search": {
                "type": "object",
                "description": "Return scoped constitution fragments for a problem query",
                "properties": {
                    "query": { "type": "string" },
                    "op": { "type": "string" },
                    "path": { "type": "array", "items": { "type": "string" } },
                    "tag": { "type": "array", "items": { "type": "string" } },
                    "limit": { "type": "integer" },
                    "format": { "type": "string", "enum": ["text", "json"] }
                }
            },
            "override": {
                "type": "object",
                "description": "Validate and cache OVERRIDE.md checksum",
                "properties": {
                    "force": {
                        "type": "boolean",
                        "description": "Force re-cache even if unchanged"
                    }
                }
            }
        }
    })
}

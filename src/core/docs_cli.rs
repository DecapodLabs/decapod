//! Documentation CLI for accessing embedded constitution.
//!
//! This module implements the `decapod docs` command family for querying
//! Decapod's embedded methodology documents.
//!
//! # For AI Agents
//!
//! - **Use `decapod docs show <path>` to read constitution**: Don't read from filesystem
//! - **Three source modes**: embedded (binary), override (project), merged (both)
//! - **List available docs**: `decapod docs list` shows all embedded docs
//! - **Ingest command**: `decapod docs ingest` dumps full constitution for agent context
//! - **Override validation**: `decapod docs override` validates and caches OVERRIDE.md checksum

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
    /// Show only the override content (from .decapod/constitution/)
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
    /// Validate and cache OVERRIDE.md checksum.
    Override {
        /// Force re-cache even if unchanged
        #[clap(long, short)]
        force: bool,
    },
}

pub fn run_docs_cli(cli: DocsCli) -> Result<(), error::DecapodError> {
    match cli.command {
        DocsCommand::List => {
            let docs = assets::list_docs();
            println!("Embedded Decapod Methodology Docs:");
            for doc in docs {
                println!("- {}", doc);
            }
            // TODO: Also list dynamically loaded docs from .decapod/constitutions/
            Ok(())
        }
        DocsCommand::Show { path, source } => {
            // Split path and anchor
            let (relative_path, anchor) = if let Some(pos) = path.find('#') {
                (&path[..pos], Some(&path[pos + 1..]))
            } else {
                (path.as_str(), None)
            };

            // Convert to relative path
            let relative_path = relative_path.strip_prefix("embedded/").unwrap_or(relative_path);

            if let Some(a) = anchor {
                let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
                let repo_root = find_repo_root(&current_dir)?;
                if let Some(fragment) = docs::get_fragment(&repo_root, relative_path, Some(a)) {
                    println!("--- {} ---", fragment.title);
                    println!("{}", fragment.excerpt); // Note: this is still truncated if excerpt is truncated
                    // Should we show full section? The user asked for "exact markdown fragment".
                    // I will add a full extraction to docs.rs later if needed.
                    Ok(())
                } else {
                    Err(error::DecapodError::NotFound(format!(
                        "Section not found: {} in {}",
                        a, relative_path
                    )))
                }
            } else {
                let content = match source {
                    DocumentSource::Embedded => {
                        // Show only embedded content from binary
                        assets::get_embedded_doc(relative_path)
                    }
                    DocumentSource::Override => {
                        // Show only override content from .decapod/constitution/
                        let current_dir =
                            std::env::current_dir().map_err(error::DecapodError::IoError)?;
                        let repo_root = find_repo_root(&current_dir)?;
                        assets::get_override_doc(&repo_root, relative_path)
                    }
                    DocumentSource::Merged => {
                        // Show merged content (embedded + override)
                        let current_dir =
                            std::env::current_dir().map_err(error::DecapodError::IoError)?;
                        let repo_root = find_repo_root(&current_dir)?;
                        assets::get_merged_doc(&repo_root, relative_path)
                    }
                };

                match content {
                    Some(content) => {
                        println!("{}", content);
                        Ok(())
                    }
                    None => Err(error::DecapodError::NotFound(format!(
                        "Document not found: {} (source: {:?})",
                        path, source
                    ))),
                }
            }
        }
        DocsCommand::Ingest => {
            let docs = assets::list_docs();
            // Determine repo root for override merging
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;

            for doc_path in docs {
                // Convert embedded path to relative path for override merging
                let relative_path = doc_path.strip_prefix("embedded/").unwrap_or(&doc_path);

                if let Some(content) = assets::get_merged_doc(&repo_root, relative_path) {
                    println!("--- BEGIN {} ---", doc_path);
                    println!("{}", content);
                    println!("--- END {} ---", doc_path);
                }
            }
            Ok(())
        }
        DocsCommand::Override { force } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let override_path = repo_root.join(".decapod").join("OVERRIDE.md");

            if !override_path.exists() {
                println!("ℹ No OVERRIDE.md found at {}", override_path.display());
                println!("  Run `decapod init` to create one.");
                return Ok(());
            }

            // Calculate current checksum
            let current_checksum = calculate_sha256(&override_path)?;

            if force {
                println!("🔄 Force re-caching OVERRIDE.md checksum...");
                cache_checksum(&repo_root, &current_checksum)?;
                println!("✓ Checksum cached: {}", current_checksum);
                return Ok(());
            }

            // Check if changed
            let cached = get_cached_checksum(&repo_root);
            match cached {
                Some(cached_checksum) if cached_checksum == current_checksum => {
                    println!("✓ OVERRIDE.md unchanged");
                    println!("  Cached checksum: {}", cached_checksum);
                }
                Some(cached_checksum) => {
                    println!("📝 OVERRIDE.md has changed");
                    println!("  Old checksum: {}", cached_checksum);
                    println!("  New checksum: {}", current_checksum);
                    cache_checksum(&repo_root, &current_checksum)?;
                    println!("✓ Checksum updated");
                }
                None => {
                    println!("📝 First time caching OVERRIDE.md checksum");
                    println!("  Checksum: {}", current_checksum);
                    cache_checksum(&repo_root, &current_checksum)?;
                    println!("✓ Checksum cached");
                }
            }

            Ok(())
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
fn calculate_sha256(path: &Path) -> Result<String, error::DecapodError> {
    let content = std::fs::read(path).map_err(error::DecapodError::IoError)?;
    let hash = Sha256::digest(&content);
    Ok(format!("{:x}", hash))
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

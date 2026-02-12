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

use crate::core::{assets, error};
use clap::Subcommand;
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
            // Convert to relative path
            let relative_path = path.strip_prefix("embedded/").unwrap_or(&path);

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
            }
        }
    })
}

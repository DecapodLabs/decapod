use crate::core::{assets, error};
use clap::Subcommand;
use std::env;
use std::path::{Path, PathBuf};

#[derive(clap::Args, Debug)]
pub struct DocsCli {
    #[clap(subcommand)]
    pub command: DocsCommand,
}

#[derive(Subcommand, Debug)]
pub enum DocsCommand {
    /// List all embedded Decapod methodology documents.
    List,
    /// Display the content of a specific embedded document.
    Show {
        #[clap(value_parser)]
        path: String,
    },
    /// Dump all embedded constitution for agentic ingestion.
    Ingest,
}

/// Load override blob layer (cached or recompiled as needed)
fn load_override_blob_layer(
    repo_root: &Path,
) -> Result<Option<assets::OverrideBlob>, error::DecapodError> {
    // Check for developer override location first
    let override_root = std::env::var("DECAPOD_DEV_OVERRIDE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.to_path_buf());

    // Only try to load blob if .decapod/constitution exists
    let constitution_dir = override_root.join(".decapod").join("constitution");
    if !constitution_dir.exists() {
        return Ok(None); // No overrides - use embedded only
    }

    assets::load_override_blob(&override_root)
        .map_err(|e| {
            error::DecapodError::IoError(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("{}", e),
            ))
        })
        .map(Some)
}

/// Access constitution document: embedded base + optional override layer
fn get_constitution_with_overrides(repo_root: &Path, relative_path: &str) -> Option<String> {
    // Get embedded base content from compiled blob (always available)
    let embedded_content = assets::get_embedded_doc(relative_path);

    if embedded_content.is_none() {
        return None;
    }

    // Try to load override layer
    match load_override_blob_layer(repo_root) {
        Ok(Some(blob)) => {
            // Override layer loaded - merge with embedded
            assets::get_merged_doc(&blob, relative_path).or(embedded_content)
        }
        Ok(None) => {
            // No overrides - use embedded only
            embedded_content
        }
        Err(_) => {
            // Override loading failed - fall back to embedded
            embedded_content
        }
    }
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
        DocsCommand::Show { path } => {
            // Determine repo root for dynamic constitution loading
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;

            // Convert to relative path for override merging
            let relative_path = path.strip_prefix("embedded/").unwrap_or(&path);

            match get_constitution_with_overrides(&repo_root, relative_path) {
                Some(content) => {
                    println!("{}", content);
                    Ok(())
                }
                None => Err(error::DecapodError::NotFound(format!(
                    "Document not found: {}",
                    path
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

                if let Some(content) = get_constitution_with_overrides(&repo_root, relative_path) {
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

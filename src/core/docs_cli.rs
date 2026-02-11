use crate::core::assets;
use crate::core::error;
use clap::Subcommand;
use std::fs;
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
    /// Dump all embedded constitutions for agentic ingestion.
    Ingest,
}

/// Attempts to read a constitution document, prioritizing user overrides.
/// Checks `<repo-dir>/.decapod/constitutions/` first, then falls back to embedded assets.
fn read_constitution_doc(repo_root: &Path, doc_path: &str) -> Option<String> {
    let override_path = repo_root
        .join(".decapod")
        .join("constitutions")
        .join(doc_path);

    if override_path.exists() {
        if let Ok(content) = fs::read_to_string(&override_path) {
            return Some(content);
        }
    }

    // Fallback to embedded asset
    assets::get_doc(&format!("embedded/{}", doc_path))
}

pub fn run_docs_cli(cli: DocsCli) -> Result<(), error::DecapodError> {
    // Determine repo root for dynamic constitution loading
    let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
    let repo_root = find_repo_root(&current_dir)?;

    match cli.command {
        DocsCommand::List => {
            let docs = assets::list_docs(); // This currently lists embedded docs
            println!("Embedded Decapod Methodology Docs:");
            for doc in docs {
                println!("- {}", doc);
            }
            // TODO: Also list dynamically loaded docs from .decapod/constitutions/
            Ok(())
        }
        DocsCommand::Show { path } => {
            match read_constitution_doc(&repo_root, &path) {
                Some(content) => {
                    println!("{}", content);
                    Ok(())
                },
                None => Err(error::DecapodError::NotFound(format!("Document not found: {}", path))),
            }
        }
        DocsCommand::Ingest => {
            let docs = assets::list_docs();
            for doc_path in docs {
                if let Some(content) = assets::get_doc(&doc_path) {
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
    let mut current_dir = PathBuf::from(start_dir);
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
        "version": "0.1.0",
        "description": "Access embedded Decapod methodology documentation",
        "commands": [
            { "name": "list", "description": "List embedded docs" },
            { "name": "show", "parameters": ["path"] }
        ],
        "storage": ["(embedded binary assets)"]
    })
}

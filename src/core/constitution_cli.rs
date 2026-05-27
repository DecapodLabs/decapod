//! Constitution CLI for accessing embedded methodology.

use crate::core::{assets, docs, error};
use clap::Subcommand;
use std::path::{Path, PathBuf};

#[derive(clap::Args, Debug)]
pub struct ConstitutionCli {
    #[clap(subcommand)]
    pub command: ConstitutionCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConstitutionCommand {
    /// List all constitution nodes available.
    List,
    /// Display the structured content of a specific constitution node.
    Get {
        #[clap(value_parser)]
        node: String,
    },
    /// Return scoped constitution fragments relevant to a query.
    Search {
        /// Problem/query text to scope against constitution docs.
        #[clap(long)]
        query: String,
        /// Max fragments to return.
        #[clap(long, default_value_t = 8)]
        limit: usize,
    },
}

pub fn run_constitution_cli(cli: ConstitutionCli) -> Result<(), error::DecapodError> {
    match cli.command {
        ConstitutionCommand::List => {
            let all_docs = assets::list_docs();
            let mut constitution_nodes = Vec::new();
            for doc in all_docs {
                if !doc.starts_with("docs/") {
                    constitution_nodes.push(doc);
                }
            }
            println!("Decapod Constitution Nodes:");
            for node in constitution_nodes {
                println!("- {}", node);
            }
            Ok(())
        }
        ConstitutionCommand::Get { node } => {
            // Get embedded node directly. Since assets::get_embedded_doc returns a serialized JSON string for new schema,
            // we can parse it and print it formatted.
            if let Some(content) = assets::get_embedded_doc(&node) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&parsed).unwrap_or(content)
                    );
                } else {
                    println!("{}", content);
                }
                Ok(())
            } else {
                Err(error::DecapodError::NotFound(format!(
                    "Node not found: {}",
                    node
                )))
            }
        }
        ConstitutionCommand::Search { query, limit } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let fragments =
                docs::resolve_scoped_fragments(&repo_root, Some(&query), None, &[], &[], limit);

            println!("Scoped constitution context:");
            for (idx, fragment) in fragments.iter().enumerate() {
                println!("\n{}. {} ({})", idx + 1, fragment.title, fragment.r#ref);
                println!("{}", fragment.excerpt);
            }
            Ok(())
        }
    }
}

fn find_repo_root(start_dir: &Path) -> Result<PathBuf, error::DecapodError> {
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

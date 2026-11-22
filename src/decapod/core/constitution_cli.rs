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
        /// Resolve the full 4-layer precedence chain (embedded -> org overlay -> OVERRIDE.md) in the current repository.
        #[clap(long)]
        resolved: bool,
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
    /// Inspect, verify, import, or clear organization-level constitution overlays.
    Overlay {
        #[clap(subcommand)]
        command: OverlayCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum OverlayCommand {
    /// Inspect organization overlay trust, freshness, and 4-layer authority status without modifying files.
    Status {
        /// Output format: text or json.
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Strictly verify organization overlay integrity, freshness, and non_overridable conflict freedom (fails closed).
    Verify {
        /// Output format: text or json.
        #[clap(long, default_value = "text")]
        format: String,
    },
    /// Verify and import an organization overlay bundle locally for disconnected/offline usage.
    Import {
        /// Path to the signed or digest-verified organization overlay JSON bundle.
        #[clap(value_parser)]
        path: PathBuf,
    },
    /// Remove the locally cached imported organization overlay bundle.
    Clear,
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
                println!("- {node}");
            }
            Ok(())
        }
        ConstitutionCommand::Get { node, resolved } => {
            if resolved {
                let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
                let repo_root = find_repo_root(&current_dir)?;
                let authority = crate::core::org_overlay::resolve_authority_chain(&repo_root)?;
                let embedded_raw = assets::get_embedded_doc(&node).ok_or_else(|| {
                    error::DecapodError::NotFound(format!("Node not found: {node}"))
                })?;
                let embedded_json: serde_json::Value = serde_json::from_str(&embedded_raw)
                    .unwrap_or_else(|_| serde_json::json!({ "text": embedded_raw }));
                let merged_markdown = assets::get_merged_doc(&repo_root, &node);
                let out = serde_json::json!({
                    "node": node,
                    "embedded": embedded_json,
                    "merged_markdown": merged_markdown,
                    "authority_chain": authority.chain,
                });
                println!("{}", serde_json::to_string_pretty(&out).unwrap());
                return Ok(());
            }
            // Get embedded node directly. Since assets::get_embedded_doc returns a serialized JSON string for new schema,
            // we can parse it and print it formatted.
            if let Some(content) = assets::get_embedded_doc(&node) {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&parsed).unwrap_or(content)
                    );
                } else {
                    println!("{content}");
                }
                Ok(())
            } else {
                Err(error::DecapodError::NotFound(format!(
                    "Node not found: {node}"
                )))
            }
        }
        ConstitutionCommand::Search { query, limit } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            let authority = crate::core::org_overlay::resolve_authority_chain(&repo_root)?;
            let context =
                docs::resolve_scoped_context(&repo_root, Some(&query), None, &[], &[], limit, None);

            println!(
                "Authority chain: binary_embedded -> organization_overlay ({:?}/{:?}) -> repository_project_override -> task_context_projection (non-authoritative)",
                authority.chain.organization_overlay.trust_state,
                authority.chain.organization_overlay.freshness_state
            );
            println!("Scoped constitution context:");
            if context.fragments.is_empty() {
                println!("No relevant advisory fragment.");
            }
            for (idx, (fragment, evidence)) in context
                .fragments
                .iter()
                .zip(&context.selection.selected)
                .enumerate()
            {
                println!("\n{}. {} ({})", idx + 1, fragment.title, fragment.r#ref);
                println!(
                    "Source: {} | {} | {} | {} | sha256:{} | {}",
                    evidence.source_layer,
                    evidence.authority,
                    evidence.binding,
                    evidence.status,
                    evidence.source_hash,
                    evidence.reasons.join("; ")
                );
                println!("{}", fragment.excerpt);
            }
            Ok(())
        }
        ConstitutionCommand::Overlay { command } => {
            let current_dir = std::env::current_dir().map_err(error::DecapodError::IoError)?;
            let repo_root = find_repo_root(&current_dir)?;
            match command {
                OverlayCommand::Status { format } => {
                    let loaded = crate::core::org_overlay::load_organization_overlay(&repo_root)?;
                    let payload = match crate::core::org_overlay::resolve_authority_chain(
                        &repo_root,
                    ) {
                        Ok(resolved) => serde_json::to_value(&resolved.chain).unwrap(),
                        Err(err) => serde_json::json!({
                            "schema_version": crate::core::org_overlay::AUTHORITY_CHAIN_SCHEMA_VERSION,
                            "authoritative_order": [
                                crate::core::org_overlay::LAYER_BINARY_EMBEDDED,
                                crate::core::org_overlay::LAYER_ORGANIZATION_OVERLAY,
                                crate::core::org_overlay::LAYER_REPOSITORY_PROJECT_OVERRIDE
                            ],
                            "non_authoritative_projection": crate::core::org_overlay::LAYER_TASK_CONTEXT_PROJECTION,
                            "organization_overlay": loaded.status,
                            "conflicts": [err.to_string()],
                            "offline_usable": loaded.status.offline_usable,
                            "task_projection_isolation_enforced": true
                        }),
                    };
                    if format == "json" {
                        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
                    } else {
                        println!(
                            "Organization Overlay Status: present={} trust={:?} freshness={:?} offline_usable={}",
                            loaded.status.present,
                            loaded.status.trust_state,
                            loaded.status.freshness_state,
                            loaded.status.offline_usable
                        );
                        if let Some(org_id) = &loaded.status.org_id {
                            println!("  org_id: {org_id}");
                        }
                        if let Some(diag) = &loaded.status.diagnostic {
                            println!("  diagnostic: {diag}");
                        }
                    }
                    Ok(())
                }
                OverlayCommand::Verify { format } => {
                    let resolved = crate::core::org_overlay::resolve_authority_chain(&repo_root)?;
                    if format == "json" {
                        println!("{}", serde_json::to_string_pretty(&resolved.chain).unwrap());
                    } else {
                        println!(
                            "Organization overlay verified: {} resolved directive(s), offline_usable={}",
                            resolved.chain.directives.len(),
                            resolved.chain.offline_usable
                        );
                    }
                    Ok(())
                }
                OverlayCommand::Import { path } => {
                    let status =
                        crate::core::org_overlay::import_overlay_bundle(&repo_root, &path)?;
                    println!("{}", serde_json::to_string_pretty(&status).unwrap());
                    Ok(())
                }
                OverlayCommand::Clear => {
                    let removed = crate::core::org_overlay::clear_imported_overlay(&repo_root)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "removed": removed,
                            "path": crate::core::org_overlay::IMPORTED_ORG_OVERLAY_REL_PATH,
                        }))
                        .unwrap()
                    );
                    Ok(())
                }
            }
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

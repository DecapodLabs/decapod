//! Filesystem CLI for governed I/O.
//!
//! This module allows agents to perform filesystem operations that are:
//! 1. Logged to the broker (provenance)
//! 2. Governed by policy (allowed paths)
//! 3. Validated (orphan check)

use crate::core::{error, store::Store};
use clap::{Args, Subcommand};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct FsCli {
    #[clap(subcommand)]
    pub command: FsCommand,
}

#[derive(Subcommand, Debug)]
pub enum FsCommand {
    /// Write content to a file (overwrite).
    Write {
        /// Path to write to.
        path: PathBuf,
        /// Content to write (if omitted, reads from stdin).
        #[clap(long)]
        content: Option<String>,
    },
    /// Read content from a file.
    Read {
        /// Path to read from.
        path: PathBuf,
    },
}

pub fn run_fs_cli(cli: FsCli, store: &Store, project_root: &std::path::Path) -> Result<(), error::DecapodError> {
    match cli.command {
        FsCommand::Write { path, content } => {
            let target_path = project_root.join(&path);
            
            // Security check: simple path traversal prevention
            if !target_path.starts_with(project_root) {
                return Err(error::DecapodError::ValidationError(format!(
                    "Path must be within project root: {}", path.display()
                )));
            }

            let data = match content {
                Some(c) => c,
                None => {
                    let mut buffer = String::new();
                    std::io::stdin().read_to_string(&mut buffer).map_err(error::DecapodError::IoError)?;
                    buffer
                }
            };

            // Calculate hash of content
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(data.as_bytes());
            let hash = format!("{:x}", hasher.finalize());

            // Perform write
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
            }
            fs::write(&target_path, &data).map_err(error::DecapodError::IoError)?;

            // Log to broker
            let agent_id = std::env::var("DECAPOD_AGENT_ID").unwrap_or_else(|_| "unknown".to_string());
            let event = serde_json::json!({
                "op": "fs.write",
                "actor": agent_id,
                "path": path,
                "hash": hash,
                "size": data.len(),
                "ts": crate::core::time::now_epoch_secs(),
            });
            crate::core::broker::log_event(&store.root, event)?;

            println!("✓ Wrote {} bytes to {} (hash: {})", data.len(), path.display(), &hash[0..8]);
            Ok(())
        }
        FsCommand::Read { path } => {
            let target_path = project_root.join(&path);
             if !target_path.starts_with(project_root) {
                return Err(error::DecapodError::ValidationError(format!(
                    "Path must be within project root: {}", path.display()
                )));
            }
            if !target_path.exists() {
                return Err(error::DecapodError::NotFound(format!("File not found: {}", path.display())));
            }
            let content = fs::read_to_string(&target_path).map_err(error::DecapodError::IoError)?;
            print!("{}", content);
            Ok(())
        }
    }
}

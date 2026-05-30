//! Store abstraction for Decapod's state management.
//!
//! This module provides the fundamental data model for Decapod's dual-store architecture.
//! Two store types are supported: User (local mutable) and Repo (project-scoped deterministic).

use crate::core::error;
use crate::core::workspace;
use std::path::{Path, PathBuf};

/// Store type discriminator for dual-store architecture.
///
/// Decapod maintains two distinct stores with different semantics:
/// - `User`: Agent-local state (blank slate, no automatic seeding)
/// - `Repo`: Project-scoped state (dogfood backlog, event-sourced, deterministic rebuild)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreKind {
    /// User store: Agent-local workspace at `~/.decapod/data/`
    User,
    /// Repo store: Project-scoped workspace at `<repo>/.decapod/data/`
    Repo,
}

/// Store handle representing a Decapod state workspace.
///
/// A Store is a logical container for Decapod's state databases and event logs.
/// All subsystem state (TODO, health, knowledge, etc.) is scoped to a store.
///
#[derive(Debug, Clone)]
pub struct Store {
    /// Store type (User or Repo)
    pub kind: StoreKind,
    /// Absolute path to the store root directory
    pub root: PathBuf,
}

pub fn find_decapod_project_root(start_dir: &Path) -> Result<PathBuf, error::DecapodError> {
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

pub fn find_governance_root(workspace_root: &Path) -> PathBuf {
    // If we are in a worktree, the governance root is the main repo.
    // Otherwise, it's just the workspace root.
    workspace::get_main_repo_root(workspace_root).unwrap_or_else(|_| workspace_root.to_path_buf())
}

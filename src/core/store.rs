//! Store abstraction for Decapod's state management.
//!
//! This module provides the fundamental data model for Decapod's dual-store architecture.
//! Two store types are supported: User (local mutable) and Repo (project-scoped deterministic).

use std::path::PathBuf;

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

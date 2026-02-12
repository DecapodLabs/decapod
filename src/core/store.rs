//! Store abstraction for Decapod's state management.
//!
//! This module provides the fundamental data model for Decapod's dual-store architecture.
//! Agents interact with two distinct store types: User (local, mutable) and Repo (project-scoped, deterministic).
//!
//! # For AI Agents
//!
//! - **User Store**: Personal workspace at `~/.decapod/data/` for agent-local state
//! - **Repo Store**: Project-scoped workspace at `<repo>/.decapod/data/` for shared, audited state
//! - All state mutations go through these stores via the broker (see `broker.rs`)
//! - Store kind determines behavior: User stores are blank-slate, Repo stores are event-sourced

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
/// # Agent Usage
///
/// Agents should never directly manipulate store files. Always use the `DbBroker`
/// to access store state through the CLI thin waist.
#[derive(Debug, Clone)]
pub struct Store {
    /// Store type (User or Repo)
    pub kind: StoreKind,
    /// Absolute path to the store root directory
    pub root: PathBuf,
}

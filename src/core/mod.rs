//! Core modules for Decapod's control plane and methodology enforcement.
//!
//! This is the foundation of Decapod's Project OS for Machines. All core subsystems
//! and shared primitives live here.
//!
//! # For AI Agents
//!
//! This is an **agent-first system**. Humans steer via intent; agents execute via this API.
//!
//! ## Module Overview
//!
//! - **`store`**: Dual-store architecture (User vs Repo)
//! - **`broker`**: Serialized state access control plane (The Thin Waist)
//! - **`db`**: Database connection and initialization utilities
//! - **`schemas`**: Canonical SQL schemas for all subsystems
//! - **`migration`**: Automatic version detection and schema migration
//! - **`proof`**: Executable validation checks with audit trails
//! - **`validate`**: Intent-driven methodology validation harness
//! - **`assets`**: Embedded constitution and template documents
//! - **`scaffold`**: Project initialization and entrypoint generation
//! - **`repomap`**: Repository structure discovery for agent onboarding
//! - **`docs_cli`**: Documentation access via `decapod docs` commands
//! - **`error`**: Canonical error type for all Decapod operations
//!
//! ## Agent Contract
//!
//! 1. **Use the CLI, not direct DB access**: `decapod` commands route through the broker
//! 2. **Validate before completion**: `decapod validate` must pass
//! 3. **Read constitution first**: `decapod docs show core/DECAPOD.md`
//! 4. **Respect store semantics**: User = blank slate, Repo = event-sourced

pub mod assets;
pub mod assurance;
pub mod broker;
pub mod db;
pub mod docs;
pub mod docs_cli;
pub mod error;
pub mod external_action;
pub mod flight_recorder;
pub mod interview;
pub mod mentor;
pub mod migration;
pub mod output;
pub mod proof;
pub mod repomap;
pub mod rpc;
pub mod scaffold;
pub mod schemas;
pub mod standards;
pub mod store;
pub mod time;
pub mod todo;
pub mod trace;
pub mod validate;
pub mod workspace;
pub mod state_commit;

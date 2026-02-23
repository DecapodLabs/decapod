//! Core modules for Decapod's control plane and methodology enforcement.
//!
//! This is the foundation of Decapod's runtime. All core subsystems
//! and shared primitives live here.

pub mod assets;
pub mod assurance;
pub mod broker;
pub mod context_capsule;
pub mod coplayer;
pub mod db;
pub mod docs;
pub mod docs_cli;
pub mod error;
pub mod external_action;
pub mod flight_recorder;
pub mod gatekeeper;
pub mod interview;
pub mod mentor;
pub mod migration;
pub mod obligation;
pub mod output;
pub mod plan_governance;
pub mod pool;
pub mod project_specs;
pub mod proof;
pub mod repomap;
pub mod rpc;
pub mod scaffold;
pub mod schemas;
pub mod standards;
pub mod state_commit;
pub mod store;
pub mod time;
pub mod todo;
pub mod trace;
pub mod validate;
pub mod workspace;
pub mod workunit;

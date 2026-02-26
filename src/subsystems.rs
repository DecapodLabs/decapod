//! Subsystem registration — centralizes all DB initialization functions.
//!
//! Adding a new subsystem: append one entry to `SUBSYSTEMS`.

use crate::core::{db, error};
use crate::core::todo;
use crate::plugins::{
    aptitude, archive, cron, decide, federation, feedback, health, lcm, policy, reflex,
};
use std::path::Path;

pub(crate) struct SubsystemInit {
    /// Subsystem identifier (used for diagnostics and future registry queries).
    #[allow(dead_code)]
    pub name: &'static str,
    pub initialize_db: fn(&Path) -> Result<(), error::DecapodError>,
}

/// All subsystems that require database initialization.
/// Order matters for daemonless first-start reliability — sequential execution
/// avoids SQLite contention during bootstrap.
pub(crate) const SUBSYSTEMS: &[SubsystemInit] = &[
    SubsystemInit { name: "todo", initialize_db: todo::initialize_todo_db },
    SubsystemInit { name: "health", initialize_db: health::initialize_health_db },
    SubsystemInit { name: "policy", initialize_db: policy::initialize_policy_db },
    SubsystemInit { name: "feedback", initialize_db: feedback::initialize_feedback_db },
    SubsystemInit { name: "archive", initialize_db: archive::initialize_archive_db },
    SubsystemInit { name: "knowledge", initialize_db: db::initialize_knowledge_db },
    SubsystemInit { name: "aptitude", initialize_db: aptitude::initialize_aptitude_db },
    SubsystemInit { name: "federation", initialize_db: federation::initialize_federation_db },
    SubsystemInit { name: "decide", initialize_db: decide::initialize_decide_db },
    SubsystemInit { name: "lcm", initialize_db: lcm::initialize_lcm_db },
    SubsystemInit { name: "cron", initialize_db: cron::initialize_cron_db },
    SubsystemInit { name: "reflex", initialize_db: reflex::initialize_reflex_db },
];

/// Initialize all subsystem databases sequentially.
pub(crate) fn initialize_all_dbs(data_root: &Path) -> Result<(), error::DecapodError> {
    for sub in SUBSYSTEMS {
        (sub.initialize_db)(data_root)?;
    }
    Ok(())
}

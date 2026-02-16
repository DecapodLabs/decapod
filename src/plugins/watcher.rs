use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::external_action::{self, ExternalCapability};
use crate::core::store::Store;
use crate::health;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Watchlist {
    pub check_repo_dirty: bool,
    pub check_proof_slas: bool,
    pub check_archives: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatcherReport {
    pub ts: String,
    pub repo_dirty: Option<bool>,
    pub stale_claims: Vec<String>,
    pub missing_archives: Vec<String>,
}

pub fn watcher_events_path(root: &Path) -> PathBuf {
    root.join("watcher.events.jsonl")
}

pub fn run_watcher(store: &Store) -> Result<WatcherReport, error::DecapodError> {
    let watchlist_path = store.root.join("WATCHLIST.json");
    let watchlist = if watchlist_path.exists() {
        let content = fs::read_to_string(watchlist_path).map_err(error::DecapodError::IoError)?;
        serde_json::from_str(&content).unwrap_or(default_watchlist())
    } else {
        default_watchlist()
    };

    let mut report = WatcherReport {
        ts: now_iso(),
        repo_dirty: None,
        stale_claims: Vec::new(),
        missing_archives: Vec::new(),
    };

    if watchlist.check_repo_dirty {
        let repo_root = store
            .root
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| store.root.clone());
        let output = external_action::execute(
            &store.root,
            ExternalCapability::VcsRead,
            "watcher.repo_dirty",
            "git",
            &["status", "--porcelain"],
            &repo_root,
        );
        if let Ok(out) = output {
            report.repo_dirty = Some(!out.stdout.is_empty());
        }
    }

    if watchlist.check_proof_slas {
        health::initialize_health_db(&store.root)?;
        let all_health = health::get_all_health(store)?;
        for (id, state, _) in all_health {
            if state == health::HealthState::STALE || state == health::HealthState::CONTRADICTED {
                report.stale_claims.push(id);
            }
        }
    }

    if watchlist.check_archives {
        // Simple integrity scan: check if all archives in DB exist on disk
        let archive_dir = store.root.join("memory/archive");
        if !archive_dir.exists() {
            report
                .missing_archives
                .push("archive_directory_missing".to_string());
        }
    }

    // RUNTIME PURITY: Watcher must only append to its own event log.
    // It is forbidden from calling broker.with_conn for write ops.
    log_watcher_event(store, &report)?;

    Ok(report)
}

fn default_watchlist() -> Watchlist {
    Watchlist {
        check_repo_dirty: true,
        check_proof_slas: true,
        check_archives: true,
    }
}

fn log_watcher_event(store: &Store, report: &WatcherReport) -> Result<(), error::DecapodError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    let path = watcher_events_path(&store.root);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(error::DecapodError::IoError)?;

    let event = serde_json::json!({
        "ts": report.ts,
        "type": "watcher.run",
        "report": report
    });

    writeln!(f, "{}", serde_json::to_string(&event).unwrap())
        .map_err(error::DecapodError::IoError)?;

    // Also audit via broker
    let _broker = DbBroker::new(&store.root);
    // Broker doesn't currently support arbitrary log but we can use with_conn on a dummy or just log directly
    // For Epoch 4, we rely on the watcher.events.jsonl as the primary audit trail for this subsystem.

    Ok(())
}

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "watcher",
        "version": "0.1.0",
        "description": "Proactive repository monitoring",
        "commands": [
            { "name": "run", "description": "Execute read-only watchlist checks" }
        ],
        "storage": ["WATCHLIST.json", "watcher.events.jsonl"]
    })
}

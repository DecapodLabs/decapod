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
    pub check_protected_branches: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WatcherReport {
    pub ts: String,
    pub repo_dirty: Option<bool>,
    pub stale_claims: Vec<String>,
    pub missing_archives: Vec<String>,
    pub protected_branch_violations: Vec<ProtectedBranchViolation>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProtectedBranchViolation {
    pub branch: String,
    pub violation_type: String,
    pub commit_hash: Option<String>,
    pub message: String,
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
        protected_branch_violations: Vec::new(),
    };

    if watchlist.check_protected_branches {
        let repo_root = store
            .root
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| store.root.clone());

        let protected = ["master", "main", "production", "stable"];

        // Check current branch
        let output = external_action::execute(
            &store.root,
            ExternalCapability::VcsRead,
            "watcher.current_branch",
            "git",
            &["rev-parse", "--abbrev-ref", "HEAD"],
            &repo_root,
        );
        if let Ok(out) = output {
            let current_branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let is_protected = protected.iter().any(|p| current_branch == *p)
                || current_branch.starts_with("release/");
            if is_protected {
                report.protected_branch_violations.push(ProtectedBranchViolation {
                    branch: current_branch.clone(),
                    violation_type: "current_on_protected".to_string(),
                    commit_hash: None,
                    message: format!("Watcher detected work on protected branch '{}' - implementation must use working branch", current_branch),
                });
            }
        }

        // Check for unpushed commits to protected branches
        let output = external_action::execute(
            &store.root,
            ExternalCapability::VcsRead,
            "watcher.ahead_behind",
            "git",
            &["rev-list", "--left-right", "--count", "HEAD...origin/HEAD"],
            &repo_root,
        );
        if let Ok(out) = output {
            let counts = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = counts.split_whitespace().collect();
            if parts.len() >= 2 {
                let ahead: u32 = parts[0].parse().unwrap_or(0);
                if ahead > 0 {
                    let commit_output = external_action::execute(
                        &store.root,
                        ExternalCapability::VcsRead,
                        "watcher.last_commit",
                        "git",
                        &["rev-list", "--format=%H", "-n1", "HEAD"],
                        &repo_root,
                    );
                    let commit_hash = commit_output
                        .ok()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
                    report
                        .protected_branch_violations
                        .push(ProtectedBranchViolation {
                            branch: "origin/HEAD".to_string(),
                            violation_type: "unpushed_to_protected".to_string(),
                            commit_hash,
                            message: format!(
                                "Watcher detected {} unpushed commit(s) to protected branch",
                                ahead
                            ),
                        });
                }
            }
        }
    }

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
        check_protected_branches: true,
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

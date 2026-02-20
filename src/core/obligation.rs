//! Governance-native obligation graph for cross-session coordination.
//!
//! Obligations are the formal, dependency-aware units of work in Decapod.
//! Unlike TODOs, obligations are proof-gated and strictly integrated with
//! governance and promotion cycles.

use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ObligationStatus {
    Open,
    Met,
    Failed,
}

impl ObligationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ObligationStatus::Open => "open",
            ObligationStatus::Met => "met",
            ObligationStatus::Failed => "failed",
        }
    }

    pub fn from_status_str(s: &str) -> Self {
        match s {
            "met" => ObligationStatus::Met,
            "failed" => ObligationStatus::Failed,
            _ => ObligationStatus::Open,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObligationNode {
    pub id: String,
    pub intent_ref: String,
    pub risk_tier: String,
    pub required_proofs: Vec<String>,
    pub state_commit_root: Option<String>,
    pub status: ObligationStatus,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObligationEdge {
    pub edge_id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
    pub created_at: String,
}

pub fn obligation_db_path(root: &Path) -> PathBuf {
    root.join(schemas::GOVERNANCE_DB_NAME)
}

pub fn initialize_obligation_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = obligation_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "obligation.init", |conn| {
        conn.execute(schemas::GOVERNANCE_DB_SCHEMA_OBLIGATIONS, [])?;
        conn.execute(schemas::GOVERNANCE_DB_SCHEMA_OBLIGATION_EDGES, [])?;
        Ok(())
    })
}

#[derive(Parser, Debug)]
#[clap(name = "obligation", about = "Manage the Obligation Engine")]
pub struct ObligationCli {
    #[clap(subcommand)]
    pub command: ObligationCommand,
}

#[derive(Subcommand, Debug)]
pub enum ObligationCommand {
    /// Add a new obligation node.
    Add {
        #[clap(long)]
        intent: String,
        #[clap(long, default_value = "medium")]
        risk: String,
        #[clap(long, default_value = "")]
        depends_on: String, // comma-separated IDs
        #[clap(long, default_value = "")]
        proofs: String, // comma-separated claim IDs or labels
    },
    /// List all obligations.
    List,
    /// Get an obligation by ID.
    Get {
        #[clap(long)]
        id: String,
    },
    /// Compute and update the status of an obligation.
    Verify {
        #[clap(long)]
        id: String,
    },
    /// Mark an obligation as complete by providing a state commit root.
    Complete {
        #[clap(long)]
        id: String,
        #[clap(long)]
        commit: String,
    },
}

pub fn run_obligation_cli(store: &Store, cli: ObligationCli) -> Result<(), error::DecapodError> {
    initialize_obligation_db(&store.root)?;
    match cli.command {
        ObligationCommand::Add {
            intent,
            risk,
            depends_on,
            proofs,
        } => {
            let id = add_obligation(store, &intent, &risk, &depends_on, &proofs)?;
            println!("Obligation added: {}", id);
        }
        ObligationCommand::List => {
            let obligations = list_obligations(store)?;
            println!("{}", serde_json::to_string_pretty(&obligations).unwrap());
        }
        ObligationCommand::Get { id } => {
            let obligation = get_obligation(store, &id)?;
            println!("{}", serde_json::to_string_pretty(&obligation).unwrap());
        }
        ObligationCommand::Verify { id } => {
            let (status, reason) = verify_obligation(store, &id)?;
            println!("Status: {:?}\nReason: {}", status, reason);
        }
        ObligationCommand::Complete { id, commit } => {
            complete_obligation(store, &id, &commit)?;
            let (status, reason) = verify_obligation(store, &id)?;
            println!("Obligation {} updated with commit {}.", id, commit);
            println!("Status: {:?}\nReason: {}", status, reason);
        }
    }
    Ok(())
}

pub fn add_obligation(
    store: &Store,
    intent: &str,
    risk: &str,
    depends_on: &str,
    proofs: &str,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);
    let id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let depends_on_ids: Vec<String> = depends_on
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let proof_list: Vec<String> = proofs
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let proof_json = serde_json::to_string(&proof_list).unwrap();

    broker.with_conn(&db_path, "decapod", None, "obligation.add", |conn| {
        // Check for cycles before adding edges
        for dep_id in &depends_on_ids {
            if detect_cycle(conn, dep_id, &id)? {
                return Err(error::DecapodError::ValidationError(format!(
                    "Circular dependency detected: {} -> {}",
                    id, dep_id
                )));
            }
        }

        conn.execute(
            "INSERT INTO obligations (id, intent_ref, risk_tier, required_proofs, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, intent, risk, proof_json, ObligationStatus::Open.as_str(), now, now],
        )?;

        for dep_id in depends_on_ids {
            let edge_id = Ulid::new().to_string();
            conn.execute(
                "INSERT INTO obligation_edges (edge_id, from_id, to_id, kind, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![edge_id, id, dep_id, "depends_on", now],
            )?;
        }

        Ok(())
    })?;

    Ok(id)
}

pub fn list_obligations(store: &Store) -> Result<Vec<ObligationNode>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "obligation.list", |conn| {
        let mut stmt = conn.prepare("SELECT id, intent_ref, risk_tier, required_proofs, state_commit_root, status, created_at, updated_at, metadata FROM obligations")?;
        let rows = stmt.query_map([], |row| {
            let proofs_json: String = row.get(3)?;
            let proofs: Vec<String> = serde_json::from_str(&proofs_json).unwrap_or_default();
            let metadata_json: Option<String> = row.get(8)?;
            let metadata: Option<serde_json::Value> = metadata_json.and_then(|s| serde_json::from_str(&s).ok());

            Ok(ObligationNode {
                id: row.get(0)?,
                intent_ref: row.get(1)?,
                risk_tier: row.get(2)?,
                required_proofs: proofs,
                state_commit_root: row.get(4)?,
                status: ObligationStatus::from_status_str(&row.get::<_, String>(5)?),
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                metadata,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    })
}

pub fn get_obligation(store: &Store, id: &str) -> Result<ObligationNode, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "obligation.get", |conn| {
        conn.query_row(
            "SELECT id, intent_ref, risk_tier, required_proofs, state_commit_root, status, created_at, updated_at, metadata FROM obligations WHERE id = ?1",
            params![id],
            |row| {
                let proofs_json: String = row.get(3)?;
                let proofs: Vec<String> = serde_json::from_str(&proofs_json).unwrap_or_default();
                let metadata_json: Option<String> = row.get(8)?;
                let metadata: Option<serde_json::Value> = metadata_json.and_then(|s| serde_json::from_str(&s).ok());

                Ok(ObligationNode {
                    id: row.get(0)?,
                    intent_ref: row.get(1)?,
                    risk_tier: row.get(2)?,
                    required_proofs: proofs,
                    state_commit_root: row.get(4)?,
                    status: ObligationStatus::from_status_str(&row.get::<_, String>(5)?),
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    metadata,
                })
            },
        ).map_err(error::DecapodError::RusqliteError)
    })
}

pub fn detect_cycle(
    conn: &rusqlite::Connection,
    from_id: &str,
    to_id: &str,
) -> Result<bool, error::DecapodError> {
    if from_id == to_id {
        return Ok(true);
    }

    let mut stmt = conn.prepare("SELECT to_id FROM obligation_edges WHERE from_id = ?1")?;
    let mut visited = std::collections::HashSet::new();
    let mut stack = vec![from_id.to_string()];

    while let Some(current) = stack.pop() {
        if current == to_id {
            return Ok(true);
        }
        if visited.contains(&current) {
            continue;
        }
        visited.insert(current.clone());

        let rows = stmt.query_map(params![current], |row| row.get::<_, String>(0))?;
        for row in rows {
            stack.push(row?);
        }
    }

    Ok(false)
}

pub fn verify_obligation(
    store: &Store,
    id: &str,
) -> Result<(ObligationStatus, String), error::DecapodError> {
    let obligation = get_obligation(store, id)?;

    // 1. Check dependencies
    let dependencies = get_dependencies(store, id)?;
    for dep in dependencies {
        if dep.status != ObligationStatus::Met {
            return Ok((
                ObligationStatus::Open,
                format!("Dependency {} is not Met", dep.id),
            ));
        }
    }

    // 2. Check proofs
    for proof_label in &obligation.required_proofs {
        if !check_proof_satisfied(store, proof_label)? {
            return Ok((
                ObligationStatus::Open,
                format!("Proof {} is not satisfied", proof_label),
            ));
        }
    }

    // 3. Check state commit
    if obligation.state_commit_root.is_none() {
        return Ok((
            ObligationStatus::Open,
            "State commit root missing".to_string(),
        ));
    }

    // All conditions met
    update_obligation_status(store, id, ObligationStatus::Met)?;
    Ok((
        ObligationStatus::Met,
        "All conditions satisfied".to_string(),
    ))
}

pub fn get_dependencies(
    store: &Store,
    id: &str,
) -> Result<Vec<ObligationNode>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "obligation.get_deps", |conn| {
        let mut stmt = conn.prepare(
            "SELECT o.id, o.intent_ref, o.risk_tier, o.required_proofs, o.state_commit_root, o.status, o.created_at, o.updated_at, o.metadata 
             FROM obligations o
             JOIN obligation_edges e ON o.id = e.to_id
             WHERE e.from_id = ?1"
        )?;
        let rows = stmt.query_map(params![id], |row| {
            let proofs_json: String = row.get(3)?;
            let proofs: Vec<String> = serde_json::from_str(&proofs_json).unwrap_or_default();
            let metadata_json: Option<String> = row.get(8)?;
            let metadata: Option<serde_json::Value> = metadata_json.and_then(|s| serde_json::from_str(&s).ok());

            Ok(ObligationNode {
                id: row.get(0)?,
                intent_ref: row.get(1)?,
                risk_tier: row.get(2)?,
                required_proofs: proofs,
                state_commit_root: row.get(4)?,
                status: ObligationStatus::from_status_str(&row.get::<_, String>(5)?),
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                metadata,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    })
}

fn check_proof_satisfied(store: &Store, proof_label: &str) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let health_db = store.root.join(schemas::GOVERNANCE_DB_NAME);

    broker.with_conn(
        &health_db,
        "decapod",
        None,
        "obligation.check_proof",
        |conn| {
            let status: Option<String> = conn
                .query_row(
                    "SELECT computed_state FROM health_cache WHERE claim_id = ?1",
                    params![proof_label],
                    |row| row.get(0),
                )
                .optional()?;

            Ok(status == Some("VERIFIED".to_string()))
        },
    )
}

fn update_obligation_status(
    store: &Store,
    id: &str,
    status: ObligationStatus,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);
    let now = chrono::Utc::now().to_rfc3339();

    broker.with_conn(
        &db_path,
        "decapod",
        None,
        "obligation.update_status",
        |conn| {
            conn.execute(
                "UPDATE obligations SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![status.as_str(), now, id],
            )?;
            Ok(())
        },
    )
}

pub fn complete_obligation(
    store: &Store,
    id: &str,
    commit: &str,
) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = obligation_db_path(&store.root);
    let now = chrono::Utc::now().to_rfc3339();

    broker.with_conn(&db_path, "decapod", None, "obligation.complete", |conn| {
        conn.execute(
            "UPDATE obligations SET state_commit_root = ?1, updated_at = ?2 WHERE id = ?3",
            params![commit, now, id],
        )?;
        Ok(())
    })
}

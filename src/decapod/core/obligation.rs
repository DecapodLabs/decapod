//! Governance-native obligation graph for cross-session coordination.
//!
//! Obligations are the formal, dependency-aware units of work in Decapod.
//! Unlike TODOs, obligations are proof-gated and strictly integrated with
//! governance and promotion cycles.
//!
//! KEY PRINCIPLE: Completion is DERIVED, never asserted.
//! - Status is computed from: dependencies satisfied, proofs verified, state_commit present
//! - No user-settable status field - status is always derived

use crate::core::broker::DbBroker;
use crate::core::db::{self, Connection, OptionalExtension, params};
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use crate::plugins::health::{HealthState, derive_health_from_connection};
use clap::{Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

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
pub struct ObligationValidationResult {
    pub obligation_id: String,
    pub derived_status: ObligationStatus,
    pub dependencies_satisfied: bool,
    pub proofs_satisfied: bool,
    pub commit_present: bool,
    pub validation_errors: Vec<String>,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphValidationResult {
    pub is_valid: bool,
    pub has_cycles: bool,
    pub cycle_errors: Vec<String>,
    pub dependency_errors: Vec<String>,
    pub unsatisfied_obligations: Vec<String>,
    pub missing_proofs: Vec<String>,
    pub missing_commits: Vec<String>,
    pub total_nodes: usize,
    pub total_edges: usize,
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
    root.join(schemas::LOCAL_DB_NAME)
}

pub fn initialize_obligation_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = obligation_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "obligation.init", |conn| {
        conn.execute(schemas::GOVERNANCE_DB_SCHEMA_OBLIGATIONS, [])?;
        conn.execute(schemas::GOVERNANCE_DB_SCHEMA_OBLIGATION_EDGES, [])?;
        // Obligation writes pass through the shared broker policy gate, which
        // reads risk_zones from the canonical datastore. Keep this table
        // available even when obligation initialization is the first local
        // subsystem touched in a fresh repository.
        conn.execute(schemas::TODO_DB_SCHEMA_RISK_ZONES, [])?;
        conn.execute(schemas::TODO_DB_SCHEMA_INDEX_RISK_ZONES_NAME, [])?;
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
    /// Verify current derived readiness without changing advisory status caches.
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
    /// Validate the entire obligation graph (cycles, dependencies, proofs, commits).
    ValidateGraph,
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
            println!("Obligation added: {id}");
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
            let result = derive_obligation_status(store, &id)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            if result.derived_status != ObligationStatus::Met {
                return Err(error::DecapodError::ValidationError(format!(
                    "Obligation {id} is not met: {}",
                    result.validation_errors.join("; ")
                )));
            }
        }
        ObligationCommand::ValidateGraph => {
            let result = validate_obligation_graph(store)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
            if !result.is_valid {
                return Err(error::DecapodError::ValidationError(
                    "Obligation graph is not satisfied; inspect the validation result".to_string(),
                ));
            }
        }
        ObligationCommand::Complete { id, commit } => {
            complete_obligation(store, &id, &commit)?;
            let (status, reason) = verify_obligation(store, &id)?;
            println!("Obligation {id} updated with commit {commit}.");
            println!("Status: {status:?}\nReason: {reason}");
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
    let id = crate::core::ulid::new_ulid();
    let now = crate::core::time::now_epoch_z();

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

    broker.with_transaction(&db_path, "decapod", None, "obligation.add", |conn| {
        // Check for cycles before adding edges
        for dep_id in &depends_on_ids {
            if read_obligation(conn, dep_id)?.is_none() {
                return Err(error::DecapodError::ValidationError(format!(
                    "Dependency not found: {dep_id}"
                )));
            }
            if detect_cycle(conn, dep_id, &id)? {
                return Err(error::DecapodError::ValidationError(format!(
                    "Circular dependency detected: {id} -> {dep_id}"
                )));
            }
        }

        conn.execute(
            "INSERT INTO obligations (id, intent_ref, risk_tier, required_proofs, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, intent, risk, proof_json, ObligationStatus::Open.as_str(), now, now],
        )?;

        for dep_id in depends_on_ids {
            let edge_id = crate::core::ulid::new_ulid();
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

const OBLIGATION_COLUMNS: &str = "id, intent_ref, risk_tier, required_proofs, state_commit_root, status, created_at, updated_at, metadata";

fn obligation_from_row(row: &db::Row<'_>) -> db::Result<ObligationNode> {
    let id: String = row.get(0)?;
    let proofs_json: String = row.get(3)?;
    let required_proofs: Vec<String> = serde_json::from_str(&proofs_json)
        .map_err(|err| db::Error::FromSqlConversionFailure(3, db::Type::Text, Box::new(err)))?;
    if required_proofs.iter().any(|proof| proof.trim().is_empty()) {
        return Err(db::Error::FromSqlConversionFailure(
            3,
            db::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Obligation {id} has an empty required proof"),
            )),
        ));
    }
    let metadata_json: Option<String> = row.get(8)?;
    Ok(ObligationNode {
        id,
        intent_ref: row.get(1)?,
        risk_tier: row.get(2)?,
        required_proofs,
        state_commit_root: row.get(4)?,
        // Retained for compatibility only. Readiness is always derived below.
        status: ObligationStatus::from_status_str(&row.get::<_, String>(5)?),
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        metadata: metadata_json.and_then(|s| serde_json::from_str(&s).ok()),
    })
}

fn read_obligation(
    conn: &Connection,
    id: &str,
) -> Result<Option<ObligationNode>, error::DecapodError> {
    conn.query_row(
        &format!("SELECT {OBLIGATION_COLUMNS} FROM obligations WHERE id = ?1"),
        params![id],
        obligation_from_row,
    )
    .optional()
    .map_err(error::DecapodError::StorageError)
}

fn read_obligations(conn: &Connection) -> Result<Vec<ObligationNode>, error::DecapodError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {OBLIGATION_COLUMNS} FROM obligations ORDER BY id"
    ))?;
    stmt.query_map([], obligation_from_row)?
        .collect::<db::Result<Vec<_>>>()
        .map_err(error::DecapodError::StorageError)
}

fn dependency_ids(conn: &Connection, id: &str) -> Result<Vec<String>, error::DecapodError> {
    // Read the edges directly: a join would silently discard dangling targets.
    let mut stmt =
        conn.prepare("SELECT to_id FROM obligation_edges WHERE from_id = ?1 ORDER BY to_id")?;
    stmt.query_map(params![id], |row| row.get(0))?
        .collect::<db::Result<Vec<_>>>()
        .map_err(error::DecapodError::StorageError)
}

pub fn list_obligations(store: &Store) -> Result<Vec<ObligationNode>, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.list",
        read_obligations,
    )
}

pub fn get_obligation(store: &Store, id: &str) -> Result<ObligationNode, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.get",
        |conn| read_obligation(conn, id)?.ok_or_else(|| missing_obligation(id)),
    )
}

fn missing_obligation(id: &str) -> error::DecapodError {
    error::DecapodError::ValidationError(format!("Obligation not found: {id}"))
}

pub fn detect_cycle(
    conn: &crate::core::db::Connection,
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
    let result = derive_obligation_status(store, id)?;
    let reason = if result.validation_errors.is_empty() {
        "All conditions satisfied".to_string()
    } else {
        result.validation_errors.join("; ")
    };
    Ok((result.derived_status, reason))
}

/// One derivation holds the broker's canonical read connection (and its local
/// storage coordination lock) for the entire dependency/proof walk. Capture
/// time once so neither cache refreshes nor an SLA boundary can mix snapshots.
struct Derivation<'a> {
    conn: &'a Connection,
    now_secs: i64,
    nodes: HashMap<String, ObligationNode>,
    dependencies: HashMap<String, Vec<String>>,
    results: HashMap<String, ObligationValidationResult>,
    cycle_errors: BTreeSet<String>,
    dependency_errors: BTreeSet<String>,
}

impl<'a> Derivation<'a> {
    fn new(conn: &'a Connection) -> Self {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        Self {
            conn,
            now_secs,
            nodes: HashMap::new(),
            dependencies: HashMap::new(),
            results: HashMap::new(),
            cycle_errors: BTreeSet::new(),
            dependency_errors: BTreeSet::new(),
        }
    }

    fn load(&mut self, id: &str) -> Result<bool, error::DecapodError> {
        if self.nodes.contains_key(id) {
            return Ok(true);
        }
        let Some(node) = read_obligation(self.conn, id)? else {
            return Ok(false);
        };
        self.dependencies
            .insert(id.to_string(), dependency_ids(self.conn, id)?);
        self.nodes.insert(id.to_string(), node);
        Ok(true)
    }

    fn derive(&mut self, id: &str) -> Result<ObligationValidationResult, error::DecapodError> {
        if !self.load(id)? {
            return Err(missing_obligation(id));
        }
        // Explicit enter/exit frames bound stack use for deep dependency chains.
        // Memoization shares results across diamonds and whole-graph validation.
        let mut stack = vec![(id.to_string(), false)];
        let mut visiting = HashSet::new();
        while let Some((current, exit)) = stack.pop() {
            if self.results.contains_key(&current) {
                continue;
            }
            if !exit {
                if visiting.contains(&current) {
                    self.cycle_errors
                        .insert(format!("Cycle reaches obligation {current}"));
                    continue;
                }
                if !self.load(&current)? {
                    continue;
                }
                visiting.insert(current.clone());
                stack.push((current.clone(), true));
                for dep in self.dependencies[&current].iter().rev() {
                    if !self.results.contains_key(dep) {
                        stack.push((dep.clone(), false));
                    }
                }
                continue;
            }

            visiting.remove(&current);
            let node = &self.nodes[&current];
            let mut errors = Vec::new();
            let mut dependencies_satisfied = true;
            for dep in &self.dependencies[&current] {
                if !self.nodes.contains_key(dep) {
                    dependencies_satisfied = false;
                    let message = format!("Missing dependency: {current} -> {dep}");
                    self.dependency_errors.insert(message.clone());
                    errors.push(message);
                } else if self
                    .results
                    .get(dep)
                    .is_none_or(|result| result.derived_status != ObligationStatus::Met)
                {
                    dependencies_satisfied = false;
                    errors.push(format!("Dependency not met: {dep} (including any cycle or unmet transitive evidence)"));
                }
            }
            let mut proofs_satisfied = true;
            for proof in &node.required_proofs {
                let health = derive_health_from_connection(self.conn, proof, self.now_secs)?;
                if !matches!(health.as_ref(), Some((_, HealthState::VERIFIED, _))) {
                    proofs_satisfied = false;
                    let reason = health
                        .map(|(_, _, reason)| reason)
                        .unwrap_or_else(|| "claim not found".to_string());
                    errors.push(format!("Proof not satisfied: {proof}: {reason}"));
                }
            }
            let commit_present = node
                .state_commit_root
                .as_deref()
                .is_some_and(|root| !root.trim().is_empty());
            if !commit_present {
                errors.push("STATE_COMMIT root missing".to_string());
            }
            let derived_status = if dependencies_satisfied && proofs_satisfied && commit_present {
                ObligationStatus::Met
            } else {
                ObligationStatus::Open
            };
            self.results.insert(
                current.clone(),
                ObligationValidationResult {
                    obligation_id: current,
                    derived_status,
                    dependencies_satisfied,
                    proofs_satisfied,
                    commit_present,
                    validation_errors: errors,
                    timestamp: format!("{}Z", self.now_secs),
                },
            );
        }
        Ok(self.results[id].clone())
    }
}

pub fn derive_obligation_status(
    store: &Store,
    id: &str,
) -> Result<ObligationValidationResult, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.check",
        |conn| Derivation::new(conn).derive(id),
    )
}

/// Publication rechecks explicit completion attempts and legacy met projections,
/// including their transitive dependencies. Unattempted backlog is not a gate.
pub(crate) fn derive_completion_attempts(
    store: &Store,
) -> Result<Vec<ObligationValidationResult>, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.completion.check",
        |conn| {
            // Select the requested admission boundary before parsing rows. An
            // unrelated backlog item, even a damaged one, is not a completion
            // attempt; malformed evidence on a selected root or dependency
            // still fails closed in the derivation.
            let mut stmt = conn.prepare(
                "SELECT id FROM obligations WHERE state_commit_root IS NOT NULL OR status = 'met' ORDER BY id",
            )?;
            let ids = stmt.query_map([], |row| row.get::<_, String>(0))?
                .collect::<db::Result<Vec<_>>>()?;
            let mut derivation = Derivation::new(conn);
            ids.into_iter().map(|id| derivation.derive(&id)).collect()
        },
    )
}

pub fn validate_obligation_graph(
    store: &Store,
) -> Result<GraphValidationResult, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.graph",
        |conn| {
            let obligations = read_obligations(conn)?;
            let ids: HashSet<_> = obligations.iter().map(|node| node.id.as_str()).collect();
            let mut derivation = Derivation::new(conn);
            let mut unsatisfied_obligations = Vec::new();
            let mut missing_proofs = Vec::new();
            let mut missing_commits = Vec::new();
            for obligation in &obligations {
                let result = derivation.derive(&obligation.id)?;
                if result.derived_status != ObligationStatus::Met {
                    unsatisfied_obligations.push(obligation.id.clone());
                }
                if !result.proofs_satisfied {
                    missing_proofs.push(obligation.id.clone());
                }
                if !result.commit_present {
                    missing_commits.push(obligation.id.clone());
                }
            }
            // Include orphan sources as well as dangling targets, even when no
            // existing obligation can reach an edge. Counts describe stored data.
            let mut stmt = conn
                .prepare("SELECT from_id, to_id FROM obligation_edges ORDER BY from_id, to_id")?;
            let edges = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut total_edges = 0;
            for edge in edges {
                let (from, to) = edge?;
                total_edges += 1;
                if !ids.contains(from.as_str()) || !ids.contains(to.as_str()) {
                    derivation
                        .dependency_errors
                        .insert(format!("Dangling dependency edge: {from} -> {to}"));
                }
            }
            let cycle_errors: Vec<_> = derivation.cycle_errors.into_iter().collect();
            let dependency_errors: Vec<_> = derivation.dependency_errors.into_iter().collect();
            Ok(GraphValidationResult {
                is_valid: cycle_errors.is_empty()
                    && dependency_errors.is_empty()
                    && unsatisfied_obligations.is_empty(),
                has_cycles: !cycle_errors.is_empty(),
                cycle_errors,
                dependency_errors,
                unsatisfied_obligations,
                missing_proofs,
                missing_commits,
                total_nodes: obligations.len(),
                total_edges,
            })
        },
    )
}

pub fn get_dependencies(
    store: &Store,
    id: &str,
) -> Result<Vec<ObligationNode>, error::DecapodError> {
    DbBroker::new(&store.root).with_conn(
        &obligation_db_path(&store.root),
        "decapod",
        None,
        "obligation.dependencies.get",
        |conn| {
            if read_obligation(conn, id)?.is_none() {
                return Err(missing_obligation(id));
            }
            dependency_ids(conn, id)?
                .into_iter()
                .map(|dep| {
                    read_obligation(conn, &dep)?.ok_or_else(|| {
                        error::DecapodError::ValidationError(format!(
                            "Missing dependency: {id} -> {dep}"
                        ))
                    })
                })
                .collect()
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
    let now = crate::core::time::now_epoch_z();
    if commit.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "STATE_COMMIT root must not be empty".to_string(),
        ));
    }

    broker.with_conn(&db_path, "decapod", None, "obligation.complete", |conn| {
        let changed = conn.execute(
            "UPDATE obligations SET state_commit_root = ?1, updated_at = ?2 WHERE id = ?3",
            params![commit, now, id],
        )?;
        if changed == 0 {
            return Err(missing_obligation(id));
        }
        Ok(())
    })
}

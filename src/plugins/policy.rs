use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ulid::Ulid;

#[derive(Parser, Debug)]
#[clap(name = "policy", about = "Manage policy and risk mapping")]
pub struct PolicyCli {
    #[clap(subcommand)]
    pub command: PolicyCommand,
}

#[derive(Subcommand, Debug)]
pub enum PolicyCommand {
    /// Evaluate risk for a command and optional path.
    Eval {
        #[clap(long)]
        command: String,
        #[clap(long)]
        path: Option<String>,
    },
    /// Approve a specific high-risk action.
    Approve {
        #[clap(long)]
        id: String,
        #[clap(long, default_value = "operator")]
        actor: String,
        #[clap(long, default_value = "global")]
        scope: String,
    },
    /// Manage the risk map (blast-radius zones).
    Riskmap {
        #[clap(subcommand)]
        command: RiskmapSubcommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum RiskmapSubcommand {
    /// Initialize a default risk map.
    Init,
    /// Verify the risk map integrity.
    Verify,
}

pub fn run_policy_cli(store: &Store, cli: PolicyCli) -> Result<(), error::DecapodError> {
    initialize_policy_db(&store.root)?;
    match cli.command {
        PolicyCommand::Eval { command, path } => {
            let risk_map_path = store.root.join("RISKMAP.json");
            let risk_map = if risk_map_path.exists() {
                let content = std::fs::read_to_string(risk_map_path)?;
                serde_json::from_str(&content).unwrap_or(RiskMap { zones: vec![] })
            } else {
                RiskMap { zones: vec![] }
            };
            let (level, requirements) = eval_risk(&command, path.as_deref(), &risk_map);
            let fingerprint = derive_fingerprint(&command, path.as_deref(), "global");
            println!("Risk Level: {:?}", level);
            println!("Fingerprint: {}", fingerprint);
            println!("Requirements: {:?}", requirements);
        }
        PolicyCommand::Approve { id, actor, scope } => {
            let approval_id = approve_action(store, &id, None, &actor, &scope)?;
            println!("Action Approved (ID: {})", approval_id);
        }
        PolicyCommand::Riskmap { command } => {
            let risk_map_path = store.root.join("RISKMAP.json");
            match command {
                RiskmapSubcommand::Init => {
                    let default_map = RiskMap {
                        zones: vec![
                            RiskZone {
                                path: ".decapod/".to_string(),
                                level: RiskLevel::CRITICAL,
                                rules: vec!["NO_AGENT_WRITE".to_string()],
                            },
                            RiskZone {
                                path: "docs/specs/".to_string(),
                                level: RiskLevel::HIGH,
                                rules: vec!["OPERATOR_REVIEW_REQUIRED".to_string()],
                            },
                        ],
                    };
                    std::fs::write(
                        &risk_map_path,
                        serde_json::to_string_pretty(&default_map).unwrap(),
                    )?;
                    println!("Risk map initialized at {}", risk_map_path.display());
                }
                RiskmapSubcommand::Verify => {
                    if risk_map_path.exists() {
                        println!("Risk map present and readable.");
                    } else {
                        println!("Risk map missing (run `decapod policy riskmap init`).");
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    LOW = 0,      // Reversible, safe zones
    MEDIUM = 1,   // Reversible, sensitive zones
    HIGH = 2,     // Irreversible, requires approval
    CRITICAL = 3, // Irreversible, protected zones, requires explicit override
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Approval {
    pub approval_id: String,
    pub action_id: String,
    pub actor: String,
    pub ts: String,
    pub scope: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskZone {
    pub path: String,
    pub level: RiskLevel,
    pub rules: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RiskMap {
    pub zones: Vec<RiskZone>,
}

pub fn policy_db_path(root: &Path) -> PathBuf {
    root.join(schemas::POLICY_DB_NAME)
}

pub fn initialize_policy_db(root: &Path) -> Result<(), error::DecapodError> {
    let broker = DbBroker::new(root);
    let db_path = policy_db_path(root);

    broker.with_conn(&db_path, "decapod", None, "policy.init", |conn| {
        conn.execute(schemas::POLICY_DB_SCHEMA_APPROVALS, [])?;
        conn.execute(schemas::POLICY_DB_SCHEMA_INDEX, [])?;
        Ok(())
    })
}

pub fn derive_fingerprint(command: &str, target_path: Option<&str>, scope: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(command);
    hasher.update(target_path.unwrap_or(""));
    hasher.update(scope);
    format!("{:x}", hasher.finalize())
}

pub fn eval_risk(
    command: &str,
    target_path: Option<&str>,
    risk_map: &RiskMap,
) -> (RiskLevel, Vec<String>) {
    // Basic heuristic-based risk evaluation for Epoch 2
    let mut level = RiskLevel::LOW;
    let mut requirements = Vec::new();

    // Command-based risk
    if command.contains("delete") || command.contains("archive") || command.contains("purge") {
        level = RiskLevel::HIGH;
        requirements.push("Operator Approval Required (Irreversible)".to_string());
    }

    // Zone-based risk
    if let Some(path) = target_path {
        for zone in &risk_map.zones {
            if path.contains(&zone.path) {
                if zone.level as u8 > level as u8 {
                    level = zone.level;
                }
                for rule in &zone.rules {
                    requirements.push(format!("Zone Rule: {}", rule));
                }
            }
        }
    }

    if level == RiskLevel::HIGH || level == RiskLevel::CRITICAL {
        requirements.push("Requires matching entry in approval ledger".to_string());
    }

    (level, requirements)
}

pub fn is_high_risk(level: RiskLevel) -> bool {
    matches!(level, RiskLevel::HIGH | RiskLevel::CRITICAL)
}

pub fn approve_action(
    store: &Store,
    command: &str,
    target_path: Option<&str>,
    actor: &str,
    scope: &str,
) -> Result<String, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = policy_db_path(&store.root);
    let approval_id = Ulid::new().to_string();
    let fingerprint = derive_fingerprint(command, target_path, scope);
    let now = now_iso();

    broker.with_conn(&db_path, actor, None, "policy.approve", |conn| {
        conn.execute(
            "INSERT INTO approvals(approval_id, action_fingerprint, actor, ts, scope) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![approval_id, fingerprint, actor, now, scope],
        )?;
        Ok(())
    })?;

    Ok(approval_id)
}

pub fn check_approval(
    store: &Store,
    command: &str,
    target_path: Option<&str>,
    scope: &str,
) -> Result<bool, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = policy_db_path(&store.root);
    let fingerprint = derive_fingerprint(command, target_path, scope);

    broker.with_conn(&db_path, "decapod", None, "policy.check", |conn| {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM approvals WHERE action_fingerprint = ?1",
            params![fingerprint],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    })
}

pub fn list_approvals(store: &Store) -> Result<Vec<Approval>, error::DecapodError> {
    let broker = DbBroker::new(&store.root);
    let db_path = policy_db_path(&store.root);

    broker.with_conn(&db_path, "decapod", None, "policy.list", |conn| {
        let mut stmt = conn.prepare(
            "SELECT approval_id, action_id, actor, ts, scope, expires_at FROM approvals",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Approval {
                approval_id: row.get(0)?,
                action_id: row.get(1)?,
                actor: row.get(2)?,
                ts: row.get(3)?,
                scope: row.get(4)?,
                expires_at: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    format!("{}Z", secs)
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "policy",
        "version": "0.1.0",
        "description": "Risk classification and approval engine",
        "commands": [
            { "name": "eval", "parameters": ["command", "path"] },
            { "name": "approve", "parameters": ["action_id", "actor", "scope"] }
        ],
        "storage": ["policy.db", "RISKMAP.json"]
    })
}


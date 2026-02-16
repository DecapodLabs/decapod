use crate::core::assets;
use crate::core::broker::DbBroker;
use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::{Parser, Subcommand};
use rusqlite::{OptionalExtension, params};
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
            let hitl_required = human_in_loop_required(store, "global", level, is_high_risk(level));
            println!("Risk Level: {:?}", level);
            println!("Fingerprint: {}", fingerprint);
            println!("Requirements: {:?}", requirements);
            println!("Human-in-the-loop Required: {}", hitl_required);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HitlRuleAction {
    Enable,
    Disable,
}

#[derive(Debug, Clone)]
struct HitlRule {
    action: HitlRuleAction,
    scope: Option<String>,
    risk_exact: Option<RiskLevel>,
    min_risk: Option<RiskLevel>,
    max_risk: Option<RiskLevel>,
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

fn parse_risk_level(raw: &str) -> Option<RiskLevel> {
    match raw.trim().to_lowercase().as_str() {
        "low" => Some(RiskLevel::LOW),
        "medium" => Some(RiskLevel::MEDIUM),
        "high" => Some(RiskLevel::HIGH),
        "critical" => Some(RiskLevel::CRITICAL),
        _ => None,
    }
}

fn trim_markdown_prefix(line: &str) -> &str {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return rest.trim();
    }
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return rest.trim();
    }
    trimmed
}

fn parse_hitl_directive(line: &str) -> Option<HitlRule> {
    let normalized = trim_markdown_prefix(line).replace('`', "");
    let mut parts = normalized.split_whitespace();
    let head = parts.next()?.to_uppercase();
    let action = match head.as_str() {
        "HITL_DISABLE" => HitlRuleAction::Disable,
        "HITL_ENABLE" => HitlRuleAction::Enable,
        _ => return None,
    };

    let mut scope = None;
    let mut risk_exact = None;
    let mut min_risk = None;
    let mut max_risk = None;

    for token in parts {
        let Some((key, value)) = token.split_once('=') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = value.trim();
        match key.as_str() {
            "scope" => {
                if !value.is_empty() {
                    scope = Some(value.to_string());
                }
            }
            "risk" | "risk_level" => risk_exact = parse_risk_level(value),
            "min_risk" => min_risk = parse_risk_level(value),
            "max_risk" => max_risk = parse_risk_level(value),
            _ => {}
        }
    }

    Some(HitlRule {
        action,
        scope,
        risk_exact,
        min_risk,
        max_risk,
    })
}

fn parse_hitl_override_rules(override_section: &str) -> Vec<HitlRule> {
    let mut rules = Vec::new();

    for line in override_section.lines() {
        let normalized = trim_markdown_prefix(line).to_lowercase();
        if normalized.is_empty() {
            continue;
        }

        // Natural-language fast path for operators: `HITL: I don't want human in the loop`.
        if normalized.starts_with("hitl:")
            && (normalized.contains("don't want human in the loop")
                || normalized.contains("do not want human in the loop")
                || normalized.contains("no human in the loop"))
        {
            rules.push(HitlRule {
                action: HitlRuleAction::Disable,
                scope: None,
                risk_exact: None,
                min_risk: None,
                max_risk: None,
            });
        }

        if let Some(rule) = parse_hitl_directive(line) {
            rules.push(rule);
        }
    }

    rules
}

fn rule_matches(rule: &HitlRule, scope: &str, level: RiskLevel) -> bool {
    if let Some(required_scope) = &rule.scope {
        if required_scope != scope {
            return false;
        }
    }
    if let Some(exact) = rule.risk_exact {
        if exact != level {
            return false;
        }
    }
    if let Some(min) = rule.min_risk {
        if (level as u8) < (min as u8) {
            return false;
        }
    }
    if let Some(max) = rule.max_risk {
        if (level as u8) > (max as u8) {
            return false;
        }
    }
    true
}

fn rule_specificity(rule: &HitlRule) -> usize {
    let mut score = 0;
    if rule.scope.is_some() {
        score += 8;
    }
    if rule.risk_exact.is_some() {
        score += 4;
    }
    if rule.min_risk.is_some() {
        score += 2;
    }
    if rule.max_risk.is_some() {
        score += 2;
    }
    score
}

fn find_repo_root_from_store(store: &Store) -> Option<PathBuf> {
    let mut current = Some(store.root.as_path());
    while let Some(path) = current {
        if path.join(".decapod").join("OVERRIDE.md").exists() {
            return Some(path.to_path_buf());
        }
        current = path.parent();
    }
    None
}

pub fn is_hitl_disabled_by_override(store: &Store, scope: &str, level: RiskLevel) -> bool {
    let Some(repo_root) = find_repo_root_from_store(store) else {
        return false;
    };
    let Some(policy_override) = assets::get_override_doc(&repo_root, "plugins/POLICY.md") else {
        return false;
    };

    let rules = parse_hitl_override_rules(&policy_override);
    if rules.is_empty() {
        return false;
    }

    let mut best_match: Option<(usize, usize, HitlRuleAction)> = None;
    for (idx, rule) in rules.iter().enumerate() {
        if !rule_matches(rule, scope, level) {
            continue;
        }
        let specificity = rule_specificity(rule);
        let candidate = (specificity, idx, rule.action);
        match best_match {
            None => best_match = Some(candidate),
            Some((best_specificity, best_idx, _)) => {
                if specificity > best_specificity
                    || (specificity == best_specificity && idx > best_idx)
                {
                    best_match = Some(candidate);
                }
            }
        }
    }

    matches!(best_match, Some((_, _, HitlRuleAction::Disable)))
}

pub fn human_in_loop_required(
    store: &Store,
    scope: &str,
    level: RiskLevel,
    approval_required_by_policy: bool,
) -> bool {
    if !approval_required_by_policy {
        return false;
    }
    !is_hitl_disabled_by_override(store, scope, level)
}

fn trust_level_to_int(level: &str) -> i32 {
    match level {
        "untrusted" => 0,
        "basic" => 1,
        "verified" => 2,
        "core" => 3,
        _ => 1,
    }
}

pub fn is_read_only_operation(op_name: &str) -> bool {
    let read_suffixes = [
        ".get",
        ".list",
        ".search",
        ".graph",
        ".schema",
        ".map",
        ".check",
        ".status",
        ".summary",
        ".targets",
        ".presence",
        ".ownerships",
        ".categories",
        ".expertise",
    ];
    read_suffixes.iter().any(|s| op_name.ends_with(s))
        || op_name.ends_with(".init")
        || op_name == "policy.check"
        || op_name == "policy.list"
}

fn required_trust_for_operation(op_name: &str) -> &'static str {
    if op_name.starts_with("federation.")
        && (op_name.contains("edit")
            || op_name.contains("supersede")
            || op_name.contains("deprecate")
            || op_name.contains("dispute")
            || op_name.contains("unlink")
            || op_name.contains("rebuild"))
    {
        return "verified";
    }
    if op_name.contains("archive")
        || op_name.contains("delete")
        || op_name.contains("purge")
        || op_name.contains("rebuild")
    {
        return "verified";
    }
    "basic"
}

fn risk_level_for_operation(op_name: &str) -> RiskLevel {
    if op_name.contains("archive")
        || op_name.contains("delete")
        || op_name.contains("purge")
        || op_name.contains("rebuild")
        || op_name.contains("supersede")
        || op_name.contains("dispute")
        || op_name.contains("deprecate")
    {
        return RiskLevel::HIGH;
    }
    RiskLevel::LOW
}

fn risk_zone_for_operation(op_name: &str) -> &'static str {
    if op_name.starts_with("todo.") {
        return "todo.claim.exclusive";
    }
    if op_name.starts_with("federation.") {
        return "federation.mutate";
    }
    if op_name.starts_with("decide.") {
        return "decisioning.mutate";
    }
    if op_name.starts_with("policy.") {
        return "policy.control";
    }
    if op_name.starts_with("teammate.") {
        return "teammate.mutate";
    }
    "control.mutate"
}

fn zone_policy_from_todo(
    root: &Path,
    zone_name: &str,
) -> Result<Option<(String, bool)>, error::DecapodError> {
    let todo_db = root.join(schemas::TODO_DB_NAME);
    if !todo_db.exists() {
        return Ok(None);
    }
    let conn = rusqlite::Connection::open(todo_db).map_err(error::DecapodError::RusqliteError)?;
    let res = conn
        .query_row(
            "SELECT required_trust_level, requires_approval FROM risk_zones WHERE zone_name = ?1",
            params![zone_name],
            |row| {
                let trust: String = row.get(0)?;
                let requires_approval: i64 = row.get(1)?;
                Ok((trust, requires_approval != 0))
            },
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(res)
}

fn actor_trust_level_raw(root: &Path, actor: &str) -> Result<String, error::DecapodError> {
    if actor == "decapod" || actor == "cli" || actor == "operator" {
        return Ok("core".to_string());
    }
    let todo_db = root.join(schemas::TODO_DB_NAME);
    if !todo_db.exists() {
        return Ok("basic".to_string());
    }
    let conn = rusqlite::Connection::open(todo_db).map_err(error::DecapodError::RusqliteError)?;
    let level: Option<String> = conn
        .query_row(
            "SELECT trust_level FROM agent_trust WHERE agent_id = ?1",
            params![actor],
            |row| row.get(0),
        )
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;
    Ok(level.unwrap_or_else(|| "basic".to_string()))
}

pub fn enforce_broker_mutation_policy(
    root: &Path,
    actor: &str,
    op_name: &str,
) -> Result<(), error::DecapodError> {
    if is_read_only_operation(op_name) {
        return Ok(());
    }

    let required_trust = required_trust_for_operation(op_name);
    let actor_trust = actor_trust_level_raw(root, actor)?;
    if trust_level_to_int(&actor_trust) < trust_level_to_int(required_trust) {
        return Err(error::DecapodError::ValidationError(format!(
            "Policy gate denied for '{}': actor '{}' trust '{}' < required '{}'",
            op_name, actor, actor_trust, required_trust
        )));
    }

    let risk = risk_level_for_operation(op_name);
    let zone_name = risk_zone_for_operation(op_name);
    if let Some((zone_trust, zone_requires_approval)) = zone_policy_from_todo(root, zone_name)? {
        if trust_level_to_int(&actor_trust) < trust_level_to_int(&zone_trust) {
            return Err(error::DecapodError::ValidationError(format!(
                "Policy gate denied for '{}': zone '{}' requires trust '{}' (actor '{}')",
                op_name, zone_name, zone_trust, actor_trust
            )));
        }
        if zone_requires_approval {
            let store = Store {
                kind: crate::core::store::StoreKind::Repo,
                root: root.to_path_buf(),
            };
            let high = matches!(risk, RiskLevel::HIGH | RiskLevel::CRITICAL);
            if human_in_loop_required(&store, zone_name, risk, high)
                && !check_approval(&store, zone_name, None, "global")?
            {
                return Err(error::DecapodError::ValidationError(format!(
                    "Policy gate denied for '{}': zone '{}' requires approval",
                    op_name, zone_name
                )));
            }
        }
    }

    Ok(())
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
    crate::core::time::now_epoch_z()
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

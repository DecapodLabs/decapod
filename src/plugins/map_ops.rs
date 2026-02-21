//! Deterministic Map Operators — structured parallel processing with scope-reduction enforcement.
//!
//! Two operator modes:
//! - `map llm`: Stateless parallel processing with prompt template + schema validation.
//! - `map agentic`: Subagent delegation with mandatory scope-reduction (`--retain`).
//!
//! Both operators are *structural* — they define the contract and audit trail.
//! Actual LLM/subagent dispatch is pluggable.

use crate::core::error;
use crate::core::schemas;
use crate::core::store::Store;
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use ulid::Ulid;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

fn map_events_path(root: &Path) -> PathBuf {
    root.join(schemas::MAP_EVENTS_NAME)
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MapEvent {
    pub event_id: String,
    pub ts: String,
    pub actor: String,
    pub op: String,
    pub item_count: usize,
    pub prompt_hash: Option<String>,
    pub schema_hash: Option<String>,
    pub delegate_hash: Option<String>,
    pub retain: Option<String>,
    pub status: String,
    pub result_hash: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct MapItemResult {
    index: usize,
    item_hash: String,
    status: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_iso() -> String {
    crate::core::time::now_epoch_z()
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn append_jsonl(path: &Path, value: &serde_json::Value) -> Result<(), error::DecapodError> {
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(error::DecapodError::IoError)?;
    writeln!(f, "{}", serde_json::to_string(value).unwrap())
        .map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn load_items(items_str: &str) -> Result<Vec<serde_json::Value>, error::DecapodError> {
    // Try parsing as JSON array directly
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(items_str) {
        return Ok(arr);
    }
    // Try as file path
    let path = Path::new(items_str);
    if path.exists() {
        let content = fs::read_to_string(path).map_err(error::DecapodError::IoError)?;
        let arr: Vec<serde_json::Value> = serde_json::from_str(&content).map_err(|e| {
            error::DecapodError::ValidationError(format!("Failed to parse items file: {}", e))
        })?;
        return Ok(arr);
    }
    Err(error::DecapodError::ValidationError(
        "Items must be a valid JSON array or a path to a JSON file".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Core operations
// ---------------------------------------------------------------------------

/// Execute a stateless map-llm operation.
pub fn map_llm(
    store: &Store,
    items_str: &str,
    prompt: &str,
    schema_str: &str,
    actor: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    let items = load_items(items_str)?;
    if items.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "Items array must not be empty".to_string(),
        ));
    }

    let prompt_hash = sha256_hex(prompt.as_bytes());
    let schema_hash = sha256_hex(schema_str.as_bytes());

    // Validate schema is parseable JSON
    let _schema: serde_json::Value = serde_json::from_str(schema_str)
        .map_err(|e| error::DecapodError::ValidationError(format!("Invalid schema JSON: {}", e)))?;

    // Process each item structurally (contract definition, not execution)
    let mut results: Vec<MapItemResult> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let item_json = serde_json::to_string(item).unwrap();
        let item_hash = sha256_hex(item_json.as_bytes());
        results.push(MapItemResult {
            index: i,
            item_hash,
            status: "pending".to_string(),
        });
    }

    // Deterministic result hash
    let results_json = serde_json::to_string(&results).unwrap();
    let result_hash = sha256_hex(results_json.as_bytes());

    let event_id = Ulid::new().to_string();
    let ts = now_iso();

    let event = serde_json::json!({
        "event_id": event_id,
        "ts": ts,
        "actor": actor,
        "op": "map.llm",
        "item_count": items.len(),
        "prompt_hash": prompt_hash,
        "schema_hash": schema_hash,
        "status": "completed",
        "result_hash": result_hash,
    });

    append_jsonl(&map_events_path(&store.root), &event)?;

    Ok(serde_json::json!({
        "event_id": event_id,
        "op": "map.llm",
        "item_count": items.len(),
        "prompt_hash": prompt_hash,
        "schema_hash": schema_hash,
        "result_hash": result_hash,
        "items": results,
    }))
}

/// Execute a map-agentic operation with scope-reduction enforcement.
pub fn map_agentic(
    store: &Store,
    items_str: &str,
    delegate: &str,
    retain: &str,
    actor: &str,
) -> Result<serde_json::Value, error::DecapodError> {
    // Scope-reduction invariant: --retain must be non-empty
    if retain.trim().is_empty() {
        return Err(error::DecapodError::ValidationError(
            "Delegation without retention violates scope-reduction invariant".to_string(),
        ));
    }

    let items = load_items(items_str)?;
    if items.is_empty() {
        return Err(error::DecapodError::ValidationError(
            "Items array must not be empty".to_string(),
        ));
    }

    let delegate_hash = sha256_hex(delegate.as_bytes());

    // Log each item delegation
    let mut results: Vec<MapItemResult> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let item_json = serde_json::to_string(item).unwrap();
        let item_hash = sha256_hex(item_json.as_bytes());
        results.push(MapItemResult {
            index: i,
            item_hash,
            status: "delegated".to_string(),
        });
    }

    let results_json = serde_json::to_string(&results).unwrap();
    let result_hash = sha256_hex(results_json.as_bytes());

    let event_id = Ulid::new().to_string();
    let ts = now_iso();

    let event = serde_json::json!({
        "event_id": event_id,
        "ts": ts,
        "actor": actor,
        "op": "map.agentic",
        "item_count": items.len(),
        "delegate_hash": delegate_hash,
        "retain": retain,
        "status": "completed",
        "result_hash": result_hash,
    });

    append_jsonl(&map_events_path(&store.root), &event)?;

    Ok(serde_json::json!({
        "event_id": event_id,
        "op": "map.agentic",
        "item_count": items.len(),
        "delegate_hash": delegate_hash,
        "retain": retain,
        "result_hash": result_hash,
        "items": results,
    }))
}

/// Read all map events from the JSONL audit trail.
pub fn read_map_events(root: &Path) -> Result<Vec<MapEvent>, error::DecapodError> {
    let path = map_events_path(root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    let mut events = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let event: MapEvent = serde_json::from_str(trimmed)
            .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?;
        events.push(event);
    }
    Ok(events)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(clap::Args, Debug)]
pub struct MapCli {
    #[clap(subcommand)]
    pub command: MapCommand,
}

#[derive(Subcommand, Debug)]
pub enum MapCommand {
    /// Stateless parallel processing: apply prompt+schema to each item
    Llm {
        /// JSON array or path to JSON file containing items
        #[clap(long)]
        items: String,
        /// Prompt template to apply to each item
        #[clap(long)]
        prompt: String,
        /// JSON schema for output validation
        #[clap(long)]
        schema: String,
        /// Actor identifier
        #[clap(long, default_value = "decapod")]
        actor: String,
    },
    /// Subagent map with scope-reduction enforcement
    Agentic {
        /// JSON array or path to JSON file containing items
        #[clap(long)]
        items: String,
        /// Delegation prompt for subagents
        #[clap(long)]
        delegate: String,
        /// What the caller retains responsibility for (REQUIRED for scope-reduction)
        #[clap(long)]
        retain: String,
        /// Actor identifier
        #[clap(long, default_value = "decapod")]
        actor: String,
    },
    /// Emit subsystem schema JSON
    Schema,
}

pub fn run_map_cli(store: &Store, cli: MapCli) -> Result<(), error::DecapodError> {
    match cli.command {
        MapCommand::Llm {
            items,
            prompt,
            schema,
            actor,
        } => {
            let result = map_llm(store, &items, &prompt, &schema, &actor)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        MapCommand::Agentic {
            items,
            delegate,
            retain,
            actor,
        } => {
            let result = map_agentic(store, &items, &delegate, &retain, &actor)?;
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        MapCommand::Schema => {
            println!("{}", serde_json::to_string_pretty(&schema()).unwrap());
        }
    }
    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "map",
        "version": "0.1.0",
        "description": "Deterministic map operators — structured parallel processing with scope-reduction",
        "commands": [
            { "name": "llm", "description": "Stateless parallel processing with prompt+schema" },
            { "name": "agentic", "description": "Subagent map with scope-reduction enforcement" },
            { "name": "schema", "description": "Emit subsystem schema JSON" },
        ],
        "storage": [schemas::MAP_EVENTS_NAME],
        "invariants": [
            "map agentic requires --retain (scope-reduction invariant)",
            "All operations are logged to append-only map.events.jsonl",
            "Deterministic: same items + same prompt + same schema → same result_hash",
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_store() -> (tempfile::TempDir, Store) {
        let tmp = tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        fs::create_dir_all(&root).unwrap();
        let store = Store {
            kind: crate::core::store::StoreKind::Repo,
            root,
        };
        (tmp, store)
    }

    #[test]
    fn test_map_llm_rejects_empty_items() {
        let (_tmp, store) = test_store();
        let result = map_llm(&store, "[]", "prompt", "{}", "agent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must not be empty"));
    }

    #[test]
    fn test_map_agentic_rejects_empty_retain() {
        let (_tmp, store) = test_store();
        let result = map_agentic(&store, "[\"item1\"]", "delegate prompt", "", "agent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("scope-reduction"));
    }

    #[test]
    fn test_map_agentic_logs_delegation() {
        let (_tmp, store) = test_store();
        let result = map_agentic(
            &store,
            "[\"item1\", \"item2\"]",
            "do the thing",
            "orchestration",
            "agent",
        )
        .unwrap();
        assert_eq!(result["item_count"].as_u64().unwrap(), 2);
        assert_eq!(result["retain"].as_str().unwrap(), "orchestration");

        let events = read_map_events(&store.root).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].op, "map.agentic");
    }

    #[test]
    fn test_map_llm_produces_result() {
        let (_tmp, store) = test_store();
        let result = map_llm(
            &store,
            "[\"a\", \"b\", \"c\"]",
            "summarize: {{item}}",
            "{\"type\": \"object\"}",
            "agent",
        )
        .unwrap();
        assert_eq!(result["item_count"].as_u64().unwrap(), 3);
        assert!(result["result_hash"].as_str().is_some());
    }
}

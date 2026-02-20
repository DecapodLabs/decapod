use crate::core::error::DecapodError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceEvent {
    pub trace_id: String,
    pub ts: String,
    pub actor: String,
    pub op: String,
    pub request: Value,
    pub response: Value,
}

pub fn redact(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted_map = Map::new();
            for (key, val) in map {
                let lower_key = key.to_lowercase();
                if lower_key.contains("token")
                    || lower_key.contains("secret")
                    || lower_key.contains("password")
                    || lower_key.contains("api_key")
                    || lower_key.contains("authorization")
                {
                    redacted_map.insert(key, Value::String("[REDACTED]".to_string()));
                } else {
                    redacted_map.insert(key, redact(val));
                }
            }
            Value::Object(redacted_map)
        }
        Value::Array(vec) => Value::Array(vec.into_iter().map(redact).collect()),
        _ => value,
    }
}

pub fn append_trace(project_root: &Path, event: TraceEvent) -> Result<(), DecapodError> {
    let trace_path = project_root.join(".decapod/data/traces.jsonl");

    // Ensure parent directory exists
    if let Some(parent) = trace_path.parent() {
        std::fs::create_dir_all(parent).map_err(DecapodError::IoError)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_path)
        .map_err(DecapodError::IoError)?;

    let redacted_event = TraceEvent {
        trace_id: event.trace_id,
        ts: event.ts,
        actor: event.actor,
        op: event.op,
        request: redact(event.request),
        response: redact(event.response),
    };

    let json = serde_json::to_string(&redacted_event)
        .map_err(|e| DecapodError::ValidationError(e.to_string()))?;
    writeln!(file, "{}", json).map_err(DecapodError::IoError)?;

    Ok(())
}

pub fn get_last_traces(project_root: &Path, n: usize) -> Result<Vec<String>, DecapodError> {
    let trace_path = project_root.join(".decapod/data/traces.jsonl");
    if !trace_path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(trace_path).map_err(DecapodError::IoError)?;
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let start = if lines.len() > n { lines.len() - n } else { 0 };
    Ok(lines[start..].to_vec())
}

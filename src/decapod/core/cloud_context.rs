//! Repository operational context over the existing Dactyl storage route.
use crate::cli::KnowledgeCommand;
use crate::core::backend::StorageContext;
use crate::core::dactyl::DactylBridge;
use crate::core::error::DecapodError;
use crate::plugins::knowledge::{KnowledgeContext, KnowledgeContextRecord};
use dactyl_db::{AccessMode, MutationOutcome, Operation, OperationResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

const READ_SQL: &str = "SELECT repository_context_v1($1) AS snapshot";
const APPEND_SQL: &str = "INSERT INTO repository_context_v1 (record) VALUES ($1)";
const SCHEMA_VERSION: &str = "decapod.operational-context.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudContextObservation {
    pub schema_version: String,
    pub revision: u64,
    pub todo_event_seq: u64,
    pub task: Option<CloudContextTask>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudContextTask {
    pub id: String,
    pub status: String,
    pub version: u64,
    pub assignee: Option<String>,
    pub principal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordRequest {
    id: String,
    kind: String,
    scope: String,
    title: String,
    content: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    supersedes_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    #[serde(flatten)]
    request: RecordRequest,
    principal: String,
    recorded_at: String,
    revision: u64,
    superseded: bool,
    written_by_requester: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    #[serde(flatten)]
    observation: CloudContextObservation,
    records: Vec<Record>,
}

fn read_snapshot(
    context: &StorageContext,
    filter: serde_json::Value,
) -> Result<Snapshot, DecapodError> {
    let bridge = DactylBridge::from_storage_context(context, AccessMode::ReadOnly)?;
    let response = bridge.read(READ_SQL, &[filter.to_string().into()])?;
    let Some(row) = response
        .as_slice()
        .first()
        .filter(|_| response.as_slice().len() == 1)
    else {
        return Err(protocol_error());
    };
    let json: String = row.get("snapshot")?;
    let snapshot: Snapshot = serde_json::from_str(&json).map_err(|_| protocol_error())?;
    if snapshot.observation.schema_version != SCHEMA_VERSION || snapshot.records.len() > 16 {
        return Err(protocol_error());
    }
    Ok(snapshot)
}

fn protocol_error() -> DecapodError {
    DecapodError::ValidationError("Invalid hosted operational context response".into())
}

pub(crate) fn reason(error: &DecapodError) -> Option<String> {
    match error {
        DecapodError::CloudAuth(diagnostic) => serde_json::to_value(diagnostic.status)
            .ok()
            .and_then(|status| status.as_str().map(str::to_owned)),
        DecapodError::DactylError(error) => Some(
            error
                .adapter_code()
                .unwrap_or("transport_unavailable")
                .to_owned(),
        ),
        _ => Some("context_resolution_failed".into()),
    }
}
fn connect(repo_root: &Path) -> Result<StorageContext, DecapodError> {
    let identity = crate::core::repo_identity::resolve_repository_identity(repo_root)?;
    crate::core::cloud_client::connect(repo_root, &identity, false, false)
}

fn scopes_for_paths(paths: &[String]) -> BTreeSet<String> {
    let mut scopes = BTreeSet::new();
    for path in paths.iter().take(64) {
        let path = Path::new(path);
        if path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        {
            for parent in path.ancestors().take_while(|p| !p.as_os_str().is_empty()) {
                if scopes.len() == 64 {
                    return scopes;
                }
                if let Some(parent) = parent.to_str().filter(|value| value.len() <= 507) {
                    scopes.insert(format!("path:{parent}"));
                }
            }
        }
    }
    scopes
}

pub(crate) fn resolve(
    repo_root: &Path,
    query: Option<&str>,
    task_id: Option<&str>,
    touched_paths: &[String],
) -> Result<KnowledgeContext, DecapodError> {
    let context = connect(repo_root)?;
    let mut filter = json!({ "scopes": scopes_for_paths(touched_paths) });
    if let Some(query) = query {
        filter["query"] = json!(query);
    }
    if let Some(id) = task_id {
        filter["task_id"] = json!(id);
    }
    let snapshot = read_snapshot(&context, filter)?;
    Ok(compose_context(snapshot))
}

fn compose_context(snapshot: Snapshot) -> KnowledgeContext {
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&snapshot).unwrap_or_default())
    );
    let mut packet = KnowledgeContext {
        schema_version: "decapod.knowledge-context.v1".into(),
        status: if snapshot.records.is_empty() {
            "no_relevant_advisory_knowledge"
        } else {
            "available"
        }
        .into(),
        source_hash: hash,
        cloud_observation: Some(snapshot.observation),
        ..Default::default()
    };
    for record in snapshot.records.into_iter().filter(|item| !item.superseded) {
        let request = record.request;
        if packet.records.len() == 8 {
            packet.excluded.insert("cloud_context_budget".into(), 1);
            break;
        }
        let excerpt: String = request.content.chars().take(320).collect();
        packet.content_bytes += excerpt.len();
        packet.records.push(KnowledgeContextRecord {
            id: request.id,
            title: request.title,
            excerpt,
            scope: request.scope,
            status: "active".into(),
            provenance: vec![request.source],
            provenance_class: format!("hosted_{}", request.kind),
            freshness: "observed_at_snapshot".into(),
            authority: "advisory".into(),
            source_hash: format!("{:x}", Sha256::digest(request.content.as_bytes())),
            reasons: vec!["authorized_repository_context".into()],
            source_observation: BTreeMap::from([
                ("recorded_by".into(), record.principal),
                ("recorded_at".into(), record.recorded_at),
                ("revision".into(), record.revision.to_string()),
            ]),
            supersedes_id: request.supersedes_id,
            feedback_summary: None,
        });
    }
    packet
}

fn lookup_request(
    context: &StorageContext,
    request: &RecordRequest,
) -> Result<Option<Record>, DecapodError> {
    let response = read_snapshot(context, json!({ "record_id": request.id }))?;
    let Some(record) = response.records.into_iter().next() else {
        return Ok(None);
    };
    if !record.written_by_requester || record.request != *request {
        return Err(DecapodError::ValidationError(
            "CLOUD_CONTEXT_CONFLICT: record ID already belongs to a different request".into(),
        ));
    }
    Ok(Some(record))
}

fn append(context: &StorageContext, request: &RecordRequest) -> Result<Record, DecapodError> {
    // An explicit immutable record ID is also the operation identity across
    // process restarts. Check authorization and exact request before replay.
    if let Some(receipt) = lookup_request(context, request)? {
        return Ok(receipt);
    }
    let bridge = DactylBridge::from_storage_context(context, AccessMode::ReadWrite)?;
    let payload = serde_json::to_string(request).map_err(|_| protocol_error())?;
    let mutation = bridge.atomic(&[Operation::write(APPEND_SQL, vec![payload.into()])]);
    match mutation {
        Ok(result) if matches!(result.results.as_slice(), [OperationResult::Write(result)] if result.affected_rows <= 1) =>
            {}
        Ok(_) => return Err(protocol_error()),
        Err(DecapodError::DactylError(error))
            if error.mutation_outcome() == MutationOutcome::Uncertain => {}
        Err(error) => return Err(error),
    }
    match lookup_request(context, request) {
        Ok(Some(receipt)) => Ok(receipt),
        _ => Err(DecapodError::ValidationError(format!(
            "CLOUD_CONTEXT_OUTCOME_UNKNOWN: immutable record '{}' requires readback. Preserve the same ID and arguments.",
            request.id
        ))),
    }
}

pub(crate) fn supported(command: &KnowledgeCommand) -> bool {
    matches!(
        command,
        KnowledgeCommand::Add { .. }
            | KnowledgeCommand::Search { .. }
            | KnowledgeCommand::Show { .. }
    )
}

pub(crate) fn run_command(
    repo_root: &Path,
    command: &KnowledgeCommand,
) -> Result<(), DecapodError> {
    let context = connect(repo_root)?;
    let output = match command {
        KnowledgeCommand::Add { id, kind, title, text, provenance, claim_id, merge_key, on_conflict, supersedes, scope } => {
            if claim_id.is_some() || merge_key.is_some()
                || (on_conflict != "merge" && !(on_conflict == "supersede" && supersedes.is_some())) {
                return Err(DecapodError::ValidationError(
                    "CLOUD_CONTEXT_PROFILE_UNSUPPORTED: hosted records use immutable IDs and explicit --supersedes".into(),
                ));
            }
            let record = append(&context, &RecordRequest {
                id: id.clone(),
                kind: kind.clone(),
                scope: scope.clone(),
                title: title.clone(),
                content: text.clone(),
                source: provenance.clone(),
                supersedes_id: supersedes.clone(),
            })?;
            serde_json::to_value(record)
        }
        KnowledgeCommand::Search { query } => serde_json::to_value(read_snapshot(&context, json!({ "query": query }))?),
        KnowledgeCommand::Show { id } => serde_json::to_value(read_snapshot(&context, json!({ "record_id": id }))?),
        KnowledgeCommand::Promote { .. } => return Err(crate::core::cloud_backend::unavailable_error()),
    }.map_err(|_| protocol_error())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|_| protocol_error())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_scopes_use_only_repository_relative_paths_and_ancestors() {
        let paths = ["src/app/worker.rs", "/tmp/private", "../private", "docs"].map(str::to_owned);
        assert_eq!(
            scopes_for_paths(&paths).into_iter().collect::<Vec<_>>(),
            [
                "path:docs",
                "path:src",
                "path:src/app",
                "path:src/app/worker.rs"
            ]
        );
    }

    #[test]
    fn context_preserves_the_hosted_observation_and_current_provenance() {
        let record = |id: &str, superseded: bool| Record {
            request: RecordRequest {
                id: id.into(),
                kind: "decision".into(),
                scope: "repo".into(),
                title: "Build policy".into(),
                content: "Use the signed build".into(),
                source: "issue:1110".into(),
                supersedes_id: (!superseded).then(|| "old".into()),
            },
            principal: "github:101".into(),
            recorded_at: "2026-11-27T12:00:00Z".into(),
            revision: if superseded { 1 } else { 2 },
            superseded,
            written_by_requester: false,
        };
        let snapshot = Snapshot {
            observation: CloudContextObservation {
                schema_version: SCHEMA_VERSION.into(),
                revision: 2,
                todo_event_seq: 4,
                task: None,
                truncated: false,
            },
            records: vec![record("current", false), record("old", true)],
        };
        let packet = compose_context(snapshot);
        assert_eq!(packet.cloud_observation.unwrap().todo_event_seq, 4);
        assert_eq!(packet.records.len(), 1);
        let current = &packet.records[0];
        assert_eq!(current.id, "current");
        assert_eq!(current.supersedes_id.as_deref(), Some("old"));
        assert_eq!(current.provenance, ["issue:1110"]);
        assert_eq!(current.source_observation["recorded_by"], "github:101");
        assert_eq!(current.authority, "advisory");
    }
}

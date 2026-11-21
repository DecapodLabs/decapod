//! Database broker for serialized state access (The Thin Waist).
//!
//! This module provides the core state mutation control plane for Decapod.
//! Stateful operations route through this layer to ensure
//! serialization, auditability, and deterministic replay.

use crate::core::db::Connection;
use crate::core::error;
use crate::core::events;
use crate::core::pool;
use crate::core::store::find_decapod_project_root;
use crate::core::time;
use crate::plugins::policy;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Database broker providing serialized access to Decapod state.
///
/// The DbBroker is the "Thin Waist" control plane for all state mutations.
/// It provides read/write access with proper locking and full audit trail.
pub struct DbBroker {
    root: PathBuf,
    approval_target: Option<String>,
}

#[derive(Clone)]
struct CacheEntry {
    value: JsonValue,
    expires_at: Instant,
}

/// Audit event for a brokered database operation.
///
/// Every call to `DbBroker::with_conn` generates a `BrokerEvent` that is
/// appended to the canonical `broker_events` table for full mutation audit trail.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BrokerEvent {
    /// Envelope schema version for machine consumers.
    #[serde(default = "default_broker_schema_version")]
    pub schema_version: String,
    /// Request identifier used by orchestrators and adapters.
    #[serde(default)]
    pub request_id: String,
    /// ISO 8601 timestamp (seconds since epoch + 'Z')
    pub ts: String,
    /// Unique event identifier (ULID)
    pub event_id: String,
    /// Actor who initiated the operation (e.g., "cli", "agent", "watcher")
    pub actor: String,
    /// Canonical actor identifier (same as actor for now; explicit for envelope stability).
    #[serde(default)]
    pub actor_id: String,
    /// Originating agent, distinct from a subsystem's service actor label.
    /// Empty on historical v1.0 events; never infer historical caller custody.
    #[serde(default)]
    pub caller_agent_id: String,
    /// Optional runtime session identifier for multi-call workflows.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Correlation ID for grouping related operations.
    #[serde(default)]
    pub correlation_id: Option<String>,
    /// Causation ID that links this event to a parent event/request.
    #[serde(default)]
    pub causation_id: Option<String>,
    /// Optional idempotency key set by orchestrator/runtime.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Optional reference to an intent or session ID
    pub intent_ref: Option<String>,
    /// Operation name (e.g., "todo.add", "health.record")
    pub op: String,
    /// Logical datastore identifier (the local adapter uses decapod.db).
    pub db_id: String,
    /// Operation status ("success" or "error")
    pub status: String,
    /// OS process ID of the writer that initiated this operation, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid: Option<u32>,
    /// OS process start-time / incarnation fingerprint for `writer_pid` to detect PID reuse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_start_id: Option<String>,
    /// OS boot / session incarnation identifier on the originating host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_boot_id: Option<String>,
    /// Persistent host identifier to distinguish same-host reboots from cross-host writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_host_id: Option<String>,
    /// Linux PID namespace identifier (`/proc/self/ns/pid`) when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid_ns: Option<String>,
}

/// Captured OS writer incarnation provenance for a brokered pending write.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterIncarnation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_start_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_boot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid_ns: Option<String>,
}

/// Verifiable liveness classification for a pending audit event's originating writer.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WriterLiveness {
    Active,
    Stopped,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WriterAssessment {
    liveness: WriterLiveness,
    human_review_required: bool,
    detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProcessProbe {
    Absent,
    Running { start_id: Option<String> },
    Unverifiable,
}

trait IncarnationProbe {
    fn current_boot_id(&self) -> Option<String>;
    fn current_host_id(&self) -> Option<String>;
    fn current_pid_ns(&self) -> Option<String>;
    fn probe_process(&self, pid: u32) -> ProcessProbe;
}

struct RealIncarnationProbe;

impl IncarnationProbe for RealIncarnationProbe {
    fn current_boot_id(&self) -> Option<String> {
        os_current_boot_id()
    }

    fn current_host_id(&self) -> Option<String> {
        os_current_host_id()
    }

    fn current_pid_ns(&self) -> Option<String> {
        os_current_pid_ns()
    }

    fn probe_process(&self, pid: u32) -> ProcessProbe {
        os_probe_process(pid)
    }
}

impl WriterIncarnation {
    /// Capture writer incarnation provenance for the current OS process, falling
    /// back field-by-field to `None` when OS metadata is unavailable.
    pub fn current() -> Self {
        let probe = RealIncarnationProbe;
        let pid = std::process::id();
        let writer_start_id = match probe.probe_process(pid) {
            ProcessProbe::Running { start_id } => start_id,
            ProcessProbe::Absent | ProcessProbe::Unverifiable => None,
        };
        Self {
            writer_pid: Some(pid),
            writer_start_id,
            writer_boot_id: probe.current_boot_id(),
            writer_host_id: probe.current_host_id(),
            writer_pid_ns: probe.current_pid_ns(),
        }
    }
}

impl DbBroker {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            approval_target: None,
        }
    }

    /// Bind policy checks for every operation on this instance to one resource.
    /// Use a fresh broker for a different resource. Operation/zone-name
    /// compatibility fallback is reserved for brokers without a target.
    pub fn with_target(mut self, target: &str) -> Self {
        self.approval_target = Some(target.to_string());
        self
    }

    pub fn is_cloud(&self) -> bool {
        let project_root =
            find_decapod_project_root(&self.root).unwrap_or_else(|_| self.root.clone());
        if crate::core::db_maintenance::resolve_accepted_project_config(&project_root)
            .ok()
            .is_some_and(|(_main, cfg, _)| cfg.repo.effective_backend().is_cloud())
        {
            return true;
        }
        if let Ok(config) = crate::cli::DecapodProjectConfig::load(&project_root) {
            return config.repo.effective_backend().is_cloud();
        }
        false
    }

    /// Execute a write through the queue (synchronous for now) and return its
    /// affected-row count. IDs belong to the caller; this API never reads
    /// connection-local generated-ID state.
    pub fn execute_write_sync(
        &self,
        db_path: &Path,
        sql: &str,
        params: &[(&str, i64)],
    ) -> Result<u64, error::DecapodError> {
        let sql = sql.to_string();
        let params: Vec<i64> = params.iter().map(|(_, v)| *v).collect();
        let db_path_owned = db_path.to_path_buf();

        let affected_rows = pool::global_pool().with_write(db_path, |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let param_vec: Vec<Box<dyn crate::core::db::ToSql>> = params
                .iter()
                .map(|v| Box::new(*v) as Box<dyn crate::core::db::ToSql>)
                .collect();
            let params_refs: Vec<&dyn crate::core::db::ToSql> =
                param_vec.iter().map(|p| p.as_ref()).collect();
            Ok(stmt.execute(params_refs.as_slice())? as u64)
        })?;
        log_write_event(&self.root, "queued_write", &db_path_owned)?;
        Ok(affected_rows)
    }

    /// Execute a closure with a serialized connection to the specified DB.
    pub fn with_conn<F, R>(
        &self,
        db_path: &Path,
        actor: &str,
        intent_ref: Option<&str>,
        op_name: &str,
        f: F,
    ) -> Result<R, error::DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, error::DecapodError>,
    {
        let is_read = policy::is_read_only_operation(op_name) && !op_name.ends_with(".init"); // .init ops do DDL writes; route through write pool
        let effective_intent = if let Some(i) = intent_ref {
            Some(i.to_string())
        } else if !is_read {
            Some(format!(
                "intent:auto:{}:{}",
                op_name,
                crate::core::ulid::new_ulid()
            ))
        } else {
            None
        };

        if !is_read {
            policy::enforce_broker_mutation_policy_for_target(
                &self.root,
                actor,
                op_name,
                self.approval_target.as_deref(),
            )?;
        }

        let db_id = db_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        if is_read {
            // Read path: use pooled read connection (no mutex serialization)
            let result = pool::global_pool().with_read(db_path, f);
            let status = if result.is_ok() { "success" } else { "error" };
            self.log_event(actor, effective_intent.as_deref(), op_name, &db_id, status)?;
            result
        } else {
            // Write path: acquire the operation lock once and keep the two-phase
            // audit record on the same connection. Logging the pending event
            // through `events::append` before acquiring this connection used to
            // make a contended claim wait through two independent lock timeouts.
            pool::global_pool().with_write(db_path, |conn| {
                self.log_event_on_conn(
                    conn,
                    actor,
                    effective_intent.as_deref(),
                    op_name,
                    &db_id,
                    "pending",
                )?;

                let result = f(conn);
                let status = if result.is_ok() { "success" } else { "error" };
                self.log_event_on_conn(
                    conn,
                    actor,
                    effective_intent.as_deref(),
                    op_name,
                    &db_id,
                    status,
                )?;
                result
            })
        }
    }

    /// Execute a state mutation and its audit/event writes as one SQLite
    /// transaction. The transaction is acquired only when the connection is
    /// not already inside a caller-owned transaction, so migration and
    /// rebuild paths can safely compose this primitive.
    pub fn with_transaction<F, R>(
        &self,
        db_path: &Path,
        actor: &str,
        intent_ref: Option<&str>,
        op_name: &str,
        f: F,
    ) -> Result<R, error::DecapodError>
    where
        F: FnOnce(&Connection) -> Result<R, error::DecapodError>,
    {
        self.with_conn(db_path, actor, intent_ref, op_name, |conn| {
            if !conn.is_autocommit() {
                return f(conn);
            }

            conn.execute_batch("BEGIN IMMEDIATE")?;
            match f(conn) {
                Ok(value) => {
                    conn.execute_batch("COMMIT")?;
                    Ok(value)
                }
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(error)
                }
            }
        })
    }

    fn log_event(
        &self,
        actor: &str,
        intent_ref: Option<&str>,
        op: &str,
        db_id: &str,
        status: &str,
    ) -> Result<(), error::DecapodError> {
        // Match the writer's pool -> audit lock order. Taking the audit lock
        // before events::append could deadlock a read-completion audit against
        // a writer that already held this database's operation lock.
        let path = events::canonical_db_path(&self.root);
        pool::global_pool().with_write(&path, |conn| {
            self.log_event_on_conn(conn, actor, intent_ref, op, db_id, status)
        })
    }

    fn log_event_on_conn(
        &self,
        conn: &Connection,
        actor: &str,
        intent_ref: Option<&str>,
        op: &str,
        db_id: &str,
        status: &str,
    ) -> Result<(), error::DecapodError> {
        let ev = self.build_event(actor, intent_ref, op, db_id, status)?;
        let audit_lock = get_audit_lock();
        let _audit_guard = audit_lock
            .lock()
            .map_err(|_| error::DecapodError::ValidationError("Audit lock poisoned".into()))?;
        events::ensure_tables(conn)?;
        events::append_on_conn(
            conn,
            events::BROKER,
            &serde_json::to_value(ev)
                .map_err(|e| error::DecapodError::ValidationError(e.to_string()))?,
        )?;
        Ok(())
    }

    fn build_event(
        &self,
        actor: &str,
        intent_ref: Option<&str>,
        op: &str,
        db_id: &str,
        status: &str,
    ) -> Result<BrokerEvent, error::DecapodError> {
        let ts = time::now_epoch_z();
        let request_id =
            env::var("DECAPOD_GROUP_BROKER_REQUEST_ID").unwrap_or_else(|_| time::new_event_id());
        let event_id = time::new_event_id();
        let session_id = env::var("DECAPOD_SESSION_ID").ok();
        let correlation_id = env::var("DECAPOD_CORRELATION_ID")
            .ok()
            .or_else(|| intent_ref.map(|s| s.to_string()));
        let causation_id = env::var("DECAPOD_CAUSATION_ID").ok();
        let idempotency_key = env::var("DECAPOD_IDEMPOTENCY_KEY").ok();
        let incarnation = if status == "pending" {
            WriterIncarnation::current()
        } else {
            WriterIncarnation::default()
        };

        Ok(BrokerEvent {
            schema_version: "1.1.0".into(),
            request_id,
            ts,
            event_id,
            actor: actor.to_string(),
            actor_id: actor.to_string(),
            caller_agent_id: crate::current_agent_id(),
            session_id,
            correlation_id,
            causation_id,
            idempotency_key,
            intent_ref: intent_ref.map(|s| s.to_string()),
            op: op.to_string(),
            db_id: db_id.to_string(),
            status: status.to_string(),
            writer_pid: incarnation.writer_pid,
            writer_start_id: incarnation.writer_start_id,
            writer_boot_id: incarnation.writer_boot_id,
            writer_host_id: incarnation.writer_host_id,
            writer_pid_ns: incarnation.writer_pid_ns,
        })
    }

    fn cache_compound_key(db_path: &Path, scope: &str, key: &str) -> String {
        format!("{}::{}::{}", db_path.to_string_lossy(), scope, key)
    }

    pub fn cache_get_json(db_path: &Path, scope: &str, key: &str) -> Option<JsonValue> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let cache = broker_read_cache();
        let mut map = cache.lock().ok()?;
        if let Some(entry) = map.get(&compound)
            && entry.expires_at > Instant::now()
        {
            return Some(entry.value.clone());
        }
        map.remove(&compound);
        None
    }

    pub fn cache_put_json(
        db_path: &Path,
        scope: &str,
        key: &str,
        value: JsonValue,
        ttl_secs: u64,
    ) -> Result<(), error::DecapodError> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let expires_at = Instant::now()
            .checked_add(Duration::from_secs(ttl_secs.max(1)))
            .unwrap_or_else(Instant::now);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.insert(compound, CacheEntry { value, expires_at });
        Ok(())
    }

    pub fn cache_invalidate_scope(db_path: &Path, scope: &str) -> Result<(), error::DecapodError> {
        let prefix = format!("{}::{}::", db_path.to_string_lossy(), scope);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.retain(|k, _| !k.starts_with(&prefix));
        Ok(())
    }

    pub fn cache_invalidate_key(
        db_path: &Path,
        scope: &str,
        key: &str,
    ) -> Result<(), error::DecapodError> {
        let compound = Self::cache_compound_key(db_path, scope, key);
        let mut map = broker_read_cache().lock().map_err(|_| {
            error::DecapodError::ValidationError("broker read cache lock poisoned".to_string())
        })?;
        map.remove(&compound);
        Ok(())
    }

    /// Verify log integrity and detect potential crash-induced divergence.
    ///
    /// Scans the audit log for `pending` events that lack a corresponding
    /// terminal `success` or `error` event, which indicates a process crash
    /// between the two-phase commit boundaries.
    pub fn verify_replay(&self) -> Result<ReplayReport, error::DecapodError> {
        let events = events::query(&self.root, events::BROKER, usize::MAX)?;
        Ok(replay_report_with_probe(
            &decode_events(events)?,
            &RealIncarnationProbe,
        ))
    }

    /// Acknowledge one interrupted audit operation without asserting its data
    /// mutation failed or undoing it. Applying requires proven writer
    /// termination; unknown/legacy writers fail closed unless explicitly
    /// acknowledged after human review via `repair_with_options`.
    pub fn repair(
        &self,
        event_id: &str,
        reason: &str,
        apply: bool,
        actor: &str,
    ) -> Result<RepairReport, error::DecapodError> {
        self.repair_with_options(event_id, reason, apply, false, actor)
    }

    /// Preview or apply an exact-event abandonment acknowledgment with optional
    /// explicit human-reviewed confirmation for `unknown` writer liveness.
    pub fn repair_with_options(
        &self,
        event_id: &str,
        reason: &str,
        apply: bool,
        acknowledge_unknown_writer: bool,
        actor: &str,
    ) -> Result<RepairReport, error::DecapodError> {
        self.repair_with_probe(
            event_id,
            reason,
            apply,
            acknowledge_unknown_writer,
            actor,
            &RealIncarnationProbe,
        )
    }

    fn repair_with_probe(
        &self,
        event_id: &str,
        reason: &str,
        apply: bool,
        acknowledge_unknown_writer: bool,
        actor: &str,
        probe: &impl IncarnationProbe,
    ) -> Result<RepairReport, error::DecapodError> {
        if event_id.trim().is_empty() || reason.trim().is_empty() {
            return Err(error::DecapodError::ValidationError(
                "Broker repair requires a pending event ID and a non-empty reason confirming the original writer has stopped.".into(),
            ));
        }
        if !apply {
            let events = decode_events(events::query(&self.root, events::BROKER, usize::MAX)?)?;
            return prepare_repair_with_probe(
                &events,
                event_id,
                reason,
                false,
                acknowledge_unknown_writer,
                probe,
            );
        }
        let db_path = events::canonical_db_path(&self.root);
        self.with_transaction(&db_path, actor, None, "broker.repair", |conn| {
            let events = decode_events(events::query_all_on_conn(conn, events::BROKER)?)?;
            let mut report = prepare_repair_with_probe(
                &events,
                event_id,
                reason,
                true,
                acknowledge_unknown_writer,
                probe,
            )?;
            if report.status == "already_abandoned" {
                return Ok(report);
            }
            let pending = events
                .iter()
                .find(|event| event.event_id == event_id)
                .expect("prepare_repair_with_probe checked the pending event");
            let acknowledgment = BrokerEvent {
                schema_version: "1.1.0".into(),
                request_id: time::new_event_id(),
                event_id: time::new_event_id(),
                ts: time::now_epoch_z(),
                actor: actor.to_string(),
                actor_id: actor.to_string(),
                caller_agent_id: crate::current_agent_id(),
                session_id: env::var("DECAPOD_SESSION_ID").ok(),
                correlation_id: pending.correlation_id.clone(),
                causation_id: Some(event_id.to_string()),
                idempotency_key: Some(format!("broker.repair:{event_id}")),
                intent_ref: pending.intent_ref.clone(),
                op: "broker.repair".into(),
                db_id: pending.db_id.clone(),
                status: "abandoned".into(),
                writer_pid: None,
                writer_start_id: None,
                writer_boot_id: None,
                writer_host_id: None,
                writer_pid_ns: None,
            };
            let mut payload = serde_json::to_value(&acknowledgment)
                .map_err(|error| error::DecapodError::ValidationError(error.to_string()))?;
            payload["reason"] = serde_json::json!(reason);
            payload["original_op"] = serde_json::json!(pending.op);
            payload["original_outcome"] = serde_json::json!("unknown");
            payload["writer_liveness"] = serde_json::to_value(report.writer_liveness)
                .map_err(|error| error::DecapodError::ValidationError(error.to_string()))?;
            payload["human_review_required"] = serde_json::json!(report.human_review_required);
            payload["acknowledge_unknown_writer"] = serde_json::json!(acknowledge_unknown_writer);
            events::append_on_conn(conn, events::BROKER, &payload)?;
            report.status = "abandoned".into();
            report.acknowledgment_event_id = Some(acknowledgment.event_id);
            Ok(report)
        })
    }
}

fn decode_events(
    events: Vec<events::StoredEvent>,
) -> Result<Vec<BrokerEvent>, error::DecapodError> {
    events
        .into_iter()
        .rev()
        .map(|event| {
            serde_json::from_value(event.payload).map_err(|error| {
                error::DecapodError::ValidationError(format!("Invalid broker event: {error}"))
            })
        })
        .collect()
}

fn non_empty_opt(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn assess_writer_liveness(event: &BrokerEvent, probe: &impl IncarnationProbe) -> WriterAssessment {
    let (Some(pid), Some(start_id), Some(boot_id)) = (
        event.writer_pid.filter(|&pid| pid > 0),
        non_empty_opt(event.writer_start_id.as_deref()),
        non_empty_opt(event.writer_boot_id.as_deref()),
    ) else {
        return WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: "Pending event lacks complete writer incarnation provenance (legacy event or unavailable OS metadata); writer termination cannot be established automatically."
                .to_string(),
        };
    };

    let current_boot = probe.current_boot_id();
    let current_boot = non_empty_opt(current_boot.as_deref());
    let Some(cur_boot) = current_boot else {
        return WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: "Current host boot incarnation is unavailable; cannot verify writer liveness."
                .to_string(),
        };
    };

    let event_host = non_empty_opt(event.writer_host_id.as_deref());
    let current_host = probe.current_host_id();
    let current_host = non_empty_opt(current_host.as_deref());

    if cur_boot != boot_id {
        if let (Some(ev_host), Some(cur_host)) = (event_host, current_host)
            && ev_host == cur_host
        {
            return WriterAssessment {
                liveness: WriterLiveness::Stopped,
                human_review_required: false,
                detail: format!(
                    "Writer originated in prior boot incarnation '{boot_id}' on host '{cur_host}' (current boot '{cur_boot}')."
                ),
            };
        }
        return WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: format!(
                "Writer boot incarnation '{boot_id}' differs from current boot '{cur_boot}' and same-host identity cannot be verified (possible cross-host write)."
            ),
        };
    }

    if let (Some(ev_host), Some(cur_host)) = (event_host, current_host)
        && ev_host != cur_host
    {
        return WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: format!(
                "Writer host identity '{ev_host}' differs from current host '{cur_host}'; cannot verify cross-host writer liveness."
            ),
        };
    }

    let event_pid_ns = non_empty_opt(event.writer_pid_ns.as_deref());
    let current_pid_ns = probe.current_pid_ns();
    let current_pid_ns = non_empty_opt(current_pid_ns.as_deref());
    if (event_pid_ns.is_some() || current_pid_ns.is_some()) && event_pid_ns != current_pid_ns {
        return WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: "Writer originated in a different or unverifiable PID namespace; cannot verify PID liveness."
                .to_string(),
        };
    }

    match probe.probe_process(pid) {
        ProcessProbe::Absent => WriterAssessment {
            liveness: WriterLiveness::Stopped,
            human_review_required: false,
            detail: format!(
                "Original writer pid={pid} (start_id={start_id}) is absent in current boot '{cur_boot}'."
            ),
        },
        ProcessProbe::Running {
            start_id: Some(observed_start),
        } => {
            if observed_start == start_id {
                WriterAssessment {
                    liveness: WriterLiveness::Active,
                    human_review_required: false,
                    detail: format!(
                        "Original writer pid={pid} (start_id={start_id}) is still active in current boot '{cur_boot}'."
                    ),
                }
            } else {
                WriterAssessment {
                    liveness: WriterLiveness::Stopped,
                    human_review_required: false,
                    detail: format!(
                        "Original writer pid={pid} (start_id={start_id}) has terminated; PID was reused by a different process (observed start_id={observed_start})."
                    ),
                }
            }
        }
        ProcessProbe::Running { start_id: None } | ProcessProbe::Unverifiable => WriterAssessment {
            liveness: WriterLiveness::Unknown,
            human_review_required: true,
            detail: format!(
                "Process pid={pid} exists or cannot be inspected in current boot '{cur_boot}', and its start_id could not be verified."
            ),
        },
    }
}

fn replay_report_with_probe(events: &[BrokerEvent], probe: &impl IncarnationProbe) -> ReplayReport {
    let mut pending_map = HashMap::new();
    let mut total_events = 0usize;

    for ev in events {
        total_events += 1;

        match ev.status.as_str() {
            "pending" => {
                pending_map.insert(ev.event_id.clone(), ev.clone());
            }
            "success" | "error" => {
                // Match terminal events to pending events by intent_ref + op + db_id.
                // When both have matching intent_ref (including both None), clear the pending.
                pending_map.retain(|_, v| {
                    let intent_match = match (&v.intent_ref, &ev.intent_ref) {
                        (Some(a), Some(b)) => a == b,
                        (None, None) => true,
                        _ => false,
                    };
                    !(intent_match && v.op == ev.op && v.db_id == ev.db_id)
                });
            }
            "abandoned" if ev.op == "broker.repair" => {
                if let Some(event_id) = &ev.causation_id {
                    pending_map.remove(event_id);
                }
            }
            _ => {}
        }
    }

    let mut divergences: Vec<_> = pending_map
        .into_values()
        .map(|ev| {
            let assessment = assess_writer_liveness(&ev, probe);
            Divergence {
                event_id: ev.event_id,
                op: ev.op,
                db_id: ev.db_id,
                ts: ev.ts,
                intent_ref: ev.intent_ref,
                reason: "Pending event without terminal status (potential crash)".to_string(),
                writer_liveness: assessment.liveness,
                human_review_required: assessment.human_review_required,
                liveness_detail: assessment.detail,
                writer_pid: ev.writer_pid,
                writer_start_id: ev.writer_start_id,
                writer_boot_id: ev.writer_boot_id,
                writer_host_id: ev.writer_host_id,
                writer_pid_ns: ev.writer_pid_ns,
            }
        })
        .collect();
    divergences.sort_by(|a, b| a.event_id.cmp(&b.event_id));

    ReplayReport {
        ts: time::now_epoch_z(),
        divergences,
        total_events,
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RepairReport {
    pub status: String,
    pub event_id: String,
    pub reason: String,
    pub original_outcome: String,
    pub acknowledgment_event_id: Option<String>,
    pub writer_liveness: WriterLiveness,
    pub human_review_required: bool,
    pub liveness_detail: String,
    pub acknowledgment_required: bool,
    pub acknowledged_unknown_writer: bool,
}

fn prepare_repair_with_probe(
    events: &[BrokerEvent],
    event_id: &str,
    reason: &str,
    apply: bool,
    acknowledge_unknown_writer: bool,
    probe: &impl IncarnationProbe,
) -> Result<RepairReport, error::DecapodError> {
    let pending = events
        .iter()
        .find(|event| event.event_id == event_id && event.status == "pending")
        .ok_or_else(|| {
            error::DecapodError::ValidationError(format!(
                "Broker repair target '{event_id}' is not a pending audit event. Run `decapod data broker verify` to select an event."
            ))
        })?;
    let acknowledgment = events.iter().find(|event| {
        event.op == "broker.repair"
            && event.status == "abandoned"
            && event.causation_id.as_deref() == Some(event_id)
    });
    let assessment = assess_writer_liveness(pending, probe);
    let acknowledgment_required = assessment.liveness == WriterLiveness::Unknown;
    if acknowledgment.is_none() {
        if !replay_report_with_probe(events, probe)
            .divergences
            .iter()
            .any(|event| event.event_id == event_id)
        {
            return Err(error::DecapodError::ValidationError(format!(
                "Broker repair target '{event_id}' already has a terminal status; no abandonment was written."
            )));
        }
        match assessment.liveness {
            WriterLiveness::Active => {
                return Err(error::DecapodError::ValidationError(format!(
                    "Refusing broker repair for '{event_id}': {} Stop the original writer before repairing.",
                    assessment.detail
                )));
            }
            WriterLiveness::Unknown if apply && !acknowledge_unknown_writer => {
                return Err(error::DecapodError::ValidationError(format!(
                    "Refusing silent broker repair for '{event_id}': {} Human review is required; inspect `decapod data broker verify` / `audit`, confirm the original writer has stopped, and re-run with `--acknowledge-unknown-writer`.",
                    assessment.detail
                )));
            }
            WriterLiveness::Stopped | WriterLiveness::Unknown => {}
        }
    }
    Ok(RepairReport {
        status: if acknowledgment.is_some() {
            "already_abandoned"
        } else {
            "preview"
        }
        .into(),
        event_id: event_id.into(),
        reason: reason.into(),
        original_outcome: "unknown".into(),
        acknowledgment_event_id: acknowledgment.map(|event| event.event_id.clone()),
        writer_liveness: assessment.liveness,
        human_review_required: assessment.human_review_required,
        liveness_detail: assessment.detail,
        acknowledgment_required,
        acknowledged_unknown_writer: acknowledge_unknown_writer,
    })
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReplayReport {
    pub ts: String,
    pub divergences: Vec<Divergence>,
    pub total_events: usize,
}

fn log_write_event(root: &Path, op: &str, db_path: &Path) -> Result<(), error::DecapodError> {
    let event = serde_json::json!({
        "op": op,
        "db": db_path.file_name().map(|s| s.to_string_lossy().to_string()),
        "ts": time::now_epoch_z(),
    });

    let _ = events::append(root, events::BROKER, &event)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Divergence {
    pub event_id: String,
    pub op: String,
    pub db_id: String,
    pub ts: String,
    pub intent_ref: Option<String>,
    pub reason: String,
    pub writer_liveness: WriterLiveness,
    pub human_review_required: bool,
    pub liveness_detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_start_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_boot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_host_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writer_pid_ns: Option<String>,
}

fn get_audit_lock() -> &'static Mutex<()> {
    static AUDIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    AUDIT_LOCK.get_or_init(|| Mutex::new(()))
}

fn broker_read_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    static READ_CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();
    READ_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "broker",
        "version": "0.2.0",
        "description": "State mutation broker (The Thin Waist)",
        "commands": [
            { "name": "audit", "description": "Show the mutation audit log" },
            { "name": "verify", "description": "Detect pending audit events without terminal evidence and assess writer incarnation liveness; not a SQLite integrity check" },
            { "name": "repair", "description": "Preview or append an exact-event abandonment acknowledgment; original data outcome remains unknown", "parameters": ["event_id", "reason", "apply", "acknowledge_unknown_writer"] }
        ],
        "envelope": {
            "schema_version": "1.1.0",
            "fields": [
                "schema_version",
                "request_id",
                "event_id",
                "ts",
                "actor",
                "actor_id",
                "caller_agent_id",
                "session_id",
                "correlation_id",
                "causation_id",
                "idempotency_key",
                "intent_ref",
                "op",
                "db_id",
                "status",
                "writer_pid",
                "writer_start_id",
                "writer_boot_id",
                "writer_host_id",
                "writer_pid_ns"
            ]
        },
        "storage": ["decapod.db:broker_events"]
    })
}

fn default_broker_schema_version() -> String {
    "1.0.0".to_string()
}

#[cfg(target_os = "macos")]
fn macos_sysctl_string(name: &std::ffi::CStr) -> Option<String> {
    let mut len: libc::size_t = 0;
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || len == 0 || len > 4096 {
        return None;
    }
    let mut buf = vec![0u8; len];
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    buf.truncate(len);
    while buf.last() == Some(&0) {
        buf.pop();
    }
    let text = String::from_utf8(buf).ok()?.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

#[cfg(target_os = "macos")]
fn os_current_boot_id() -> Option<String> {
    macos_sysctl_string(c"kern.bootsessionuuid")
}

#[cfg(target_os = "macos")]
fn os_current_host_id() -> Option<String> {
    let mut uuid = [0u8; 16];
    let timeout = libc::timespec {
        tv_sec: 0,
        tv_nsec: 100_000_000,
    };
    let rc = unsafe { libc::gethostuuid(uuid.as_mut_ptr(), &timeout) };
    if rc != 0 || uuid == [0u8; 16] {
        return None;
    }
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        uuid[0],
        uuid[1],
        uuid[2],
        uuid[3],
        uuid[4],
        uuid[5],
        uuid[6],
        uuid[7],
        uuid[8],
        uuid[9],
        uuid[10],
        uuid[11],
        uuid[12],
        uuid[13],
        uuid[14],
        uuid[15],
    ))
}

#[cfg(target_os = "macos")]
fn os_current_pid_ns() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn os_probe_process(pid: u32) -> ProcessProbe {
    let Ok(pid_i32) = i32::try_from(pid) else {
        return ProcessProbe::Absent;
    };
    if pid_i32 <= 0 {
        return ProcessProbe::Absent;
    }
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
    let expected_size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let ret = unsafe {
        libc::proc_pidinfo(
            pid_i32,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            expected_size,
        )
    };
    if ret == expected_size {
        let info = unsafe { info.assume_init() };
        if info.pbi_status == libc::SZOMB {
            return ProcessProbe::Absent;
        }
        return ProcessProbe::Running {
            start_id: Some(format!(
                "macos:{}.{:06}",
                info.pbi_start_tvsec, info.pbi_start_tvusec
            )),
        };
    }
    let kill_rc = unsafe { libc::kill(pid_i32 as libc::pid_t, 0) };
    if kill_rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        ProcessProbe::Absent
    } else {
        ProcessProbe::Unverifiable
    }
}

#[cfg(target_os = "linux")]
fn read_trimmed_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn os_current_boot_id() -> Option<String> {
    read_trimmed_file("/proc/sys/kernel/random/boot_id")
}

#[cfg(target_os = "linux")]
fn os_current_host_id() -> Option<String> {
    read_trimmed_file("/etc/machine-id").or_else(|| read_trimmed_file("/var/lib/dbus/machine-id"))
}

#[cfg(target_os = "linux")]
fn os_current_pid_ns() -> Option<String> {
    std::fs::read_link("/proc/self/ns/pid")
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn os_probe_process(pid: u32) -> ProcessProbe {
    let Ok(pid_i32) = i32::try_from(pid) else {
        return ProcessProbe::Absent;
    };
    if pid_i32 <= 0 {
        return ProcessProbe::Absent;
    }
    let stat_path = format!("/proc/{pid}/stat");
    match std::fs::read_to_string(&stat_path) {
        Ok(contents) => {
            if let Some((_, rest)) = contents.rsplit_once(')') {
                let fields: Vec<&str> = rest.split_whitespace().collect();
                if matches!(fields.first().copied(), Some("Z" | "X" | "x")) {
                    return ProcessProbe::Absent;
                }
                if let Some(starttime) = fields.get(19).copied()
                    && starttime.parse::<u64>().is_ok()
                {
                    return ProcessProbe::Running {
                        start_id: Some(format!("linux:{starttime}")),
                    };
                }
            }
            ProcessProbe::Unverifiable
        }
        Err(_) => {
            let kill_rc = unsafe { libc::kill(pid_i32 as libc::pid_t, 0) };
            if kill_rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                ProcessProbe::Absent
            } else {
                ProcessProbe::Unverifiable
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_current_boot_id() -> Option<String> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_current_host_id() -> Option<String> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_current_pid_ns() -> Option<String> {
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn os_probe_process(_pid: u32) -> ProcessProbe {
    ProcessProbe::Unverifiable
}

#[cfg(test)]
#[path = "../../../tests/unit/core/broker_tests.rs"]
mod tests;

//! Redacted semantic inventory. All physical reads belong to Dactyl's bounded
//! snapshot capability; inspection never initializes or repairs the datastore.

use std::{collections::BTreeMap, path::Path, time::Duration};

use ::dactyl_db::{DactylError, SnapshotLimits, SnapshotReader};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::core::{dactyl::DactylBridge, error::DecapodError, schemas, storage_codec};

const MAX_TABLES: usize = 256;
const MAX_UNIQUE: usize = 1024;
const MAX_JSON_BYTES: usize = 1024 * 1024;

struct ReportBudget(u64);
impl std::io::Write for ReportBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("inventory report budget"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct HashWriter(Sha256);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn binary_digest() -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(std::env::current_exe().ok()?).ok()?;
    let before = file.metadata().ok()?;
    if before.len() > 512 * 1024 * 1024 {
        return None;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let after = file.metadata().ok()?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return None;
    }
    Some(format!("sha256:{:x}", hash.finalize()))
}

#[derive(Debug, Clone, Serialize, Deserialize, clap::Args)]
#[serde(default, deny_unknown_fields)]
pub struct InventoryOptions {
    #[clap(long, default_value_t = 10_000)]
    pub rows_per_table: u32,
    #[clap(long, default_value_t = 64 * 1024 * 1024)]
    pub scan_bytes: u64,
    #[clap(long, default_value_t = 4 * 1024 * 1024)]
    pub report_bytes: u64,
    #[clap(long, default_value_t = 5)]
    pub duration_seconds: u64,
    /// Explicit authorization for a snapshot longer than the default five seconds.
    #[clap(long)]
    pub allow_long_read: bool,
}

impl Default for InventoryOptions {
    fn default() -> Self {
        Self {
            rows_per_table: 10_000,
            scan_bytes: 64 * 1024 * 1024,
            report_bytes: 4 * 1024 * 1024,
            duration_seconds: 5,
            allow_long_read: false,
        }
    }
}

impl InventoryOptions {
    fn limits(&self) -> Result<SnapshotLimits, DecapodError> {
        if self.rows_per_table == 0
            || self.rows_per_table > 100_000
            || self.scan_bytes == 0
            || self.scan_bytes > 1024 * 1024 * 1024
            || self.report_bytes < 4096
            || self.report_bytes > 16 * 1024 * 1024
            || self.duration_seconds == 0
            || self.duration_seconds > 300
            || (self.duration_seconds > 5 && !self.allow_long_read)
        {
            return Err(DecapodError::ValidationError(
                "INVENTORY_BUDGET: rows 1..100000, scan bytes 1..1GiB, report bytes 4KiB..16MiB, duration 1..300 seconds; reads over five seconds require --allow-long-read".into()));
        }
        Ok(SnapshotLimits {
            duration: Duration::from_secs(self.duration_seconds),
            result_bytes: self.scan_bytes,
            ..SnapshotLimits::default()
        })
    }
}

fn quoted(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn scalar(reader: &mut dyn SnapshotReader, sql: &str) -> Result<Value, DactylError> {
    Ok(reader
        .read(sql, &[])?
        .as_slice()
        .first()
        .and_then(|row| row.values.first())
        .cloned()
        .unwrap_or(Value::Null))
}

/// Report filesystem observations separately from the database read snapshot.
/// No file, WAL, journal, backup or artifact is read as raw database content.
fn file_observation(path: &Path) -> Value {
    match std::fs::metadata(path) {
        Ok(metadata) => {
            #[cfg(unix)]
            let allocated = {
                use std::os::unix::fs::MetadataExt;
                metadata.blocks().checked_mul(512)
            };
            #[cfg(not(unix))]
            let allocated: Option<u64> = None;
            json!({"status":"observed", "apparent_bytes":metadata.len(), "allocated_bytes":allocated})
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            json!({"status":"absent", "apparent_bytes":null, "allocated_bytes":null})
        }
        Err(_) => json!({"status":"unavailable", "apparent_bytes":null, "allocated_bytes":null}),
    }
}

fn files(path: &Path) -> Value {
    let mut result = serde_json::Map::new();
    for (name, suffix) in [
        ("main", ""),
        ("wal", "-wal"),
        ("shm", "-shm"),
        ("rollback_journal", "-journal"),
        ("lifecycle_lock", ".lock"),
    ] {
        let mut file = path.as_os_str().to_os_string();
        file.push(suffix);
        result.insert(name.into(), file_observation(Path::new(&file)));
    }
    Value::Object(result)
}

#[derive(Default)]
struct Field {
    lengths: Vec<u64>,
    nulls: u64,
    bytes: u64,
    unique: BTreeMap<[u8; 32], (u64, u64)>,
    untracked: u64,
    repeated_bytes: u64,
    repeated_values: u64,
}

impl Field {
    fn observe(&mut self, value: &Value) {
        if value.is_null() {
            self.nulls += 1;
            return;
        }
        let (length, bytes, kind) = match value {
            Value::String(text) => (text.len() as u64, text.as_bytes().to_vec(), 1_u8),
            // Dactyl's BLOB profile is a JSON byte array. Text containing JSON
            // remains a String and is never conflated with a BLOB.
            Value::Array(array) => {
                let bytes: Vec<u8> = array
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|byte| byte as u8)
                    .collect();
                (bytes.len() as u64, bytes, 2)
            }
            _ => {
                let bytes = value.to_string().into_bytes();
                (bytes.len() as u64, bytes, 3)
            }
        };
        self.bytes += length;
        self.lengths.push(length);
        if length < 128 {
            return;
        }
        let mut hash = Sha256::new();
        hash.update([kind]);
        hash.update(&bytes);
        let hash: [u8; 32] = hash.finalize().into();
        if let Some((count, _)) = self.unique.get_mut(&hash) {
            *count += 1;
            self.repeated_bytes += length;
            self.repeated_values += 1;
        } else if self.unique.len() < MAX_UNIQUE {
            self.unique.insert(hash, (1, length));
        } else {
            self.untracked += 1;
        }
    }

    fn report(mut self) -> Value {
        self.lengths.sort_unstable();
        let percentile = |p: usize| -> Option<u64> {
            if self.lengths.is_empty() {
                None
            } else {
                self.lengths
                    .get((self.lengths.len() * p).div_ceil(100).saturating_sub(1))
                    .copied()
            }
        };
        json!({"non_null":self.lengths.len(), "null":self.nulls, "logical_bytes":self.bytes,
            "p50":percentile(50), "p95":percentile(95), "p99":percentile(99),
            "max":self.lengths.last(),
            "largest_lengths":self.lengths.iter().rev().take(5).collect::<Vec<_>>(),
            "potential_repeated_bytes":self.repeated_bytes, "repeated_values":self.repeated_values,
            "reuse_coverage":if self.untracked == 0 {"complete_within_sample"} else {"bounded_lower_bound"},
            "untracked_candidate_values":self.untracked})
    }
}

#[derive(Default, Serialize)]
struct Formats {
    checkpoint_sample_observations: u64,
    checkpoint_json_unavailable_observations: u64,
    checkpoint_versions: BTreeMap<u64, u64>,
    checkpoint_unknown_version_rows: u64,
    checkpoint_version_overflow_rows: u64,
    lcm_plain_text_cells: u64,
    lcm_v1_header_cells: u64,
    lcm_declared_decoded_bytes_unverified: u64,
    lcm_unknown_cells: u64,
}

impl Formats {
    fn observe_lcm(&mut self, value: &Value) {
        match value {
            Value::String(_) => self.lcm_plain_text_cells += 1,
            Value::Array(bytes) => {
                let header: Option<Vec<u8>> = bytes
                    .iter()
                    .take(44)
                    .map(|v| u8::try_from(v.as_u64()?).ok())
                    .collect();
                match header
                    .as_deref()
                    .and_then(|bytes| storage_codec::header_decoded_len(bytes).ok())
                {
                    Some(length) => {
                        self.lcm_v1_header_cells += 1;
                        self.lcm_declared_decoded_bytes_unverified += length as u64;
                    }
                    None => self.lcm_unknown_cells += 1,
                }
            }
            _ => self.lcm_unknown_cells += 1,
        }
    }
}

fn component_classes(
    payload: &str,
    checkpoint_event: bool,
    classes: &mut BTreeMap<String, Field>,
    skipped: &mut u64,
    formats: &mut Formats,
) {
    if payload.len() > MAX_JSON_BYTES {
        *skipped += 1;
        formats.checkpoint_json_unavailable_observations += u64::from(checkpoint_event);
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(payload) else {
        *skipped += 1;
        formats.checkpoint_json_unavailable_observations += u64::from(checkpoint_event);
        return;
    };
    let checkpoint = if checkpoint_event {
        value.get("projection").map(|_| &value).or_else(|| {
            value
                .get("payload")
                .filter(|v| v.get("projection").is_some())
        })
    } else {
        None
    };
    if let Some(checkpoint) = checkpoint {
        if let Some(version) = checkpoint.get("schema_version").and_then(Value::as_u64) {
            if formats.checkpoint_versions.contains_key(&version)
                || formats.checkpoint_versions.len() < 32
            {
                *formats.checkpoint_versions.entry(version).or_default() += 1;
            } else {
                formats.checkpoint_version_overflow_rows += 1;
            }
        } else {
            formats.checkpoint_unknown_version_rows += 1;
        }
        for (class, pointer) in [
            ("checkpoint_task", "/projection/task"),
            ("checkpoint_owners", "/projection/owners"),
            ("checkpoint_dependencies", "/projection/dependencies"),
            ("checkpoint_tags", "/projection/tags"),
            ("checkpoint_verification", "/projection/verification"),
            ("checkpoint_description", "/projection/task/description"),
            ("checkpoint_scope", "/projection/task/scope"),
            (
                "checkpoint_proof_plan",
                "/projection/verification/proof_plan",
            ),
            (
                "checkpoint_verification_artifacts",
                "/projection/verification/verification_artifacts",
            ),
        ] {
            if let Some(value) = checkpoint.pointer(pointer) {
                let logical = match value {
                    Value::String(_) | Value::Null => value.clone(),
                    _ => Value::String(value.to_string()),
                };
                classes.entry(class.into()).or_default().observe(&logical);
            }
        }
    }
    if let Some(object) = value.as_object() {
        let metadata: serde_json::Map<String, Value> = [
            "event_id",
            "ts",
            "seq",
            "event_type",
            "actor",
            "stream",
            "subject_id",
            "subject_kind",
        ]
        .into_iter()
        .filter_map(|key| object.get(key).map(|v| (key.into(), v.clone())))
        .collect();
        if !metadata.is_empty() {
            let class = if checkpoint_event {
                "checkpoint_event_metadata"
            } else {
                "embedded_event_metadata"
            };
            classes
                .entry(class.into())
                .or_default()
                .observe(&Value::String(Value::Object(metadata).to_string()));
        }
    }
}

fn checkpoint_sample(
    reader: &mut dyn SnapshotReader,
    ordering: &str,
    rows: u32,
    classes: &mut BTreeMap<String, Field>,
    skipped: &mut u64,
    formats: &mut Formats,
    output: &mut Value,
) -> Result<(), DactylError> {
    // CASE returns only these two public labels, never stored event-type text.
    let totals = reader.read("SELECT CASE WHEN event_type='task.journal.checkpoint' THEN 'task_checkpoint' ELSE 'other_event' END AS class,count(*) AS rows,count(*)-count(payload) AS null_payload_rows,coalesce(sum(length(CAST(payload AS BLOB))),0) AS payload_cast_blob_bytes FROM events GROUP BY class ORDER BY class", &[])?;
    *output = json!({
        "task_checkpoint":{"rows":0,"null_payload_rows":0,"payload_cast_blob_bytes":0},
        "other_event":{"rows":0,"null_payload_rows":0,"payload_cast_blob_bytes":0}
    });
    for row in totals.as_slice() {
        let name = row.get_str_ref("class")?;
        if !["task_checkpoint", "other_event"].contains(&name) {
            return Err(DactylError::Conversion("inventory_event_class".into()));
        }
        output[name] = json!({"rows":row.get_int("rows")?,
            "null_payload_rows":row.get_int("null_payload_rows")?,
            "payload_cast_blob_bytes":row.get_int("payload_cast_blob_bytes")?});
    }
    let mut payload = Field::default();
    reader.visit(&format!("SELECT payload FROM events WHERE event_type='task.journal.checkpoint' ORDER BY {ordering} LIMIT {rows}"), &[], &mut |row| {
        formats.checkpoint_sample_observations += 1;
        if let Some(value) = row.values.first() {
            payload.observe(value);
            if let Value::String(value) = value {
                component_classes(value, true, classes, skipped, formats);
            } else {
                *skipped += 1;
                formats.checkpoint_json_unavailable_observations += 1;
            }
        }
        Ok(true)
    })?;
    output["task_checkpoint"]["payload_sample"] = payload.report();
    Ok(())
}

/// Inspect one repository-local store. `scope` is an agent-supplied, non-secret
/// label, not an authorization or a database path. Payloads/IDs never leave here.
pub fn inspect(
    data_root: &Path,
    scope: &str,
    options: &InventoryOptions,
) -> Result<Value, DecapodError> {
    let limits = options.limits()?;
    if scope.is_empty()
        || scope.len() > 128
        || !scope
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    {
        return Err(DecapodError::ValidationError(
            "INVENTORY_SCOPE: use a non-secret ASCII label of at most 128 bytes".into(),
        ));
    }
    let path = data_root.join(schemas::LOCAL_DB_NAME);
    let mut report = json!({"schema":"decapod.storage-inventory.v1", "scope":scope,
        "application_version":env!("CARGO_PKG_VERSION"), "application_binary_sha256":binary_digest(),
        "observation_time":chrono::Utc::now().to_rfc3339(),
    "options":options, "status":"unavailable", "coverage":"unavailable",
    "consistency":"one Dactyl deferred read transaction; filesystem stats observed separately before/after",
    "files_before":files(&path), "tables":[], "component_classes":{}, "physical":null,
    "event_payload_classes":null,
    "measured_savings_bytes":null,
    "interpretation":[
        "B-tree allocated, payload and unused bytes are nested quantities; do not add them together",
        "Freelist bytes are reusable pages, not proof that retained live content is disposable",
        "Field lengths are UTF-8 text/raw BLOB/canonical-JSON scalar bytes, not physical cell allocation",
            "Field reuse compares SHA-256 over type-tagged exact bytes >=128 bytes; checkpoint classes compare decoded logical component bytes; embedded metadata uses sorted JSON projection; no content hashes are emitted",
            "Component classes overlap stored field totals and must not be added to them",
        "Potential reuse excludes reference/index/codec overhead and is not measured savings",
        "Table samples follow declared primary-key order, otherwise rowid; quantiles describe the sample only",
        "No backups/artifacts or hosted PostgreSQL footprint are inferred from this local report"
    ]});
    if !path.exists() {
        report["limitation"] = json!("missing_database");
        return Ok(report);
    }
    let bridge = match DactylBridge::open_canonical(data_root, dactyl_db::AccessMode::ReadOnly) {
        Ok(bridge) => bridge,
        Err(DecapodError::ValidationError(message))
            if message.starts_with("STORAGE_OWNER_MAINTENANCE_BLOCKED:") =>
        {
            report["limitation"] = json!("foreground_storage_owner_active");
            return Ok(report);
        }
        Err(_) => {
            report["limitation"] = json!("safe_read_only_open_unavailable");
            return Ok(report);
        }
    };
    let mut classes = BTreeMap::<String, Field>::new();
    let mut formats = Formats::default();
    let mut skipped_json = 0;
    let result = bridge.inspect_snapshot(limits, &mut |reader| {
        report["engine"] = json!({"name":"sqlite", "version":scalar(reader, "SELECT sqlite_version()")?,
            "encoding":scalar(reader, "SELECT encoding FROM pragma_encoding")?,
            "user_version":scalar(reader, "SELECT user_version FROM pragma_user_version")?,
            "application_id":scalar(reader, "SELECT application_id FROM pragma_application_id")?});
        report["physical"] = json!({"page_size":scalar(reader, "SELECT page_size FROM pragma_page_size")?,
            "page_count":scalar(reader, "SELECT page_count FROM pragma_page_count")?,
            "freelist_count":scalar(reader, "SELECT freelist_count FROM pragma_freelist_count")?,
            "journal_mode":scalar(reader, "SELECT journal_mode FROM pragma_journal_mode")?});
        let catalog = reader.read("SELECT type,name,tbl_name,sql,CASE WHEN type='table' AND coalesce(rootpage,0)=0 THEN 1 ELSE 0 END AS virtual_table FROM sqlite_schema WHERE type IN ('table','index','trigger','view') ORDER BY type,name LIMIT 1025", &[])?;
        if catalog.as_slice().len() > 1024 { return Err(DactylError::Conversion("inventory_catalog_budget".into())); }
        let mut schema_hash = HashWriter(Sha256::new());
        serde_json::to_writer(&mut schema_hash, &catalog)
            .map_err(|_| DactylError::Conversion("inventory_schema_encoding".into()))?;
        report["effective_schema_digest"] = json!(format!("sha256:{:x}", schema_hash.0.finalize()));
        // DBSTAT is optional in the host engine. Query the eponymous read-only
        // table; never CREATE a virtual table or silently install an extension.
        match reader.read("SELECT name,sum(pgsize) AS allocated_bytes,sum(payload) AS payload_bytes,sum(unused) AS unused_bytes,sum(ncell) AS physical_cells,count(*) AS pages FROM dbstat GROUP BY name ORDER BY name LIMIT 1025", &[]) {
            Ok(rows) => { report["btree"] = serde_json::to_value(rows).map_err(|_| DactylError::Conversion("inventory_btree_encoding".into()))?; }
            Err(error) if error.adapter_kind() == Some(dactyl_db::AdapterErrorKind::Query) => { report["btree"] = json!({"status":"unavailable", "reason":"host_dbstat_unavailable"}); }
            Err(error) => return Err(error),
        }
        let mut table_count = 0;
        report["observed_application_schema"] = json!({"todo":null});
        for item in catalog.as_slice() {
            if item.get_str_ref("type")? != "table" { continue; }
            table_count += 1;
            if table_count > MAX_TABLES { return Err(DactylError::Conversion("inventory_table_budget".into())); }
            let table = item.get_str_ref("name")?;
            // SQLite documents a zero/NULL root page for virtual tables.
            // Classify catalog metadata, without interpreting SQL text or
            // preparing a virtual table that could execute extension work.
            // https://www.sqlite.org/schematab.html
            if item.get_int("virtual_table")? == 1 {
                report["tables"].as_array_mut().unwrap().push(json!({"table":table,
                    "rows":null, "sample_rows":null, "fields":null, "coverage":"unavailable",
                    "limitation":"virtual_table_requires_reviewed_scope"}));
                continue;
            }
            let columns = reader.read("SELECT name,pk FROM pragma_table_xinfo($1) WHERE hidden!=1 ORDER BY cid", &[table.into()])?;
            let names: Vec<String> = columns.as_slice().iter().map(|row| row.get_str_ref("name").map(str::to_owned)).collect::<Result<_, _>>()?;
            if table == "meta" && ["namespace", "key", "value"].iter().all(|name| names.iter().any(|column| column == name)) {
                let value = scalar(reader, "SELECT value FROM meta WHERE namespace='todo' AND key='schema_version' LIMIT 1")?;
                let version = value.as_str().and_then(|v| v.parse::<u32>().ok());
                report["observed_application_schema"]["todo"] = json!({
                    "version":version, "status":if version.is_some() {"observed"} else {"unavailable"}});
            }
            let mut keys: Vec<(i64, String)> = columns.as_slice().iter().filter_map(|row| {
                let pk = row.get_int("pk").ok()?; (pk > 0).then(|| (pk, row.get_str_ref("name").unwrap().to_owned()))
            }).collect();
            keys.sort();
            let ordering = if keys.is_empty() { "rowid".into() } else { keys.iter().map(|(_,key)| quoted(key)).collect::<Vec<_>>().join(",") };
            let total = scalar(reader, &format!("SELECT count(*) FROM {}", quoted(table)))?;
            let mut fields: Vec<Field> = names.iter().map(|_| Field::default()).collect();
            let payload_column = names.iter().position(|name| name == "payload");
            let checkpoint_columns = payload_column.is_some() && names.iter().any(|name| name == "event_type");
            let scan = reader.visit(&format!("SELECT * FROM {} ORDER BY {ordering} LIMIT {}", quoted(table), options.rows_per_table), &[], &mut |row| {
                for (field, value) in fields.iter_mut().zip(&row.values) { field.observe(value); }
                if table == "events" && let Some(Value::String(payload)) = payload_column.and_then(|index| row.values.get(index)) {
                    component_classes(payload, false, &mut classes, &mut skipped_json, &mut formats);
                }
                if table == "summaries" {
                    for (name, value) in names.iter().zip(&row.values) {
                        if name == "summary_text" || name == "original_hashes" {
                            formats.observe_lcm(value);
                        }
                    }
                }
                Ok(true)
            })?;
            let field_report: BTreeMap<String, Value> = names.into_iter().zip(fields.into_iter().map(Field::report)).collect();
            report["tables"].as_array_mut().unwrap().push(json!({"table":table, "rows":total,
                "sample_rows":scan.rows, "coverage":if total.as_u64() == Some(scan.rows) {"complete"} else {"sampled"}, "fields":field_report}));
            if table == "events" && checkpoint_columns {
                checkpoint_sample(reader, &ordering, options.rows_per_table, &mut classes, &mut skipped_json, &mut formats, &mut report["event_payload_classes"])?;
            }
        }
        Ok(())
    });
    report["component_classes"] = json!(
        classes
            .into_iter()
            .map(|(name, field)| (name, field.report()))
            .collect::<BTreeMap<_, _>>()
    );
    report["component_json_unavailable_observations"] = json!(skipped_json);
    let checkpoints = report["event_payload_classes"]["task_checkpoint"]["rows"].as_u64();
    report["checkpoint_sampling"] = json!({
        "method":"separate task.journal.checkpoint primary-key prefix; same per-table row ceiling and shared snapshot budgets",
        "rows":checkpoints,"sample_rows":formats.checkpoint_sample_observations,
        "coverage":if checkpoints.is_none() {"unavailable"} else if result.is_err() {"incomplete"} else if formats.checkpoint_json_unavailable_observations > 0 {"limited"} else if checkpoints == Some(formats.checkpoint_sample_observations) {"complete"} else {"sampled"}
    });
    report["event_payload_byte_method"] = json!(
        "Exact SQLite length(CAST(payload AS BLOB)) grouped into checkpoint/other classes in engine encoding; excludes cell/index overhead; classes partition events and must not be added to table totals"
    );
    report["sampled_format_observations"] = json!(formats);
    report["format_observation_scope"] = json!(
        "Sampled event_type task.journal.checkpoint JSON version fields and summaries text/BLOB headers only; no chain, digest, body or migration validation"
    );
    report["files_after"] = files(&path);
    match result {
        Ok(()) => {
            let unavailable = report["tables"]
                .as_array()
                .unwrap()
                .iter()
                .any(|table| table["coverage"] == "unavailable");
            let complete = report["tables"]
                .as_array()
                .unwrap()
                .iter()
                .all(|table| table["coverage"] == "complete");
            report["status"] = json!(if unavailable { "partial" } else { "ok" });
            report["coverage"] = json!(if unavailable {
                "incomplete"
            } else if complete {
                "complete"
            } else {
                "sampled"
            });
            if unavailable {
                report["limitation"] = json!("unsupported_virtual_tables");
            }
        }
        Err(error) => {
            report["status"] = json!("partial");
            report["coverage"] = json!("incomplete");
            report["limitation"] = json!(match error {
                DecapodError::DactylError(DactylError::Conversion(code))
                    if [
                        "inventory_catalog_budget",
                        "inventory_table_budget",
                        "inventory_schema_encoding",
                        "inventory_btree_encoding",
                        "inventory_event_class"
                    ]
                    .contains(&code.as_str()) =>
                    code,
                DecapodError::DactylError(error) => error
                    .adapter_code()
                    .unwrap_or("inspection_unavailable")
                    .to_owned(),
                _ => "inspection_unavailable".into(),
            });
        }
    }
    if serde_json::to_writer_pretty(&mut ReportBudget(options.report_bytes), &report).is_err() {
        return Ok(json!({
            "schema":"decapod.storage-inventory.v1", "scope":scope,
            "application_version":env!("CARGO_PKG_VERSION"),
            "status":"partial", "coverage":"incomplete",
            "limitation":"inventory_report_budget_exceeded", "measurements":null
        }));
    }
    Ok(report)
}

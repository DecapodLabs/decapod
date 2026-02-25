use crate::core::db;
use crate::core::error;
use crate::core::time;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use ulid::Ulid;

const BROKER_INTERNAL_ENV: &str = "DECAPOD_GROUP_BROKER_INTERNAL";
const BROKER_DISABLE_ENV: &str = "DECAPOD_GROUP_BROKER_DISABLE";
const BROKER_IDLE_SECS_ENV: &str = "DECAPOD_GROUP_BROKER_IDLE_SECS";
const BROKER_REQUEST_ID_ENV: &str = "DECAPOD_GROUP_BROKER_REQUEST_ID";
const BROKER_PROTOCOL_CLIENT_OVERRIDE_ENV: &str = "DECAPOD_GROUP_BROKER_PROTOCOL_CLIENT_OVERRIDE";
const BROKER_PROTOCOL_SERVER_OVERRIDE_ENV: &str = "DECAPOD_GROUP_BROKER_PROTOCOL_SERVER_OVERRIDE";
const BROKER_PHASE_HOOK_FILE_ENV: &str = "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE";
const BROKER_HALT_PHASE_ENV: &str = "DECAPOD_GROUP_BROKER_TEST_HALT_PHASE";
const BROKER_PROTOCOL_DEFAULT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrokerRequest {
    protocol_version: u32,
    request_id: String,
    argv: Vec<String>,
    payload_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrokerResponse {
    protocol_version: u32,
    status: String,
    commit_marker: Option<String>,
    result_envelope: serde_json::Value,
    retry_after_ms_hint: Option<u64>,
}

#[derive(Debug, Clone)]
struct DedupeRecord {
    payload_hash: String,
    status: String,
    commit_marker: Option<String>,
    result_envelope: serde_json::Value,
    retry_after_ms_hint: Option<u64>,
}

pub fn is_internal_invocation() -> bool {
    std::env::var(BROKER_INTERNAL_ENV)
        .map(|v| v == "1")
        .unwrap_or(false)
}

pub fn maybe_route_mutation(
    broker_root: &Path,
    argv: &[String],
) -> Result<bool, error::DecapodError> {
    if std::env::var(BROKER_DISABLE_ENV)
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return Ok(false);
    }
    if is_internal_invocation() {
        return Ok(false);
    }

    #[cfg(unix)]
    {
        match run_unix_broker(broker_root, argv) {
            Ok(()) => Ok(true),
            // Some constrained sandboxes disallow AF_UNIX sockets. Fall back to direct path.
            Err(error::DecapodError::IoError(io_err))
                if io_err.kind() == std::io::ErrorKind::PermissionDenied =>
            {
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(not(unix))]
    {
        let _ = broker_root;
        let _ = argv;
        Ok(false)
    }
}

#[cfg(unix)]
fn run_unix_broker(broker_root: &Path, argv: &[String]) -> Result<(), error::DecapodError> {
    fs::create_dir_all(broker_root).map_err(error::DecapodError::IoError)?;
    let socket_path = broker_socket_path(broker_root);
    let lock_path = broker_lock_path(broker_root);

    let request = BrokerRequest {
        protocol_version: client_protocol_version(),
        request_id: std::env::var(BROKER_REQUEST_ID_ENV)
            .unwrap_or_else(|_| Ulid::new().to_string()),
        argv: argv.to_vec(),
        payload_hash: hash_payload(argv),
    };

    match send_request(&socket_path, &request) {
        Ok(resp) => return apply_response(resp),
        Err(error::DecapodError::ValidationError(msg))
            if msg.contains("BROKER_PROTOCOL_MISMATCH") =>
        {
            return Err(error::DecapodError::ValidationError(msg));
        }
        Err(_) => {}
    }

    for phase in 0..2u8 {
        let mut attempts = 0u32;
        loop {
            attempts += 1;
            match try_acquire_lock(&lock_path)? {
                Some(lease) => {
                    let resp = run_as_leader(lease, broker_root, &socket_path, request.clone())?;
                    return apply_response(resp);
                }
                None => {
                    match send_request(&socket_path, &request) {
                        Ok(resp) => return apply_response(resp),
                        Err(error::DecapodError::ValidationError(msg))
                            if msg.contains("BROKER_PROTOCOL_MISMATCH") =>
                        {
                            return Err(error::DecapodError::ValidationError(msg));
                        }
                        Err(_) => {}
                    }
                    if attempts >= 40 {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10 + jitter_ms(30)));
                }
            }
        }
        if phase == 0 {
            std::thread::sleep(Duration::from_millis(4000 + jitter_ms(2000)));
        }
    }
    Err(error::DecapodError::ValidationError(
        "BROKER_UNKNOWN: no final confirmation; retry with same request_id after backoff"
            .to_string(),
    ))
}

#[cfg(unix)]
fn run_as_leader(
    _lease: BrokerLease,
    broker_root: &Path,
    socket_path: &Path,
    local_request: BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    use std::os::unix::net::UnixListener;

    if socket_path.exists() {
        let _ = fs::remove_file(socket_path);
    }
    let listener = match UnixListener::bind(socket_path) {
        Ok(listener) => listener,
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse => {
            let _ = fs::remove_file(socket_path);
            UnixListener::bind(socket_path).map_err(error::DecapodError::IoError)?
        }
        Err(err) => return Err(error::DecapodError::IoError(err)),
    };
    listener
        .set_nonblocking(true)
        .map_err(error::DecapodError::IoError)?;

    emit_phase_hook("queued", &local_request.request_id);
    let local_response = execute_request(broker_root, &local_request)?;

    let idle_timeout = Duration::from_secs(
        std::env::var(BROKER_IDLE_SECS_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3),
    );
    let mut last_activity = Instant::now();

    loop {
        if last_activity.elapsed() >= idle_timeout {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                if handle_client(broker_root, stream).is_ok() {
                    last_activity = Instant::now();
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }

    let _ = fs::remove_file(socket_path);
    Ok(local_response)
}

#[cfg(unix)]
fn handle_client(
    broker_root: &Path,
    stream: std::os::unix::net::UnixStream,
) -> Result<(), error::DecapodError> {
    let mut reader = BufReader::new(stream.try_clone().map_err(error::DecapodError::IoError)?);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(error::DecapodError::IoError)?;
    let req: BrokerRequest = serde_json::from_str(line.trim()).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_INVALID_REQUEST: {}", e))
    })?;
    let server_version = server_protocol_version();
    if req.protocol_version != server_version {
        let resp = BrokerResponse {
            protocol_version: server_version,
            status: "NOT_COMMITTED".to_string(),
            commit_marker: None,
            result_envelope: serde_json::json!({
                "request_id": req.request_id,
                "error": "BROKER_PROTOCOL_MISMATCH",
                "expected_protocol_version": server_version,
                "received_protocol_version": req.protocol_version,
            }),
            retry_after_ms_hint: Some(5000),
        };
        write_response(stream, &resp)?;
        return Ok(());
    }
    emit_phase_hook("queued", &req.request_id);

    let resp = execute_request(broker_root, &req)?;
    write_response(stream, &resp)?;
    Ok(())
}

#[cfg(unix)]
fn send_request(
    socket_path: &Path,
    request: &BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    use std::os::unix::net::UnixStream;

    let mut stream = UnixStream::connect(socket_path).map_err(error::DecapodError::IoError)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(error::DecapodError::IoError)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(15)))
        .map_err(error::DecapodError::IoError)?;

    let payload = serde_json::to_string(request).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_ENCODE_ERROR: {}", e))
    })?;
    stream
        .write_all(payload.as_bytes())
        .map_err(error::DecapodError::IoError)?;
    stream
        .write_all(b"\n")
        .map_err(error::DecapodError::IoError)?;
    stream.flush().map_err(error::DecapodError::IoError)?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(error::DecapodError::IoError)?;
    let resp: BrokerResponse = serde_json::from_str(line.trim()).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_INVALID_RESPONSE: {}", e))
    })?;
    if resp.protocol_version != client_protocol_version() {
        return Err(error::DecapodError::ValidationError(format!(
            "BROKER_PROTOCOL_MISMATCH: client={} broker={}",
            client_protocol_version(),
            resp.protocol_version
        )));
    }
    Ok(resp)
}

fn execute_request(
    broker_root: &Path,
    request: &BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    if let Some(existing) = dedupe_lookup(broker_root, request)? {
        if existing.payload_hash != request.payload_hash {
            return Ok(BrokerResponse {
                protocol_version: server_protocol_version(),
                status: "NOT_COMMITTED".to_string(),
                commit_marker: existing.commit_marker,
                result_envelope: serde_json::json!({
                    "request_id": request.request_id,
                    "payload_hash": request.payload_hash,
                    "error": "BROKER_DEDUPE_PAYLOAD_MISMATCH",
                }),
                retry_after_ms_hint: Some(5000),
            });
        }
        return Ok(BrokerResponse {
            protocol_version: server_protocol_version(),
            status: existing.status,
            commit_marker: existing.commit_marker,
            result_envelope: existing.result_envelope,
            retry_after_ms_hint: existing.retry_after_ms_hint,
        });
    }

    let exe = std::env::args()
        .next()
        .ok_or_else(|| error::DecapodError::ValidationError("BROKER_EXEC_PATH_MISSING".into()))?;
    emit_phase_hook("pre_exec", &request.request_id);
    let output = match Command::new(exe)
        .args(&request.argv)
        .env(BROKER_INTERNAL_ENV, "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", &request.request_id)
        .output()
    {
        Ok(output) => output,
        Err(err) => {
            return Ok(BrokerResponse {
                protocol_version: server_protocol_version(),
                status: "UNKNOWN".to_string(),
                commit_marker: None,
                result_envelope: serde_json::json!({
                    "request_id": request.request_id,
                    "payload_hash": request.payload_hash,
                    "error": format!("BROKER_EXEC_SPAWN_FAILED: {}", err),
                }),
                retry_after_ms_hint: Some(5000),
            });
        }
    };

    let code_opt = output.status.code();
    let code = code_opt.unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let result_envelope = serde_json::json!({
        "request_id": request.request_id,
        "payload_hash": request.payload_hash,
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
    });

    let status = if code_opt.is_none() {
        "UNKNOWN"
    } else if code == 0 {
        "COMMITTED"
    } else {
        "NOT_COMMITTED"
    };

    emit_phase_hook("post_exec_pre_ack", &request.request_id);
    let response = BrokerResponse {
        protocol_version: server_protocol_version(),
        status: status.to_string(),
        commit_marker: Some(format!("{}:{}", time::now_epoch_z(), Ulid::new())),
        result_envelope: result_envelope.clone(),
        retry_after_ms_hint: if status == "COMMITTED" {
            None
        } else {
            Some(5000)
        },
    };
    dedupe_store(broker_root, request, &response)?;
    Ok(response)
}

fn apply_response(resp: BrokerResponse) -> Result<(), error::DecapodError> {
    let stdout = resp
        .result_envelope
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let stderr = resp
        .result_envelope
        .get("stderr")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !stdout.is_empty() {
        print!("{}", stdout);
    }
    if !stderr.is_empty() {
        eprint!("{}", stderr);
    }

    match resp.status.as_str() {
        "COMMITTED" => Ok(()),
        "NOT_COMMITTED" => {
            let typed_error = resp
                .result_envelope
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("BROKER_NOT_COMMITTED");
            Err(error::DecapodError::ValidationError(format!(
                "{typed_error}: request failed (commit_marker={})",
                resp.commit_marker.unwrap_or_else(|| "<none>".to_string())
            )))
        }
        _ => Err(error::DecapodError::ValidationError(format!(
            "BROKER_UNKNOWN: no final confirmation (retry_after_ms_hint={})",
            resp.retry_after_ms_hint.unwrap_or(5000)
        ))),
    }
}

fn hash_payload(argv: &[String]) -> String {
    let mut hasher = Sha256::new();
    for arg in argv {
        hasher.update(arg.as_bytes());
        hasher.update(b"\0");
    }
    format!("{:x}", hasher.finalize())
}

fn broker_lock_path(broker_root: &Path) -> PathBuf {
    broker_root.join("broker.lock")
}

fn broker_socket_path(broker_root: &Path) -> PathBuf {
    broker_root.join("broker.sock")
}

fn dedupe_db_path(broker_root: &Path) -> PathBuf {
    broker_root.join("broker_dedupe.db")
}

fn dedupe_lookup(
    broker_root: &Path,
    request: &BrokerRequest,
) -> Result<Option<DedupeRecord>, error::DecapodError> {
    let db_path = dedupe_db_path(broker_root);
    if !db_path.exists() {
        return Ok(None);
    }
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    ensure_dedupe_schema(&conn)?;

    let mut stmt = conn.prepare(
        "SELECT payload_hash, status, commit_marker, result_envelope, retry_after_ms_hint
         FROM request_dedupe WHERE request_id = ?1",
    )?;
    let row = stmt
        .query_row([request.request_id.as_str()], |r| {
            let payload_hash: String = r.get(0)?;
            let status: String = r.get(1)?;
            let commit_marker: Option<String> = r.get(2)?;
            let result_json: String = r.get(3)?;
            let retry_hint_i64: Option<i64> = r.get(4)?;
            let retry_hint = retry_hint_i64.and_then(|v| u64::try_from(v).ok());
            Ok((payload_hash, status, commit_marker, result_json, retry_hint))
        })
        .optional()
        .map_err(error::DecapodError::RusqliteError)?;

    let Some((payload_hash, status, commit_marker, result_json, retry_after_ms_hint)) = row else {
        return Ok(None);
    };
    let result_envelope: serde_json::Value = serde_json::from_str(&result_json).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "BROKER_DEDUPE_DECODE_FAILED for request_id={}: {}",
            request.request_id, e
        ))
    })?;
    Ok(Some(DedupeRecord {
        payload_hash,
        status,
        commit_marker,
        result_envelope,
        retry_after_ms_hint,
    }))
}

fn dedupe_store(
    broker_root: &Path,
    request: &BrokerRequest,
    response: &BrokerResponse,
) -> Result<(), error::DecapodError> {
    let db_path = dedupe_db_path(broker_root);
    let conn = db::db_connect(&db_path.to_string_lossy())?;
    ensure_dedupe_schema(&conn)?;
    let result_json = serde_json::to_string(&response.result_envelope).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_DEDUPE_ENCODE_FAILED: {}", e))
    })?;

    conn.execute(
        "INSERT OR REPLACE INTO request_dedupe(request_id, payload_hash, status, commit_marker, result_envelope, retry_after_ms_hint, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            request.request_id,
            request.payload_hash,
            response.status,
            response.commit_marker,
            result_json,
            response.retry_after_ms_hint.map(|v| v as i64),
            time::now_epoch_z(),
        ],
    )?;
    Ok(())
}

fn ensure_dedupe_schema(conn: &rusqlite::Connection) -> Result<(), error::DecapodError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS request_dedupe(
            request_id TEXT PRIMARY KEY,
            payload_hash TEXT NOT NULL,
            status TEXT NOT NULL,
            commit_marker TEXT,
            result_envelope TEXT NOT NULL,
            retry_after_ms_hint INTEGER,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_request_dedupe_created_at ON request_dedupe(created_at);",
    )?;
    Ok(())
}

fn write_response(
    mut stream: std::os::unix::net::UnixStream,
    response: &BrokerResponse,
) -> Result<(), error::DecapodError> {
    let body = serde_json::to_string(response).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_ENCODE_ERROR: {}", e))
    })?;
    stream
        .write_all(body.as_bytes())
        .map_err(error::DecapodError::IoError)?;
    stream
        .write_all(b"\n")
        .map_err(error::DecapodError::IoError)?;
    stream.flush().map_err(error::DecapodError::IoError)?;
    Ok(())
}

fn client_protocol_version() -> u32 {
    std::env::var(BROKER_PROTOCOL_CLIENT_OVERRIDE_ENV)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(BROKER_PROTOCOL_DEFAULT)
}

fn server_protocol_version() -> u32 {
    std::env::var(BROKER_PROTOCOL_SERVER_OVERRIDE_ENV)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(BROKER_PROTOCOL_DEFAULT)
}

fn emit_phase_hook(phase: &str, request_id: &str) {
    if let Ok(path) = std::env::var(BROKER_PHASE_HOOK_FILE_ENV)
        && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{}|{}", phase, request_id);
        }
    if std::env::var(BROKER_HALT_PHASE_ENV).ok().as_deref() == Some(phase) {
        loop {
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn try_acquire_lock(lock_path: &Path) -> Result<Option<BrokerLease>, error::DecapodError> {
    // Leader election lock: create_new gives single-winner semantics per path.
    let file = match OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)
    {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            if cleanup_stale_lock(lock_path)? {
                return try_acquire_lock(lock_path);
            }
            return Ok(None);
        }
        Err(err) => return Err(error::DecapodError::IoError(err)),
    };
    let pid = std::process::id();
    let _ = file.set_len(0);
    let _ = (&file).write_all(format!("{}\n", pid).as_bytes());
    let _ = (&file).flush();

    Ok(Some(BrokerLease {
        path: lock_path.to_path_buf(),
        _file: file,
    }))
}

fn cleanup_stale_lock(lock_path: &Path) -> Result<bool, error::DecapodError> {
    let raw = match fs::read_to_string(lock_path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(error::DecapodError::IoError(err)),
    };
    let pid = match raw.trim().parse::<u32>() {
        Ok(pid) if pid > 0 => pid,
        _ => return Ok(false),
    };
    if is_pid_alive(pid) {
        return Ok(false);
    }
    match fs::remove_file(lock_path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(error::DecapodError::IoError(err)),
    }
}

fn is_pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn jitter_ms(max_exclusive: u64) -> u64 {
    if max_exclusive <= 1 {
        return 0;
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    now_ms % max_exclusive
}

struct BrokerLease {
    path: PathBuf,
    _file: File,
}

impl Drop for BrokerLease {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

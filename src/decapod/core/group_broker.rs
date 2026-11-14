use crate::core::bounded_process::{BUILD_TIMEOUT, BoundedCommand, CONTROL_TIMEOUT};
use crate::core::db;
use crate::core::db::OptionalExtension;
use crate::core::error;
use crate::core::time;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BROKER_INTERNAL_ENV: &str = "DECAPOD_GROUP_BROKER_INTERNAL";
const BROKER_DISABLE_ENV: &str = "DECAPOD_GROUP_BROKER_DISABLE";
const BROKER_IDLE_SECS_ENV: &str = "DECAPOD_GROUP_BROKER_IDLE_SECS";
const BROKER_REQUEST_ID_ENV: &str = "DECAPOD_GROUP_BROKER_REQUEST_ID";
const BROKER_PROTOCOL_CLIENT_OVERRIDE_ENV: &str = "DECAPOD_GROUP_BROKER_PROTOCOL_CLIENT_OVERRIDE";
const BROKER_PROTOCOL_SERVER_OVERRIDE_ENV: &str = "DECAPOD_GROUP_BROKER_PROTOCOL_SERVER_OVERRIDE";
const BROKER_PHASE_HOOK_FILE_ENV: &str = "DECAPOD_GROUP_BROKER_TEST_HOOK_FILE";
const BROKER_HALT_PHASE_ENV: &str = "DECAPOD_GROUP_BROKER_TEST_HALT_PHASE";
const BROKER_PROTOCOL_DEFAULT: u32 = 3;
const REQUEST_BUDGET: Duration = Duration::from_secs(120);
const MAX_REQUEST_BUDGET: Duration = Duration::from_secs(300);
const REQUEST_BUDGET_ENV: &str = "DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS";
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

fn invalid_request_budget() -> error::DecapodError {
    error::DecapodError::ValidationError(
        "BROKER_REQUEST_BUDGET_INVALID: expected milliseconds from 1 through 300000".into(),
    )
}

fn request_budget() -> Result<Duration, error::DecapodError> {
    match std::env::var(REQUEST_BUDGET_ENV) {
        Err(std::env::VarError::NotPresent) => Ok(REQUEST_BUDGET),
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|milliseconds| {
                *milliseconds > 0 && *milliseconds <= MAX_REQUEST_BUDGET.as_millis() as u64
            })
            .map(Duration::from_millis)
            .ok_or_else(invalid_request_budget),
        Err(_) => Err(invalid_request_budget()),
    }
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// Only request-scoped context crosses the existing private socket. In particular,
// never inherit another caller's identity, session credential or claim behavior.
const CALLER_ENV_KEYS: &[&str] = &[
    "DECAPOD_AGENT_ID",
    "DECAPOD_AGENT_PROVIDER",
    "DECAPOD_SESSION_ID",
    "DECAPOD_SESSION_PASSWORD",
    "DECAPOD_TASK_ID",
    "DECAPOD_CORRELATION_ID",
    "DECAPOD_CAUSATION_ID",
    "DECAPOD_IDEMPOTENCY_KEY",
    "DECAPOD_CLAIM_AUTORUN",
    // Diagnostic scope belongs to this caller, never a previous broker caller.
    "DECAPOD_VALIDATE_SKIP_GIT_GATES",
    "DECAPOD_VALIDATE_SKIP_TOOLING_GATES",
    "DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES",
];

// Deliberately no Debug implementation: the credential is ephemeral transport,
// never diagnostic output, a result envelope, or a durable dedupe payload.
#[derive(Clone, Serialize, Deserialize)]
struct CallerContext {
    cwd: PathBuf,
    environment: BTreeMap<String, String>,
}

impl CallerContext {
    fn capture() -> Result<Self, error::DecapodError> {
        let mut environment = BTreeMap::new();
        for key in CALLER_ENV_KEYS {
            if let Ok(value) = std::env::var(key) {
                environment.insert((*key).to_owned(), value);
            }
        }
        environment.insert("DECAPOD_AGENT_ID".into(), crate::current_agent_id());
        // Session validation may have auto-acquired a password process-locally.
        if let Some(password) = crate::SESSION_P_VAL.get() {
            environment.insert("DECAPOD_SESSION_PASSWORD".into(), password.clone());
        }
        let caller = Self {
            cwd: std::env::current_dir()?.canonicalize()?,
            environment,
        };
        caller.validate()?;
        Ok(caller)
    }

    fn validate(&self) -> Result<(), error::DecapodError> {
        if !self.cwd.is_absolute()
            || self.cwd.to_str().is_none()
            || !self.environment.contains_key("DECAPOD_AGENT_ID")
            || self
                .environment
                .keys()
                .any(|key| !CALLER_ENV_KEYS.contains(&key.as_str()))
        {
            return Err(error::DecapodError::ValidationError(
                "BROKER_INVALID_CALLER_CONTEXT".into(),
            ));
        }
        Ok(())
    }

    fn apply(&self, command: &mut Command) {
        command.current_dir(&self.cwd);
        for key in CALLER_ENV_KEYS {
            command.env_remove(key);
        }
        command.envs(&self.environment);
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct BrokerRequest {
    protocol_version: u32,
    request_id: String,
    argv: Vec<String>,
    caller: CallerContext,
    payload_hash: String,
    #[serde(default)]
    deadline_ms: u64,
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

/// An isolated child may use the schema initialized by its foreground owner.
/// Bind this startup optimization to the exact canonical database, never cwd.
pub(crate) fn has_initialized_owner(root: &Path) -> bool {
    is_internal_invocation()
        && std::env::var_os("DECAPOD_STORAGE_OWNER_READY").is_some_and(|expected| {
            dedupe_db_path(root)
                .canonicalize()
                .is_ok_and(|path| path.as_os_str() == expected)
        })
}

pub fn maybe_route_mutation(
    broker_root: &Path,
    argv: &[String],
    finish: &mut dyn FnMut(&str, &str) -> String,
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
        run_unix_broker(broker_root, argv, finish).map(|()| true)
    }

    #[cfg(not(unix))]
    {
        let _ = broker_root;
        let _ = (argv, finish);
        Ok(false)
    }
}

#[cfg(unix)]
fn run_unix_broker(
    broker_root: &Path,
    argv: &[String],
    finish: &mut dyn FnMut(&str, &str) -> String,
) -> Result<(), error::DecapodError> {
    crate::core::fs_permissions::ensure_storage_dir(broker_root)
        .map_err(error::DecapodError::IoError)?;
    let socket_path = broker_socket_path(broker_root)?;
    crate::core::local_socket::ensure_directory(&socket_path, broker_root)?;
    let lock_path = broker_lock_path(broker_root);

    let budget = request_budget()?;
    let caller = CallerContext::capture()?;
    let request = BrokerRequest {
        protocol_version: client_protocol_version(),
        request_id: std::env::var(BROKER_REQUEST_ID_ENV)
            .unwrap_or_else(|_| crate::core::ulid::new_ulid()),
        argv: argv.to_vec(),
        payload_hash: hash_payload(argv, &caller),
        caller,
        deadline_ms: epoch_ms().saturating_add(budget.as_millis() as u64),
    };

    // Retry only a connection that was never established. After sending any
    // bytes, loss of acknowledgement is UNKNOWN and must not trigger execution.
    loop {
        match send_request(&socket_path, &request) {
            Ok(resp) => return apply_response(resp, &request.request_id, finish),
            Err(error::DecapodError::IoError(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::ConnectionRefused
                        | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(e) => return Err(e),
        }
        if epoch_ms() >= request.deadline_ms {
            return Err(error::DecapodError::ValidationError(
                "BROKER_NOT_ADMITTED: election deadline elapsed".into(),
            ));
        }
        if let Some(lease) = try_acquire_lock(&lock_path)? {
            let resp = run_as_leader(lease, broker_root, &socket_path, request.clone())?;
            return apply_response(resp, &request.request_id, finish);
        }
        std::thread::sleep(Duration::from_millis(10 + jitter_ms(30)));
    }
}

#[cfg(unix)]
fn run_as_leader(
    _lease: BrokerLease,
    broker_root: &Path,
    socket_path: &Path,
    local_request: BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    use std::os::unix::net::UnixListener;

    crate::core::local_socket::remove_stale(socket_path)?;
    let listener = UnixListener::bind(socket_path)?;
    let _endpoint = crate::core::local_socket::Endpoint::capture(socket_path)?;
    // Bound the kernel admission queue, including while a command executes.
    use std::os::fd::AsRawFd;
    if unsafe { libc::listen(listener.as_raw_fd(), 64) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let session_started = Instant::now();
    // The foreground election honors the originating request's bounded budget.
    // Queueing and execution still share each request's original deadline.
    let session_budget =
        Duration::from_millis(local_request.deadline_ms.saturating_sub(epoch_ms()))
            .min(MAX_REQUEST_BUDGET);
    listener
        .set_nonblocking(true)
        .map_err(error::DecapodError::IoError)?;

    // The owner thread serves storage while isolated command children run.
    // Its RAII lifetime is the foreground election, never a detached daemon.
    // Keep rollout explicit until migration/file-replacement and every nested
    // connection lifetime have been covered by the owner protocol regressions.
    let storage_owner = if std::env::var("DECAPOD_STORAGE_OWNER").as_deref() == Ok("1") {
        let decapod_root = broker_root.parent().ok_or_else(|| {
            error::DecapodError::ValidationError("STORAGE_OWNER_ROOT_MISSING".into())
        })?;
        // Complete file-replacing migration recovery before retaining the file.
        crate::core::migration::check_and_migrate_with_backup_report(
            decapod_root,
            crate::subsystems::initialize_all_dbs,
        )?;
        Some(crate::core::storage_owner::Owner::start(&dedupe_db_path(
            broker_root,
        ))?)
    } else {
        None
    };
    emit_phase_hook("queued", &local_request.request_id);
    let local_response = execute_request(broker_root, &local_request, storage_owner.is_some())?;
    // A claim's client-side preparation must not wait for an idle period under
    // sustained traffic. Release this election immediately after its durable ack.
    if response_has_claim_followup(&local_response) {
        drop(listener);
        return Ok(local_response);
    }

    let idle_timeout = Duration::from_secs(
        std::env::var(BROKER_IDLE_SECS_ENV)
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(3),
    );
    let mut last_activity = Instant::now();

    loop {
        if last_activity.elapsed() >= idle_timeout || session_started.elapsed() >= session_budget {
            break;
        }

        match listener.accept() {
            Ok((stream, _)) => {
                // A committed request is activity even if its caller disconnected
                // before the acknowledgement. Delivery does not own this election.
                let _ = handle_client(broker_root, stream, storage_owner.is_some());
                last_activity = Instant::now();
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }

    Ok(local_response)
}

#[cfg(unix)]
fn handle_client(
    broker_root: &Path,
    stream: std::os::unix::net::UnixStream,
    owner_ready: bool,
) -> Result<(), error::DecapodError> {
    stream.set_nonblocking(false)?;
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(error::DecapodError::IoError)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(error::DecapodError::IoError)?;
    let line = read_request_line(&stream)?;
    let req: BrokerRequest = serde_json::from_str(line.trim()).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_INVALID_REQUEST: {e}"))
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

    let resp = execute_request(broker_root, &req, owner_ready)?;
    write_response(stream, &resp)?;
    Ok(())
}

#[cfg(unix)]
fn read_request_line(
    stream: &std::os::unix::net::UnixStream,
) -> Result<String, error::DecapodError> {
    read_line_until(
        stream,
        Instant::now() + Duration::from_secs(1),
        MAX_REQUEST_BYTES,
    )
}

#[cfg(unix)]
fn read_line_until(
    mut stream: &std::os::unix::net::UnixStream,
    deadline: Instant,
    max_bytes: u64,
) -> Result<String, error::DecapodError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| {
                error::DecapodError::ValidationError("BROKER_FRAME_READ_DEADLINE".into())
            })?;
        stream.set_read_timeout(Some(remaining))?;
        let n = stream.read(&mut buffer)?;
        if n == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
        let previous = bytes.len();
        bytes.extend_from_slice(&buffer[..n]);
        if bytes.len() as u64 > max_bytes {
            return Err(error::DecapodError::ValidationError(
                "BROKER_REQUEST_TOO_LARGE".into(),
            ));
        }
        if let Some(end) = buffer[..n].iter().position(|b| *b == b'\n') {
            bytes.truncate(previous + end);
            return String::from_utf8(bytes)
                .map_err(|_| error::DecapodError::ValidationError("BROKER_REQUEST_UTF8".into()));
        }
    }
}

#[cfg(unix)]
fn send_request(
    socket_path: &Path,
    request: &BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    let mut stream = crate::core::local_socket::connect(socket_path)?;
    let remaining = Duration::from_millis(request.deadline_ms.saturating_sub(epoch_ms()).max(1));
    stream.set_read_timeout(Some(remaining))?;
    stream.set_write_timeout(Some(remaining))?;
    let unknown = |_| {
        error::DecapodError::ValidationError(format!(
            "BROKER_UNKNOWN: acknowledgement lost for request_id={}; inspect state or retrieve the same request ID; never replay under a new ID",
            request.request_id
        ))
    };

    let payload = serde_json::to_string(request).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_ENCODE_ERROR: {e}"))
    })?;
    stream.write_all(payload.as_bytes()).map_err(unknown)?;
    stream.write_all(b"\n").map_err(unknown)?;
    stream.flush().map_err(unknown)?;
    emit_phase_hook("sent", &request.request_id);
    // One budget covers queueing and execution; retries never reset it.
    let deadline =
        Instant::now() + Duration::from_millis(request.deadline_ms.saturating_sub(epoch_ms()));
    // Recompute the remaining budget for every read, including a trickling peer.
    // The ceiling accommodates JSON escaping of bounded stdout and stderr.
    let line = read_line_until(&stream, deadline, 1024 * 1024 * 1024)
        .map_err(|_| unknown(std::io::Error::other("reply deadline or frame")))?;
    let resp: BrokerResponse = serde_json::from_str(line.trim())
        .map_err(|_| unknown(std::io::Error::other("invalid reply")))?;
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
    owner_ready: bool,
) -> Result<BrokerResponse, error::DecapodError> {
    request.caller.validate()?;
    if request.payload_hash != hash_payload(&request.argv, &request.caller) {
        return Err(error::DecapodError::ValidationError(
            "BROKER_INVALID_PAYLOAD_HASH".into(),
        ));
    }
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

    // Native peers cannot extend execution beyond the same public hard ceiling.
    // An existing durable outcome takes precedence over retry timing. Reject
    // only new admission here; malformed retries cannot erase known uncertainty.
    if request.deadline_ms.saturating_sub(epoch_ms()) > MAX_REQUEST_BUDGET.as_millis() as u64 {
        return Ok(BrokerResponse {
            protocol_version: server_protocol_version(),
            status: "NOT_COMMITTED".into(),
            commit_marker: None,
            result_envelope: serde_json::json!({
                "error": "BROKER_REQUEST_BUDGET_INVALID",
                "request_id": request.request_id,
            }),
            retry_after_ms_hint: None,
        });
    }
    if epoch_ms() >= request.deadline_ms {
        return Ok(BrokerResponse {
            protocol_version: server_protocol_version(),
            status: "NOT_COMMITTED".into(),
            commit_marker: None,
            retry_after_ms_hint: None,
            result_envelope: serde_json::json!({"error": "BROKER_DEADLINE_BEFORE_EXECUTION", "request_id": request.request_id}),
        });
    }
    // Persist uncertainty BEFORE execution. A crash between the mutation and
    // final receipt must never turn a same-ID retry into a second execution.
    dedupe_store(
        broker_root,
        request,
        &BrokerResponse {
            protocol_version: server_protocol_version(),
            status: "UNKNOWN".into(),
            commit_marker: None,
            retry_after_ms_hint: None,
            result_envelope: serde_json::json!({"error": "BROKER_EXECUTION_STARTED_OUTCOME_UNKNOWN", "request_id": request.request_id}),
        },
    )?;

    // Resolve the running binary independently of either caller's working directory.
    let exe = std::env::current_exe()?;
    emit_phase_hook("pre_exec", &request.request_id);
    let claim_request = request.argv.iter().any(|arg| arg == "todo")
        && request.argv.iter().any(|arg| arg == "claim");
    let timeout = if claim_request
        || request.argv.windows(2).any(|args| {
            (args[0] == "todo" && args[1] == "claim")
                || (args[0] == "workspace" && matches!(args[1].as_str(), "status" | "prune"))
        }) {
        CONTROL_TIMEOUT
    } else {
        BUILD_TIMEOUT
    };
    let timeout = timeout.min(Duration::from_millis(
        request.deadline_ms.saturating_sub(epoch_ms()).max(1),
    ));
    let mut command = Command::new(exe);
    command.env_remove("DECAPOD_STORAGE_OWNER_READY");
    if owner_ready {
        command.env(
            "DECAPOD_STORAGE_OWNER_READY",
            dedupe_db_path(broker_root).canonicalize()?,
        );
    }
    request.caller.apply(&mut command);
    let output = match command
        .args(&request.argv)
        .env(BROKER_INTERNAL_ENV, "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", &request.request_id)
        .bounded_output(timeout)
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
        // A failing command can have committed part of a multi-step operation.
        "UNKNOWN"
    };

    emit_phase_hook("post_exec_pre_ack", &request.request_id);
    let response = BrokerResponse {
        protocol_version: server_protocol_version(),
        status: status.to_string(),
        commit_marker: Some(format!(
            "{}:{}",
            time::now_epoch_z(),
            crate::core::ulid::new_ulid()
        )),
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

fn response_has_claim_followup(response: &BrokerResponse) -> bool {
    response.status == "COMMITTED"
        && response.result_envelope["stdout"]
            .as_str()
            .and_then(|stdout| serde_json::from_str::<serde_json::Value>(stdout).ok())
            .is_some_and(|out| {
                out["cmd"] == "todo.claim"
                    && out["status"] == "ok"
                    && out["container"]["code"] == "claim_container_followup"
            })
}

fn apply_response(
    resp: BrokerResponse,
    request_id: &str,
    finish: &mut dyn FnMut(&str, &str) -> String,
) -> Result<(), error::DecapodError> {
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
        if resp.status == "COMMITTED" {
            print!("{}", finish(stdout, request_id));
        } else {
            print!("{stdout}");
        }
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
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
            "BROKER_UNKNOWN: no final confirmation for request_id={request_id}; inspect persisted state; same-ID retrieval will not re-execute (retry_after_ms_hint={})",
            resp.retry_after_ms_hint.unwrap_or(5000)
        ))),
    }
}

fn hash_payload(argv: &[String], caller: &CallerContext) -> String {
    let mut hasher = Sha256::new();
    // Length-delimited structured encoding binds replay to both the original
    // actor and workspace, not merely identical command arguments. Credential
    // rotation for that actor must not create a new logical mutation.
    let environment: BTreeMap<_, _> = caller
        .environment
        .iter()
        .filter(|(key, _)| key.as_str() != "DECAPOD_SESSION_PASSWORD")
        .collect();
    let payload = serde_json::to_vec(&(argv, &caller.cwd, environment))
        .expect("broker payload contains only serializable strings and paths");
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

fn broker_lock_path(broker_root: &Path) -> PathBuf {
    broker_root.join("broker.lock")
}

#[cfg(unix)]
#[doc(hidden)]
pub fn broker_socket_path(broker_root: &Path) -> std::io::Result<PathBuf> {
    crate::core::local_socket::path(broker_root, "broker.sock")
}

#[cfg(all(test, unix))]
fn ensure_socket_directory(path: &Path) -> Result<(), error::DecapodError> {
    use std::os::unix::fs::PermissionsExt;
    crate::core::fs_permissions::ensure_storage_dir(path).map_err(error::DecapodError::IoError)?;
    let forbidden = if crate::core::fs_permissions::shared_storage() {
        0o007
    } else {
        0o077
    };
    if std::fs::metadata(path)?.permissions().mode() & forbidden != 0 {
        return Err(error::DecapodError::ValidationError(format!(
            "BROKER_UNSAFE_SOCKET_DIRECTORY: {} permits access outside the approved private/shared-group boundary. No permissions were changed; ask the owner to review this runtime directory before retrying.",
            path.display()
        )));
    }
    Ok(())
}

fn dedupe_db_path(broker_root: &Path) -> PathBuf {
    broker_root.join(crate::core::schemas::LOCAL_DB_NAME)
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
        .map_err(error::DecapodError::StorageError)?;

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
        error::DecapodError::ValidationError(format!("BROKER_DEDUPE_ENCODE_FAILED: {e}"))
    })?;

    conn.execute(
        "INSERT OR REPLACE INTO request_dedupe(request_id, payload_hash, status, commit_marker, result_envelope, retry_after_ms_hint, created_at)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        crate::core::db::params![
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

fn ensure_dedupe_schema(conn: &crate::core::db::Connection) -> Result<(), error::DecapodError> {
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
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_ENCODE_ERROR: {e}"))
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
        && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
    {
        let _ = writeln!(file, "{phase}|{request_id}");
    }
    if std::env::var(BROKER_HALT_PHASE_ENV).ok().as_deref() == Some(phase)
        && std::env::var("DECAPOD_GROUP_BROKER_TEST_HALT_REQUEST")
            .ok()
            .is_none_or(|id| id == request_id)
    {
        loop {
            if std::env::var_os("DECAPOD_GROUP_BROKER_TEST_RELEASE")
                .is_some_and(|path| Path::new(&path).exists())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn try_acquire_lock(lock_path: &Path) -> Result<Option<BrokerLease>, error::DecapodError> {
    // Keep the advisory-lock inode stable. Unlinking a locked file can elect
    // two leaders on different inodes. broker.lock is only a diagnostic PID
    // marker; the OS releases broker.election automatically on interruption.
    let election = crate::core::fs_permissions::open_storage_file(
        &lock_path.with_extension("election"),
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false),
    )
    .map_err(error::DecapodError::IoError)?;
    match election.try_lock_exclusive() {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
        Err(err) => return Err(error::DecapodError::IoError(err)),
    }
    let lease = BrokerLease {
        path: lock_path.to_path_buf(),
        _file: election,
    };
    let mut marker = crate::core::fs_permissions::open_storage_file(
        lock_path,
        OpenOptions::new().create(true).write(true).truncate(true),
    )
    .map_err(error::DecapodError::IoError)?;
    writeln!(marker, "{}", std::process::id()).map_err(error::DecapodError::IoError)?;
    Ok(Some(lease))
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
        let _ = FileExt::unlock(&self._file);
    }
}
#[cfg(all(test, unix))]
#[path = "../../../tests/unit/core/group_broker_tests.rs"]
mod tests;

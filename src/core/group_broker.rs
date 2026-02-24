use crate::core::error;
use crate::core::time;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrokerRequest {
    request_id: String,
    argv: Vec<String>,
    payload_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BrokerResponse {
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

pub fn maybe_route_mutation(decapod_root: &Path, argv: &[String]) -> Result<bool, error::DecapodError> {
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
        return run_unix_broker(decapod_root, argv).map(|_| true);
    }

    #[cfg(not(unix))]
    {
        let _ = decapod_root;
        let _ = argv;
        Ok(false)
    }
}

#[cfg(unix)]
fn run_unix_broker(decapod_root: &Path, argv: &[String]) -> Result<(), error::DecapodError> {
    fs::create_dir_all(decapod_root).map_err(error::DecapodError::IoError)?;
    let socket_path = broker_socket_path(decapod_root);
    let lock_path = broker_lock_path(decapod_root);

    let request = BrokerRequest {
        request_id: Ulid::new().to_string(),
        argv: argv.to_vec(),
        payload_hash: hash_payload(argv),
    };

    if let Ok(resp) = send_request(&socket_path, &request) {
        return apply_response(resp);
    }

    let mut attempts = 0u32;
    loop {
        attempts += 1;
        match try_acquire_lock(&lock_path)? {
            Some(lease) => {
                let resp = run_as_leader(lease, &socket_path, request)?;
                return apply_response(resp);
            }
            None => {
                if let Ok(resp) = send_request(&socket_path, &request) {
                    return apply_response(resp);
                }
                if attempts >= 40 {
                    return Err(error::DecapodError::ValidationError(
                        "BROKER_UNKNOWN: unable to reach or acquire group broker".to_string(),
                    ));
                }
                std::thread::sleep(Duration::from_millis(10 + jitter_ms(30)));
            }
        }
    }
}

#[cfg(unix)]
fn run_as_leader(
    _lease: BrokerLease,
    socket_path: &Path,
    local_request: BrokerRequest,
) -> Result<BrokerResponse, error::DecapodError> {
    use std::os::unix::net::UnixListener;

    if socket_path.exists() {
        let _ = fs::remove_file(socket_path);
    }
    let listener = UnixListener::bind(socket_path).map_err(error::DecapodError::IoError)?;
    listener
        .set_nonblocking(true)
        .map_err(error::DecapodError::IoError)?;

    let local_response = execute_request(&local_request)?;

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
                if handle_client(stream).is_ok() {
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
fn handle_client(stream: std::os::unix::net::UnixStream) -> Result<(), error::DecapodError> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .map_err(error::DecapodError::IoError)?,
    );
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(error::DecapodError::IoError)?;
    let req: BrokerRequest = serde_json::from_str(line.trim()).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_INVALID_REQUEST: {}", e))
    })?;

    let resp = execute_request(&req)?;
    let mut writer = stream;
    let body = serde_json::to_string(&resp).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_ENCODE_ERROR: {}", e))
    })?;
    writer
        .write_all(body.as_bytes())
        .map_err(error::DecapodError::IoError)?;
    writer
        .write_all(b"\n")
        .map_err(error::DecapodError::IoError)?;
    writer.flush().map_err(error::DecapodError::IoError)?;
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
    serde_json::from_str(line.trim()).map_err(|e| {
        error::DecapodError::ValidationError(format!("BROKER_PROTOCOL_INVALID_RESPONSE: {}", e))
    })
}

fn execute_request(request: &BrokerRequest) -> Result<BrokerResponse, error::DecapodError> {
    let exe = std::env::current_exe().map_err(error::DecapodError::IoError)?;
    let output = Command::new(exe)
        .args(&request.argv)
        .env(BROKER_INTERNAL_ENV, "1")
        .env("DECAPOD_GROUP_BROKER_REQUEST_ID", &request.request_id)
        .output()
        .map_err(error::DecapodError::IoError)?;

    let code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let result_envelope = serde_json::json!({
        "request_id": request.request_id,
        "payload_hash": request.payload_hash,
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
    });

    let status = if code == 0 {
        "COMMITTED"
    } else {
        "NOT_COMMITTED"
    };

    Ok(BrokerResponse {
        status: status.to_string(),
        commit_marker: Some(format!("{}:{}", time::now_epoch_z(), Ulid::new())),
        result_envelope,
        retry_after_ms_hint: if code == 0 { None } else { Some(5000) },
    })
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
        "NOT_COMMITTED" => Err(error::DecapodError::ValidationError(format!(
            "BROKER_NOT_COMMITTED: request failed (commit_marker={})",
            resp.commit_marker.unwrap_or_else(|| "<none>".to_string())
        ))),
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

fn broker_lock_path(decapod_root: &Path) -> PathBuf {
    decapod_root.join("broker.lock")
}

fn broker_socket_path(decapod_root: &Path) -> PathBuf {
    decapod_root.join("broker.sock")
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
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => return Ok(None),
        Err(err) => return Err(error::DecapodError::IoError(err)),
    };

    Ok(Some(BrokerLease {
        path: lock_path.to_path_buf(),
        _file: file,
    }))
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

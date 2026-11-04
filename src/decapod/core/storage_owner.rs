//! Foreground, single-threaded physical storage ownership.
//!
//! A proxy leases the connection, not an individual SQL statement. This keeps
//! transactions and connection-local state from interleaving between callers.
//! The enclosing group-broker election owns this runtime; no daemon is spawned.

use crate::core::dactyl_db::{Error, Result};
use crate::core::error::DecapodError;
use dactyl_db::{AccessMode, Parameter, Rows, StoreSchema};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const VERSION: u32 = 1;
const MAX_FRAME: usize = 16 * 1024 * 1024;
const LEASE_IDLE_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
const DRAIN_BYTE: u8 = b'D';

// Credentials deliberately never enter the storage protocol. This metadata is
// attribution, not authentication or authority to exceed the socket boundary.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Caller {
    agent_id: String,
    session_id: Option<String>,
    task_id: Option<String>,
    correlation_id: Option<String>,
    causation_id: Option<String>,
    request_id: Option<String>,
}

impl Caller {
    fn capture() -> Self {
        Self {
            agent_id: crate::current_agent_id(),
            session_id: std::env::var("DECAPOD_SESSION_ID").ok(),
            task_id: std::env::var("DECAPOD_TASK_ID").ok(),
            correlation_id: std::env::var("DECAPOD_CORRELATION_ID").ok(),
            causation_id: std::env::var("DECAPOD_CAUSATION_ID").ok(),
            request_id: std::env::var("DECAPOD_GROUP_BROKER_REQUEST_ID").ok(),
        }
    }

    fn valid(&self) -> bool {
        !self.agent_id.trim().is_empty()
            && self.agent_id.len() <= 4096
            && [
                &self.session_id,
                &self.task_id,
                &self.correlation_id,
                &self.causation_id,
                &self.request_id,
            ]
            .into_iter()
            .all(|value| value.as_ref().is_none_or(|value| value.len() <= 4096))
    }
}

// Tagged encoding avoids losing distinctions such as integer versus real or
// non-finite f64 values when JSON is used as the transport encoding.
#[derive(Serialize, Deserialize)]
enum WireParameter {
    Null,
    Bool(bool),
    Integer(i64),
    Real(u64),
    Text(String),
    Blob(Vec<u8>),
}

impl From<&Parameter> for WireParameter {
    fn from(value: &Parameter) -> Self {
        match value {
            Parameter::Null => Self::Null,
            Parameter::Bool(value) => Self::Bool(*value),
            Parameter::Integer(value) => Self::Integer(*value),
            Parameter::Real(value) => Self::Real(value.to_bits()),
            Parameter::Text(value) => Self::Text(value.clone()),
            Parameter::Blob(value) => Self::Blob(value.clone()),
        }
    }
}

impl From<WireParameter> for Parameter {
    fn from(value: WireParameter) -> Self {
        match value {
            WireParameter::Null => Self::Null,
            WireParameter::Bool(value) => Self::Bool(value),
            WireParameter::Integer(value) => Self::Integer(value),
            WireParameter::Real(value) => Self::Real(f64::from_bits(value)),
            WireParameter::Text(value) => Self::Text(value),
            WireParameter::Blob(value) => Self::Blob(value),
        }
    }
}

#[derive(Serialize, Deserialize)]
enum Request {
    Open {
        version: u32,
        database: PathBuf,
        read_only: bool,
        caller: Caller,
    },
    Read {
        sql: String,
        params: Vec<WireParameter>,
    },
    Write {
        sql: String,
        params: Vec<WireParameter>,
    },
    Inspect,
    Close,
}

#[derive(Serialize, Deserialize)]
enum Response {
    Ready { version: u32 },
    Rows(Rows),
    Count(u64),
    Schema(StoreSchema),
    Closed,
    Error(String),
    SqlError(WireSqlError),
}

// Known SQL failures retain their normalized category and detail. Transport
// failures remain a separate outcome-unknown error and cannot authorize replay.
#[derive(Serialize, Deserialize)]
enum WireSqlError {
    Config(String),
    Adapter {
        kind: WireAdapterKind,
        code: Option<String>,
        message: String,
    },
    UnsupportedOperation(String),
    ColumnNotFound(String),
    Conversion(String),
}

macro_rules! adapter_kinds {
    ($($kind:ident),+ $(,)?) => {
        #[derive(Serialize, Deserialize)]
        enum WireAdapterKind { $($kind),+ }
        impl From<dactyl_db::AdapterErrorKind> for WireAdapterKind {
            fn from(value: dactyl_db::AdapterErrorKind) -> Self {
                match value { $(dactyl_db::AdapterErrorKind::$kind => Self::$kind),+ }
            }
        }
        impl From<WireAdapterKind> for dactyl_db::AdapterErrorKind {
            fn from(value: WireAdapterKind) -> Self {
                match value { $(WireAdapterKind::$kind => Self::$kind),+ }
            }
        }
    };
}
adapter_kinds!(
    Busy,
    Locked,
    Timeout,
    Constraint,
    Conflict,
    VersionConflict,
    TransactionAborted,
    IdempotencyConflict,
    IdempotencyInProgress,
    Query,
    InvalidOperation,
    ReadOnly,
    Capability,
    Value,
    Storage,
    Corrupt,
    Transport,
    Protocol,
    Authentication,
    Authorization,
    RateLimited,
    Quota,
    NotFound,
    Unavailable,
    Cancellation,
    Unknown
);

impl From<dactyl_db::DactylError> for WireSqlError {
    fn from(value: dactyl_db::DactylError) -> Self {
        use dactyl_db::DactylError;
        match value {
            DactylError::Config(message) => Self::Config(message),
            DactylError::Adapter {
                kind,
                code,
                message,
            } => Self::Adapter {
                kind: kind.into(),
                code,
                message,
            },
            DactylError::UnsupportedOperation(message) => Self::UnsupportedOperation(message),
            DactylError::ColumnNotFound(message) => Self::ColumnNotFound(message),
            DactylError::Conversion(message) => Self::Conversion(message),
        }
    }
}
impl From<WireSqlError> for dactyl_db::DactylError {
    fn from(value: WireSqlError) -> Self {
        match value {
            WireSqlError::Config(message) => Self::Config(message),
            WireSqlError::Adapter {
                kind,
                code,
                message,
            } => Self::Adapter {
                kind: kind.into(),
                code,
                message,
            },
            WireSqlError::UnsupportedOperation(message) => Self::UnsupportedOperation(message),
            WireSqlError::ColumnNotFound(message) => Self::ColumnNotFound(message),
            WireSqlError::Conversion(message) => Self::Conversion(message),
        }
    }
}
fn sql_failure(error: Error) -> Response {
    match error {
        Error::Dactyl(error) => Response::SqlError(error.into()),
        _ => Response::Error("STORAGE_OWNER_EXECUTION_OUTCOME_UNKNOWN".into()),
    }
}

fn failure(code: &str) -> Error {
    Error::StorageOwner(code.to_owned())
}

fn write_frame<T: Serialize>(stream: &mut impl Write, value: &T) -> Result<()> {
    let body = serde_json::to_vec(value).map_err(|_| failure("STORAGE_OWNER_ENCODE"))?;
    if body.len() > MAX_FRAME {
        return Err(failure("STORAGE_OWNER_FRAME_LIMIT"));
    }
    stream
        .write_all(&(body.len() as u32).to_be_bytes())
        .map_err(|_| failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"))?;
    stream
        .write_all(&body)
        .map_err(|_| failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"))?;
    stream
        .flush()
        .map_err(|_| failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"))
}

fn read_frame<T: serde::de::DeserializeOwned>(stream: &mut impl Read) -> Result<T> {
    let mut size = [0; 4];
    stream
        .read_exact(&mut size)
        .map_err(|_| failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"))?;
    let length = u32::from_be_bytes(size) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err(failure("STORAGE_OWNER_FRAME_LIMIT"));
    }
    let mut body = vec![0; length];
    stream
        .read_exact(&mut body)
        .map_err(|_| failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"))?;
    serde_json::from_slice(&body).map_err(|_| failure("STORAGE_OWNER_PROTOCOL"))
}

fn canonical_database(path: &Path) -> Result<PathBuf> {
    if path.file_name().and_then(|name| name.to_str()) != Some(crate::core::schemas::LOCAL_DB_NAME)
    {
        return Err(failure("STORAGE_OWNER_NONCANONICAL_DATABASE"));
    }
    // The owner is created only after store initialization. Resolve symlinks so
    // worktrees and alternate path spellings discover the same endpoint.
    path.canonicalize().map_err(Error::Io)
}

fn socket_path(database: &Path) -> std::io::Result<PathBuf> {
    crate::core::local_socket::path(
        database.parent().expect("canonical database parent"),
        "storage.sock",
    )
}

/// Probe without SQL. An accepted-but-unanswered handshake is an error, not
/// evidence that the database can safely be replaced or opened locally.
#[cfg(unix)]
pub(crate) fn owner_is_active(db_path: &Path) -> Result<bool> {
    Ok(Proxy::try_connect(db_path, AccessMode::ReadOnly, Duration::from_millis(250))?.is_some())
}

/// One foreground-owned storage process. SQLite execution is killable without
/// leaving an unjoinable thread in the caller. No worker is detached.
pub(crate) struct Owner {
    worker: Option<std::process::Child>,
    lifetime: Option<std::process::ChildStdin>,
    socket: PathBuf,
    socket_identity: Option<(u64, u64)>,
}

impl Owner {
    /// Caller holds the group-broker election and has completed migrations.
    pub(crate) fn start(db_path: &Path) -> std::result::Result<Self, DecapodError> {
        use std::process::{Command, Stdio};
        let database = canonical_database(db_path).map_err(DecapodError::StorageError)?;
        let socket = socket_path(&database)?;
        let mut command = Command::new(std::env::current_exe().map_err(DecapodError::IoError)?);
        command
            .arg("__storage-owner-worker")
            .arg(&database)
            .env("DECAPOD_STORAGE_OWNER_WORKER", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Storage ownership is not the initiating agent's identity/session.
        for name in [
            "DECAPOD_AGENT_ID",
            "DECAPOD_AGENT_PROVIDER",
            "DECAPOD_SESSION_ID",
            "DECAPOD_SESSION_PASSWORD",
            "DECAPOD_TASK_ID",
            "DECAPOD_CORRELATION_ID",
            "DECAPOD_CAUSATION_ID",
            "DECAPOD_IDEMPOTENCY_KEY",
        ] {
            command.env_remove(name);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            let parent_pid = std::process::id() as libc::pid_t;
            // As in bounded_process, install the death signal before exec and
            // close the race where the parent exits before prctl completes.
            unsafe {
                command.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != parent_pid {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "storage owner parent exited",
                        ));
                    }
                    Ok(())
                });
            }
        }
        let mut child = command.spawn().map_err(DecapodError::IoError)?;
        let lifetime = child.stdin.take();
        let stdout = child.stdout.take();
        let mut owner = Self {
            worker: Some(child),
            lifetime,
            socket,
            socket_identity: None,
        };
        let Some(mut stdout) = stdout else {
            owner.shutdown();
            return Err(DecapodError::ValidationError(
                "STORAGE_OWNER_STARTUP_PIPE_MISSING".into(),
            ));
        };
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut ready = [0u8; 1];
            let result = stdout.read_exact(&mut ready).is_ok() && ready == *b"R";
            let _ = ready_tx.send(result);
        });
        let ready = ready_rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or(false);
        if !ready {
            owner.shutdown();
        }
        // The reader consumes one byte, or EOF after the failed worker is killed.
        let _ = reader.join();
        if ready {
            use std::os::unix::fs::MetadataExt;
            owner.socket_identity = std::fs::symlink_metadata(&owner.socket)
                .ok()
                .map(|metadata| (metadata.dev(), metadata.ino()));
            Ok(owner)
        } else {
            Err(DecapodError::ValidationError(
                "STORAGE_OWNER_STARTUP_FAILED: worker terminated without a ready handshake".into(),
            ))
        }
    }

    fn shutdown(&mut self) {
        // Normal foreground shutdown drains an established lease. Keep the
        // lifetime pipe open until the child exits: EOF means parent death and
        // intentionally interrupts even SQL already executing in the worker.
        let draining = self.lifetime.as_mut().is_some_and(|pipe| {
            pipe.write_all(&[DRAIN_BYTE])
                .and_then(|()| pipe.flush())
                .is_ok()
        });
        if let Some(mut child) = self.worker.take() {
            let grace = if draining {
                SHUTDOWN_GRACE
            } else {
                Duration::ZERO
            };
            let deadline = Instant::now() + grace;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    _ if Instant::now() >= deadline => {
                        // Reap an owned child after SIGKILL, never join SQL work.
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        }
        self.lifetime.take();
        use std::os::unix::fs::MetadataExt;
        if let Some(identity) = self.socket_identity.take()
            && std::fs::symlink_metadata(&self.socket)
                .ok()
                .is_some_and(|metadata| (metadata.dev(), metadata.ino()) == identity)
        {
            let _ = std::fs::remove_file(&self.socket);
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn bind_listener(
    database: &Path,
) -> std::result::Result<std::os::unix::net::UnixListener, DecapodError> {
    use std::os::unix::net::UnixListener;
    let socket = socket_path(database)?;
    crate::core::local_socket::ensure_directory(&socket, database.parent().unwrap())?;
    // Election ownership and the lifecycle lock protect concurrent startup.
    crate::core::local_socket::remove_stale(&socket)?;
    let listener = UnixListener::bind(&socket).map_err(DecapodError::IoError)?;
    listener
        .set_nonblocking(true)
        .map_err(DecapodError::IoError)?;
    Ok(listener)
}

/// Internal early entrypoint. A lifetime pipe is mandatory; EOF kills this
/// dedicated process even while SQLite is evaluating a non-terminating query.
pub(crate) fn run_worker(path: &Path) -> std::result::Result<(), DecapodError> {
    if std::env::var("DECAPOD_STORAGE_OWNER_WORKER").as_deref() != Ok("1") {
        return Err(DecapodError::ValidationError(
            "STORAGE_OWNER_WORKER_AUTHORITY_MISSING".into(),
        ));
    }
    // /dev/stdin can resolve to a platform descriptor path; fstat checks the
    // actual inherited descriptor without accepting a terminal or regular file.
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    let is_pipe = unsafe {
        libc::fstat(libc::STDIN_FILENO, stat.as_mut_ptr()) == 0
            && (stat.assume_init().st_mode & libc::S_IFMT) == libc::S_IFIFO
    };
    if !is_pipe {
        return Err(DecapodError::ValidationError(
            "STORAGE_OWNER_LIFETIME_PIPE_REQUIRED".into(),
        ));
    }
    let stop = Arc::new(AtomicBool::new(false));
    let watchdog_stop = Arc::clone(&stop);
    std::thread::spawn(move || {
        let mut byte = [0u8; 1];
        loop {
            match std::io::stdin().read(&mut byte) {
                // Parent death is not graceful drain: terminate even if the
                // storage thread is blocked inside SQL evaluation.
                Ok(0) | Err(_) => std::process::exit(0),
                Ok(_) if byte[0] == DRAIN_BYTE => {
                    watchdog_stop.store(true, Ordering::Release);
                }
                Ok(_) => {}
            }
        }
    });
    crate::core::dactyl::ensure_local_sqlite_runtime()?;
    let database = canonical_database(path).map_err(DecapodError::StorageError)?;
    let conn = crate::core::db::db_connect_owner(&database)?;
    let listener = bind_listener(&database)?;
    let _endpoint = crate::core::local_socket::Endpoint::capture(&socket_path(&database)?)?;
    std::io::stdout()
        .write_all(b"R")
        .map_err(DecapodError::IoError)?;
    std::io::stdout().flush().map_err(DecapodError::IoError)?;
    let result = serve_connections(&conn, &database, listener, &stop);
    result.map_err(DecapodError::StorageError)
}

pub(crate) fn emit_open_hook() {
    emit_phase_hook("storage_owner_open");
}

fn emit_phase_hook(phase: &str) {
    if let Ok(path) = std::env::var("DECAPOD_GROUP_BROKER_TEST_HOOK_FILE")
        && let Ok(mut hook) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
    {
        let _ = writeln!(hook, "{phase}|{}", std::process::id());
    }
}

fn serve_connections(
    conn: &crate::core::db::Connection,
    database: &Path,
    listener: std::os::unix::net::UnixListener,
    stop: &AtomicBool,
) -> Result<()> {
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                // Do not admit a new lease after drain was requested between
                // the loop check and accept. Already-admitted leases finish.
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let result = serve_lease(conn, database, stream);
                #[cfg(test)]
                if let Err(error) = &result {
                    eprintln!("storage lease ended: {error}");
                }
                let _ = result;
                // Always attempt physical rollback: facade bookkeeping is not
                // authority to expose an unfinished transaction to the next lease.
                reset_connection(conn)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err(error) => return Err(Error::Io(error)),
        }
    }
    Ok(())
}

fn reset_connection(conn: &crate::core::db::Connection) -> Result<()> {
    if let Err(error) = conn.owner_write("ROLLBACK", &[]) {
        // SQLite returns this exact condition for an already-autocommit handle.
        // Every other failed rollback retires the worker instead of serving again.
        let no_transaction = matches!(&error, Error::Dactyl(dactyl_db::DactylError::Adapter { message, .. })
            if message.contains("no transaction is active"));
        if !no_transaction {
            return Err(error);
        }
    }
    conn.owner_write("PRAGMA query_only=OFF", &[])?;
    // Explicit temp-schema qualification can create temporary objects even
    // without the TEMP keyword. Remove every connection-local object before
    // granting the next caller a lease. Failure retires the worker.
    let objects = conn.owner_read("SELECT type, name FROM sqlite_temp_master WHERE name NOT LIKE 'sqlite_%' AND type IN ('table','view','trigger') ORDER BY CASE type WHEN 'trigger' THEN 0 WHEN 'view' THEN 1 ELSE 2 END", &[])?;
    for object in objects.0 {
        let kind = object.values[0]
            .as_str()
            .ok_or_else(|| failure("STORAGE_OWNER_RESET_SCHEMA"))?;
        let name = object.values[1]
            .as_str()
            .ok_or_else(|| failure("STORAGE_OWNER_RESET_SCHEMA"))?
            .replace('"', r#""""#);
        conn.owner_write(&format!("DROP {kind} IF EXISTS temp.\"{name}\""), &[])?;
    }
    Ok(())
}

/// The reusable path currently supports full transactions only. Reject
/// savepoints and rollback-to rather than approximating physical transaction
/// state. Reject leading comments and multi-statements for an unambiguous gate.
fn permitted_write(sql: &str) -> bool {
    let trimmed = sql.trim();
    let statement = trimmed.strip_suffix(';').unwrap_or(trimmed);
    if statement.contains(';') {
        return false;
    }
    let words: Vec<_> = statement
        .split_whitespace()
        .take(3)
        .map(str::to_ascii_uppercase)
        .collect();
    match words.first().map(String::as_str) {
        Some("SAVEPOINT" | "RELEASE") | None => false,
        Some("ROLLBACK") => words.len() == 1 || (words.len() == 2 && words[1] == "TRANSACTION"),
        // Only statements without connection-local configuration are supported.
        // In particular TEMP, ATTACH, PRAGMA and virtual tables would leak state
        // between agents; they need an explicit future protocol, not SQL passthrough.
        Some("INSERT" | "UPDATE" | "DELETE" | "REPLACE" | "BEGIN" | "COMMIT" | "END") => true,
        Some("CREATE") => matches!(
            words.get(1).map(String::as_str),
            Some("TABLE" | "INDEX" | "UNIQUE" | "VIEW" | "TRIGGER")
        ),
        Some("ALTER" | "DROP") => matches!(
            words.get(1).map(String::as_str),
            Some("TABLE" | "INDEX" | "VIEW" | "TRIGGER")
        ),
        _ => false,
    }
}

/// SQL in a Read frame must not turn off the server's query-only protection or
/// change connection ownership. SQLite itself rejects writes in CTEs under
/// query_only. Disallow control statements rather than guessing PRAGMA safety.
fn permitted_read(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    if [
        "last_insert_rowid",
        "total_changes",
        "changes",
        "load_extension",
    ]
    .iter()
    .any(|name| lower.contains(name))
    {
        return false;
    }
    let keyword: String = sql
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    matches!(
        keyword.to_ascii_uppercase().as_str(),
        "SELECT" | "WITH" | "EXPLAIN"
    )
}

#[cfg(unix)]
fn serve_lease(
    conn: &crate::core::db::Connection,
    database: &Path,
    mut stream: std::os::unix::net::UnixStream,
) -> Result<()> {
    // Darwin may inherit O_NONBLOCK from the listening descriptor. Timeouts
    // only bound blocking I/O; normalize accepted streams on every Unix target.
    stream.set_nonblocking(false).map_err(Error::Io)?;
    stream
        .set_read_timeout(Some(LEASE_IDLE_TIMEOUT))
        .map_err(Error::Io)?;
    stream
        .set_write_timeout(Some(LEASE_IDLE_TIMEOUT))
        .map_err(Error::Io)?;
    let Request::Open {
        version,
        database: requested,
        read_only,
        caller,
    } = read_frame(&mut stream)?
    else {
        return Err(failure("STORAGE_OWNER_OPEN_REQUIRED"));
    };
    if version != VERSION || requested != database || !caller.valid() {
        write_frame(
            &mut stream,
            &Response::Error("STORAGE_OWNER_CONTEXT_MISMATCH".into()),
        )?;
        return Err(failure("STORAGE_OWNER_CONTEXT_MISMATCH"));
    }
    // Bind the immutable context to this entire stream lease. SQL parameters
    // and application audit events still come from the isolated caller process.
    let _caller = caller;
    conn.owner_write(
        if read_only {
            "PRAGMA query_only=ON"
        } else {
            "PRAGMA query_only=OFF"
        },
        &[],
    )?;
    write_frame(&mut stream, &Response::Ready { version: VERSION })?;
    // A normal drain cannot split an established caller's transaction. The
    // enclosing owner enforces the shutdown deadline, and idle reads are timed.
    loop {
        let request = read_frame(&mut stream)?;
        let response = match request {
            Request::Read { sql, params } if permitted_read(&sql) => {
                let params: Vec<Parameter> = params.into_iter().map(Into::into).collect();
                emit_phase_hook("storage_owner_read");
                match conn.owner_read(&sql, &params) {
                    Ok(rows) => Response::Rows(rows),
                    Err(error) => sql_failure(error),
                }
            }
            Request::Read { .. } => Response::Error("STORAGE_OWNER_READ_CONTROL_REJECTED".into()),
            Request::Write { sql, params } if !read_only && permitted_write(&sql) => {
                let params: Vec<Parameter> = params.into_iter().map(Into::into).collect();
                match conn.owner_write(&sql, &params) {
                    Ok(count) => Response::Count(count),
                    Err(error) => sql_failure(error),
                }
            }
            Request::Write { .. } if read_only => Response::Error("STORAGE_OWNER_READ_ONLY".into()),
            Request::Write { .. } => {
                Response::Error("STORAGE_OWNER_TRANSACTION_CONTROL_UNSUPPORTED".into())
            }
            Request::Inspect => match conn.inspect_schema() {
                Ok(schema) => Response::Schema(schema),
                Err(error) => sql_failure(error),
            },
            Request::Close => {
                reset_connection(conn)?;
                write_frame(&mut stream, &Response::Closed)?;
                return Ok(());
            }
            Request::Open { .. } => Response::Error("STORAGE_OWNER_ALREADY_OPEN".into()),
        };
        write_frame(&mut stream, &response)?;
    }
}

#[cfg(unix)]
pub(crate) struct Proxy {
    lease: Rc<Lease>,
}

#[cfg(unix)]
struct Lease {
    stream: RefCell<std::os::unix::net::UnixStream>,
    access_mode: AccessMode,
    caller: Caller,
    transaction_state: Rc<Cell<bool>>,
    broken: Cell<bool>,
}

#[cfg(unix)]
thread_local! {
    // Nested same-thread opens share transaction affinity. Weak entries never
    // extend a lease beyond the final caller-owned facade connection.
    static LEASES: RefCell<HashMap<PathBuf, Weak<Lease>>> = RefCell::new(HashMap::new());
}

#[cfg(unix)]
impl Proxy {
    pub(crate) fn try_connect(
        db_path: &Path,
        access_mode: AccessMode,
        timeout: Duration,
    ) -> Result<Option<Self>> {
        use std::os::unix::net::UnixStream;
        if db_path.file_name().and_then(|name| name.to_str())
            != Some(crate::core::schemas::LOCAL_DB_NAME)
            || !db_path.exists()
        {
            return Ok(None);
        }
        let database = canonical_database(db_path)?;
        let caller = Caller::capture();
        let existing = LEASES.with(|leases| leases.borrow().get(&database).and_then(Weak::upgrade));
        if let Some(lease) = existing {
            // Never inherit another request's transaction or upgrade a read-only
            // lease. The caller must close the earlier handle before changing
            // access policy or identity on the same owner thread.
            if lease.caller != caller || lease.access_mode != access_mode {
                return Err(failure("STORAGE_OWNER_NESTED_CONTEXT_CONFLICT"));
            }
            return Ok(Some(Self { lease }));
        }
        let endpoint = socket_path(&database).map_err(Error::Io)?;
        let mut stream = match UnixStream::connect(&endpoint) {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(Error::Io(error)),
        };
        stream
            .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))
            .map_err(Error::Io)?;
        stream
            .set_write_timeout(Some(timeout.max(Duration::from_millis(1))))
            .map_err(Error::Io)?;
        write_frame(
            &mut stream,
            &Request::Open {
                version: VERSION,
                database: database.clone(),
                read_only: access_mode == AccessMode::ReadOnly,
                caller: caller.clone(),
            },
        )?;
        match read_frame(&mut stream)? {
            Response::Ready { version: VERSION } => {
                let lease = Rc::new(Lease {
                    stream: RefCell::new(stream),
                    access_mode,
                    caller,
                    transaction_state: Rc::new(Cell::new(false)),
                    broken: Cell::new(false),
                });
                LEASES.with(|leases| {
                    let mut leases = leases.borrow_mut();
                    leases.retain(|_, lease| lease.strong_count() > 0);
                    leases.insert(database, Rc::downgrade(&lease));
                });
                Ok(Some(Self { lease }))
            }
            Response::Error(code) => Err(failure(&code)),
            _ => Err(failure("STORAGE_OWNER_PROTOCOL_MISMATCH")),
        }
    }

    pub(crate) fn access_mode(&self) -> AccessMode {
        self.lease.access_mode
    }

    pub(crate) fn transaction_state(&self) -> Rc<Cell<bool>> {
        Rc::clone(&self.lease.transaction_state)
    }

    pub(crate) fn read(&self, sql: &str, params: &[Parameter]) -> Result<Rows> {
        match self.lease.exchange(Request::Read {
            sql: sql.to_owned(),
            params: params.iter().map(Into::into).collect(),
        })? {
            Response::Rows(rows) => Ok(rows),
            _ => Err(failure("STORAGE_OWNER_UNEXPECTED_RESPONSE")),
        }
    }

    pub(crate) fn write(&self, sql: &str, params: &[Parameter]) -> Result<u64> {
        match self.lease.exchange(Request::Write {
            sql: sql.to_owned(),
            params: params.iter().map(Into::into).collect(),
        })? {
            Response::Count(count) => Ok(count),
            _ => Err(failure("STORAGE_OWNER_UNEXPECTED_RESPONSE")),
        }
    }

    pub(crate) fn inspect_schema(&self) -> Result<StoreSchema> {
        match self.lease.exchange(Request::Inspect)? {
            Response::Schema(schema) => Ok(schema),
            _ => Err(failure("STORAGE_OWNER_UNEXPECTED_RESPONSE")),
        }
    }
}

#[cfg(unix)]
impl Lease {
    fn exchange(&self, request: Request) -> Result<Response> {
        if self.broken.get() {
            return Err(failure("STORAGE_OWNER_TRANSPORT_OUTCOME_UNKNOWN"));
        }
        let mut stream = self
            .stream
            .try_borrow_mut()
            .map_err(|_| failure("STORAGE_OWNER_REENTRANT_REQUEST"))?;
        let response = (|| {
            write_frame(&mut *stream, &request)?;
            read_frame(&mut *stream)
        })();
        match response {
            Ok(Response::Error(code)) => Err(failure(&code)),
            Ok(Response::SqlError(error)) => Err(Error::Dactyl(error.into())),
            Ok(value) => Ok(value),
            Err(error) => {
                self.broken.set(true);
                let _ = stream.shutdown(std::net::Shutdown::Both);
                Err(error)
            }
        }
    }
}

#[cfg(unix)]
impl Drop for Lease {
    fn drop(&mut self) {
        // A failed close must not lead to retrying a mutation. Closing the stream
        // also makes the owner roll back any remaining transaction.
        let _ = self.exchange(Request::Close);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    struct TestOwner {
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
        socket: PathBuf,
    }
    impl TestOwner {
        fn start(path: &Path) -> Self {
            let database = path.canonicalize().unwrap();
            let socket = socket_path(&database).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let worker = std::thread::spawn(move || {
                let conn = crate::core::db::db_connect_owner(&database).unwrap();
                let listener = bind_listener(&database).unwrap();
                tx.send(()).unwrap();
                serve_connections(&conn, &database, listener, &thread_stop).unwrap();
            });
            rx.recv_timeout(Duration::from_secs(10)).unwrap();
            Self {
                stop,
                worker: Some(worker),
                socket,
            }
        }
    }
    impl Drop for TestOwner {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.worker.take() {
                worker.join().unwrap();
            }
            let _ = std::fs::remove_file(&self.socket);
        }
    }

    fn setup() -> (tempfile::TempDir, PathBuf, TestOwner) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::core::schemas::LOCAL_DB_NAME);
        drop(crate::core::db::db_connect_owner(&path).unwrap());
        let owner = TestOwner::start(&path);
        (dir, path, owner)
    }

    fn proxy(path: &Path, mode: AccessMode) -> Proxy {
        Proxy::try_connect(path, mode, Duration::from_secs(2))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn connection_local_state_cannot_leak_between_leases() {
        let (_dir, path, _owner) = setup();
        {
            let first = proxy(&path, AccessMode::ReadWrite);
            first
                .write("CREATE TABLE temp.explicit_temp(value TEXT)", &[])
                .unwrap();
            for sql in [
                "CREATE TEMP TABLE leaked(value TEXT)",
                "PRAGMA foreign_keys=OFF",
                "ATTACH ':memory:' AS other",
                "SAVEPOINT nested",
            ] {
                assert!(first.write(sql, &[]).is_err(), "accepted {sql}");
            }
        }
        let second = proxy(&path, AccessMode::ReadOnly);
        assert!(second.read("SELECT * FROM temp.leaked", &[]).is_err());
        assert!(
            second
                .read("SELECT * FROM temp.explicit_temp", &[])
                .is_err()
        );
        assert!(second.read("SELECT last_insert_rowid()", &[]).is_err());
        assert_eq!(second.read("SELECT 1", &[]).unwrap().0[0].values[0], 1);
    }

    #[test]
    fn abandoned_transaction_is_rolled_back_before_next_caller() {
        let (_dir, path, _owner) = setup();
        {
            let first = proxy(&path, AccessMode::ReadWrite);
            first.write("CREATE TABLE items(value TEXT)", &[]).unwrap();
            first.write("BEGIN IMMEDIATE", &[]).unwrap();
            first
                .write("INSERT INTO items VALUES('uncommitted')", &[])
                .unwrap();
            // Simulate transport loss rather than the cooperative Close path.
            first
                .lease
                .stream
                .borrow()
                .shutdown(std::net::Shutdown::Both)
                .unwrap();
        }
        let second = proxy(&path, AccessMode::ReadWrite);
        assert_eq!(
            second.read("SELECT count(*) FROM items", &[]).unwrap().0[0].values[0],
            0
        );
    }

    #[test]
    fn readonly_lease_rejects_writes_and_control_bypass() {
        let (_dir, path, _owner) = setup();
        {
            let writer = proxy(&path, AccessMode::ReadWrite);
            writer
                .write("CREATE TABLE items(value INTEGER)", &[])
                .unwrap();
        }
        let reader = proxy(&path, AccessMode::ReadOnly);
        assert!(reader.write("INSERT INTO items VALUES(1)", &[]).is_err());
        assert!(
            reader
                .read("INSERT INTO items VALUES(1) RETURNING value", &[])
                .is_err()
        );
        assert!(reader.read("PRAGMA query_only=OFF", &[]).is_err());
        assert_eq!(
            reader.read("SELECT count(*) FROM items", &[]).unwrap().0[0].values[0],
            0
        );
    }

    #[test]
    fn same_thread_nested_handles_share_one_lease_and_transaction_state() {
        let (_dir, path, _owner) = setup();
        let first = proxy(&path, AccessMode::ReadWrite);
        let second = proxy(&path, AccessMode::ReadWrite);
        assert!(Rc::ptr_eq(&first.lease, &second.lease));
        assert!(Rc::ptr_eq(
            &first.transaction_state(),
            &second.transaction_state()
        ));
        first.transaction_state().set(true);
        assert!(second.transaction_state().get());
        assert!(Proxy::try_connect(&path, AccessMode::ReadOnly, Duration::from_secs(1)).is_err());
        drop(first);
        assert_eq!(second.read("SELECT 1", &[]).unwrap().0[0].values[0], 1);
    }

    #[test]
    fn concurrent_callers_do_not_interleave_a_live_transaction() {
        let (_dir, path, _owner) = setup();
        let first = proxy(&path, AccessMode::ReadWrite);
        first
            .write("CREATE TABLE items(value INTEGER)", &[])
            .unwrap();
        first.write("BEGIN IMMEDIATE", &[]).unwrap();
        first.write("INSERT INTO items VALUES(7)", &[]).unwrap();
        let other_path = path.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let reader = proxy(&other_path, AccessMode::ReadOnly);
            let result = reader.read("SELECT value FROM items", &[]).unwrap();
            done_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(50)).is_err());
        first.write("COMMIT", &[]).unwrap();
        drop(first);
        let rows = done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(rows.0[0].values[0], 7);
        other.join().unwrap();
    }

    #[test]
    fn graceful_drain_allows_an_established_lease_to_commit() {
        let (_dir, path, owner) = setup();
        let connection = proxy(&path, AccessMode::ReadWrite);
        connection
            .write("CREATE TABLE drained(value INTEGER)", &[])
            .unwrap();
        connection.write("BEGIN IMMEDIATE", &[]).unwrap();
        connection
            .write("INSERT INTO drained VALUES(1)", &[])
            .unwrap();
        // Equivalent to the worker watchdog observing the normal drain byte.
        // The caller already received Ready and must retain its transaction.
        owner.stop.store(true, Ordering::Release);
        connection
            .write("INSERT INTO drained VALUES(2)", &[])
            .unwrap();
        connection.write("COMMIT", &[]).unwrap();
        drop(connection);
        drop(owner);
        let physical = crate::core::db::db_connect_owner(&path).unwrap();
        let count: i64 = physical
            .query_row("SELECT count(*) FROM drained", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn owner_exit_removes_endpoint() {
        let (_dir, path, owner) = setup();
        assert!(socket_path(&path).unwrap().exists());
        drop(owner);
        assert!(!socket_path(&path).unwrap().exists());
        assert!(
            Proxy::try_connect(&path, AccessMode::ReadWrite, Duration::from_secs(1))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn known_sql_errors_preserve_detail_and_category() {
        let (_dir, path, _owner) = setup();
        let connection = proxy(&path, AccessMode::ReadWrite);
        let error = connection
            .read("SELECT * FROM missing_table", &[])
            .err()
            .unwrap();
        assert!(error.to_string().contains("no such table"));
        assert!(matches!(
            error,
            Error::Dactyl(dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Query,
                ..
            })
        ));
        connection
            .write("CREATE TABLE unique_values(value TEXT UNIQUE)", &[])
            .unwrap();
        connection
            .write("INSERT INTO unique_values VALUES('one')", &[])
            .unwrap();
        let error = connection
            .write("INSERT INTO unique_values VALUES('one')", &[])
            .err()
            .unwrap();
        assert!(matches!(
            error,
            Error::Dactyl(dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Constraint,
                ..
            })
        ));
    }

    #[test]
    fn unsupported_savepoint_controls_cannot_leak_transactions() {
        let (_dir, path, _owner) = setup();
        {
            let connection = proxy(&path, AccessMode::ReadWrite);
            connection
                .write("CREATE TABLE items(value INTEGER)", &[])
                .unwrap();
            assert!(connection.write("SAVEPOINT isolated", &[]).is_err());
            assert!(
                connection
                    .write("/* hidden */ SAVEPOINT isolated", &[])
                    .is_err()
            );
            assert!(connection.write("/* hidden */ BEGIN", &[]).is_err());
            assert!(connection.write("-- hidden\nBEGIN", &[]).is_err());
            assert!(connection.write("BEGIN; SAVEPOINT isolated", &[]).is_err());
            connection.write("BEGIN IMMEDIATE", &[]).unwrap();
            connection
                .write("INSERT INTO items VALUES(9)", &[])
                .unwrap();
            assert!(connection.write("ROLLBACK TO isolated", &[]).is_err());
            assert!(
                connection
                    .write("ROLLBACK TRANSACTION TO isolated", &[])
                    .is_err()
            );
            assert!(connection.write("RELEASE isolated", &[]).is_err());
        }
        let next = proxy(&path, AccessMode::ReadWrite);
        assert_eq!(
            next.read("SELECT count(*) FROM items", &[]).unwrap().0[0].values[0],
            0
        );
        next.write("BEGIN IMMEDIATE", &[]).unwrap();
        next.write("INSERT INTO items VALUES(1)", &[]).unwrap();
        next.write("COMMIT", &[]).unwrap();
    }

    #[test]
    fn wire_parameters_preserve_type_and_bits() {
        for value in [
            Parameter::Integer(4),
            Parameter::Real(4.0),
            Parameter::Real(f64::NAN),
            Parameter::Blob(vec![0, 255]),
        ] {
            let encoded = serde_json::to_vec(&WireParameter::from(&value)).unwrap();
            let decoded: Parameter = serde_json::from_slice::<WireParameter>(&encoded)
                .unwrap()
                .into();
            match (value, decoded) {
                (Parameter::Real(a), Parameter::Real(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                (a, b) => assert_eq!(a, b),
            }
        }
    }
}

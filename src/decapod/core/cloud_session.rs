//! Private custody for the Dactyl cloud connector.
//!
//! The service remains the authority for identity, access and readiness. This
//! file only serializes a caller's credentials and recoverable progress. Its
//! JSON representation is secret and must never be copied into Git or logs.

use crate::core::error::DecapodError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const RECORD_VERSION: u16 = 1;
const MAX_RECORD_BYTES: u64 = 64 * 1024;
const MAX_LOCK_WAIT: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Empty,
    Preparing,
    Waiting,
    ExchangingBootstrap,
    HaveCode,
    ExchangingSession,
    CheckingReadiness,
    Ready,
    Refreshing,
    RestartRequired,
}

impl Phase {
    fn may_have_consumed_a_secret(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::ExchangingBootstrap
                | Self::ExchangingSession
                | Self::Refreshing
        )
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("access_token", &"[REDACTED]")
            .field("has_refresh_token", &self.refresh_token.is_some())
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub flow_id: Option<String>,
    pub browser_url: Option<String>,
    pub expires_at: Option<i64>,
    pub pkce_verifier: String,
    pub poll_secret: String,
    pub code: Option<String>,
}

impl std::fmt::Debug for Bootstrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bootstrap")
            .field("has_flow", &self.flow_id.is_some())
            .field("has_browser_url", &self.browser_url.is_some())
            .field("expires_at", &self.expires_at)
            .field("pkce_verifier", &"[REDACTED]")
            .field("poll_secret", &"[REDACTED]")
            .field("has_code", &self.code.is_some())
            .finish()
    }
}

impl Bootstrap {
    /// Persist this material in `Preparing` before sending the start request.
    pub fn new() -> Result<Self, DecapodError> {
        Ok(Self {
            flow_id: None,
            browser_url: None,
            expires_at: None,
            pkce_verifier: random_secret()?,
            poll_secret: random_secret()?,
            code: None,
        })
    }

    pub fn pkce_challenge(&self) -> String {
        base64url(&Sha256::digest(self.pkce_verifier.as_bytes()))
    }

    pub fn poll_challenge(&self) -> String {
        base64url(&Sha256::digest(self.poll_secret.as_bytes()))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionState {
    pub phase: Phase,
    pub session: Option<Session>,
    pub bootstrap: Option<Bootstrap>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    origin: String,
    repository: String,
    project: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredRecord {
    version: u16,
    binding: Binding,
    state: SessionState,
}

#[derive(Clone)]
pub struct SessionStore {
    directory: PathBuf,
    binding: Binding,
    project_root: PathBuf,
    main_root: PathBuf,
}

impl SessionStore {
    /// `origin` must be the canonical service origin returned by Dactyl. The
    /// repository is a routing assertion, never evidence of authorization.
    /// Sibling worktrees share the main repository's private custody scope.
    pub fn for_project(
        origin: &str,
        repository: &str,
        project_root: &Path,
    ) -> Result<Self, DecapodError> {
        Self::for_project_in(&machine_data_dir()?, origin, repository, project_root)
    }

    fn for_project_in(
        machine_directory: &Path,
        origin: &str,
        repository: &str,
        project_root: &Path,
    ) -> Result<Self, DecapodError> {
        if !machine_directory.is_absolute()
            || machine_directory
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
            || !bounded_text(origin, 2048)
            || !bounded_text(repository, 256)
            || origin.chars().any(char::is_whitespace)
            || repository.chars().any(char::is_whitespace)
        {
            return Err(invalid(
                "cloud session scope or machine directory is invalid",
            ));
        }
        let project_root = fs::canonicalize(project_root)?;
        // The existing bounded Git root resolver requires a UTF-8 argument.
        // Reject unsupported paths instead of resolving an unrelated cwd.
        if project_root.to_str().is_none() {
            return Err(invalid(
                "cloud session custody requires a UTF-8 project path",
            ));
        }
        let main_root =
            fs::canonicalize(crate::core::workspace::get_main_repo_root(&project_root)?)?;
        let binding = Binding {
            origin: origin.to_owned(),
            // The managed service binds and returns this routing assertion
            // verbatim. Do not merge scopes the transport treats as distinct.
            repository: repository.to_owned(),
            project: format!(
                "{:x}",
                Sha256::digest(main_root.as_os_str().as_encoded_bytes())
            ),
        };
        let scope = serde_json::to_vec(&(RECORD_VERSION, &binding))
            .map_err(|_| invalid("cloud session scope could not be encoded"))?;
        let directory = machine_directory
            .join("cloud-sessions")
            .join(format!("{:x}", Sha256::digest(scope)));
        reject_repository_storage(&directory, &project_root, &main_root)?;
        Ok(Self {
            directory,
            binding,
            project_root,
            main_root,
        })
    }

    /// The stable sidecar inode stays locked across load, network mutation and
    /// durable save. Never lock the replaceable JSON inode or remove a lock file.
    pub fn lock(&self, timeout: Duration) -> Result<SessionGuard, DecapodError> {
        reject_repository_storage(&self.directory, &self.project_root, &self.main_root)?;
        crate::core::fs_permissions::ensure_private_dir(&self.directory)?;
        check_private_directory(&self.directory)?;
        let file = open_secret_file(
            &self.directory.join("session.lock"),
            OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false),
        )?;
        let timeout = timeout.min(MAX_LOCK_WAIT);
        let start = Instant::now();
        loop {
            match FileExt::try_lock_exclusive(&file) {
                Ok(()) => break,
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) && start.elapsed() < timeout =>
                {
                    std::thread::sleep(
                        Duration::from_millis(10).min(timeout.saturating_sub(start.elapsed())),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(invalid(
                        "cloud session custody is busy; retry after the other invocation finishes",
                    ));
                }
                Err(_) => return Err(invalid("cloud session custody lock is unavailable")),
            }
        }
        let state = read_record(&self.directory.join("state.json"), &self.binding)?;
        let mut guard = SessionGuard {
            file,
            store: self.clone(),
            state,
        };
        if guard.state.phase.may_have_consumed_a_secret() {
            // The prior invocation ended without recording the response. None
            // of these service mutations has a replay/recovery contract.
            let mut recovered = guard.state.clone();
            recovered.phase = Phase::RestartRequired;
            guard.replace(recovered)?;
        }
        Ok(guard)
    }
}

pub struct SessionGuard {
    file: File,
    store: SessionStore,
    state: SessionState,
}

impl std::fmt::Debug for SessionGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionGuard")
            .field("state", &self.state)
            .finish()
    }
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl SessionGuard {
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    /// A durability failure can leave the new complete record visible. Return
    /// immediately on error and reconcile by reacquiring; never send the next
    /// network mutation after a failed intent or response checkpoint.
    pub fn replace(&mut self, state: SessionState) -> Result<(), DecapodError> {
        validate_state(&state)?;
        let path = self.store.directory.join("state.json");
        match open_secret_file(&path, OpenOptions::new().read(true)) {
            Ok(_) => (),
            Err(DecapodError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
        let record = StoredRecord {
            version: RECORD_VERSION,
            binding: self.store.binding.clone(),
            state: state.clone(),
        };
        let bytes = serde_json::to_vec(&record)
            .map_err(|_| invalid("cloud session state could not be encoded"))?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err(invalid("cloud session state exceeds its bounded size"));
        }
        crate::core::atomic::write_atomic(&path, &bytes)?;
        self.state = state;
        Ok(())
    }

    /// The caller must require an explicit restart action before discarding
    /// uncertain progress. Resetting custody never revokes a service session.
    pub fn clear_for_restart(&mut self) -> Result<(), DecapodError> {
        self.replace(SessionState::default())
    }
}

fn machine_data_dir() -> Result<PathBuf, DecapodError> {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path).join("decapod"));
    }
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .ok_or_else(|| invalid("a machine data directory is required for cloud session custody"))?;
    Ok(PathBuf::from(home).join(".local/share/decapod"))
}

fn reject_repository_storage(path: &Path, project: &Path, main: &Path) -> Result<(), DecapodError> {
    let mut existing = path;
    let mut suffix = Vec::new();
    loop {
        match fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    existing
                        .file_name()
                        .ok_or_else(|| invalid("invalid cloud custody path"))?
                        .to_owned(),
                );
                existing = existing
                    .parent()
                    .ok_or_else(|| invalid("invalid cloud custody ancestry"))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let mut resolved = fs::canonicalize(existing)?;
    for component in suffix.into_iter().rev() {
        resolved.push(component);
    }
    if resolved.starts_with(project) || resolved.starts_with(main) {
        return Err(invalid(
            "cloud credentials must be stored outside the project and its Git worktrees",
        ));
    }
    for candidate in [path, resolved.as_path()] {
        for ancestor in candidate.ancestors() {
            if ancestor.file_name().is_some_and(|name| name == ".git") {
                return Err(invalid("cloud credentials must be stored outside Git"));
            }
            match fs::symlink_metadata(ancestor.join(".git")) {
                Ok(_) => {
                    return Err(invalid(
                        "cloud credentials must be stored outside every Git worktree",
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

fn check_private_directory(path: &Path) -> Result<(), DecapodError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Err(invalid(
            "cloud custody directory must be a real private directory",
        ));
    }
    check_secret_permissions(&metadata)
}

fn check_secret_permissions(metadata: &fs::Metadata) -> Result<(), DecapodError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
            return Err(invalid(
                "cloud custody requires owner-only permissions; existing permissions were not changed",
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = metadata; // Windows retains the existing user ACL boundary.
    Ok(())
}

fn open_secret_file(path: &Path, options: &mut OpenOptions) -> Result<File, DecapodError> {
    crate::core::fs_permissions::check_file(path, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("cloud custody requires a regular private file"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(invalid(
                "cloud custody does not accept files with multiple links",
            ));
        }
    }
    check_secret_permissions(&metadata)?;
    Ok(file)
}

fn read_record(path: &Path, binding: &Binding) -> Result<SessionState, DecapodError> {
    let file = match open_secret_file(path, OpenOptions::new().read(true)) {
        Ok(file) => file,
        Err(DecapodError::IoError(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SessionState::default());
        }
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err(invalid("cloud session state exceeds its bounded size"));
    }
    let record: StoredRecord = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("cloud session state is invalid; preserve it for review"))?;
    if record.version != RECORD_VERSION || record.binding != *binding {
        return Err(invalid(
            "cloud session binding or record version does not match this project",
        ));
    }
    validate_state(&record.state)?;
    Ok(record.state)
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

fn secret(value: &str, maximum: usize) -> bool {
    bounded_text(value, maximum) && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn challenge_secret(value: &str) -> bool {
    value.len() == 43 && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        // A 32-byte unpadded base64url value ends in two zero padding bits.
        && value.as_bytes().last().is_some_and(|last| b"AEIMQUYcgkosw048".contains(last))
}

fn validate_state(state: &SessionState) -> Result<(), DecapodError> {
    let malformed = || invalid("cloud session state has invalid or incomplete bounded fields");
    if let Some(session) = &state.session
        && (!secret(&session.access_token, 16 * 1024)
            || session
                .refresh_token
                .as_deref()
                .is_some_and(|token| !secret(token, 4096))
            || session.expires_at <= 0)
    {
        return Err(malformed());
    }
    if let Some(bootstrap) = &state.bootstrap
        && (!challenge_secret(&bootstrap.pkce_verifier)
            || !challenge_secret(&bootstrap.poll_secret)
            || bootstrap
                .flow_id
                .as_deref()
                .is_some_and(|value| !secret(value, 128))
            || bootstrap
                .browser_url
                .as_deref()
                .is_some_and(|value| !bounded_text(value, 8192))
            || bootstrap.expires_at.is_some_and(|value| value <= 0)
            || bootstrap
                .code
                .as_deref()
                .is_some_and(|value| value.len() < 16 || !secret(value, 2048)))
    {
        return Err(malformed());
    }
    let handoff = state.bootstrap.as_ref().is_some_and(|bootstrap| {
        bootstrap.flow_id.is_some()
            && bootstrap.browser_url.is_some()
            && bootstrap.expires_at.is_some()
    });
    let code = state
        .bootstrap
        .as_ref()
        .is_some_and(|bootstrap| bootstrap.code.is_some());
    let prepared = state.bootstrap.as_ref().is_some_and(|bootstrap| {
        bootstrap.flow_id.is_none()
            && bootstrap.browser_url.is_none()
            && bootstrap.expires_at.is_none()
            && bootstrap.code.is_none()
    });
    let valid = match state.phase {
        Phase::Empty => state.session.is_none() && state.bootstrap.is_none(),
        Phase::Preparing => state.session.is_none() && prepared,
        Phase::Waiting | Phase::ExchangingBootstrap => state.session.is_none() && handoff && !code,
        Phase::HaveCode | Phase::ExchangingSession => state.session.is_none() && handoff && code,
        Phase::CheckingReadiness => state.session.is_some() && handoff && !code,
        Phase::Ready => state.session.is_some() && state.bootstrap.is_none(),
        Phase::Refreshing => {
            state
                .session
                .as_ref()
                .is_some_and(|session| session.refresh_token.is_some())
                && (state.bootstrap.is_none() || (handoff && !code))
        }
        Phase::RestartRequired => state.bootstrap.is_none() || prepared || handoff,
    };
    if valid { Ok(()) } else { Err(malformed()) }
}

fn random_secret() -> Result<String, DecapodError> {
    let mut bytes = [0_u8; 32];
    File::open("/dev/urandom")
        .and_then(|mut input| input.read_exact(&mut bytes))
        .map_err(|_| invalid("operating-system randomness is unavailable for cloud sign-in"))?;
    Ok(base64url(&bytes))
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut output = String::with_capacity((bytes.len() * 4).div_ceil(3));
    for chunk in bytes.chunks(3) {
        let word = ((chunk[0] as u32) << 16)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 8)
            | chunk.get(2).copied().unwrap_or(0) as u32;
        output.push(ALPHABET[((word >> 18) & 63) as usize] as char);
        output.push(ALPHABET[((word >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[((word >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(word & 63) as usize] as char);
        }
    }
    output
}

fn invalid(message: &str) -> DecapodError {
    DecapodError::SessionError(message.to_owned())
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_session_tests.rs"]
mod tests;

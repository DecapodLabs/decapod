//! AI-facing session orchestration. All backend/auth I/O belongs to Dactyl.

use std::fs::{self, File, OpenOptions};
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dactyl_db::{AuthenticationSecrets, AuthenticationStart, CloudConnector, UserSession};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::auth::{CloudCredential, CloudSessionStore, CredentialSource};
use crate::core::cloud_backend::CloudSession;
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus};
use crate::core::propodus::PropodusClientError;

const MAX_PRIVATE_STATE_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
struct Pending {
    version: u16,
    origin: String,
    repository: String,
    secrets: AuthenticationSecrets,
    flow: AuthenticationStart,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    session: Option<CloudSession>,
    #[serde(default)]
    provider_token: Option<String>,
    #[serde(default)]
    exchange_inflight: bool,
    #[serde(default)]
    token_inflight: bool,
}

fn error(status: CloudAuthStatus, message: &str) -> PropodusClientError {
    PropodusClientError::Authentication(CloudAuthDiagnostic::new(
        status,
        message,
        "Agent: retry the original command to resume the saved connection. Ask the user only for the browser action shown; do not ask them to run CLI commands or repeat init.",
    ))
}

fn custody<T>(result: Result<T, impl std::fmt::Display>) -> Result<T, PropodusClientError> {
    result.map_err(|_| {
        error(
            CloudAuthStatus::AuthRequired,
            "private cloud session state is unavailable",
        )
    })
}

fn backend<T>(result: Result<T, dactyl_db::DactylError>) -> Result<T, PropodusClientError> {
    result.map_err(|e| {
        let status = match e.adapter_kind() {
            Some(
                dactyl_db::AdapterErrorKind::Unavailable
                | dactyl_db::AdapterErrorKind::Timeout
                | dactyl_db::AdapterErrorKind::Transport,
            ) => CloudAuthStatus::Offline,
            _ => CloudAuthStatus::AuthRequired,
        };
        error(status, &format!("cloud connector: {e}"))
    })
}

fn save(path: &Path, value: &impl Serialize) -> Result<(), PropodusClientError> {
    if let Some(parent) = path.parent() {
        custody(crate::core::fs_permissions::ensure_private_dir(parent))?;
    }
    custody(crate::core::fs_permissions::check_file(path, false))?;
    let bytes = custody(serde_json::to_vec(value))?;
    if bytes.len() > MAX_PRIVATE_STATE_BYTES {
        return Err(error(
            CloudAuthStatus::AuthRequired,
            "private cloud state exceeds its bound",
        ));
    }
    custody(crate::core::atomic::write_atomic(path, &bytes))
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, PropodusClientError> {
    match fs::symlink_metadata(path) {
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return custody(Err::<Option<T>, _>(error)),
    }
    let file = custody(crate::core::fs_permissions::open_private_file(
        path,
        OpenOptions::new().read(true),
    ))?;
    let mut bytes = Vec::new();
    custody(
        file.take((MAX_PRIVATE_STATE_BYTES + 1) as u64)
            .read_to_end(&mut bytes),
    )?;
    if bytes.len() > MAX_PRIVATE_STATE_BYTES {
        return Err(error(
            CloudAuthStatus::AuthRequired,
            "private cloud state exceeds its bound",
        ));
    }
    custody(serde_json::from_slice(&bytes)).map(Some)
}

fn pending_path(directory: &Path, repository: &str) -> PathBuf {
    directory.join(format!(
        "onboarding-{:x}.json",
        Sha256::digest(repository.to_ascii_lowercase().as_bytes())
    ))
}

fn preserve_uncertain_pending(path: &Path) -> Result<(), PropodusClientError> {
    let stamp = Utc::now().timestamp_nanos_opt().ok_or_else(|| {
        error(
            CloudAuthStatus::AuthRequired,
            "could not preserve uncertain sign-in state",
        )
    })?;
    let abandoned = path.with_extension(format!("uncertain-{}-{stamp}", std::process::id()));
    custody(fs::rename(path, abandoned))?;
    custody(crate::core::atomic::sync_parent_directory(
        path.parent().unwrap_or_else(|| Path::new(".")),
    ))
}

fn lock(directory: &Path) -> Result<File, PropodusClientError> {
    custody(crate::core::fs_permissions::ensure_private_dir(directory))?;
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = custody(crate::core::fs_permissions::open_private_file(
        &directory.join("session.lock"),
        &mut options,
    ))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(_) => {
                return Err(error(
                    CloudAuthStatus::AuthRequired,
                    "another cloud session operation is still in progress",
                ));
            }
        }
    }
}

fn usable(credential: &CloudCredential) -> bool {
    if credential.source != CredentialSource::MachineFile {
        return true;
    }
    credential
        .expires_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .is_some_and(|expiry| {
            expiry.with_timezone(&Utc) > Utc::now() + chrono::Duration::seconds(60)
        })
}

/// Remember a definitive API rejection without retrying any data operation.
/// A late rejection of an older bearer cannot invalidate a concurrently renewed
/// session. Explicit operator credentials never mutate the saved session.
pub(crate) fn reject_bearer(
    origin: &str,
    repository: &str,
    bearer: &str,
) -> Result<(), PropodusClientError> {
    if std::env::var_os(crate::core::auth::CLOUD_ACCESS_TOKEN_ENV).is_some() {
        return Ok(());
    }
    let store = custody(CloudSessionStore::for_repository(origin, repository))?;
    reject_bearer_in_store(&store, bearer)
}

fn reject_bearer_in_store(
    store: &CloudSessionStore,
    bearer: &str,
) -> Result<(), PropodusClientError> {
    if !custody(store.load_machine_session())?.is_some_and(|saved| saved.token == bearer) {
        return Ok(());
    }
    let directory = custody(store.scope_directory())?;
    let _lock = lock(directory)?;
    if custody(store.load_machine_session())?.is_some_and(|saved| saved.token == bearer) {
        save(
            &directory.join("rejected-bearer.json"),
            &format!("{:x}", Sha256::digest(bearer.as_bytes())),
        )?;
    }
    Ok(())
}

fn session(value: UserSession) -> Result<CloudSession, PropodusClientError> {
    let expiry = i64::try_from(value.expires_at_unix)
        .ok()
        .and_then(|n| DateTime::<Utc>::from_timestamp(n, 0))
        .ok_or_else(|| {
            error(
                CloudAuthStatus::AuthRequired,
                "cloud connector returned an invalid session expiry",
            )
        })?;
    Ok(CloudSession {
        access_token: value.access_token,
        refresh_token: Some(value.refresh_token),
        session_id: None,
        expires_at: Some(expiry.to_rfc3339()),
    })
}

/// Reuse a session or coordinate sign-in through the selected Dactyl connector.
/// A per-repository lock serializes refresh rotation across agents/processes.
pub(crate) fn ensure(
    origin: &str,
    repository: &str,
) -> Result<CloudCredential, PropodusClientError> {
    backend(CloudConnector::validate_origin(origin))?;
    let store = custody(CloudSessionStore::for_repository(origin, repository))?;
    let interactive = std::io::stdin().is_terminal()
        && std::env::var_os("DECAPOD_HEADLESS").is_none()
        && std::env::var_os("GITHUB_ACTIONS").is_none();
    let explicit = if std::env::var_os(crate::core::auth::CLOUD_ACCESS_TOKEN_ENV).is_some() {
        Some(custody(store.load_credential(None))?)
    } else {
        None
    };
    ensure_with(origin, repository, &store, interactive, explicit, || {
        CloudConnector::from_runtime(origin)
    })
}

fn ensure_with(
    origin: &str,
    repository: &str,
    store: &CloudSessionStore,
    interactive: bool,
    explicit: Option<CloudCredential>,
    connector: impl Fn() -> Result<CloudConnector, dactyl_db::DactylError>,
) -> Result<CloudCredential, PropodusClientError> {
    // A caller-supplied session takes precedence over saved browser-flow state.
    // It must neither replay an interrupted sign-in nor replace private records.
    if let Some(credential) = explicit {
        return Ok(credential);
    }
    let directory = custody(store.scope_directory())?;
    let _lock = lock(directory)?;
    let pending_file = pending_path(directory, repository);
    let refresh_file = directory.join("refresh-inflight.json");
    let rejected_file = directory.join("rejected-bearer.json");
    let mut pending: Option<Pending> = read(&pending_file)?;
    if let Some(p) = &pending {
        if p.version != 1
            || p.origin.trim_end_matches('/') != origin.trim_end_matches('/')
            || !p.repository.eq_ignore_ascii_case(repository)
        {
            return Err(error(
                CloudAuthStatus::AuthRequired,
                "pending sign-in is bound to a different cloud scope",
            ));
        }
        if p.exchange_inflight || (p.token_inflight && p.session.is_none()) {
            // An expired browser flow does not establish the outcome of an
            // already-issued one-time request. Preserve it before expiry cleanup
            // and without even rediscovering the connector.
            preserve_uncertain_pending(&pending_file)?;
            return Err(error(
                CloudAuthStatus::AuthRequired,
                "a one-time authentication response was lost; restart sign-in",
            ));
        }
        let expired = DateTime::parse_from_rfc3339(&p.flow.expires_at)
            .map_err(|_| {
                error(
                    CloudAuthStatus::AuthRequired,
                    "pending sign-in has an invalid expiry",
                )
            })?
            .with_timezone(&Utc)
            <= Utc::now();
        if expired {
            custody(fs::remove_file(&pending_file))?;
            pending = None;
        }
    }
    if pending.is_none() {
        let credential = custody(store.load_machine_session())?;
        if let Some(credential) = credential {
            let rejected: Option<String> = read(&rejected_file)?;
            let fingerprint = format!("{:x}", Sha256::digest(credential.token.as_bytes()));
            if usable(&credential) && rejected.as_deref() != Some(fingerprint.as_str()) {
                return Ok(credential);
            }
            if let Some(refresh) = &credential.refresh_token {
                let fingerprint = format!("{:x}", Sha256::digest(refresh.as_bytes()));
                let previous: Option<String> = read(&refresh_file)?;
                if previous.as_deref() != Some(fingerprint.as_str()) {
                    let connector = backend(connector())?;
                    // A crash/lost response must never replay this rotating token.
                    save(&refresh_file, &fingerprint)?;
                    match connector.refresh_session(refresh) {
                        Ok(refreshed) => {
                            let provider_token = refreshed.provider_token.clone();
                            let updated = session(refreshed)?;
                            custody(store.store_managed_session(
                                &updated,
                                provider_token.as_deref(),
                                true,
                            ))?;
                            custody(fs::remove_file(&refresh_file))?;
                            if rejected_file.exists() {
                                custody(fs::remove_file(&rejected_file))?;
                            }
                            return custody(store.load_machine_session())?.ok_or_else(|| {
                                error(
                                    CloudAuthStatus::AuthRequired,
                                    "refreshed session was not persisted",
                                )
                            });
                        }
                        Err(e)
                            if e.adapter_kind()
                                == Some(dactyl_db::AdapterErrorKind::Authentication) =>
                        {
                            // Definitive expiry/revocation needs a new sign-in,
                            // which this same agent invocation coordinates.
                        }
                        Err(e) => return backend(Err(e)),
                    }
                }
            }
        }
    }
    let connector = backend(connector())?;
    let had_pending = pending.is_some();
    let mut pending = match pending {
        Some(value) => value,
        None => {
            let secrets = backend(AuthenticationSecrets::generate())?;
            let flow = backend(connector.start_authentication(repository, &secrets))?;
            let value = Pending {
                version: 1,
                origin: origin.into(),
                repository: repository.into(),
                secrets,
                flow,
                code: None,
                session: None,
                provider_token: None,
                exchange_inflight: false,
                token_inflight: false,
            };
            save(&pending_file, &value)?;
            value
        }
    };
    backend(connector.validate_start(&pending.flow))?;
    if pending.session.is_none() && pending.code.is_none() {
        if interactive {
            eprintln!(
                "Sign in to connect this repository: {}",
                pending.flow.browser_url
            );
        }
        if interactive && !had_pending {
            crate::core::propodus::try_open_browser(&pending.flow.browser_url);
        }
        let deadline = Instant::now() + Duration::from_secs(if interactive { 120 } else { 0 });
        loop {
            let status =
                backend(connector.authentication_status(&pending.flow.flow_id, &pending.secrets))?;
            if status == "ready" {
                break;
            }
            if status != "pending" {
                // The server consumed this flow, but no local code/session can
                // prove its result. Preserve that uncertainty so the next run
                // can create a fresh flow instead of polling the old one forever.
                preserve_uncertain_pending(&pending_file)?;
                return Err(error(
                    CloudAuthStatus::AuthRequired,
                    "the sign-in code is no longer available; restart sign-in",
                ));
            }
            if Instant::now() >= deadline {
                return Err(PropodusClientError::Authentication(
                    CloudAuthDiagnostic::new(
                        CloudAuthStatus::OnboardingPending,
                        "Sign in once to connect this repository. Your agent will take it from here.",
                        "Agent: share authorization_url, then retry the original command every 2 seconds for up to 5 minutes. Continue as soon as sign-in completes, without waiting for another user message. Preserve the pending flow if still incomplete. The user does not need another CLI command.",
                    ).with_authorization_url(&pending.flow.browser_url),
                ));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        pending.exchange_inflight = true;
        save(&pending_file, &pending)?;
        pending.code = Some(backend(
            connector.exchange_authentication_code(&pending.flow.flow_id, &pending.secrets),
        )?);
        pending.exchange_inflight = false;
        save(&pending_file, &pending)?;
    }
    if pending.session.is_none() {
        pending.token_inflight = true;
        save(&pending_file, &pending)?;
        let credentials = backend(connector.exchange_session(
            pending.code.as_deref().ok_or_else(|| {
                error(CloudAuthStatus::AuthRequired, "pending sign-in has no code")
            })?,
            &pending.secrets,
        ))?;
        pending.provider_token = credentials.provider_token.clone();
        pending.session = Some(session(credentials)?);
        pending.token_inflight = false;
        pending.code = None;
        save(&pending_file, &pending)?;
    }
    let session = pending
        .session
        .as_ref()
        .ok_or_else(|| error(CloudAuthStatus::AuthRequired, "sign-in returned no session"))?;
    backend(connector.complete_authentication(
        &pending.flow.flow_id,
        &pending.secrets,
        &session.access_token,
    ))?;
    // A browser login does not establish repository access. Publish the usable
    // session only after the backend confirms this repository's authorization.
    custody(store.store_managed_session(session, pending.provider_token.as_deref(), false))?;
    custody(fs::remove_file(pending_file))?;
    if refresh_file.exists() {
        custody(fs::remove_file(refresh_file))?;
    }
    if rejected_file.exists() {
        custody(fs::remove_file(rejected_file))?;
    }
    custody(store.load_machine_session())?.ok_or_else(|| {
        error(
            CloudAuthStatus::AuthRequired,
            "managed session was not persisted",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Barrier, mpsc};
    use std::thread;

    const REPOSITORY: &str = "owner/repository";
    const FIRST_FLOW: &str = "00000000-0000-4000-8000-000000000001";
    const SECOND_FLOW: &str = "00000000-0000-4000-8000-000000000002";
    const PREFIX: &str = "/functions/v1/propodus-onboarding";

    struct Request {
        path: String,
        headers: BTreeMap<String, String>,
        body: Value,
    }

    struct Fixture {
        origin: String,
        received: mpsc::Receiver<Request>,
        handle: thread::JoinHandle<()>,
    }

    fn receive(stream: &mut TcpStream) -> Request {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let (headers, offset, length) = loop {
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "HTTP request ended before its headers");
            bytes.extend_from_slice(&buffer[..count]);
            assert!(bytes.len() < 32768, "unexpectedly large fixture request");
            if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                break (headers, end + 4, length);
            }
        };
        while bytes.len() - offset < length {
            let mut buffer = [0; 4096];
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "HTTP request ended before its body");
            bytes.extend_from_slice(&buffer[..count]);
        }
        let mut lines = headers.lines();
        let path = lines
            .next()
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_owned();
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.to_ascii_lowercase(), value.trim().to_owned()))
            })
            .collect();
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[offset..offset + length]).unwrap()
        };
        Request {
            path,
            headers,
            body,
        }
    }

    /// A bounded, synthetic HTTP fixture. It never executes SQL or hosted Auth.
    fn fixture(responses: impl FnOnce(&str) -> Vec<(&'static str, Value)>) -> Fixture {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let responses = responses(&origin);
        let (sender, received) = mpsc::channel();
        let handle = thread::spawn(move || {
            for (expected, response) in responses {
                let deadline = Instant::now() + Duration::from_secs(10);
                let (mut stream, _) = loop {
                    match listener.accept() {
                        Ok(stream) => break stream,
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                && Instant::now() < deadline =>
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => {
                            panic!("expected {expected}, but no request arrived: {error}")
                        }
                    }
                };
                let request = receive(&mut stream);
                assert_eq!(request.path, expected);
                sender.send(request).unwrap();
                let body = response.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            let deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < deadline {
                assert!(
                    listener.accept().is_err(),
                    "authentication request was unexpectedly replayed"
                );
                thread::sleep(Duration::from_millis(5));
            }
        });
        Fixture {
            origin,
            received,
            handle,
        }
    }

    fn connector(origin: &str) -> CloudConnector {
        CloudConnector::with_config(dactyl_db::SupabaseRpcConfig::v1(
            origin,
            "sb_publishable_fixture",
        ))
        .unwrap()
    }

    fn flow(origin: &str, id: &str) -> AuthenticationStart {
        AuthenticationStart {
            flow_id: id.to_owned(),
            browser_url: format!("{origin}{PREFIX}/begin?flow_id={id}&ticket=browser-ticket"),
            expires_at: (Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        }
    }

    fn pending(origin: &str) -> Pending {
        Pending {
            version: 1,
            origin: origin.to_owned(),
            repository: REPOSITORY.to_owned(),
            secrets: AuthenticationSecrets::generate().unwrap(),
            flow: flow(origin, FIRST_FLOW),
            code: None,
            session: None,
            provider_token: None,
            exchange_inflight: false,
            token_inflight: false,
        }
    }

    fn assert_status(
        result: Result<CloudCredential, PropodusClientError>,
        status: CloudAuthStatus,
    ) {
        match result {
            Err(PropodusClientError::Authentication(diagnostic)) => {
                assert_eq!(diagnostic.status, status)
            }
            other => panic!("unexpected session result: {other:?}"),
        }
    }

    fn assert_private(path: &Path) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o077, 0);
        }
        #[cfg(not(unix))]
        let _ = path;
    }

    fn uncertain_records(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".uncertain-")
            })
            .collect()
    }

    #[test]
    fn pending_scopes_do_not_collide_for_distinct_repository_names() {
        let directory = Path::new("/private/session");
        assert_ne!(
            pending_path(directory, "owner/a"),
            pending_path(directory, "owner/b")
        );
        assert_eq!(pending_path(directory, "owner/a").parent(), Some(directory));
    }

    #[test]
    fn managed_short_lifetime_does_not_force_immediate_refresh() {
        let mut credential = CloudCredential {
            token: "opaque".into(),
            source: CredentialSource::MachineFile,
            refresh_token: Some("refresh".into()),
            session_id: None,
            expires_at: Some((Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()),
        };
        assert!(usable(&credential));
        credential.expires_at = Some((Utc::now() + chrono::Duration::seconds(30)).to_rfc3339());
        assert!(!usable(&credential));
        credential.expires_at = Some("invalid".into());
        assert!(!usable(&credential));
    }

    #[test]
    fn late_rejection_of_an_old_bearer_preserves_the_new_session() {
        let root = tempfile::tempdir().unwrap();
        let store =
            CloudSessionStore::for_repository_in(root.path(), "https://example.test", REPOSITORY)
                .unwrap();
        let directory = store.scope_directory().unwrap();
        reject_bearer_in_store(&store, "unpersisted-explicit-token").unwrap();
        assert!(
            !directory.exists(),
            "a caller-supplied bearer must not create a saved session"
        );
        store
            .store_machine_session(&CloudSession {
                access_token: "new-valid-access".into(),
                refresh_token: Some("new-valid-refresh".into()),
                session_id: None,
                expires_at: Some((Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()),
            })
            .unwrap();
        reject_bearer_in_store(&store, "old-rejected-access").unwrap();
        assert!(!directory.join("rejected-bearer.json").exists());
        assert_eq!(
            ensure_with(
                "https://example.test",
                REPOSITORY,
                &store,
                false,
                None,
                || panic!("the new session must remain reusable")
            )
            .unwrap()
            .token,
            "new-valid-access"
        );
    }

    #[test]
    fn supplied_session_takes_precedence_without_replaying_or_replacing_pending_state() {
        let origin = "http://127.0.0.1:9";
        let root = tempfile::tempdir().unwrap();
        let store = CloudSessionStore::for_repository_in(root.path(), origin, REPOSITORY).unwrap();
        let path = pending_path(store.scope_directory().unwrap(), REPOSITORY);
        save(&path, &json!({"interrupted":"private state"})).unwrap();
        let before = fs::read(&path).unwrap();
        let explicit = CloudCredential {
            token: "supplied-session".into(),
            source: CredentialSource::Environment,
            refresh_token: None,
            session_id: None,
            expires_at: None,
        };
        let credential = ensure_with(
            origin,
            REPOSITORY,
            &store,
            false,
            Some(explicit.clone()),
            || panic!("a supplied session must not initiate authentication"),
        )
        .unwrap();
        assert_eq!(credential, explicit);
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(store.load_machine_session().unwrap().is_none());
    }

    #[test]
    fn oversized_private_state_is_refused_without_erasing_the_previous_record() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.json");
        save(&path, &"previous private state").unwrap();
        let before = fs::read(&path).unwrap();
        assert!(save(&path, &"a".repeat(MAX_PRIVATE_STATE_BYTES)).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let oversized = vec![b' '; MAX_PRIVATE_STATE_BYTES + 1];
        fs::write(&path, &oversized).unwrap();
        assert!(read::<serde_json::Value>(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), oversized);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_pending_symlinks_fail_before_authentication() {
        let origin = "http://127.0.0.1:9";
        let root = tempfile::tempdir().unwrap();
        let store = CloudSessionStore::for_repository_in(root.path(), origin, REPOSITORY).unwrap();
        let directory = store.scope_directory().unwrap();
        custody(crate::core::fs_permissions::ensure_private_dir(directory)).unwrap();
        let path = pending_path(directory, REPOSITORY);
        std::os::unix::fs::symlink(root.path().join("absent.json"), &path).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("unsafe pending state must fail before authentication")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn headless_sign_in_resumes_the_saved_flow_then_reuses_the_managed_session() {
        let fixture = fixture(|origin| {
            vec![
                (
                    "/functions/v1/propodus-onboarding/start",
                    serde_json::to_value(flow(origin, FIRST_FLOW)).unwrap(),
                ),
                (
                    "/functions/v1/propodus-onboarding/status",
                    json!({"status":"pending"}),
                ),
                (
                    "/functions/v1/propodus-onboarding/status",
                    json!({"status":"ready"}),
                ),
                (
                    "/functions/v1/propodus-onboarding/exchange",
                    json!({"auth_code":"one-time-code"}),
                ),
                (
                    "/auth/v1/token?grant_type=pkce",
                    json!({"access_token":"managed-access","refresh_token":"managed-refresh","provider_token":"scm-provider-token","expires_in":900}),
                ),
                (
                    "/functions/v1/propodus-onboarding/complete",
                    json!({"status":"complete"}),
                ),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let store =
            CloudSessionStore::for_repository_in(root.path(), &fixture.origin, REPOSITORY).unwrap();
        let connector = connector(&fixture.origin);
        assert_status(
            ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
                Ok(connector.clone())
            }),
            CloudAuthStatus::OnboardingPending,
        );
        let directory = store.scope_directory().unwrap();
        let pending_file = pending_path(directory, REPOSITORY);
        let saved: Pending = read(&pending_file).unwrap().unwrap();
        assert_eq!(saved.repository, REPOSITORY);
        assert_eq!(saved.origin, fixture.origin);
        assert_private(&pending_file);
        assert!(store.load_machine_session().unwrap().is_none());

        let credential = ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
            Ok(connector.clone())
        })
        .unwrap();
        assert_eq!(credential.token, "managed-access");
        assert_eq!(credential.refresh_token.as_deref(), Some("managed-refresh"));
        assert_eq!(
            store.load_provider_token().unwrap().as_deref(),
            Some("scm-provider-token")
        );
        assert!(!pending_file.exists());
        assert_private(&directory.join("session_token.json"));
        let reused = ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
            panic!("usable sessions must not invoke auth or connector discovery")
        })
        .unwrap();
        assert_eq!(reused, credential);

        fixture.handle.join().unwrap();
        let requests: Vec<_> = fixture.received.try_iter().collect();
        assert_eq!(requests.len(), 6);
        assert_eq!(
            requests[1].body["poll_secret"],
            requests[2].body["poll_secret"]
        );
        assert_eq!(requests[1].body["flow_id"], FIRST_FLOW);
        assert_eq!(requests[2].body["flow_id"], FIRST_FLOW);
        assert!(requests[0].body.get("poll_secret").is_none());
        assert!(requests[0].body.get("code_verifier").is_none());
        assert_eq!(
            requests[5].headers.get("authorization").map(String::as_str),
            Some("Bearer managed-access")
        );
        for request in &requests[..5] {
            assert!(!request.headers.contains_key("authorization"));
        }
    }

    #[test]
    fn uncertain_one_time_exchanges_are_preserved_without_any_backend_call() {
        for (token_exchange, expired) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            let origin = "http://127.0.0.1:9";
            let root = tempfile::tempdir().unwrap();
            let store =
                CloudSessionStore::for_repository_in(root.path(), origin, REPOSITORY).unwrap();
            let directory = store.scope_directory().unwrap();
            let path = pending_path(directory, REPOSITORY);
            let mut value = pending(origin);
            value.exchange_inflight = !token_exchange;
            value.token_inflight = token_exchange;
            if expired {
                value.flow.expires_at = (Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
            }
            if token_exchange {
                value.code = Some("unconfirmed-token-exchange-code".into());
            }
            save(&path, &value).unwrap();
            let before = fs::read(&path).unwrap();
            assert_status(
                ensure_with(origin, REPOSITORY, &store, false, None, || {
                    panic!("unknown one-time outcomes require local recovery only")
                }),
                CloudAuthStatus::AuthRequired,
            );
            assert!(!path.exists());
            let archives = uncertain_records(directory);
            assert_eq!(archives.len(), 1);
            assert_eq!(fs::read(&archives[0]).unwrap(), before);
            assert_private(&archives[0]);
            assert!(store.load_machine_session().unwrap().is_none());
        }
    }

    #[test]
    fn repository_authorization_must_complete_before_a_session_can_be_reused() {
        let fixture = fixture(|_| {
            vec![
                (
                    "/functions/v1/propodus-onboarding/complete",
                    json!({"status":"pending"}),
                ),
                (
                    "/functions/v1/propodus-onboarding/complete",
                    json!({"status":"complete"}),
                ),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let store =
            CloudSessionStore::for_repository_in(root.path(), &fixture.origin, REPOSITORY).unwrap();
        let mut saved = pending(&fixture.origin);
        saved.session = Some(CloudSession {
            access_token: "not-yet-authorized-access".into(),
            refresh_token: Some("not-yet-authorized-refresh".into()),
            session_id: None,
            expires_at: Some((Utc::now() + chrono::Duration::minutes(15)).to_rfc3339()),
        });
        let path = pending_path(store.scope_directory().unwrap(), REPOSITORY);
        save(&path, &saved).unwrap();
        let connector = connector(&fixture.origin);
        assert!(
            ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || Ok(
                connector.clone()
            ))
            .is_err()
        );
        assert!(store.load_machine_session().unwrap().is_none());
        assert!(
            path.exists(),
            "the agent must resume finalization without repeating login"
        );
        let credential = ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
            Ok(connector.clone())
        })
        .unwrap();
        assert_eq!(credential.token, "not-yet-authorized-access");
        assert!(!path.exists());
        fixture.handle.join().unwrap();
        assert_eq!(
            fixture.received.try_iter().count(),
            2,
            "only repository finalization may be retried"
        );
    }

    #[test]
    fn terminal_flow_without_local_result_can_be_restarted_on_the_next_invocation() {
        for terminal in ["consumed", "complete"] {
            let fixture = fixture(|origin| {
                vec![
                    (
                        "/functions/v1/propodus-onboarding/status",
                        json!({"status":terminal}),
                    ),
                    (
                        "/functions/v1/propodus-onboarding/start",
                        serde_json::to_value(flow(origin, SECOND_FLOW)).unwrap(),
                    ),
                    (
                        "/functions/v1/propodus-onboarding/status",
                        json!({"status":"pending"}),
                    ),
                ]
            });
            let root = tempfile::tempdir().unwrap();
            let store =
                CloudSessionStore::for_repository_in(root.path(), &fixture.origin, REPOSITORY)
                    .unwrap();
            let directory = store.scope_directory().unwrap();
            let path = pending_path(directory, REPOSITORY);
            save(&path, &pending(&fixture.origin)).unwrap();
            let connector = connector(&fixture.origin);
            assert_status(
                ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
                    Ok(connector.clone())
                }),
                CloudAuthStatus::AuthRequired,
            );
            assert!(!path.exists());
            assert_eq!(uncertain_records(directory).len(), 1);
            assert_status(
                ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
                    Ok(connector.clone())
                }),
                CloudAuthStatus::OnboardingPending,
            );
            let next: Pending = read(&path).unwrap().unwrap();
            assert_eq!(next.flow.flow_id, SECOND_FLOW);
            fixture.handle.join().unwrap();
            let requests: Vec<_> = fixture.received.try_iter().collect();
            assert_eq!(requests.len(), 3);
            assert_eq!(requests[0].body["flow_id"], FIRST_FLOW);
            assert_eq!(requests[2].body["flow_id"], SECOND_FLOW);
        }
    }

    #[test]
    fn concurrent_agents_rotate_the_repository_session_only_once() {
        let fixture = fixture(|_| {
            vec![(
                "/auth/v1/token?grant_type=refresh_token",
                json!({"access_token":"rotated-access","refresh_token":"rotated-refresh","expires_in":900}),
            )]
        });
        let root = tempfile::tempdir().unwrap();
        let store =
            CloudSessionStore::for_repository_in(root.path(), &fixture.origin, REPOSITORY).unwrap();
        store
            .store_managed_session(
                &CloudSession {
                    access_token: "expired-access".into(),
                    refresh_token: Some("old-refresh".into()),
                    session_id: None,
                    expires_at: Some((Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
                },
                Some("retained-scm-token"),
                false,
            )
            .unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = [REPOSITORY, REPOSITORY]
            .into_iter()
            .map(|repository| {
                let store = store.clone();
                let connector = connector(&fixture.origin);
                let origin = fixture.origin.clone();
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    ensure_with(&origin, repository, &store, false, None, || {
                        Ok(connector.clone())
                    })
                    .unwrap()
                })
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap().token, "rotated-access");
        }
        assert!(
            !store
                .scope_directory()
                .unwrap()
                .join("refresh-inflight.json")
                .exists()
        );
        assert_eq!(
            store.load_provider_token().unwrap().as_deref(),
            Some("retained-scm-token")
        );
        fixture.handle.join().unwrap();
        let requests: Vec<_> = fixture.received.try_iter().collect();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].body, json!({"refresh_token":"old-refresh"}));
        assert!(!requests[0].headers.contains_key("authorization"));
    }

    #[test]
    fn uncertain_refresh_rotation_starts_new_sign_in_without_replaying_the_token() {
        let fixture = fixture(|origin| {
            vec![
                (
                    "/functions/v1/propodus-onboarding/start",
                    serde_json::to_value(flow(origin, FIRST_FLOW)).unwrap(),
                ),
                (
                    "/functions/v1/propodus-onboarding/status",
                    json!({"status":"pending"}),
                ),
            ]
        });
        let root = tempfile::tempdir().unwrap();
        let store =
            CloudSessionStore::for_repository_in(root.path(), &fixture.origin, REPOSITORY).unwrap();
        store
            .store_managed_session(
                &CloudSession {
                    access_token: "expired-access".into(),
                    refresh_token: Some("possibly-consumed-refresh".into()),
                    session_id: None,
                    expires_at: Some((Utc::now() - chrono::Duration::minutes(1)).to_rfc3339()),
                },
                None,
                false,
            )
            .unwrap();
        let refresh_path = store
            .scope_directory()
            .unwrap()
            .join("refresh-inflight.json");
        save(
            &refresh_path,
            &format!("{:x}", Sha256::digest(b"possibly-consumed-refresh")),
        )
        .unwrap();
        let before = fs::read(&refresh_path).unwrap();
        let connector = connector(&fixture.origin);
        assert_status(
            ensure_with(&fixture.origin, REPOSITORY, &store, false, None, || {
                Ok(connector.clone())
            }),
            CloudAuthStatus::OnboardingPending,
        );
        assert_eq!(fs::read(&refresh_path).unwrap(), before);
        assert_private(&refresh_path);
        fixture.handle.join().unwrap();
        let requests: Vec<_> = fixture.received.try_iter().collect();
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].body["repository"], REPOSITORY);
    }

    #[test]
    fn malformed_or_wrong_scope_pending_state_fails_without_network_and_is_preserved() {
        let origin = "http://127.0.0.1:9";
        let root = tempfile::tempdir().unwrap();
        let store = CloudSessionStore::for_repository_in(root.path(), origin, REPOSITORY).unwrap();
        let path = pending_path(store.scope_directory().unwrap(), REPOSITORY);
        let mut wrong_scope = pending(origin);
        wrong_scope.repository = "owner/different".into();
        save(&path, &wrong_scope).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("wrong-scope state must fail before I/O")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let mut wrong_origin = pending("https://different.example");
        wrong_origin.repository = REPOSITORY.into();
        save(&path, &wrong_origin).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("wrong-origin state must fail before I/O")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let mut invalid_expiry = pending(origin);
        invalid_expiry.flow.expires_at = "invalid".into();
        save(&path, &invalid_expiry).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("invalid-expiry state must fail before I/O")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        save(&path, &json!({"incomplete":"private state"})).unwrap();
        let malformed = fs::read(&path).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("malformed state must fail before I/O")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert_eq!(fs::read(&path).unwrap(), malformed);
    }

    #[cfg(unix)]
    #[test]
    fn a_pending_file_symlink_is_refused_before_authentication_or_replacement() {
        let origin = "http://127.0.0.1:9";
        let root = tempfile::tempdir().unwrap();
        let store = CloudSessionStore::for_repository_in(root.path(), origin, REPOSITORY).unwrap();
        let directory = store.scope_directory().unwrap();
        let target = root.path().join("other-private-state.json");
        save(&target, &pending(origin)).unwrap();
        custody(crate::core::fs_permissions::ensure_private_dir(directory)).unwrap();
        let path = pending_path(directory, REPOSITORY);
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let bytes = fs::read(&target).unwrap();
        assert_status(
            ensure_with(origin, REPOSITORY, &store, false, None, || {
                panic!("unsafe pending state must fail before authentication")
            }),
            CloudAuthStatus::AuthRequired,
        );
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&target).unwrap(), bytes);
    }
}

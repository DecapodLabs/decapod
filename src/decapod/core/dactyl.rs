//! Decapod's narrow boundary to the Dactyl physical storage contract.
//!
//! Dactyl's local route opens the existing SQLite file directly through its
//! private host-runtime connector. No second format or compatibility database
//! is introduced.

use crate::core::backend::{BackendRoute, StorageContext};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::schemas;
use dactyl_db::{AccessMode, AtomicResult, Connection, OpenOptions, Operation, Parameter, Rows};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub use dactyl_db::{OperationResult, WriteResult};

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_millis(250);
const DATASTORE_ENV: &str = "DATASTORE";
const DATASTORE_ROUTE_ENV: &str = "DATASTORE_ROUTE";
const DATASTORE_TOKEN_ENV: &str = "DATASTORE_TOKEN";
pub const SQLITE_LIBRARY_ENV: &str = "DACTYL_SQLITE_LIBRARY";
const HOST_RUNTIME_CONFIG_FILE: &str = "runtime.toml";

static AMBIENT_ROUTE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Default, Deserialize, Serialize)]
struct HostRuntimeConfig {
    #[serde(default = "default_runtime_schema_version")]
    schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sqlite_library: Option<String>,
}

fn default_runtime_schema_version() -> String {
    "1".to_string()
}

/// Prepare the native SQLite capability required by Dactyl's local adapter.
///
/// The capability is machine-local rather than repository-local: the same
/// host library can serve every Decapod project, while different hosts may
/// resolve different paths. An explicit shell variable or persisted runtime
/// value is trusted and applied without repeating discovery. Discovery only
/// runs when neither value exists, and a discovered path is persisted beside
/// Decapod's machine-local session state.
pub fn ensure_local_sqlite_runtime() -> Result<(), DecapodError> {
    if std::env::var(SQLITE_LIBRARY_ENV)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(());
    }

    if let Some(configured) = configured_sqlite_library()? {
        set_sqlite_library_env(&configured);
        return Ok(());
    }

    match DactylBridge::open_memory() {
        Ok(_) => Ok(()),
        Err(error) if is_sqlite_runtime_unavailable(&error) => {
            let Some(discovered) = discover_sqlite_library() else {
                return Err(sqlite_runtime_required_error());
            };

            set_sqlite_library_env(&discovered);
            if let Err(error) = persist_sqlite_library(&discovered) {
                eprintln!(
                    "warn: native SQLite is available at '{}', but Decapod could not persist it for future runs: {error}; set {SQLITE_LIBRARY_ENV} in the current shell to reuse it",
                    discovered.display()
                );
            }

            if let Err(error) = DactylBridge::open_memory() {
                if is_sqlite_runtime_unavailable(&error) {
                    return Err(sqlite_runtime_required_error());
                }
                return Err(error);
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn is_sqlite_runtime_unavailable(error: &DecapodError) -> bool {
    matches!(
        error,
        DecapodError::DactylError(error)
            if error.adapter_code() == Some("sqlite_runtime_unavailable")
                || error.adapter_code() == Some("sqlite_runtime_incompatible")
    )
}

fn configured_sqlite_library() -> Result<Option<String>, DecapodError> {
    let path = host_runtime_config_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(&path).map_err(DecapodError::IoError)?;
    let config: HostRuntimeConfig = toml::from_str(&raw).map_err(|error| {
        DecapodError::Config(format!(
            "invalid Decapod machine runtime config '{}': {error}; remove or repair the file, then retry",
            path.display()
        ))
    })?;
    Ok(config
        .sqlite_library
        .filter(|value| !value.trim().is_empty()))
}

fn persist_sqlite_library(path: &Path) -> Result<(), DecapodError> {
    let config_path = host_runtime_config_path()?;
    let parent = config_path.parent().ok_or_else(|| {
        DecapodError::Config("Decapod machine runtime config has no parent directory".to_string())
    })?;
    fs::create_dir_all(parent).map_err(DecapodError::IoError)?;

    let config = HostRuntimeConfig {
        schema_version: default_runtime_schema_version(),
        sqlite_library: Some(path.to_string_lossy().into_owned()),
    };
    let body = toml::to_string_pretty(&config)
        .map_err(|error| DecapodError::Config(format!("encode machine runtime config: {error}")))?;
    let temporary = config_path.with_extension(format!("toml.{}.tmp", std::process::id()));
    fs::write(&temporary, body).map_err(DecapodError::IoError)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&temporary)
            .map_err(DecapodError::IoError)?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&temporary, permissions).map_err(DecapodError::IoError)?;
    }
    fs::rename(&temporary, &config_path).map_err(DecapodError::IoError)
}

fn host_runtime_config_path() -> Result<PathBuf, DecapodError> {
    // Keep this beside the machine-local session records by using the same
    // resolver, including XDG_CONFIG_HOME, rather than inventing a second
    // notion of the user's Decapod configuration directory.
    Ok(crate::machine_config_dir()?.join(HOST_RUNTIME_CONFIG_FILE))
}

fn set_sqlite_library_env(path: impl AsRef<Path>) {
    // Startup capability resolution runs before Decapod starts worker threads.
    // Dactyl's public configuration surface is the process environment.
    unsafe { std::env::set_var(SQLITE_LIBRARY_ENV, path.as_ref().to_string_lossy().as_ref()) };
}

fn discover_sqlite_library() -> Option<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(search_path) = std::env::var("LD_LIBRARY_PATH") {
        directories.extend(
            search_path
                .split(':')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        );
    }
    for path in [
        "/usr/lib",
        "/usr/lib64",
        "/usr/local/lib",
        "/lib",
        "/lib64",
        "/opt/homebrew/opt/sqlite/lib",
        "/usr/local/opt/sqlite/lib",
        "/nix/var/nix/profiles/default/lib",
    ] {
        directories.push(PathBuf::from(path));
    }
    if let Ok(home) = std::env::var("HOME") {
        directories.push(PathBuf::from(home).join(".nix-profile/lib"));
    }

    // Nix keeps package libraries under content-addressed store entries rather
    // than a conventional global loader path. Inspect only each entry's direct
    // `lib` directory so a missing host symlink does not hide an installed runtime.
    if let Ok(entries) = fs::read_dir("/nix/store") {
        directories.extend(entries.flatten().map(|entry| entry.path().join("lib")));
    }

    directories.sort();
    directories.dedup();
    let mut candidates = directories
        .into_iter()
        .flat_map(|directory| fs::read_dir(directory).into_iter().flatten().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_sqlite_library_name)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        (sqlite_library_name_rank(name), path.clone())
    });
    candidates.into_iter().next()
}

fn is_sqlite_library_name(name: &str) -> bool {
    name == "sqlite3.dll"
        || name == "libsqlite3.dylib"
        || name == "libsqlite3.so"
        || name.starts_with("libsqlite3.so.")
}

fn sqlite_library_name_rank(name: &str) -> u8 {
    match name {
        "libsqlite3.so" | "libsqlite3.dylib" | "sqlite3.dll" => 0,
        "libsqlite3.so.0" => 1,
        _ => 2,
    }
}

fn sqlite_runtime_required_error() -> DecapodError {
    let config_path = host_runtime_config_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "~/.config/decapod/runtime.toml".to_string());
    let install = if cfg!(target_os = "macos") {
        "brew install sqlite"
    } else if cfg!(target_os = "windows") {
        "winget install SQLite.SQLite"
    } else if cfg!(target_os = "linux") {
        "Debian/Ubuntu: sudo apt-get install libsqlite3-0; Fedora/RHEL: sudo dnf install sqlite-libs; Nix: nix profile install nixpkgs#sqlite"
    } else {
        "install the SQLite runtime shared library supplied by your operating system"
    };
    DecapodError::ValidationError(format!(
        "AUTOREMEDIABLE_VALIDATION_ERROR code=LOCAL_SQLITE_RUNTIME_REQUIRED severity=transient auto_remediable=true audience=agent agent_action=\"Install the OS SQLite runtime using the platform command below, then retry; if it is already installed, set {SQLITE_LIBRARY_ENV} to its absolute shared-library path\" user_note=\"backend=local requires Dactyl's native host SQLite library; Cloud backend does not require SQLite.\"\nLOCAL_SQLITE_RUNTIME_REQUIRED: no host SQLite shared library was found for backend=local. Install: {install}\nAlternative: export {SQLITE_LIBRARY_ENV}=/path/to/libsqlite3.so and retry. Decapod stores discovered paths in the user-level config '{config_path}' for future projects."
    ))
}

/// A route-scoped Dactyl driver. The underlying connection never escapes this
/// wrapper, so Decapod callers use Dactyl's operation/result contract rather
/// than backend-specific handles.
pub struct DactylBridge {
    connection: Connection,
}

impl DactylBridge {
    /// Open Dactyl's isolated in-memory store for conformance tests and
    /// adapter probes. This is not the Decapod canonical local store.
    pub fn open_memory() -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            AccessMode::ReadWrite,
            None,
        )
    }

    /// Open Dactyl's isolated in-memory store with an explicit access mode.
    pub fn open_memory_with_access_mode(access_mode: AccessMode) -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            access_mode,
            None,
        )
    }

    /// Open an existing local SQLite file with an explicit access mode.
    pub fn open_local(
        path: impl AsRef<Path>,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        let path = path.as_ref();
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(path.to_string_lossy().into_owned()),
            access_mode,
            None,
        )
    }

    /// Open the repository's canonical local datastore through Dactyl.
    ///
    /// This is the only supported local runtime entrypoint for
    /// `.decapod/data/decapod.db`. The path remains Decapod-owned policy, but
    /// physical opening and all subsequent operations belong to Dactyl.
    pub fn open_canonical(
        data_root: impl AsRef<Path>,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        Self::open_local(data_root.as_ref().join(schemas::LOCAL_DB_NAME), access_mode)
    }

    /// Bind a governed backend route to Dactyl.
    ///
    /// A local route is always opened through Dactyl. Cloud routes
    /// require a separate machine-local bearer credential and are passed
    /// through as opaque HTTP endpoints; this method never derives provider
    /// URLs or silently falls back to local storage.
    pub fn from_backend_route(
        route: &BackendRoute,
        access_mode: AccessMode,
        bearer: Option<&str>,
    ) -> Result<Self, DecapodError> {
        match route {
            BackendRoute::Local { path } => {
                Self::open_from_ambient("sqlite", &path.to_string_lossy(), None, access_mode, None)
            }
            BackendRoute::Cloud { uri, .. } => {
                let bearer = bearer
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                            CloudAuthStatus::Missing,
                            "cloud storage requires a machine-local session credential",
                            "acquire or refresh the cloud session, then retry the command",
                        ))
                    })?;
                Self::open_from_ambient("neon", uri, Some(bearer), access_mode, None)
            }
        }
    }

    /// Open the physical driver from a Decapod-owned storage context.
    ///
    /// The context's bearer is passed as opaque authentication material to
    /// Dactyl and is never interpreted as organization, user, or repository
    /// policy by this bridge.
    pub fn from_storage_context(
        context: &StorageContext,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        context.validate()?;
        let dactyl_context = dactyl_db::StorageContext::new(
            context.version(),
            serde_json::to_value(context).map_err(|error| {
                DecapodError::Config(format!("failed to encode storage context: {error}"))
            })?,
        )?;

        match context.route() {
            BackendRoute::Local { .. } => {
                Self::from_backend_route(context.route(), access_mode, context.bearer())
            }
            BackendRoute::Cloud { uri, .. } => {
                let bearer = context.bearer().ok_or_else(|| {
                    DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                        CloudAuthStatus::Missing,
                        "cloud storage requires a machine-local session credential",
                        "acquire or refresh the cloud session, then retry the command",
                    ))
                })?;
                Self::open_from_ambient(
                    "neon",
                    uri,
                    Some(bearer),
                    access_mode,
                    Some(dactyl_context),
                )
            }
        }
    }

    pub fn read(&self, sql: &str, params: &[Parameter]) -> Result<Rows, DecapodError> {
        Ok(self.connection.read(sql, params)?)
    }

    pub fn write(&self, sql: &str, params: &[Parameter]) -> Result<WriteResult, DecapodError> {
        Ok(self.connection.write_result(sql, params)?)
    }

    pub fn atomic(&self, operations: &[Operation]) -> Result<AtomicResult, DecapodError> {
        Ok(self.connection.atomic(operations)?)
    }

    pub fn access_mode(&self) -> AccessMode {
        self.connection.access_mode()
    }

    /// Inspect the backend-neutral schema exposed by Dactyl.
    pub fn inspect_schema(&self) -> Result<dactyl_db::StoreSchema, DecapodError> {
        Ok(self.connection.inspect_schema()?)
    }

    /// Return whether a caller-owned table is present through Dactyl's
    /// backend-neutral schema inspection contract.
    pub fn has_table(&self, name: &str) -> Result<bool, DecapodError> {
        Ok(self.inspect_schema()?.table(name).is_some())
    }

    fn open_route(
        route: dactyl_db::DatastoreRoute,
        access_mode: AccessMode,
        context: Option<dactyl_db::StorageContext>,
    ) -> Result<Self, DecapodError> {
        let connection = Connection::open_with_options_and_context(
            route,
            OpenOptions {
                access_mode,
                lock_timeout: DEFAULT_LOCK_TIMEOUT,
            },
            context,
        )?;
        Ok(Self { connection })
    }

    /// Let Dactyl resolve its own route from the ambient values supplied by
    /// Decapod. The values are scoped to connection construction; Dactyl
    /// captures the route and token in its connection, while Decapod never
    /// leaks another project's endpoint or credential into the process.
    fn open_from_ambient(
        datastore: &str,
        route: &str,
        token: Option<&str>,
        access_mode: AccessMode,
        context: Option<dactyl_db::StorageContext>,
    ) -> Result<Self, DecapodError> {
        let lock = AMBIENT_ROUTE_LOCK.get_or_init(|| Mutex::new(()));
        let _lock = lock.lock().map_err(|_| {
            DecapodError::Config("Dactyl ambient route lock was poisoned".to_string())
        })?;
        let _environment = AmbientDactylEnvironment::install(datastore, route, token);
        let resolved = dactyl_db::DatastoreRoute::from_env()?;
        Self::open_route(resolved, access_mode, context)
    }
}

struct AmbientDactylEnvironment {
    datastore: Option<OsString>,
    route: Option<OsString>,
    token: Option<OsString>,
}

impl AmbientDactylEnvironment {
    fn install(datastore: &str, route: &str, token: Option<&str>) -> Self {
        let previous = Self {
            datastore: std::env::var_os(DATASTORE_ENV),
            route: std::env::var_os(DATASTORE_ROUTE_ENV),
            token: std::env::var_os(DATASTORE_TOKEN_ENV),
        };
        unsafe {
            std::env::set_var(DATASTORE_ENV, datastore);
            std::env::set_var(DATASTORE_ROUTE_ENV, route);
            match token {
                Some(token) => std::env::set_var(DATASTORE_TOKEN_ENV, token),
                None => std::env::remove_var(DATASTORE_TOKEN_ENV),
            }
        }
        previous
    }
}

impl Drop for AmbientDactylEnvironment {
    fn drop(&mut self) {
        unsafe {
            restore_env(DATASTORE_ENV, self.datastore.take());
            restore_env(DATASTORE_ROUTE_ENV, self.route.take());
            restore_env(DATASTORE_TOKEN_ENV, self.token.take());
        }
    }
}

fn restore_env(name: &str, value: Option<OsString>) {
    unsafe {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::error::{DecapodError, StorageFailureKind};
    use crate::core::repo_identity::RepositoryIdentity;
    use tempfile::tempdir;

    fn no_params() -> Vec<Parameter> {
        Vec::new()
    }

    fn local_runtime_available() -> bool {
        match DactylBridge::open_memory() {
            Ok(_) => true,
            Err(DecapodError::DactylError(error))
                if error.adapter_code() == Some("sqlite_runtime_unavailable") =>
            {
                false
            }
            Err(error) => panic!("unexpected local Dactyl open failure: {error}"),
        }
    }

    #[test]
    fn missing_local_runtime_is_reported_as_typed_storage_failure() {
        if let Err(error) = DactylBridge::open_memory() {
            assert_eq!(error.storage_failure_kind(), StorageFailureKind::Io);
            assert!(matches!(
                error,
                DecapodError::DactylError(error)
                    if error.adapter_code() == Some("sqlite_runtime_unavailable")
            ));
        }
    }

    #[test]
    fn missing_runtime_message_is_concise_and_agent_actionable() {
        let message = sqlite_runtime_required_error().to_string();
        assert!(message.starts_with(
            "Validation error: AUTOREMEDIABLE_VALIDATION_ERROR code=LOCAL_SQLITE_RUNTIME_REQUIRED"
        ));
        assert!(message.contains("auto_remediable=true"));
        assert!(message.contains("audience=agent"));
        assert!(message.contains("agent_action=\"Install the OS SQLite runtime"));
        assert!(message.contains("backend=local"));
        assert!(message.contains("DACTYL_SQLITE_LIBRARY"));
        assert!(message.contains("export DACTYL_SQLITE_LIBRARY="));
        assert!(message.contains(".config/decapod/runtime.toml"));
        assert!(message.contains("Cloud backend does not require SQLite"));
        assert!(!message.contains("storage open failed at stage="));
    }

    #[test]
    fn host_runtime_config_is_machine_local_and_serializable() {
        let config = HostRuntimeConfig {
            schema_version: default_runtime_schema_version(),
            sqlite_library: Some("/opt/sqlite/lib/libsqlite3.so".to_string()),
        };
        let encoded = toml::to_string_pretty(&config).expect("runtime config encoding");
        let decoded: HostRuntimeConfig = toml::from_str(&encoded).expect("runtime config decode");
        assert_eq!(decoded.schema_version, "1");
        assert_eq!(
            decoded.sqlite_library.as_deref(),
            Some("/opt/sqlite/lib/libsqlite3.so")
        );
    }

    #[test]
    fn sqlite_library_discovery_accepts_supported_host_names_only() {
        assert!(is_sqlite_library_name("libsqlite3.so"));
        assert!(is_sqlite_library_name("libsqlite3.so.0"));
        assert!(is_sqlite_library_name("libsqlite3.dylib"));
        assert!(is_sqlite_library_name("sqlite3.dll"));
        assert!(!is_sqlite_library_name("libsqlite.so"));
        assert!(!is_sqlite_library_name("sqlite3"));
    }

    #[test]
    fn explicit_ids_and_atomic_rollback_are_backend_neutral() {
        if !local_runtime_available() {
            eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
            return;
        }
        let bridge = DactylBridge::open_memory().expect("memory bridge");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, revision INTEGER NOT NULL)",
                no_params(),
            )])
            .expect("schema");

        let insert = |id: i64, title: &str| {
            Operation::write(
                "INSERT INTO tasks (id, title, revision) VALUES (?, ?, ?)",
                vec![id.into(), title.into(), 0_i64.into()],
            )
        };

        bridge
            .atomic(&[insert(101, "stable")])
            .expect("explicit insert");
        let rolled_back = bridge.atomic(&[insert(102, "temporary"), insert(101, "conflict")]);
        assert!(rolled_back.is_err(), "duplicate key must abort the batch");

        let rows = bridge
            .read("SELECT id, title, revision FROM tasks", &no_params())
            .expect("read rows");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
        assert_eq!(
            rows.as_slice()[0].get_str("title").expect("title"),
            "stable"
        );
    }

    #[test]
    fn read_only_access_is_rejected_by_physical_driver() {
        if !local_runtime_available() {
            eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
            return;
        }
        let bridge = DactylBridge::open_memory_with_access_mode(AccessMode::ReadOnly)
            .expect("read-only memory bridge");
        let error = bridge
            .write("INSERT INTO missing (id) VALUES (?)", &[1_i64.into()])
            .expect_err("read-only route must reject writes");
        assert_eq!(error.storage_failure_kind(), StorageFailureKind::Capability);
    }

    #[test]
    fn local_route_reopens_existing_sqlite() {
        if !local_runtime_available() {
            eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
            return;
        }
        let tmp = tempdir().expect("temporary local database directory");
        let database_path = tmp.path().join("decapod.db");
        std::fs::File::create(&database_path).expect("empty local database target");
        let bridge = DactylBridge::open_local(&database_path, AccessMode::ReadWrite)
            .expect("Dactyl local database bridge");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
                no_params(),
            )])
            .expect("local schema");
        bridge
            .write(
                "INSERT INTO tasks (id, title) VALUES (?, ?)",
                &[101_i64.into(), "persisted".into()],
            )
            .expect("local row");
        drop(bridge);

        let reopened = DactylBridge::open_local(&database_path, AccessMode::ReadOnly)
            .expect("reopen Dactyl local database");
        let rows = reopened
            .read("SELECT id, title FROM tasks", &no_params())
            .expect("read persisted local database");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows.as_slice()[0].get_int("id").expect("id"), 101);
        assert_eq!(
            rows.as_slice()[0].get_str("title").expect("title"),
            "persisted"
        );
        drop(reopened);
    }

    #[test]
    fn canonical_route_is_dactyl_owned_and_schema_inspection_is_portable() {
        if !local_runtime_available() {
            eprintln!("skipping local Dactyl conformance: host SQLite runtime unavailable");
            return;
        }
        let tmp = tempdir().expect("temporary canonical data root");
        let canonical_path = tmp.path().join("decapod.db");
        std::fs::File::create(&canonical_path).expect("canonical Dactyl database target");

        let bridge = DactylBridge::open_canonical(tmp.path(), AccessMode::ReadWrite)
            .expect("canonical Dactyl route");
        bridge
            .atomic(&[Operation::schema(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL)",
                no_params(),
            )])
            .expect("canonical schema");

        assert!(
            bridge
                .has_table("tasks")
                .expect("portable schema inspection")
        );
        assert!(canonical_path.exists());
    }

    #[test]
    fn cloud_route_requires_explicit_bearer_and_keeps_repository_scope() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let route = BackendRoute::Cloud {
            repository: identity,
            uri: "https://example.invalid/api/v1/store".to_string(),
        };

        let missing = match DactylBridge::from_backend_route(&route, AccessMode::ReadWrite, None) {
            Err(error) => error,
            Ok(_) => panic!("cloud must fail closed without a bearer"),
        };
        assert!(matches!(missing, DecapodError::CloudAuth(_)));

        let bridge = DactylBridge::from_backend_route(
            &route,
            AccessMode::ReadOnly,
            Some("opaque-session-token"),
        )
        .expect("constructing a cloud route does not perform I/O");
        assert_eq!(bridge.access_mode(), AccessMode::ReadOnly);
    }

    #[test]
    fn storage_context_binds_the_opaque_bearer_without_serializing_it() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let route = BackendRoute::Cloud {
            repository: identity,
            uri: "https://example.invalid/api/v1/store".to_string(),
        };
        let context = StorageContext::from_route(route, Some("opaque-session-token"))
            .expect("remote context");
        let bridge = DactylBridge::from_storage_context(&context, AccessMode::ReadOnly)
            .expect("constructing cloud route does not perform I/O");
        assert_eq!(context.version(), StorageContext::CURRENT_VERSION);
        assert_eq!(context.bearer(), Some("opaque-session-token"));
        assert!(bridge.access_mode() == AccessMode::ReadOnly);
        assert_eq!(bridge.connection.datastore(), dactyl_db::Datastore::Neon);
        assert_eq!(
            bridge.connection.route().route(),
            "https://example.invalid/api/v1/store"
        );
        assert_eq!(
            bridge.connection.route().token(),
            Some("opaque-session-token")
        );
        let forwarded = bridge
            .connection
            .context()
            .expect("remote context is attached to the Dactyl connection");
        assert_eq!(forwarded.version(), context.version());
        assert_eq!(
            forwarded.payload(),
            &serde_json::to_value(&context).expect("context payload")
        );
        let encoded = serde_json::to_string(&context).expect("context JSON");
        assert!(!encoded.contains("opaque-session-token"));
        assert!(!encoded.contains("organization"));
    }

    #[test]
    fn unsupported_context_version_fails_before_dactyl_open() {
        let identity = RepositoryIdentity {
            canonical_name: "DecapodLabs/decapod".to_string(),
            owner: "DecapodLabs".to_string(),
            repository: "decapod".to_string(),
            remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
        };
        let context = StorageContext::from_route(
            BackendRoute::Cloud {
                repository: identity,
                uri: "https://example.invalid/api/v1/store".to_string(),
            },
            Some("opaque-session-token"),
        )
        .expect("remote context");
        let mut encoded = serde_json::to_value(&context).expect("context JSON");
        encoded["version"] = serde_json::json!(StorageContext::CURRENT_VERSION + 1);
        let future: StorageContext = serde_json::from_value(encoded).expect("future context");

        assert!(matches!(
            DactylBridge::from_storage_context(&future, AccessMode::ReadOnly),
            Err(DecapodError::Config(message))
                if message.contains("unsupported storage context version")
        ));
    }

    #[test]
    fn dactyl_errors_map_to_decapod_storage_classes() {
        let busy = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Busy,
            code: None,
            message: "busy".to_string(),
        });
        assert_eq!(busy.storage_failure_kind(), StorageFailureKind::Contention);

        let constraint = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Constraint,
            code: None,
            message: "constraint".to_string(),
        });
        assert_eq!(
            constraint.storage_failure_kind(),
            StorageFailureKind::Constraint
        );

        let unavailable = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Unavailable,
            code: Some("service_unavailable".to_string()),
            message: "temporarily unavailable".to_string(),
        });
        assert_eq!(unavailable.storage_failure_kind(), StorageFailureKind::Io);
        assert!(unavailable.storage_failure_kind().is_retryable());

        let authorization = DecapodError::from(dactyl_db::DactylError::Adapter {
            kind: dactyl_db::AdapterErrorKind::Authorization,
            code: Some("repository_not_authorized".to_string()),
            message: "not authorized".to_string(),
        });
        assert_eq!(
            authorization.storage_failure_kind(),
            StorageFailureKind::Unknown
        );
        assert!(!authorization.storage_failure_kind().is_retryable());
    }
}

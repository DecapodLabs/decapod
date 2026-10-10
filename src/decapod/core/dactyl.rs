//! Decapod's narrow boundary to the Dactyl physical storage contract.
//!
//! Dactyl's local route opens the existing SQLite file directly through its
//! private host-runtime connector. No second format or compatibility database
//! is introduced.

use crate::core::backend::{BackendRoute, CloudDatastore, StorageContext};
use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::schemas;
use crate::core::storage_lock::{StorageLock, StorageLockMode};
use ::dactyl_db::{
    AccessMode, AtomicResult, BackupResult, Connection, IntegrityReport, OpenOptions, Operation,
    Parameter, RecoveryJournalMode, RecoveryOptions, RecoveryResult, Rows,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use dactyl_db::{OperationResult, WriteResult};

const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_millis(250);
pub const SQLITE_LIBRARY_ENV: &str = "DACTYL_SQLITE_LIBRARY";
const HOST_RUNTIME_CONFIG_FILE: &str = "runtime.toml";

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

    crate::core::fs_permissions::check_file(&path, false).map_err(DecapodError::IoError)?;
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
    crate::core::fs_permissions::ensure_private_dir(parent).map_err(DecapodError::IoError)?;

    let config = HostRuntimeConfig {
        schema_version: default_runtime_schema_version(),
        sqlite_library: Some(path.to_string_lossy().into_owned()),
    };
    let body = toml::to_string_pretty(&config)
        .map_err(|error| DecapodError::Config(format!("encode machine runtime config: {error}")))?;
    crate::core::atomic::write_atomic(&config_path, body.as_bytes()).map_err(DecapodError::IoError)
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
    _storage_lock: Option<StorageLock>,
}

impl DactylBridge {
    /// Open Dactyl's isolated in-memory store for conformance tests and
    /// adapter probes. This is not the Decapod canonical local store.
    pub fn open_memory() -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            AccessMode::ReadWrite,
            None,
            None,
        )
    }

    /// Open Dactyl's isolated in-memory store with an explicit access mode.
    pub fn open_memory_with_access_mode(access_mode: AccessMode) -> Result<Self, DecapodError> {
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(":memory:"),
            access_mode,
            None,
            None,
        )
    }

    /// Open an existing local SQLite file with an explicit access mode.
    pub fn open_local(
        path: impl AsRef<Path>,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        let path = path.as_ref();
        let storage_lock = local_storage_lock(path, access_mode)?;
        Self::open_route(
            dactyl_db::DatastoreRoute::sqlite(path.to_string_lossy().into_owned()),
            access_mode,
            None,
            storage_lock,
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
        let context = StorageContext::from_route(route.clone(), bearer)?;
        Self::from_storage_context(&context, access_mode)
    }

    /// Map Decapod's logical context to one explicit Dactyl capability.
    /// Authentication and repository scope are supplied separately; no ambient
    /// `DATASTORE*` values are changed or trusted by this composition boundary.
    pub fn from_storage_context(
        context: &StorageContext,
        access_mode: AccessMode,
    ) -> Result<Self, DecapodError> {
        context.validate()?;
        match context.route() {
            BackendRoute::Local { path } => Self::open_local(path, access_mode),
            BackendRoute::Cloud { uri, .. } => {
                let bearer = context.bearer().ok_or_else(|| {
                    DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                        CloudAuthStatus::Missing,
                        "cloud storage requires a machine-local session credential",
                        "acquire or refresh the cloud session, then retry the command",
                    ))
                })?;
                let route = match context.cloud_datastore() {
                    CloudDatastore::Neon => {
                        dactyl_db::DatastoreRoute::neon(uri, Some(bearer.to_string()))
                    }
                    CloudDatastore::Supabase => {
                        #[cfg(feature = "supabase-cloud")]
                        {
                            dactyl_db::DatastoreRoute::supabase(uri, Some(bearer.to_string()))
                        }
                        #[cfg(not(feature = "supabase-cloud"))]
                        {
                            return Err(DecapodError::Config(
                                "Supabase cloud capability is unavailable in this build"
                                    .to_string(),
                            ));
                        }
                    }
                };
                let wire_context = dactyl_db::StorageContext::new(
                    context.version(),
                    serde_json::to_value(context).map_err(|error| {
                        DecapodError::Config(format!("failed to encode storage context: {error}"))
                    })?,
                )?;
                Self::open_route(route, access_mode, Some(wire_context), None)
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

    /// Verify the selected store through Dactyl's native integrity contract.
    ///
    /// Decapod reports the result but deliberately does not attempt a repair.
    /// Dactyl owns the physical verification and typed failure categories.
    pub fn verify_integrity(&self) -> Result<IntegrityReport, DecapodError> {
        Ok(self.connection.verify_integrity()?)
    }

    /// Create a Dactyl online-backup snapshot at an operator-selected path.
    ///
    /// The canonical connection remains guarded for the lifetime of this
    /// bridge, so cooperating Decapod writers cannot overlap the operation.
    /// Dactyl owns SQLite's WAL/SHM-aware snapshot and atomic publication.
    pub fn backup(&self, destination: impl AsRef<Path>) -> Result<BackupResult, DecapodError> {
        let destination = destination.as_ref();
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        crate::core::fs_permissions::ensure_storage_dir(parent).map_err(DecapodError::IoError)?;
        crate::core::fs_permissions::check_storage(destination).map_err(DecapodError::IoError)?;
        // Dactyl owns the snapshot, but currently creates maintenance temporaries
        // with ambient mode. Confine those in a fresh private sibling directory.
        let staging = parent.join(format!(".decapod-backup-{}", crate::core::ulid::new_ulid()));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&staging).map_err(DecapodError::IoError)?;
        let snapshot = staging.join("snapshot.db");
        let result: Result<BackupResult, DecapodError> = (|| {
            let mut result = self.connection.backup(&snapshot)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if crate::core::fs_permissions::shared_storage() {
                    0o660
                } else {
                    0o600
                };
                // This is the new private staging inode, never an existing user file.
                let mode = fs::metadata(&snapshot)
                    .map_err(DecapodError::IoError)?
                    .permissions()
                    .mode()
                    & mode;
                fs::set_permissions(&snapshot, fs::Permissions::from_mode(mode))
                    .map_err(DecapodError::IoError)?;
            }
            fs::File::open(&snapshot)
                .and_then(|f| f.sync_all())
                .map_err(DecapodError::IoError)?;
            // Same-filesystem publication without overwriting a concurrently
            // created destination. Removing staging leaves a single link.
            fs::hard_link(&snapshot, destination).map_err(DecapodError::IoError)?;
            result.destination = destination.to_string_lossy().into_owned();
            Ok(result)
        })();
        let cleanup = fs::remove_dir_all(&staging).map_err(DecapodError::IoError);
        let result = result?;
        cleanup?;
        Ok(result)
    }

    /// Explicitly activate Dactyl's verified logical dump/reload replacement.
    ///
    /// Dactyl v0.10.0 currently activates recovered databases in DELETE
    /// journal mode. The archive path is supplied by the operator and must be
    /// distinct and unused; no startup, validation, append, or open path calls
    /// this method.
    pub fn recover_from_dump_reload(
        &mut self,
        preserve_original_at: impl AsRef<Path>,
    ) -> Result<RecoveryResult, DecapodError> {
        let archive = preserve_original_at.as_ref();
        let parent = archive
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        crate::core::fs_permissions::check_directory(
            parent,
            crate::core::fs_permissions::shared_storage(),
        )
        .map_err(DecapodError::IoError)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if fs::metadata(parent)
                .map_err(DecapodError::IoError)?
                .permissions()
                .mode()
                & 0o077
                != 0
            {
                return Err(DecapodError::ValidationError(
                    "STORAGE_RECOVERY_PRIVATE_DIRECTORY_REQUIRED: the pinned Dactyl recovery API creates temporary files beside the active database and does not expose creation modes. Recovery requires a private (0700) database directory; no existing permissions were changed. Shared-directory recovery needs upstream Dactyl maintenance-permission support. Preserve the store and ask its owner to review the recovery location.".to_string()
                ));
            }
        }
        crate::core::fs_permissions::check_storage(archive).map_err(DecapodError::IoError)?;
        let options = RecoveryOptions::new(
            preserve_original_at.as_ref().to_string_lossy(),
            RecoveryJournalMode::Delete,
        );
        let result = self.connection.recover_from_dump_reload(options)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Dactyl has published a newly rebuilt inode within a private
            // directory. Preserve the archived original's intentional safe mode.
            let permissions = fs::metadata(&result.preserved_original_path)
                .map_err(DecapodError::IoError)?
                .permissions();
            fs::set_permissions(
                &result.active_path,
                fs::Permissions::from_mode(permissions.mode() & 0o777),
            )
            .map_err(DecapodError::IoError)?;
        }
        crate::core::fs_permissions::check_storage(Path::new(&result.active_path))
            .map_err(DecapodError::IoError)?;
        Ok(result)
    }

    fn open_route(
        route: dactyl_db::DatastoreRoute,
        access_mode: AccessMode,
        context: Option<dactyl_db::StorageContext>,
        storage_lock: Option<StorageLock>,
    ) -> Result<Self, DecapodError> {
        let connection = Connection::open_with_options_and_context(
            route,
            OpenOptions {
                access_mode,
                lock_timeout: DEFAULT_LOCK_TIMEOUT,
            },
            context,
        )?;
        Ok(Self {
            connection,
            _storage_lock: storage_lock,
        })
    }
}

fn local_storage_lock(
    path: &Path,
    access_mode: AccessMode,
) -> Result<Option<StorageLock>, DecapodError> {
    if path == Path::new(":memory:") {
        return Ok(None);
    }

    if access_mode == AccessMode::ReadWrite
        && let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
    {
        crate::core::fs_permissions::ensure_storage_dir(parent).map_err(DecapodError::IoError)?;
    }

    if access_mode == AccessMode::ReadWrite {
        crate::core::fs_permissions::open_storage_file(
            path,
            fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true),
        )
        .map_err(DecapodError::IoError)?;
    }
    crate::core::fs_permissions::check_storage(path).map_err(DecapodError::IoError)?;
    let mode = if access_mode == AccessMode::ReadOnly {
        StorageLockMode::Shared
    } else {
        StorageLockMode::Exclusive
    };
    let timeout = if std::env::var_os("DECAPOD_VALIDATE_WORKER").is_some() {
        Duration::from_millis(250)
    } else {
        Duration::from_secs(5)
    };
    Ok(Some(StorageLock::acquire(path, mode, timeout)?))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/dactyl_tests.rs"]
mod tests;

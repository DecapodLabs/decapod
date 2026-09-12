use dactyl_db::{AccessMode, AdapterErrorKind, RecoveryJournalMode};
use decapod::core::dactyl::DactylBridge;
use decapod::core::error::{DecapodError, StorageFailureKind};
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{TempDir, tempdir};

fn open_local(path: &Path, access_mode: AccessMode) -> Option<DactylBridge> {
    match DactylBridge::open_local(path, access_mode) {
        Ok(bridge) => Some(bridge),
        Err(DecapodError::DactylError(error))
            if error.adapter_code() == Some("sqlite_runtime_unavailable") =>
        {
            eprintln!("skipping native SQLite maintenance test: {error}");
            None
        }
        Err(error) => panic!("open local Dactyl bridge: {error}"),
    }
}

#[test]
fn healthy_verification_reports_dactyl_metadata() {
    let directory = tempdir().expect("temporary directory");
    let path = directory.path().join("healthy.db");
    let Some(db) = open_local(&path, AccessMode::ReadWrite) else {
        return;
    };
    db.write(
        "create table records (id integer primary key, value text)",
        &[],
    )
    .expect("schema");
    db.write("pragma journal_mode = WAL", &[])
        .expect("WAL mode");
    db.write("pragma user_version = 37", &[])
        .expect("user version");
    db.write("pragma application_id = 4242", &[])
        .expect("application id");
    db.write("insert into records (value) values ('healthy')", &[])
        .expect("application row");

    let report = db.verify_integrity().expect("healthy verification");
    assert_eq!(report.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(report.user_version, 37);
    assert_eq!(report.application_id, 4242);
}

#[test]
fn online_backup_preserves_wal_data_metadata_and_has_no_sidecars() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("source.db");
    let backup_path = directory.path().join("backup.db");
    let Some(source) = open_local(&source_path, AccessMode::ReadWrite) else {
        return;
    };
    source
        .write(
            "create table records (id integer primary key, payload blob)",
            &[],
        )
        .expect("schema");
    source
        .write("pragma journal_mode = WAL", &[])
        .expect("WAL mode");
    source
        .write("pragma user_version = 9", &[])
        .expect("user version");
    source
        .write("pragma application_id = 99", &[])
        .expect("application id");
    source
        .write(
            "insert into records (payload) values (?)",
            &[vec![1_u8, 2, 3].into()],
        )
        .expect("application row");

    let result = source.backup(&backup_path).expect("online backup");
    assert_eq!(result.source_journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(result.destination_journal_mode.to_ascii_lowercase(), "wal");
    assert!(result.bytes > 0);
    assert!(backup_path.is_file());
    assert!(!sidecar(&backup_path, "-wal").exists());
    assert!(!sidecar(&backup_path, "-shm").exists());

    drop(source);
    // A WAL database must be reopened read-write so SQLite can establish its
    // own current sidecars; the backup itself was published without copying
    // source `-wal`/`-shm` files.
    let backup = open_local(&backup_path, AccessMode::ReadWrite).expect("open backup");
    let report = backup.verify_integrity().expect("backup verification");
    assert_eq!(report.journal_mode.to_ascii_lowercase(), "wal");
    assert_eq!(report.user_version, 9);
    assert_eq!(report.application_id, 99);
    let rows = backup
        .read("select payload from records", &[])
        .expect("backup data");
    assert_eq!(rows.as_slice()[0].get_blob(0).expect("payload"), &[1, 2, 3]);
}

#[test]
fn damaged_secondary_index_is_reported_and_explicit_recovery_preserves_data() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("corrupt.db");
    let archive_path = directory.path().join("corrupt.before-recovery.db");
    let (page_size, root_page) = {
        let Some(db) = open_local(&source_path, AccessMode::ReadWrite) else {
            return;
        };
        db.write(
            "create table records (id integer primary key, name text not null, payload blob)",
            &[],
        )
        .expect("schema");
        db.write("create unique index records_name on records(name)", &[])
            .expect("secondary index");
        db.write("pragma journal_mode = WAL", &[])
            .expect("WAL mode");
        db.write("pragma user_version = 9", &[])
            .expect("user version");
        db.write("pragma application_id = 99", &[])
            .expect("application id");
        db.write(
            "insert into records (name, payload) values (?, ?)",
            &["one".into(), vec![1_u8].into()],
        )
        .expect("first row");
        db.write(
            "insert into records (name, payload) values (?, ?)",
            &["two".into(), vec![2_u8].into()],
        )
        .expect("second row");
        db.write("pragma wal_checkpoint(TRUNCATE)", &[])
            .expect("checkpoint");
        let page_size = db
            .read("pragma page_size", &[])
            .expect("page size")
            .as_slice()[0]
            .get_int(0)
            .expect("page size value");
        let root_page = db
            .read(
                "select rootpage from sqlite_schema where type = 'index' and name = 'records_name'",
                &[],
            )
            .expect("index root page")
            .as_slice()[0]
            .get_int(0)
            .expect("root page value");
        (page_size, root_page)
    };

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&source_path)
        .expect("corruption fixture");
    file.seek(SeekFrom::Start((root_page as u64 - 1) * page_size as u64))
        .expect("index page");
    file.write_all(&[0]).expect("damage index page");
    file.sync_all().expect("sync corruption fixture");
    let damaged_bytes = fs::read(&source_path).expect("read damaged fixture");

    let Some(mut db) = open_local(&source_path, AccessMode::ReadWrite) else {
        return;
    };
    let error = db
        .verify_integrity()
        .expect_err("damaged index must fail verification");
    assert_eq!(
        fs::read(&source_path).expect("verification must not repair"),
        damaged_bytes,
        "integrity verification must be read-only"
    );
    assert_eq!(error.storage_failure_kind(), StorageFailureKind::Corrupt);
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Corrupt));
            assert_eq!(error.adapter_code(), Some("corrupt_database"));
        }
        other => panic!("unexpected corruption error: {other}"),
    }

    let recovered = db
        .recover_from_dump_reload(&archive_path)
        .expect("explicit Dactyl recovery");
    assert_eq!(recovered.journal_mode, RecoveryJournalMode::Delete);
    assert_eq!(recovered.user_version, 9);
    assert_eq!(recovered.application_id, 99);
    assert!(archive_path.is_file());

    let health = db.verify_integrity().expect("recovered verification");
    assert_eq!(health.journal_mode.to_ascii_lowercase(), "delete");
    assert_eq!(health.user_version, 9);
    assert_eq!(health.application_id, 99);
    let rows = db
        .read("select name, payload from records order by id", &[])
        .expect("recovered rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.as_slice()[0].get_str("name").expect("name"), "one");
    assert_eq!(
        rows.as_slice()[1].get_blob("payload").expect("payload"),
        &[2]
    );
    assert!(!sidecar(&source_path, "-wal").exists());
    assert!(!sidecar(&source_path, "-shm").exists());
}

#[test]
fn recovery_requires_quiesced_connections_and_preserves_existing_archive() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("source.db");
    let archive_path = directory.path().join("archive.db");
    let Some(mut primary) = open_local(&source_path, AccessMode::ReadWrite) else {
        return;
    };
    primary
        .write("create table records (id integer primary key)", &[])
        .expect("schema");
    let sibling = open_local(&source_path, AccessMode::ReadWrite).expect("sibling connection");
    let error = primary
        .recover_from_dump_reload(&archive_path)
        .expect_err("open sibling must block recovery");
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Conflict));
            assert_eq!(error.adapter_code(), Some("open_connections"));
        }
        other => panic!("unexpected open-connection error: {other}"),
    }
    drop(sibling);

    fs::write(&archive_path, b"operator-owned archive").expect("archive sentinel");
    let error = primary
        .recover_from_dump_reload(&archive_path)
        .expect_err("existing archive must not be overwritten");
    match error {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Conflict));
            assert_eq!(error.adapter_code(), Some("path_exists"));
        }
        other => panic!("unexpected archive error: {other}"),
    }
    assert_eq!(
        fs::read(&archive_path).expect("archive contents"),
        b"operator-owned archive"
    );
    assert_eq!(
        primary
            .read("select count(*) from records", &[])
            .expect("source remains readable")
            .as_slice()[0]
            .get_int(0)
            .expect("count"),
        0
    );
}

#[test]
fn coordination_contention_is_bounded_and_never_cleans_the_sidecar() {
    let directory = tempdir().expect("temporary directory");
    let source_path = directory.path().join("locked.db");
    let sidecar_path = sidecar(&source_path, ".lock");
    fs::File::create(&source_path).expect("database target");
    let external = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&sidecar_path)
        .expect("sidecar");
    external.lock_exclusive().expect("external lock");

    let error = match DactylBridge::open_local(&source_path, AccessMode::ReadWrite) {
        Ok(_) => panic!("coordination lock should block the bridge"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("STORAGE_LOCK_TIMEOUT"));
    assert_eq!(error.storage_failure_kind(), StorageFailureKind::Contention);
    assert!(sidecar_path.exists());
    external.unlock().expect("unlock sidecar");
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

#[test]
fn operator_cli_exposes_verify_backup_and_recovery_without_automatic_repair() {
    let tmp = TempDir::new().expect("temporary project");
    let project = tmp.path();
    let executable = env!("CARGO_BIN_EXE_decapod");
    let init = Command::new(executable)
        .args(["init", "--force", "--no-ci"])
        .current_dir(project)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = Command::new(executable)
        .args(["session", "acquire"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("session");
    assert!(
        session.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
        .expect("session password");

    // macOS temporary directories can have a `/var` -> `/private/var`
    // spelling difference. Dactyl requires the archive and active database to
    // have the same physical parent spelling, so use the resolved data path.
    let data_root = fs::canonicalize(project.join(".decapod/data")).expect("data directory");
    let source_path = data_root.join("decapod.db");
    let Some(db) = open_local(&source_path, AccessMode::ReadWrite) else {
        return;
    };
    db.write(
        "create table records (id integer primary key, value text)",
        &[],
    )
    .expect("schema");
    db.write("insert into records (value) values ('operator')", &[])
        .expect("row");
    drop(db);

    let run = |args: &[&str]| {
        Command::new(executable)
            .args(args)
            .current_dir(project)
            .env("DECAPOD_AGENT_ID", "maintenance-test")
            .env("DECAPOD_SESSION_PASSWORD", &password)
            .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
            .output()
            .expect("maintenance command")
    };

    let verify = run(&["data", "db", "verify"]);
    assert!(
        verify.status.success(),
        "verify failed: {}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let verify_json: serde_json::Value =
        serde_json::from_slice(&verify.stdout).expect("verify JSON");
    assert_eq!(verify_json["status"], "ok");
    assert_eq!(verify_json["diagnostic_status"], "healthy");
    assert_eq!(verify_json["automatic_repair"], false);

    let verify_alias = run(&["data", "database", "verify"]);
    assert!(
        verify_alias.status.success(),
        "database alias failed: {}",
        String::from_utf8_lossy(&verify_alias.stderr)
    );

    let backup_path = project.join("backup.db");
    let backup = run(&[
        "data",
        "db",
        "backup",
        "--destination",
        backup_path.to_str().expect("backup path"),
    ]);
    assert!(
        backup.status.success(),
        "backup failed: {}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup_json: serde_json::Value =
        serde_json::from_slice(&backup.stdout).expect("backup JSON");
    assert_eq!(backup_json["operation"], "backup");
    assert_eq!(backup_json["diagnostic_status"], "healthy");
    assert!(backup_path.is_file());

    let archive_path = data_root.join("recovery-original.db");
    let recovery = run(&[
        "data",
        "db",
        "recover",
        "--preserve-original-at",
        archive_path.to_str().expect("archive path"),
    ]);
    assert!(
        recovery.status.success(),
        "recovery failed: {}",
        String::from_utf8_lossy(&recovery.stderr)
    );
    let recovery_json: serde_json::Value =
        serde_json::from_slice(&recovery.stdout).expect("recovery JSON");
    assert_eq!(recovery_json["operation"], "recovery");
    assert_eq!(recovery_json["diagnostic_status"], "healthy");
    assert_eq!(recovery_json["recovery"]["journal_mode"], "delete");
    assert!(archive_path.is_file());
    assert!(!sidecar(&source_path, "-wal").exists());
    assert!(!sidecar(&source_path, "-shm").exists());
}

#[test]
fn operator_cli_reports_malformed_database_without_repairing_it() {
    let tmp = TempDir::new().expect("temporary project");
    let project = tmp.path();
    let executable = env!("CARGO_BIN_EXE_decapod");
    let init = Command::new(executable)
        .args(["init", "--force", "--no-ci"])
        .current_dir(project)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let session = Command::new(executable)
        .args(["session", "acquire"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("session");
    assert!(
        session.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&session.stderr)
    );
    let password = String::from_utf8_lossy(&session.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Password: ").map(str::to_owned))
        .expect("session password");

    let data_root = fs::canonicalize(project.join(".decapod/data")).expect("data directory");
    let source_path = data_root.join("decapod.db");
    let malformed = b"not a SQLite database";
    fs::write(&source_path, malformed).expect("malformed database");

    let verify = Command::new(executable)
        .args(["data", "db", "verify"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("verify");
    assert!(!verify.status.success(), "malformed database must fail");
    let json: serde_json::Value = serde_json::from_slice(&verify.stdout).expect("diagnostic JSON");
    assert_eq!(json["status"], "corrupt");
    assert_eq!(json["diagnostic_status"], "corrupt");
    assert_eq!(json["failure_code"], "malformed_database");
    assert_eq!(json["automatic_repair"], false);
    assert_eq!(json["operator_action_required"], true);
    assert_eq!(
        fs::read(&source_path).expect("database remains unchanged"),
        malformed
    );

    let validation = Command::new(executable)
        .args(["validate", "--format", "json"])
        .current_dir(project)
        .env("DECAPOD_AGENT_ID", "maintenance-corruption-test")
        .env("DECAPOD_SESSION_PASSWORD", &password)
        .env("DECAPOD_VALIDATE_SKIP_GIT_GATES", "1")
        .output()
        .expect("ordinary validation");
    assert!(
        !validation.status.success(),
        "ordinary validation must surface malformed storage rather than repair it"
    );
    assert_eq!(
        fs::read(&source_path).expect("validation must not repair the database"),
        malformed
    );
}

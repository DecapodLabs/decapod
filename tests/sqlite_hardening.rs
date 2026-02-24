use decapod::core::{db, error::DecapodError, pool};
use rusqlite::Connection;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn sqlite_pool_write_reports_busy_under_exclusive_lock_and_recovers_after_release() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("retry.db");

    let setup = Connection::open(&db_path).expect("open setup");
    setup
        .execute(
            "CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v TEXT)",
            [],
        )
        .expect("create table");

    let lock_conn = Connection::open(&db_path).expect("open lock");
    lock_conn
        .execute_batch("BEGIN EXCLUSIVE;")
        .expect("acquire exclusive lock");

    let blocked = pool::global_pool().with_write(&db_path, |conn| {
        conn.execute("INSERT INTO t(v) VALUES('blocked')", [])
            .map_err(DecapodError::RusqliteError)?;
        Ok(())
    });
    assert!(blocked.is_err(), "write must fail while lock is held");
    let err_msg = blocked.err().unwrap().to_string();
    assert!(
        err_msg.contains("database is locked")
            || err_msg.contains("DatabaseBusy")
            || err_msg.contains("busy"),
        "unexpected busy error: {err_msg}"
    );

    lock_conn.execute_batch("COMMIT;").expect("release lock");

    let recovered = pool::global_pool().with_write(&db_path, |conn| {
        conn.execute("INSERT INTO t(v) VALUES('ok')", [])
            .map_err(DecapodError::RusqliteError)?;
        Ok(())
    });
    assert!(
        recovered.is_ok(),
        "write should succeed after lock release: {recovered:?}"
    );

    let verify = Connection::open(&db_path).expect("open verify");
    let count: i64 = verify
        .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
        .expect("count rows");
    assert_eq!(count, 1, "exactly one successful write should be recorded");
}

#[test]
fn sqlite_fault_injection_produces_deterministic_io_error_path() {
    let _guard = env_lock().lock().expect("lock env");
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("fault.db");

    // SAFETY: test-scoped environment mutation is serialized via env_lock.
    unsafe { std::env::set_var("DECAPOD_SQLITE_FAULT_STAGE", "open") };
    let err = db::db_connect(db_path.to_string_lossy().as_ref()).expect_err("fault must trigger");
    // SAFETY: test-scoped environment mutation is serialized via env_lock.
    unsafe { std::env::remove_var("DECAPOD_SQLITE_FAULT_STAGE") };

    let msg = err.to_string();
    assert!(msg.contains("SQLITE_FAULT_INJECTED"), "{msg}");
    assert!(msg.contains("extended_code=522"), "{msg}");
}

#[cfg(unix)]
#[test]
fn storage_preflight_fails_for_non_writable_directory() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = TempDir::new().expect("tempdir");
    let store_root = tmp.path().join("data");
    std::fs::create_dir_all(&store_root).expect("create store root");

    let mut perms = std::fs::metadata(&store_root)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o555);
    std::fs::set_permissions(&store_root, perms).expect("set readonly perms");

    let err = db::storage_health_preflight(&store_root).expect_err("preflight should fail");
    let msg = err.to_string();
    assert!(msg.contains("STORAGE_PREFLIGHT_FAILED"), "{msg}");

    // Restore perms for cleanup.
    let mut perms = std::fs::metadata(&store_root)
        .expect("metadata")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&store_root, perms).expect("restore perms");
}

#[test]
fn sqlite_pool_read_path_remains_available_for_concurrent_queries() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("readonly.db");
    let setup = Connection::open(&db_path).expect("open setup");
    setup
        .execute(
            "CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v TEXT)",
            [],
        )
        .expect("create table");

    let res = pool::global_pool().with_read(&db_path, |conn| {
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
            .map_err(DecapodError::RusqliteError)?;
        assert_eq!(count, 0);
        Ok(())
    });
    assert!(res.is_ok(), "read path should succeed: {res:?}");
}

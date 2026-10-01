use super::*;
use crate::core::db::Connection;
use crate::core::error::DecapodError;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::tempdir;

#[test]
fn operation_lock_timeout_is_bounded() {
    let directory = tempdir().expect("temporary directory");
    let db_path = directory.path().join("operation-lock-timeout.db");
    let setup = Connection::open(&db_path).expect("open setup");
    setup
        .execute(
            "CREATE TABLE IF NOT EXISTS t(id INTEGER PRIMARY KEY, v TEXT)",
            [],
        )
        .expect("create table");
    drop(setup);

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let holder_path = db_path.clone();
    let holder_entered = Arc::clone(&entered);
    let holder_release = Arc::clone(&release);
    let holder = thread::spawn(move || {
        global_pool().with_write(&holder_path, |_conn| {
            holder_entered.wait();
            holder_release.wait();
            Ok::<_, DecapodError>(())
        })
    });

    entered.wait();
    let blocked = global_pool().with_write(&db_path, |_conn| Ok::<_, DecapodError>(()));
    assert!(
        blocked.is_err(),
        "second operation must not wait indefinitely"
    );
    let message = blocked.unwrap_err().to_string();
    assert!(message.contains("STORAGE_POOL_LOCK_TIMEOUT"), "{message}");

    release.wait();
    holder
        .join()
        .expect("holder thread")
        .expect("holder operation");
}

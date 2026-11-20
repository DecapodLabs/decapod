//! Repository-context integration checks for the one-shot local migration.
use decapod::core::db::Connection;
use decapod::core::{schemas, storage_scope};
use std::fs;
use std::process::Command;

#[test]
fn local_scope_has_one_row_and_no_extra_index() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let first = storage_scope::ensure(&conn, &data).unwrap();
    assert_eq!(storage_scope::ensure(&conn, &data).unwrap(), first);
    assert_eq!(storage_scope::read(&conn).unwrap(), first);
    assert!(!first.origin_observed);
    assert_eq!(first.owner_kind, 0);
    assert_eq!(first.organization_id, None);
    let count: i64 = conn
        .query_row("SELECT count(*) FROM repository_scope", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let indexes: i64 = conn
        .query_row(
            "SELECT count(*) FROM pragma_index_list('repository_scope')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        indexes, 0,
        "singleton integer PK should use SQLite rowid without a second B-tree"
    );
    assert!(
        conn.execute(
            "INSERT INTO repository_scope(repository_key,version,stable_id,provider,owner_kind)
         VALUES(2,1,'01234567890123456789012345',0,0)",
            [],
        )
        .is_err()
    );
}

#[test]
fn observed_remote_never_acts_as_an_org_grant_or_automatic_rebinding() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    assert!(
        Command::new("git")
            .current_dir(root)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .current_dir(root)
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/OwnerA/RepoA.git"
            ])
            .status()
            .unwrap()
            .success()
    );
    let data = root.join(".decapod/data");
    fs::create_dir_all(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    let first = storage_scope::ensure(&conn, &data).unwrap();
    assert!(first.origin_observed);
    assert_eq!(first.owner_name.as_deref(), Some("OwnerA"));
    assert_eq!(first.repository_name.as_deref(), Some("RepoA"));
    assert_eq!(
        first.organization_id, None,
        "an observed Git path cannot establish organization membership"
    );
    assert_eq!(
        first.owner_kind, 0,
        "Git remote cannot distinguish user from organization"
    );
    assert!(
        Command::new("git")
            .current_dir(root)
            .args([
                "remote",
                "set-url",
                "origin",
                "https://github.com/OwnerB/RepoB.git"
            ])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(first, storage_scope::ensure(&conn, &data).unwrap());
}

#[test]
fn invalid_existing_scope_is_not_repaired() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let conn = Connection::open(data.join(schemas::LOCAL_DB_NAME)).unwrap();
    storage_scope::ensure(&conn, &data).unwrap();
    conn.execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    conn.execute("UPDATE repository_scope SET version=77", [])
        .unwrap();
    assert!(storage_scope::read(&conn).is_err());
    assert!(storage_scope::ensure(&conn, &data).is_err());
}

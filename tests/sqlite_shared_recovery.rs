//! Consumer proof for the released secure recovery primitive. No live datastore
//! or global umask is changed: permissive creation runs only in a child process.
#![cfg(any(target_os = "linux", target_os = "android"))]
use dactyl_db::{AccessMode, AdapterErrorKind};
use decapod::core::dactyl::DactylBridge;
use decapod::core::error::DecapodError;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().mode() & 0o7777
}
fn assert_code(error: DecapodError, expected: &str) {
    match error {
        DecapodError::DactylError(error) => assert_eq!(error.adapter_code(), Some(expected)),
        other => panic!("unexpected recovery error: {other}"),
    }
}

#[test]
fn shared_recovery_preserves_group_mode_original_and_no_clobber_under_umask_zero() {
    if std::env::var_os("DECAPOD_RECOVERY_CHILD").is_none() {
        let output = Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "recovery-child"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "shared_recovery_preserves_group_mode_original_and_no_clobber_under_umask_zero",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("DECAPOD_RECOVERY_CHILD", "1")
            .env("DECAPOD_STORAGE_SHARED_GROUP", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    for source_mode in [0o600, 0o640, 0o660] {
        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o2770)).unwrap();
        let source = temp.path().join("source.db");
        let archive = temp.path().join("original.db");
        let mut db = DactylBridge::open_local(&source, AccessMode::ReadWrite).unwrap();
        db.write(
            "create table records (id integer primary key, value text)",
            &[],
        )
        .unwrap();
        db.write("insert into records values (1, 'shared')", &[])
            .unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(source_mode)).unwrap();
        let original = fs::metadata(&source).unwrap();
        let bytes = fs::read(&source).unwrap();

        // No-clobber failure must leave the original and operator file intact.
        fs::write(&archive, b"operator archive").unwrap();
        fs::set_permissions(&archive, fs::Permissions::from_mode(0o660)).unwrap();
        assert_code(
            db.recover_from_dump_reload(&archive).unwrap_err(),
            "path_exists",
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(fs::read(&archive).unwrap(), b"operator archive");
        fs::remove_file(&archive).unwrap();

        let recovered = db.recover_from_dump_reload(&archive).unwrap();
        assert_eq!(
            recovered.journal_mode,
            dactyl_db::RecoveryJournalMode::Delete
        );
        assert_eq!(mode(temp.path()), 0o2770);
        for path in [&source, &archive] {
            let metadata = fs::metadata(path).unwrap();
            assert_eq!(metadata.uid(), original.uid());
            assert_eq!(metadata.gid(), original.gid());
            assert_eq!(mode(path), source_mode);
        }
        assert_eq!(fs::metadata(&archive).unwrap().ino(), original.ino());
        assert_ne!(fs::metadata(&source).unwrap().ino(), original.ino());
        assert_eq!(fs::read(&archive).unwrap(), bytes);
        assert_eq!(
            db.read("select value from records", &[])
                .unwrap()
                .as_slice()[0]
                .get_str(0)
                .unwrap(),
            "shared"
        );
        db.write("insert into records values (2, 'after recovery')", &[])
            .unwrap();
        assert_eq!(
            db.read("select count(*) from records", &[])
                .unwrap()
                .as_slice()[0]
                .get_int(0)
                .unwrap(),
            2
        );
        assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".dactyl-recovery-")
        }));
    }
}

#[test]
fn recovery_metadata_refusal_reaches_the_consumer_without_changing_source() {
    let temp = tempfile::tempdir().unwrap();
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let source = temp.path().join("source.db");
    let archive = temp.path().join("original.db");
    let mut db = DactylBridge::open_local(&source, AccessMode::ReadWrite).unwrap();
    db.write("create table records (id integer)", &[]).unwrap();
    // A real access ACL, using the mapped UID so this works in a user namespace.
    let mut acl = 2_u32.to_le_bytes().to_vec();
    for (tag, permissions, uid) in [
        (1_u16, 6_u16, u32::MAX),
        (2, 4, fs::metadata(&source).unwrap().uid()),
        (4, 4, u32::MAX),
        (16, 4, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        acl.extend_from_slice(&tag.to_le_bytes());
        acl.extend_from_slice(&permissions.to_le_bytes());
        acl.extend_from_slice(&uid.to_le_bytes());
    }
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(source.as_os_str().as_bytes()).unwrap();
    let result = unsafe {
        libc::setxattr(
            path.as_ptr(),
            c"system.posix_acl_access".as_ptr(),
            acl.as_ptr().cast(),
            acl.len(),
            0,
        )
    };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EOPNOTSUPP) {
            eprintln!(
                "ACL fixture unavailable on this filesystem; no ACL-preservation pass claimed"
            );
            return;
        }
        panic!("cannot create test ACL: {error}");
    }
    let bytes = fs::read(&source).unwrap();
    let metadata = fs::metadata(&source).unwrap();
    match db.recover_from_dump_reload(&archive).unwrap_err() {
        DecapodError::DactylError(error) => {
            assert_eq!(error.adapter_kind(), Some(AdapterErrorKind::Capability));
            assert_eq!(error.adapter_code(), Some("recovery_metadata_unsupported"));
        }
        other => panic!("unexpected metadata error: {other}"),
    }
    assert_eq!(fs::read(&source).unwrap(), bytes);
    assert_eq!(fs::metadata(&source).unwrap().mode(), metadata.mode());
    assert_eq!(fs::metadata(&source).unwrap().ino(), metadata.ino());
    assert!(!archive.exists());
    assert!(!fs::read_dir(temp.path()).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dactyl-recovery-")
    }));
}

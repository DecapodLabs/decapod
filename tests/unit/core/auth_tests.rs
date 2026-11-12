use super::*;

#[test]
fn oversized_machine_sessions_are_refused_without_replacing_private_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session_token.json");
    let mut session = CloudSession {
        access_token: "original-access".into(),
        refresh_token: Some("original-refresh".into()),
        session_id: None,
        expires_at: None,
    };
    store_machine_session_at(&session, &path).unwrap();
    let before = fs::read(&path).unwrap();
    session.access_token = "a".repeat(MAX_SESSION_FILE_BYTES);
    assert!(store_machine_session_at(&session, &path).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    let oversized = vec![b' '; MAX_SESSION_FILE_BYTES + 1];
    fs::write(&path, &oversized).unwrap();
    let error = load_machine_session_at(&path).unwrap_err();
    assert!(error.to_string().contains("size bound"));
    assert_eq!(fs::read(&path).unwrap(), oversized);
}

#[cfg(unix)]
#[test]
fn dangling_machine_session_symlinks_are_refused() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session_token.json");
    std::os::unix::fs::symlink(directory.path().join("absent.json"), &path).unwrap();
    assert!(load_machine_session_at(&path).is_err());
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(feature = "supabase-cloud")]
#[test]
fn managed_provider_token_is_atomic_scoped_and_kept_out_of_data_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let store =
        CloudSessionStore::for_service_in(directory.path(), "https://one.example.test").unwrap();
    let other =
        CloudSessionStore::for_service_in(directory.path(), "https://two.example.test").unwrap();
    let mut session = CloudSession {
        access_token: "user-a".into(),
        refresh_token: Some("refresh-a".into()),
        session_id: None,
        expires_at: None,
    };
    store
        .store_managed_session(&session, Some("scm-provider-secret"), false)
        .unwrap();
    assert_eq!(
        store.load_provider_token().unwrap().as_deref(),
        Some("scm-provider-secret")
    );
    assert_eq!(other.load_provider_token().unwrap(), None);
    let data = store.load_machine_session().unwrap().unwrap();
    assert_eq!(data.token, "user-a");
    assert!(!format!("{data:?}").contains("scm-provider-secret"));
    session.access_token = "rotated-user-a".into();
    store.store_managed_session(&session, None, true).unwrap();
    assert_eq!(
        store.load_provider_token().unwrap().as_deref(),
        Some("scm-provider-secret")
    );
    session.access_token = "new-user-b".into();
    store.store_managed_session(&session, None, false).unwrap();
    assert_eq!(store.load_provider_token().unwrap(), None);
    assert!(
        store
            .store_managed_session(&session, Some("invalid\nprovider"), false)
            .is_err()
    );
    assert_eq!(
        store.load_machine_session().unwrap().unwrap().token,
        "new-user-b"
    );
}

#[test]
fn scoped_machine_sessions_never_reuse_legacy_or_other_service_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join("session_token.json");
    let session = CloudSession {
        access_token: "synthetic-legacy-access".to_string(),
        refresh_token: Some("synthetic-legacy-refresh".to_string()),
        session_id: Some("synthetic-session".to_string()),
        expires_at: Some("2099-01-01T00:00:00Z".to_string()),
    };
    store_machine_session_at(&session, &legacy).unwrap();
    let first =
        CloudSessionStore::for_service_in(directory.path(), "https://one.example.test").unwrap();
    let second =
        CloudSessionStore::for_service_in(directory.path(), "https://two.example.test").unwrap();
    assert!(first.load_machine_session().unwrap().is_none());
    assert!(second.load_machine_session().unwrap().is_none());
    first
        .store_machine_session(&CloudSession {
            access_token: "synthetic-one-access".to_string(),
            ..session.clone()
        })
        .unwrap();
    assert_eq!(
        first.load_machine_session().unwrap().unwrap().token,
        "synthetic-one-access"
    );
    assert!(second.load_machine_session().unwrap().is_none());
    assert_eq!(
        load_machine_session_at(&legacy).unwrap().unwrap().token,
        "synthetic-legacy-access"
    );
    let same =
        CloudSessionStore::for_service_in(directory.path(), "https://one.example.test/").unwrap();
    assert_eq!(
        same.load_machine_session().unwrap().unwrap().token,
        "synthetic-one-access"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&first.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn cloud_credential_debug_redacts_access_refresh_and_session_material() {
    let credential = CloudCredential {
        token: "synthetic-access-secret".to_string(),
        source: CredentialSource::MachineFile,
        refresh_token: Some("synthetic-refresh-secret".to_string()),
        session_id: Some("synthetic-session-secret".to_string()),
        expires_at: None,
    };
    let debug = format!("{credential:?}");
    assert!(!debug.contains("synthetic-"));
    assert!(debug.contains("REDACTED"));
}

#[test]
fn session_temporary_is_private_before_writing_and_never_reuses_an_existing_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.tmp");
    let file = create_private_session_file(&path).unwrap();
    assert_eq!(file.metadata().unwrap().len(), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    }
    drop(file);
    std::fs::write(&path, "existing-sentinel").unwrap();
    assert!(create_private_session_file(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "existing-sentinel");
}

#[cfg(unix)]
#[test]
fn session_creation_and_replacement_are_private_with_permissive_umask() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var_os("DECAPOD_SESSION_PERMISSION_CHILD").is_none() {
        let output = std::process::Command::new("sh")
            .args(["-c", "umask 000; exec \"$@\"", "session-child"])
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "core::auth::tests::session_creation_and_replacement_are_private_with_permissive_umask", "--nocapture", "--test-threads=1"])
            .env("DECAPOD_SESSION_PERMISSION_CHILD", "1")
            .env_remove(crate::core::fs_permissions::SHARED_STORAGE_ENV)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.path().join("config/sessions/session.json");
    let session = CloudSession {
        access_token: "synthetic-test-value".to_string(),
        refresh_token: None,
        session_id: None,
        expires_at: None,
    };
    for _ in 0..2 {
        store_machine_session_at(&session, &path).unwrap();
    }
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

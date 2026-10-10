use super::*;

#[test]
fn typesafe_key_prefers_environment_over_machine_file() {
    assert_eq!(
        resolve_typesafe_api_key(Some("environment-key"), Some("machine-key")).unwrap(),
        Some("environment-key".to_string())
    );
}

#[test]
fn typesafe_key_falls_back_to_machine_file() {
    assert_eq!(
        resolve_typesafe_api_key(None, Some("machine-key")).unwrap(),
        Some("machine-key".to_string())
    );
    assert_eq!(resolve_typesafe_api_key(None, None).unwrap(), None);
}

#[test]
fn typesafe_key_rejects_whitespace_without_disclosing_value() {
    let error = resolve_typesafe_api_key(Some("bad key"), None).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("TYPESAFE_API_KEY"));
    assert!(!message.contains("bad key"));
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

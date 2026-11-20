use super::*;
use std::process::Command;

const ORIGIN: &str = "https://managed.example.test";
const REPOSITORY: &str = "DecapodLabs/decapod";

struct Fixture {
    root: tempfile::TempDir,
    project: PathBuf,
    machine: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        fs::create_dir(&project).unwrap();
        git(&project, &["init", "--quiet"]);
        let machine = root.path().join("machine");
        Self {
            root,
            project,
            machine,
        }
    }

    fn store(&self) -> SessionStore {
        SessionStore::for_project_in(&self.machine, ORIGIN, REPOSITORY, &self.project).unwrap()
    }
}

fn git(project: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(project)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ready() -> SessionState {
    SessionState {
        phase: Phase::Ready,
        session: Some(Session {
            access_token: "synthetic-access-secret".into(),
            refresh_token: Some("synthetic-refresh-secret".into()),
            expires_at: 4_000_000_000,
        }),
        bootstrap: None,
    }
}

fn handoff() -> Bootstrap {
    Bootstrap {
        flow_id: Some("a3ef332f-ab1d-4d1e-a19f-bd18992364b8".into()),
        browser_url: Some(
            "https://managed.example.test/handoff?flow=synthetic-browser-secret".into(),
        ),
        expires_at: Some(4_000_000_000),
        code: Some("synthetic-auth-code-secret".into()),
        ..Bootstrap::new().unwrap()
    }
}

fn state_at(phase: Phase) -> SessionState {
    let mut state = SessionState {
        phase,
        ..SessionState::default()
    };
    match phase {
        Phase::Empty => (),
        Phase::Preparing => state.bootstrap = Some(Bootstrap::new().unwrap()),
        Phase::Waiting | Phase::ExchangingBootstrap => {
            let mut bootstrap = handoff();
            bootstrap.code = None;
            state.bootstrap = Some(bootstrap);
        }
        Phase::HaveCode | Phase::ExchangingSession => state.bootstrap = Some(handoff()),
        Phase::CheckingReadiness => {
            let mut bootstrap = handoff();
            bootstrap.code = None;
            state.bootstrap = Some(bootstrap);
            state.session = ready().session;
        }
        Phase::Ready | Phase::Refreshing => state.session = ready().session,
        Phase::RestartRequired => {
            state.session = ready().session;
            state.bootstrap = Some(handoff());
        }
    }
    state
}

#[test]
fn scope_isolates_origins_repositories_and_projects() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    let same = SessionStore::for_project_in(&fixture.machine, ORIGIN, REPOSITORY, &fixture.project)
        .unwrap();
    assert_eq!(same.lock(Duration::ZERO).unwrap().state(), &ready());
    for (origin, repository) in [
        ("https://other.example.test", REPOSITORY),
        (ORIGIN, "DecapodLabs/another"),
        (ORIGIN, "decapodlabs/DECAPOD"),
    ] {
        let other =
            SessionStore::for_project_in(&fixture.machine, origin, repository, &fixture.project)
                .unwrap();
        assert_eq!(
            other.lock(Duration::ZERO).unwrap().state().phase,
            Phase::Empty
        );
        assert_ne!(store.directory, other.directory);
    }
    let other_project = fixture.root.path().join("other-project");
    fs::create_dir(&other_project).unwrap();
    git(&other_project, &["init", "--quiet"]);
    let other =
        SessionStore::for_project_in(&fixture.machine, ORIGIN, REPOSITORY, &other_project).unwrap();
    assert_eq!(
        other.lock(Duration::ZERO).unwrap().state().phase,
        Phase::Empty
    );
    assert_ne!(store.directory, other.directory);
}

#[test]
fn sibling_worktrees_share_one_bound_session() {
    let fixture = Fixture::new();
    git(
        &fixture.project,
        &[
            "-c",
            "user.name=Custody Test",
            "-c",
            "user.email=custody@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let sibling = fixture.root.path().join("worktree");
    git(
        &fixture.project,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "custody-sibling",
            sibling.to_str().unwrap(),
        ],
    );
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    let other =
        SessionStore::for_project_in(&fixture.machine, ORIGIN, REPOSITORY, &sibling).unwrap();
    assert_eq!(other.directory, store.directory);
    assert_eq!(other.lock(Duration::ZERO).unwrap().state(), &ready());
}

#[test]
fn copied_credentials_cannot_change_their_bound_scope() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    let other = SessionStore::for_project_in(
        &fixture.machine,
        "https://other.example.test",
        REPOSITORY,
        &fixture.project,
    )
    .unwrap();
    drop(other.lock(Duration::ZERO).unwrap());
    fs::copy(
        store.directory.join("state.json"),
        other.directory.join("state.json"),
    )
    .unwrap();
    let error = other.lock(Duration::ZERO).unwrap_err().to_string();
    assert!(error.contains("binding"));
    assert!(!error.contains("synthetic-"));
    assert_eq!(store.lock(Duration::ZERO).unwrap().state(), &ready());
}

#[test]
fn credentials_cannot_be_placed_inside_any_repository() {
    let fixture = Fixture::new();
    let inside = fixture.project.join("machine");
    assert!(SessionStore::for_project_in(&inside, ORIGIN, REPOSITORY, &fixture.project).is_err());
    assert!(!inside.exists());
    let other = fixture.root.path().join("unrelated-repository");
    fs::create_dir(&other).unwrap();
    git(&other, &["init", "--quiet"]);
    assert!(
        SessionStore::for_project_in(&other.join("machine"), ORIGIN, REPOSITORY, &fixture.project)
            .is_err()
    );
    assert!(
        SessionStore::for_project_in(
            &fixture.root.path().join(".git/private"),
            ORIGIN,
            REPOSITORY,
            &fixture.project
        )
        .is_err()
    );
    assert!(
        SessionStore::for_project_in(
            Path::new("relative-machine"),
            ORIGIN,
            REPOSITORY,
            &fixture.project
        )
        .is_err()
    );
}

#[test]
fn nonexistent_parent_traversal_cannot_redirect_custody_into_a_repository() {
    let fixture = Fixture::new();
    let outside = fixture.root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let traversal = outside.join("new/../../project/machine");
    assert!(
        SessionStore::for_project_in(&traversal, ORIGIN, REPOSITORY, &fixture.project).is_err()
    );
    assert!(
        !outside.join("new").exists(),
        "rejected path created intermediate ancestry"
    );
    assert!(
        !fixture.project.join("machine").exists(),
        "rejected path wrote private custody into Git"
    );
    assert!(
        !fixture.machine.exists(),
        "rejected path wrote a custody lock or state"
    );
}

#[cfg(unix)]
#[test]
fn repository_storage_alias_is_rejected_before_credentials_are_written() {
    let fixture = Fixture::new();
    let alias = fixture.root.path().join("alias");
    std::os::unix::fs::symlink(&fixture.project, &alias).unwrap();
    assert!(
        SessionStore::for_project_in(&alias.join("machine"), ORIGIN, REPOSITORY, &fixture.project)
            .is_err()
    );
    assert!(!fixture.project.join("machine").exists());
}

#[test]
fn interrupted_non_replayable_operations_require_an_explicit_restart() {
    for phase in [
        Phase::Preparing,
        Phase::ExchangingBootstrap,
        Phase::ExchangingSession,
        Phase::Refreshing,
    ] {
        let fixture = Fixture::new();
        let store = fixture.store();
        let state = state_at(phase);
        store
            .lock(Duration::ZERO)
            .unwrap()
            .replace(state.clone())
            .unwrap();
        let mut resumed = store.lock(Duration::ZERO).unwrap();
        assert_eq!(resumed.state().phase, Phase::RestartRequired);
        assert_eq!(resumed.state().session, state.session);
        assert_eq!(resumed.state().bootstrap, state.bootstrap);
        resumed.clear_for_restart().unwrap();
        assert_eq!(resumed.state(), &SessionState::default());
        drop(resumed);
        assert_eq!(
            store.lock(Duration::ZERO).unwrap().state(),
            &SessionState::default()
        );
    }
}

#[test]
fn accepted_responses_and_safe_readiness_checks_remain_resumable() {
    for phase in [
        Phase::Waiting,
        Phase::HaveCode,
        Phase::CheckingReadiness,
        Phase::Ready,
        Phase::RestartRequired,
    ] {
        let fixture = Fixture::new();
        let store = fixture.store();
        let state = state_at(phase);
        store
            .lock(Duration::ZERO)
            .unwrap()
            .replace(state.clone())
            .unwrap();
        assert_eq!(store.lock(Duration::ZERO).unwrap().state(), &state);
    }
}

#[test]
fn state_replace_is_complete_and_rejects_incomplete_fields() {
    let fixture = Fixture::new();
    let store = fixture.store();
    let mut guard = store.lock(Duration::ZERO).unwrap();
    assert_eq!(guard.state(), &SessionState::default());
    guard.replace(ready()).unwrap();
    assert!(
        guard
            .replace(SessionState {
                phase: Phase::Ready,
                ..SessionState::default()
            })
            .is_err()
    );
    assert!(
        guard
            .replace(SessionState {
                phase: Phase::HaveCode,
                bootstrap: Some(Bootstrap::new().unwrap()),
                session: None
            })
            .is_err()
    );
    assert_eq!(guard.state(), &ready());
    drop(guard);
    assert_eq!(store.lock(Duration::ZERO).unwrap().state(), &ready());
    let names: Vec<_> = fs::read_dir(&store.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2);
    assert!(
        names
            .iter()
            .all(|name| name == "state.json" || name == "session.lock")
    );
}

#[test]
fn every_phase_requires_its_complete_custody_shape_before_orchestration() {
    for phase in [
        Phase::Empty,
        Phase::Preparing,
        Phase::Waiting,
        Phase::ExchangingBootstrap,
        Phase::HaveCode,
        Phase::ExchangingSession,
        Phase::CheckingReadiness,
        Phase::Ready,
        Phase::Refreshing,
    ] {
        let fixture = Fixture::new();
        let store = fixture.store();
        let mut guard = store.lock(Duration::ZERO).unwrap();
        let valid = state_at(phase);
        guard.replace(valid.clone()).unwrap();
        let mut invalid = valid.clone();
        match phase {
            Phase::Empty => invalid.session = ready().session,
            Phase::Preparing => {
                invalid.bootstrap.as_mut().unwrap().flow_id = Some("partial-flow".into())
            }
            Phase::Waiting | Phase::ExchangingBootstrap => {
                invalid.bootstrap.as_mut().unwrap().code = Some("unexpected-auth-code".into())
            }
            Phase::HaveCode | Phase::ExchangingSession => {
                invalid.bootstrap.as_mut().unwrap().code = None
            }
            Phase::CheckingReadiness => invalid.session = None,
            Phase::Ready => invalid.bootstrap = Some(handoff()),
            Phase::Refreshing => invalid.session.as_mut().unwrap().refresh_token = None,
            Phase::RestartRequired => unreachable!(),
        }
        assert!(
            guard.replace(invalid).is_err(),
            "accepted malformed {phase:?}"
        );
        assert_eq!(guard.state(), &valid);
        if valid
            .bootstrap
            .as_ref()
            .is_some_and(|bootstrap| bootstrap.flow_id.is_some())
        {
            let mut incomplete = valid.clone();
            incomplete.bootstrap.as_mut().unwrap().browser_url = None;
            assert!(
                guard.replace(incomplete).is_err(),
                "accepted incomplete handoff for {phase:?}"
            );
        }
    }
}

#[test]
fn invalid_or_oversized_state_is_preserved_and_never_treated_as_missing() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    let path = store.directory.join("state.json");
    for bytes in [
        b"{synthetic-malformed-secret".to_vec(),
        vec![b'x'; MAX_RECORD_BYTES as usize + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        let message = store.lock(Duration::ZERO).unwrap_err().to_string();
        assert!(!message.contains("synthetic-malformed-secret"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn unknown_record_versions_fail_without_rewriting_evidence() {
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    let path = store.directory.join("state.json");
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["version"] = serde_json::json!(99);
    let bytes = serde_json::to_vec(&record).unwrap();
    fs::write(&path, &bytes).unwrap();
    assert!(store.lock(Duration::ZERO).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn credentials_reject_readable_or_writable_shared_permissions() {
    use std::os::unix::fs::PermissionsExt;
    for mode in [0o644, 0o640, 0o666, 0o660] {
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .lock(Duration::ZERO)
            .unwrap()
            .replace(ready())
            .unwrap();
        let path = store.directory.join("state.json");
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert!(store.lock(Duration::ZERO).is_err());
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            mode
        );
    }
    let fixture = Fixture::new();
    let store = fixture.store();
    store
        .lock(Duration::ZERO)
        .unwrap()
        .replace(ready())
        .unwrap();
    fs::set_permissions(&store.directory, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(store.lock(Duration::ZERO).is_err());
    assert_eq!(
        fs::metadata(&store.directory).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[test]
fn credential_and_lock_symlinks_and_hardlinks_are_rejected() {
    for file_name in ["state.json", "session.lock"] {
        let fixture = Fixture::new();
        let store = fixture.store();
        store
            .lock(Duration::ZERO)
            .unwrap()
            .replace(ready())
            .unwrap();
        let path = store.directory.join(file_name);
        let preserved = fixture.root.path().join("preserved");
        fs::rename(&path, &preserved).unwrap();
        let bytes = fs::read(&preserved).unwrap();
        std::os::unix::fs::symlink(&preserved, &path).unwrap();
        assert!(store.lock(Duration::ZERO).is_err());
        assert_eq!(fs::read(&preserved).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
        fs::hard_link(&preserved, &path).unwrap();
        assert!(store.lock(Duration::ZERO).is_err());
        assert_eq!(fs::read(&preserved).unwrap(), bytes);
    }
}

#[cfg(unix)]
#[test]
fn atomic_replacement_preserves_a_private_stable_lock_inode() {
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let store = fixture.store();
    let mut guard = store.lock(Duration::ZERO).unwrap();
    let before = fs::metadata(store.directory.join("session.lock")).unwrap();
    assert_eq!(before.mode() & 0o777, 0o600);
    guard.replace(ready()).unwrap();
    guard.clear_for_restart().unwrap();
    let after = fs::metadata(store.directory.join("session.lock")).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    assert_eq!(
        fs::metadata(store.directory.join("state.json"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&store.directory).unwrap().mode() & 0o777,
        0o700
    );
}

#[test]
fn separate_processes_serialize_one_session_and_release_the_lock_on_exit() {
    const PROJECT: &str = "DECAPOD_CLOUD_CUSTODY_TEST_PROJECT";
    const MACHINE: &str = "DECAPOD_CLOUD_CUSTODY_TEST_MACHINE";
    const EXPECTED: &str = "DECAPOD_CLOUD_CUSTODY_TEST_EXPECTED";
    if let Some(project) = std::env::var_os(PROJECT) {
        let store = SessionStore::for_project_in(
            &PathBuf::from(std::env::var_os(MACHINE).unwrap()),
            ORIGIN,
            REPOSITORY,
            &PathBuf::from(project),
        )
        .unwrap();
        let result = store.lock(Duration::from_millis(30));
        assert_eq!(
            result.is_ok(),
            std::env::var(EXPECTED).unwrap() == "available"
        );
        return;
    }
    let fixture = Fixture::new();
    let store = fixture.store();
    let guard = store.lock(Duration::ZERO).unwrap();
    let child = |expected: &str| {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "core::cloud_session::tests::separate_processes_serialize_one_session_and_release_the_lock_on_exit", "--nocapture", "--test-threads=1"])
            .env(PROJECT, &fixture.project).env(MACHINE, &fixture.machine).env(EXPECTED, expected)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    };
    child("busy");
    drop(guard);
    child("available");
    assert!(store.lock(Duration::ZERO).is_ok());
}

#[test]
fn bootstrap_secrets_and_challenges_use_the_managed_wire_encoding() {
    let mut bootstrap = Bootstrap::new().unwrap();
    assert!(challenge_secret(&bootstrap.pkce_verifier));
    assert!(challenge_secret(&bootstrap.poll_secret));
    assert_ne!(bootstrap.pkce_verifier, bootstrap.poll_secret);
    assert_ne!(bootstrap.pkce_challenge(), bootstrap.pkce_verifier);
    assert_ne!(bootstrap.poll_challenge(), bootstrap.poll_secret);
    // PKCE S256 example from RFC 7636, Appendix B.
    bootstrap.pkce_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into();
    assert_eq!(
        bootstrap.pkce_challenge(),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
    assert!(!challenge_secret(&"A".repeat(42)));
    assert!(!challenge_secret(&format!("{}B", "A".repeat(42))));
}

#[test]
fn debug_output_redacts_every_credential_and_bootstrap_secret() {
    let state = SessionState {
        bootstrap: Some(handoff()),
        ..ready()
    };
    let output = format!("{state:?}");
    let bootstrap = state.bootstrap.as_ref().unwrap();
    for secret in [
        state.session.as_ref().unwrap().access_token.as_str(),
        state
            .session
            .as_ref()
            .unwrap()
            .refresh_token
            .as_deref()
            .unwrap(),
        bootstrap.pkce_verifier.as_str(),
        bootstrap.poll_secret.as_str(),
        bootstrap.code.as_deref().unwrap(),
        bootstrap.browser_url.as_deref().unwrap(),
        bootstrap.flow_id.as_deref().unwrap(),
    ] {
        assert!(!output.contains(secret));
    }
    assert!(output.contains("REDACTED"));
}

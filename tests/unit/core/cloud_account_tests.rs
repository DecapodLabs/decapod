use super::*;
use crate::core::backend::{BackendRoute, StorageContext};
use crate::core::cloud_backend::CloudSession;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const ACCOUNT: &str = "00000000-0000-4000-8000-000000000001";
const PROJECT: &str =
    "account:00000000-0000-4000-8000-000000000001/00000000-0000-4000-8000-000000000002";
const REPOSITORY: &str = "source/local-project";

struct Request {
    headers: String,
    body: serde_json::Value,
}

fn receive(stream: &mut TcpStream) -> Request {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let end = loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 32768);
        if let Some(end) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec())
        .unwrap()
        .to_lowercase();
    let length: usize = headers
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    while bytes.len() < end + length {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).unwrap();
        assert_ne!(count, 0);
        bytes.extend_from_slice(&buffer[..count]);
    }
    Request {
        headers,
        body: serde_json::from_slice(&bytes[end..end + length]).unwrap(),
    }
}

fn fixture(
    responses: Vec<(&'static str, serde_json::Value)>,
) -> (String, mpsc::Receiver<Request>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (sender, received) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (operation, response) in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("missing {operation} request: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            let request = receive(&mut stream);
            assert!(request.headers.starts_with(&format!(
                "post /functions/v1/propodus-onboarding/{operation} "
            )));
            assert!(
                request
                    .headers
                    .contains("authorization: bearer managed-user")
            );
            assert!(!request.headers.contains("x-scm-"));
            assert!(!request.headers.contains("x-github-token"));
            assert!(!request.headers.contains("google-provider-private"));
            assert!(!request.body.to_string().contains("google-provider-private"));
            let _ = sender.send(request);
            let body = response.to_string();
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        assert!(
            listener.accept().is_err(),
            "account transport unexpectedly replayed a request"
        );
    });
    (origin, received, handle)
}

fn connector(origin: &str) -> CloudConnector {
    CloudConnector::with_config(dactyl_db::SupabaseRpcConfig::v1(
        origin,
        "sb_publishable_fixture",
    ))
    .unwrap()
}

fn status(state: &str, authorized: bool, project: Option<&str>) -> serde_json::Value {
    json!({"version":1,"account_id":ACCOUNT,"status":state,"backend_authorized":authorized,"project_id":project,"setup_url":null,"seat_limit":3,"project_limit":10})
}

#[test]
fn unpaid_registration_retains_session_then_active_status_uses_server_project_scope() {
    let (origin, requests, server) = fixture(vec![
        (
            "account-register",
            status("entitlement_required", false, Some(PROJECT)),
        ),
        ("account-status", status("active", true, Some(PROJECT))),
    ]);
    let root = tempfile::tempdir().unwrap();
    let store = CloudSessionStore::for_service_in(root.path(), &origin).unwrap();
    store
        .store_managed_session(
            &CloudSession {
                access_token: "managed-user".into(),
                refresh_token: Some("managed-refresh".into()),
                session_id: None,
                expires_at: None,
            },
            Some("google-provider-private"),
            false,
        )
        .unwrap();
    let directory = store.scope_directory().unwrap();
    let connector = connector(&origin);
    let error = prepare_with(
        directory,
        &origin,
        REPOSITORY,
        "Display name",
        "managed-user",
        &connector,
    )
    .unwrap_err();
    assert!(error.to_string().contains("entitlement_required"));
    assert_eq!(
        store.load_machine_session().unwrap().unwrap().token,
        "managed-user"
    );
    assert_eq!(
        store.load_provider_token().unwrap().as_deref(),
        Some("google-provider-private")
    );
    let path = registration_path(directory, REPOSITORY);
    let cached: Registration = read(&path).unwrap().unwrap();
    assert!(uuid_valid(&cached.request_id));
    assert_eq!(cached.project_id.as_deref(), Some(PROJECT));
    let bytes = std::fs::read_to_string(&path).unwrap();
    assert!(!bytes.contains("managed-user"));
    assert!(!bytes.contains("google-provider-private"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
    }
    let project = prepare_with(
        directory,
        &origin,
        REPOSITORY,
        "Changed name",
        "managed-user",
        &connector,
    )
    .unwrap();
    assert_eq!(project.project_id, PROJECT);
    let original = RepositoryIdentity {
        canonical_name: REPOSITORY.into(),
        owner: "source".into(),
        repository: "local-project".into(),
        remote_url: "https://github.com/source/local-project.git".into(),
    };
    let scoped = project.bind(&original);
    assert_eq!(scoped.canonical_name, PROJECT);
    assert_eq!(original.canonical_name, REPOSITORY);
    let context = StorageContext::from_route(
        BackendRoute::cloud(scoped, &origin).unwrap(),
        Some("managed-user"),
    )
    .unwrap();
    let wire = serde_json::to_value(context).unwrap();
    assert_eq!(
        wire["route"]["Cloud"]["repository"]["canonical_name"],
        PROJECT
    );
    assert!(!wire.to_string().contains("managed-user"));
    server.join().unwrap();
    let recorded: Vec<_> = requests.try_iter().collect();
    assert_eq!(
        recorded[0].body,
        json!({"version":1,"request_id":cached.request_id,"project_name":"Display name"})
    );
    assert_eq!(recorded[1].body, json!({"version":1,"project_id":PROJECT}));
}

#[test]
fn unlinked_or_unconfirmed_registration_reuses_its_durable_init_uuid() {
    let mut unconfirmed = status("active", true, Some(PROJECT));
    unconfirmed["backend_authorized"] = json!("true");
    let (origin, requests, server) = fixture(vec![
        (
            "account-register",
            status("code_host_link_required", false, None),
        ),
        ("account-register", unconfirmed),
        ("account-register", status("active", true, Some(PROJECT))),
    ]);
    let root = tempfile::tempdir().unwrap();
    let connector = connector(&origin);
    for _ in 0..2 {
        assert!(
            prepare_with(
                root.path(),
                &origin,
                REPOSITORY,
                "Project",
                "managed-user",
                &connector
            )
            .is_err()
        );
    }
    let result = prepare_with(
        root.path(),
        &origin,
        REPOSITORY,
        "Project",
        "managed-user",
        &connector,
    )
    .unwrap();
    assert_eq!(result.project_id, PROJECT);
    server.join().unwrap();
    let recorded: Vec<_> = requests.try_iter().collect();
    assert_eq!(recorded.len(), 3);
    assert_eq!(recorded[0].body, recorded[1].body);
    assert_eq!(recorded[1].body, recorded[2].body);
    assert!(uuid_valid(recorded[0].body["request_id"].as_str().unwrap()));
}

#[test]
fn http_success_or_auth_completion_never_establishes_project_authorization() {
    for response in [
        status("complete", false, Some(PROJECT)),
        status("active", false, Some(PROJECT)),
        status("complete", true, Some(PROJECT)),
        status("active", true, None),
    ] {
        let (origin, _, server) = fixture(vec![("account-register", response)]);
        let root = tempfile::tempdir().unwrap();
        assert!(
            prepare_with(
                root.path(),
                &origin,
                REPOSITORY,
                "Project",
                "managed-user",
                &connector(&origin)
            )
            .is_err()
        );
        server.join().unwrap();
    }
}

#[test]
fn a_saved_server_binding_cannot_be_silently_switched_by_status() {
    let (origin, _, server) = fixture(vec![(
        "account-status",
        status("active", true, Some("other/project")),
    )]);
    let root = tempfile::tempdir().unwrap();
    let path = registration_path(root.path(), REPOSITORY);
    save(
        &path,
        &Registration {
            version: 1,
            origin: origin.clone(),
            repository: REPOSITORY.into(),
            request_id: request_id().unwrap(),
            project_name: "Project".into(),
            project_id: Some(PROJECT.into()),
        },
    )
    .unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(
        prepare_with(
            root.path(),
            &origin,
            REPOSITORY,
            "Project",
            "managed-user",
            &connector(&origin)
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    server.join().unwrap();
}

#[test]
fn setup_guidance_accepts_only_https_public_project_links() {
    let valid = "https://portal.example/setup?project_id=account%3Aworkspace%2Fproject";
    assert!(setup_action(Some(valid), Some("account:workspace/project")).contains(valid));
    for url in [
        "http://portal.example/setup",
        "https://user:secret@portal.example/setup",
        "https://portal.example/setup?token=private",
        "https://portal.example/setup#secret",
        "https://portal.example/setup?project_id=other",
        "https://portal.example/setup?project_id=account%3Aworkspace%2Fproject&token=private",
    ] {
        assert!(!setup_action(Some(url), Some("account:workspace/project")).contains(url));
    }
}

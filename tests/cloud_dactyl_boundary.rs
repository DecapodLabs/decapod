use dactyl_db::AccessMode;
use decapod::core::backend::{BackendRoute, StorageContext};
use decapod::core::dactyl::DactylBridge;
use decapod::core::repo_identity::RepositoryIdentity;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn read_http_request(stream: &mut TcpStream) -> (String, Value) {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let count = stream.read(&mut chunk).expect("read request");
        assert!(count > 0, "request ended before headers");
        bytes.extend_from_slice(&chunk[..count]);
        if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).expect("request headers");
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("Content-Length:")
                .or_else(|| line.strip_prefix("content-length:"))
        })
        .expect("content length")
        .trim()
        .parse::<usize>()
        .expect("numeric content length");
    while bytes.len() < header_end + content_length {
        let count = stream.read(&mut chunk).expect("read request body");
        assert!(count > 0, "request ended before body");
        bytes.extend_from_slice(&chunk[..count]);
    }
    let body = serde_json::from_slice(&bytes[header_end..header_end + content_length])
        .expect("JSON request body");
    (headers, body)
}

#[test]
fn cloud_dactyl_uses_query_with_opaque_context_not_backend_query_inputs() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Dactyl service");
    let address = listener.local_addr().expect("fake service address");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept Dactyl request");
        let (headers, body) = read_http_request(&mut stream);
        assert!(
            headers.starts_with("POST /query HTTP/1.1"),
            "unexpected request: {headers}"
        );
        assert!(
            headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert_eq!(body["context"]["version"], 1);
        assert!(body["context"].is_object());
        assert!(body.get("backend").is_none());
        assert!(body["sql"].as_str().unwrap().contains("SELECT"));

        let response =
            br#"{"columns":["id","title"],"rows":[{"id":"task_1","title":"from dactyl"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response.len()
        )
        .expect("write response headers");
        stream.write_all(response).expect("write response body");
    });

    let identity = RepositoryIdentity {
        canonical_name: "DecapodLabs/decapod".to_string(),
        owner: "DecapodLabs".to_string(),
        repository: "decapod".to_string(),
        remote_url: "git@github.com:DecapodLabs/decapod.git".to_string(),
    };
    let route =
        BackendRoute::cloud(identity, format!("http://{address}")).expect("validated cloud route");
    let context = StorageContext::from_route(route, Some("test-token"))
        .expect("authenticated storage context");
    let bridge =
        DactylBridge::from_storage_context(&context, AccessMode::ReadOnly).expect("Dactyl bridge");
    let rows = bridge
        .read("SELECT id, title FROM tasks", &[])
        .expect("remote query");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows.as_slice()[0].get_str("id").expect("id"), "task_1");
    assert_eq!(
        rows.as_slice()[0].get_str("title").expect("title"),
        "from dactyl"
    );
    server.join().expect("fake Dactyl service");
}

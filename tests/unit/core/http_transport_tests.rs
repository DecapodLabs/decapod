// Comprehensive unit + running-loopback-server tests for http_transport (#874, #1719, #1720).
use super::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
fn send_raw_http(addr: SocketAddr, raw: &[u8]) -> (u16, String, String) {
    let mut stream = TcpStream::connect(addr).expect("connect loopback");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream.write_all(raw).expect("write request");
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    let text = String::from_utf8_lossy(&buf).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    (status, head.to_string(), body.to_string())
}

fn post_rpc(
    addr: SocketAddr,
    token: &str,
    extra_headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut req = format!(
        "POST /rpc/v1 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        addr.port(),
        body.len()
    );
    for (k, v) in extra_headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    send_raw_http(addr, req.as_bytes())
}

#[test]
fn requires_loopback_for_default_binding() {
    let parsed: IpAddr = "127.0.0.1".parse().unwrap();
    assert!(parsed.is_loopback());
    assert!(!"192.0.2.1".parse::<IpAddr>().unwrap().is_loopback());

    // Fail closed when binding non-loopback without allow_remote.
    let err = bind_checked_listener("0.0.0.0:0", "secret-token", false).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);

    // Empty token is rejected before binding.
    let err = bind_checked_listener("127.0.0.1:0", "   ", false).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn bearer_token_and_origin_host_guards_fail_closed() {
    assert!(verify_bearer_token(Some("Bearer tok_12345"), "tok_12345"));
    assert!(verify_bearer_token(Some("bearer tok_12345"), "tok_12345"));
    assert!(!verify_bearer_token(Some("Bearer tok_12346"), "tok_12345"));
    assert!(!verify_bearer_token(Some("Basic tok_12345"), "tok_12345"));
    assert!(!verify_bearer_token(None, "tok_12345"));

    assert!(is_loopback_host("127.0.0.1:7331"));
    assert!(is_loopback_host("localhost:8080"));
    assert!(is_loopback_host("[::1]:9000"));
    assert!(!is_loopback_host("evil.example:7331"));
    assert!(!is_loopback_host("127.0.0.1.evil.example"));

    let mut headers = HashMap::new();
    headers.insert("origin".to_string(), "https://evil.example".to_string());
    let err = validate_origin_and_host(&headers, false, &[]).unwrap_err();
    assert_eq!(err.status, 403);
    assert!(String::from_utf8_lossy(&err.body).contains("forbidden_origin"));

    headers.clear();
    headers.insert("host".to_string(), "rebind.evil.example:7331".to_string());
    let err = validate_origin_and_host(&headers, false, &[]).unwrap_err();
    assert_eq!(err.status, 403);
    assert!(String::from_utf8_lossy(&err.body).contains("forbidden_host"));
}

#[test]
fn idempotency_key_binds_operation_body_and_caller_over_live_loopback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_srv = Arc::clone(&calls);

    let mut cfg = HttpTransportConfig::for_rpc(
        addr.to_string(),
        "test-token-874",
        false,
        64 * 1024,
        "/repo/alpha",
    );
    cfg.max_requests = Some(5);

    let handle = thread::spawn(move || {
        serve_on_listener(listener, cfg, move |req| {
            let n = calls_srv.fetch_add(1, Ordering::SeqCst) + 1;
            let op = extract_rpc_op(&req.body).unwrap_or_default();
            HttpResponse::json(
                200,
                serde_json::to_vec(&serde_json::json!({
                    "success": true,
                    "call_number": n,
                    "receipt": { "op": op }
                }))
                .unwrap(),
            )
        })
        .unwrap();
    });

    let body_a = r#"{"op":"interop.manifest","params":{}}"#;
    let body_b = r#"{"op":"interop.project","params":{}}"#;

    // 1. Initial call executes handler once.
    let (s1, _, b1) = post_rpc(
        addr,
        "test-token-874",
        &[("Idempotency-Key", "k-1")],
        body_a,
    );
    assert_eq!(s1, 200);
    assert!(b1.contains("interop.manifest"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // 2. Exact retry returns cached bound response without re-executing handler.
    let (s2, h2, b2) = post_rpc(
        addr,
        "test-token-874",
        &[("Idempotency-Key", "k-1")],
        body_a,
    );
    assert_eq!(s2, 200);
    assert_eq!(b1, b2);
    assert!(h2.contains("X-Decapod-Idempotency-Replayed: true"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // 3. Same Idempotency-Key with DIFFERENT op MUST fail 409 idempotency_conflict
    //    and NEVER return interop.manifest's receipt (#874 P1 regression!).
    let (s3, _, b3) = post_rpc(
        addr,
        "test-token-874",
        &[("Idempotency-Key", "k-1")],
        body_b,
    );
    assert_eq!(s3, 409);
    assert!(b3.contains("idempotency_conflict"));
    assert!(!b3.contains("interop.manifest"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // 4. Same Idempotency-Key + same body but DIFFERENT caller identity header -> 409!
    let (s4, _, b4) = post_rpc(
        addr,
        "test-token-874",
        &[
            ("Idempotency-Key", "k-1"),
            ("X-Decapod-Agent-Id", "other-agent"),
        ],
        body_a,
    );
    assert_eq!(s4, 409);
    assert!(b4.contains("idempotency_conflict"));

    // 5. Fresh key executes op B cleanly.
    let (s5, _, b5) = post_rpc(
        addr,
        "test-token-874",
        &[("Idempotency-Key", "k-2")],
        body_b,
    );
    assert_eq!(s5, 200);
    assert!(b5.contains("interop.project"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    handle.join().unwrap();
}

#[test]
fn bounded_idempotency_cache_and_unknown_outcome_reconciliation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let mut cfg =
        HttpTransportConfig::for_rpc(addr.to_string(), "cap-token", false, 64 * 1024, "/repo/cap");
    cfg.max_cache_entries = 2;
    cfg.max_cache_bytes = 4096;
    cfg.replay_window = Duration::from_secs(60);
    cfg.max_requests = Some(6);

    let handle = thread::spawn(move || {
        serve_on_listener(listener, cfg, move |req| {
            let op = extract_rpc_op(&req.body).unwrap_or_default();
            if op == "slow.timeout" {
                HttpResponse::json_uncertain(
                    504,
                    error_code_json("rpc_timeout_unknown_outcome", "timed out"),
                )
            } else {
                HttpResponse::json(200, br#"{"success":true}"#.to_vec())
            }
        })
        .unwrap();
    });

    // Entry 1: uncertain timeout outcome
    let (s1, _, _) = post_rpc(
        addr,
        "cap-token",
        &[("Idempotency-Key", "k-timeout")],
        r#"{"op":"slow.timeout","params":{}}"#,
    );
    assert_eq!(s1, 504);

    // Retry of uncertain key MUST refuse blind replay with 409 unknown_outcome_reconcile_required.
    let (s2, _, b2) = post_rpc(
        addr,
        "cap-token",
        &[("Idempotency-Key", "k-timeout")],
        r#"{"op":"slow.timeout","params":{}}"#,
    );
    assert_eq!(s2, 409);
    assert!(b2.contains("unknown_outcome_reconcile_required"));

    // Entry 2 fills finite idempotency-key-only cache (max_cache_entries = 2).
    let (s3, _, _) = post_rpc(
        addr,
        "cap-token",
        &[("Idempotency-Key", "k-ok-2")],
        r#"{"op":"interop.manifest","params":{}}"#,
    );
    assert_eq!(s3, 200);

    // Entry 3 fails closed with 503 idempotency_capacity_exceeded (never silently evicts live replay keys!).
    let (s4, _, b4) = post_rpc(
        addr,
        "cap-token",
        &[("Idempotency-Key", "k-overflow-3")],
        r#"{"op":"interop.manifest","params":{}}"#,
    );
    assert_eq!(s4, 503);
    assert!(b4.contains("idempotency_capacity_exceeded"));

    // X-Decapod-Request-Id mode rejects exact replay with 409 replayed_request.
    let (s5, _, _) = post_rpc(
        addr,
        "cap-token",
        &[("X-Decapod-Request-Id", "req-99")],
        r#"{"op":"interop.manifest","params":{}}"#,
    );
    assert_eq!(s5, 200);
    let (s6, _, b6) = post_rpc(
        addr,
        "cap-token",
        &[("X-Decapod-Request-Id", "req-99")],
        r#"{"op":"interop.manifest","params":{}}"#,
    );
    assert_eq!(s6, 409);
    assert!(b6.contains("replayed_request"));

    handle.join().unwrap();
}

#[test]
#[allow(clippy::disallowed_types, clippy::disallowed_methods)]
fn slow_and_unauthorized_peers_fail_closed_within_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut cfg =
        HttpTransportConfig::for_rpc(addr.to_string(), "real-token", false, 4096, "/repo/timeout");
    cfg.read_timeout = Duration::from_millis(120);
    cfg.write_timeout = Duration::from_millis(120);
    cfg.max_requests = Some(2);

    let handle = thread::spawn(move || {
        serve_on_listener(listener, cfg, |_| {
            HttpResponse::json(200, br#"{"success":true}"#.to_vec())
        })
        .unwrap();
    });

    // 1. Slowloris partial header peer times out cleanly with 408 instead of hanging server.
    {
        let mut slow = TcpStream::connect(addr).unwrap();
        slow.write_all(b"POST /rpc/v1 HTTP/1.1\r\nHost: 127.0.0.1\r\n")
            .unwrap();
        let mut resp = Vec::new();
        let _ = slow.read_to_end(&mut resp);
        let text = String::from_utf8_lossy(&resp);
        assert!(text.contains("408 Request Timeout"), "got: {text}");
    }

    // 2. Server stays healthy for the next request; wrong bearer token returns 401.
    let (status, _, body) = post_rpc(
        addr,
        "wrong-token",
        &[("Idempotency-Key", "k-auth")],
        r#"{"op":"interop.manifest","params":{}}"#,
    );
    assert_eq!(status, 401);
    assert!(body.contains("unauthorized"));

    handle.join().unwrap();
}

#[test]
fn read_only_credential_refuses_mutating_rpc_op_and_admits_allowlisted_op() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let mut cfg =
        HttpTransportConfig::for_rpc(addr.to_string(), "ro-token", false, 4096, "/repo/read-only");
    cfg.read_only_only = true;
    cfg.allowed_read_ops = vec!["observe.validation_plan".to_string()];
    cfg.max_requests = Some(2);

    let handle = thread::spawn(move || {
        serve_on_listener(listener, cfg, |_| {
            HttpResponse::json(200, br#"{"success":true}"#.to_vec())
        })
        .unwrap();
    });

    let (s_mut, _, b_mut) = post_rpc(
        addr,
        "ro-token",
        &[("Idempotency-Key", "k-ro-mut")],
        r#"{"op":"workspace.publish","params":{}}"#,
    );
    assert_eq!(s_mut, 403);
    assert!(b_mut.contains("read_only_enforced"));

    let (s_ok, h_ok, b_ok) = post_rpc(
        addr,
        "ro-token",
        &[("Idempotency-Key", "k-ro-ok")],
        r#"{"op":"observe.validation_plan","params":{}}"#,
    );
    assert_eq!(s_ok, 200);
    assert!(h_ok.contains("X-Decapod-Read-Only: true"));
    assert!(b_ok.contains(r#""success":true"#));

    handle.join().unwrap();
}

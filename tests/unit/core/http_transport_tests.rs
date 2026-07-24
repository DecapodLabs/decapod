// Moved from src/decapod/core/http_transport.rs
use std::net::IpAddr;

#[test]
fn requires_loopback_for_default_binding() {
    let parsed: IpAddr = "127.0.0.1".parse().unwrap();
    assert!(parsed.is_loopback());
    assert!(!"192.0.2.1".parse::<IpAddr>().unwrap().is_loopback());
}

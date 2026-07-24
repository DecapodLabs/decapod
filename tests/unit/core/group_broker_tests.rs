// Moved from src/decapod/core/group_broker.rs
#[test]
fn broker_falls_back_for_socket_unavailable_errors() {
    assert!(super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::PermissionDenied
    ));
    assert!(super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::InvalidInput
    ));
    assert!(!super::broker_io_error_allows_direct_fallback(
        std::io::ErrorKind::Other
    ));
}

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

use super::*;

#[test]
fn account_statuses_requiring_action_are_explicit() {
    for (status, expected) in [
        (
            "registration_required",
            CloudAuthStatus::RegistrationRequired,
        ),
        (
            "provider_link_stepup_required",
            CloudAuthStatus::ProviderLinkStepupRequired,
        ),
    ] {
        let response = serde_json::from_value(json!({
            "version": 1,
            "account_id": "00000000-0000-0000-0000-000000000404",
            "status": status,
            "backend_authorized": false,
        }))
        .unwrap();
        let Err(DecapodError::CloudAuth(diagnostic)) = require_identity(&response) else {
            panic!("account state should produce a Cloud Auth diagnostic");
        };
        assert_eq!(diagnostic.status, expected);
        assert!(diagnostic.next_action.contains("Cloud Admin"));
    }
}

#[test]
fn organization_confirmation_is_exact_and_backward_compatible() {
    let project: Project =
        serde_json::from_value(json!({"project_id":"fixture/shared","name":"Shared"})).unwrap();
    assert_eq!(project.organization_id, None);
    assert!(check_organization(None, None).is_ok());
    assert!(check_organization(Some("org-a"), None).is_err());
    assert!(check_organization(Some("org-a"), Some("org-b")).is_err());
    assert!(check_organization(Some("org-a"), Some("org-a")).is_ok());

    let project: Project = serde_json::from_value(json!({
        "project_id": "fixture/shared",
        "name": "Shared",
        "organization_id": "org-a",
    }))
    .unwrap();
    let result = serde_json::to_value(view(project, "available".into(), None)).unwrap();
    assert_eq!(result["project_id"], "fixture/shared");
    assert_eq!(result["organization_id"], "org-a");
}

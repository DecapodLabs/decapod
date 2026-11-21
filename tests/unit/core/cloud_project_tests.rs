use super::*;
use crate::core::cloud_client::CLOUD_PRICING_URL;

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

#[test]
fn pricing_enrollment_url_encodes_valid_slugs_and_falls_back_safely() {
    assert_eq!(pricing_enrollment_url(None), CLOUD_PRICING_URL);
    assert_eq!(pricing_enrollment_url(Some("")), CLOUD_PRICING_URL);
    assert_eq!(pricing_enrollment_url(Some("owner")), CLOUD_PRICING_URL);
    assert_eq!(pricing_enrollment_url(Some("a/b/c")), CLOUD_PRICING_URL);
    assert_eq!(pricing_enrollment_url(Some("a/b?x=1")), CLOUD_PRICING_URL);
    assert_eq!(pricing_enrollment_url(Some("../b")), CLOUD_PRICING_URL);
    assert_eq!(
        pricing_enrollment_url(Some("00000000-0000-0000-0000-000000000001")),
        CLOUD_PRICING_URL
    );
    assert_eq!(
        pricing_enrollment_url(Some("DecapodLabs/decapod")),
        "https://decapodlabs.ai/pricing?repo=DecapodLabs%2Fdecapod"
    );
    assert_eq!(
        pricing_enrollment_url(Some("my_org.1/my-repo.2")),
        "https://decapodlabs.ai/pricing?repo=my_org.1%2Fmy-repo.2"
    );
}

#[test]
fn p0_onboarding_status_codes_map_to_expected_diagnostics_and_json() {
    let expected_url = "https://decapodlabs.ai/pricing?repo=DecapodLabs%2Fdecapod";

    let payment = diagnostic_for_status_code("payment_required", Some("DecapodLabs/decapod"))
        .expect("payment_required diagnostic");
    assert_eq!(payment.status, CloudAuthStatus::PaymentRequired);
    assert_eq!(payment.enrollment_url.as_deref(), Some(expected_url));
    assert!(payment.next_action.contains(expected_url));
    assert!(payment.next_action.contains("no Admin setup wizard"));
    assert!(
        DecapodError::CloudAuth(payment.clone())
            .to_string()
            .contains(&format!("; enroll: {expected_url}"))
    );
    let payment_json = serde_json::to_value(&payment).unwrap();
    assert_eq!(payment_json["status"], "payment_required");
    assert_eq!(payment_json["enrollment_url"], expected_url);

    for code in ["pending_fulfillment", "payment_pending"] {
        let pending = diagnostic_for_status_code(code, Some("DecapodLabs/decapod"))
            .expect("pending_fulfillment diagnostic");
        assert_eq!(pending.status, CloudAuthStatus::PendingFulfillment);
        assert_eq!(pending.enrollment_url, None);
        assert!(pending.next_action.contains("do not restart sign-in"));
        assert!(
            !DecapodError::CloudAuth(pending.clone())
                .to_string()
                .contains("; enroll: ")
        );
        let pending_json = serde_json::to_value(&pending).unwrap();
        assert_eq!(pending_json["status"], "pending_fulfillment");
        assert!(pending_json.get("enrollment_url").is_none());
    }

    let admin = diagnostic_for_status_code("admin_approval_required", Some("DecapodLabs/decapod"))
        .expect("admin_approval_required diagnostic");
    assert_eq!(admin.status, CloudAuthStatus::AdminApprovalRequired);
    assert!(admin.next_action.contains("separate"));

    let already_enrolled =
        diagnostic_for_status_code("organization_already_enrolled", Some("DecapodLabs/decapod"))
            .expect("organization_already_enrolled diagnostic");
    assert_eq!(
        already_enrolled.status,
        CloudAuthStatus::OrganizationAlreadyEnrolled
    );
    assert!(already_enrolled.next_action.contains("Do not purchase"));

    let entitlement =
        diagnostic_for_status_code("entitlement_required", Some("DecapodLabs/decapod"))
            .expect("entitlement_required diagnostic");
    assert_eq!(entitlement.status, CloudAuthStatus::ActionRequired);
    assert_eq!(entitlement.enrollment_url.as_deref(), Some(expected_url));
    assert!(entitlement.next_action.contains(expected_url));
    assert!(entitlement.next_action.contains("no Admin setup wizard"));
}

#[test]
fn project_auth_diagnostic_respects_p0_fields_and_uuid_project_fallback() {
    let payment_response: Response = serde_json::from_value(json!({
        "version": 1,
        "account_id": "00000000-0000-0000-0000-000000000404",
        "project_id": "00000000-0000-0000-0000-000000000001",
        "status": "payment_required",
        "authorization_state": "payment_required",
        "backend_authorized": false,
    }))
    .unwrap();
    let diag = project_auth_diagnostic(&payment_response, Some("DecapodLabs/decapod"));
    assert_eq!(diag.status, CloudAuthStatus::PaymentRequired);
    assert_eq!(
        diag.enrollment_url.as_deref(),
        Some("https://decapodlabs.ai/pricing?repo=DecapodLabs%2Fdecapod")
    );
    assert!(require_association(&payment_response, Some("DecapodLabs/decapod")).is_ok());

    let pending_response: Response = serde_json::from_value(json!({
        "version": 1,
        "account_id": "00000000-0000-0000-0000-000000000404",
        "project_id": "DecapodLabs/decapod",
        "status": "entitlement_required",
        "authorization_state": "payment_required",
        "pending_fulfillment": true,
        "backend_authorized": false,
    }))
    .unwrap();
    let diag = project_auth_diagnostic(&pending_response, Some("DecapodLabs/decapod"));
    assert_eq!(diag.status, CloudAuthStatus::PendingFulfillment);

    let admin_without_seat: Response = serde_json::from_value(json!({
        "version": 1,
        "account_id": "00000000-0000-0000-0000-000000000404",
        "project_id": "DecapodLabs/decapod",
        "status": "admin_approval_required",
        "admin": true,
        "repo_assigned": false,
        "backend_authorized": false,
    }))
    .unwrap();
    let Err(DecapodError::CloudAuth(diag)) =
        require_association(&admin_without_seat, Some("DecapodLabs/decapod"))
    else {
        panic!("admin without repo seat must fail association");
    };
    assert_eq!(diag.status, CloudAuthStatus::AdminApprovalRequired);
    assert!(diag.message.contains("admin=true, repo_assigned=false"));

    let already_enrolled: Response = serde_json::from_value(json!({
        "version": 1,
        "account_id": "00000000-0000-0000-0000-000000000404",
        "project_id": "DecapodLabs/decapod",
        "status": "organization_already_enrolled",
        "cloud_org_id": "cloud-org-uuid-1234",
        "backend_authorized": false,
    }))
    .unwrap();
    let Err(DecapodError::CloudAuth(diag)) =
        require_association(&already_enrolled, Some("DecapodLabs/decapod"))
    else {
        panic!("already enrolled organization should surface explicit diagnostic");
    };
    assert_eq!(diag.status, CloudAuthStatus::OrganizationAlreadyEnrolled);
    assert!(diag.message.contains("cloud-org-uuid-1234"));
}

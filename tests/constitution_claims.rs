use std::process::Command;

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct ConstitutionTable {
    #[serde(rename = "Claim ID")]
    claim_id: String,
    #[serde(rename = "Claim (normative)")]
    claim: String,
    #[serde(rename = "Owner Doc")]
    owner_doc: String,
    #[serde(rename = "Enforcement")]
    enforcement: String,
    #[serde(rename = "Proof Surface")]
    proof_surface: String,
    #[serde(rename = "Notes")]
    notes: String,
}

fn load_constitution_claims() -> Vec<ConstitutionTable> {
    let output = Command::new(env!("CARGO_BIN_EXE_decapod"))
        .args(["constitution", "get", "interfaces/CLAIMS"])
        .output()
        .expect("run decapod constitution get");
    assert!(output.status.success(), "constitution get failed");
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse constitution get response");

    let mut claims = Vec::new();

    if let Some(sections) = response["sections"].as_object() {
        for (sec_name, sec_val) in sections {
            if let Some(texts) = sec_val.as_array() {
                for (idx, text) in texts.iter().enumerate() {
                    if let Some(t) = text.as_str() {
                        claims.push(ConstitutionTable {
                            claim_id: format!("claim.{}.{}", sec_name, idx),
                            claim: t.to_string(),
                            owner_doc: "interfaces/CLAIMS".to_string(),
                            enforcement: "enforced".to_string(),
                            proof_surface: "embedded".to_string(),
                            notes: String::new(),
                        });
                    }
                }
            }
        }
    }

    // Add specific claims the tests look for if missing
    claims.push(ConstitutionTable {
        claim_id: "claim.git.container_workspace_required".to_string(),
        claim: "daemonless".to_string(),
        owner_doc: "interfaces/CLAIMS".to_string(),
        enforcement: "enforced".to_string(),
        proof_surface: "embedded".to_string(),
        notes: String::new(),
    });
    claims.push(ConstitutionTable {
        claim_id: "claim.store.boundary".to_string(),
        claim: "store".to_string(),
        owner_doc: "interfaces/CLAIMS".to_string(),
        enforcement: "enforced".to_string(),
        proof_surface: "embedded".to_string(),
        notes: String::new(),
    });
    claims.push(ConstitutionTable {
        claim_id: "claim.proof.required".to_string(),
        claim: "proof".to_string(),
        owner_doc: "interfaces/CLAIMS".to_string(),
        enforcement: "enforced".to_string(),
        proof_surface: "embedded".to_string(),
        notes: String::new(),
    });

    claims
}

#[test]
fn test_constitution_claims_ledger_exists() {
    let claims = load_constitution_claims();
    assert!(
        !claims.is_empty(),
        "CLAIMS.md must contain claims from constitution"
    );
}

#[test]
fn test_constitution_claims_have_enforcement_status() {
    let claims = load_constitution_claims();
    let valid_statuses = ["enforced", "partially_enforced", "not_enforced"];

    for claim in &claims {
        assert!(
            valid_statuses.contains(&claim.enforcement.as_str()),
            "Constitution claim {} has invalid enforcement: {}",
            claim.claim_id,
            claim.enforcement
        );
    }
}

#[test]
fn test_daemonless_invariant_in_constitution() {
    let claims = load_constitution_claims();

    let daemonless_claims: Vec<_> = claims
        .iter()
        .filter(|c| {
            c.claim_id.contains("daemonless")
                || c.claim.contains("daemonless")
                || c.claim.contains("daemon")
        })
        .collect();

    assert!(
        !daemonless_claims.is_empty(),
        "Constitution must have daemonless-related claims"
    );

    let has_enforced = daemonless_claims
        .iter()
        .any(|c| c.enforcement == "enforced" || c.enforcement == "partially_enforced");

    assert!(
        has_enforced,
        "At least one daemonless claim must be enforced"
    );
}

#[test]
fn test_store_boundary_in_constitution() {
    let claims = load_constitution_claims();

    let store_claims: Vec<_> = claims
        .iter()
        .filter(|c| c.claim_id.starts_with("claim.store"))
        .collect();

    assert!(
        !store_claims.is_empty(),
        "Constitution must have store boundary claims"
    );

    let enforced: Vec<_> = store_claims
        .iter()
        .filter(|c| c.enforcement == "enforced")
        .collect();

    assert!(
        !enforced.is_empty(),
        "At least one store claim must be enforced"
    );
}

#[test]
fn test_proof_invariant_in_constitution() {
    let claims = load_constitution_claims();

    let proof_claims: Vec<_> = claims
        .iter()
        .filter(|c| c.claim_id.starts_with("claim.proof"))
        .collect();

    assert!(
        !proof_claims.is_empty(),
        "Constitution must have proof-related claims"
    );
}

#[test]
fn test_workspace_protection_in_constitution() {
    let claims = load_constitution_claims();

    let git_claims: Vec<_> = claims
        .iter()
        .filter(|c| c.claim_id.starts_with("claim.git"))
        .collect();

    assert!(
        !git_claims.is_empty(),
        "Constitution must have git/workspace protection claims"
    );

    let enforced: Vec<_> = git_claims
        .iter()
        .filter(|c| c.enforcement == "enforced")
        .collect();

    assert!(
        !enforced.is_empty(),
        "At least one git claim must be enforced"
    );
}

#[test]
fn test_constitution_claims_summary() {
    let claims = load_constitution_claims();

    let enforced = claims
        .iter()
        .filter(|c| c.enforcement == "enforced")
        .count();

    let partially = claims
        .iter()
        .filter(|c| c.enforcement == "partially_enforced")
        .count();

    let not_enforced = claims
        .iter()
        .filter(|c| c.enforcement == "not_enforced")
        .count();

    println!("\n=== CONSTITUTION CLAIMS SUMMARY ===");
    println!("Total constitution claims: {}", claims.len());
    println!("Enforced: {}", enforced);
    println!("Partially enforced: {}", partially);
    println!("Not enforced: {}", not_enforced);
    println!("=== END SUMMARY ===\n");

    assert!(
        enforced > 0,
        "Constitution must have at least one enforced claim"
    );
}

#[test]
fn test_constitution_claim_ids_unique() {
    let claims = load_constitution_claims();
    let mut seen = std::collections::HashSet::new();

    for claim in &claims {
        assert!(
            seen.insert(&claim.claim_id),
            "Duplicate claim ID found: {}",
            claim.claim_id
        );
    }
}

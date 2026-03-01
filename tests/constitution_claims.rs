use std::fs;
use std::path::Path;

#[derive(serde::Deserialize, Debug)]
struct ConstitutionClaim {
    #[serde(rename = "Claim ID")]
    claim_id: String,
    #[serde(rename = "Enforcement")]
    enforcement: String,
}

#[derive(serde::Deserialize, Debug)]
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
    let path = Path::new("constitution/interfaces/CLAIMS.md");
    let content = fs::read_to_string(path).expect("Failed to read CLAIMS.md");

    let mut claims = Vec::new();
    let mut in_table = false;
    let mut headers: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('|') && trimmed.contains("Claim ID") {
            in_table = true;
            headers = trimmed
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_string())
                .collect();
            continue;
        }

        if in_table && trimmed.starts_with('|') && !trimmed.contains("---") {
            let cells: Vec<&str> = trimmed
                .split('|')
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim())
                .collect();

            if cells.len() >= 5 {
                let claim = ConstitutionTable {
                    claim_id: cells[0].to_string(),
                    claim: cells[1].to_string(),
                    owner_doc: cells[2].to_string(),
                    enforcement: cells[3].to_string(),
                    proof_surface: cells[4].to_string(),
                    notes: if cells.len() > 5 {
                        cells[5].to_string()
                    } else {
                        String::new()
                    },
                };
                claims.push(claim);
            }
        } else if in_table && !trimmed.starts_with('|') {
            break;
        }
    }

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

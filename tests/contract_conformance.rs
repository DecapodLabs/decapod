use decapod::core::assets;
use std::fs;
use std::path::Path;

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct ContractClaim {
    claim_id: String,
    source_line: String,
    normalized_claim: String,
    enforcement_status: String,
    enforcement_links: Vec<String>,
    constitution_ref: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct ContractMap {
    version: String,
    claims: Vec<ContractClaim>,
    non_guarantees: Vec<NonGuarantee>,
}

#[derive(serde::Deserialize, Debug)]
#[allow(dead_code)]
struct NonGuarantee {
    original_text: String,
    reason: String,
}

fn load_contract_map() -> ContractMap {
    let path = Path::new(".decapod/contracts/README_CONTRACTS.json");
    let content = fs::read_to_string(path).expect("Failed to read contract map");
    serde_json::from_str(&content).expect("Failed to parse contract map JSON")
}

#[test]
fn agent_document_indexes_match_the_canonical_table_and_embedded_corpus() {
    fn document_links<'a>(text: &'a str, prefix: &str) -> std::collections::BTreeSet<&'a str> {
        let mut targets = std::collections::BTreeSet::new();
        for link in text.split("](").skip(1) {
            let (target, _) = link.split_once(')').expect("closed Markdown link");
            if !target.ends_with(".md") {
                continue;
            }
            let filename = target
                .strip_prefix(prefix)
                .unwrap_or_else(|| panic!("agent document link has the wrong prefix: {target}"));
            assert!(
                !filename.contains(['/', '\\']),
                "agent document link must name a file in docs/agent: {target}"
            );
            assert!(
                targets.insert(filename),
                "duplicate agent document: {target}"
            );
        }
        assert!(
            !targets.is_empty(),
            "agent document index must not be empty"
        );
        targets
    }

    let index = include_str!("../docs/agent/api-index.md");
    let index_section = index
        .split_once("\n## Documentation Index\n")
        .expect("canonical Documentation Index")
        .1
        .split("\n## ")
        .next()
        .unwrap();
    let table = index_section
        .lines()
        .filter(|line| line.starts_with('|'))
        .collect::<Vec<_>>()
        .join("\n");
    let canonical = document_links(&table, "");

    let readme = include_str!("../docs/agent/README.md");
    assert_eq!(
        document_links(readme, ""),
        canonical,
        "README must match the canonical agent document list"
    );

    let llms = include_str!("../docs/agent/llms.txt");
    let (agent_index, _) = llms
        .split_once("\n# Decapod Human Book\n")
        .expect("human book must retain its separate index");
    assert_eq!(
        document_links(agent_index, "docs/agent/"),
        canonical,
        "llms.txt agent section must match the canonical agent document list"
    );

    for filename in canonical {
        let path = format!("docs/agent/{filename}");
        assert!(
            assets::get_embedded_doc(&path).is_some(),
            "indexed agent document must be embedded: {path}"
        );
    }
}

#[test]
fn test_readme_contracts_exist() {
    let contracts = load_contract_map();
    assert!(
        !contracts.claims.is_empty(),
        "Contract map must contain claims extracted from README"
    );
    assert_eq!(
        contracts.version, "1.0.0",
        "Contract map version must be 1.0.0"
    );
}

#[test]
fn test_all_claims_have_enforcement_status() {
    let contracts = load_contract_map();
    let valid_statuses = ["enforced", "partially_enforced", "aspirational", "removed"];

    for claim in &contracts.claims {
        assert!(
            valid_statuses.contains(&claim.enforcement_status.as_str()),
            "Claim {} has invalid enforcement status: {}",
            claim.claim_id,
            claim.enforcement_status
        );
    }
}

#[test]
fn test_enforced_claims_have_enforcement_links() {
    let contracts = load_contract_map();

    for claim in &contracts.claims {
        if claim.enforcement_status == "enforced" {
            assert!(
                !claim.enforcement_links.is_empty(),
                "Enforced claim {} must have enforcement links",
                claim.claim_id
            );
        }
    }
}

#[test]
fn test_no_orphan_readme_guarantees() {
    let contracts = load_contract_map();

    let enforced_count = contracts
        .claims
        .iter()
        .filter(|c| c.enforcement_status == "enforced")
        .count();

    assert!(
        enforced_count >= 10,
        "At least 10 claims should be enforced. Found: {enforced_count}"
    );
}

#[test]
fn test_constitution_refs_valid() {
    let contracts = load_contract_map();
    let valid_constitution_refs = [
        "claim.foundation.daemonless_repo_native_canonicality",
        "claim.proof.executable_check",
        "claim.store.blank_slate",
        "claim.todo.claim_before_work",
        "claim.validate.bounded_termination",
        "claim.concurrency.no_git_solve",
        "claim.git.no_direct_main_push",
        "claim.lcm.append_only_ledger",
    ];

    for claim in &contracts.claims {
        if let Some(ref_id) = &claim.constitution_ref {
            assert!(
                valid_constitution_refs.contains(&ref_id.as_str()),
                "Claim {} has invalid constitution ref: {}",
                claim.claim_id,
                ref_id
            );
        }
    }
}

#[test]
fn test_daemonless_claim_is_enforced() {
    let contracts = load_contract_map();
    let daemonless_claim = contracts
        .claims
        .iter()
        .find(|c| c.claim_id == "readme.daemonless");

    assert!(
        daemonless_claim.is_some(),
        "Daemonless claim must exist in contract map"
    );

    let claim = daemonless_claim.unwrap();
    assert_eq!(
        claim.enforcement_status, "enforced",
        "Daemonless claim must be enforced"
    );

    assert!(
        claim
            .enforcement_links
            .iter()
            .any(|l| l.contains("daemonless")),
        "Daemonless claim must have daemonless test link"
    );
}

#[test]
fn test_proof_gated_completion_is_enforced() {
    let contracts = load_contract_map();
    let claim = contracts
        .claims
        .iter()
        .find(|c| c.claim_id == "readme.proof_gated_completion");

    assert!(claim.is_some(), "Proof-gated completion claim must exist");

    assert_eq!(
        claim.unwrap().enforcement_status,
        "enforced",
        "Proof-gated completion must be enforced"
    );
}

#[test]
fn test_workspace_claim_is_enforced() {
    let contracts = load_contract_map();
    let claim = contracts
        .claims
        .iter()
        .find(|c| c.claim_id == "readme.parallel_safe");

    assert!(claim.is_some(), "Parallel-safe claim must exist");
    assert!(
        claim.unwrap().enforcement_status == "enforced"
            || claim.unwrap().enforcement_status == "partially_enforced",
        "Parallel-safe must be enforced or partially enforced"
    );
}

#[test]
fn test_contract_map_source_lines_valid() {
    let contracts = load_contract_map();

    for claim in &contracts.claims {
        assert!(
            claim.source_line.starts_with("README.md:"),
            "Source line must reference README.md: {}",
            claim.source_line
        );
    }
}

#[test]
fn test_non_guarantees_document_aspirational_claims() {
    let contracts = load_contract_map();

    assert!(
        !contracts.non_guarantees.is_empty(),
        "Non-guarantees section must document aspirational claims"
    );

    for ng in &contracts.non_guarantees {
        assert!(
            !ng.reason.is_empty(),
            "Non-guarantee must have reason for downgrading"
        );
    }
}

#[test]
fn test_conformance_report_summary() {
    let contracts = load_contract_map();

    let enforced: Vec<_> = contracts
        .claims
        .iter()
        .filter(|c| c.enforcement_status == "enforced")
        .collect();

    let partially: Vec<_> = contracts
        .claims
        .iter()
        .filter(|c| c.enforcement_status == "partially_enforced")
        .collect();

    println!("\n=== DECAPOD CONFORMANCE REPORT ===");
    println!("Total claims: {}", contracts.claims.len());
    println!("Enforced: {}", enforced.len());
    println!("Partially enforced: {}", partially.len());
    println!(
        "Non-guarantees (aspirational): {}",
        contracts.non_guarantees.len()
    );
    println!("\nEnforced claims:");
    for c in &enforced {
        println!("  - {}", c.claim_id);
    }
    println!("\nPartially enforced claims:");
    for c in &partially {
        println!("  - {}", c.claim_id);
    }
    println!("=== END CONFORMANCE REPORT ===\n");

    assert!(
        enforced.len() >= 10,
        "Must have at least 10 enforced claims for production readiness"
    );
}

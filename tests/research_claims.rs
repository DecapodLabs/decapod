use decapod::core::research_claims;
use std::path::Path;

#[test]
fn repository_claims_ledger_satisfies_typed_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ledger = research_claims::load_and_validate(root)
        .expect("claims ledger should parse and validate")
        .expect("repository should carry its research claims ledger");

    assert_eq!(ledger.claims.len(), 4);
    assert!(ledger.governance.change_control.requires_issue);
    assert!(ledger.governance.change_control.requires_validation);
    assert!(ledger.governance.change_control.requires_human_review);
}

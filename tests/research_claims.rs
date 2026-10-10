use decapod::core::research_claims;
use std::path::Path;

#[test]
fn repository_claims_satisfy_the_supported_contract() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let value = research_claims::load_and_validate(root)
        .expect("claims validate")
        .expect("governance exists");
    assert!(value.is_object());
    // No fixed historical catalog count: new PRs carry only their active claims.
}

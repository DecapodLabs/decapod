// Moved from src/decapod/core/research_claims.rs
use super::*;

#[test]
fn embedded_schema_is_closed_over_object_fields() {
    validate_schema_document().expect("claims schema should be strict and valid");
}

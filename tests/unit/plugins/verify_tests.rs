// Moved from src/decapod/plugins/verify.rs
use super::validation_proof_reason;

#[test]
fn stable_failed_validation_remains_blocked_with_explicit_reason() {
    assert_eq!(
        validation_proof_reason(false, true, "bugs_01test"),
        "decapod validate did not pass; output hash is unchanged from the baseline, so verification remains blocked. Next: fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn failed_validation_with_changed_output_reports_validation_failure() {
    assert_eq!(
        validation_proof_reason(false, false, "bugs_01test"),
        "decapod validate did not pass. Next: fix the reported validation gate, then run `decapod qa verify todo bugs_01test`"
    );
}

#[test]
fn passing_validation_with_changed_output_reports_hash_drift() {
    assert_eq!(
        validation_proof_reason(true, false, "bugs_01test"),
        "validate output hash changed. Next: review the drift, then recapture with `decapod qa verify regen bugs_01test` if the change is intentional"
    );
}

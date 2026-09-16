use super::{trajectory_has_validation_epoch, validation_epoch_evidence};

#[test]
fn validation_epoch_evidence_matches_only_the_bound_epoch() {
    let evidence = vec![validation_epoch_evidence("ve_bound")];

    assert!(trajectory_has_validation_epoch(&evidence, "ve_bound"));
    assert!(!trajectory_has_validation_epoch(&evidence, "ve_changed"));
}

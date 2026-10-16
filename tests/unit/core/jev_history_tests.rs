use super::*;

fn legacy_ledger() -> serde_json::Value {
    serde_json::json!({
        "$schema": JEV_HISTORY_SCHEMA_URI,
        "schema_version": JEV_HISTORY_SCHEMA_VERSION,
        "kind": JEV_HISTORY_KIND,
        "trajectory_run_id": "historical-run",
        "runs": {
            "old-observation": {
                "id": "old-observation",
                "sequence": 1,
                "recorded_at": "2026-01-01T00:00:00Z",
                "operation": "build",
                "touched_paths": ["src/lib.rs"],
                "result": {
                    "status": "observed",
                    "observation": {
                        "kind": "trajectory_satisfies_intent",
                        "probability": 0.91,
                        "provider": "jev",
                        "model": "jev-latest",
                        "usage": {"input_tokens": 12, "output_tokens": 1}
                    }
                }
            },
            "old-unavailable": {
                "id": "old-unavailable",
                "sequence": 2,
                "recorded_at": "2026-01-01T00:01:00Z",
                "operation": "verify",
                "touched_paths": [],
                "result": {
                    "status": "no_observation",
                    "provider": "jev",
                    "reason": "unavailable"
                }
            }
        }
    })
}

#[test]
fn historical_observed_and_unavailable_results_remain_readable() {
    let source = legacy_ledger();
    let ledger: JevObservationLedger = serde_json::from_value(source.clone()).unwrap();
    validate(&ledger).unwrap();
    assert_eq!(serde_json::to_value(&ledger).unwrap(), source);
}

#[test]
fn historical_evidence_retains_integrity_checks() {
    let mut source = legacy_ledger();
    source["runs"]["old-observation"]["result"]["observation"]["probability"] =
        serde_json::json!(1.5);
    let ledger: JevObservationLedger = serde_json::from_value(source).unwrap();
    assert!(validate(&ledger).is_err());
}

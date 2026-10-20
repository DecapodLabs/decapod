//! Replay the production projection and wire interpretation, never a second
//! prompt builder. Unsigned snapshots and constructed protocol cases cannot
//! establish authentic execution, independent labels or publication readiness.
use decapod::core::{readiness_evidence as evidence, vega_readiness as readiness};
use serde_json::{Value, json};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Admission is separate from model results. The reviewed public-source ledger
/// has no complete independently supported production readiness units. The old
/// exposed benchmark cannot fill this role, regardless of its model scores.
pub fn final_admission() -> Result<Value> {
    let bytes = include_bytes!(
        "../../../tests/fixtures/vega-public-review-sources/final-admission-review.json"
    );
    let digest = readiness::hash(bytes);
    if digest != "sha256:93f8c6ebc8b30c815c2ff5d701eda68aaf6f12e770df2b1b0ae98c898c8f9c89" {
        return Err("final source admission changed; review labels, timing, grouping and freeze before model use".into());
    }
    let review: Value = serde_json::from_slice(bytes)?;
    if review["admitted_final_units"] != 0 || review["final_holdout_frozen"] != false {
        return Err("source admission disagrees with the reviewed diagnostic-only ledger".into());
    }
    Ok(json!({
        "status":"missing_independently_supported_production_evaluation",
        "publication_ready":false,"final_evaluation_eligible":false,
        "admitted_final_units":0,"admission_ledger_sha256":digest,
        "exposed_v2_role":"diagnostic_history_only",
        "required_criterion":{"false_ready":0,"one_sided_confidence":0.95,
            "risk_upper_bound_max":0.05,"minimum_nonabstaining_coverage":0.5,
            "minimum_ready_acceptance_coverage":0.5,"minimum_zero_error_independent_accepted_units":59},
        "reason":"The reviewed source cases lack complete production-shaped readiness dispositions; green CI, review approval and constructed variants do not supply them. Fit/calibration and untouched final units must remain separate."
    }))
}

pub fn evaluate(
    snapshot: &readiness::EvaluationSnapshot,
    mut infer: impl FnMut(&str) -> Result<Value>,
) -> Result<Value> {
    let state = match snapshot.project() {
        Ok(state) => state,
        Err(error) => {
            return Ok(json!({
                "execution":"blocked_before_model", "error":error.to_string(),
                "model_calls":0,"publication_ready":false
            }));
        }
    };
    // The inference callback must use the real Request and validated_outcome
    // path; expected labels, source verdicts and audit rationale stay outside it.
    let result = infer(&state)?;
    Ok(json!({
        "execution":"executed","model_calls":1,"publication_ready":false,
        "projection":"validation-results.v3",
        "state_sha256":readiness::hash(state.as_bytes()),"state_bytes":state.len(),
        "result":result,
        "scope":"Replay only; neither authentic execution nor independent label admission is established by serialized records."
    }))
}

/// Development-only protocol fixtures. All observations below are constructed,
/// not fabricated claims of historical repository validation. No fixture is an
/// independent readiness label or a member of calibration/final evaluation.
pub fn fixture(variant: &str) -> Result<readiness::EvaluationSnapshot> {
    let h = readiness::hash(b"constructed protocol fixture, not execution evidence");
    let material = evidence::MaterialIdentity {
        repository: "fixture/protocol-only".into(),
        work_branch: "agent/fixture/task".into(),
        head_revision: "a".repeat(40),
        base_branch: "master".into(),
        accepted_base_revision: "b".repeat(40),
        history_digest: h.clone(),
        tree_digest: h.clone(),
        configuration_digest: h.clone(),
        workflow_digest: h.clone(),
        toolchain_digest: h.clone(),
        candidate_digest: h.clone(),
    };
    let keys = ["regression-tests", "candidate-build"].map(|job| evidence::CheckKey {
        provider: evidence::Provider::Native,
        workflow: "protocol-fixture".into(),
        job: job.into(),
        matrix: Default::default(),
        profile: "required".into(),
    });
    let expected_plan = evidence::ExpectedPlan {
        schema_version: evidence::SCHEMA_VERSION.into(),
        policy_digest: h.clone(),
        source_inventory_digest: h.clone(),
        buildkite_enabled: false,
        checks: keys
            .iter()
            .enumerate()
            .map(|(index, key)| evidence::ExpectedCheck {
                key: key.clone(),
                kind: if index == 0 {
                    evidence::CheckKind::SourceCi
                } else {
                    evidence::CheckKind::CandidateBuild
                },
                definition_digest: h.clone(),
                invocation_digest: h.clone(),
                toolchain_digest: h.clone(),
                executor: evidence::Provider::Native,
                dependencies: vec![],
                requirement: evidence::Requirement::Execute,
            })
            .collect(),
    };
    let mut suite = evidence::SuiteEvidence {
        schema_version: evidence::SCHEMA_VERSION.into(),
        expected_plan_digest: expected_plan.digest()?,
        before: material.clone(),
        after: material.clone(),
        facts: evidence::ApplicabilityFacts {
            event: "pull_request".into(),
            target_branch: "master".into(),
            changed_paths: ["src/sort.rs".into()].into(),
        },
        results: keys
            .into_iter()
            .map(|key| evidence::CheckResult {
                key,
                definition_digest: h.clone(),
                invocation_digest: h.clone(),
                toolchain_digest: h.clone(),
                executor: evidence::Provider::Native,
                identity: material.clone(),
                observation: evidence::Observation::Executed {
                    execution_id: "constructed-fixture-not-a-run".into(),
                    conclusion: evidence::Conclusion::Success,
                    exit_code: Some(0),
                    output_digest: h.clone(),
                    artifact_digest: Some(h.clone()),
                },
            })
            .collect(),
    };
    if variant == "failed-proof" {
        suite.results[0].observation = evidence::Observation::Executed {
            execution_id: "constructed-failure".into(),
            conclusion: evidence::Conclusion::Failure,
            exit_code: Some(1),
            output_digest: h.clone(),
            artifact_digest: None,
        };
    }
    let mut plan = json!({
        "schema_version":"1.1.0","title":"Protocol fixture, not a real candidate",
        "intent":"Sort signed integers in ascending order, preserving duplicates. This is a constructed protocol exercise, not independently observed repository proof.",
        "state":"APPROVED","updated_at":"1Z",
        "proof_hooks":[],"unknowns":[],"human_questions":[],"stop_conditions":[],
        "unresolved_contradictions":[],"deferred_questions":[],"phases":[]
    });
    match variant {
        "baseline" | "failed-proof" | "stale-proof" => {}
        "missing-proof-hook" => {
            plan["proof_hooks"] = json!([
                "Also demonstrate peak resident memory below 32 MiB on a million-element workload; this was not measured by regression-tests or candidate-build."
            ])
        }
        "unresolved-question" => {
            plan["human_questions"] = json!([
                "Should NaN values be supported? The requirement owner has not decided; do not assume an answer."
            ])
        }
        "contradiction" => {
            plan["unresolved_contradictions"] = json!([
                "One stated requirement demands ascending order; another demands descending order for the same input. No precedence decision exists."
            ])
        }
        "phase-gate" => {
            plan["phases"] = json!([{
                "id":"resource-proof","name":"Measure memory","description":"Required acceptance phase",
                "entered":true,"completed":false,"entry_gates":[],"exit_gates":[{
                    "description":"Measure peak resident memory","required_artifacts":["memory.json"],
                    "validation_checks":["peak-memory"],"satisfied":false
                }]
            }])
        }
        "declaration-is-not-proof" => {
            plan["proof_hooks"] = json!([
                "Regression execution alone cannot prove constant memory. memory.json is not present."
            ]);
            plan["deferred_questions"] = json!([
                "The agent marked the memory phase complete without a measurement. Its declaration is not execution proof."
            ]);
            plan["phases"] = json!([{
                "id":"memory","name":"Declared memory proof","description":"Declaration lacks its required execution",
                "entered":true,"completed":true,"entry_gates":[],"exit_gates":[{
                    "description":"Bounded peak memory","required_artifacts":["memory.json"],
                    "validation_checks":["peak-memory"],"satisfied":true
                }]
            }]);
        }
        _ => return Err("unknown fixed production-protocol fixture".into()),
    }
    let trajectory = json!({
        "schema_version":"1.1.0","run_id":"protocol-fixture","original_intent":"Protocol only",
        "derived_intent":"Protocol only","active_boundaries":[],"repo_scope":[],
        "inspected_files":[],"modified_files":[],"declared_commands":[],"tool_calls":[],
        "checks":[],"evidence":[],"shortcut_risk_signals":[],"unresolved_assumptions":[],
        "proof_status":"no_checks_run","artifact_hash":h,
        "verdicts":{"intent_alignment":"unassessed","boundary_discipline":"unassessed",
            "shortcut_risk":"unassessed","completion_proof":"unassessed"}
    });
    let mut snapshot = readiness::EvaluationSnapshot {
        binding: readiness::Binding {
            material,
            governance_digest: h.clone(),
            proof_plan_digest: expected_plan.digest()?,
            evidence_digest: evidence::digest(&suite)?,
            configuration_digest: h,
            trajectory_run_id: "protocol-fixture".into(),
        },
        context: serde_json::from_value(json!({"plan":plan,"trajectory":trajectory}))?,
        expected_plan,
        evidence: suite,
        scope: "constructed_protocol_fixture_not_execution_proof".into(),
        dispositions: vec![],
        blockers: vec![],
    };
    if variant == "stale-proof" {
        snapshot.binding.material.head_revision = "c".repeat(40);
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exposed_benchmark_and_unadmitted_sources_cannot_be_final_evidence() {
        let admission = final_admission().unwrap();
        assert_eq!(admission["final_evaluation_eligible"], false);
        assert_eq!(admission["admitted_final_units"], 0);
        assert_eq!(
            admission["required_criterion"]["minimum_zero_error_independent_accepted_units"],
            59
        );
    }
    #[test]
    fn failed_or_stale_deterministic_proof_never_calls_vega() {
        for variant in ["failed-proof", "stale-proof"] {
            let result =
                evaluate(&fixture(variant).unwrap(), |_| panic!("Vega must not run")).unwrap();
            assert_eq!(result["model_calls"], 0);
            assert_eq!(result["publication_ready"], false);
            assert_eq!(result["execution"], "blocked_before_model");
        }
    }
    #[test]
    fn actual_projection_preserves_every_obligation_before_callback() {
        for (variant, key) in [
            ("missing-proof-hook", "proof_hooks"),
            ("unresolved-question", "human_questions"),
            ("contradiction", "unresolved_contradictions"),
            ("phase-gate", "phases"),
            ("declaration-is-not-proof", "deferred_questions"),
        ] {
            let result = evaluate(&fixture(variant).unwrap(), |state| {
                let value: Value = serde_json::from_str(state)?;
                assert_eq!(value["projection"], "validation-results.v3");
                assert!(
                    !value["plan_obligations"][key]
                        .as_array()
                        .unwrap()
                        .is_empty()
                );
                assert!(
                    value["coverage"]
                        .as_str()
                        .unwrap()
                        .contains("not independent execution proof")
                );
                assert_eq!(value["validation"]["check_count"], 2);
                assert!(value.get("proposed_patch").is_none());
                Ok(json!({"test_callback_only":true}))
            })
            .unwrap();
            assert_eq!(result["model_calls"], 1);
            assert_eq!(result["publication_ready"], false);
        }
    }
    #[test]
    fn replay_matches_live_projection_and_refuses_overflow_or_changed_custody() {
        let snapshot = fixture("declaration-is-not-proof").unwrap();
        let direct = readiness::project(
            &snapshot.binding,
            &readiness::ValidationSummary {
                scope: &snapshot.scope,
                evidence: &snapshot.evidence,
                dispositions: &snapshot.dispositions,
                blockers: &snapshot.blockers,
            },
            &snapshot.context,
        )
        .unwrap();
        assert_eq!(snapshot.project().unwrap(), direct);
        for mutation in [
            "overflow",
            "trajectory",
            "intent",
            "evidence",
            "missing-check",
            "plan",
        ] {
            let mut value = serde_json::to_value(&snapshot).unwrap();
            match mutation {
                "overflow" => {
                    value["context"]["plan"]["deferred_questions"] =
                        json!(["required obligation ".repeat(32 * 1024)])
                }
                "trajectory" => value["context"]["trajectory"]["run_id"] = "different".into(),
                "intent" => value["context"]["plan"]["intent"] = " ".into(),
                "evidence" => {
                    value["binding"]["evidence_digest"] = readiness::hash(b"changed").into()
                }
                "missing-check" => {
                    value["evidence"]["results"].as_array_mut().unwrap().pop();
                }
                "plan" => value["expected_plan"]["checks"] = json!([]),
                _ => unreachable!(),
            }
            let input: readiness::EvaluationSnapshot = serde_json::from_value(value).unwrap();
            let result = evaluate(&input, |_| {
                panic!("invalid or omitted proof cannot reach inference")
            })
            .unwrap();
            assert_eq!(result["model_calls"], 0, "{mutation}");
            assert_eq!(result["publication_ready"], false);
        }
    }
}

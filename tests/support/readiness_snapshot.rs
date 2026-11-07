//! Constructed replay identities are protocol fixtures, never publication proof.
//! Both model evaluations use the shared production projection at execution.
use super::{evidence, readiness};
use serde_json::json;

pub fn fixture(
    variant: &str,
) -> std::result::Result<readiness::EvaluationSnapshot, Box<dyn std::error::Error>> {
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

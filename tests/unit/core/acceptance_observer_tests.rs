//! Deterministic observer-contract tests, not real-model acceptance evidence.
use super::*;
use crate::core::readiness_evidence::{
    ApplicabilityFacts, CheckKey, CheckKind, CheckResult, Conclusion, ExpectedCheck,
    MaterialIdentity, Provider, Requirement, SCHEMA_VERSION,
};

fn proof(head: &str, success: bool) -> Value {
    let identity = MaterialIdentity {
        repository: "owner/fixture".into(),
        work_branch: "agent/task-repair".into(),
        head_revision: head.repeat(40),
        base_branch: "main".into(),
        accepted_base_revision: "b".repeat(40),
        history_digest: digest(b"history"),
        tree_digest: digest(head.as_bytes()),
        configuration_digest: digest(b"config"),
        workflow_digest: digest(b"workflow"),
        toolchain_digest: digest(b"tools"),
        candidate_digest: digest(head.as_bytes()),
    };
    let check = ExpectedCheck {
        key: CheckKey {
            provider: Provider::Native,
            workflow: ".decapod/config.toml".into(),
            job: "accepted-contract".into(),
            matrix: BTreeMap::new(),
            profile: crate::core::repository_profile::REQUIRED_PROFILE.into(),
        },
        kind: CheckKind::RepositoryLocal,
        definition_digest: digest(b"definition"),
        invocation_digest: digest(b"invocation"),
        toolchain_digest: digest(b"tools"),
        executor: Provider::Native,
        dependencies: vec![],
        requirement: Requirement::Execute,
    };
    let plan = ExpectedPlan {
        schema_version: SCHEMA_VERSION.into(),
        policy_digest: digest(b"policy"),
        source_inventory_digest: digest(b"sources"),
        buildkite_enabled: false,
        checks: vec![check.clone()],
    };
    let suite = SuiteEvidence {
        schema_version: SCHEMA_VERSION.into(),
        expected_plan_digest: plan.digest().unwrap(),
        before: identity.clone(),
        after: identity.clone(),
        facts: ApplicabilityFacts {
            event: "pull_request".into(),
            target_branch: "main".into(),
            changed_paths: ["src/access.rs".into()].into(),
        },
        results: vec![CheckResult {
            key: check.key,
            definition_digest: check.definition_digest,
            invocation_digest: check.invocation_digest,
            toolchain_digest: check.toolchain_digest,
            executor: Provider::Native,
            identity: identity.clone(),
            observation: Observation::Executed {
                execution_id: "native-observer-control".into(),
                conclusion: if success {
                    Conclusion::Success
                } else {
                    Conclusion::Failure
                },
                exit_code: Some(if success { 0 } else { 1 }),
                output_digest: digest(b"actual output"),
                artifact_digest: None,
            },
        }],
    };
    let mut receipt = json!({"kind":"native_readiness_receipt","status":"ok","scope":"repository_local_readiness",
        "material":identity,"proof_plan_digest":plan.digest().unwrap(),"evidence_digest":readiness_evidence::digest(&suite).unwrap()});
    receipt["receipt_hash"] = readiness_evidence::digest(&receipt).unwrap().into();
    json!({"operation":"validate","exit_code":if success{0}else{1},"output":{"publication_ready":success},
        "plan":plan,"observations":suite,"receipt":receipt,"reason_codes":["READINESS_BLOCKED"]})
}

fn push(
    capture: &mut Capture,
    docs: &mut BTreeMap<String, Value>,
    source: Source,
    mut value: Value,
) {
    if source == Source::Kernel {
        value["binary_sha256"] = capture.kernel_sha256.clone().into();
        if value["host_tool_call_id"].is_null() {
            value["host_tool_call_id"] = "governance-1".into();
        }
    }
    if matches!(source, Source::Kernel | Source::Git) {
        value["repository"] = capture.repository.clone().into();
    }
    let sequence = capture.events.len() as u64 + 1;
    let name = format!("event-{sequence}");
    capture.artifacts.insert(
        name.clone(),
        Artifact {
            file: format!("{name}.json"),
            sha256: digest(&serde_json::to_vec(&value).unwrap()),
            source,
            public_uri: None,
        },
    );
    capture.events.push(EventInput {
        sequence,
        elapsed_ms: sequence,
        artifact: name.clone(),
        pointer: String::new(),
    });
    docs.insert(name, value);
}

fn model_call(id: &str, name: &str) -> Value {
    json!({"type":"message_end","message":{"role":"assistant","provider":"openai-codex","model":"pinned-test-model","stopReason":"toolUse","content":[{"type":"toolCall","id":id,"name":name,"arguments":{}}]}})
}
fn named(
    c: &mut Capture,
    d: &mut BTreeMap<String, Value>,
    name: &str,
    source: Source,
    value: Value,
) {
    push(c, d, source, value);
    let event = c.events.last_mut().unwrap();
    let old = event.artifact.clone();
    event.artifact = name.into();
    let value = d.remove(&old).unwrap();
    d.insert(name.into(), value);
    let mut artifact = c.artifacts.remove(&old).unwrap();
    artifact.file = format!("{name}.json");
    c.artifacts.insert(name.into(), artifact);
}
fn fixture() -> (Capture, BTreeMap<String, Value>) {
    let objective = "Repair repository-scoped access checks and publish a verified PR.";
    let mut c = Capture {
        schema: SCHEMA.into(),
        run_id: "observer-contract-fixture".into(),
        owner_issue: 1758,
        repository: "owner/fixture".into(),
        objective: objective.into(),
        objective_revision: 1,
        approved_objective_sha256: digest(objective.as_bytes()),
        harness_version: "0.73.1".into(),
        kernel_version: "0.148.8".into(),
        kernel_sha256: digest(b"binary"),
        kernel_source: "a".repeat(40),
        artifacts: BTreeMap::new(),
        events: vec![],
    };
    let mut d = BTreeMap::new();
    let mut add = |name, source, value| named(&mut c, &mut d, name, source, value);
    add(
        "objective",
        Source::Host,
        json!({"type":"message_end","message":{"role":"user","content":[{"type":"text","text":objective}]}}),
    );
    add(
        "initial-model",
        Source::Host,
        model_call("governance-1", "bash"),
    );
    add(
        "governance-start",
        Source::Host,
        json!({"type":"tool_execution_start","toolCallId":"governance-1","toolName":"bash","args":{}}),
    );
    add(
        "context",
        Source::Kernel,
        json!({"operation":"context","exit_code":0,"output":{}}),
    );
    add(
        "claim",
        Source::Kernel,
        json!({"operation":"claim","exit_code":0,"output":{"status":"ok","id":"task"}}),
    );
    add(
        "workspace",
        Source::Kernel,
        json!({"operation":"workspace","exit_code":0,"output":{"can_work":true,"branch":"agent/task-repair"}}),
    );
    add("failed", Source::Kernel, proof("a", false));
    add(
        "governance-end",
        Source::Host,
        json!({"type":"tool_execution_end","toolCallId":"governance-1","toolName":"bash","isError":true}),
    );
    add("repair-model", Source::Host, model_call("repair-1", "edit"));
    add(
        "repair-start",
        Source::Host,
        json!({"type":"tool_execution_start","toolCallId":"repair-1","toolName":"edit","args":{}}),
    );
    add(
        "repair-end",
        Source::Host,
        json!({"type":"tool_execution_end","toolCallId":"repair-1","toolName":"edit","isError":false}),
    );
    add(
        "source",
        Source::Git,
        json!({"head":"c".repeat(40),"base":"b".repeat(40),"clean":true,"tool_call_id":"repair-1"}),
    );
    add(
        "proof-model",
        Source::Host,
        model_call("governance-2", "bash"),
    );
    add(
        "proof-start",
        Source::Host,
        json!({"type":"tool_execution_start","toolCallId":"governance-2","toolName":"bash","args":{}}),
    );
    let mut fresh = proof("c", true);
    fresh["host_tool_call_id"] = "governance-2".into();
    add("fresh", Source::Kernel, fresh);
    add(
        "publish",
        Source::Kernel,
        json!({"host_tool_call_id":"governance-2","operation":"publish","exit_code":0,"output":{"status":"ok","commit_hash":"c".repeat(40),"branch":"agent/task-repair","pr_verified":true,"remote_verified":true,"pr_url":"https://github.com/owner/fixture/pull/2"}}),
    );
    add(
        "proof-end",
        Source::Host,
        json!({"type":"tool_execution_end","toolCallId":"governance-2","toolName":"bash","isError":false}),
    );
    add(
        "forge",
        Source::Github,
        json!({"head":"c".repeat(40),"base":"b".repeat(40),"branch":"agent/task-repair","base_branch":"main","url":"https://github.com/owner/fixture/pull/2","merged":false}),
    );
    (c, d)
}

#[test]
fn trusted_capture_correlates_historical_admission_but_never_grants_authority() {
    let (c, d) = fixture();
    let result = correlate(&c, &d).unwrap();
    assert_eq!(result["observed_autonomous_admission"], true, "{result:#}");
    assert_eq!(result["current_admission_applicable"], true);
    assert_eq!(result["publication_ready"], false);
    assert_eq!(result["merge_authorized"], false);
    assert_eq!(result, correlate(&c, &d).unwrap());
}

#[test]
fn observed_pi_codex_composite_tool_ids_preserve_the_actual_join() {
    let (c, mut d) = fixture();
    for value in d.values_mut() {
        let text = serde_json::to_string(value)
            .unwrap()
            .replace("governance-1", "call_context|fc_context")
            .replace("repair-1", "call_repair|fc_repair")
            .replace("governance-2", "call_proof|fc_proof");
        *value = serde_json::from_str(&text).unwrap();
    }
    assert_eq!(
        correlate(&c, &d).unwrap()["observed_autonomous_admission"],
        true
    );
}

#[test]
fn narration_scripted_events_missing_context_and_controller_repair_cannot_pass() {
    for control in [
        "narration",
        "scripted",
        "context",
        "controller",
        "objective",
        "tool-result",
        "attribution",
    ] {
        let (mut c, mut d) = fixture();
        match control {
            "narration" => {
                for a in c.artifacts.values_mut() {
                    a.source = Source::AgentDeclaration;
                }
            }
            "scripted" => {
                for a in c.artifacts.values_mut() {
                    if a.source == Source::Host {
                        a.source = Source::Scripted;
                    }
                }
            }
            "context" => {
                d.get_mut("context").unwrap()["operation"] = "not-context".into();
            }
            "attribution" => {
                d.get_mut("fresh")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("host_tool_call_id");
            }
            "controller" => {
                d.get_mut("source")
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove("tool_call_id");
            }
            "objective" => {
                d.get_mut("objective").unwrap()["message"]["content"][0]["text"] =
                    "A different objective".into();
            }
            "tool-result" => {
                c.events.retain(|e| e.artifact != "repair-end");
            }
            _ => unreachable!(),
        }
        assert_eq!(
            correlate(&c, &d).unwrap()["observed_autonomous_admission"],
            false,
            "{control}"
        );
    }
}

#[test]
fn missing_checks_old_receipts_wrong_forge_subject_and_created_not_admitted_fail_closed() {
    for control in [
        "missing",
        "stale-receipt",
        "wrong-head",
        "wrong-base",
        "created-only",
    ] {
        let (c, mut d) = fixture();
        match control {
            "missing" => d.get_mut("fresh").unwrap()["observations"]["results"] = json!([]),
            "stale-receipt" => {
                d.get_mut("fresh").unwrap()["receipt"] = proof("a", true)["receipt"].clone()
            }
            "wrong-head" => d.get_mut("forge").unwrap()["head"] = "d".repeat(40).into(),
            "wrong-base" => d.get_mut("forge").unwrap()["base"] = "d".repeat(40).into(),
            "created-only" => {
                *d.get_mut("publish").unwrap() = json!({"operation":"publish","repository":c.repository,"binary_sha256":c.kernel_sha256,"exit_code":1,"reason_codes":["PR_CREATED_NOT_ADMITTED","WORKUNIT_PROOF_STALE"]})
            }
            _ => unreachable!(),
        }
        let result = correlate(&c, &d).unwrap();
        assert_eq!(result["observed_autonomous_admission"], false, "{control}");
        assert_eq!(result["current_admission_applicable"], false, "{control}");
    }
}

#[test]
fn actual_source_or_target_movement_invalidates_applicability_without_erasing_history() {
    for control in ["head", "base", "dirty"] {
        let (mut c, mut d) = fixture();
        let mut state = json!({"head":"c".repeat(40),"base":"b".repeat(40),"clean":true});
        match control {
            "head" => state["head"] = "d".repeat(40).into(),
            "base" => state["base"] = "d".repeat(40).into(),
            _ => state["clean"] = false.into(),
        }
        push(&mut c, &mut d, Source::Git, state);
        let result = correlate(&c, &d).unwrap();
        assert_eq!(result["observed_autonomous_admission"], true);
        assert_eq!(result["current_admission_applicable"], false);
        assert!(
            result["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["kind"] == "evidence_became_stale")
        );
    }
}

#[test]
fn post_objective_input_and_observed_host_bypass_cannot_be_zero_intervention_success() {
    for event in [
        json!({"type":"human_intervention"}),
        json!({"type":"direct_publication"}),
        json!({"type":"denied_transition_bypass"}),
        json!({"type":"message_end","message":{"role":"user","content":[{"type":"text","text":"Here is the repair"}]}}),
    ] {
        let (mut c, mut d) = fixture();
        push(&mut c, &mut d, Source::Host, event);
        assert_eq!(
            correlate(&c, &d).unwrap()["observed_autonomous_admission"],
            false
        );
    }
}

#[test]
fn secrets_are_not_exported_and_unsafe_public_text_is_rejected() {
    let (mut c, mut d) = fixture();
    d.get_mut("initial-model").unwrap()["message"]["content"] = json!([{"type":"thinking","thinking":"private reasoning Bearer secret"},{"type":"text","text":"Password: private"}]);
    let text = serde_json::to_string(&correlate(&c, &d).unwrap()).unwrap();
    assert!(
        !text.contains("private reasoning")
            && !text.contains("Password:")
            && !text.contains("Bearer secret")
    );
    c.objective = "Bearer secret".into();
    c.approved_objective_sha256 = digest(c.objective.as_bytes());
    assert!(correlate(&c, &d).is_err());
    c.objective = "API key=sk-private-example".into();
    c.approved_objective_sha256 = digest(c.objective.as_bytes());
    assert!(correlate(&c, &d).is_err());
}

#[test]
fn missing_or_modified_capture_bytes_cannot_export_a_presentation() {
    let dir = tempfile::tempdir().unwrap();
    let (c, d) = fixture();
    let index = dir.path().join("capture.json");
    fs::write(&index, serde_json::to_vec(&c).unwrap()).unwrap();
    for (name, value) in d {
        fs::write(
            dir.path().join(format!("{name}.json")),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(
        export(&index).unwrap()["observed_autonomous_admission"],
        true
    );
    fs::write(dir.path().join("fresh.json"), b"{}").unwrap();
    assert!(export(&index).is_err());
    fs::remove_file(dir.path().join("fresh.json")).unwrap();
    assert!(export(&index).is_err());
}

#[test]
fn event_replay_nonmonotone_order_and_path_escape_are_rejected() {
    let (mut c, d) = fixture();
    c.events[1].sequence = 1;
    assert!(correlate(&c, &d).is_err());
    let (mut c, d) = fixture();
    c.events[1].elapsed_ms = 0;
    assert!(correlate(&c, &d).is_err());
    let dir = tempfile::tempdir().unwrap();
    let (mut c, _) = fixture();
    c.artifacts.get_mut("objective").unwrap().file = "../outside.json".into();
    let index = dir.path().join("capture.json");
    fs::write(&index, serde_json::to_vec(&c).unwrap()).unwrap();
    assert!(export(&index).is_err());
}

#[test]
fn an_agent_tool_before_the_failure_cannot_explain_an_operator_repair_afterward() {
    let (mut c, mut d) = fixture();
    let proposal = d["repair-model"]["message"]["content"][0].clone();
    d.get_mut("initial-model").unwrap()["message"]["content"]
        .as_array_mut()
        .unwrap()
        .push(proposal);
    c.events.retain(|e| e.artifact != "repair-model");
    assert_eq!(
        correlate(&c, &d).unwrap()["observed_autonomous_admission"],
        false
    );
}

#[test]
fn fresh_validation_does_not_reuse_an_old_publisher_response() {
    let (mut c, mut d) = fixture();
    push(
        &mut c,
        &mut d,
        Source::Host,
        model_call("governance-3", "bash"),
    );
    push(
        &mut c,
        &mut d,
        Source::Host,
        json!({"type":"tool_execution_start","toolCallId":"governance-3","toolName":"bash","args":{}}),
    );
    let mut fresh = proof("c", true);
    fresh["host_tool_call_id"] = "governance-3".into();
    push(&mut c, &mut d, Source::Kernel, fresh);
    push(
        &mut c,
        &mut d,
        Source::Host,
        json!({"type":"tool_execution_end","toolCallId":"governance-3","toolName":"bash","isError":false}),
    );
    push(
        &mut c,
        &mut d,
        Source::Github,
        json!({"head":"c".repeat(40),"base":"b".repeat(40),"branch":"agent/task-repair","base_branch":"main","url":"https://github.com/owner/fixture/pull/2","merged":false}),
    );
    let result = correlate(&c, &d).unwrap();
    assert_eq!(result["observed_autonomous_admission"], true);
    assert_eq!(result["current_admission_applicable"], false);
}

#[test]
fn foreign_kernel_repository_or_binary_cannot_join_this_run() {
    for field in ["repository", "binary_sha256"] {
        let (c, mut d) = fixture();
        d.get_mut("fresh").unwrap()[field] = "different".into();
        assert!(correlate(&c, &d).is_err());
    }
}

#[cfg(unix)]
#[test]
fn symlink_capture_is_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let (c, d) = fixture();
    let index = dir.path().join("capture.json");
    fs::write(&index, serde_json::to_vec(&c).unwrap()).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), serde_json::to_vec(&d["objective"]).unwrap()).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("objective.json")).unwrap();
    assert!(export(&index).is_err());
}

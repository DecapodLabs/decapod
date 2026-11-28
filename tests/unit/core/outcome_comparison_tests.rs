use super::*;

fn evidence(class: EvidenceClass) -> Evidence {
    Evidence {
        class,
        sha256: "a".repeat(64),
    }
}
fn unknown_metric() -> Metric {
    Metric {
        value: None,
        evidence: None,
        missing_reason: Some(MissingReason::NotObserved),
    }
}
fn unknown_verdict() -> Verdict {
    Verdict {
        value: None,
        evidence: None,
        missing_reason: Some(MissingReason::NotObserved),
    }
}
fn run(id: &str, arm: Arm) -> Run {
    Run {
        run_id: id.into(),
        pair_id: "pair-1".into(),
        task_id: "fixture-greeting".into(),
        stratum: "cold".into(),
        arm,
        host: "codex".into(),
        host_version: "0.169.0".into(),
        host_sha256: Some("b".repeat(64)),
        model: Some("fixture-model".into()),
        kernel_sha256: "c".repeat(64),
        source_commit: "d".repeat(40),
        accepted_base: "e".repeat(40),
        task_source_sha256: "f".repeat(64),
        environment_sha256: Some("a".repeat(64)),
        candidate_commit: Some("d".repeat(40)),
        candidate_source_sha256: Some("e".repeat(64)),
        candidate_dirty: Some(false),
        policy_sha256: None,
        constitution_sha256: None,
        context_sha256: None,
        identity_missing: ["policy_sha256", "constitution_sha256", "context_sha256"]
            .into_iter()
            .map(|k| (k.into(), MissingReason::NotObserved))
            .collect(),
        protocol_sha256: "b".repeat(64),
        evidence_class: EvidenceClass::RealHost,
        status: TerminalStatus::Completed,
        functional_correctness: Verdict {
            value: Some(true),
            evidence: Some(evidence(EvidenceClass::IndependentCheck)),
            missing_reason: None,
        },
        governed_completion: unknown_verdict(),
        context_delivery: unknown_verdict(),
        first_missing_boundary: None,
        selected_context_ids: vec![],
        metrics: METRICS
            .iter()
            .map(|key| ((*key).into(), unknown_metric()))
            .collect(),
    }
}
fn input(runs: Vec<Run>) -> Input {
    Input {
        schema_version: "decapod.outcome-observations.v1".into(),
        runs,
    }
}

#[test]
fn duplicate_partial_and_cancelled_runs_are_not_silently_deduplicated_or_dropped() {
    let first = run("run-1", Arm::Baseline);
    assert!(compare(input(vec![first.clone(), first])).is_err());
    let mut cancelled = run("run-2", Arm::Governed);
    cancelled.status = TerminalStatus::Cancelled;
    let report = compare(input(vec![run("run-1", Arm::Baseline), cancelled])).unwrap();
    assert_eq!(report.runs.len(), 2);
    assert_eq!(report.summaries[1].unsuccessful_or_partial, 1);
    assert_eq!(report.summaries[1].reported_independent_correct, 0);
    assert!(!report.publication_ready);
    assert!(!report.merge_authorized);
}

#[test]
fn mismatched_task_model_version_source_and_protocol_joins_fail() {
    for field in [
        "task",
        "model",
        "version",
        "source",
        "protocol",
        "environment",
    ] {
        let mut other = run("run-2", Arm::Governed);
        match field {
            "task" => other.task_id = "other-task".into(),
            "model" => other.model = Some("other-model".into()),
            "version" => other.host_version = "0.0.0".into(),
            "source" => other.task_source_sha256 = "0".repeat(64),
            "protocol" => other.protocol_sha256 = "1".repeat(64),
            _ => other.environment_sha256 = Some("2".repeat(64)),
        }
        assert!(
            compare(input(vec![run("run-1", Arm::Baseline), other])).is_err(),
            "{field}"
        );
    }
}

#[test]
fn unknown_attention_is_not_zero_and_unreviewed_human_time_cannot_create_a_ratio() {
    let report = compare(input(vec![run("run-1", Arm::Baseline)])).unwrap();
    assert_eq!(report.summaries[0].reviewed_active_human_minutes, None);
    assert_eq!(
        report.summaries[0].reported_verified_useful_work_per_reviewed_human_minute,
        None
    );
    assert_eq!(report.incomplete_pairs, vec!["pair-1"]);
    let mut unreviewed = run("run-1", Arm::Baseline);
    unreviewed.metrics.insert(
        "active_human_minutes".into(),
        Metric {
            value: Some(2.0),
            evidence: Some(evidence(EvidenceClass::OperatorReport)),
            missing_reason: None,
        },
    );
    assert!(compare(input(vec![unreviewed.clone()])).is_err());
    unreviewed
        .metrics
        .get_mut("active_human_minutes")
        .unwrap()
        .evidence = Some(evidence(EvidenceClass::HumanReview));
    let report = compare(input(vec![unreviewed])).unwrap();
    assert_eq!(
        report.summaries[0].reported_verified_useful_work_per_reviewed_human_minute,
        Some(0.5)
    );
}

#[test]
fn synthetic_and_operator_assertions_do_not_count_as_independent_real_outcomes() {
    for class in [
        EvidenceClass::DeterministicFixture,
        EvidenceClass::OperatorReport,
    ] {
        let mut record = run("run-1", Arm::Baseline);
        record.evidence_class = class;
        assert_eq!(
            compare(input(vec![record])).unwrap().summaries[0].reported_independent_correct,
            0
        );
    }
    let mut record = run("run-1", Arm::Baseline);
    record.functional_correctness.evidence = Some(evidence(EvidenceClass::OperatorReport));
    assert_eq!(
        compare(input(vec![record])).unwrap().summaries[0].reported_independent_correct,
        0
    );
}

#[test]
fn negative_clock_missing_metric_and_secret_identifiers_are_rejected() {
    let mut record = run("run-1", Arm::Baseline);
    record.metrics.insert(
        "wall_ms".into(),
        Metric {
            value: Some(-2.0),
            evidence: Some(evidence(EvidenceClass::OperatorReport)),
            missing_reason: None,
        },
    );
    assert!(compare(input(vec![record])).is_err());
    let mut record = run("run-1", Arm::Baseline);
    record.metrics.remove("input_tokens");
    assert!(compare(input(vec![record])).is_err());
    let mut record = run("run-1", Arm::Baseline);
    record.run_id = format!("ghp_{}", "x".repeat(40));
    assert!(compare(input(vec![record])).is_err());
}

#[test]
fn strict_file_input_rejects_unknown_transcript_fields_and_unsafe_links() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("records.json");
    let mut value = serde_json::to_value(input(vec![run("run-1", Arm::Baseline)])).unwrap();
    value["raw_transcript"] = serde_json::json!("must not enter compact output");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(read_and_compare(&path).is_err());
    #[cfg(unix)]
    {
        let link = temp.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_and_compare(&link).is_err());
    }
}

#[test]
fn missing_identity_is_explicit_and_never_replaced_by_a_synthetic_digest() {
    let mut run = run("unknown-identities", Arm::Governed);
    run.host_sha256 = None;
    assert!(compare(input(vec![run.clone()])).is_err());
    run.identity_missing
        .insert("host_sha256".into(), MissingReason::NotObserved);
    let report = compare(input(vec![run])).unwrap();
    assert!(report.runs[0].host_sha256.is_none());
    assert_eq!(report.unverified_pair_identities, vec!["pair-1"]);
    assert!(!report.publication_ready);
}

#[test]
fn functional_success_without_native_completion_is_not_governed_useful_work() {
    let mut record = run("run-1", Arm::Governed);
    let report = compare(input(vec![record.clone()])).unwrap();
    assert_eq!(report.summaries[1].reported_independent_correct, 1);
    assert_eq!(report.summaries[1].reported_verified_useful_work, 0);
    record.governed_completion = Verdict {
        value: Some(true),
        evidence: Some(evidence(EvidenceClass::OperatorReport)),
        missing_reason: None,
    };
    assert_eq!(
        compare(input(vec![record.clone()])).unwrap().summaries[1].reported_verified_useful_work,
        0
    );
    record.governed_completion.evidence = Some(evidence(EvidenceClass::NativeVerifier));
    assert_eq!(
        compare(input(vec![record])).unwrap().summaries[1].reported_verified_useful_work,
        1
    );
}

#[test]
fn harmful_context_requires_observed_delivery_and_reviewed_bounded_rate() {
    let mut record = run("run-1", Arm::Governed);
    record.metrics.insert(
        "harmful_context_rate".into(),
        Metric {
            value: Some(0.0),
            evidence: Some(evidence(EvidenceClass::IndependentCheck)),
            missing_reason: None,
        },
    );
    assert!(compare(input(vec![record.clone()])).is_err());
    record.context_delivery = Verdict {
        value: Some(true),
        evidence: Some(evidence(EvidenceClass::RealHost)),
        missing_reason: None,
    };
    assert!(compare(input(vec![record.clone()])).is_ok());
    record
        .metrics
        .get_mut("harmful_context_rate")
        .unwrap()
        .value = Some(1.1);
    assert!(compare(input(vec![record.clone()])).is_err());
    record
        .metrics
        .get_mut("harmful_context_rate")
        .unwrap()
        .value = Some(0.5);
    record
        .metrics
        .get_mut("harmful_context_rate")
        .unwrap()
        .evidence = Some(evidence(EvidenceClass::OperatorReport));
    assert!(compare(input(vec![record])).is_err());
}

#[test]
fn historical_measurements_preserve_observed_clocks_and_explicit_unknowns() {
    let input: Input = serde_json::from_str(include_str!(
        "../../../docs/research/host-outcome-pilot/sec12-antigravity-input.json"
    ))
    .unwrap();
    let report = compare(input).unwrap();
    let historical = serde_json::to_value(&report).unwrap();
    assert_eq!(
        historical["schema_version"],
        "decapod.outcome-comparison.v1"
    );
    assert!(historical["runs"][2]["metrics"].get("kernel_ms").is_none());

    let view = serde_json::to_value(report.measurements()).unwrap();
    assert_eq!(view["incomplete_pairs"], historical["incomplete_pairs"]);
    let governed = view["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["run_id"] == "agy-cold-governed")
        .unwrap();
    assert_eq!(governed["status"], "partial");
    assert_eq!(governed["metrics"]["wall_ms"]["value"], 162_534.156);
    assert_eq!(
        governed["metrics"]["setup_ms"]["evidence"]["class"],
        "operator_report"
    );
    assert_eq!(
        governed["metrics"]["validation_ms"]["evidence"]["class"],
        "independent_check"
    );
    assert_eq!(
        governed["metrics"]["context_bytes"]["value"].as_f64(),
        Some(15_898.0)
    );
    for name in [
        "kernel_ms",
        "process_startup_ms",
        "provider_wait_ms",
        "decapod_response_bytes",
        "repeated_setup_calls",
    ] {
        let metric = &governed["metrics"][name];
        assert!(metric["value"].is_null());
        assert!(metric["evidence"].is_null());
        assert_eq!(metric["missing_reason"], "not_observed");
    }
    assert_eq!(
        governed["metrics"]["active_human_minutes"]["missing_reason"],
        "not_reviewed"
    );
}

#[test]
fn optional_stages_remain_bounded_evidence_referenced_observations() {
    let mut record = run("run-1", Arm::Governed);
    let observed = Metric {
        value: Some(3.75),
        evidence: Some(evidence(EvidenceClass::OperatorReport)),
        missing_reason: None,
    };
    record.metrics.insert("kernel_ms".into(), observed.clone());
    let report = compare(input(vec![record.clone()])).unwrap();
    let view = serde_json::to_value(report.measurements()).unwrap();
    assert_eq!(view["runs"][0]["metrics"]["kernel_ms"]["value"], 3.75);
    assert_eq!(
        view["runs"][0]["metrics"]["kernel_ms"]["evidence"]["sha256"],
        "a".repeat(64)
    );

    for name in ["repeated_setup_calls", "undeclared_model_ms"] {
        let mut invalid = record.clone();
        invalid.metrics.insert(name.into(), observed.clone());
        assert!(compare(input(vec![invalid])).is_err(), "{name}");
    }
    record.metrics.get_mut("kernel_ms").unwrap().evidence = None;
    assert!(compare(input(vec![record])).is_err());
}

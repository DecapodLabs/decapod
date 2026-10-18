//! Bounded evaluation of frozen benchmark-derived proxies. No fitting or
//! candidate verdict is used to label a case. Policy-only cases are excluded.
use decapod::core::vega_readiness::Decision;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// Declared before any model observations, consistent with the existing frozen
// contract. This window stops new model dispatches; the kernel's original hard
// whole-worker deadline bounds active inference. Incomplete data never passes.
pub const COLLECTION_SECONDS: u64 = 600;

/// Exact one-sided 95% Clopper-Pearson upper limit. Cases from the same issue
/// count once, conservatively as false-ready if any accepted variant is wrong.
/// The interval still requires exchangeable independent issue units; this
/// curated benchmark does not establish that assumption or production risk.
fn upper_risk(errors: usize, accepted: usize) -> Option<f64> {
    if accepted == 0 || errors > accepted {
        return None;
    }
    if errors == accepted {
        return Some(1.0);
    }
    if errors == 0 {
        return Some(1.0 - (0.05_f64.ln() / accepted as f64).exp());
    }
    let cdf = |p: f64| {
        let mut term = accepted as f64 * (-p).ln_1p();
        let mut terms = vec![term];
        for i in 1..=errors {
            term += ((accepted - i + 1) as f64 / i as f64).ln() + p.ln() - (-p).ln_1p();
            terms.push(term);
        }
        let max = terms.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (max + terms.iter().map(|v| (v - max).exp()).sum::<f64>().ln()).exp()
    };
    let mut lo = errors as f64 / accepted as f64;
    let mut hi = 1.0;
    for _ in 0..80 {
        let middle = (lo + hi) * 0.5;
        if cdf(middle) > 0.05 {
            lo = middle;
        } else {
            hi = middle;
        }
    }
    Some(hi)
}

/// Deliberately omit expected labels, variant IDs, split, and label rationale.
/// The model gets requirements, patch and explicitly scoped evidence only.
fn state(case: &Value) -> Result<String> {
    let input = &case["input"];
    let value = json!({
        "intent": input["intent"],
        "proposed_patch": input["proposed_patch"],
        "additional_obligation_or_conflict": input["additional_obligation_or_conflict"],
        "evidence_authority": input["evidence_authority"],
        "benchmark_source": case["source"],
        "scope": "Benchmark requirement and explicitly constructed obligations only; upstream test oracle is reported, not a current Decapod validation receipt. No actual PR is authorized by this experiment."
    });
    Ok(serde_json::to_string(&value)?)
}

pub fn run(
    corpus: &Value,
    split: &str,
    infer: impl FnMut(&str, Instant) -> Result<Value>,
) -> Result<Value> {
    let started = Instant::now();
    run_until(
        corpus,
        split,
        infer,
        started,
        started + Duration::from_secs(COLLECTION_SECONDS),
    )
}

fn run_until(
    corpus: &Value,
    split: &str,
    mut infer: impl FnMut(&str, Instant) -> Result<Value>,
    started: Instant,
    deadline: Instant,
) -> Result<Value> {
    if !matches!(split, "development" | "holdout")
        || corpus["schema"] != "decapod.vega-evaluation-inputs.v1"
        || corpus["dataset_revision"] != super::evaluation_corpus::DATASET_REVISION
        || corpus["dataset_sha256"] != super::evaluation_corpus::DATASET_SHA256
        || corpus["split_frozen_before_inference"] != true
        || corpus["fitting_performed"] != false
        || corpus["upstream_calibration_modified"] != false
    {
        return Err(
            "evaluation requires the unchanged frozen corpus and upstream calibration".into(),
        );
    }
    let cases = corpus["cases"].as_array().ok_or("missing frozen cases")?;
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    let mut units = BTreeSet::new();
    let mut accepted_units = BTreeSet::new();
    let mut false_ready_units = BTreeSet::new();
    let mut accepted_cases = 0;
    let mut false_ready_cases = 0;
    let mut ready_cases = 0;
    let mut ready_accepts = 0;
    let mut abstentions = 0;
    let mut non_abstaining = 0;
    let mut executed = 0;
    let mut failed = 0;
    let mut unsupported = 0;
    let mut unexecuted = 0;
    let mut policy_excluded = 0;
    for case in cases.iter().filter(|case| case["split"] == split) {
        if case["deterministic_policy_case"] == true {
            policy_excluded += 1;
            continue;
        }
        let id = case["id"].as_str().ok_or("missing case ID")?;
        let unit = case["issue_lineage"].as_str().ok_or("missing issue unit")?;
        let label = case["label"].as_str().ok_or("missing label")?;
        if !matches!(label, "benchmark_resolved" | "not_ready" | "abstain")
            || case["deterministic_policy_case"] != false
            || !ids.insert(id.to_owned())
        {
            return Err("malformed or duplicate judgment case".into());
        }
        units.insert(unit.to_owned());
        ready_cases += usize::from(label == "benchmark_resolved");
        let mut record = json!({"id":id,"issue_lineage":unit,"label":label});
        if Instant::now() >= deadline {
            unexecuted += 1;
            record["execution"] = "not_executed_deadline".into();
        } else {
            let state = state(case)?;
            record["state_sha256"] = decapod::core::vega_readiness::hash(state.as_bytes()).into();
            if state.len() > 32 * 1024 {
                unsupported += 1;
                record["execution"] = "unsupported_input_bytes".into();
            } else {
                let attempt = Instant::now();
                // The caller runs real inference. A timeout, bad token count,
                // malformed outcome or abstention is never relabelled ready.
                let outcome = infer(&state, deadline).and_then(|value| {
                    let answer: decapod::core::vega_readiness::Outcome =
                        serde_json::from_value(value["outcome"].clone())?;
                    Ok((value, answer.decision().map_err(std::io::Error::other)?))
                });
                record["wall_clock_ms"] = (attempt.elapsed().as_secs_f64() * 1000.0).into();
                match outcome {
                    Ok((value, decision)) if decision != Decision::Disabled => {
                        executed += 1;
                        record["execution"] = "executed".into();
                        record["outcome"] = value["outcome"].clone();
                        record["execution_evidence"] = value["execution_evidence"].clone();
                        record["decision"] = serde_json::to_value(decision)?;
                        if decision == Decision::Abstain {
                            abstentions += 1;
                        } else {
                            non_abstaining += 1;
                        }
                        if decision == Decision::Accept {
                            accepted_cases += 1;
                            accepted_units.insert(unit.to_owned());
                            if label == "benchmark_resolved" {
                                ready_accepts += 1;
                            } else {
                                false_ready_cases += 1;
                                false_ready_units.insert(unit.to_owned());
                            }
                        }
                    }
                    other => {
                        failed += 1;
                        let error = match other {
                            Err(error) => error.to_string(),
                            Ok(_) => "disabled provider is not a model decision".into(),
                        };
                        record["execution"] = "failed".into();
                        record["error"] = error.chars().take(512).collect::<String>().into();
                    }
                }
            }
        }
        records.push(record);
    }
    let total = records.len();
    let expected_units = corpus["independent_instances"][split]
        .as_u64()
        .ok_or("missing frozen split count")? as usize;
    if units.len() != expected_units
        || total != expected_units * 5
        || policy_excluded != expected_units * 3
    {
        return Err("frozen split coverage changed".into());
    }
    let coverage = non_abstaining as f64 / total as f64;
    let ready_coverage = ready_accepts as f64 / ready_cases as f64;
    let upper = upper_risk(false_ready_units.len(), accepted_units.len());
    let criterion = split == "holdout"
        && executed == total
        && failed == 0
        && unsupported == 0
        && unexecuted == 0
        && false_ready_cases == 0
        && upper.is_some_and(|upper| upper <= 0.05)
        && coverage >= 0.5
        && ready_coverage >= 0.5;
    Ok(
        json!({"schema":"decapod.vega-readiness-evaluation.v1","split":split,
        "publication_ready":false,"production_calibration":"unsatisfied",
        "scope":"Agent-authored benchmark-derived challenge set, not independently reviewed general PR-readiness labels",
        "fitting_performed":false,"upstream_calibration_modified":false,
        "sample_units":units.len(),"judgment_cases":total,"policy_cases_excluded":policy_excluded,
        "executed":executed,"inference_failures":failed,"unsupported_inputs":unsupported,
        "not_executed_deadline":unexecuted,"abstentions":abstentions,
        "accepted_cases":accepted_cases,"false_ready_cases":false_ready_cases,
        "accepted_issue_units":accepted_units.len(),"false_ready_issue_units":false_ready_units.len(),
        "one_sided_95pct_upper_false_ready":upper,"non_abstaining_coverage":coverage,
        "benchmark_resolved_acceptance_coverage":ready_coverage,
        "frozen_numeric_criterion_met":criterion,"collection_dispatch_cutoff_seconds":COLLECTION_SECONDS,
        "execution_deadline":"Original hard kernel Rust-worker deadline; no per-case timeout reset. A synchronous in-flight inference may cross the soft dispatch cutoff.",
        "wall_clock_ms":started.elapsed().as_secs_f64()*1000.0,
        "limitations":["Issue-group interval assumes independent exchangeable units; selected repositories and related tasks may violate that assumption.",
            "Benchmark positives certify only upstream scoped test-oracle resolution, not present Decapod proof or all real-world readiness obligations.",
            "Current holdout has only 23 issue units; even zero false-ready among all accepted units cannot meet the frozen 5% upper-risk criterion.",
            "No independent backbone reference or exercised production CPU fallback is established by this evaluation."],
        "records":records}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value {
        let mut cases = Vec::new();
        for (i, label) in [
            "benchmark_resolved",
            "not_ready",
            "not_ready",
            "not_ready",
            "abstain",
        ]
        .into_iter()
        .enumerate()
        {
            cases.push(
                json!({"id":format!("unit:{i}"),"issue_lineage":"unit","label":label,
                "split":"holdout","deterministic_policy_case":false,
                "input":{"intent":"fixture"},"source":{}}),
            );
        }
        for i in 0..3 {
            cases.push(json!({"id":format!("policy:{i}"),"split":"holdout","deterministic_policy_case":true}));
        }
        json!({"schema":"decapod.vega-evaluation-inputs.v1",
            "dataset_revision":super::super::evaluation_corpus::DATASET_REVISION,
            "dataset_sha256":super::super::evaluation_corpus::DATASET_SHA256,
            "split_frozen_before_inference":true,"fitting_performed":false,
            "upstream_calibration_modified":false,"independent_instances":{"holdout":1},"cases":cases})
    }
    fn accepted() -> Value {
        json!({"outcome":{"probabilities":[0.1,0.9],"prediction_set":[true],"abstain":false,
            "unbound":false,"settle_steps":1,"energy":0.0,"kinetic":0.0,
            "nearest":0.0,"confidence":0.9},"execution_evidence":{"scope":"synthetic unit fixture only"}})
    }
    #[test]
    fn related_variants_cannot_inflate_independent_accept_count() {
        let report = run(&fixture(), "holdout", |_, _| Ok(accepted())).unwrap();
        assert_eq!(report["executed"], 5);
        assert_eq!(report["accepted_cases"], 5);
        assert_eq!(report["false_ready_cases"], 4);
        assert_eq!(report["accepted_issue_units"], 1);
        assert_eq!(report["false_ready_issue_units"], 1);
        assert_eq!(report["policy_cases_excluded"], 3);
        assert_eq!(report["publication_ready"], false);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
    }
    #[test]
    fn errors_are_not_abstentions_and_expired_budget_never_dispatches() {
        let report = run(&fixture(), "holdout", |_, _| Err("fixture failure".into())).unwrap();
        assert_eq!(report["inference_failures"], 5);
        assert_eq!(report["abstentions"], 0);
        assert_eq!(report["non_abstaining_coverage"], 0.0);
        let now = Instant::now();
        let report = run_until(
            &fixture(),
            "holdout",
            |_, _| panic!("expired evaluation dispatched"),
            now,
            now - Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(report["executed"], 0);
        assert_eq!(report["not_executed_deadline"], 5);
        assert_eq!(report["abstentions"], 0);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
    }
    #[test]
    fn exact_small_sample_risk_and_zero_coverage_do_not_pass() {
        assert_eq!(upper_risk(0, 0), None);
        assert!((upper_risk(0, 1).unwrap() - 0.95).abs() < 1e-12);
        assert!(upper_risk(0, 23).unwrap() > 0.12);
        assert!(upper_risk(0, 59).unwrap() < 0.05);
        assert!((upper_risk(1, 2).unwrap() - 0.95_f64.sqrt()).abs() < 1e-12);
        assert_eq!(upper_risk(2, 2), Some(1.0));
    }
    #[test]
    fn model_input_omits_expected_labels_and_variant_metadata() {
        let case = json!({"label":"secret-label","id":"secret-id","split":"holdout",
            "rationale":"secret-rationale","source":{"base_commit":"abc"},
            "input":{"intent":"fix","proposed_patch":"patch","evaluation_scenario":"gold",
            "additional_obligation_or_conflict":"requirement","evidence_authority":"published benchmark"}});
        let input = state(&case).unwrap();
        for leak in [
            "secret-label",
            "secret-id",
            "secret-rationale",
            "holdout",
            "gold",
        ] {
            assert!(!input.contains(leak));
        }
        assert!(input.contains("requirement"));
        assert!(input.contains("published benchmark"));
    }
}

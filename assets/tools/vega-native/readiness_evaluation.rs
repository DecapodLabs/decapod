//! Bounded evaluation of frozen benchmark-derived proxies. No fitting or
//! candidate verdict is used to label a case. Policy-only cases are excluded.
use decapod::core::vega_readiness::Decision;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shard {
    pub index: usize,
    pub count: usize,
}
impl Shard {
    fn validate(self) -> Result<()> {
        if self.count != super::scientific_collection::SHARDS || self.index >= self.count {
            return Err("unreviewed evaluation shard selector".into());
        }
        Ok(())
    }
    fn includes(self, ordinal: usize) -> bool {
        ordinal % self.count == self.index
    }
}
#[derive(Debug, thiserror::Error)]
#[error("unsupported tokenizer scope: {0}")]
pub struct UnsupportedTokens(pub String);

fn judgment_cases<'a>(corpus: &'a Value, split: &str) -> Result<Vec<&'a Value>> {
    if !matches!(split, "development" | "holdout")
        || corpus["schema"] != super::evaluation_corpus::CORPUS_SCHEMA
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
    let mut ids = BTreeSet::new();
    let mut units = BTreeSet::new();
    let mut judgments = Vec::new();
    let mut policy = 0;
    for case in cases.iter().filter(|case| case["split"] == split) {
        if case["deterministic_policy_case"] == true {
            policy += 1;
            continue;
        }
        let id = case["id"].as_str().ok_or("missing case ID")?;
        let unit = case["issue_lineage"].as_str().ok_or("missing issue unit")?;
        let label = case["label"].as_str().ok_or("missing label")?;
        if !matches!(label, "benchmark_resolved" | "not_ready" | "abstain")
            || case["deterministic_policy_case"] != false
            || !ids.insert(id)
        {
            return Err("malformed or duplicate judgment case".into());
        }
        units.insert(unit);
        judgments.push(case);
    }
    let expected = usize::try_from(
        corpus["benchmark_units"][split]
            .as_u64()
            .ok_or("missing frozen split count")?,
    )?;
    if expected == 0
        || units.len() != expected
        || judgments.len() != expected * super::evaluation_corpus::JUDGMENT_VARIANTS
        || policy != expected * 3
    {
        return Err("frozen split coverage changed".into());
    }
    Ok(judgments)
}
fn run_until(
    corpus: &Value,
    split: &str,
    infer: impl FnMut(&str, Instant) -> Result<Value>,
    started: Instant,
    deadline: Instant,
) -> Result<Value> {
    run_selected(corpus, split, infer, started, deadline, None)
}
pub fn run_shard_until(
    corpus: &Value,
    split: &str,
    shard: Shard,
    deadline: Instant,
    infer: impl FnMut(&str, Instant) -> Result<Value>,
) -> Result<Value> {
    shard.validate()?;
    run_selected(corpus, split, infer, Instant::now(), deadline, Some(shard))
}
fn run_selected(
    corpus: &Value,
    split: &str,
    mut infer: impl FnMut(&str, Instant) -> Result<Value>,
    started: Instant,
    deadline: Instant,
    shard: Option<Shard>,
) -> Result<Value> {
    let cases = judgment_cases(corpus, split)?;
    let mut records = Vec::new();
    for (ordinal, case) in cases.iter().enumerate() {
        let mut record =
            json!({"id":case["id"],"issue_lineage":case["issue_lineage"],"label":case["label"]});
        if shard.is_some_and(|s| !s.includes(ordinal)) {
            record["execution"] = "not_selected_for_shard".into();
        } else if Instant::now() >= deadline {
            record["execution"] = "not_executed_deadline".into();
        } else {
            let input = state(case)?;
            record["state_sha256"] = decapod::core::vega_readiness::hash(input.as_bytes()).into();
            if input.len() > 32 * 1024 {
                record["execution"] = "unsupported_input_bytes".into();
            } else {
                let attempt = Instant::now();
                let outcome = infer(&input, deadline).and_then(|value| {
                    let answer: decapod::core::vega_readiness::Outcome =
                        serde_json::from_value(value["outcome"].clone())?;
                    Ok((value, answer.decision().map_err(std::io::Error::other)?))
                });
                record["wall_clock_ms"] = (attempt.elapsed().as_secs_f64() * 1000.0).into();
                match outcome {
                    Ok((value, decision)) if decision != Decision::Disabled => {
                        record["execution"] = "executed".into();
                        record["outcome"] = value["outcome"].clone();
                        record["execution_evidence"] = value["execution_evidence"].clone();
                        record["decision"] = serde_json::to_value(decision)?;
                    }
                    other => {
                        let (execution, error) = match other {
                            Err(error) => (
                                if error.is::<UnsupportedTokens>() {
                                    "unsupported_input_tokens"
                                } else {
                                    "failed"
                                },
                                error.to_string(),
                            ),
                            Ok(_) => ("failed", "disabled provider is not a model decision".into()),
                        };
                        record["execution"] = execution.into();
                        record["error"] = error.chars().take(512).collect::<String>().into();
                    }
                }
            }
        }
        records.push(record);
    }
    let mut report = summarize(
        corpus,
        split,
        &records,
        shard,
        started.elapsed().as_secs_f64() * 1000.0,
    )?;
    report["available_dispatch_seconds_at_start"] = deadline
        .saturating_duration_since(started)
        .as_secs_f64()
        .into();
    Ok(report)
}

/// Pure reduction of actual recorded outcomes; never replay inference or use a
/// candidate verdict to label a case. Recheck identities, labels and decisions.
pub fn summarize(
    corpus: &Value,
    split: &str,
    records: &[Value],
    shard: Option<Shard>,
    elapsed_ms: f64,
) -> Result<Value> {
    if let Some(shard) = shard {
        shard.validate()?;
    }
    if !elapsed_ms.is_finite() || elapsed_ms < 0.0 {
        return Err("invalid evaluation timing".into());
    }
    let cases = judgment_cases(corpus, split)?;
    if records.len() != cases.len() {
        return Err("missing evaluation records".into());
    }
    let mut by_id = BTreeMap::new();
    for record in records {
        let id = record["id"].as_str().ok_or("missing recorded ID")?;
        if by_id.insert(id, record).is_some() {
            return Err("duplicate evaluation record".into());
        }
    }
    let mut units = BTreeSet::new();
    let mut accepted_units = BTreeSet::new();
    let mut false_ready_units = BTreeSet::new();
    let (
        mut executed,
        mut failed,
        mut unsupported,
        mut token_overflows,
        mut unexecuted,
        mut unselected,
    ) = (0, 0, 0, 0, 0, 0);
    let (mut accepted, mut false_ready, mut ready_cases, mut ready_accepts, mut abstentions) =
        (0, 0, 0, 0, 0);
    let (mut required_abstentions, mut correct_abstentions, mut decisive_ambiguity, mut decisive) =
        (0, 0, 0, 0);
    let mut ordered = Vec::new();
    for (ordinal, case) in cases.iter().enumerate() {
        let id = case["id"].as_str().ok_or("missing case ID")?;
        let record = *by_id.get(id).ok_or("missing expected case record")?;
        let unit = case["issue_lineage"].as_str().ok_or("missing issue unit")?;
        let label = case["label"].as_str().ok_or("missing label")?;
        if record["label"] != label || record["issue_lineage"] != unit {
            return Err("recorded label or issue lineage differs from frozen case".into());
        }
        units.insert(unit);
        ready_cases += usize::from(label == "benchmark_resolved");
        required_abstentions += usize::from(label == "abstain");
        let selected = shard.is_none_or(|s| s.includes(ordinal));
        let execution = record["execution"]
            .as_str()
            .ok_or("missing execution disposition")?;
        if !selected {
            if execution != "not_selected_for_shard"
                || record.get("outcome").is_some()
                || record.get("decision").is_some()
            {
                return Err("shard membership differs".into());
            }
            unselected += 1;
        } else {
            if execution != "not_executed_deadline"
                && record["state_sha256"]
                    != decapod::core::vega_readiness::hash(state(case)?.as_bytes())
            {
                return Err("recorded model input differs from frozen case".into());
            }
            if execution != "executed"
                && (record.get("outcome").is_some() || record.get("decision").is_some())
            {
                return Err("nonexecution cannot carry a model outcome".into());
            }
            match execution {
                "not_executed_deadline" => unexecuted += 1,
                "unsupported_input_bytes" => unsupported += 1,
                "unsupported_input_tokens" => {
                    unsupported += 1;
                    token_overflows += 1;
                }
                "failed" => failed += 1,
                "executed" => {
                    let answer: decapod::core::vega_readiness::Outcome =
                        serde_json::from_value(record["outcome"].clone())?;
                    let decision = answer.decision().map_err(std::io::Error::other)?;
                    if decision == Decision::Disabled
                        || record["decision"] != serde_json::to_value(decision)?
                    {
                        return Err("recorded decision differs from finite outcome".into());
                    }
                    executed += 1;
                    if decision == Decision::Abstain {
                        abstentions += 1;
                        correct_abstentions += usize::from(label == "abstain");
                    } else {
                        decisive += 1;
                        decisive_ambiguity += usize::from(label == "abstain");
                    }
                    if decision == Decision::Accept {
                        accepted += 1;
                        accepted_units.insert(unit);
                        if label == "benchmark_resolved" {
                            ready_accepts += 1;
                        } else {
                            false_ready += 1;
                            false_ready_units.insert(unit);
                        }
                    }
                }
                _ => return Err("unknown or unselected disposition in selected case".into()),
            }
        }
        ordered.push(record.clone());
    }
    let total = cases.len();
    let coverage = decisive as f64 / total as f64;
    let ready_coverage = if ready_cases == 0 {
        0.0
    } else {
        ready_accepts as f64 / ready_cases as f64
    };
    let upper = upper_risk(false_ready_units.len(), accepted_units.len());
    let criterion = split == "holdout"
        && executed == total
        && failed == 0
        && unsupported == 0
        && unexecuted == 0
        && unselected == 0
        && false_ready == 0
        && correct_abstentions == required_abstentions
        && upper.is_some_and(|v| v <= 0.05)
        && coverage >= 0.5
        && ready_coverage >= 0.5;
    Ok(json!({
        "schema":"decapod.vega-readiness-evaluation.v1","split":split,
        "publication_ready":false,"production_calibration":"unsatisfied",
        "scope":"Agent-authored benchmark-derived challenge set, not independently reviewed general PR-readiness labels",
        "fitting_performed":false,"upstream_calibration_modified":false,
        "sample_units":units.len(),"judgment_cases":total,"policy_cases_excluded":units.len()*3,
        "executed":executed,"inference_failures":failed,"unsupported_inputs":unsupported,
        "unsupported_input_tokens":token_overflows,"not_executed_deadline":unexecuted,
        "not_selected_for_shard":unselected,"abstentions":abstentions,
        "shard":shard.map(|s|json!({"index":s.index,"count":s.count})),
        "abstention_labels":{"required":required_abstentions,"correct":correct_abstentions,
            "incorrect_definitive_answers":decisive_ambiguity,
            "unobserved":required_abstentions-correct_abstentions-decisive_ambiguity},
        "accepted_cases":accepted,"false_ready_cases":false_ready,
        "accepted_issue_units":accepted_units.len(),"false_ready_issue_units":false_ready_units.len(),
        "one_sided_95pct_upper_false_ready":upper,"non_abstaining_coverage":coverage,
        "benchmark_resolved_acceptance_coverage":ready_coverage,
        "frozen_numeric_criterion_met":criterion,"collection_dispatch_cutoff_seconds":if shard.is_some(){Value::Null}else{json!(COLLECTION_SECONDS)},
        "execution_deadline":"Caller absolute soft dispatch deadline, capped by original hard kernel worker/global deadlines; no per-case reset. Active synchronous inference may cross soft cutoff.",
        "wall_clock_ms":elapsed_ms,
        "limitations":["Issue-group interval assumes independent exchangeable units; related tasks and selected repositories may violate this.",
            "Benchmark positives certify scoped upstream test-oracle resolution, not present Decapod proof or all real-world obligations.",
            "The 23 issue-unit holdout cannot meet the frozen 5% upper-risk criterion even with zero false-ready.",
            "This evaluation establishes neither independent backbone equivalence nor production runtime-binding/loader-companion fallback."],
        "records":ordered
    }))
}

/// Combine six disjoint current-run shards. Recompute every metric rather than
/// trusting supplied shard counters. Every frozen case appears exactly once.
pub fn aggregate(corpus: &Value, split: &str, shards: &[Value]) -> Result<Value> {
    let count = super::scientific_collection::SHARDS;
    if shards.len() != count {
        return Err("missing evaluation shard".into());
    }
    let cases = judgment_cases(corpus, split)?;
    let mut combined = Vec::new();
    let mut elapsed = 0.0;
    for (index, report) in shards.iter().enumerate() {
        if report["schema"] != "decapod.vega-readiness-evaluation.v1"
            || report["split"] != split
            || report["shard"] != json!({"index":index,"count":count})
            || report["fitting_performed"] != false
            || report["upstream_calibration_modified"] != false
        {
            return Err("duplicate, reordered or malformed evaluation shard".into());
        }
        let records = report["records"]
            .as_array()
            .ok_or("missing shard records")?;
        let wall = report["wall_clock_ms"]
            .as_f64()
            .ok_or("missing shard timing")?;
        let checked = summarize(corpus, split, records, Some(Shard { index, count }), wall)?;
        for (ordinal, record) in checked["records"]
            .as_array()
            .ok_or("missing checked records")?
            .iter()
            .enumerate()
        {
            if ordinal % count == index {
                combined.push(record.clone());
            }
        }
        elapsed += wall;
    }
    if combined.len() != cases.len() {
        return Err("incomplete shard union".into());
    }
    let mut result = summarize(corpus, split, &combined, None, elapsed)?;
    result["collection_dispatch_cutoff_seconds"] = Value::Null;
    result["execution_deadline"]="Separate command absolute soft deadlines shared by both backends, bounded by original kernel command and whole-validation deadlines; no per-case reset".into();
    result["aggregation"]="Six disjoint current-kernel-run shards; metrics rederived from recorded outcomes and frozen labels".into();
    Ok(result)
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
        json!({"schema":super::super::evaluation_corpus::CORPUS_SCHEMA,
            "dataset_revision":super::super::evaluation_corpus::DATASET_REVISION,
            "dataset_sha256":super::super::evaluation_corpus::DATASET_SHA256,
            "split_frozen_before_inference":true,"fitting_performed":false,
            "upstream_calibration_modified":false,"benchmark_units":{"holdout":1},"cases":cases})
    }
    fn accepted() -> Value {
        json!({"outcome":{"probabilities":[0.1,0.9],"prediction_set":[true],"abstain":false,
            "unbound":false,"settle_steps":1,"energy":0.0,"kinetic":0.0,
            "nearest":0.0,"confidence":0.9},"execution_evidence":{"scope":"synthetic unit fixture only"}})
    }
    #[test]
    fn related_variants_cannot_inflate_independent_accept_count() {
        let report = run(&fixture(), "holdout", |_, _| Ok(accepted())).unwrap();
        assert_eq!(report["executed"], 6);
        assert_eq!(report["accepted_cases"], 6);
        assert_eq!(report["false_ready_cases"], 5);
        assert_eq!(
            report["abstention_labels"]["incorrect_definitive_answers"],
            2
        );
        assert_eq!(report["accepted_issue_units"], 1);
        assert_eq!(report["false_ready_issue_units"], 1);
        assert_eq!(report["policy_cases_excluded"], 3);
        assert_eq!(report["publication_ready"], false);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
    }
    fn six_shards(corpus: &Value) -> Vec<Value> {
        (0..super::super::scientific_collection::SHARDS)
            .map(|index| {
                let mut calls = 0;
                let report = run_shard_until(
                    corpus,
                    "holdout",
                    Shard { index, count: 6 },
                    Instant::now() + Duration::from_secs(10),
                    |_, _| {
                        calls += 1;
                        Ok(accepted())
                    },
                )
                .unwrap();
                assert_eq!(calls, 1);
                assert_eq!(report["executed"], 1);
                assert_eq!(report["not_selected_for_shard"], 5);
                assert_eq!(report["frozen_numeric_criterion_met"], false);
                report
            })
            .collect()
    }
    #[test]
    fn disjoint_shards_rederive_full_metrics_without_inflating_independent_units() {
        let corpus = fixture();
        let mut shards = six_shards(&corpus);
        shards[0]["false_ready_cases"] = json!(0); // An untrusted counter, never authority.
        let report = aggregate(&corpus, "holdout", &shards).unwrap();
        assert_eq!(report["executed"], 6);
        assert_eq!(report["not_selected_for_shard"], 0);
        assert_eq!(report["false_ready_cases"], 5);
        assert_eq!(report["accepted_issue_units"], 1);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
        assert_eq!(report["production_calibration"], "unsatisfied");
    }
    #[test]
    fn missing_duplicate_relabelled_or_forged_decisions_never_aggregate() {
        let corpus = fixture();
        let shards = six_shards(&corpus);
        assert!(aggregate(&corpus, "holdout", &shards[..5]).is_err());
        let mut duplicate = shards.clone();
        duplicate[1] = duplicate[0].clone();
        assert!(aggregate(&corpus, "holdout", &duplicate).is_err());
        let mut relabelled = shards.clone();
        relabelled[0]["records"][0]["label"] = json!("not_ready");
        assert!(aggregate(&corpus, "holdout", &relabelled).is_err());
        let mut forged = shards.clone();
        forged[0]["records"][0]["decision"] = json!("abstain");
        assert!(aggregate(&corpus, "holdout", &forged).is_err());
        let mut missing = shards;
        missing[0]["records"].as_array_mut().unwrap().pop();
        assert!(aggregate(&corpus, "holdout", &missing).is_err());
    }
    #[test]
    fn unsupported_tokens_are_coverage_failures_not_abstentions_or_model_failures() {
        let report = run(&fixture(), "holdout", |_, _| {
            Err(UnsupportedTokens("unit fixture only".into()).into())
        })
        .unwrap();
        assert_eq!(report["unsupported_input_tokens"], 6);
        assert_eq!(report["unsupported_inputs"], 6);
        assert_eq!(report["inference_failures"], 0);
        assert_eq!(report["abstentions"], 0);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
    }
    #[test]
    fn shard_selector_and_original_expired_deadline_cannot_grant_a_new_dispatch_window() {
        let corpus = fixture();
        let expired = Instant::now() - Duration::from_secs(1);
        let report = run_shard_until(
            &corpus,
            "holdout",
            Shard { index: 0, count: 6 },
            expired,
            |_, _| panic!("expired shard dispatched"),
        )
        .unwrap();
        assert_eq!(report["not_executed_deadline"], 1);
        assert_eq!(report["not_selected_for_shard"], 5);
        assert!(
            run_shard_until(
                &corpus,
                "holdout",
                Shard { index: 6, count: 6 },
                expired,
                |_, _| unreachable!()
            )
            .is_err()
        );
        assert!(
            run_shard_until(
                &corpus,
                "holdout",
                Shard { index: 0, count: 7 },
                expired,
                |_, _| unreachable!()
            )
            .is_err()
        );
    }
    #[test]
    fn errors_are_not_abstentions_and_expired_budget_never_dispatches() {
        let report = run(&fixture(), "holdout", |_, _| Err("fixture failure".into())).unwrap();
        assert_eq!(report["inference_failures"], 6);
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
        assert_eq!(report["not_executed_deadline"], 6);
        assert_eq!(report["abstentions"], 0);
        assert_eq!(report["frozen_numeric_criterion_met"], false);
    }
    #[test]
    fn required_abstention_accuracy_is_separate_from_zero_decision_coverage() {
        let report = run(&fixture(), "holdout", |_, _| {
            let mut value = accepted();
            value["outcome"]["probabilities"] = json!([0.5, 0.5]);
            value["outcome"]["confidence"] = json!(0.5);
            value["outcome"]["abstain"] = json!(true);
            Ok(value)
        })
        .unwrap();
        assert_eq!(report["abstentions"], 6);
        assert_eq!(report["abstention_labels"]["required"], 2);
        assert_eq!(report["abstention_labels"]["correct"], 2);
        assert_eq!(
            report["abstention_labels"]["incorrect_definitive_answers"],
            0
        );
        assert_eq!(report["non_abstaining_coverage"], 0.0);
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

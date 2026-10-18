//! Public, frozen benchmark-derived inputs. Labels are an agent-authored
//! adaptation of upstream benchmark oracles, not human PR-readiness reviews.
use parquet::file::reader::{FileReader, SerializedFileReader};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs::File, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub const DATASET_REVISION: &str = "846e647b9f33c0b51b739d005d13d85493c9af09";
pub const DATASET_SHA256: &str =
    "sha256:92abca7cb527b41a9f66d03a26ce441ff7319e3a49f985998fd56be4bb9b08b2";
pub const DATASET_BYTES: u64 = 1_875_846;
pub const CORPUS_SCHEMA: &str = "decapod.vega-evaluation-inputs.v2";
pub const JUDGMENT_VARIANTS: usize = 6;
pub const DATASET_URL: &str = "https://huggingface.co/datasets/SWE-bench/SWE-bench_Multilingual/resolve/846e647b9f33c0b51b739d005d13d85493c9af09/data/test-00000-of-00001.parquet";
// Related repositories stay on the same side; every variant inherits its
// original repository/issue group. Holdout was selected before any model runs.
const DEVELOPMENT: &[&str] = &["astral-sh/ruff", "nushell/nushell", "sharkdp/bat"];
const HOLDOUT: &[&str] = &[
    "burntsushi/ripgrep",
    "tokio-rs/tokio",
    "tokio-rs/axum",
    "uutils/coreutils",
];
fn case(
    row: &Value,
    split: &str,
    variant: &str,
    label: &str,
    policy: bool,
    patch: &str,
    extra: &str,
) -> Value {
    let id = row["instance_id"].as_str().unwrap();
    json!({"id":format!("{id}:{variant}"),"split":split,"group":row["repo"],"issue_lineage":id,
    "label":label,"label_author":"agent-authored adaptation; not human-reviewed PR-readiness ground truth",
    "label_scope":"benchmark requirement and explicitly stated additional obligations only",
    "deterministic_policy_case":policy,
    "source":{"dataset_revision":DATASET_REVISION,"dataset_sha256":DATASET_SHA256,
        "repo":row["repo"],"base_commit":row["base_commit"],"instance_id":id,
        "FAIL_TO_PASS":row["FAIL_TO_PASS"],"PASS_TO_PASS":row["PASS_TO_PASS"],"test_patch":row["test_patch"]},
    "input":{"intent":row["problem_statement"],"proposed_patch":patch,
        "evidence_authority":"upstream benchmark curated oracle; not a current candidate validation receipt",
        "evaluation_scenario":variant,"additional_obligation_or_conflict":extra},
    "rationale":match variant {
        "gold"=>"Upstream reports the supplied gold patch resolves this benchmark's F2P and P2P oracle. Scope excludes untested global PR-readiness claims.",
        "unmodified"=>"The benchmark's F2P oracle identifies an unresolved obligation at the unmodified pinned base. No passing current validation is invented.",
        "missing-obligation"=>"The extra explicit obligation has no supporting evidence even if the issue-specific patch is correct.",
        "changed-requirements"=>"The amended requirement adds an obligation not covered by the accepted benchmark oracle.",
        "ambiguous"=>"The subjective amended requirement has no threshold or acceptance measurement; abstention is required.",
        "contradictory-evidence"=>"Two constructed semantic acceptance reports conflict about the same explicit requirement, and no independently checkable resolution is supplied. This is not a claim about actual upstream checks; abstention is required.",
        _=>"A failed, stale or missing required proof independently prohibits publication; exclude this from model-judgment coverage."
    }})
}
pub fn build(path: &Path) -> Result<Value> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() != DATASET_BYTES {
        return Err("dataset size/type mismatch".into());
    }
    let bytes = std::fs::read(path)?;
    if decapod::core::vega_readiness::hash(&bytes) != DATASET_SHA256 {
        return Err("dataset integrity mismatch".into());
    }
    let reader = SerializedFileReader::new(File::open(path)?)?;
    let mut cases = Vec::new();
    let mut ids = BTreeSet::new();
    let mut base_patches = BTreeSet::new();
    let mut development = 0;
    let mut holdout = 0;
    for record in reader.get_row_iter(None)? {
        let row = record?.to_json_value();
        let repo = row["repo"].as_str().ok_or("missing repository")?;
        let split = if DEVELOPMENT.contains(&repo) {
            "development"
        } else if HOLDOUT.contains(&repo) {
            "holdout"
        } else {
            continue;
        };
        let id = row["instance_id"].as_str().ok_or("missing instance ID")?;
        let base = row["base_commit"].as_str().ok_or("missing base")?;
        let patch = row["patch"].as_str().ok_or("missing patch")?;
        if base.len() != 40
            || !base.bytes().all(|b| b.is_ascii_hexdigit())
            || patch.is_empty()
            || !row["FAIL_TO_PASS"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
            || !ids.insert(id.to_owned())
            || !base_patches.insert((repo.to_owned(), base.to_owned(), patch.to_owned()))
        {
            return Err("malformed or duplicate benchmark instance".into());
        }
        if split == "development" {
            development += 1;
        } else {
            holdout += 1;
        }
        for (variant, label, policy, selected_patch, extra) in [
            (
                "gold",
                "benchmark_resolved",
                false,
                patch,
                "No extra obligation beyond the benchmark issue; benchmark acceptance is reported upstream, not rerun here.",
            ),
            (
                "unmodified",
                "not_ready",
                false,
                "",
                "The pinned base is unmodified; F2P obligations remain unresolved.",
            ),
            (
                "missing-obligation",
                "not_ready",
                false,
                patch,
                "Also supply measured peak memory under a documented workload. No measurement is supplied.",
            ),
            (
                "changed-requirements",
                "not_ready",
                false,
                patch,
                "Requirements changed after the benchmark: document and test the new API on all supported platforms. No such evidence is supplied.",
            ),
            (
                "ambiguous",
                "abstain",
                false,
                patch,
                "The change must be fast enough for our users. No workload or numeric threshold is specified.",
            ),
            (
                "contradictory-evidence",
                "abstain",
                false,
                patch,
                "Constructed semantic-evidence conflict beyond deterministic checks: report A says the patch satisfies the stated user requirement, while report B says the same behavior violates that requirement. Neither supplies a source-bound measurement or a reproducible example resolving their disagreement. These are evaluation scenarios, not fabricated claims about actual benchmark CI.",
            ),
            (
                "failed-proof",
                "not_ready",
                true,
                patch,
                "Constructed policy condition: a required check has failed. This is not a claim about an actual upstream run.",
            ),
            (
                "stale-proof",
                "not_ready",
                true,
                patch,
                "Constructed policy condition: the validation receipt names another source revision.",
            ),
            (
                "missing-proof",
                "not_ready",
                true,
                patch,
                "Constructed policy condition: required execution evidence is absent.",
            ),
        ] {
            cases.push(case(
                &row,
                split,
                variant,
                label,
                policy,
                selected_patch,
                extra,
            ));
        }
    }
    if development != 20 || holdout != 23 {
        return Err("frozen Rust corpus count changed".into());
    }
    cases.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(json!({"schema":CORPUS_SCHEMA,"dataset_url":DATASET_URL,
        "dataset_revision":DATASET_REVISION,"dataset_sha256":DATASET_SHA256,"dataset_license":"MIT; source repository patches retain their respective licences",
        "upstream_scope":"SWE-bench Multilingual manually curated issue/PR test oracles; not independent labels of general PR readiness",
        "split_frozen_before_inference":true,"fitting_performed":false,"upstream_calibration_modified":false,
        "benchmark_units":{"development":development,"holdout":holdout},
        "judgment_cases":43*JUDGMENT_VARIANTS,"policy_cases_excluded_from_judgment":43*3,
        "revision_rationale":"v2 adds contradictory semantic-evidence abstention cases before any model outputs or fitting; v1 remains in Git. Original cases and project split unchanged.",
        "caveats":["Same-issue variants are dependent; repositories share domain and possible training contamination.",
            "Source snapshots, patches and oracle tests are pinned; benchmark execution has not been repeated by this builder.",
            "Positive labels cover only the benchmark requirement, not current Decapod proof or production readiness.",
            "Even 23 independent held-out accepts with zero false-ready outcomes cannot meet a 5% one-sided 95% risk bound; variants must not inflate n."],
        "acceptance":{"zero_false_ready":true,"one_sided_95pct_upper_risk_max":0.05,
            "non_abstaining_coverage_min":0.5,"ready_case_acceptance_coverage_min":0.5,
            "production_calibration":"unsatisfied"},"cases":cases}))
}

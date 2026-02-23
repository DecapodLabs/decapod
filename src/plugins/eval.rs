use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::error;
use crate::core::store::Store;
use crate::core::time;

#[derive(Args, Debug)]
pub struct EvalCli {
    #[clap(subcommand)]
    pub command: EvalCommand,
}

#[derive(Subcommand, Debug)]
pub enum EvalCommand {
    /// Create a reproducible evaluation plan artifact
    Plan {
        #[clap(long)]
        task_set_id: String,
        #[clap(long = "task-ref")]
        task_refs: Vec<String>,
        #[clap(long, default_value_t = 5)]
        runs_per_variant: u32,
        #[clap(long)]
        model_id: String,
        #[clap(long, default_value = "unknown")]
        agent_version: String,
        #[clap(long, default_value = "unknown")]
        agent_id: String,
        #[clap(long)]
        prompt_hash: String,
        #[clap(long, default_value_t = 42)]
        seed: u64,
        #[clap(long = "tool-version")]
        tool_versions: Vec<String>,
        #[clap(long = "env")]
        env_fingerprint: Vec<String>,
        #[clap(long)]
        judge_model_id: String,
        #[clap(long)]
        judge_prompt_hash: String,
        #[clap(long, default_value_t = 3000)]
        judge_timeout_ms: u64,
    },

    /// Ingest one run result under a plan
    IngestRun {
        #[clap(long)]
        plan_id: String,
        #[clap(long)]
        run_id: String,
        #[clap(long)]
        variant: String,
        #[clap(long)]
        task_ref: String,
        #[clap(long, default_value_t = 1)]
        attempt_index: u32,
        #[clap(long)]
        status: String,
        #[clap(long)]
        failure_reason: Option<String>,
        #[clap(long, default_value_t = 0)]
        duration_ms: u64,
        #[clap(long)]
        cost_usd: Option<f64>,
        #[clap(long)]
        trace_file: Option<PathBuf>,
        #[clap(long)]
        trace_id: Option<String>,
    },

    /// Validate strict judge JSON contract and persist verdict artifact
    Judge {
        #[clap(long)]
        plan_id: String,
        #[clap(long)]
        run_id: String,
        #[clap(long)]
        json_file: Option<PathBuf>,
        #[clap(long)]
        json: Option<String>,
        #[clap(long, default_value_t = 3000)]
        timeout_ms: u64,
        #[clap(long)]
        simulate_delay_ms: Option<u64>,
    },

    /// Aggregate repeated runs and compute bootstrap confidence interval
    Aggregate {
        #[clap(long)]
        plan_id: String,
        #[clap(long, default_value = "baseline")]
        baseline_variant: String,
        #[clap(long, default_value = "candidate")]
        candidate_variant: String,
        #[clap(long, default_value_t = 400)]
        iterations: usize,
        #[clap(long)]
        aggregate_id: Option<String>,
        #[clap(long)]
        baseline_aggregate_id: Option<String>,
        #[clap(long)]
        acknowledge_setting_drift: bool,
    },

    /// Promotion gate over aggregate statistics
    Gate {
        #[clap(long)]
        aggregate_id: String,
        #[clap(long, default_value_t = 5)]
        min_runs: u32,
        #[clap(long, default_value_t = 0.0)]
        max_regression: f64,
        #[clap(long)]
        mark_required: bool,
    },

    /// Deterministically bucket failures into actionable categories
    BucketFailures {
        #[clap(long)]
        plan_id: String,
        #[clap(long, default_value = "candidate")]
        variant: String,
        #[clap(long, value_enum, default_value_t = BucketMode::Deterministic)]
        mode: BucketMode,
        #[clap(long)]
        model_id: Option<String>,
        #[clap(long)]
        prompt_hash: Option<String>,
        #[clap(long, default_value_t = 0.0)]
        temperature: f32,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum BucketMode {
    Deterministic,
    AgentAssisted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalPlan {
    pub schema_version: String,
    pub kind: String,
    pub plan_id: String,
    pub task_set_id: String,
    pub task_refs: Vec<String>,
    pub runs_per_variant: u32,
    pub created_at: String,
    pub settings: EvalSettings,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalSettings {
    pub agent_id: String,
    pub model_id: String,
    pub agent_version: String,
    pub prompt_hash: String,
    pub seed: u64,
    pub tool_versions: BTreeMap<String, String>,
    pub environment_fingerprint: BTreeMap<String, String>,
    pub judge_model_id: String,
    pub judge_prompt_hash: String,
    pub judge_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceBundle {
    pub schema_version: String,
    pub kind: String,
    pub trace_id: String,
    pub run_id: String,
    pub event_count: usize,
    pub events: Vec<TraceEvent>,
    pub attachments: Vec<TraceAttachment>,
    pub trace_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub ts_ms: u64,
    pub event_type: String,
    pub tool: Option<String>,
    pub token_in: Option<u64>,
    pub token_out: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceAttachment {
    pub kind: String,
    pub content_address: String,
    pub media_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRun {
    pub schema_version: String,
    pub kind: String,
    pub run_id: String,
    pub plan_id: String,
    pub variant: String,
    pub task_ref: String,
    pub attempt_index: u32,
    pub status: String,
    pub failure_reason: Option<String>,
    pub duration_ms: u64,
    pub cost_usd: Option<f64>,
    pub trace_bundle_ref: Option<String>,
    pub verdict_ref: Option<String>,
    pub ingested_at: String,
    pub run_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalVerdict {
    pub schema_version: String,
    pub kind: String,
    pub verdict_id: String,
    pub plan_id: String,
    pub run_id: String,
    pub success: bool,
    pub explanation: String,
    pub failure_reason: Option<String>,
    pub reached_captcha: bool,
    pub impossible_task: bool,
    pub timed_out: bool,
    pub judge_model_id: String,
    pub judge_prompt_hash: String,
    pub input_digest: String,
    pub judged_at: String,
    pub verdict_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalAggregate {
    pub schema_version: String,
    pub kind: String,
    pub aggregate_id: String,
    pub plan_id: String,
    pub plan_hash: String,
    pub baseline_variant: String,
    pub candidate_variant: String,
    pub baseline_n: u32,
    pub candidate_n: u32,
    pub baseline_success_rate: f64,
    pub candidate_success_rate: f64,
    pub delta_success_rate: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    pub bootstrap_iterations: usize,
    pub regression_flag: bool,
    pub judged_runs: u32,
    pub judge_timeout_failures: u32,
    pub computed_at: String,
    pub aggregate_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureBucketArtifact {
    pub schema_version: String,
    pub kind: String,
    pub plan_id: String,
    pub variant: String,
    pub mode: String,
    pub model_id: Option<String>,
    pub prompt_hash: Option<String>,
    pub temperature: f32,
    pub promotion_dependency_allowed: bool,
    pub total_failures: u32,
    pub buckets: Vec<FailureBucket>,
    pub computed_at: String,
    pub artifact_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureBucket {
    pub bucket_id: String,
    pub count: u32,
    pub sample_run_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvalGateRequirement {
    schema_version: String,
    kind: String,
    aggregate_id: String,
    min_runs: u32,
    max_regression: f64,
    decision_at_mark: bool,
    marked_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JudgeInput {
    success: bool,
    explanation: String,
    #[serde(default)]
    failure_reason: Option<String>,
    #[serde(default)]
    reached_captcha: bool,
    #[serde(default)]
    impossible_task: bool,
}

pub fn run_eval_cli(store: &Store, cli: EvalCli) -> Result<(), error::DecapodError> {
    match cli.command {
        EvalCommand::Plan {
            task_set_id,
            task_refs,
            runs_per_variant,
            model_id,
            agent_version,
            agent_id,
            prompt_hash,
            seed,
            tool_versions,
            env_fingerprint,
            judge_model_id,
            judge_prompt_hash,
            judge_timeout_ms,
        } => {
            let tool_versions = parse_kv_pairs(&tool_versions, "--tool-version")?;
            let env_fingerprint = parse_kv_pairs(&env_fingerprint, "--env")?;
            let settings = EvalSettings {
                agent_id,
                model_id,
                agent_version,
                prompt_hash,
                seed,
                tool_versions,
                environment_fingerprint: env_fingerprint,
                judge_model_id,
                judge_prompt_hash,
                judge_timeout_ms,
            };

            let mut task_refs_sorted = task_refs;
            task_refs_sorted.sort();
            task_refs_sorted.dedup();

            let plan_hash = hash_json(&serde_json::json!({
                "task_set_id": task_set_id,
                "task_refs": task_refs_sorted,
                "runs_per_variant": runs_per_variant,
                "settings": settings,
            }))?;
            let plan_id = format!("P_{}", &plan_hash[..12].to_uppercase());

            let plan = EvalPlan {
                schema_version: "1.0.0".to_string(),
                kind: "EVAL_PLAN".to_string(),
                plan_id: plan_id.clone(),
                task_set_id,
                task_refs: task_refs_sorted,
                runs_per_variant,
                created_at: time::now_epoch_z(),
                settings,
                plan_hash,
            };

            let path = write_json(eval_plan_path(store, &plan_id), &plan)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.plan",
                    "status": "ok",
                    "plan_id": plan.plan_id,
                    "plan_hash": plan.plan_hash,
                    "path": path,
                }))
                .unwrap()
            );
        }
        EvalCommand::IngestRun {
            plan_id,
            run_id,
            variant,
            task_ref,
            attempt_index,
            status,
            failure_reason,
            duration_ms,
            cost_usd,
            trace_file,
            trace_id,
        } => {
            let plan = load_plan(store, &plan_id)?;
            let status = normalize_status(&status)?;
            let trace_bundle_ref = if let Some(trace_file) = trace_file {
                let trace_seed =
                    fs::read_to_string(&trace_file).map_err(error::DecapodError::IoError)?;
                let trace_payload: serde_json::Value =
                    serde_json::from_str(&trace_seed).map_err(|e| {
                        error::DecapodError::ValidationError(format!(
                            "invalid trace JSON '{}': {}",
                            trace_file.display(),
                            e
                        ))
                    })?;
                let mut events = Vec::new();
                if let Some(raw_events) = trace_payload.get("events").and_then(|v| v.as_array()) {
                    for ev in raw_events {
                        events.push(TraceEvent {
                            ts_ms: ev.get("ts_ms").and_then(|v| v.as_u64()).unwrap_or(0),
                            event_type: ev
                                .get("event_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string(),
                            tool: ev
                                .get("tool")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            token_in: ev.get("token_in").and_then(|v| v.as_u64()),
                            token_out: ev.get("token_out").and_then(|v| v.as_u64()),
                            detail: ev
                                .get("detail")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        });
                    }
                }
                let mut attachments = Vec::new();
                if let Some(raw_atts) = trace_payload.get("attachments").and_then(|v| v.as_array())
                {
                    for a in raw_atts {
                        if let Some(addr) = a.get("content_address").and_then(|v| v.as_str()) {
                            attachments.push(TraceAttachment {
                                kind: a
                                    .get("kind")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("artifact")
                                    .to_string(),
                                content_address: addr.to_string(),
                                media_type: a
                                    .get("media_type")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string()),
                            });
                        }
                    }
                }

                let resolved_trace_id = trace_id.unwrap_or_else(|| format!("T_{}", run_id));
                let mut trace = TraceBundle {
                    schema_version: "1.0.0".to_string(),
                    kind: "TRACE_BUNDLE".to_string(),
                    trace_id: resolved_trace_id.clone(),
                    run_id: run_id.clone(),
                    event_count: events.len(),
                    events,
                    attachments,
                    trace_hash: String::new(),
                };
                trace.trace_hash = hash_json(&serde_json::to_value(&trace).unwrap())?;
                let trace_path = write_json(eval_trace_path(store, &resolved_trace_id), &trace)?;
                Some(trace_path)
            } else {
                None
            };

            let mut run = EvalRun {
                schema_version: "1.0.0".to_string(),
                kind: "EVAL_RUN".to_string(),
                run_id: run_id.clone(),
                plan_id: plan.plan_id,
                variant,
                task_ref,
                attempt_index,
                status,
                failure_reason,
                duration_ms,
                cost_usd,
                trace_bundle_ref,
                verdict_ref: None,
                ingested_at: time::now_epoch_z(),
                run_hash: String::new(),
            };
            run.run_hash = hash_json(&serde_json::to_value(&run).unwrap())?;

            let path = write_json(eval_run_path(store, &run_id), &run)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.ingest-run",
                    "status": "ok",
                    "path": path,
                    "run_id": run_id,
                    "run_hash": run.run_hash,
                }))
                .unwrap()
            );
        }
        EvalCommand::Judge {
            plan_id,
            run_id,
            json_file,
            json,
            timeout_ms,
            simulate_delay_ms,
        } => {
            let plan = load_plan(store, &plan_id)?;
            let mut run = load_run(store, &run_id)?;
            if run.plan_id != plan.plan_id {
                return Err(error::DecapodError::ValidationError(format!(
                    "run '{}' belongs to plan '{}', not '{}'",
                    run_id, run.plan_id, plan.plan_id
                )));
            }

            if let Some(delay_ms) = simulate_delay_ms {
                if delay_ms > timeout_ms {
                    return Err(error::DecapodError::ValidationError(format!(
                        "EVAL_JUDGE_TIMEOUT: judge execution exceeded timeout ({}ms > {}ms)",
                        delay_ms, timeout_ms
                    )));
                }
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }

            let payload_text = match (json_file, json) {
                (Some(path), None) => {
                    fs::read_to_string(&path).map_err(error::DecapodError::IoError)?
                }
                (None, Some(raw)) => raw,
                (Some(_), Some(_)) => {
                    return Err(error::DecapodError::ValidationError(
                        "provide either --json-file or --json, not both".to_string(),
                    ));
                }
                (None, None) => {
                    return Err(error::DecapodError::ValidationError(
                        "judge requires --json-file or --json input".to_string(),
                    ));
                }
            };

            let input_digest = hash_bytes(payload_text.as_bytes());
            let input: JudgeInput = serde_json::from_str(&payload_text).map_err(|e| {
                error::DecapodError::ValidationError(format!(
                    "EVAL_JUDGE_JSON_CONTRACT_ERROR: malformed judge JSON: {}",
                    e
                ))
            })?;
            if input.explanation.trim().is_empty() {
                return Err(error::DecapodError::ValidationError(
                    "EVAL_JUDGE_JSON_CONTRACT_ERROR: explanation must be non-empty".to_string(),
                ));
            }

            let timed_out = false;
            let mut verdict = EvalVerdict {
                schema_version: "1.0.0".to_string(),
                kind: "EVAL_VERDICT".to_string(),
                verdict_id: format!("V_{}", run_id),
                plan_id: plan.plan_id,
                run_id: run_id.clone(),
                success: input.success,
                explanation: input.explanation,
                failure_reason: input.failure_reason,
                reached_captcha: input.reached_captcha,
                impossible_task: input.impossible_task,
                timed_out,
                judge_model_id: plan.settings.judge_model_id,
                judge_prompt_hash: plan.settings.judge_prompt_hash,
                input_digest,
                judged_at: time::now_epoch_z(),
                verdict_hash: String::new(),
            };
            verdict.verdict_hash = hash_json(&serde_json::to_value(&verdict).unwrap())?;

            let verdict_path = write_json(eval_verdict_path(store, &run_id), &verdict)?;
            run.verdict_ref = Some(verdict_path.clone());
            run.run_hash = hash_json(&serde_json::to_value(&run).unwrap())?;
            write_json(eval_run_path(store, &run_id), &run)?;

            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.judge",
                    "status": "ok",
                    "verdict_id": verdict.verdict_id,
                    "path": verdict_path,
                    "timed_out": false,
                }))
                .unwrap()
            );
        }
        EvalCommand::Aggregate {
            plan_id,
            baseline_variant,
            candidate_variant,
            iterations,
            aggregate_id,
            baseline_aggregate_id,
            acknowledge_setting_drift,
        } => {
            let plan = load_plan(store, &plan_id)?;
            let runs = load_all_runs_for_plan(store, &plan_id)?;
            let verdicts = load_all_verdicts(store)?;

            let baseline = variant_scores(&runs, &verdicts, &baseline_variant);
            let candidate = variant_scores(&runs, &verdicts, &candidate_variant);

            if baseline.is_empty() || candidate.is_empty() {
                return Err(error::DecapodError::ValidationError(format!(
                    "aggregate requires judged runs for both variants (baseline={}, candidate={})",
                    baseline.len(),
                    candidate.len()
                )));
            }

            if let Some(base_agg_id) = baseline_aggregate_id {
                let base = load_aggregate(store, &base_agg_id)?;
                if base.plan_hash != plan.plan_hash && !acknowledge_setting_drift {
                    return Err(error::DecapodError::ValidationError(format!(
                        "EVAL_SETTINGS_MISMATCH: baseline aggregate '{}' has different plan hash ({} != {}). Use --acknowledge-setting-drift to force comparison.",
                        base_agg_id, base.plan_hash, plan.plan_hash
                    )));
                }
            }

            let (ci_low, ci_high) =
                bootstrap_delta_ci(&baseline, &candidate, iterations, plan.settings.seed);
            let baseline_rate = mean(&baseline);
            let candidate_rate = mean(&candidate);
            let delta = candidate_rate - baseline_rate;

            let judged_runs = (baseline.len() + candidate.len()) as u32;
            let judge_timeout_failures = runs
                .iter()
                .filter(|r| {
                    if let Some(verdict_path) = &r.verdict_ref
                        && let Ok(raw) = fs::read_to_string(verdict_path)
                        && let Ok(v) = serde_json::from_str::<EvalVerdict>(&raw)
                    {
                        return v.timed_out;
                    }
                    false
                })
                .count() as u32;

            let regression_flag = ci_high < 0.0;
            let computed_at = time::now_epoch_z();

            let fallback_id = format!(
                "A_{}_vs_{}_{}",
                candidate_variant,
                baseline_variant,
                &hash_json(&serde_json::json!({
                    "plan_id": plan.plan_id,
                    "baseline": baseline_variant,
                    "candidate": candidate_variant,
                    "at": computed_at,
                }))?[..10]
            );

            let mut agg = EvalAggregate {
                schema_version: "1.0.0".to_string(),
                kind: "EVAL_AGGREGATE".to_string(),
                aggregate_id: aggregate_id.unwrap_or(fallback_id),
                plan_id: plan.plan_id,
                plan_hash: plan.plan_hash,
                baseline_variant,
                candidate_variant,
                baseline_n: baseline.len() as u32,
                candidate_n: candidate.len() as u32,
                baseline_success_rate: baseline_rate,
                candidate_success_rate: candidate_rate,
                delta_success_rate: delta,
                ci_low,
                ci_high,
                bootstrap_iterations: iterations,
                regression_flag,
                judged_runs,
                judge_timeout_failures,
                computed_at,
                aggregate_hash: String::new(),
            };
            agg.aggregate_hash = hash_json(&serde_json::to_value(&agg).unwrap())?;

            let path = write_json(eval_aggregate_path(store, &agg.aggregate_id), &agg)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.aggregate",
                    "status": "ok",
                    "path": path,
                    "aggregate_id": agg.aggregate_id,
                    "delta_success_rate": agg.delta_success_rate,
                    "ci": [agg.ci_low, agg.ci_high],
                    "baseline_n": agg.baseline_n,
                    "candidate_n": agg.candidate_n,
                }))
                .unwrap()
            );
        }
        EvalCommand::Gate {
            aggregate_id,
            min_runs,
            max_regression,
            mark_required,
        } => {
            let agg = load_aggregate(store, &aggregate_id)?;
            let (pass, reasons) = evaluate_gate_decision(&agg, min_runs, max_regression);

            if mark_required {
                let required = EvalGateRequirement {
                    schema_version: "1.0.0".to_string(),
                    kind: "EVAL_GATE_REQUIREMENT".to_string(),
                    aggregate_id: aggregate_id.clone(),
                    min_runs,
                    max_regression,
                    decision_at_mark: pass,
                    marked_at: time::now_epoch_z(),
                };
                write_json(eval_gate_requirement_path(store), &required)?;
            }

            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.gate",
                    "status": if pass { "ok" } else { "failed" },
                    "aggregate_id": aggregate_id,
                    "pass": pass,
                    "reasons": reasons,
                    "min_runs": min_runs,
                    "max_regression": max_regression,
                    "marked_required": mark_required,
                }))
                .unwrap()
            );

            if !pass {
                return Err(error::DecapodError::ValidationError(
                    "EVAL_GATE_FAILED: promotion gate rejected aggregate".to_string(),
                ));
            }
        }
        EvalCommand::BucketFailures {
            plan_id,
            variant,
            mode,
            model_id,
            prompt_hash,
            temperature,
        } => {
            let runs = load_all_runs_for_plan(store, &plan_id)?;
            let verdicts = load_all_verdicts(store)?;

            if matches!(mode, BucketMode::AgentAssisted)
                && (model_id.is_none() || prompt_hash.is_none())
            {
                return Err(error::DecapodError::ValidationError(
                    "agent-assisted bucketing requires --model-id and --prompt-hash".to_string(),
                ));
            }

            let mut reasons: Vec<(String, String)> = Vec::new();
            for run in runs.iter().filter(|r| r.variant == variant) {
                let verdict = verdicts.get(&run.run_id);
                let success = verdict
                    .map(|v| v.success)
                    .unwrap_or_else(|| run.status == "pass");
                if success {
                    continue;
                }
                let reason = verdict
                    .and_then(|v| v.failure_reason.clone())
                    .or_else(|| run.failure_reason.clone())
                    .unwrap_or_else(|| "unspecified_failure".to_string());
                reasons.push((run.run_id.clone(), reason));
            }

            let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
            for (run_id, reason) in reasons {
                let bucket = classify_failure(&reason);
                grouped.entry(bucket).or_default().push(run_id);
            }

            let mut buckets: Vec<FailureBucket> = grouped
                .into_iter()
                .map(|(bucket_id, mut run_ids)| {
                    run_ids.sort();
                    let count = run_ids.len() as u32;
                    let sample_run_ids = run_ids.into_iter().take(3).collect();
                    FailureBucket {
                        bucket_id,
                        count,
                        sample_run_ids,
                    }
                })
                .collect();
            buckets.sort_by(|a, b| a.bucket_id.cmp(&b.bucket_id));

            let mut artifact = FailureBucketArtifact {
                schema_version: "1.0.0".to_string(),
                kind: "FAILURE_BUCKETS".to_string(),
                plan_id: plan_id.clone(),
                variant: variant.clone(),
                mode: match mode {
                    BucketMode::Deterministic => "deterministic".to_string(),
                    BucketMode::AgentAssisted => "agent-assisted".to_string(),
                },
                model_id,
                prompt_hash,
                temperature,
                promotion_dependency_allowed: matches!(mode, BucketMode::Deterministic),
                total_failures: buckets.iter().map(|b| b.count).sum(),
                buckets,
                computed_at: time::now_epoch_z(),
                artifact_hash: String::new(),
            };
            artifact.artifact_hash = hash_json(&serde_json::to_value(&artifact).unwrap())?;

            let path = write_json(eval_bucket_path(store, &plan_id, &variant), &artifact)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "cmd": "eval.bucket-failures",
                    "status": "ok",
                    "path": path,
                    "total_failures": artifact.total_failures,
                    "promotion_dependency_allowed": artifact.promotion_dependency_allowed,
                }))
                .unwrap()
            );
        }
    }
    Ok(())
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "eval",
        "version": "0.1.0",
        "description": "Variance-aware evaluation artifacts and promotion gates",
        "commands": [
            {"name": "plan", "parameters": ["task_set_id", "task_refs", "runs_per_variant", "settings"]},
            {"name": "ingest-run", "parameters": ["plan_id", "run_id", "variant", "task_ref", "status", "trace"]},
            {"name": "judge", "parameters": ["plan_id", "run_id", "json", "timeout_ms"]},
            {"name": "aggregate", "parameters": ["plan_id", "baseline_variant", "candidate_variant", "iterations"]},
            {"name": "gate", "parameters": ["aggregate_id", "min_runs", "max_regression", "mark_required"]},
            {"name": "bucket-failures", "parameters": ["plan_id", "variant", "mode"]}
        ],
        "artifacts": ["EVAL_PLAN", "EVAL_RUN", "EVAL_VERDICT", "EVAL_AGGREGATE", "TRACE_BUNDLE", "FAILURE_BUCKETS"],
        "storage": ["eval/plans", "eval/runs", "eval/verdicts", "eval/aggregates", "eval/traces", "eval/failure_buckets"]
    })
}

pub fn verify_eval_gate_for_publish(store_root: &Path) -> Result<(), error::DecapodError> {
    let req_path = eval_gate_requirement_path_from_store_root(store_root);
    if !req_path.exists() {
        return Ok(());
    }

    let raw = fs::read_to_string(&req_path).map_err(error::DecapodError::IoError)?;
    let req: EvalGateRequirement = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "Invalid eval gate requirement artifact {}: {}",
            req_path.display(),
            e
        ))
    })?;

    let agg_path = eval_aggregate_path_from_store_root(store_root, &req.aggregate_id);
    if !agg_path.exists() {
        return Err(error::DecapodError::ValidationError(format!(
            "Cannot publish: required eval aggregate '{}' missing at {}",
            req.aggregate_id,
            agg_path.display()
        )));
    }
    let raw = fs::read_to_string(&agg_path).map_err(error::DecapodError::IoError)?;
    let agg: EvalAggregate = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "Invalid eval aggregate artifact {}: {}",
            agg_path.display(),
            e
        ))
    })?;

    let (pass, reasons) = evaluate_gate_decision(&agg, req.min_runs, req.max_regression);
    if !pass {
        return Err(error::DecapodError::ValidationError(format!(
            "Cannot publish: eval gate failed for aggregate '{}': {}",
            req.aggregate_id,
            reasons.join(" | ")
        )));
    }
    Ok(())
}

pub fn validate_eval_gate_if_required(
    store_root: &Path,
) -> Result<Vec<String>, error::DecapodError> {
    let req_path = eval_gate_requirement_path_from_store_root(store_root);
    if !req_path.exists() {
        return Ok(vec![]);
    }

    let raw = fs::read_to_string(&req_path).map_err(error::DecapodError::IoError)?;
    let req: EvalGateRequirement = serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "Invalid eval gate requirement artifact {}: {}",
            req_path.display(),
            e
        ))
    })?;

    let agg = load_aggregate_from_store_root(store_root, &req.aggregate_id)?;
    let (pass, reasons) = evaluate_gate_decision(&agg, req.min_runs, req.max_regression);
    if pass {
        Ok(vec![])
    } else {
        Ok(vec![format!(
            "Required eval gate failed (aggregate={}): {}",
            req.aggregate_id,
            reasons.join(" | ")
        )])
    }
}

fn evaluate_gate_decision(
    aggregate: &EvalAggregate,
    min_runs: u32,
    max_regression: f64,
) -> (bool, Vec<String>) {
    let mut reasons = Vec::new();
    if aggregate.baseline_n < min_runs {
        reasons.push(format!(
            "baseline_n {} is below minimum {}",
            aggregate.baseline_n, min_runs
        ));
    }
    if aggregate.candidate_n < min_runs {
        reasons.push(format!(
            "candidate_n {} is below minimum {}",
            aggregate.candidate_n, min_runs
        ));
    }
    if aggregate.bootstrap_iterations == 0 {
        reasons.push("bootstrap_iterations must be > 0".to_string());
    }
    if aggregate.judge_timeout_failures > 0 {
        reasons.push(format!(
            "judge_timeout_failures must be 0 (got {})",
            aggregate.judge_timeout_failures
        ));
    }
    if aggregate.ci_high < -max_regression {
        reasons.push(format!(
            "regression detected: CI upper {:.4} < -max_regression {:.4}",
            aggregate.ci_high, max_regression
        ));
    }
    (reasons.is_empty(), reasons)
}

fn variant_scores(
    runs: &[EvalRun],
    verdicts: &HashMap<String, EvalVerdict>,
    variant: &str,
) -> Vec<f64> {
    let mut out = Vec::new();
    for run in runs.iter().filter(|r| r.variant == variant) {
        if let Some(verdict) = verdicts.get(&run.run_id) {
            out.push(if verdict.success { 1.0 } else { 0.0 });
        }
    }
    out
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn bootstrap_delta_ci(
    baseline: &[f64],
    candidate: &[f64],
    iterations: usize,
    seed: u64,
) -> (f64, f64) {
    let n_b = baseline.len();
    let n_c = candidate.len();
    if n_b == 0 || n_c == 0 || iterations == 0 {
        return (0.0, 0.0);
    }

    let mut state = seed.max(1);
    let mut samples = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let mut b_sum = 0.0;
        let mut c_sum = 0.0;
        for _ in 0..n_b {
            state = xorshift64(state);
            let idx = (state as usize) % n_b;
            b_sum += baseline[idx];
        }
        for _ in 0..n_c {
            state = xorshift64(state);
            let idx = (state as usize) % n_c;
            c_sum += candidate[idx];
        }
        samples.push((c_sum / n_c as f64) - (b_sum / n_b as f64));
    }

    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let low_idx = ((iterations as f64) * 0.025).floor() as usize;
    let high_idx = ((iterations as f64) * 0.975).ceil() as usize;
    let hi = high_idx.min(iterations.saturating_sub(1));
    (samples[low_idx.min(hi)], samples[hi])
}

fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn classify_failure(reason: &str) -> String {
    let r = reason.to_ascii_lowercase();
    if r.contains("captcha") || r.contains("cloudflare") {
        return "captcha_or_bot_protection".to_string();
    }
    if r.contains("timeout") || r.contains("timed out") {
        return "timeout_or_latency".to_string();
    }
    if r.contains("selector") || r.contains("element") || r.contains("dom") {
        return "selector_or_dom_drift".to_string();
    }
    if r.contains("auth") || r.contains("login") || r.contains("permission") {
        return "auth_or_permission".to_string();
    }
    if r.contains("network") || r.contains("dns") || r.contains("connection") {
        return "network_or_service".to_string();
    }
    "other".to_string()
}

fn normalize_status(status: &str) -> Result<String, error::DecapodError> {
    match status {
        "pass" | "fail" => Ok(status.to_string()),
        _ => Err(error::DecapodError::ValidationError(format!(
            "invalid status '{}': expected pass|fail",
            status
        ))),
    }
}

fn parse_kv_pairs(
    raw: &[String],
    flag: &str,
) -> Result<BTreeMap<String, String>, error::DecapodError> {
    let mut out = BTreeMap::new();
    for entry in raw {
        let mut parts = entry.splitn(2, '=');
        let key = parts.next().unwrap_or_default().trim();
        let val = parts.next().unwrap_or_default().trim();
        if key.is_empty() || val.is_empty() {
            return Err(error::DecapodError::ValidationError(format!(
                "invalid {} entry '{}': expected key=value",
                flag, entry
            )));
        }
        out.insert(key.to_string(), val.to_string());
    }
    Ok(out)
}

fn write_json<T: Serialize>(path: PathBuf, value: &T) -> Result<String, error::DecapodError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(error::DecapodError::IoError)?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to serialize eval artifact: {}", e))
    })?;
    fs::write(&path, bytes).map_err(error::DecapodError::IoError)?;
    Ok(path.to_string_lossy().to_string())
}

fn hash_json(value: &serde_json::Value) -> Result<String, error::DecapodError> {
    let bytes = serde_json::to_vec(value).map_err(|e| {
        error::DecapodError::ValidationError(format!("failed to canonicalize eval JSON: {}", e))
    })?;
    Ok(hash_bytes(&bytes))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn load_plan(store: &Store, plan_id: &str) -> Result<EvalPlan, error::DecapodError> {
    load_json(eval_plan_path(store, plan_id), "EVAL_PLAN")
}

fn load_run(store: &Store, run_id: &str) -> Result<EvalRun, error::DecapodError> {
    load_json(eval_run_path(store, run_id), "EVAL_RUN")
}

fn load_aggregate(store: &Store, aggregate_id: &str) -> Result<EvalAggregate, error::DecapodError> {
    load_json(eval_aggregate_path(store, aggregate_id), "EVAL_AGGREGATE")
}

fn load_aggregate_from_store_root(
    store_root: &Path,
    aggregate_id: &str,
) -> Result<EvalAggregate, error::DecapodError> {
    load_json(
        eval_aggregate_path_from_store_root(store_root, aggregate_id),
        "EVAL_AGGREGATE",
    )
}

fn load_json<T: for<'de> Deserialize<'de>>(
    path: PathBuf,
    kind: &str,
) -> Result<T, error::DecapodError> {
    let raw = fs::read_to_string(&path).map_err(error::DecapodError::IoError)?;
    serde_json::from_str(&raw).map_err(|e| {
        error::DecapodError::ValidationError(format!(
            "invalid {} artifact {}: {}",
            kind,
            path.display(),
            e
        ))
    })
}

fn load_all_runs_for_plan(
    store: &Store,
    plan_id: &str,
) -> Result<Vec<EvalRun>, error::DecapodError> {
    let mut runs = Vec::new();
    let dir = eval_runs_dir(store);
    if !dir.exists() {
        return Ok(runs);
    }
    for entry in fs::read_dir(dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let run: EvalRun = load_json(path, "EVAL_RUN")?;
        if run.plan_id == plan_id {
            runs.push(run);
        }
    }
    runs.sort_by(|a, b| a.run_id.cmp(&b.run_id));
    Ok(runs)
}

fn load_all_verdicts(store: &Store) -> Result<HashMap<String, EvalVerdict>, error::DecapodError> {
    let mut verdicts = HashMap::new();
    let dir = eval_verdicts_dir(store);
    if !dir.exists() {
        return Ok(verdicts);
    }
    for entry in fs::read_dir(dir).map_err(error::DecapodError::IoError)? {
        let entry = entry.map_err(error::DecapodError::IoError)?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let v: EvalVerdict = load_json(path, "EVAL_VERDICT")?;
        verdicts.insert(v.run_id.clone(), v);
    }
    Ok(verdicts)
}

fn eval_root(store: &Store) -> PathBuf {
    store.root.join("eval")
}

fn eval_root_from_store_root(store_root: &Path) -> PathBuf {
    store_root.join("eval")
}

fn eval_plan_path(store: &Store, plan_id: &str) -> PathBuf {
    eval_root(store)
        .join("plans")
        .join(format!("{}.json", plan_id))
}

fn eval_runs_dir(store: &Store) -> PathBuf {
    eval_root(store).join("runs")
}

fn eval_run_path(store: &Store, run_id: &str) -> PathBuf {
    eval_runs_dir(store).join(format!("{}.json", run_id))
}

fn eval_trace_path(store: &Store, trace_id: &str) -> PathBuf {
    eval_root(store)
        .join("traces")
        .join(format!("{}.json", trace_id))
}

fn eval_verdicts_dir(store: &Store) -> PathBuf {
    eval_root(store).join("verdicts")
}

fn eval_verdict_path(store: &Store, run_id: &str) -> PathBuf {
    eval_verdicts_dir(store).join(format!("{}.json", run_id))
}

fn eval_aggregate_path(store: &Store, aggregate_id: &str) -> PathBuf {
    eval_root(store)
        .join("aggregates")
        .join(format!("{}.json", aggregate_id))
}

fn eval_aggregate_path_from_store_root(store_root: &Path, aggregate_id: &str) -> PathBuf {
    eval_root_from_store_root(store_root)
        .join("aggregates")
        .join(format!("{}.json", aggregate_id))
}

fn eval_bucket_path(store: &Store, plan_id: &str, variant: &str) -> PathBuf {
    eval_root(store)
        .join("failure_buckets")
        .join(format!("{}_{}.json", plan_id, variant))
}

fn eval_gate_requirement_path(store: &Store) -> PathBuf {
    eval_root(store).join("gate.required.json")
}

fn eval_gate_requirement_path_from_store_root(store_root: &Path) -> PathBuf {
    eval_root_from_store_root(store_root).join("gate.required.json")
}

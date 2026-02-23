# EVAL_TRANSLATION_MAP.md - Browser-Agent Evaluation to Decapod Kernel

- Variance-heavy web tasks -> `EVAL_PLAN` + repeated `EVAL_RUN` artifacts with CI-based `EVAL_AGGREGATE`.
- Reproducible settings -> plan-level captured model/agent/judge/tool/env/seed fields with deterministic `plan_hash`.
- Judge-as-validation -> `decapod eval judge` strict JSON contract persisted as `EVAL_VERDICT`.
- Observability traces -> `TRACE_BUNDLE` artifacts with standardized events + content-addressed attachments.
- Failure reason clustering -> `decapod eval bucket-failures` deterministic buckets persisted as `FAILURE_BUCKETS`.
- Regression prevention on PR/publish -> `decapod eval gate` + optional required gate artifact checked by `validate` and `workspace publish`.
- Optional external platforms -> adapter sinks only; promotion authority remains repo-native artifacts.


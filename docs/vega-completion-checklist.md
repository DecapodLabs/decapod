# Issue #1445 delivery checklist

The deliverable is one working optional Vega post-validation integration, on
`agent/vega-followup/feat_01m52fnwybfvhfdj`, original transferred task
`feat_01m52fnwybfvhfdj`. An inactive or advisory-only delivery is not accepted.
The agent publishes after complete exact-candidate validation; the maintainer
retains merge and release authority. No issue is closed by this checklist.

## Issue boundaries

GitHub's #1445 child-issue list is empty. Its cross-reference timeline identifies
accepted prerequisites, not an additional unimplemented Vega issue tree:

| Issue / PR | Contribution | Disposition in this delivery |
|---|---|---|
| #1445 | Rust Vega post-validation decisions, portable configuration, qualified native execution and readiness evidence | Sole outstanding feature issue; remains open until all acceptance conditions pass |
| #1448, #1450 | Independently accepted source-bound CI API attestation and activation | Merged prerequisites; use their released enforcement, do not duplicate |
| #1452 / #1453 | Task-scoped custody transfer and bounded durable-export recovery | Issue closed, PR merged, released v0.118.0; original transfer already completed |
| #1455 | Required separately locked native Rust worker validation | Merged, released v0.119.0; current recipe uses this support |
| #1457 / #1458 | Exact reviewed golden-fixture link in immutable native snapshots | Issue closed, PR merged, released v0.119.1; native snapshots have executed |
| #1052, #1069, #1070, #1398 | Broader deterministic governance, complete evidence and execution parity | Consumed authority boundaries; this feature does not claim their complete remaining scope |
| #1399 | Derived-release prediction and full packaging expansion | Distinct release work, not Vega implementation or a closing reference for this PR |

The older native-proof branch is superseded by accepted prerequisite production
changes. No new transfer, controller prerequisite, ownership or branch is needed.
Accepted master `7371ad25` is integrated and the controller is official v0.119.1.

## Completion map

| Requirement | Implementation / regression coverage | Executed evidence and outstanding condition |
|---|---|---|
| Optional team/init selection; independent local installations | `lib.rs::retire_decision_provider`, `tests/init_config_behavior.rs`, `core/vega_readiness.rs`, `vega_runtime.rs` | 9f source CI passed; current exact-candidate run required. Preserve unknown/custom configuration; known retired Jev settings removed |
| Single-link immutable artifacts, offline reuse, integrity and atomic activation | `vega_setup.rs::copy_component`, tests `unsafe_hardlinked_components_remain_rejected`, `cancelled_copy_never_publishes_partial_artifact_and_cache_can_retry`, `failed_install_preserves_prior_activation_and_reusable_cache` | 9f source suite passed; no relaxation of `nlink`, no weights in crate, no decision-time update |
| Bounded production intent/obligations/observations | `vega_readiness::project`, `ProjectionContext`, `EvaluationSnapshot::project`; `production_evaluation` regressions | New shared-code repair authored. Proof hooks/questions/contradictions/phase gates retained; declarations explicitly not independent execution proof; byte/token overflow rejects |
| Deterministic failure before model; stale/malformed/abstaining fail closed | `readiness_profile::finish`, `validation_runtime`, `Response::validated_outcome`, projection and production replay tests | Existing authority regressions passed at 9f; new shared response and zero-model-call cases await prescribed validation |
| Metal, explicit CPU and safe fallback | Separate `vega_infer.rs`, `decoder.rs`, production runtime binding and retry classifiers | 9f actual M1 Max model inference and CPU executed. Unsupported-CUDA → CPU component path exercised. NVIDIA hardware/full Linux companion-loader path not measured |
| Tokenizer/blocks/pooling/engine/calibrated output parity | Separate Rust byte-BPE/scalar-F64 references; native recurrence checks | 9f fixed 138/314/4096-token Metal+CPU comparisons passed; 4097 rejected. Same-agent authorship/shared generic infrastructure and untested modes remain limits |
| Original validation deadline throughout native discovery | `rust_worker::execute` passes absolute deadline; `supplemental_discovery_stops_slow_rust_probe_at_original_deadline` | Already on the feature branch and exercised by source validation; no new prerequisite |
| Actual production-shaped model study | `production_evaluation::evaluate` calls the real projection; native collector calls actual `inference_response` and shared `validated_outcome` | New Metal/CPU development protocol collector authored, not yet measured. Constructed observations count as zero independently labelled units |
| Qualified readiness profile, separate fitting/calibration/final evaluation, unchanged risk/coverage | Pinned base-engine route; original calibration unchanged; reviewed source admission ledger; final scientific gate | **Unsatisfied.** Exposed v2 is diagnostic, not final evidence. Existing 24 audited source candidates admit zero complete production readiness units. No fitting or new untouched final study is claimed |
| Agent repair guidance and PR publication | `ValidationGuidance::for_vega_outcome`, existing publisher and full `decapod validate` | Source/diagnostic success cannot publish. Required final evaluation remains a failing/missing condition; no feature PR yet |

## Evidence custody

The last complete run before this repair is `9f017d31`:
[Buildkite 2276](https://buildkite.com/decapodlabs/decapod/builds/2276), all 39
source steps and 49 worker tests passed. Three fixed numerical fixtures passed
on both actual Metal and CPU. Full report
`vega-1191-validation-20261019T232350Z`, SHA256
`bfbceed66da9bc6b789c937bfb204f7c84d3ea5a6f9d29e1e204d51a989592e5`,
failed the scientific acceptance gate. Those failures remain intact. The later
32ba documentation checkpoint did not rerun or pass full validation.

The new collector must not repeat the exposed full-patch v2 benchmark as an
untouched holdout. It preserves all old inputs/results and continues independent
numerical/native checks, plus shared-production protocol diagnostics. Final
acceptance explicitly reports missing independently supported production units.
This is an evidence-admission correction, not a disabled/advisory product mode
or a passing replacement study. New measurements belong to their exact new SHA.

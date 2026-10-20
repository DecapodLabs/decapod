# Issue #1445 delivery checklist

The deliverable is one working optional Vega post-validation integration, on
`agent/vega-followup/feat_01m52fnwybfvhfdj`, original transferred task
`feat_01m52fnwybfvhfdj`. An inactive or advisory-only delivery is not accepted.
The agent publishes after complete exact-candidate validation; the maintainer
retains merge and release authority. No issue is closed by this checklist.

The maintainer's explicit 2026-10-20 acceptance revision replaces the
implementation-added 59-acceptance/5%-risk/95%-confidence/50%-coverage research
prerequisite with functional active integration acceptance. This does **not** pass
the old criterion or establish domain-calibrated accuracy. Historical failures,
model/calibration pins and numerical tolerances remain unchanged. Current-candidate
functional cases must actually pass; no inactive/advisory substitute or forced
model verdict is accepted.

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
| #1024 | Init/re-init preserves authored project configuration | Implement the Vega selection/retired-provider portion; no broader lifecycle closure |
| #779, #1064, #1084 | Existing provider transport, publication and obligation surfaces | Consume existing behavior; no unrelated infrastructure or automatic closure |
| #1399 | Derived-release prediction and full packaging expansion | Distinct release work, not Vega implementation or a closing reference for this PR |

The older native-proof branch is superseded by accepted prerequisite production
changes. No new transfer, controller prerequisite, ownership or branch is needed.
Accepted master `7371ad25` is integrated and the controller is official v0.119.1.

## Completion map

| Requirement | Implementation / regression coverage | Executed evidence and outstanding condition |
|---|---|---|
| Optional team/init selection; independent local installations (#1024 portion) | `lib.rs::retire_decision_provider`, `tests/init_config_behavior.rs`, `core/vega_readiness.rs`, `vega_runtime.rs` | fef source suite passed; final exact-candidate run required. Preserve unknown/custom configuration; known retired Jev settings removed |
| Single-link immutable artifacts, offline reuse, integrity and atomic activation | `vega_setup.rs::copy_component`, tests `unsafe_hardlinked_components_remain_rejected`, `cancelled_copy_never_publishes_partial_artifact_and_cache_can_retry`, `failed_install_preserves_prior_activation_and_reusable_cache` | fef source suite passed; no relaxation of `nlink`, no weights in crate, no decision-time update |
| Bounded production intent/obligations/observations (#1084 boundary) | `vega_readiness::project`, `ProjectionContext`, `EvaluationSnapshot::project`; `production_evaluation` regressions | fef shared-code tests and actual Metal/CPU protocol execution passed. Proof hooks/questions/contradictions/phase gates retained; declarations are not execution; overflow rejects |
| Deterministic failure before model; stale/malformed/abstaining fail closed (#1052/#1069/#1070 portions) | `readiness_profile::finish`, `validation_runtime`, `Response::validated_outcome`, publication/receipt invalidation and replay tests | fef 1463 library and53worker tests passed; failed/stale protocol proof made zero model calls. Final current candidate must pass again |
| Metal, explicit CPU and safe fallback | Separate `vega_infer.rs`, `decoder.rs`, production runtime binding and retry classifiers | fef actual M1 Max model inference and CPU executed. Unsupported-CUDA → CPU component path exercised. NVIDIA hardware/full Linux companion-loader path not measured |
| Tokenizer/blocks/pooling/engine/calibrated output parity | Separate Rust byte-BPE/scalar-F64 references; native recurrence checks | fef fixed 138/314/4096-token Metal+CPU comparisons passed;4097 rejected. Same-agent authorship/shared generic infrastructure and untested modes remain limits; tolerances unchanged |
| Original validation deadline throughout native discovery | `rust_worker::execute` passes absolute deadline; `supplemental_discovery_stops_slow_rust_probe_at_original_deadline` | Already on the feature branch and exercised by source validation; no new prerequisite |
| Active useful production-path model decisions | `production_evaluation::{execute_contract_checks,functional_snapshot,evaluate,expected_matches}`, `vega_infer::{production_inference,collect_readiness_shard,verify_scientific_contents}` | 59ea collected all24 records: gate/prompt positives accept, unmet obligations reject, uncertainty/manipulation abstain, contradictions reject, failed/stale proof blocks before inference. Calibration-positive abstains on both backends, missing its expected-accept label;22/24 match and strict aggregate fails. No relabelling or threshold change. Fixed cases are not independent accuracy labels |
| Calibration implementation and compatible pinned artifacts | `vega_calibration`, independent engine/calibration references, setup compatibility checks | 59ea numerical checks passed; valid model abstention preserves required non-readiness. New exact-feature-order/permutation regression authored. Same upstream base/adapter-off route, hashes and thresholds. Domain reliability remains unestablished; failed research is preserved, not current delivery qualification |
| Agent repair guidance and one PR (#1064 consumer boundary) | `ValidationGuidance::for_vega_outcome`, existing publisher and full `decapod validate` | Exact final source/Buildkite/native/numerical/functional aggregate must pass. Source or collection success alone cannot publish. No feature PR yet; close only #1445 if fully completed |

## Evidence custody

The last complete run before this acceptance revision is `fef84896`:
[Buildkite 2301](https://buildkite.com/decapodlabs/decapod/builds/2301), all 39
source steps,1463 library tests/3ignored and53 worker tests passed. Three fixed numerical fixtures passed
on both actual Metal and CPU. Full report
`vega-1191-validation-20261020T042518Z`, SHA256
`06be1331c630bce469e9250e111baf4f2bc8e6902716924d183037e56e51d2af`,
failed the old research admission gate. Those failures and older runs remain intact.

The following revised run on `59eaf087` passed [Buildkite2311](https://buildkite.com/decapodlabs/decapod/builds/2311),57native tests and all three numerical fixtures,
but failed the strict expected-outcome comparison for the valid abstaining
calibration-positive control. Full report081406Z SHA256
`d7fc06ddf9187b1e1ed6536bf4572ea38d6f5446846aadab9c6d16cce7305c76`.
The model did approve two substantive positive controls on both backends and all
non-ready controls remained non-ready. No feature PR or full passing claim follows
from this partial result. Labels, raw outcomes and the failed report are retained.

The current collector never repeats exposed v2 as an untouched holdout. It
preserves all old inputs/results and retains independent numerical/native checks.
Fixed functional cases must pass through the same production projection, real
model, response checks and final predicate under the explicit revised contract.
These outcomes cannot establish a passing statistical study or calibrated domain
accuracy. New measurements belong to their exact new SHA. Issue updates record
only the completed Vega portions of broader issues, never their unrelated closure.

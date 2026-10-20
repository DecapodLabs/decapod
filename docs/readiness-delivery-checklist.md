# Readiness delivery after #1460

This checklist maps the requested remaining umbrella work to shipped components,
regression coverage and evidence. An implementation or authored test is not an
executed result. Final validation references and publication links belong in the
PR and corresponding issue updates; no row below claims full umbrella completion.

| Issue | Required delivery | Existing code and regression anchor | Evidence/disposition |
| --- | --- | --- | --- |
| #1397 | Immutable event/base/head before skips; detached private checkout; strict version-only projection exception; historical generator identity; actual CLI replay | `core/validate.rs`, `core/release_metadata.rs`, `core/release_projection_drift.rs`, `core/entrypoint_integrity/history.rs`, `assets/tools/ci_event_context.rs`; `ci_checkout_policy`, `ci_event_context_policy`, `substrate_integrity`, material-spec and history unit tests | Current focused work. Actual CLI replay exposed Dockerfile refresh whitespace disagreement. Focused results and full candidate validation are reported separately in the trajectory and PR; authored tests alone establish neither. |
| #1398 / #1070 | Complete applicable workflow/matrix/shard/platform execution, real step/artifact evidence and immutable source/base/policy/toolchain/binary/executor binding | `core/readiness_profile.rs`, `assets/tools/source_ci_profile.rs`, `source_ci_custody.rs`, `source_ci_toolchain.rs`, `integration_custody.rs`; existing source/custody adversarial tests | Shipped foundations retained. Full applicable graph and actual hosted/platform evidence must be reconciled and executed before closure. |
| #779 (required portion) | Hosted distinct repository/execution roots with authenticated preparation/execution evidence | Existing preparation.v2/execution.v3 protocol and accepted PR #1411 foundations | Real compiler fixtures are not hosted distinct-R/H evidence; the latter remains required. |
| #1024 | Existing intent/lifecycle discovery, structured arguments, safe idempotent init/migration/re-init, preservation and actionable first-agent repair | Existing planner, `core/scaffold.rs`, lifecycle and init/config regression suites | Audit shipped behavior against each acceptance item; Dockerfile header preservation contributes only its relevant idempotence portion. |
| #1399 | Real isolated release-plz transformation; actual cargo-dist packaging of all four configured archives; bind derived source/artifacts | `assets/tools/derived_release_proof.rs`, `derived_release_aggregate.rs`, `release_artifacts.rs`, release fixtures and workflows | Source fixtures/declarations are insufficient. Actual nonpublishing transformation and all required platform archives remain execution obligations. |
| #1069 / #1052 | Binary readiness, bounded execution/cancellation, exact final material/base rechecks, nonempty plan, strict version-only reuse and terminal current-head checks | Existing readiness planner/runtime/evidence verifier and publication predicate | Reconcile entire scopes and run affected regressions plus full validation; focused #1397 delivery alone does not close these issues. |
| #1064 / #1084 (required portions) | Consume publication lifecycle and obligation ledger where needed | Existing publication/custody verifier, obligation ledger and consumer projections | Reuse shipped mechanisms; unrelated publication modes and broader ledger scope are not claimed solved. |
| Dependabot #4 | Remove vulnerable Thrift from the native worker without suppressing the advisory | `assets/tools/vega-native/Cargo.toml` and `Cargo.lock`: pinned Parquet 59.3.1 removes Thrift; existing corpus integrity/reader and native regression checks | GHSA-2f9f-gq7v-9h6m affects Thrift below 0.23.0. Parquet 57/58 still require 0.17; the selected 59 release removes it. Execute required native checks before claiming completion; the alert closes after accepted dependency metadata is rescanned. |

Completed prerequisites #1396, #1448/#1450, #1452/#1453, #1455 and #1457/#1458
remain accepted. #1460 is merged and #1445 is closed. The active Vega integration,
Metal/CPU behavior, original numerical tolerances, evidence binding, safe-abstention
qualification and unestablished domain accuracy remain preserved.

The controller was installed through the official supported cargo-binstall route
and reports 0.120.0 via `decapod system version`. Every new commit has one material
path plus the kernel-required governance checkpoint, following the maintainer's
delegated resolution of the literal one-path/kernel-checkpoint conflict. Companion
source, tests, documentation and generated projections receive separate commits;
the completed candidate, not each intermediate assembly commit, needs full proof.

# Readiness delivery after #1460

This checklist maps the requested remaining umbrella work to shipped components,
regression coverage and evidence. An implementation or authored test is not an
executed result. Final validation references and publication links belong in the
PR and corresponding issue updates; no row below claims full umbrella completion.

| Issue | Required delivery | Existing code and regression anchor | Evidence/disposition |
| --- | --- | --- | --- |
| #1397 | Immutable event/base/head before skips; detached private checkout; strict version-only projection exception; historical generator identity; actual CLI replay | `core/validate.rs`, `core/release_metadata.rs`, `core/release_projection_drift.rs`, `core/entrypoint_integrity/history.rs`, `assets/tools/ci_event_context.rs`; `ci_checkout_policy`, `ci_event_context_policy`, `substrate_integrity`, material-spec and history unit tests | Completed in [PR #1462](https://github.com/DecapodLabs/decapod/pull/1462), head `55e27243`, merged as `8396a6e4` and officially released in v0.120.1. Full Decapod validation passed; Buildkite 2363–2367 and PR-event 2368 passed. Original failures remain recorded. |
| #1398 / #1070 | Complete applicable workflow/matrix/shard/platform execution, real step/artifact evidence and immutable source/base/policy/toolchain/binary/executor binding | `core/readiness_profile.rs`, `assets/tools/source_ci_profile.rs`, `source_ci_custody.rs`, `source_ci_toolchain.rs`, `integration_custody.rs`; existing source/custody adversarial tests | Shipped foundations retained. Full applicable graph and actual hosted/platform evidence must be reconciled and executed before closure. |
| #779 (required portion) | Hosted distinct repository/execution roots with authenticated preparation/execution evidence | Existing preparation.v2/execution.v3 protocol and accepted PR #1411 foundations | Real compiler fixtures are not hosted distinct-R/H evidence; the latter remains required. |
| #1024 | Existing intent/lifecycle discovery, structured arguments, safe idempotent init/migration/re-init, preservation and actionable first-agent repair | Existing planner, `core/scaffold.rs`, lifecycle and init/config regression suites | Audit shipped behavior against each acceptance item; Dockerfile header preservation contributes only its relevant idempotence portion. |
| #1399 | Real isolated release-plz transformation; actual cargo-dist packaging of all four configured archives; bind derived source/artifacts | `assets/tools/derived_release_proof.rs`, `derived_release_aggregate.rs`, `release_artifacts.rs`, release fixtures and workflows | Source fixtures/declarations are insufficient. Actual nonpublishing transformation and all required platform archives remain execution obligations. |
| #1069 / #1052 | Binary readiness, bounded execution/cancellation, exact final material/base rechecks, nonempty plan, strict version-only reuse and terminal current-head checks | Existing readiness planner/runtime/evidence verifier and publication predicate | Reconcile entire scopes and run affected regressions plus full validation; focused #1397 delivery alone does not close these issues. |
| #1064 / #1084 (required portions) | Consume publication lifecycle and obligation ledger where needed | Existing publication/custody verifier, obligation ledger and consumer projections | Reuse shipped mechanisms; unrelated publication modes and broader ledger scope are not claimed solved. |
| Dependabot #4 | Remove vulnerable Thrift from the native worker without suppressing the advisory | `assets/tools/vega-native/Cargo.toml` and `Cargo.lock`: pinned Parquet 59.3.1 removes Thrift; existing corpus integrity/reader and native regression checks | Completed by PR #1462. GitHub reports alert 4 fixed at 2026-10-21T03:25:47Z. Actual native qualification and reader compatibility passed; all nine native Git/model identities and numerical tolerances were preserved. |


Completed prerequisites #1396, #1448/#1450, #1452/#1453, #1455 and #1457/#1458
remain accepted. #1460 is merged and #1445 is closed. The active Vega integration,
Metal/CPU behavior, original numerical tolerances, evidence binding, safe-abstention
qualification and unestablished domain accuracy remain preserved.

The active controller was installed from the published official v0.121.0 release
through the supported cargo-binstall route and verified to report 0.121.0 via
`decapod system version`. Executable location and machine observations remain in
private evidence outside Git. Every new commit has one material
path plus the kernel-required governance checkpoint, following the maintainer's
delegated resolution of the literal one-path/kernel-checkpoint conflict. Companion
source, tests, documentation and generated projections receive separate commits;
the completed candidate, not each intermediate assembly commit, needs full proof.

Merged [PR #1464](https://github.com/DecapodLabs/decapod/pull/1464) extends the existing native runner with finite reviewed
command sequences, shared scratch, complete step observations and original
deadline/cancellation propagation. Native invocation identity binds the declared
limit rather than remaining wall-clock time, and Cargo's candidate binary name
is distinguished from controller execution. Focused regressions cover these
boundaries; they do not establish missing platform or release execution.
The v0.120.1 path-dependency lock repair follows Cargo's normal offline resolver;
compatible references also normalize, so it is not version-only evidence reuse.
Actual four-archive derived-release execution and production lock-refresh parity
remain #1399 obligations. #1464 passed full exact-candidate validation at
`c77df1f0`; Buildkite 2400–2404 passed, ordinary PR CI finished with 167 passes
and 17 skips, and the maintainer merged it as `b6c74f32`. Official v0.121.0
includes it. Its 60 native tests and all eight actual scientific runs passed
under the existing functional safe-abstention qualification; original mismatches
and unestablished accuracy remain visible.

Current release preparation shares exact native lock projection between the
isolated driver and production maintenance binary, pins production setup to the
reviewed toolchain, and separates release-PR preparation from original accepted
publication source. Focused Rust regressions cover exact/stale/tampered lock
deltas, unsafe links, atomic failure preservation and cargo-dist mutation. The
repository selects the existing strict source step/artifact collector. These
changes require their own terminal exact-candidate validation and actual derived
execution evidence; no remaining umbrella is closed by authored code or fixtures.

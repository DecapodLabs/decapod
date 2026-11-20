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

The controller was installed through the official supported cargo-binstall route
and reports 0.120.0 via `decapod system version`. Every new commit has one material
path plus the kernel-required governance checkpoint, following the maintainer's
delegated resolution of the literal one-path/kernel-checkpoint conflict. Companion
source, tests, documentation and generated projections receive separate commits;
the completed candidate, not each intermediate assembly commit, needs full proof.

The governed follow-up extends the existing native runner with finite reviewed
command sequences, shared scratch, complete step observations and original
deadline/cancellation propagation. Native invocation identity binds the declared
limit rather than remaining wall-clock time, and Cargo's candidate binary name
is distinguished from controller execution. Focused regressions cover these
boundaries; they do not establish missing platform or release execution.
The v0.120.1 path-dependency lock repair follows Cargo's normal offline resolver;
compatible references also normalize, so it is not version-only evidence reuse.
Actual four-archive derived-release execution and production lock-refresh parity
remain #1399 obligations. Final exact-candidate evidence belongs in the follow-up
PR and issue updates; no remaining umbrella is closed by these component checks.


## Derived preparation research reconciled on 2026-11-11

PR #1613 preserves the original development head
`f475d0788e48185022b51c6f0db7fd2fd090430b` and reconciles its residual work with
accepted `master` `53a55742dfbd100d2f0fd94a91d9b5bdd6c484ee` (v0.137.2).
The preceding controller-version and native-worker statements describe their
historical delivery context, not the current release configuration.

The current kernel already verifies the exact root Cargo/changelog release
delta and rejects root `Cargo.lock` mutation during cargo-dist packaging. Vega's
separate workspace was retired by #1589/#1593. Its maintenance binary, worker
lock projection, stale dependency pins and policy transition are not restored.
The current root lockfile, accepted workflow, collector configuration and
release ownership remain authoritative.

`tests/fixtures/release_pinned_preparation.yml` retains the proposed isolated
release-plz preparation and pinned setup actions as an **inactive review
fixture**. Its source clone prepares the next release PR while publication
continues to refer to the original accepted checkout. The obsolete Vega lock
commands are removed. The release-owner boundary, release job admission and
all downstream artifact jobs/matrices are byte-identical to the accepted
workflow. The regression also requires the current controller to reject this
unaccepted fixture's workflow bytes, even if a candidate refreshes its declared
inventory digest. This file is neither an activated workflow nor independent
controller acceptance; unknown execution contracts remain blocked.

### Preserved historical execution evidence

The original record at
[`f475d078`](https://github.com/DecapodLabs/decapod/blob/f475d0788e48185022b51c6f0db7fd2fd090430b/docs/readiness-delivery-checklist.md)
describes a real opt-in Rust diagnostic against committed source `16b8a32e`:
pinned release-plz 0.3.171 computed 0.121.0 to 0.121.1, ran API compatibility
checks successfully, changed the expected manifest/changelog/root-lock and
then-existing worker-lock paths, and checked idempotent worker projection in
72.3 seconds. No package, tag, release, registry upload or deployment followed.
Those observations apply to that historical source; the diagnostic was not
rerun by this reconciliation. The earlier official v0.121.0 controller's
`CONTROLLER_CONTRACT_UNSUPPORTED` result is retained, not converted into a pass.

### Remaining proof

Issue #1399 still requires actual nonpublishing release preparation on the
final candidate, cargo-dist production of all four configured platform
archives, and the complete bound execution/collection graph. Unit fixtures,
workflow equivalence, a green source test subset or the historical preparation
observation cannot satisfy these obligations. Any future workflow activation
requires separately reviewed controller support and exact-candidate validation.

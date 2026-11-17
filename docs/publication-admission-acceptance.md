# Evidence-bound PR admission (#1747, #1748)

The acceptance target is a failed agent action, governed rejection, source
repair, fresh required execution and a PR admitted only for the proven artifact.
A prior success is historical evidence after source, policy, required context or
accepted target changes. Admission does not authorize a later merge.

## Source and issue review

The audit starts at accepted `99ec8c2f677a8f21ac2ce6d511d19dcf917c4886` and
reviews the complete relevant path from CLI/RPC through current-workbranch
validation, native and provider evidence, canonical completion, publication and
test selection. Open and closed issue searches covered evidence, admission,
staleness and publication, followed by the relevant issue bodies. This is a
source/contract audit, not a claim that unrelated repository subsystems or all
historical acceptance cases have been independently proven.

| Surface | Existing owner and reviewed behavior | Result |
| --- | --- | --- |
| Public CLI/RPC and final publisher | `lib.rs`, `validation_scope.rs`, `workspace.rs` | Partial and CI-local diagnostics cannot establish full readiness. Publication previously discarded the completed proof, retaining only an optional cleanup locator. |
| Accepted requirements and complete plan | `repository_profile.rs`, `readiness_profile.rs`, `validation_runtime.rs`, `init_reconciliation/` | Preserve accepted commands/source coverage, compile a complete plan, freeze source/policy/target and reconstruct it after execution. |
| Actual source and execution inputs | `native_proof.rs`, `readiness_evidence/{mod,material}.rs`, `validation_runtime/repository_host.rs` | Bind exact bytes, modes, source history, candidate, accepted base, execution/tool identities and required observations. Hashes establish binding, not independent producer authentication. |
| Remote execution | `validation_runtime/snapshot/`, `snapshot_provider/public_buildkite/ci_attested.rs` | Provider-owned execution and supported controller contracts remain required; a local pass cannot stand in for hosted source proof. |
| Durable completion and custody | `validation_history.rs`, `workunit.rs`, `completion_evidence.rs`, `todo.rs`, `trajectory/`, `validation_rules.rs` | Canonical observed evidence, task revision/lease, workunit contract and current applicability are separate from declarations. Historical completion cannot authorize another publication. |
| Forge transition | `workspace_publication.rs` | Exact destination, PR identity and complete diff were already read back. The latest observed base was not compared to the base that passed proof; local inputs were not remeasured through the forge transition. #1747 closes that continuity gap. |
| Permanent coverage | `tests/unit/core/workspace_publication_tests.rs`, `tests/consumer_readiness.rs`, `tests/support/fresh_docs_readiness.rs`, `tests/source_ci_profile_policy.rs` | Deterministic integration, genuine CLI local-target checks and a separate live-forge acceptance have distinct assurance scopes. A regression requires publication test changes to retain the existing always-selected full library recipe. |

Related open tracking issues #1052, #1069, #1070, #1726, #1730 and #1731 retain
their broader obligations. Closed #1658 protects accepted policy from candidate
repinning; open #1659 and #1660 contain merged override/workunit repairs plus
remaining acceptance work. Closed #1325, #1429 and #1397 concern historical
receipt currency, task revisions and immutable CI context. Closed #1692 is an
escalation convergence harness, not proof of this publication lifecycle.
The existing `continuous-governance-implementation-map.md` describes historical
versions: canonical local Git CLI readiness is now implemented, while forge
publication from that local-only authority remains explicitly unsupported.

## Enforcement and deterministic regression gate

The publisher now carries `PublicationReadiness`, wrapping the existing live
completion and task binding. It checks exact repository/branch/head/base,
governance and measured current inputs before creation and at final readback.
It neither reloads success from a file nor reruns a narrower substitute suite.

If a PR exists when admission fails, `PR_CREATED_NOT_ADMITTED` reports its URL.
The publisher verifies the matching same-repository PR is a draft or closed,
demoting it to draft when needed. Failed or ambiguous quarantine remains an
explicit rejection; an already-merged PR cannot be reported as quarantined.
It is never a successful publication. An external
`publication-rejection-*.json` preserves the reason and original proof subject
without changing successful historical evidence. An unrestricted user can
still act outside the cooperative gate; independent forge authority is separate.

Run the repository's required `decapod validate` for the checkpointed exact
candidate. Its `supabase-consumer` recipe executes `cargo test --lib` and therefore
the core library's publication regressions, including:

- `failing_agent_repairs_source_and_only_fresh_exact_proof_admits_pr`
- `accepted_base_movement_after_success_cannot_admit_a_pr`
- `local_changes_during_lookup_or_final_readback_never_report_admission`
- `successful_report_or_receipt_without_live_completion_cannot_admit_pr`
- `omitted_skipped_or_failed_required_observation_cannot_authorize_creation`
- `current_source_governance_policy_and_measured_tools_must_still_match`
- `rejection_is_appended_without_rewriting_successful_historical_proof`
- quarantine confirmation, lost-acknowledgement, denial and unrelated-PR controls

Actual command execution and Git objects are real in these tests; forge I/O and
the fixture's project-prerequisite/accepted-target host are controlled seams.
Passing them alone does not prove the complete real-forge workflow.

The live `master` ruleset is
[12701816](https://github.com/DecapodLabs/decapod/rules/12701816). It was inspected
and backed up before adding the observed `CI / test (target="core") (push)`
check and `CI / supabase-consumer (push)` (the actual library-unit-test owner)
from Buildkite App 805657 with strict target freshness. Existing rules and
the OrganizationAdmin bypass were preserved. A bypass actor retains explicit
authority; do not describe that as unconditionally enforced against admins.

## Real-forge acceptance

The strict real-forge test passed on macOS after the timestamp dependency repair: **1 passed, 0 failed**, 305.54 seconds.
It used real CLI calls, required native commands, Git commits, remote refs and
GitHub publication/readback. No forge response or proof runner was replaced.
The invalid-publication assertion requires the source-readiness rejection itself;
a timeout, lock or transport failure cannot satisfy that adversarial control.

- Live PR: [acceptance #1](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/pull/1).
- Immutable [nine-file evidence archive](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/tree/ae15eeffd6812002fbda2a2adaca23b006de0ba6).
- Machine-readable [acceptance report](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/acceptance.json).

Every archived file was independently checked against its Git blob identity and
is byte-identical to the retained native evidence. Raw transcripts, credentials
and machine state remain outside Git.

| Observed subject | Exact identity |
| --- | --- |
| Invalid source commit | `9c6f2066b33952cdeed941184f3c9723c6192a63` |
| First repaired/admitted head | `b386cdce5485a75e7760d4490c1f7e21db1816ca` |
| Final revalidated/admitted head | `ccbc08207c8c592824c4b3fc75a8541da22bff8e` |
| Validated `main` base | `3a18ec7e802ddba8c9b36d6e6eb4adaf8aa607b1` |
| Independently advanced `main` target | `e36fd2979d57ea1892d70a532d568b087e294fac` |
| Controller source | `fc52356fbce48f504026100c70980353548d0bcd` |
| Controller SHA-256 | `29edb514f927ecd43461847412ad476ec673b8e2cbaf8059b712d130348d6435` |

| Adversarial control | Observed enforcement |
| --- | --- |
| Actual invalid agent source | `payload-contract` executed and failed with exit 1; independent `guard-contract` executed and passed. Validation was blocked; publication returned `PUBLICATION_NOT_READY` and no PR existed. |
| Agent repairs the source | Both required commands executed successfully; fresh receipts identify the repaired head and exact accepted base. |
| Source mutation after real GitHub creation | `PR_CREATED_NOT_ADMITTED`, `WORKUNIT_PROOF_STALE`, `quarantine=draft_confirmed`; no successful publication was returned. A separate native rejection preserves the original proof subject. |
| Fresh proof after quarantine | New execution evidence admitted the matching exact-head PR. |
| Source mutation after admission | `GOVERNANCE_CHECKPOINT_MISSING`; no commit was created and prior success could not authorize publication. Governed projection repair created a new checkpoint/commit, requiring fresh execution and admission for the new head. |
| Independently advance the target | Publication rejected with `STALE_ACCEPTED_BASE`; repeated full validation rejected the same changed target. |
| Final readback | PR head remains the exact validated head. The actual `main` ref advanced; the PR response retained its historical base. Current applicability is false and merge authorization is false. |

The final PR is deliberately left open as a draft after the target-advancement
control. Its earlier admission is historical. PR creation, observed admission
and permission to merge remain separate outcomes. The real target-ref check is
independent of potentially historical PR base metadata.

The unchanged native receipts, their observations and the full post-creation
rejection are in the archive. Each successful receipt records
`scope=repository_local_readiness` and both actual required assertions. This
fixture establishes real GitHub publication with its accepted native source
contract; it does not attest a hosted CI matrix, release packaging or deployment.
The Decapod source candidate separately requires full Buildkite validation.

| Receipt | Proven head | File SHA-256 |
| --- | --- | --- |
| [Repaired source](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/repaired-receipt.json) | `b386cdce5485a75e7760d4490c1f7e21db1816ca` | `4341add017bc6391511ee2f2a5ab82d791e5344d789ea4f8a708a1130cc2ee8f` |
| [Fresh after quarantine](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/fresh-after-quarantine-receipt.json) | `b386cdce5485a75e7760d4490c1f7e21db1816ca` | `d4c5eec4936fe38a0d6ec52392c8d23e027940de6de902e61a854082082a361d` |
| [Fresh after stale-source repair](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/fresh-after-stale-source-receipt.json) | `ccbc08207c8c592824c4b3fc75a8541da22bff8e` | `b1071c5ce84511bb7fdba5e6fc0c254b513e97ea9b5ab91f709559c1309e0529` |

[Failed native observations](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/invalid-source-observations.json)
and the [native post-creation rejection](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-06/blob/ae15eeffd6812002fbda2a2adaca23b006de0ba6/created-not-admitted-rejection.json)
retain the negative evidence separately from successful historical receipts.

To reproduce the entire sequence, use an authenticated GitHub CLI, working Git
SSH access, Rust 1.96.1 and macOS (the platform used for this run). Supply a new
empty disposable repository and a new private output directory each time:

```sh
gh repo create OWNER/decapod-admission-acceptance-UNIQUE --public
git clone https://github.com/DecapodLabs/decapod.git decapod-admission-repro
cd decapod-admission-repro
git checkout --detach fc52356fbce48f504026100c70980353548d0bcd
DECAPOD_ACCEPTANCE_REPOSITORY=OWNER/decapod-admission-acceptance-UNIQUE \
DECAPOD_ACCEPTANCE_OUTPUT=/absolute/new/private/evidence-directory \
cargo test-main --locked --test entrypoint_correctness \
  publication_admission::real_forge_reject_repair_admit_and_invalidate \
  -- --ignored --exact --nocapture --test-threads=1
```

The harness bootstraps its accepted contract before agent work, then performs
all task/worktree/plan/checkpoint, failure, repair, publication, readback and
target-advancement operations. It emits `acceptance.json` only after all
assertions complete and archives reviewed evidence on an independent branch.
It neither merges nor deletes repositories. Earlier unsuccessful attempts
remain preserved; their partial results were not reported as a passing run.
The required deterministic counterparts remain ordinary full-library tests;
the opt-in live harness is also compiled by the existing CLI test target.


## Current dependency policy and acceptance integrity (#1755)

The delivery PR is [#1754](https://github.com/DecapodLabs/decapod/pull/1754).
Historical push validation of `b79a28fdbd45a9f608c8f063eca04368937a6fe5`
passed in Buildkite 11552. Its later [PR dependency check](https://buildkite.com/decapodlabs/decapod/builds/11557#01a1e907-4684-47d4-84a1-7f33b7381377)
rejected `chrono` under [RUSTSEC-2026-0543](https://github.com/RustSec/advisory-db/blob/main/crates/chrono/RUSTSEC-2026-0543.md).
That earlier success did not authorize delivery of the failing present artifact.

Decapod now uses [time 0.3.55](https://docs.rs/time/0.3.55/time/) and removes
chrono from both resolved Cargo and Bazel graphs. No advisory exception or
required-check reduction was introduced. Explicit UTC string serde preserves
persisted Task/KnowledgeEntry/Decision JSON, automatic 0/3/6/9 fractional digits,
observation +00:00 offsets and nine-digit Z Dactyl transaction markers. Dactyl
still reads legacy SQL UTC timestamps. Invalid expiry and out-of-range UTC
conversion return errors instead of admitting credentials or panicking.

The [full library lane on the repaired implementation](https://buildkite.com/decapodlabs/decapod/builds/11579#01a1e93a-2a53-4ef4-a660-8cdec1460936)
ran **1,797 passed, 0 failed, 8 ignored** on
`72e2c7e248d6805a14700f21a5189fb9f90c2d6c`, including timestamp wire, SQL,
expiry and extreme-offset controls and all publication regressions. This is
implementation execution evidence for that exact commit, not authorization for
later report or harness changes. Source delivery still requires full current
Buildkite proof and governed publication for the exact final source head/base;
its delivery receipt and provider links are attached to #1754 and the focused
issues. PR admission does not grant merge authorization.

The earlier [complete acceptance archive](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-04/tree/1c21226bc3c5c65afb504a37bbe00829e308c593)
remains historical for its recorded controller and fixture identities.
[Attempt 05](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-05/tree/68fddc9b49d3ad6446c8f0c841320e26ff45dca7)
is partial evidence: its permissive test process returned success, but the
invalid-publication invocation timed out in the machine-contract gate. That
result does not prove the intended publication rejection. The permanent harness
now requires PUBLICATION_NOT_READY with READINESS_BLOCKED, excludes
VALIDATE_TIMEOUT_OR_LOCK, and requires the independent guard's executed success.
The strict attempt 06 above passed those assertions with the repaired controller.
Earlier failed and superseded source/provider runs remain failures or historical
observations; no green result is transferred to another artifact.

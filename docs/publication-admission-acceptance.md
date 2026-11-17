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
explicit rejection; it is never a successful publication. An external
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

Disposable repository:
[alexhraber/decapod-admission-acceptance-20261116-1747](https://github.com/alexhraber/decapod-admission-acceptance-20261116-1747).
This first attempt retained a declaration error before source execution; its
negative result was not accepted as the required failing-agent demonstration.
The next fresh attempt uses
[alexhraber/decapod-admission-acceptance-20261117-1747-02](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-02).
That attempt reached actual failed-source rejection, repair, post-creation
quarantine and exact-head admission, then retained a harness cleanup failure.
A fresh complete attempt uses
[alexhraber/decapod-admission-acceptance-20261117-1747-03](https://github.com/alexhraber/decapod-admission-acceptance-20261117-1747-03).
The complete live run, exact binary/head/base identities, receipts, rejection
reasons and reproduction sequence are pending execution. No live-forge passing
claim is established by the authored harness or by repository creation.

The acceptance must observe invalid-work rejection, actual source repair, fresh
complete checks, exact-head PR admission, stale source rejection, independently
advanced target rejection and final GitHub head/base readback. It must also
preserve failed post-creation admission as a rejection, distinguish creation from
admission, and leave merge authority unclaimed. Retain raw machine transcripts
and credentials outside Git; publish only reviewed reproducible evidence.

The opt-in real-forge test is compiled by the existing entrypoint target and is
ignored in ordinary CI because it requires explicit authority over an empty
disposable GitHub repository. After full candidate validation, run it against
the actual candidate binary with:

```sh
DECAPOD_ACCEPTANCE_REPOSITORY=OWNER/decapod-admission-acceptance-UNIQUE \
DECAPOD_ACCEPTANCE_OUTPUT=/absolute/new/private/evidence-directory \
cargo test-main --test entrypoint_correctness \
  publication_admission::real_forge_reject_repair_admit_and_invalidate \
  -- --ignored --exact --nocapture --test-threads=1
```

`tests/acceptance/publication_admission.rs` preserves private transcripts and
emits `acceptance.json` only after every assertion completes. It bootstraps an
explicit accepted two-command source contract on the disposable target, drives
real CLI task/worktree/plan/checkpoint operations, tests source failure and
repair, observes real GitHub creation before deliberately changing local source,
requires created-but-not-admitted quarantine, then restores the source and earns
fresh admission on the same PR. Subsequent source and independently advanced
target controls must reject publication. The target is advanced only under this
explicit fixture authority. The test neither merges nor deletes repositories.
Its required deterministic counterparts remain ordinary mandatory library tests.

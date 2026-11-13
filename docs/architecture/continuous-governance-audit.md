# Continuous governance implementation audit

Audit base: `3b6b31f83edfb2b77f07aa4df3b8d958074134d9` (master).
Tracking: #1052; native integration rationale: discussion #1556.
This is an implementation map, not a completion receipt. Final candidate
validation and the complete executable demonstration remain separate gates.

## Authority and current coverage

| Classification | Guarantee and authority | Executable evidence and limits |
| --- | --- | --- |
| Already enforced | `readiness_evidence::evaluate` checks the independent expected plan, exact identities and required execution rows. Missing, skipped, contradictory or substituted evidence cannot supply success. | `src/decapod/core/readiness_evidence/tests.rs`: identity, policy, missing/duplicate rows, fake shortened plans and skipped wrappers. All 55 evidence-module tests passed on bootstrap commit `28016a2c`; provider fixtures test interpretation, not live provider authentication. |
| Already enforced | `readiness_evidence::material` hashes actual file bytes and modes; undeclared nonignored inputs and unsupported submodules fail closed. Symlink ancestors and escaping targets are rejected. | Material tests in the same executed suite use real temporary Git repositories. Ignored generated inputs still require an independently complete declared inventory; the kernel cannot infer arbitrary external dependencies. |
| Already enforced | `validation_runtime::discover_project` requires the accepted base to be an ancestor of HEAD; runtime repeatedly observes the remote target and rechecks material, plan and measured tools. | `tests/consumer_readiness.rs` contains real local execution and target/source/config mutation cases. An ancestor base means the candidate tree is the integration tree for that base; a moved target requires incorporation and new proof. Final-candidate execution of this suite is required. |
| Already enforced | A candidate cannot replace accepted `decapod.pr-readiness.v1` with `repo.pr-readiness.v1`. Repository-local execution has a separate declared assurance scope. | `accepted_strict_profile_cannot_be_downgraded_by_candidate_local_contract` and provider/scope cases in `consumer_readiness.rs`; these do not prove preservation of an already accepted local contract. |
| Already enforced by this repair | Accepted local commands, execution declarations and source coverage constrain candidate repinning through `repository_profile::preserve_accepted`. Additional required checks remain possible. | #1658 was reproduced through the real public runtime: invalid source plus a replacement print-success command and repinned digest returned `ready=true` before the fix. The same attempt now fails before executor selection. The 19-case consumer suite passed in 8.24s; the additional-check positive control passed separately in 4.35s. These fixtures replace project prerequisites and accepted-target transport, so they do not establish complete actual-CLI governance acceptance. |
| Already enforced | `validation_history` stores complete validator-produced observations in the existing verification stream. Opaque completion values prevent CLI evidence-path import from issuing success. Historical completion binds original task lineage and accepted integration. | `validation_history_archive_tests.rs`, `validation_history_integration_tests.rs`, historical task-journal tests; #1610 is closed. Historical success never authorizes another publication. Full regression execution remains required. |
| Partially enforced | Task transactions maintain revisions, leases, custody and proof; authoring admission and workspace operations enforce supported boundaries. | `todo::admit_authoring`, task-journal, custody, broker and workspace tests. Existing individual cases need the requested combined ten-workspace scenario and actual-binary recovery demonstration. |
| Already enforced | Startup cannot silently import legacy task authority over a journaled store. | Bootstrap repair preserves an inert zero-byte regular TODO file without SQLite opening, import or deletion. Nine migration tests passed; populated files, sidecars and symlinks remain guarded. General conflicting-store recovery is unresolved (#1657). |
| Advisory | Generated entrypoints, model summaries, tool text, trajectory claims and actor labels describe work but cannot independently authenticate it. | A runtime hook supplies automatic invocation only when the host uses it. The reference contract must invoke the existing kernel instead of interpreting policy itself (#880/#1277). |
| Missing independent boundary | Authenticated shared acceptance and unavoidable remote merge restrictions require authority outside an unrestricted local producer. | Propodus #124 owns the service boundary, with #71/#88 and Decapod #1038. Dactyl #85 transports typed context/results and preserves unknown outcomes. No local hash or caller-supplied identity proves this service exists. |

## Ready, complete and publish paths

- Plain `decapod validate` is the full readiness path. CI-local validation
  explicitly reports `publication_ready=false`; `govern project-check` is only
  prerequisites. Neither can be promoted to full readiness by a caller field.
- `todo done` can record a declaration with pending proof. Exclusive completion
  and `todo done --validated` use transactional proof gates. A declared task
  status is not verified completion; `qa verify` reconstructs supported evidence.
- Historical `qa verify commit` / `todo done --validated --commit` reuse a
  complete original observation only with original task and integration lineage.
  `qa verify recover` captures current selected material and runs real proof;
  it cannot recreate absent historical execution.
- `workspace publish` checks current readiness, committed governance/specs,
  exact destination readback and remote head/base races. A successful push is
  insufficient. `workspace_publication::verify_remote_unchanged` retains a
  changed-base blocker.
- Report projections and public pure judgment helpers consume already trusted
  observations; their serializable success fields do not constitute an evidence
  importer or a separate acceptance service.
- An unrestricted shell can edit files, actor environment, local stores and Git,
  or publish directly through forge credentials. Hooks and path admission do not
  prevent those actions. Independent protected publication/merge enforcement is
  required for an unavoidable remote gate. Local producer and verifier share an
  operating-system trust domain unless an actual separately controlled executor
  supplies authenticated evidence.
- Test-oriented `DECAPOD_VALIDATE_SKIP_*` variables exist in prerequisite and
  worktree gates. Their effect on full readiness must be audited through the
  supported CLI; a passed prerequisite run with skipped gates cannot establish
  complete intent/custody acceptance. They have not been used to obtain any
  readiness result in this work.

## Convergent implementation order

1. Repair the inert-file false blocker (#1657), preserving every existing task
   byte and keeping ambiguous migrations blocked.
2. Close the accepted local-policy boundary (#1658) before expanding integration.
   Preserve strict source proof and independently authorized policy evolution.
3. Compose existing context, authoring admission, observed execution,
   validation/evidence, publication and recovery operations into a small
   daemonless reference invocation contract. No adapter policy interpreter.
4. Exercise A–H through the actual binary and ten deterministic independent
   workers. Distinguish simulated workers from native LLM integrations.
5. Validate the exact final candidate against its current accepted target and
   publish only after existing gates pass; human merge authority remains intact.

The local proof demonstration requires an accepted reviewed local contract and a
second unchanged contract with unmet external assurance. A CI-only limitation
cannot silently replace a required local or external check. Decapod's own strict
contract must retain hosted/platform/release obligations actually required by its
accepted policy.

## Initial measurements and remaining proof

Bootstrap cold unit-test compilation took 3m30s. Nine migration tests took 1.53s;
55 readiness-evidence tests took 1.80s. These are focused development measurements,
not full validation latency or final-candidate proof. The inert-file repair adds
no schema, event or persistent record, and repeated startup tests compare the
complete task-journal verification projection before and after reconciliation.

Before task creation, the independent installed journal verifier reported the
same 86 tasks, 739 records and current digest before and after the patched
read-only task lookup. New governed task registration subsequently changes that
coordination state normally.

Full validation on governed commit `3245b1a6` was blocked before execution by
`PROOF_PROFILE_RECONCILIATION_REQUIRED`, with `publication_ready=false`.
Reconciliation requires inspection of the actual source inventory and accepted
executor, retaining every required check. No PR readiness is inferred from
focused tests.

Source review found no candidate changes in the discovered proof-source inventory.
Since its last configuration pin, accepted master changed only the Decapod
package version from 0.142.0 to 0.143.0 in Cargo.toml and Cargo.lock. After saving
the original configuration outside Git, its source-inventory digest was explicitly
reconciled to `sha256:adbba9921d8d0a9936d04ce00b7d0eb62efc878ded0102f8c2d3e2e603c3f46c`.
Required checks, profile and provider declarations were preserved.

Validation/reconstruction/invalidation latency, unchanged-input reuse, ten-worker
overhead, retained storage growth and independent A–H proof remain to be measured
on the completed implementation. General legacy-store recovery, external native
agent integrations and authenticated cloud acceptance remain explicit boundaries.

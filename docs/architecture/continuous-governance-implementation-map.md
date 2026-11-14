# Continuous governance implementation map

This audit is grounded in accepted master
`f779e39111ec8b3c8c41569f68a3a7604df167e7` (Decapod 0.144.4), the observed
workunit repair in PR #1681, and the separately identified delivery dependencies
below. A draft PR, a documented rule and an agent's assertion are not proof that
an acceptance condition passed. Candidate regression code exists; final-candidate
hosted execution and complete native publication evidence remain pending.

## Existing kernel and enforcement

| Classification | Mechanism and executable coverage | Assurance boundary |
| --- | --- | --- |
| Already enforced | `validation_runtime.rs`, `native_proof.rs` and `repository_profile.rs` bind candidate Git material, accepted policy and the selected required commands. `tests/consumer_readiness.rs`, included by the required CLI aggregate, executes genuine repository-local commands and negative policy/source cases. | A reviewed local contract may establish repository-local readiness. It does not discharge the stricter Decapod source-PR provider contract. |
| Already enforced | Accepted-policy command and reviewed-definition preservation from merged PR #1675; the real local consumer fixture detects candidate replacement of a check and repinning of its digest. | Historical accepted evidence for candidate `b586dac3ec7d7048f296c214818c5f735138c89a` is not evidence for the current draft. |
| Already enforced | `governance_document/checkpoint_history.rs::inspect_checkpoint_history` reconstructs immutable commit checkpoints; `require_proven` rejects findings and absent proof. Publication checks the committed bundle and current governance. | A present checkpoint cannot manufacture an older missing checkpoint. `publication_ready` is false for the standalone history report. No independently authenticated offline historical-exception issuer is implemented (#1569). |
| Already enforced | `validation_history.rs`, `plugins/verify.rs` and `todo.rs` consume exact canonical validation and independently verified integration for historical completion. Regression suites cover wrong task/candidate/integration and custody. | Closed issue #1610 is addressed by `todo done --validated --commit <candidate>`; it preserves chronology and does not assert current publication readiness. This path was actually used for the merged #1675 task. |
| Already enforced | Existing task journal, exclusive lease and workspace bindings, shared-resource reservations and group-broker request deduplication. `workunit_journal_tests`, `group_broker` and workspace/custody suites exercise real state transitions. | Disjoint custody is possible. This is not proof of the requested complete ten-worker A–H scenario. Uncertain broker outcomes require reconciliation, not blind replay. |
| Partially enforced | `workunit.rs` retained journal generations, execution custody and declared proof artifact hashes, but these did not independently establish that required validation ran. | PR #1681 makes canonical observed validation necessary for new `VERIFIED` results, replay, resume, completion evidence and the publication workunit gate. Final proof is pending. |
| Partially enforced | Diagnostic skip flags were separate from complete proof requirements but could reach success-producing consumers. | PR #1677 centralizes full-readiness override denial, including broker caller context. It is a dependency of complete convergence; this branch does not duplicate that interpreter. |
| Partially enforced | Full runtime freezes exact source and checks accepted remote target/integration through the supported provider. Current local workunit proof additionally remeasures canonical evidence inputs. | Unfetched remote movement cannot be learned from a local ref alone. Workunit verification is a prerequisite, not a replacement for full publication freshness. |
| Advisory | `AGENTS.md`, orientation, capsule retrieval and agent hooks discover or invoke existing CLI/RPC decisions. | An unrestricted shell can avoid the hook and directly mutate files or invoke Git/forge APIs. No OS-level mutation prevention is claimed. |
| Missing | A native runtime reference integration without model-specific governance instructions, the complete ten-worker convergence demonstration, and final A–H binary-level acceptance proof. | Existing interfaces and deterministic unit workers are groundwork, not those demonstrations. |
| Missing | Independently authenticated local exception authority and Decapod client wiring for repository read/write/owner repair (#1569, #1590). | Propodus server source has authority/delegation/CAS repair contracts; closed server issues do not prove deployed service authority or released Decapod client behavior. Local labels and config cannot substitute for a verified cloud principal. |

## Acceptance and publication routes

1. **Full validation:** CLI `validate`, RPC `validate.run`, the current-workbranch
   readiness controller and `validation_runtime::run` evaluate the accepted
   contract. Provider execution and its evidence must agree on exact source,
   target, requirements and artifacts. CI-local validation and
   `govern project-check` are limited prerequisites and cannot establish full
   publication readiness. PR #1677 closes diagnostic override admission.
2. **Workunit status:** `record_proof_result` records a declaration. In this
   repair it downgrades existing success and cannot create an observed result.
   `transition_status(Verified)` requires the canonical producer record and
   independently current witness. Public `validate_verified_manifest(root, …)`
   requires the exact durable journal projection. Legacy imported declarations
   remain readable and unproven.
3. **Task completion:** Current `--validated` completion executes the required
   proof; historical `--validated --commit` consumes applicable canonical proof
   and accepted integration. Plain lifecycle `done` does not create proof: an
   exclusive lease requires proof or yield, and unvalidated closure remains
   claimed pending proof. A successful task command is not merge readiness.
4. **Portable completion:** `completion_evidence::build_record`, `verify_record`
   and receiving-side import now consume current observed workunit proof.
   Exported bytes are custody, not an authorization token. A genuine same-repo
   import can be accepted only after receiver reconstruction; a receiver with
   missing binding or changed artifact rejects it.
5. **Governed publication:** `workspace::publish_workspace_with_readiness`
   checks worktree/custody, immutable checkpoints, current governance, merge
   conflicts and complete readiness before pushing the exact commit. It verifies
   the actual remote destination/PR and rechecks movement after forge calls.
   Publication does not grant human merge authority.
6. **Exceptions and bypasses:** Existing cooperative `validate --bypass-validation`
   records an explicitly limited exception with `proof_ready=false` and
   `publication_ready=false`. A spec decision reference is cooperative provenance,
   not a signed human permission to waive unrelated proof. Direct Git pushes or
   forge PR creation outside Decapod remain possible for an unrestricted actor;
   independent remote merge enforcement requires forge/service authority.

## Smallest current repair and adversarial controls

PR #1681 reuses the canonical verification journal and existing task/workunit
transactions. A bounded, deduplicated witness reconstructs applicability without
rerunning commands. The producer observes the workunit contract and generation
before execution and checks them again before archival. Consumer selection never
imports an agent-authored success field as canonical proof.

| Attack precondition and attempt | Required kernel result | Real regression boundary |
| --- | --- | --- |
| Claimed execution, fabricated passing file or legacy `VERIFIED` projection | Remain unproven; cannot mint required observed proof | `workunit_cli`, workunit journal and publication helper tests |
| Genuine native proof followed by source, untracked input, branch, target or policy movement | Preserve historical observation; refuse current applicability | `observed_success_cannot_authorize_changed_source_target_policy_or_candidate` |
| Workunit contract changes while genuine checks execute | Refuse successful canonical archival | `changed_workunit_during_actual_execution_cannot_publish_a_canonical_success` |
| Workunit declares less than the canonical task requires | Source proof may pass; task readiness fails and no accepted workunit success is appended | `weaker_workunit_cannot_discharge_additional_canonical_task_obligations` |
| New declaration after actual verified observation | Downgrade current status; retain historical record | `new_declaration_downgrades_actual_verified_proof_and_preserves_history` |
| Task lease/revision changes before append or identical-operation replay | Refuse on the same canonical SQLite transaction; append no workunit success | `verified_append_and_idempotent_replay_fence_changed_task_custody_in_transaction` |
| Explicit separate workspace DB tries to reuse parent verified authority | `WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`; preserve history | `separately_initialized_workspace_cannot_replay_main_store_verified_authority` |
| Portable envelope is copied after the receiver artifact changes | Receiver rejects; retained original custody does not overwrite or repair changed source evidence | `local_record_replays_and_detects_artifact_change` |

These are implemented real-runtime/journal regression cases, not final-candidate
passing claims. The native producer fixture executes actual source assertions
through the sealed adapter and a genuine bare-Git target transport. It is not a
full forge CLI demonstration or an independently authenticated remote verifier.

## Limits requiring subsequent delivery

Separately initialized workspace/task databases need a compatible supported
convergence migration before cross-store verified append can be enabled. Unknown
or additional full-task gates remain blocked rather than silently omitted. The
local agent and producer can share an unrestricted OS user: hashes prove binding,
not protection against that user's raw SQL/filesystem writes. Independent verifier
custody, cloud identity/revocation, remote merge enforcement and an actual native
agent integration remain distinct work.

Hosted source verification must report witness storage growth, execution time
and reconstruction cost before performance claims. No production storage increase
or five-minute budget is inferred from a source diff. Existing historical records
are retained; non-workunit records omit the new witness.

[Discussion #1556](https://github.com/DecapodLabs/decapod/discussions/1556)
requires runtime-native governance, not more prompt text.
[Issue #1610](https://github.com/DecapodLabs/decapod/issues/1610) explains the
post-merge chronology requirement. Neither is treated as executable evidence.

# Governed state-machine audit

Audit date: 2026-11-07. Base: `master` at `e5760258` (`chore: release v0.130.2`).
This worktree is `feat/governed-state-transitions`. The shared checkout on
`state-resolve-diff` was not modified.

Decapod is a governance kernel. It does not choose the next action. The audit
asked whether attempted transitions are admissible and whether resulting claims
are provable from canonical state.

## Canonical sources that already existed

| Concern | Canonical source | Classification |
| --- | --- | --- |
| Task coordination status | `tasks.status` plus revision CAS in `core/todo.rs` | ALREADY_CORRECT as coordination, not proof |
| Completion proof binding | `task_verification` artifacts bound to `task_id` and `task_revision` | ALREADY_CORRECT for revision binding |
| Exclusive lease completion | `exclusive_lease_proof_gate_on_conn` | ALREADY_CORRECT |
| Dependency proof | `fleet_coord::evaluate_dependency_readiness` requires a passing verification status | ALREADY_CORRECT |
| Unvalidated `todo done` | `mark_todo_claimed_pending_proof` writes `CLAIMED`, not `pass` | ALREADY_CORRECT |
| Caller isolation for broker mutations | `core/group_broker.rs` protocol version 3 | ALREADY_CORRECT |
| Direct SQLite rewrite | Documented as outside the process boundary | ALREADY_CORRECT as an explicit trust boundary |
| Laya worker `publication_ready: true` | Rejected by `laya_protocol::Response::validate_binding` | ALREADY_CORRECT |
| Flight recorder | Read-only renderer over the event stream | ALREADY_CORRECT as a view, not a second authority |
| State commit bytes | `core/state_commit.rs` deterministic CBOR scope record and Merkle root | ALREADY_CORRECT for encoding |

## Findings

### CRITICAL — a success flag could still become publication

`projection::project_validation_report` treated `publication_ready: true` as
eligibility whenever the status string was not stale. A report of
`{"passes": true, "publication_ready": true}` had no receipt and no head, but a
passing status plus the boolean was enough.

`state commit verify` with no expected root hashed whatever bytes it was given
and printed `VERIFIED`. Empty input was not fail-closed.

### HIGH — placeholder rule success and forbidden-path exemption

`validation_rules::is_applicable` returned false when a changed path matched
`forbidden_changed_paths`. The task rule `decapod.task.forbidden_paths` puts
plan forbidden paths in that field, so touching a forbidden path skipped the
rule instead of failing it.

`evaluate_workunit_rule` returned `Pass` when a manifest key existed. The
comment said proof gates were not checked.

`evaluate_rule` skipped evidence freshness. The loop body was empty.

`RuleRegistry::build_and_cache_rules` copied a project or task rule over the
builtin rule body. A task row with the same id could replace a kernel rule,
including a `NonOverridable` one.

### HIGH — pass status without substantive proof

`effective_verification_status` returned the stored status when `task_id` and
`task_revision` matched, even if the artifact was only `{"passes": true}` plus
notes. `from_todo` then mapped that to `WorkClaimStatus::Complete`.

### MEDIUM — state commit did not carry freshness

`verify` checked the scope-record hash. A hash that still matches an old head,
old rule fingerprint, or old verifier could be treated as current by a caller.
There was no freshness function next to the commit API.

### MEDIUM — no single judgment for long-running handoff

Intent, todos, verification rows, trajectories, and workunits are durable.
Nothing forced a new session to ignore prose notes and re-judge evidence
against the current governing identity. Recovery text in the state model says
notes are not authority, but that rule was not a function with adversarial tests.

### LOW — validation rules do not run outside a governed worktree

`validate_validation_rules` returns success without evaluation when the
checkout is not a governed agent worktree. That is intentional for the product
repository's own `decapod validate`. It is not a completion proof. Inside a
worktree the same rules are evaluated.

### ALREADY_CORRECT and intentionally retained

- Task `status=done` remains coordination state. Proof is a separate row.
- Comments and `last_verified_notes` are not read as proof.
- Exclusive versus shared claims, lease expiry, and dependency cycles stay in
  `fleet_coord` and `todo`. This change does not add a scheduler.
- `state.resolve` / `state.diff` exist only on another branch. They were not
  copied here.
- No workflow engine, daemon, or second task database was added.

## Baseline comparison

A clean detached worktree at exact base `e5760258` (`/Users/arx/src/decapod-worktrees/baseline-e5760258`) ran:

`cargo test --all-features --offline`

Lib result: **1554 passed, 42 failed, 8 ignored**. The 42 names, in order, are:

- `core::fs_permissions::tests::permissive_umask_flows_are_confined`
- `core::fs_permissions::tests::shared_store_requires_explicit_opt_in_and_never_accepts_world_write`
- `core::snapshot_provider::snapshot::host_adoption::tests::adopts_only_exact_published_workspace_head_without_mutation_or_fetch`
- `core::snapshot_provider::snapshot::host_adoption::tests::invalid_locator_dirty_source_and_cancellation_prevent_fetch`
- `core::snapshot_provider::snapshot::host_adoption::tests::wrong_parent_tree_source_base_uuid_ref_and_metadata_never_adopted`
- `core::snapshot_provider::snapshot::host_adoption::tests::wrong_remote_ref_base_tags_and_midflight_race_never_adopted`
- `core::snapshot_provider::snapshot::tests::actual_byte_check_defeats_a_trusted_index_stat_cache`
- `core::snapshot_provider::snapshot::tests::cleanup_retains_real_workspace_without_any_remote_operation`
- `core::snapshot_provider::snapshot::tests::existing_equal_or_ancestor_workspace_can_be_reused_or_fast_forwarded`
- `core::snapshot_provider::snapshot::tests::first_publication_is_only_exact_existing_local_branch_without_force`
- `core::snapshot_provider::snapshot::tests::full_tag_inventory_preserves_annotated_objects_and_rejects_local_change`
- `core::snapshot_provider::snapshot::tests::known_diverged_workspace_is_never_force_pushed`
- `core::snapshot_provider::snapshot::tests::missing_ancestor_object_or_changed_base_prevents_any_push`
- `core::snapshot_provider::snapshot::tests::prepare_preserves_branch_index_and_content`
- `core::snapshot_provider::snapshot::tests::resume_retains_exact_transport_without_recreating_commit_or_changing_refs`
- `core::snapshot_provider::snapshot::tests::uncertain_push_retains_exact_recovery_snapshot`
- `core::snapshot_provider::snapshot::tests::validation_create_fast_forward_and_exact_reuse_need_no_operator_receipt`
- `core::trajectory::execution::tests::execution_approval_expiry_and_record_changes_are_not_hash_only`
- `core::trajectory::execution::tests::execution_capture_requires_staging_new_material_and_binds_deletions`
- `core::trajectory::execution::tests::execution_checkpoint_cannot_drop_prior_approval_requirements`
- `core::trajectory::execution::tests::execution_checkpoint_commit_resume_is_deterministic_and_read_only`
- `core::trajectory::execution::tests::execution_checkpoints_do_not_persist_secret_source_or_paths`
- `core::trajectory::execution::tests::execution_resume_accepts_applicable_zone_approval_without_widening_action`
- `core::trajectory::execution::tests::execution_resume_binds_meaningful_trajectory_and_task_identity`
- `core::trajectory::execution::tests::execution_resume_rechecks_plan_acceptance_and_human_decisions`
- `core::trajectory::execution::tests::execution_resume_rechecks_task_revision_owner_generation_and_live_expiry`
- `core::trajectory::execution::tests::execution_resume_refuses_branch_substitution_and_rewritten_history`
- `core::trajectory::execution::tests::execution_resume_refuses_dirty_and_committed_source_changes`
- `core::trajectory::execution::tests::execution_resume_rejects_revoked_and_wrong_scope_approvals`
- `core::validation_epoch::input::tests::bounded_epoch_enforces_byte_and_depth_limits`
- `core::validation_epoch::input::tests::bounded_epoch_preserves_ordinary_epoch_digest`
- `core::validation_epoch::input::tests::bounded_epoch_refuses_symlink_directories_and_preserves_build_exclusions`
- `core::validation_runtime::tests::supplemental_worker_executes_locked_rust_and_retains_failure_without_source_writes`
- `core::vega_setup::tests::cancelled_copy_never_publishes_partial_artifact_and_cache_can_retry`
- `core::vega_setup::tests::component_reuse_has_one_download_and_separate_verified_single_link_copies`
- `core::workspace_lifecycle::tests::recovery_uses_creating_engine_despite_changed_default_and_never_falls_back`
- `core::workunit::journal_tests::checkpoint_and_resume_retries_preserve_unknown_valid_projection`
- `core::workunit::journal_tests::checkpoint_refuses_workunit_intent_unrelated_to_authoritative_task`
- `core::workunit::journal_tests::current_checkpoint_resume_rejects_source_drift_without_advancing_journal`
- `core::workunit::journal_tests::fresh_bound_proof_is_resumable_but_modified_artifacts_cannot_be_reanimated`
- `core::workunit::journal_tests::newly_failed_proof_downgrades_verified_state`
- `core::workunit::journal_tests::resume_fences_changed_authoritative_task_lease`

The branch suite on `feat/governed-state-transitions` after the finishing pass ran the same command. Lib result: **1578 passed, 42 failed, 8 ignored**. The 42 failing names are identical to the baseline list above, in the same order. Passed-count growth is new tests, not fewer failures. Both runs stopped at the lib suite, so integration tests did not start on either side. No failure was introduced by this branch.

## Minimal changes applied

- `core/governed_transition.rs` is the judgment kernel: admissibility,
  proof, freshness, publication, recovery, idempotent apply, and caller
  resolution.
- Verification status requires a substantive `proof_plan_results` digest.
- Validation projection eligibility comes from that judgment.
- Forbidden paths no longer exempt Error or Critical rules.
- Workunit and publication evaluators cannot pass on a sentinel.
- Project and task overrides cannot replace a non-overridable rule body.
- State-commit verify requires bytes and an expected root. `freshness` reports
  current, stale, or unknown for head, rules, and verifier.
- Adversarial tests cover the claims in the kernel and at the todo and
  projection seams.

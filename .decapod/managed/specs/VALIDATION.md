# Validation

## `#1672` Hop 1 shard unbundling, `#1400` batch custody download & next-hop workflow digest verification

Verified in `WT_B` (`agent/decapod-ci-seo/test_01m8fksbstm838m1-ci-sharding-latency` off `origin/master@78b9c0f18a`, `CARGO_BUILD_JOBS=2`):

- Unit suites: `snapshot_provider` (`219 passed, 0 failed`, incl. `official_cli_execute_batch_runs_concurrently_preserving_order_and_failures` and `download_artifacts_batch_verifies_ordered_outcomes_and_handles_missing`), `source_import` (`11 passed, 0 failed`), `readiness_profile::tests` (`35 passed, 0 failed`, incl. `reviewed_next_hop_release_workflow_digest_is_admitted_without_weakening_unknown_digests`), and `init_reconciliation::tests` (`22 passed, 0 failed`).
- Unbundled CLI & policy targets: `ci_checkout_policy` (`33/33`), `cli_contracts` (`26/26`, `31.06s`), `consumer_readiness` (`48/48`, `93.62s`, incl. all `6` newly annotated standalone tests), `effective_grant_cli_contracts` (`3/3`, `47.64s`), `entrypoint_correctness` (`28/28`, `40.57s`), `entrypoint_validation_contracts` (`7/7`, `308.16s`), `infer_precision` (`10/10`, `32.26s`), `init_ci_scaffolding` (`4/4`), `init_config_behavior` (`39/39`), `release_metadata_policy` (`37/37`), `release_workflow_policy` (`22/22`), `group_broker_strict` (`1/1`), and `source_ci_profile_policy` (`57/57` with `--skip actual_runner_ --skip provider_carrier_executes_immutable_r_and_wrapper_preserves_fixed_context_and_failure`, filtering only the `4` Linux-x86_64 / macOS `/var` canonical-path environment tests).

## Recovery workflow documentation & end-to-end verification (`#1956`, `#1957`)

- `tests/recovery_workflows_1956_1957.rs` (`issue_1956_terminal_reflection_uncommitted_recovery_preserves_budget_and_honors_authority`, `issue_1957_machine_only_completed_task_preserves_unverified_history_while_unblocking_code_candidate`) and `cli_contract_enforcement::test_agent_docs_ingest_works` verify both non-destructive recovery workflows end-to-end against `CARGO_BIN_EXE_decapod` (`2/2` and `1/1` passed).

## Section 9 CI/Publication latency & Cloud `ci_local` verification (`#1400`, `#1573`, `#1833`)

- `src/decapod/core/snapshot_provider/github_artifacts.rs` (`inventory_download_uses_discovered_metadata_without_redundant_reads_and_verifies_zip`) verifies `GithubWorkflowInventory::download_artifact` incurs 0 redundant pre/post API reads while validating exact byte size, SHA-256 digest, and failing closed on tampered ZIP bytes.
- `tests/unit/core/repository_profile_tests.rs` (`cloud_backend_evaluates_decision_id_in_both_local_and_ci_local_without_sqlite_fallback`) verifies Cloud-backend repositories evaluate `DECAPOD_REPO_TRANSITION_DECISION_ID` in both `local` and `ci_local` modes without consulting local SQLite, while failing closed when no decision ID is supplied.
- Local verification on `v0.168.0` (`cb54769bd0`): `cargo fmt --all -- --check`, `CLIPPY_CONF_DIR=.config cargo clippy --all-targets --all-features -- -D warnings`, `github_artifacts` (`10/10`), `source_import` (`11/11`), `repository_profile` (`40/40`), `recovery_workflows_1956_1957` (`2/2`), and `cli_contract_enforcement::test_agent_docs_ingest_works` (`1/1`) all passed.

## Section 12 Host Integration, N-API Native Binding, Harness Packaging, `Arm::Static`, and Empirical Study Verification (#875, #1277, #1095, #880, #1888, #1711, #873)

Verified inside isolated worktree `agent/agy-hosts/test_01m8dkhxjeg9vm6c-sec12` (rebased onto `origin/master` `714b90cbd`, `v0.168.0`, `CARGO_BUILD_JOBS=2`):

- `cargo fmt --all -- --check` and `cargo fmt --manifest-path bindings/typescript/native/Cargo.toml -- --check`: passed.
- `CLIPPY_CONF_DIR=.config cargo clippy --all-targets --all-features -- -D warnings` and `cargo clippy --manifest-path bindings/typescript/native/Cargo.toml --all-targets -- -D warnings`: passed with 0 warnings.
- `cargo test --test agent_interop_conformance`: 11 passed, 0 failed (`harness_native_cli_install_executes_mcp_and_hooks_and_preserves_user_config`, `library_rpc_seam_supports_concurrent_isolated_callers_and_cancellation`, `a2a_agent_card_and_handoff_validation`).
- `cargo test --test agent_rpc_suite`: 6 passed, 0 failed; `cargo test --test projection_consistency`: 7 passed, 0 failed; `cargo test --test houseboat_engine`: 14 passed, 0 failed (`houseboat_1711_darkmatter_readonly_snapshot_fixture_and_cli`); `cargo test --test activation_conformance`: 2 passed, 0 failed; `cargo test --lib flight_recorder::outcomes`: 9 passed, 0 failed.
- `bindings/typescript`: `npm run build:native` compiled `native/decapod.darwin-arm64.node` (`decapod-napi v0.168.0`, `dactyl-db 0.21.0`); `npm run typecheck` passed; `npm test` passed 8/8 (`native_napi`, `cli_process`, `AbortSignal`, Darkmatter boundary); `npm run bench` measured `evalPrompt` `0.004 ms` vs `5.24 ms` (`1310x`) and `rpc(schema.get)` `0.045 ms` vs `5.197 ms` (`115.5x`); `npm pack --dry-run` verified 5 packed files.
- `packages/pi-decapod`: `npm run typecheck` passed; `npm test` passed 5/5 (`registerPiDecapodHooks` backed by `decapod.darwin-arm64.node`); `npm pack --dry-run` verified 3 packed files.
- `docs/research/host-outcome-pilot/sec12-host-activation-2026-11-26.md`: `decapod trace flight-recorder compare --input docs/research/host-outcome-pilot/sec12-antigravity-input.json` validated the 6 real-host `antigravity 1.5.18` runs (`unverified_pair_identities: []`, `reviewed_active_human_minutes: null`) and preserved external credential/quota blockers for `claude 2.1.311` (`loggedIn: false`, `#1888`), `codex-cli 0.172.0` (`usage limit`), and `opencode v2.2.5`.

## Bootstrap `.sh` mode (`executor_contract`) & stale accepted-base `repo.pr-readiness.v1` reconciliation

- `src/decapod/core/validation_runtime/executor_contract.rs` unit tests verify `100755 blob ` admission strictly for `.sh` scripts in `profile::definition_files()`, strict `100644 blob ` enforcement on `.rs` definitions, protocol marker, and `verify_carrier_modes`, and fail-closed rejection of symlinks (`120000`), arbitrary modes, and unauthorized byte mutations.
- `tests/unit/core/repository_profile_tests.rs` verifies `qualifies_stale_accepted_base_inventory_reconciliation` admits stale accepted-base `reviewed_sources_digest` reconciliation when `base_tree == head_tree == candidate_sources_digest`, `changed_paths == []`, `base_oid != head_oid`, and commands are preserved verbatim, while failing closed (`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`) on any candidate modification to workflows, scripts, manifests, Git file modes, or proof commands; `src/decapod/core/validation_rules.rs` (`fresh_ci_checkout_requires_independent_approval_after_inventory_upgrade`) tests independent maintainer signature enforcement against a proof-relevant `scripts/check_source.py` change; `tests/validate_termination.rs` (`stale_accepted_base_reconciliation_cli_roundtrip`) exercises the end-to-end CLI roundtrip.
- `.decapod/config.toml` reconciles `proof.required_profile.reviewed_sources_digest` (`sha256:526afaf8226d412781485dd66d907884b3fd2d6cde7da05447e6ac6f8e1a620c`) against `origin/master` (`ea7bd4275`, `v0.166.1`).
- `src/decapod/core/validation_runtime/snapshot/source_policy.rs` (`selective_status_effective_context_requires_exact_seven_success_bindings`) verifies `selective-core-status` and `selective-cli-status` accept exact `CUSTODY_PREPARE_RESULT=success` and `CUSTODY_WORKER_RESULT=success` alongside the 5 `SOURCE_CI_*` bindings while rejecting failed/skipped/undeclared keys and preserving 5-key strictness on non-status checks.

## `#1949` follow-up: non-ecosystem manifest scoping, multi-hop helper closure, untruncated text diagnostics, and removal of manual human CLI prompt

Verified on `origin/master` (`1134da6b1`, `v0.166.0`) inside isolated worktree `agent/clack-ivy-da170052-4ca1-4f15-8a7b-7cf632d83d5d/code_01m8aejkc2mfqakr-repository-work`:

- `cargo fmt --all -- --check` and `CLIPPY_CONF_DIR=.config cargo clippy --tests -- -D warnings`: passed with 0 warnings.
- `cargo test --lib repository_profile`: 37 passed, 0 failed (including `unreferenced_non_ecosystem_manifests_qualify_without_manual_approval`, `ecosystem_coupled_manifest_changes_fail_closed_without_manual_cli_prompt`, and `multi_hop_and_same_dir_transitive_helper_references_fail_closed`).
- `cargo test --lib validation_rules`: 17 passed, 0 failed.
- `cargo test --lib untruncated_policy_transition_failures`: 1 passed, 0 failed (verifies `validate::untruncated_policy_transition_failures` preserves full >300-char transition diagnostics without clipping).
- `cargo test --test consumer_readiness`: 42 passed, 0 failed (including updated `source_inventory_change_succeeds_after_policy_control_approval_and_full_validation` asserting absence of `decapod govern policy approve` and new `unreferenced_non_ecosystem_manifest_qualifies_while_coupled_manifest_fails_closed`).

## `#1949` `repo.pr-readiness.v1` transition scoping, diagnostics, guidance & `dactyl-db 0.19.1` WAL checkpoint verification

Verified on `origin/master` (`84be80796`) inside isolated worktree `agent/clack-june-db301f84-74f7-46aa-8274-45c738d9489f/bugs_01m8abegxx1kjwxg-fix-1949`:

- `cargo fmt --all -- --check`: passed.
- `cargo test --lib repository_profile`: verifies `unreferenced_scripts_transition_qualifies_without_manual_approval` (`ci_local = false` and `true`) and `proof_referenced_script_and_transitively_referenced_helper_still_fail_closed_with_diagnostics`.
- `cargo test --lib validation_rules`: verifies `RuleEvaluationResult.message`, `remediation.observed_condition`, `remediation.required_steps` transition ID substitution, and `ValidationGuidance::for_human_authorization_required()`.
- `cargo test --test sqlite_maintenance governed_db_maintenance_wal_checkpoint`: verifies `decapod data db maintain wal-checkpoint` and RPC `data.database.wal_checkpoint` execute via `Connection::execute`, return `null` for unavailable frame counters, and preserve Dactyl's `sqlite3_stmt_readonly` read guard.
- `cargo test --test consumer_readiness unreferenced_packaging_script`: verifies end-to-end `repo.pr-readiness.v1` validation admits unreferenced `scripts/` packaging edits without manual approval.

## Dactyl `0.19.1` dependency, Bazel lockfile & security verification

- Verified `dactyl-db = "=0.19.1"` against `cargo update -p dactyl-db`, `/opt/homebrew/bin/bazelisk mod deps --lockfile_mode=update`, `/opt/homebrew/bin/bazelisk test //:cargo_inputs_policy`, `cargo fmt --all -- --check`, `cargo test --test release_dependency_policy`, `cargo test --test trust_egress_gate`, and `cargo audit -q` (`0` vulnerabilities).

## Kernel benchmarks (`#1938`), diagnostic telemetry (`#1095`), and `--device-id` hyphen parsing verification

- **Kernel workload & regression budget suite**: `tests/kernel_workload_1938.rs` (also wired into `tests/core/core.rs` and `.config/build/decapod.BUILD.bzl`) executes `small` (`12` tasks / `262` events), `medium` (`36` tasks / `986` events), and `large` (`48` tasks / `2,312` events) skewed datasets, asserts SHA-256 lossless event reconstruction across `compact_historical_events` + `gc_unreferenced_components` + `reclaim_pages_explicit`, verifies `task_journal_verify` and `db_maintenance` probes, and enforces reviewed `p99`, read, replay, compaction, maintenance, pre-build, bytes/event, and write-amplification budgets.
- **Telemetry redaction, fail-closed identifier gating, quantile, and bound tests**: `src/decapod/core/telemetry.rs` unit tests (`negative_redaction_strips_prompts_secrets_raw_evidence_and_customer_repo_identifiers`, `explicit_authorized_identifier_observation_uses_per_instance_ephemeral_salt`) and `src/decapod/core/observability_projections/telemetry.rs` + `tests/observability_projections.rs` verify `p50`/`p95`/`p99` quantiles, `queue_wait` vs `execution` vs `lock_wait` separation, `ProofFreshness`, `denied`/`unknown` preservation, ring-buffer/cardinality/retention eviction, `can_grant_authority = false`, default fail-closed `repo_digest == None` suppression, per-instance non-correlatable `anonymize_identifier_with_salt` digests under explicit opt-in, and negative redaction of prompts, secrets, raw evidence, paths, URLs, emails, and customer/repo identifiers.
- **CLI `--device-id` hyphen regression**: `tests/unit/cli_tests.rs::cloud_revoke_device_id_accepts_hyphen_prefixed_jwk_thumbprint` and `tests/cloud_supabase_boundary.rs` (`cloud_revoke_supports_dpop_session_logout_management_revoke_and_retry_on_failure`) verify `'-'`-prefixed RFC 7638 JWK thumbprints parse and route cleanly.

## Phase 2 durability, lease fencing, and active-renewal custody verification (#1892, #1935, #1934, #1763)

Rebased onto post-`#1939` `origin/master` (`0db8ba28e`) inside isolated worktree `agent/agy-1933-decapod-p2/bugs_01m85q9m9gwmde6r-durability-fencing`:

- Covers `core::validation_history::archive_tests` (`deterministic_active_lease_renewal_preserves_evidence_while_expired_and_reclaimed_fail`, `publication_readiness_survives_active_heartbeat_renewal_and_rejects_custody_rotation`), `tests/crash_durability_1934.rs` (6 scenarios), `tests/lease_fencing_consistency_1935.rs` (6 scenarios), and `tests/core/core.rs` (`core_tests` Bazel + Cargo registration), qualified by full GitHub Actions + Buildkite CI and local formatting/audit checks.

## Datastore and evidence encoding compatibility qualification (#1937)

- Rebased onto post-`#1944` `origin/master` (`b76f30d18`) and re-verified with `cargo audit` (`0` vulnerabilities, `0` warnings), `cargo fmt --all -- --check`, `clippy --all-targets --all-features -- -D warnings`, `cargo test --locked --test datastore_encoding_compat_1937`, `cargo test --locked --test trust_egress_gate`, and `cargo test --locked --lib core::migration`.

## DPoP E2E & `dactyl-db = "=0.19.0"` Cloud Admin verification (#1878)

Verified on `origin/master` (`863311461`, `v0.162.1`) inside isolated worktree `agent/swarm-w6-1878/feat_01m81s3sm3jet4vj-dpop-e2e`:

- **Dependency lockfile & Bazel policy**:
  - `Cargo.toml` / `Cargo.lock` bumped strictly `dactyl-db` `"=0.18.0"` -> `"=0.19.0"` (`url` preserved at `2.5.9` for `cargo deny`).
  - `/opt/homebrew/bin/bazelisk mod deps --lockfile_mode=update` and `/opt/homebrew/bin/bazelisk test //:cargo_inputs_policy`: `1/1 passed`; all local `bazel-*` symlinks removed before `specs.refresh`.
- **Formatting, lints, feature matrix & docs sync**:
  - `cargo fmt --all -- --check`: passed.
  - `cargo clippy --all-targets -- -D warnings`: passed (0 warnings).
  - `cargo check --no-default-features`: passed (0 warnings).
  - `cargo run --quiet --bin decapod -- docs build`: synchronized `docs/agent/command-contracts.md` using the candidate worktree binary.
- **Unit & integration suites**:
  - `cargo test --lib cloud_`: `93 passed; 0 failed` (including `dactyl_019_problem_codes_and_staged_first_party_origin_helpers`, `device_dpop_signer_with_audience_and_token_prefix_helpers`, `device_session_sidecar_enforces_scope_and_clears_on_restart`).
  - `cargo test --lib container_signer`: `2 passed; 0 failed` (including `container_signer_allows_cloud_control_gateway_path_and_denies_019_admin_ops`).
  - `cargo test --lib advertised_rpc_sessions_match_the_actual_entry_guards` (`1/1`) and `cargo test --lib core::interop` (`25/25`): passed.
  - `cargo test --test cloud_dpop_e2e_1878`: `2 passed; 0 failed` (end-to-end enrollment, `401 use_dpop_nonce` retry, `/cloud-query`, `/cloud-batch`, `/cloud-control`, `/device-refresh` rotation, container signer Unix socket signing + `/device-enroll` denial, DPoP session logout, `refresh_reuse_detected`, `device_revoked`, `record_rejected_credential`, strict `DECAPOD_CLOUD_AUTH_SCHEME=dpop` 404 fail-closed, and zero SQLite creation).
  - `cargo test --test cloud_admin_boundary`: `7 passed; 0 failed` (including `dactyl_019_six_account_operations_and_problem_codes_route_cleanly_without_sqlite` covering `RepositoryEnroll`, `ResourceLimits` direct + transitional `/propodus-onboarding/` + legacy `/account-status` fallback, `OrgAdmins`, `OrgRecovery`, `PolicyTemplates`, `OrgUpgrades`, `version_conflict`, `grant_revoked`, `seat_revoked`, and zero SQLite creation).
  - `cargo test --test cloud_supabase_boundary`: `52 passed; 0 failed`.
  - `cargo test --test cloud_wave_parity`: `5 passed; 0 failed`.
  - `cargo test --test cloud_workspace_validation_1886`: `4 passed; 0 failed`.
  - `cargo test --test cli_contracts`: `134 passed; 0 failed; 1 ignored` (including updated `cloud_admin_help_secrets_and_rpc_capabilities_stay_in_sync`).
  - `cargo test --test doc_alignment`: `5 passed; 0 failed`; `cargo test --test interop_conformance`: `12 passed; 0 failed`.
- **Remaining external gates**:
  - Live hosted Propodus/Supabase/Stripe deployments are outside offline loopback tests; Buildkite was waived by operator instruction; GitHub Actions CI runs upon PR push.

## Read-only observability projections & authenticated HTTP transport verification (#860, #874, #1712-#1714, #1719-#1720, #1723, #1727-#1728, #1730-#1732)

Verified on `origin/master` (`9fbf3c310`, `v0.162.0`) inside isolated worktree `agent/swarm-w6-proj/secu_01m81fz0x05nhj39-projections`:

- **Unit & guard parity suites**:
  - `cargo test --lib http_transport`: `5 passed; 0 failed` (loopback-by-default fail-closed binding, empty-token rejection, constant-time Bearer check, foreign `Origin`/`Host` rejection, live loopback request-bound `Idempotency-Key` conflict `409 idempotency_conflict`, finite entry & byte cache limits `503 idempotency_capacity_exceeded`, uncertain-outcome non-replay `409 unknown_outcome_reconcile_required`, `X-Decapod-Request-Id` replay rejection `409 replayed_request`, slowloris `408 Request Timeout` handling).
  - `cargo test --lib advertised_rpc_sessions_match_the_actual_entry_guards`: `1 passed; 0 failed` (verifies all 6 `observe.*` RPC ops in `interop::RPC_OPS` match entry guards in `src/decapod/lib.rs`).
- **End-to-end integration suite (`tests/observability_projections.rs`)**: `10 passed; 0 failed`:
  - `issue_1712_and_1720_specs_governance_traversal_security_and_adversarial_paths`: specs pagination, `original_intent` vs `refined_intent`, auto-approved plan non-human label, malicious spec text security flags & secret redaction, traversal edges + missing obligation detection, traversal/symlink/secret path rejection.
  - `issue_1713_timeline_witnessed_vs_declared_and_tampered_history_detection`: deterministic multi-reader replay, `Declared` vs `Witnessed` trust classes, and tampered-history detection (`TRAJECTORY_HASH_MISMATCH`, `MISSING_INITIAL_LOOP_ATTEMPT`, `FORGED_OR_DANGLING_CUSTODY_LINK`, `FORGED_VALIDATION_RECEIPT`).
  - `issue_1727_and_1728_snapshot_consistency_inventory_and_anchor_staleness`: concurrent mid-read Git index change marks `consistent = false`, staged/unstaged/untracked/binary/secret inventory classification, and SHA-256 `SourceAnchor` staleness detection immediately after edit.
  - `issue_860_and_1723_workunit_activity_transitions_and_shared_renderer`: typed workunit transitions, stale completion classified as `Drifting` + `StaleInput`, dependency blocking, `is_kernel_proof = false` / `semantic_alignment = "not_proven"` claimed effects, and shared human/machine renderer.
  - `issue_1730_validation_plan_requirement_drop_joint_checks_and_cyclic_constraints`: rules across all 4 authorities, dropped `R3` requirement surfacing, unrun joint check with `clean_merge_does_not_prove = true`, union-find global contradiction detection for `A=B, B=C, A!=C`, and `ZERO_ASSERTION_SUITE` blocking.
  - `issue_1731_pr_manifest_correspondence_and_drift_states`: unpublished plan non-masquerade (`plan_is_not_published = true`, never `human_approved`) and full drift/correspondence flags (`PrSuperseded`, `UncommittedEditsOmitted`, `WorkspaceDiffersFromPlannedScope`, `FinalShaChanged`, `BranchRebased`, `TargetMainMoved`, `CheckTiedToWrongHead`).
  - `issue_1732_context_provenance_redacts_secrets_and_prompts_and_denies_cross_scope`: redaction of `raw_prompt`, `thinking`, API keys, and Bearer tokens, `agent_reported` trust preservation, and `CROSS_SCOPE_DENIED`.
  - `issue_1719_1720_observe_loopback_server_enforces_all_security_gates`: live loopback HTTP server verifying valid GET + `X-Decapod-Read-Only: true` + `X-Decapod-Snapshot-Id`, wrong Bearer (`401`), revoked token ID (`401 token_revoked`), cross-tenant (`403 cross_tenant_denied`), cross-repo (`403 cross_repo_denied`), foreign Origin (`403 forbidden_origin`), mutating RPC op (`403 read_only_enforced`), `DELETE` method (`405 method_not_allowed`), and allowlisted `POST /rpc/v1 observe.validation_plan` (`200`).
  - `issue_874_bounded_child_timeout_output_limit_and_http_stdio_parity`: stalled child timeout (`504 rpc_timeout_unknown_outcome`), oversized stdout cap (`502 rpc_output_limit_exceeded`), and live loopback HTTP vs `decapod rpc --stdin` semantic parity.
  - `cli_observe_subcommands_and_rpc_ops_end_to_end`: all 6 `decapod observe <subcommand>` JSON/text outputs, all 6 `decapod rpc --op observe.*` invocations, and non-loopback `decapod observe serve --bind 0.0.0.0:0 --token-stdin` refusal.
- **Formatting & static analysis**: `cargo fmt --all -- --check` and `CLIPPY_CONF_DIR=.config cargo clippy --all-targets -- -D warnings`. Hosted GitHub Actions / Buildkite CI remains pending PR execution.

## Multi-org Cloud parity and organization constitution overlay verification (#1895, #877)

- **Multi-org Cloud tenancy parity (`#1895`)**: `tests/cloud_tenancy_parity_1895.rs` verifies multi-organization switching, cross-tenant refusal, `#215` org-admin vs zero-paid-seat isolation on Repo 1B, concurrent CAS race exclusivity, cross-repo `operation_id` replay rejection, stale `task_revision` rejection, commit/policy-bound evidence capture, mid-session seat revocation and network-cut fail-closed behavior with zero `.decapod/data/decapod.db` creation, independent observer state checks, and local SQLite equivalence on the supported operation subset (using a local 2-org/2-DB gateway simulation rather than live hosted Cloud infrastructure). Unit coverage in `src/decapod/core/cloud_parity_matrix.rs` asserts matrix schema invariants and Cloud refusal behavior (`CLOUD_LEASE_RENEW_UNSUPPORTED`).
- **Organization constitution overlays (`#877`)**: Unit tests in `src/decapod/core/org_overlay.rs` and integration suite `tests/org_constitution_overlay_877.rs` verify all eight authority and integrity states (`base-only`, `organization-only`, `project-only`, `all-three`, `conflicting`, `stale`, `tampered`, `offline`), `ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT`, `ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED`, `ORG_OVERLAY_PRIVATE_LEAK_REJECTED`, Layer 4 non-authority, `interop.manifest` authority hierarchy export, and `validate_project_config_downgrade_check` / `validate_override_authority` (`ORG_OVERLAY_POLICY_DOWNGRADE`) enforcement inside `src/decapod/core/validate.rs`.

## First-class `decapod cloud` admin CLI/RPC verification (#1896, #1836, #1835, #1877)

Verified on `origin/master` (`a53f2eb7d`, `v0.161.0`) inside isolated worktree `agent/swarm-cloud-admin/feat_01m80wt1ty67gx2n-cloud-admin-cli`:

- **Formatting, feature matrix, lints & documentation sync (`--locked`)**:
  - `cargo fmt --all -- --check`: passed.
  - `cargo check --no-default-features`: passed (0 warnings).
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings; verified `DecapodError` stays within the `clippy::result_large_err` 128-byte ceiling via 1-byte `Option<CloudAuthErrorCode>`).
  - `cargo run --locked --features supabase-cloud -- docs build`: synchronized `docs/agent/command-contracts.md` (`decapod cloud` contract section + `Operation: Cloud{Orgs,Members,Invites,Identity,Billing}`).
- **Unit & integration suites (`--locked --features supabase-cloud`)**:
  - `--lib cloud_`: `87 passed; 0 failed` (`1920 filtered out`), including `core::cloud_admin::tests::*` (secret redaction/zeroization, strict HTTPS portal URL validation, URL fragment masking, UUID/bounded-ID validation) and `core::cloud_client::cloud_admin_client_tests::*` (mutating vs read `outcome_unknown` separation, AAL2 `mfa_step_up_required` vs `admin_required`, admin diagnostic codes).
  - `--lib advertised_rpc_sessions_match_the_actual_entry_guards` (`1/1`), `--lib core::interop` (`25/25`), and `--lib propodus` (`18/18`): passed.
  - `--test cloud_admin_boundary`: `6 passed; 0 failed` (loopback HTTP E2E coverage across CLI and RPC `--op`/`--stdin` for `orgs` `#215` role/seat separation + header warnings, `members` slot-to-UUID resolution + pending-slot fail-closed + `assign`/`revoke-seat`/`transfer-ownership`/`project-settings`, `mfa_step_up_required` + single-send `outcome_unknown` non-retry, `invites` & `identity` `--secret-stdin` + argv secret rejection + zero-paid-seats notice + fragment redaction, strict `https://` billing portal validation, `overage_cap_exhausted` `spend_cap_warning`, and asserted `.decapod/data/decapod.db` / `todo.db` non-creation).
  - `--test cli_contracts`: `134 passed; 0 failed; 1 ignored` (including `cloud_admin_help_secrets_and_rpc_capabilities_stay_in_sync`).
  - Existing Cloud/interop regression suites: `cloud_supabase_boundary` (`52/52`), `cloud_cli_boundary` (`2/2`), `cloud_command_path` (`1/1`), `cloud_dactyl_boundary` (`1/1`), `cloud_todo_resume_wire` (`3/3`), `propodus_contract` (`6/6`), `release_dependency_policy` (`4/4`), `doc_alignment` (`5/5`), `interop_conformance` (`12/12`), `capability_contract_drift` (`3/3`), and `rpc_golden_vectors` (`6/6`).
- **Observed host/pre-existing gaps**:
  - Live hosted Propodus/Stripe end-to-end runs remain unexecuted locally (`supabase_service` hosted checks require live deployment credentials). Buildkite validation was waived by operator instruction; hosted GitHub Actions CI remains pending PR execution.
  - Pre-existing unrelated checks on this checkout: `integration_sharding_policy` passed `22/23` (`relocated_gatling_shards_route_exact_filters_and_preserve_runfiles` failed on macOS `/var` -> `/private/var` temp symlink); legacy `cloud_backend_storage` (`DECAPOD_CLOUD_DATASTORE=neon`) failed `5/9` on pre-existing `provider_unavailable` vs `offline` and origin-error text assertions untouched by this PR.

## Effective operation grants and guidance hygiene verification (#1905, #1893)

Verified on `origin/master` (`485a35148`, `v0.160.0` + PR `#1908` + PR `#1911` + PR `#1909` + PR `#1912`) inside isolated worktree `agent/swarm-1905/feat_01m80ckdn0ym2ja0-effective-grants-1905`:

- **Formatting, lints, docs generation & feature matrix (`--locked`, post-`485a35148` merge)**:
  - `cargo fmt --all -- --check`: passed.
  - `cargo check --no-default-features`: passed.
  - `cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings).
  - `decapod docs build`: synced `docs/agent/command-contracts.md` (`### Operation: PolicyGrantExplain` + `### Operation: InteropCapabilityContracts`).
- **Unit, Cloud-consumer, plugins & integration suites (`--locked`, post-`485a35148` merge)**:
  - `--features supabase-cloud --lib -- effective_grant project_specs docs entrypoint_integrity capability_contract`: `80 passed; 0 failed` (`1920 filtered out`; covers Red->Green `#1` path narrowing/disjoint workflow denial, `#2` copied parent grant / forged approval / `PlanState::Approved` / candidate-branch `.decapod/config.toml` widening including canonicalized primary-checkout match / budget widening refusal, `#3` `S0 -> S1` prestate drift / lease expiry / harness mismatch / policy revision change + lawful re-formation, `#4` 24-permutation order independence / replay determinism / strict scope ambiguity rejection / plain topic label acceptance / missing fields, adversarial shared-parent cumulative budget `2+2 > 3` refusal vs `2+1 <= 3` acceptance + delegation depth limit, Cloud `VerifiedCloudScope`, witness-attribution separation, capability contract unit suite, and `#1893` regression assertions). Pre-sync full `--lib` also passed (`1981 passed; 0 failed; 8 ignored`).
  - `--features supabase-cloud` Cloud-consumer targets: `cloud_cli_boundary` (`2/2`), `cloud_command_path` (`1/1`), `cloud_dactyl_boundary` (`1/1`), `cloud_supabase_boundary` (`52/52`), `cloud_todo_resume_wire` (`3/3`), `propodus_contract` (`6/6`), `release_dependency_policy` (`4/4`), `supabase_service` (`3/3`, `4 ignored`).
  - All 8 `plugins_*` targets (clean ambient env): `plugins_aptitude_tests` (`10/10`), `plugins_decide_tests` (`18/18`), `plugins_federation_tests` (`24/24`), `plugins_health_tests` (`15/15`), `plugins_internalize_tests` (`6/6`, `1 ignored`), `plugins_obligation_tests` (`24/24`), `plugins_policy_tests` (`21/21`), `plugins_todo_tests` (`40/40`, including `handoff_advances_lease_generation_and_preserves_intent`, `handoff_checks_real_actor_and_preserves_owner_until_approved`, `handoff_failure_returns_agent_recovery_without_authorizing_or_mutating`, `operator_recovery_requires_exact_one_use_authority`).
  - `--test cli_contracts`: `133 passed; 0 failed; 1 ignored` (covers `effective_grant_cli_contracts::red_green_5_cli_rpc_grant_refusal_lawful_repair_and_fresh_authorization`, `cli_cumulative_delegation_budget_rejects_sibling_overflow`, `guidance_surfaces_exclude_maintainer_entrypoint_drift_setting` across `--help`/`docs ingest`/`rpc context.resolve`, and `todo_help_schema_and_docs_stay_in_sync` including `delegate`).
  - `--test assets_schema_contract`: `4 passed; 0 failed`; `--test capability_contract_drift`: `3 passed; 0 failed`.
  - `--test concurrent_worktree_integration_1469`: `3 passed; 0 failed` (preserves legacy advisory `paths:[...]` claim overlaps via `is_advisory_paths`).
  - `--test doc_alignment`: `5 passed; 0 failed`.
  - `--test interop_conformance`: `12 passed; 0 failed`.
  - `--test policy_self_authorization_1776`: `9 passed; 0 failed`.
  - `--test todo_enforcement`: `6 passed; 0 failed`.
  - `--test workunit_cli`: `7 passed; 0 failed`.
  - Pre-sync `--test datastore_contract`: `4 passed; 1 failed` (`logical_schema_inventory_is_structurally_valid_and_matches_runtime_core` pre-existing `0.143.1` vs `0.160.0` version string mismatch on `master`); pre-sync `--test source_ci_profile_policy` on `macos-aarch64`: `55 passed; 4 failed` (4 Linux-only `linux-x86_64` / macOS `/var` temp symlink checks; Buildkite waived by operator).

## Composed service capability contract verification (#1906)

Verified on branch `agent/swarm-1906/feat_01m80ckft9ab64f9-capability-contracts-1906` merged with `origin/master` (`8c2f55246`, `v0.160.0`):

- Formatting, lint, and feature-matrix compilation (after final edit):
  - `cargo fmt --all -- --check`: passed.
  - `CLIPPY_CONF_DIR=.config cargo clippy --locked --all-targets --all-features -- -D warnings`: passed.
  - `cargo check --locked --no-default-features`: passed.
- Composed capability contract unit and integration tests (after final edit):
  - `cargo test --locked --lib core::capability_contract`: `7 passed; 0 failed` (schema constants, self-verification rejection, self-reported observation rejection, HTTP 200 not masking Dactyl field/error drift, four distinct composite consequence states, fault-retention enforcement on compensation, and 21-scenario multi-repo compatibility fixture).
  - `cargo test --locked --test capability_contract_drift`: `3 passed; 0 failed` (CLI `capabilities contracts --fixture --format json`, RPC `interop.capability_contracts` input rejection, local offline repo usability, and `govern project-check` failing closed on `CAPABILITY_CONTRACT_DRIFT`).
  - `decapod capabilities contracts --fixture --format text`: `21/21` scenarios passed (`declared`, `observed`, `currently_verified`, `incompatible`, `unsupported`, `stale`, and `prepared`/`observed`/`held`/`compensated` trace separation).
  - `decapod docs build`: synchronized `docs/agent/command-contracts.md` (`+8` insertions for `decapod capabilities contracts` and `Operation: InteropCapabilityContracts`).
- Core repository conformance and boundary suites:
  - `assets_schema_contract`: `4 passed; 0 failed`; `contract_conformance`: `12 passed; 0 failed`; `doc_alignment`: `5 passed; 0 failed`; `rpc_golden_vectors`: `6 passed; 0 failed`; `consumer_readiness`: `40 passed; 0 failed`; `propodus_contract`: `6 passed; 0 failed`; `cloud_dactyl_boundary`: `1 passed; 0 failed`; `constitution_claims`: `4 passed; 0 failed`; `substrate_integrity`: `9 passed; 0 failed`.
  - Pre-existing/host-platform caveats on macOS (`macos-aarch64`): `source_ci_profile_policy` (`55 passed; 4 failed` on `linux-x86_64` and `/var` temp canonicalization checks) and `integration_sharding_policy` (`22 passed; 1 failed` on `/var` temp canonicalization `relocated_gatling_shards_route_exact_filters_and_preserve_runfiles`).

## Operational reliability fixes verification (#1881, #1837, #1884, #1838)

Verified on `origin/master` (`295f0af39`, `v0.160.0` + PR `#1908`) inside isolated worktree `agent/swarm-readiness-b/bugs_01m80ex82zgnq661-ops-1881-1837-1884-1838`:

- **Formatting, lints, and feature matrix**:
  - `cargo fmt --all -- --check`: passed.
  - `CLIPPY_CONF_DIR=.config cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings; added `HeartbeatTaskRow` type alias in `src/decapod/core/todo.rs` for Rust `1.96` `clippy::type_complexity`).
  - `cargo check --no-default-features`: passed.
- **Unit & integration suites (`--locked`)**:
  - `--lib plugins::container::tests`: `65 passed; 0 failed` (covers `podman_machine_start_captures_output_without_reading_terminal_input` child-isolated fixture writer for `#1838`).
  - `--lib core::workspace::tests`: `40 passed; 0 failed` (covers `container_launch_snapshot_lifecycle_is_repeatable_and_failure_safe` using `.decapod/backups`).
  - `--lib core::cloud_workspace::tests -- --test-threads=1`: `3 passed; 0 failed` (note: parallel `host_cloud_relay_enforces_container_admission_scope_and_allowlist` hit transient `Broken pipe` / `Socket is not connected` under concurrent `--lib workspace` load before passing serially `3/3`).
  - `--lib native_proof`: `6 passed; 0 failed` (covers `readonly_identity_reuses_shared_tool_bytes_within_one_capture` for `#1884`).
  - `--lib task_journal`: `74 passed; 0 failed`.
  - `--lib reflection`: `33 passed; 0 failed`.
  - `--lib validation_history`: `34 passed; 0 failed` (covers `capture_task_binding_with_budget_can_extend_a_lease_without_rotating_custody` for `#1837`).
  - `--lib core::todo::tests`: `10 passed; 0 failed`.
  - `--lib plugins::verify::tests`: `14 passed; 0 failed`.
  - `--test todo_enforcement`: `6 passed; 0 failed` (covers `test_workspace_ensure_container_reports_default_backup_denial_before_build` and `test_workspace_ensure_container_creates_coordination_todo` for `#1881`).
  - `--test cloud_cli_boundary`: `2 passed; 0 failed` (preserves Wave 2 `CLOUD_LEASE_RENEW_UNSUPPORTED`).
  - `--test consumer_readiness`: `40 passed; 0 failed`.
  - `--test source_ci_profile_policy` on macOS (`macos-aarch64`): `55 passed; 4 failed` (4 Linux-only `supabase-consumer`/`fmt` platform checks and `/var`->`/private/var` temp path symlink in `provider_carrier_executes_immutable_r_and_wrapper_preserves_fixed_context_and_failure`, unrelated to this PR; full Linux CI and waived Buildkite remain separate external gates).

## Portable policy transition approvals & target-map unification verification (#1833, #1832)

Verified on `origin/master` (`efc05d792`, `v0.160.0`) inside isolated worktree `agent/swarm-readiness-a/code_01m80cm518wkhbww-policy-1833-1832`:

- **Source-CI inventory reconciliation, formatting, lints & feature matrix**:
  - `.decapod/config.toml` `[proof.required_profile].reviewed_sources_digest` updated to `sha256:96d47fcfe781090fd7945ddb1e42a9b82ec8e7cf6498c2323d364d4a3c512975` for `v0.160.0` `Cargo.toml`/`Cargo.lock` release metadata.
  - `cargo fmt --all -- --check`: passed.
  - `CLIPPY_CONF_DIR=.config cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings).
  - `cargo check --no-default-features`: passed.
- **Unit & integration suites (`--locked`)**:
  - `--lib repository_profile`: `25 passed; 0 failed` (covers `transition_approval::tests::signature_requires_accepted_key_exact_transition_and_live_expiry`, `transition_approval::cloud::tests::cloud_decision_requires_exact_live_repository_authority`, and `preserve_accepted_rejects_candidate_target_map_changes`).
  - `--lib validation_rules`: `17 passed; 0 failed` (covers `fresh_ci_checkout_requires_independent_approval_after_inventory_upgrade`).
  - `--lib validation_history`: `34 passed; 0 failed` (covers `target_scoped_completed_evidence_cannot_authorize_full_repo_or_other_target` and `current_target_only_diagnostic` verification).
  - `--lib public_buildkite`: `91 passed; 0 failed; 1 ignored` (covers `log_reader_rejects_escape_second_hop_and_http_denial_without_retry` timeout tolerance).
  - `--test consumer_readiness`: `40 passed; 0 failed` (covers `single_platform_to_platform_map_requires_exact_approval_and_runs_host_checks`, `target_map_addition_and_update_require_exact_approval_and_preserve_receipt_scope`, and `decapod_strict_profile_downgrade_remains_unconditionally_blocked_even_with_approval`).
  - `--test source_ci_profile_policy` on macOS (`macos-aarch64`): `55 passed; 4 failed` (the 4 failures are host-OS mismatches on macOS vs Linux-only `supabase-consumer`/`fmt` fixtures and `/var` -> `/private/var` temp path canonicalization in `provider_carrier_executes_immutable_r_and_wrapper_preserves_fixed_context_and_failure`, unrelated to `#1833`/`#1832`; full Linux `Tests (core)` and Buildkite remain separate CI/operator gates, with Buildkite waived by operator).

## Cloud Wave 2 verification (#1836, #1835, #1886)

Verified on the merged `origin/master` (`v0.159.0`, including PR `#1901` and
PR `#1902`) candidate:

- **Feature matrix, formatting, and lints**:
  - `reviewed_sources_digest` in `.decapod/config.toml` reconciled to `sha256:875df5f0a0da8415e102fa983a448cf995b93f52aa689d7a4dc71726a36c7ae9` after `v0.159.0` (`#1902`) bumped `Cargo.toml`/`Cargo.lock` with unchanged CI/toolchain/workflow contracts.
  - `cargo check --locked --no-default-features --lib` with `RUSTFLAGS="-D warnings"`: passed (0 warnings).
  - `cargo fmt --all -- --check`: passed.
  - `CLIPPY_CONF_DIR=.config cargo clippy --all-targets --all-features -- -D warnings`: passed (0 warnings).
- **Unit & migration suites (`--locked --features supabase-cloud --lib`)**:
  - `--lib` (`v0.159.0`): `1970 passed; 0 failed; 8 ignored` (including `cloud_dpop_tests` cross-platform OS keystore + `with_os_keychain` tests, `propodus_tests` bounded metadata warning & `quota_exceeded`/`overage_cap_exhausted`/`provider_disabled` mapping tests, `cloud_workspace_tests` custody bind/reconcile + container relay tests, and `#1901` unit checks).
  - `--lib core::migration`: `13 passed; 0 failed`.
- **Cloud & boundary integration suites (`--locked --features supabase-cloud`)**:
  - `cloud_workspace_validation_1886`: `4 passed; 0 failed` (accepted Cloud `workspace ensure` + `validate` with conflicting SQLite bytes `existing_sqlite = false/true`, lost-ack `get_operation` reconciliation, and `--container` host-relay admission + replacement-container refusal).
  - `cloud_supabase_boundary`: `52 passed; 0 failed` (including `live_cloud_status_reads_only_the_authenticated_spend_summary`, `quota_overage_and_provider_disabled_errors_surface_explicit_diagnostics`, `cloud_revoke_supports_dpop_session_logout_management_revoke_and_retry_on_failure`, and `cloud_devices_reports_non_secret_inventory_and_surfaces_deprecation_warnings`).
  - `cloud_wave_parity` (`5/5`), `cloud_cli_boundary` (`2/2`), `cloud_command_path` (`1/1`), `cloud_dactyl_boundary` (`1/1`), `cloud_mixed_worktree` (`3/3`), `cloud_todo_resume_wire` (`3/3`), `policy_self_authorization_1776` (`9/9`), `propodus_contract` (`6/6`), `release_dependency_policy` (`4/4`), `release_workflow_policy` (`22/22`), and `supabase_service` (`3 passed; 0 failed; 4 ignored` external PG17/hosted fixtures).

## Context, admission, and outcome checks

Focused Rust regressions cover repetition and no-match retrieval, override behavior, authorization boundaries, envelope budgets, atomic correction and expiry, legacy capsule hashes, source admission, public history generation, explicit host identity, and incomplete outcome samples. The accepted JSON and acceptance-observer tests remain part of the regression suite. The Constitution schema is independently checked by the existing workflow.

The handoff at `6199af03` recorded 121 focused passes against its v0.148.12 source. Its push validation rejected stale merge-base and spec metadata. That evidence remains historical. This candidate requires its own check results on the accepted target. Host records under `docs/research/host-outcome-pilot` and issues #880 and #1095 retain timeouts, missing measurements, and the distinction between observed context resolution and delivery to a model. The public chronology makes no claim about priority or measured effectiveness.

## Cloud control-plane, Dactyl 0.18.0, and Local/Cloud branding verification (#1809, #1808, #1769, #1667, #1777, #1778, #1590, #1377, #1380)

- **Dependency & build locks**: `tests/release_dependency_policy.rs` verifies
  registry `dactyl-db = "=0.18.0"` (SHA-256
  `6c26eaf28272a50517d7fe300b56940ac03627477b48579293b231c33de905b4`),
  `quinn-proto >= 0.11.19`, and preserved `sqlite`/`neon`/`supabase-cloud`
  features. Bazel `//:cargo_inputs_policy` passes under `--lockfile_mode=error`
  against the updated `MODULE.bazel.lock`. The reviewed 105-file proof source
  inventory digest in `.decapod/config.toml` (`[proof.required_profile].reviewed_sources_digest`)
  is reconciled to `sha256:35d28c6858a245822d6d893e5dc658b5f3bcf47485f2601c4f019d756c82502f`,
  accounting solely for `Cargo.toml`, `Cargo.lock`, and `MODULE.bazel.lock`
  with zero workflow, executor, or toolchain changes. Native `nix build` was
  not executed on this macOS host (`nix` is not installed); Nix CI verifies
  `flake.nix`.
- **Unit, branding, docs & integration proofs**: `cloud_dpop` (4/4),
  `cloud_proof` (2/2), `container_signer` (1/1), `dactyl_todo` (7/7),
  `db_maintenance` (6/6), `sql_context` (16/16), `backend_tests`,
  `propodus_tests`, `cloud_supabase_boundary` (41/41), `cloud_wave_parity`
  (5/5, including `dactyl_018_dpop_signer_and_control_request_wire_contract`
  verifying canonical `/functions/v1/decapod-onboarding/config`, RFC 9449 `DPoP`
  headers, `propodus_repository_access_v1`, and `propodus_proof_acceptance_v1`),
  `cloud_cli_boundary`, `cloud_mixed_worktree` (3/3),
  `cloud_command_path`, `cloud_dactyl_boundary`, `cli_contract_enforcement`
  (24/24), `doc_alignment` (5/5), `shared_state_semantics_1247` (10/10),
  `policy_self_authorization_1776::candidate_worktree_cannot_switch_cloud_backend_to_local`,
  `decapod docs build` embedded docs sync, `cargo fmt --all -- --check`, and
  `clippy --all-targets --all-features -- -D warnings`.
- **Owner repair wire marker & public-repo redaction (#1590, #1809)**:
  `src/decapod/core/db_maintenance.rs` binds `OWNER_REPAIR_V1_SQL` strictly to
  the public registered wire marker
  `SELECT propodus.owner_repair_v1($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`
  with unit regression checks verifying exact wire marker equality and zero
  internal table/CTE expansion, and `src/decapod/core/cloud_proof.rs` /
  `db_maintenance.rs` omit private repository filenames and migration ranges.
  (Buildkite build `13224` passed 74+ checks and hit an unrelated transient
  Linux `ETXTBSY` runner flake in `podman_machine_start_captures_output_without_reading_terminal_input`;
  retried cleanly with fresh Buildkite validation.)
- **Local PostgreSQL 17 + PostgREST conformance (#1377, #1369)**: Executed
  disposable local PG17 + PostgREST conformance harness, passing
  `supabase_service::local_native_managed_project_contract` and
  `local_native_managed_project_revocation` (with independent PG17 ledger
  observation and revocation enforcement) against the `dactyl-db 0.18.0` binary.
  Managed Auth OAuth issuance and hosted production verification remain separate
  external acceptance gates (`#1378`, `#1379`).

## Current report validation scope

The current verification boundary begins at accepted release commit 9d1aed27.
A source pipeline rejected the previous candidate's comparison after external
integration advanced master. Preserve that rejection as non-ready evidence;
independently require complete applicable checks, current source/policy and
exact publisher readback for this report's actual head/base. The required
full-library owner remains mandatory, including the publication regressions.

The hosted checkpoint check reports the immutable PR event base and the
resolved target ref when those revisions differ. Historical checkpoint results
remain bound to the original commits. Evidence for current admission must
bind the newly accepted base. The macOS live-admission regression records
its fixture-owned toolchain file by canonical path so the same checks run
when the host temp directory is reached through a symlink.

The reviewed inventory includes the accepted v0.158.2 package version fields in
`Cargo.toml` and `Cargo.lock`. All 107 proof inputs match the accepted target at
`aa4d25da`.

## Live rejection specificity (#1748)

The disposable GitHub acceptance requires the invalid payload's actual failure,
the independent guard's executed success, and PUBLICATION_NOT_READY with
READINESS_BLOCKED at publication. A timeout, lock or transport failure cannot
satisfy that negative control. A previously permissive run remains partial
evidence even when its test process reported success; fresh execution must prove
the stricter assertion on the corrected controller.

The admission repair incorporates accepted target
`e7b54246d5c4cc325883ea1281aea039a0ad064f` after validation and the first
branch-push CI refused moved targets and required target ancestry. The source
inventory review is renewed for accepted release 0.148.6. Earlier tag inventory,
stale-base and entrypoint-alignment failures remain failed attempts, with no
executed required suite or publication authority inferred from them. A separate
development-controller build is necessary to exercise the explicitly authorized
source configuration policy absent from installed 0.148.0; compilation alone
does not establish readiness or replace complete validation and live acceptance.

## Required publication regression coverage (#1747, #1748)

The complete fourth real-forge acceptance passed: one test, zero failures,
311.75 seconds on macOS, using source controller
`99a6f59f5252ce37845b47b2d9673b169d3df509`. The actual GitHub PR is
`alexhraber/decapod-admission-acceptance-20261117-1747-04#1`; immutable evidence
commit `1c21226bc3c5c65afb504a37bbe00829e308c593` retains three native receipts,
their observations, failed-source observations and post-creation rejection.
Every archived byte was checked against its Git blob identity. The final
validated head is `cd938efe2d50c59676212ea9a747617f227c106a`, bound to base
`0e36f8e4b336985fbe0a3b4d92fbd48f43b4027b`. Advancing the target to
`2a5b1f276093139d160c85d72af5b4253b0ecfc0` invalidated applicability: both
publication and repeated validation rejected with `STALE_ACCEPTED_BASE`.
The draft PR remains historical admission, never authorization to merge.
`docs/publication-admission-acceptance.md` records the reproducible command,
all exact subjects, receipt hashes, rejected controls and assurance limits.

Full Buildkite validation passed for source candidate
`e81b89b36d330e36ab06a74b4eae409117e1facd` against accepted
`e7b54246d5c4cc325883ea1281aea039a0ad064f`. The second real-forge attempt
rejected invalid source, admitted only its repaired head, confirmed draft
quarantine after a real post-creation mutation, and rejected a later source
mutation. That last operation regenerated and staged managed specs before
refusing a missing authoring checkpoint. Recovery must refresh through the
governed owner, checkpoint a new commit and earn new proof/admission; it must
not hand-restore machine-owned state or reuse the older artifact's success.
The harness now performs that recovery before its target-advancement controls.
The third real-forge run rejected both publication and repeated validation after
the remote target advanced. GitHub's PR response retained its older base SHA;
the acceptance therefore reads the actual `git/ref/heads/main` independently
and records the PR metadata separately. Current target identity cannot be
inferred from that historical PR field. The publisher already verifies exact
remote branch refs before and after readback, independently of PR metadata.

The first actual CI library run executed all seven live-proof publication
regressions and the three source-development policy controls successfully;
its new config-preservation fixture failed on inserting an absent TOML key.
That fixture is repaired. The next Clippy run identified duplicated inclusion
of the shared consumer fixture, now consolidated at the library test root.
The first live GitHub attempt failed before source execution because its
declaration used the reserved `program` toolchain label. Its assertion refused
to treat that adapter failure as actual failed-source execution. The attempt
and disposable repository are preserved. Their failures were not relabeled as
success; the separate fourth run supplies the complete passing acceptance.

`tests/unit/core/workspace_publication_tests.rs::live_admission` executes real
required source commands and real Git commits/remote readback, then exercises
the publisher's admission boundary with deterministic forge responses. It covers
failure, repair, fresh proof, changed same-path bytes/head, target movement,
missing/skipped/failed required observations, source/policy/governance/tool drift,
and post-creation rejection. Additional quarantine tests cover lost replies,
permission denial and unrelated PR protection. These fixtures do not establish
real GitHub publication. The separately documented disposable-repository
acceptance records actual CLI/forge evidence and its limitations.
The existing always-selected `supabase-consumer` recipe owns these library
tests, including test-only changes; `tests/source_ci_profile_policy.rs` protects
that full-suite obligation without changing the accepted executor. GitHub
ruleset 12701816 requires Buildkite's
`CI / test (target="core") (push)` from App 805657 and strict target freshness;
`CI / supabase-consumer (push)` from the same App is also required: its actual
`cargo test --lib` recipe owns the library-unit publication regressions. The
core integration recipe alone does not execute those unit tests. The existing
OrganizationAdmin bypass remains a separate explicit authority.

## Real recovery fixture prerequisites (#1696)

`tests/support/recovery_filesystem.rs` owns the existing private filesystem
probe shared by unit filesystem regressions and `tests/sqlite_maintenance.rs`.
It checks device identity, ownership, space and atomic no-clobber rename support
before running recovery success assertions. Linux may select the preexisting
`/dev/shm` candidate when the default temporary filesystem is incompatible;
unavailable candidates produce explicit diagnostics and exhaustion fails.
Production Dactyl guards and unsupported-path refusal tests remain authoritative.
The standalone and aggregate Bazel targets declare this helper as a real input.

## Autonomous convergence protocol fixtures (#1690–#1692)

`tests/unit/core/reflection_tests.rs` exercises source discovery, engineering
choices, inspected predicates, stagnation, regression, exhaustion, canonical
business dependencies, actual policy approvals, forged/stale observations,
idempotent retry and concurrent task isolation. Its deterministic comparison
executes a baseline mock that delegates every proposed question and compares
actual governed outcomes, rounds and retained bytes. It does not evaluate an
LLM or claim improved semantic reasoning or completed engineering tasks.
`tests/infer_precision.rs` exercises the real orientation-to-mentor continuation,
preserves the native diagnostic/publication guard, verifies all four generated
entrypoints route to the embedded contract, and checks that architecture words
do not manufacture missing human authority. Existing native proof remains
authoritative; source-CI observations cannot substitute for the required full
exact-candidate validation receipt. Fixture output supplies actual measurements
only when its owning test lane executes successfully.

## PR1661 repair evidence contract

The non-Git entrypoint regression must demonstrate successful prerequisite-only
diagnostics, unavailable authoring custody, and rejected full publication
validation for the same initialized project. Guided-path traversal is checked
through `govern project-check`, which must return the actual configuration
failure rather than an earlier workspace diagnostic. Existing transactional
binding, full lease-lifecycle, shared-resource, cross-process recovery and
combined-candidate tests remain required. Clippy must accept the real PathBuf
returned by workspace ensure without a redundant conversion. Exact final source
and current accepted master require a new full validation result; earlier PR
statuses, source fixtures and project checks cannot establish that readiness.
After a reviewed proof-source digest update, `rpc --op specs.refresh` must bind
the manifest to the final config bytes. Standalone validation rejects a stale
`config_input_hash` even when the underlying prerequisite checks pass.

The required `tests/consumer_readiness.rs` regression carries a recorded
Decapod intent through a real source-check failure, a committed source repair,
native validation and an inspectable receipt. It verifies that failure issues
no readiness receipt, fresh proof identifies the repaired commit, and receipt
digests bind the preserved intent trajectory and governance. This exercises
the public runtime and native executor with an independently read local Git
target. It establishes repository-local readiness only; it is not hosted
cloud, billing, provider OAuth or publication acceptance.

The accepted `tests/entrypoint_correctness.rs` test
`fresh_docs_cli_admission_then_complete_runtime_proves_current_source` also
exercises real CLI initialization, claimed workspace admission and first-proof
execution through `tests/support/fresh_docs_readiness.rs`. The cloud handoff
must preserve that Decapod lifecycle and its terminal evidence checks.

In the accepted 0.141.0 runtime, terminal rules consume completed source
execution evidence and recheck current governance and trajectory digests before
publishing the readiness bundle. The cloud handoff cannot substitute a partial
project-check receipt for that result. Release reconciliation changes the root
package version and generated release surfaces while retaining this proof path.

## Init-once agent continuation

This continuation adopts PR #1635's exact `ee44e5f8773697b5341107c0d3153de4ebe28bfb`
history in an independently admitted workspace. Earlier proof belongs to that
candidate; it does not establish readiness for the new changes. Required
managed scenarios now resume the first init through agent-owned `cloud login`,
then use the saved session for ordinary task access. A new pending-add scenario
checks the structured browser URL, preserved operation identity, reuse of the
same flow and absence of data dispatch. Existing lost-response scenarios now
exercise `cloud login --restart` while retaining the one-time no-replay checks.
All changes still require plain full `decapod validate` on their exact commit.
Hosted provider login, entitlement/payment execution and attachment to an
existing directory project ID remain unexecuted or unsupported contract limits.

PR #1635 subsequently merged as `c87e4f49cab8e2ccff5f9bdc5833d8602dfffdb8`
while provider jobs were queued. Their accepted-target ancestry check correctly
rejected the older candidate before running those checks. This continuation
merges the new accepted target and retains the full existing journal/checkpoint
lineage; the remaining agent UX changes are a follow-up PR. Earlier diagnostics
on `a0962755` passed 1,674 library tests (eight ignored) and all 33 Supabase
boundary tests, including the new handoff and stable-operation-ID scenarios.
That execution does not replace fresh complete proof on the reconciled commit.
The subsequent accepted release commit
`19acd21f7ff50656323d037915273f03cdedc04e` changes package versions and the
changelog and is incorporated as well; the current proof must use that target.
The 0.139.0 source-CI evaluator then correctly reported release-bound drift on
`0b3d3f2a`. A private installation of the published 0.139.0 controller, obtained
through the supported session-update flow, regenerates the entrypoints, managed
Dockerfile and spec manifest. The temporary workspace update opt-in is removed.
This projection preparation is not full readiness; the exact refreshed candidate
still needs every required provider check and independent controller collection.

## Reflection adversarial proof contract

The work branch incorporates accepted target
`d31112a1d66899e93dad48a10cc3a6363da2c7db`, including SDK 0.14.0 and the
required terminal-onboarding fixtures. The installed controller is published
0.140.3. This conflict-repair update is pushed without tests or full validation
at the user's explicit request. Earlier measurements below remain historical;
current-candidate readiness is pending. Generated surfaces match the evaluator.

Required Buildkite library execution on `dc7bf2fa198abeebba072c5394709af0dfca9d86`
passed 1709 tests (8 ignored), including all 16 reflection cases and the canonical
lifecycle case. Evidence is build 8602, job
`01a1cfd6-5d0d-4abb-853c-b4ed9164f022`. The selected scope flaw was detected
before action, an evidence-bound narrower proposal proceeded in round 2, and
the original approval/proof requirements and false completion proof were retained.
Stagnation terminates in round 2 and unresolved progress in round 3. Six ordinary
phase transitions preserve convergence; an actual governing edit invalidates it.
Source/custody changes, replay, competing submissions, untrusted instructions
and invented human approval are covered. These are deterministic protocol
assertions, not a language-model evaluation or proof of a future cache design.

The canonical lifecycle fixture registers verified writers in its isolated
store, then exercises actual federation mutations. Supersession, withdrawal,
dispute, inferred/imported classification, repository-path scope and source
provenance passed. Production writer trust and approval policy remain unchanged;
confidence is source classification, not authenticated human authority.

The existing required entrypoint suite includes `infer_precision.rs`. Its first
execution reached the actual CLI-to-RPC round-2 continuation, then failed a test
expectation that successful `blocked_by` was always an array. The unchanged
envelope omits empty blockers. The fixture accepts omission or an empty array,
and rejects malformed/nonempty blockers. Missing outer task binding must still
fail before recording round 1. Current-candidate CLI proof must be rerun.

Numeric measurements are emitted by existing fixtures: safe protocol call
75.213 ms / 575 outcome bytes / 1490 governance bytes growth; conflict and
revision 58.845 and 43.966 ms, with 1213 and 575 outcome bytes. The two-round
compact journal was 2843 bytes and total governance growth 4017 bytes. The safe
fixture observed zero main-database-file growth. Timings exclude CLI startup,
are samples under concurrent CI load, and carry no portable performance promise.

The only added build input relative to accepted target is the existing
`infer_precision.rs` file in the required entrypoint suite. Reviewed inventory
`sha256:d8781827659ed6d9c4702265a3d159509daf789e067ac13e961434a36d5b6d37`
preserves the profile, accepted executor, required graph and final verdict.
The current task journal was retained canonically; accepted journals remain in
Git history.

Failed or incomplete earlier attempts do not establish readiness: provider quota,
accepted-base/tag movement and release-projection drift remain in trajectory
evidence. Full validation on `dc7bf2fa` returned `SNAPSHOT_NOT_READY` for the
CLI fixture failure in build 8602 after 3427640 ms provider wait; private snapshot
`decapod-snapshot-YH2wgS` preserves that attempt. Passing individual jobs and
checkpoint-history verification never replace the complete exact-candidate gate.
Plain `decapod validate` owns the configured build/tests; no separate manual
build, fabricated proof or weakened check is allowed.

## Historical SDK 0.14 adoption (#1638)

The previous accepted registry dependency was Dactyl 0.14.0, checksum
`d10824bf8882a26b92479f07edb26c1f6fde8da3019f4001befaf28699d7a990`.
Historical 0.12 observations below describe earlier candidates. The existing
release-policy test retains registry-only and feature checks, and now rejects
any locked `quinn-proto` below 0.11.19, even when that dependency is inactive.
Supported Cargo metadata resolution verifies both the repository lock and a
new lock-free registry consumer; it is not compilation or Rust 1.85 execution
evidence. The SDK's own minimum-toolchain proof remains upstream.

Pinned Bazel 9.3.0 `mod deps --lockfile_mode=update` regenerates the actual
crate-universe graph. Its complete JSON is serialized with one-space indentation
only after parsed value equality, preserving the existing bounded discovery
and strict normal `lockfile_mode=error` policy. Nix directly uses Cargo.lock.
The full configured source-CI matrix and independent released-controller
exact-candidate validation remain mandatory; no manual build, fixture result
or metadata operation establishes publication or protected hosted readiness.

## Recovery and accepted-target reconciliation (#1628)

The client continues from both original checkpoints, including
`d85650bafa610dc952fbc91acd91ba5e28b329b7`, with the accepted billing/release
history merged into the same work branch. The newer mandatory project roster,
embedded price and client-built billing URL conflict with the backend-only
contract and are replaced by the managed Dactyl sign-in/account flow. The fresh
headless initialization fixture also asserts that these project fields are absent.

Dactyl's unchanged `86ed55dbf4ff78bb3c6c6aa52832949101d9ea91` candidate has
verified native evidence from run `43131221323`: both mandatory proof groups,
all fourteen checks, a ready verdict and its exact-candidate readiness receipt.
Its wrapper failed afterward during PR publication/readback. This upstream
offline proof does not establish client compilation or protected hosted access.
At that observation the registry exposed only `dactyl-db 0.11.1`. During
continuation Dactyl PR #102 and release #103 merged, publishing `0.12.0` from
`598415953b5ee568f6d42652982897c58b4433cc`. The crate SHA256 was verified as
`b9d8eda3a7bad7490d23dbca5c468bf89b28de4c8b4fd566034b342334ba481e`; its
public API and packaged source match that release. The existing client now
uses its supported Serde/status/code/completion contract. The original stronger
repository-response API is not assumed to exist in the registry release.
On registry candidate `0bff2528d498c7423783656b45a47fde8cd92dbc`, the
independent GitHub diagnostics passed 1,674 library tests (eight ignored),
including all eighteen private-custody unit tests. The consumer integration
suite failed because successful RPC fixtures omitted the released protocol's
required `generated_keys` field, and three human-diagnostic assertions assumed
lowercase status text. Complete successful fixtures now supply an empty key
list; the diagnostic assertions match the released transport classification
and actual status spelling. Existing negative, no-fallback, bounded timeout
and redaction assertions remain required. This diagnostic run was not Buildkite
readiness and had not executed the twelve managed sign-in scenarios. Their
later successful diagnostic execution is recorded below; the current exact
candidate still requires complete native proof.
The released connector classifies a dropped non-timeout authentication response
as `Unavailable`. Completion recovery must retain the saved session and return
`onboarding_pending` for this kind as well as transport/timeout failures; the
existing lost-completion fixture verifies the repeated idempotent confirmation
and the absence of repeated one-time exchanges.
The pending-account scenario exercises the published `entitlement_required`,
`seat_unassigned` and `quote_confirmation_required` reasons individually. Each
must retain the issued session, return action-required, avoid the repository
read and avoid replaying either exchange until the service confirms completion.
The 0.12.0 lockfile also downgrades inactive HTTP/3 dependencies under Dactyl
MSRV bounds; the required dependency audit must pass without exemptions.

The first registry candidate reached actual Buildkite execution. Independent
GitHub diagnostics on that same candidate passed all-feature clippy and
dependency lint, but Bazel setup rejected the stale `MODULE.bazel.lock` before
the Bazel tests could run. The reported recovery, repository-pinned Bazel 9.3.0
`mod deps --lockfile_mode=update`, regenerates the extension's Cargo inputs.
The committed build policy remains `lockfile_mode=error`; no test/build gate is
replaced by this metadata preparation. Fresh Buildkite proof on the current
exact candidate remains required.

The generated lock was 1,053,030 bytes, exceeding bounded discovery by 4,454
bytes. Reformatting its complete JSON with one-space indentation yields
1,029,040 bytes; parsed JSON equality verifies that no value was removed or
changed. Pinned Bazel accepts the result with `lockfile_mode=error`. Source and
runtime discovery limits remain unchanged. This is metadata consistency proof,
not an executed client test or full-readiness result.

The current registry version and checksum are established by Cargo and
`tests/release_dependency_policy.rs`. Existing versioned SQLite recovery
documentation describes the unchanged recovery primitive; those historical
capability references do not establish the managed connector's release or proof.

The first current-target validation stopped before Buildkite because the
immutable candidate still carried v0.138.2 entrypoint headers while the installed
released controller was v0.138.3. Supported initialization regenerates the four
entrypoints and managed image pin; spec refresh updates their projections.
The original journal, reviewed project configuration and required proof coverage
are preserved. This projection repair supplies no compilation or test evidence.

## Managed connector execution prerequisites (#1628)

The managed connector requires a published Dactyl dependency containing its
public API and successful execution of the complete required source checks.
No governed source-overlay recipe is configured for integration diagnostics
before those checks pass. Source review, formatting, native checkpoints and
workbranch pushes do not establish compilation, test execution or readiness.

The registry manifest and lockfile, `tests/release_dependency_policy.rs` and
complete required source profile remain authoritative. Dependency alignment
and package publication remain the separate
[#1380](https://github.com/DecapodLabs/decapod/issues/1380) handoff; no path/Git
fallback, dependency override or relaxed proof gate supplies that evidence.

## Managed cloud client proof and package prerequisite (#1628)

The configured complete source-CI profile remains required. New
`tests/cloud_managed_session.rs` fixtures exercise the actual binary through
headless sign-in, durable continuation, credential refresh, interrupted mutation,
explicit restart and authenticated read readiness. The cloud backend, CLI,
Supabase boundary, TODO resume and init regression suites preserve local
independence and fail-closed cloud behavior. Unit coverage in
`tests/unit/core/cloud_session_tests.rs` exercises private record permissions,
path rejection, exact binding, phase invariants and serialized custody.

These tests must run on the exact checkpointed candidate. Static review and
source formatting do not establish compilation or execution. The managed API
is consumed from the published Dactyl 0.14.0 registry dependency.
The accepted source-CI executor retains its own required recipe. Its diagnostic
record on `ee288c86` showed that a candidate helper edit did not add a new target
to that recipe. The existing required `cloud_supabase_boundary` target now
includes the shared `common/cloud_managed_scenarios.rs` test module, so all
thirteen scenarios execute under the accepted recipe without changing executor
authority. The standalone `cloud_managed_session` entry uses the same scenarios.
Both entries include the private seed helper once at their crate root; this
avoids the `clippy::duplicate_mod` failure observed on `2f01156c` without a lint
exemption. Every existing required command/target remains intact.

That diagnostic run passed 1,674 library tests (eight ignored) and nineteen of
twenty existing boundary tests. Its remaining failure showed that unsupported
`todo done --validated` reached discovery before rejecting unavailable proof
capture. A shared guard now rejects it before connector construction, while
retaining the direct-store guard and the existing no-network/no-mutation test.
The full native validation of that superseded candidate was interrupted; it
does not supply a final readiness verdict.

On `2f01156cfebd9622cc9ae6f43aa80403054a2461`, GitHub consumer job
`130991125949` passed all 1,674 library tests (eight ignored), including all
eighteen custody tests, and all thirty-three boundary tests. The boundary
results explicitly include all twelve managed CLI scenarios and the corrected
proof-before-network case. Other selected cloud/registry integration targets
also passed; the two protected Supabase profiles remained ignored. This is
diagnostic execution, not a full Buildkite readiness receipt. The follow-up
test layout removes only the duplicate seed-helper inclusion (thirty-two
boundary tests) to resolve clippy while retaining every product assertion.
Native validation hit the public metadata reader's rate limit before obtaining
readiness. A fresh complete verdict on the final exact candidate remains
required; neither GitHub success nor a rate-limit failure supplies it.
`tests/release_dependency_policy.rs`, Cargo manifests and source-CI configuration
remain intact; an unpublished source overlay cannot establish release-policy or
publication readiness. No local fixture claims hosted managed Auth, production
entitlement policy, real PostgreSQL, deployment or full remote governance parity.

The legacy protected CLI profiles in `tests/supabase_service.rs` and
`tests/propodus_live.rs` fail explicitly with `MANAGED_CLI_FIXTURE_UNAVAILABLE`
before inspecting fixture credentials or dispatching any CLI/network request.
Their real-service assertions remain intact for a future managed fixture; no
ignored profile is represented as passing. The explicit legacy PropodusClient
API test retains its own service boundary. Account completion response loss
resumes the idempotent confirmation using the same body and bearer; the binary
fixture asserts that one-time token exchange is never replayed.

## Native Laya research reconciliation (#1614)

The historical candidate failed dependency denial on Tokio 1.52.3
(`RUSTSEC-2026-0386`) and lacked a current-source Interfaces review. The
reconciled source retains accepted v0.138.0's Tokio 1.53.3 root lockfile and
sole-Laya kernel ownership. Native task/workspace admission, genuine current
spec reviews, material checkpoints and exact-candidate validation must establish
the new candidate's custody; neither an old result nor copied metadata does so.

An upstream advance invalidates the old publication base even when a previous
candidate has successful source-CI receipts. Current integration preserves the
accepted test-only plugin selection and its material validation contract. Old
receipts retain their original base/head bindings; reconciliation onto the new
accepted base requires native custody, checkpoints and fresh exact-candidate
publication checks. `PUBLICATION_MATERIAL_BASE_CHANGED` is not a test waiver.

The original frozen fitting/evaluation corpus definitions replace unsupported
placeholder families. Their partitions remain disjoint; the evaluation allows
zero false-ready adverse controls and cannot be reused for tuning.
`core::laya_readiness::fitting_cases::frozen_corpora_name_executable_and_disjoint_contract_families`
executes the eight declared bounded Rust witnesses and checks partition
separation. It generates no fitting/evaluation inputs, runs no model and
produces no evaluation scores. The explicit bounded-process signal-module path
supports compilation of shared source by the separate native worker workspace.

The new regression is part of the library unit suite. The actual source paths
select the existing required `supabase-consumer` check, whose second command
runs unfiltered `cargo test --locked --features supabase-cloud --lib` with its
listed integration targets. Execution requires the complete command record and
the named test result. The narrower `selective-core` recipe selected by these
paths runs `entrypoint_correctness`, not this regression. A green job with
`not_applicable`, an ignored test, or compilation alone cannot establish that
the bounded witnesses ran.

The independent `assets/tools/laya-native` workspace remains unqualified.
Root-library CI does not compile, audit or qualify that separate workspace;
its preserved dependency set, including zip 1.3.1, requires its own remediation
and verification before activation. The recorded fitted-worker diagnostic
approved six of twenty adverse controls and rejected four of twelve ready
controls. Those failures remain evidence, the held-out evaluation remains
unscored, and no threshold, numerical tolerance, coefficient bound, model
profile or policy requirement is relaxed. Native compilation, dependency
checks, real backend/protocol execution and a separately justified successful
qualification must precede any activation or readiness claim.

## Historical full-proof completion (#1610)

Required regressions exercise both native and provider history production,
complete proof-plan and material bindings, archive durability, and failure to
persist. A partial, failed, stale or decision-blocked result must not be archived
as successful full proof. Recording history must leave the candidate SHA intact.

`validation_history_archive_tests` exercises the actual pre-validation binding
and canonical append transaction, then removes temporary execution evidence and
looks up the full record by SHA. Native and provider-shaped complete-suite
fixtures must preserve the original task, epoch, plan and results without
changing Git HEAD or tracked governance. These fixtures test archival mechanics;
their observations are not evidence of an external provider run.

Completion tests must show that an original owner can complete a merged task,
or repair its done-but-unverified state, from recorded candidate proof without
launching current validation or borrowing another task's workspace. Adversarial
cases cover wrong task, commit or repository, corrupt evidence, intent edit and
revert, changed lease generation, incomplete task history, unproven integration,
dependency failures and revision races. Failed attempts must preserve task,
proof and journal state. Subsequent QA inspection must preserve this historical
meaning after unrelated work and epoch changes. Publication continues requiring
fresh complete proof and cannot consume this historical completion permission.

Additional ergonomics and private-repository regressions (#1807, #1810) cover:
- Authenticated read-only private-repo accepted-target resolution via mock
  `OfficialCli` `GithubGet` (`validation_history::integration_tests`) and redacted
  error reporting when unauthenticated and authenticated transports are unavailable;
- Pre-capture, custody-preserving lease TTL verification before freezing
  `HistoricalTaskBinding`: default advisory and `strict` modes leave `revision` and
  `lease_generation` untouched, only explicit `renew` rotates custody
  (`validation_history::archive_tests::capture_task_binding_with_budget_never_rotates_custody_unless_renew_is_explicit`;
  end-to-end custody stability across workunit checkpoints is covered by
  `entrypoint_correctness::canonical_workunit_withdraws_superseded_proof_and_recovers_after_lease_renewal`);
- Request-bound pre-broker `todo done --validated` handoff validation and cleanup
  (`plugins::verify::historical_tests::prepared_done_handoff_roundtrips_and_rejects_mismatched_commit_or_request`
  and `tests/validate_ergonomics_1807_1810.rs::todo_done_validated_decouples_broker_and_cleans_handoff`);
- Unverified `done` (`CLAIMED`) self-service recovery via `qa verify regen` without
  self-deadlock (`tests/validate_ergonomics_1807_1810.rs::unverified_done_todo_recovers_via_qa_verify_regen_without_self_deadlock`);
- Unix `RLIMIT_NOFILE` diagnostics in `system doctor check` and `validate_open_file_limits`
  plus automatic soft-limit elevation for spawned harnesses
  (`core::bounded_process::tests`, `plugins::doctor::tests`, and
  `tests/validate_ergonomics_1807_1810.rs::rlimit_nofile_diagnosed_in_doctor_and_raised_for_validate_harnesses`).

The inline Rust regressions are part of the library unit suite selected by the
complete source profile. Their presence and source review are not execution
evidence; the exact candidate still requires configured full validation.

## Immutable push-range ancestry

Ordinary push-range validation uses the previous branch tip as its `base` and
requires that tip to be an ancestor of the exact new push head. The separately
captured accepted target remains part of validation; a protected-master push
can have the accepted target equal to the new head. PR validation retains its
base-ancestry and exact merge-base requirements. See
`src/decapod/core/release_metadata.rs::validation_change_base` and
`tests/release_metadata_policy.rs` for the ordinary-range regressions.

Governed `refs/heads/agent/*/*` pushes use a distinct normalized context.
`.github/actions/ci-context/action.yml` clears `push.before` for this route and
captures the accepted branch declared by the committed `repo.base_branch`.
`assets/tools/ci_event_context.rs::resolve_workbranch` and
`verify_normalized_workbranch` require that captured accepted tip to be an
ancestor of the exact candidate, verify its checkout and complete unmodified
Git history, and recheck the captured ref. The branch namespace selects this
context; it does not itself grant rewrite or publication authority.

Rebasing an existing governed branch requires explicit maintainer authorization,
a retained original tip, and an exact old-head lease for the remote update.
Preserve authentic task custody and the active journal, record fresh native
material checkpoints for every rewritten material commit, and establish fresh
full publication proof against the accepted target. Earlier receipts and
historical completion records retain their original candidate/base bindings;
they do not prove the rebased tree. Divergent accepted targets, missing history,
and stale or fabricated checkpoint evidence still fail validation.

## Version smoke regression

The unchanged Nix workflow (`.github/workflows/nix-flake.yml`) smoke-checks the
built binary with `system version`. The CLI regression
`system_version_does_not_open_or_repair_project_storage` runs that command with
invalid project configuration and datastore modes 0700, 0775 and 0777; metadata
reporting must succeed without creating state or changing permissions. Stateful
commands must continue rejecting unsafe storage. In `tests/trajectory_cli.rs`,
`local_command_announces_a_detected_version_transition_once` checks that version
reporting leaves the old release counter untouched, then a normal project
command emits the pending migration notice exactly once. Full validation must
execute the source tests; these contracts alone do not prove Nix packaging or
post-merge reproducibility.

## Post-merge Gatling test-budget recovery (#1596)

The `Tests (Full)` job in GitHub Actions run `42744631477` ended with a
Bazel `//:gatling` TIMEOUT after 300.1 seconds, while executing the complete
50-test CLI integration suite with `--test-threads=4`. This is evidence of an
insufficient aggregate CI deadline, not of a specific assertion failure or a
successful run. The previous five-minute `moderate` budget did not establish
whether the remaining tests pass.

`.config/build/decapod.BUILD.bzl` now gives only `tests/gatling.rs` the
existing Bazel `long` class (a bounded 900-second ceiling), already used for
two other integration-heavy targets. The test inventory, assertions,
per-process deadlines, four-thread setting, nonzero exit behavior and required
CI checks are unchanged. A hang or failed assertion must still fail CI.

The concurrent `Deploy Docs` failure in run `42744631501` was GitHub Pages
remaining `deployment_queued` before an API deployment error. A later
unchanged-source deployment succeeded, so no speculative Pages workflow or
permission changes belong in this repair. Treat a new failure as independent
evidence requiring its own diagnosis.

Acceptance requires an exact-head, v0.136.1-governed candidate with material
checkpoint proof and the configured required CI graph; a declaration of a
larger timeout does not by itself prove the full test suite passes. The
post-merge `Tests (Full)` run is the decisive 900-second regression signal.

## macOS bounded-reader recovery (#1571)

The accepted v0.133.2 before-fix regression
`core::validation_epoch::input::tests::bounded_epoch_preserves_ordinary_epoch_digest`
failed with `ENOTDIR`. Descriptor-relative macOS enumeration must make the same
bounded and ordinary digests agree while retaining symlink/FIFO rejection and
all input bounds. The macOS iterator regressions also require independent cursors,
no pathname substitution, safe ownership after dropping the original handle, and
correct non-following file/directory/link classification. These are local input
safety checks, not evidence of cloud, hosted CI or publication readiness.

## Claim boundary delivery checks

The required CLI target runs the two `claim_boundaries` regressions with
independent caller environments and a shared worktree datastore. Their results
do not replace standalone validation: the tracked agent entrypoints and managed
Dockerfile must also match the evaluating release. A release refresh must use
the CLI and be committed, so CI does not repair those inputs during proof.

## Real recovery regression

The authorized private Rust fixture reproduced #1473 with two completed tasks:
full scoped validation passed, but fresh per-execution evidence identities made
the second snapshot hash differ. Unit tests in `verify_tests.rs` compare repeated
report identities while rejecting material/policy/verdict drift. The separate
opt-in pending-batch test in `tests/support/live_recovery.rs` preserves the real
failed attempt and retries without deleting evidence, pruning or reopening.
It requires per-task verification and final unscoped publication readiness;
compilation or an ignored test remains insufficient acceptance.

## Documented agent bootstrap execution

The always-selected `cli-contracts` proof executes the inline
`documented_bootstrap` module in `tests/cli_contract_enforcement.rs`, which
interprets the generated
`AGENTS.md` initialization commands through real CLI subprocesses. Human
provisioning creates a committed local-backend project with container workspaces
disabled. The fixture sets `DECAPOD_CLAIM_AUTORUN=false` because claiming
otherwise attempts an optional container launch independently of that project
setting. This disables no claim lease, orientation, or proof gate. It asserts
that container image storage was not initialized and explicitly closes its
temporary directory so cleanup failures fail the test. Installation, remote
backends and Docker execution are outside this fixture's scope. The agent acquires its own session, initializes context, creates
and claims a task, creates a governed workspace, enters the returned path, and
orients before any implementation file is written.

The fixture removes inherited Decapod and Git environment overrides and CI mode
flags rather than setting Git-gate bypasses. Orientation must fail with the
worktree-specific diagnostic from the main checkout both before and after
workspace creation. Successful orientation must retain the created task's intent
and governed-plan binding in the actual worktree. Implementation then changes
only that worktree, with the main checkout's HEAD and tracked work unchanged.
The module is part of the target source itself, so Bazel cannot omit an
undeclared support file. Unlike the selective CLI matrix, this proof also runs
on test-only changes. This runtime regression complements the four-entrypoint text-order checks in
`src/decapod/core/assets.rs`; neither substitutes for exact-candidate validation.

## Native readiness receipt custody

`workspace::ensure_required_governance_artifacts_in_pr` requires current plan,
claims and trajectory material but not the legacy tracked
`sections.validation` preparation receipt. Requiring that output before native
proof is circular, and writing it into the candidate after proof would stale
the proof itself. `decapod govern project-check` retains the explicit legacy
preparation writer; inventory continues to report its actual integrity and
freshness rather than laundering absent or stale receipts into success.
Malformed configuration and missing required governance still fail.

Native execution success is published as one atomic completion directory only
after its receipt and verdict have been written and synced in private staging.
The runtime writes its compact historical proof before that publication point.
The outer full-readiness boundary then appends that
proof to the canonical history; failure of that append revokes returned
publication readiness while preserving the execution evidence. Failed execution, stale candidate/target
or blocked decision produces no success receipt; receipt or verdict creation,
partial-write or sync failure leaves no published success bundle and revokes
returned readiness. Pending staging files are never completion evidence. No checkout or
Git metadata is changed by receipt production. Regression coverage in
`tests/consumer_readiness.rs` uses actual native source execution to assert
binding, candidate immutability and absence on failed/stale proof;
`tests/unit/core/workspace_publication_tests.rs` preserves material gates and
legacy integrity diagnostics. Runtime unit regressions inject creation,
partial-write and sync failures for each completion artifact and assert absence
of a successful verdict/receipt pair, plus preservation of an existing bundle. These new regressions await the official
configured validation of this candidate; their presence is not a passing result.

## Validation Overview

Decapod validates the exact candidate against declared intent, accepted policy,
a bounded required proof profile and verified execution custody. The agent owns
repair and revalidation; human review retains merge and release authority.
Buildkite evidence is required when that provider is configured. GitHub mirror
results, prepared prerequisites and replay fixtures do not independently grant
publication readiness.

Optional Laya assessment follows successful deterministic proof and fresh material
bindings. Disabled mode executes no inference. Required mode retains declared
obligations in the bounded `validation-results.v3` projection and requires a
valid affirmative response under the pinned manifest. Failed proof, oversized
inputs, missing runtime, stale bindings, rejection and abstention cannot become
publication permission. Four captured projection hashes remain compatibility
vectors; they establish serialized input preservation, not model accuracy.

Canonical proof encodings, event identities and historical Git commits are not
rewritten. Removed research profiles have no current acceptance obligation and
no historical failed result is relabeled as success. External model and hosted
service profiles are executed only under their explicit fixture requirements;
unavailable or ignored profiles are not passed evidence.

Publication reviews bind current Architecture, Interfaces and Security content
to the current code fingerprint. `updated` requires a semantic change from the
accepted base; otherwise an honest `unchanged-with-reason` review is required.
Full validation and checkpoint inspection remain separate checks.

## Native sequence verification

`tests/native_proof_execution.rs` exercises real compiled Rust commands sharing
scratch artifacts, later-step nonzero exit, cumulative output limits, one original
deadline, cancellation and tool substitution between steps. It also checks that
unbound successors, shell executables and more than 64 commands fail before
execution. Earlier subprocess output remains available when a later preparation
fails. These tests are focused adapter checks, not proof that every required
platform, workflow or release archive executed. Full publication still requires
the prescribed exact-candidate `decapod validate` result and all required evidence.
On macOS, canonical `TMPDIR=/private/tmp` keeps fixture paths within the strict
owned-snapshot contract. The short process-group cleanup fixture warms first-use
Mach-O loading separately; its original 100 ms descendant deadline is unchanged.

## Runtime recovery proof obligations

The durability slice requires adversarial proof at real domain boundaries:
transaction failure must roll back task state and journal together; duplicate
identical operations are idempotent; conflicting IDs, predecessor gaps, corrupt
snapshots, generation regressions and divergent live state fail closed. Modern
claim, renew, yield, handoff, release, dependency and proof state must round-trip
without changing verification bytes or their task-revision binding. A legacy
baseline must visibly preserve its incomplete-history limitation.

Workunit tests exercise concurrent read-modify-write, stale prepared generations,
interruption between durable append and JSON publication, repeated recovery, and
current-source/intent/approval drift at resume. Atomic-file tests inject directory
sync failure after rename and verify that a complete file remains visible while
the operation reports unconfirmed durability. Unix tests reject symlinked parent
sync targets. These are required checks, not claims that arbitrary hardware
power-loss behavior or every external runtime has been measured.

## Evidence Artifacts

Evidence must identify the input state, evaluator, executed check, result, and
relevant artifact. Plans, checkpoint records, provider probabilities, wrapper
success, and an agent saying “done” cannot substitute for execution evidence.

- `passed` means the identified check has supporting successful evidence
- `failed` means a check ran and did not satisfy its condition
- `partial` means only part of the required proof is available
- `unavailable` means the required evidence could not be obtained
- `no_checks_run` is a computed absence of proof, with an unsupported completion
  verdict; it is not a passing check input

A required check that is missing, empty, duplicate, stale, running, canceled,
failed, or skipped cannot establish readiness. Non-applicability must be derived
from the current immutable source range and reviewed policy, independently of a
provider's skipped result. Mocked or simulated stages retain that disposition.
Required check invocations must actually execute. Reuse of compiled artifacts
follows the declared, source-bound proof contract.

Verification loops require non-empty proof references for `grader_result: pass`
and non-empty feedback for `grader_result: fail`. Loop result `pass`/`fail` is
separate from lifecycle status `passed`/`failed`. Retry attempts remain contiguous,
and feedback is bounded. Failed historical attempts remain visible when new
successful evidence replaces their blocking condition.

The integration inventory adapter preserves a failed supervised process status
and bounded, escaped stdout and stderr excerpts. An executable-launch failure
remains a failure; the diagnostic path neither retries it nor changes file
permissions. This keeps operating-system details available for the agent to
diagnose before proposing a repair (`assets/tools/integration_shards.rs`).

A provider-declared new ordinary branch at the accepted target has no candidate
delta. Its context verifier still checks complete history, exact commit/tree,
clean source and the captured target. The constitution workflow runs the same
Ajv schema recipe, rechecks that identity around execution, and emits no PR
readiness receipt. Fixtures cover creation metadata, SHA-1/SHA-256 identity,
changed and unrelated heads, hidden/staged source, missing bases and workflow
failure propagation. Those fixtures do not establish a live release event result.

## Agent steering

Validation reports identify this phase, the agent as operator, the entry command,
and the next repair or review action. Reconciliation assigns missing build, test
and CI configuration to the agent and names validation as the resume command.
The agent follows those actions within the accepted intent and authority. A
decision gate or unavailable authorization/access remains a human-input boundary.

Configured candidate builds and downstream consumers belong inside the selected
validation route. Required consumers must be bound to the exact authenticated
producer artifact; missing, empty, duplicate or substituted inputs fail. The
command-execution fixtures prove ordering and failure propagation with controlled
tools. Only actual applicable execution and provider evidence can establish
readiness for the product candidate.

## Governance invariants

The agent contract requires prompt evaluation before repository/tool actions.
The agent must respect the result and stop for human review when blocked. The
validator cannot certify every prior action taken through an external tool.

Validation checks repository evidence for these product contracts:

1. Human intent, the agent's recorded interpretation, task binding, allowed scope,
   unresolved questions, and proof hooks remain consistent enough to execute
2. Mutations use an active session and the correct owned workspace. Root files,
   the root index, other task workspaces, and unowned resources remain untouched
3. Required approvals and decision gates remain binding. Advisory observations,
   inferred intent, and missing coordination state cannot satisfy them
4. Plans, current claims, trajectories, receipts, and unresolved obligations
   remain loadable and preserve their custody across restarts and handoffs
5. Specifications reflect the agent's current interpretation; generated metadata
   is current and never replaces substantive authored content
6. Completion and publication bind their proof to the actual candidate and active
   policy, rather than a historical receipt or a different source tree
7. Execution and recovery remain bounded, with owned subprocesses terminated and
   reaped and uncertainty preserved on timeout or interruption

The validation worker subprocess (`__project-check-worker`) reports through
anonymous pipes. To prevent pipe deadlock when worker output exceeds the
system pipe capacity (#1526), the parent drains stdout and stderr on
dedicated threads while polling for child exit, so the worker never blocks
on a full pipe while the parent waits for termination. Reader threads are
joined after child exit for complete output, and joined on timeout so no
reader is left running.

`TODO_PROJECTION_MISSING` is a coordination blocker with
`execution_ready: false`, not lost request meaning. Its recovery must prove the
intended task, agent ownership, lease, and control plane before execution resumes.
A storage error must not be relabeled as an absent projection.

`tests/unit/core/task_transfer_tests.rs` exercises two separate local stores and
real Git repositories: identity/intent/history preservation, source yield and
actor checks, source seal rejection of later custody mutations, exact private
artifact recovery, replay/conflicting-history refusal, repository/receiver/commit
binding, malformed/truncated journal rejection, explicit operator acceptance,
digest/byte bounds, and shared/connected-task rejection. Historical passed proof
must remain only in provenance, with unavailable current proof and a new revision.
These library regressions run through the existing required Supabase library-test
recipe. Their presence is not passing evidence until normal validation executes
them. Synthetic stores cannot prove source authenticity or global exclusivity;
the trust contract requires the operator's authenticated source handoff.

Transfer regressions use synthetic task identities only. Candidate transfer
commands cannot authorize custody of the real feature task before this controller
is independently accepted and released. The unchanged accepted source executor
and complete configured proof graph remain responsible for compiling and testing
the candidate; refreshing a reviewed inventory declaration supplies no proof.
Compiling the production binary alone does not execute the transfer regressions.
The required library-test build must also compile their real repository fixtures
and run them; fixture compilation failure leaves the transfer claim unsupported.

## Obligation current-evidence regression proof

`tests/plugins/obligation.rs` covers transitive and diamond completion without
status-cache writes; uncached passing proofs; poisoned or stale `VERIFIED`
cache entries; newer failure and expiry; malformed requirement arrays; dangling
and orphan edges; cycles; deep chains; atomic add failures; and public CLI JSON
plus failure exit codes. These tests run in the existing required plugins suite.
`tests/unit/core/validate_tests.rs` checks completion attempts and legacy `met`
projections while preserving unrelated open backlog.

HealthEngine integration and unit tests exercise numeric chronology, tied
outcome permutations, conservative tied expiry, invalid timestamps/results/SLAs,
missing claims/tables, alias ambiguity, exact-ID precedence, storage-error
propagation, and the cache-free connection helper. They remain in the required
plugins and library suites. Source inspection and a reproduction against a
released binary describe the defect; only execution against the current
candidate establishes that the repair passes. Plain full `decapod validate`
owns the configured build and test evidence before publication.

## Promotion Gates

Full `decapod validate` outside CI proves the exact current workbranch against
its accepted target and complete applicable profile. It binds source, governance,
reviewed CI/release definitions, toolchain, and execution identities. Changes to
those inputs or the accepted target invalidate prior readiness.

Workbranch pushes and iteration may precede validation. Creating a PR, including
a draft, requires converged intent, mergeability, and successful full validation.
A focused test, passing `govern project-check`, empty proof registry, or green
provider summary is insufficient. The publisher invokes the full boundary
independently before its combined push-and-PR operation.

The source-readiness profile is finite and reviewed. Its source inventory,
required platform/matrix instances, and selected integration shards must match
the current repository. Missing adapters, unknown repositories/profiles, altered
workflow contracts, unproved exclusions, or self-dependent validation fail
explicitly. Profile declarations are reviewed policy and cannot be silently
updated to match whatever evidence happens to exist.

After source and required governance material are ready, the agent checkpoints
and commits the candidate, then invokes plain validation. Configured build
prerequisites and tests belong inside that route. Canonical agent templates in
`src/decapod/core/assets.rs` forbid a separate manual build or shadow preflight.
An unreviewed inventory requires reconciliation. A reviewed inventory can change candidate helper inputs while the remote
adapter uses its independently supported accepted executor. An unsupported
carrier, protocol or accepted executor instead reports
`CONTROLLER_CONTRACT_UNSUPPORTED` and requires an independently supported
controller. The current runtime does not automatically build or promote a new
controller to resolve that incompatibility. Neither outcome grants readiness.

The current core profile reports `source_pr_readiness`. Release-only
four-platform packaging is outside that profile. A passing source report makes
no packaging or delivery claim. Publishing and deployment simulations must be
labeled; they are not actual uploads, tags, releases, or deployments.

## Local and remote execution scopes

`govern project-check` executes bounded project/governance prerequisites. It is
useful during bootstrap and inside finite recipes, but grants no publication
readiness on its own.

Inside GitHub Actions or Buildkite, public CLI, alias, and RPC validation run
real bounded CI-local checks. Reports state `scope=ci_local` and
`publication_ready=false`. They do not write a full-readiness receipt, recursively
submit another suite, or accept snapshot transport arguments. Provider environment
flags alone cannot authorize publication.

Native execution, optional Buildkite execution, and independently accepted GitHub
Actions execution enforce the same complete `decapod.pr-readiness.v1` required
profile (`readiness_profile::required_source_catalog_for_provider`). The canonical
`proof.validation_buildkite_preflights` and `proof.validation_github_actions_preflights`
settings (mutually exclusive) select the remote carrier when matched on the accepted
target. Remote carriers execute prerequisites remotely and report local project
checks as `not_run_provider_owned`. The Buildkite route pushes or reuses the exact
existing workspace-branch commit and collects authenticated execution artifacts;
GitHub Actions results neither satisfy nor block a Buildkite-selected profile, and
repository-local `repo.pr-readiness.v1` / selected-root profiles reject both remote
preflight flags (`REPOSITORY_PROFILE_PROVIDER_UNSUPPORTED`,
`SELECTED_REQUIREMENTS_PROFILE_UNSUPPORTED`).

The checked workspace-publication path binds its push request to the exact
repository, workspace, candidate and branch. This validation-owned request does
not require a separate human operator receipt. Generic external writes retain
their approval requirements, and validation still enforces the configured
executable allowlist and actual provider access. Altering the sealed command or
using another ref cannot acquire this validation-specific authorization.

Remote proof must authenticate the expected event, build, attempt, logical job,
artifact producer, raw bytes, and reviewed execution recipe. Once a PR exists,
independent PR/manual runs for the same head do not replace required push-event
execution. Duplicate matching runs and changed attempt bindings are errors.
Imported runtime identity and native provider identity remain distinct; a receipt
must not invent native fields the importer cannot expose.

Collection uses bounded authorized reads, a shared deadline, cancellation,
explicit capture limits, and final uncached identity checks. Failed or denied
transfers cannot yield partial-batch proof or authorize another route. Timing
reports are diagnostics, not evidence of correctness; overlapping request work
must not be summed as wall time. Fixture collectors do not prove live provider
acceptance.

Independent build envelopes and each build's envelope, steps, and jobs inventory
are fetched in ordered batches through the same sealed public reader. The
collection adapter forwards those batches instead of serializing them. Every
live metadata read remains fresh, including the final complete run-set recheck;
batching changes transport scheduling, not which checks establish readiness.
An interrupted controller has no final verdict, even when provider jobs passed.
No transport timing or process heartbeat can replace the completed report.

## Authority, specifications, and proof currency

The specs manifest binds both project authority inputs, `config.toml` and
`OVERRIDE.md`, as well as supported release-bound projections. Validation checks
actual entrypoint content, declared fingerprints, compiled expectations, and the
managed Dockerfile pin. Inheriting intact current material is permitted; every
commit need not manufacture a change to every governed path.

The full readiness controller independently checks two acceptance invariants in
`validation_runtime::load_project` before execution selection and again during
final discovery. `plan_governance::enforce_scope_paths` applies declared forbidden
path prefixes and file-touch budgets to the complete accepted-base/candidate Git
delta. Deleted paths and both sides of renames count; a clean worktree and passing
compilation do not waive scope. Optional execution/project checks additionally
include staged, unstaged, and untracked paths, preserve NUL-framed filenames, and
fail closed when scope evidence cannot be read. These are explicit path/budget
constraints, not an automated interpretation of arbitrary natural-language intent.

`entrypoint_integrity::validate_committed_history` reuses immutable Git history
and canonical template validation for every feature-branch entrypoint rewrite.
Restoring canonical final bytes cannot conceal an earlier unauthorized rewrite.
The full controller calls this check independently of CI-local Git-gate skip
flags, on native and provider routes; a provider's green health check alone is
insufficient. Existing accepted-template lineage and installed-binary authority
remain unchanged.

`PUBLICATION_BUNDLE_CURRENCY` proves required HEAD participation and loadability.
Sibling gates separately prove fingerprint integrity, authored material change,
epoch currency, and receipt binding. Presence alone is insufficient.

Refresh runs only in the supported isolated workspace and preserves authored
content. Supported surfaces are `rpc --op specs.refresh`, validation refresh,
and workspace creation; fresh initialization may scaffold absent specifications.
A protected-root refresh reports `workspace_required` and leaves existing root
material untouched. `workspace status` never refreshes. Malformed generated
boundaries that risk prose loss report `SPEC_REFRESH_AUTHORED_CONTENT_LOSS` before
any spec or manifest write.

A governed source change requires a material authored living-spec update against
the PR base. Attestations, capability declarations, and capability overlays are
excluded from the material comparison. Fingerprint-only change fails with
`FINGERPRINT_ONLY_SPECS`. Not every spec must change, but affected contracts must
be accurate. Release-only treatment depends on strict immutable change
classification, never a label or branch name.

`STALE_CONFIG_INPUT_HASH`, `OUT_OF_SYNC_SPECS`, and `STALE_SPECS_FINGERPRINT`
identify projection/authority drift and require supported refresh and revalidation.
`STALE_VALIDATION_EVIDENCE` identifies a different active validation epoch. Prior
evidence is preserved; the agent commits the complete prior governance journal
before initializing a replacement run and obtaining fresh proof, rather than
relabeling an old receipt. Uncommitted replacement fails with
`TRAJECTORY_UNARCHIVED_WORK`; continuing the current run remains available. Same-epoch validation is idempotent.

Epoch bindings cover plan/review, claims, authored spec digests, entrypoints, and
managed Dockerfile while excluding the receipt itself. A proof-only receipt commit
must not invalidate its own inputs. Checkpoints bind staged material and semantic
inputs and are verified through the exact head, including merges. A later
checkpoint cannot retroactively prove an earlier unchecked commit.

## Regression Guardrails

CI source selection uses a complete immutable event/base/head context. Required
contexts reject missing, partial, malformed, unresolvable, or checkout-inconsistent
evidence, including a missing accepted-base ref. Base ancestry and synthetic
merge parent/tree identity must be verified. Provider context recognition is
scoped to the actual checkout and its owned validation clone so inherited
variables cannot classify unrelated test repositories.

Release-only classification permits an advancing root package version, its
matching root lockfile version, and optional changelog content. Source,
dependency, feature, registry, checksum, file-mode, add/delete, or mixed-range
changes retain normal checks. Version-only releases still compile the candidate
and execute actual validation. A narrow dependency-metadata projection may
normalize only the matching local root version; malformed, duplicate,
mismatched, or registry-sourced root records fail closed. Original Cargo inputs
remain authoritative for product version and release classification.

Proof execution must preserve its bound checkout, index, and non-ignored source
material. A stable HEAD is insufficient. Drift checks reject staged-only changes
and staged changes hidden by restoring the worktree. Any permitted generated
release drift must be reproduced byte-for-byte by the actual projection generator
from accepted inputs, without normalizing authored prose or unrelated surfaces.

Provider governance checks use a newly owned private clone with exact Git
identity and full history. A supported merge-checkout normalization must verify
ordered accepted-base/authored-head parents and exact head-tree parity, then run
strict checkpoint/material verification. It must not mutate the provider checkout
or ignore source/private-clone races.

## Evidence Matrix

Required test selection comes from reviewed project policy. These proof areas
must remain covered as implementations evolve:

Public-provider timeout tests exercise the production retry decision with
controlled process results and waits. They distinguish the three-attempt limit
from exhaustion of the shared deadline, while real execution retains its bounded
process capture, backoff, cancellation, and authorization checks. Scheduler load
must not decide which independent bound a unit test expects to observe.

- Interfaces: prompt evaluation, CLI/RPC envelopes, typed failures, bootstrap
  availability, schema compatibility, and accurate agent documentation
- Intent and context: plan interlocks, task/scope binding, override precedence,
  malformed authority rejection, deterministic capsules, and evidence portability
- Coordination: exclusive claims, conditional conflicts, event-atomic mutation,
  retries, competing clients, crash replay, and committed-claim/container follow-up
- Isolation: dirty-root preservation, correct canonical store resolution,
  branch-local governance, destination collisions, symlink rejection, invocation
  ownership, and safe cleanup after interruption
- Persistence: explicit IDs, transaction rollback, read-only enforcement,
  close/reopen durability, idempotent event migration, replay determinism,
  integrity diagnostics, bounded locking, backup, and operator-directed recovery
- Specifications: material-change detection, release-bound integrity, authored
  prose preservation, idempotent refresh, config drift, and epoch invalidation
- Publication and completion: full range/checkpoint verification, remote head/base
  and PR-file verification, partial/unknown outcomes, idempotent retry, current
  workunit proof, and rejection of stale or omitted living-spec references
- Proof execution: complete check inventories, real exits/logs/artifacts,
  toolchain custody, deadlines/cancellation, unchanged source, adversarial provider
  bindings, missing or duplicate evidence, and independently proved exclusions

Task-proof revision regressions exercise current binding, missing/malformed
bindings, changed completed intent, dependency claim/completion denial, aggregate
selection, observational comments, custody edits, and conservative legacy replay.
Verification fixtures prove post-transition capture, current-revision recapture,
stale pass/failure rejection, prune ABA protection, and transaction rollback
for result/recovery/prune events. Batch recovery must not partly rewrite proof
when any captured task or baseline changes. Final aggregate failure, a stale
second target, an event-insertion failure, or drift in output/material/epoch
must leave the whole recovery batch pending. Temporary admission exemptions must
restore their previous value after success, errors, and unwinding. These live in the existing selected
unit/plugin test targets; fixture validation callbacks are test-only and cannot
manufacture production completion evidence.

Integration preparation builds selected executables once; workers consume those
artifacts without compiling. Shards must form the exact selected inventory with
no overlap, use executable libtest names, preserve normal ignored-test behavior,
and propagate every child failure. The repository's selective integration steps
retain their 300-second execution budget. Raising a timeout, omitting a test, or
passing one isolated case does not prove the complete suite fits that budget.

The real-CLI workunit recovery cases live in
`tests/support/cli_workunit_recovery.rs`, included by the required `cli_contracts`
target. The selective CLI matrix still executes that target alongside
`cli_contract_enforcement` and `entrypoint_correctness`; no recovery assertion or
target is excluded. This balances the aggregate real-process suites without
extending their 300-second budgets. A successful contracts job alone does not
prove the complete selective CLI matrix passed.
The enforcement target also runs exclusively with respect to sibling Bazel test
targets: its real CLI/validation processes repeatedly passed alone but exhausted
the same 300-second budget under sibling load. Serialization preserves all cases,
commands and finite per-target limits; current execution must verify the repair.

The `group_broker` Bazel target likewise runs exclusively relative to sibling
targets. CI recorded its final two burst clients reaching the production
120-second request deadline under concurrent target load, after earlier clients
completed sequentially. Scheduling isolation preserves both 20-client burst
tests, durable-result assertions, uncertain-outcome/replay checks and all
production deadlines. This configuration change requires fresh complete core
suite execution; an isolated historical pass alone does not establish readiness.

Source-owned shell, checkout, setup, release, and dependency-policy regressions
must exercise the actual authored helper or workflow body. Tool setup verifies
pinned official bytes and exact version output before adding tools to PATH.
Regular-file release staging preserves executable bytes. Tests do not publish
real packages merely to establish routing or archive policy.

## Validation Rules System

Decapod enforces governance through a unified, rule-driven validation system that
extends the Constitution and OVERRIDE.md authority model. Rules translate
applicable principles into mechanically checkable requirements.

### Rule Registry and Precedence

The rule registry resolves an effective rule set from three tiers:

1. **Built-in rules** (Constitution authority): Embedded default rules supplied by
   Decapod covering scope, custody, evidence, and workunit governance.
2. **Project overrides** (OVERRIDE.md authority): Project-specific rule
   customizations in `.decapod/OVERRIDE.md` under the `

## VALIDATION_RULES Overrides
   section. Overrides can tighten severity, add constraints, or configure
   evaluator parameters within declared override eligibility.
3. **Task constraints** (Governed plan authority): Task-specific constraints from
   the governed plan (forbidden paths, file-touch budgets) automatically become
   rules with TaskConstraints authority.

Precedence: Kernel invariants (NonOverridable) > Task constraints > Project
overrides > Built-in defaults. TightenOnly rules can only increase severity or
add constraints.

### Built-in Validation Rules

| Rule ID | Authority | Severity | Evaluator | Description |
|---------|-----------|----------|-----------|-------------|
| `decapod.scope.authorized_paths` | Constitution | Error | scope.validator | All changed files must be within authorized scope defined in governed plan |
| `decapod.scope.protected_paths` | Constitution | Critical | gatekeeper | Protected paths in config.toml and governed plan must not be modified |
| `decapod.custody.task_ownership` | KernelInvariant | Critical | workspace | All work must be in claimed task workspace with valid ownership |
| `decapod.custody.workspace_isolation` | KernelInvariant | Critical | workspace | Changes isolated to task workspace; root repo must not be modified |
| `decapod.evidence.required_and_fresh` | Constitution | Error | evidence.validator | Required evidence artifacts must exist and be fresh (max age configurable) |
| `decapod.evidence.proof_backed_completion` | KernelInvariant | Critical | workunit | Task completion requires passing proof-plan gates with valid evidence |
| `decapod.scope.file_touch_budget` | Constitution | Error | scope.validator | Changed file count must not exceed declared file-touch budget |

### Rule Evaluation and Verdicts

Each rule evaluates to one of three verdicts:

- **PASS**: Required check executed successfully and established the specified condition.
- **FAIL**: Evaluated condition was violated.
- **UNVERIFIABLE**: Condition could not be established due to missing evidence, inputs,
  or evaluator capability. UNVERIFIABLE never counts as PASS.

FAIL and mandatory UNVERIFIABLE verdicts block governed completion and publication
transitions. Every blocking result includes a structured remediation contract with:
- Stable error code
- Rule identifier and applicable authority
- Actual violation and expected condition
- Affected paths or governance entities
- Required remediation steps
- Whether autonomous remediation is permitted
- Whether human authorization is required
- Evidence needed for revalidation

### Override Integration

Project-specific rules are declared in `.decapod/OVERRIDE.md` under a dedicated

## VALIDATION_RULES Overrides section. Each rule override uses a fenced block
with the rule ID as heading:

```markdown
### decapod.scope.authorized_paths
````markdown
severity: error
applicability:
  required_changed_paths: ["src/**"]
  forbidden_changed_paths: ["**/secrets/**"]
evidence_max_age_seconds: 43200
````
```

Overrides are validated for:
- Unknown rule IDs (fail closed)
- Duplicate definitions (fail closed)
- Malformed definitions (fail closed)
- Unauthorized weakening of kernel invariants (rejected)
- Override eligibility violations (TightenOnly, NonOverridable enforced)

### Applicability and Context Awareness

Rules declare applicability conditions (required/forbidden paths, task types, Git
refs, CI/local-only). The registry only evaluates applicable rules. Custody rules
(task_ownership, workspace_isolation) only apply to governed agent workspaces
with active task claims. They do not apply to:
- Detached/post-merge validation contexts
- Consumer repository validation
- Release/post-merge validation contexts
- Fixture/test validation contexts

### Remediation and Recovery

Every blocking result provides actionable remediation:
- **Autonomous**: Agent can repair (e.g., revert out-of-scope changes)
- **Authorization required**: Human approval needed (e.g., protected paths, scope expansion)
- **Evidence production**: Agent must produce fresh evidence (e.g., re-run validation)

Agents must follow the remediation contract and revalidate. Repeated evaluation of
unchanged inputs must not invent progress. Kernel invariants cannot be waived by
candidate-controlled policy changes.

### Integration with Validation Pipeline

The validation rules gate runs as part of `decapod validate` after override
authority resolution and before project specs validation. It evaluates all
applicable rules against the current candidate state, evidence artifacts, and
governance context. Results are included in the validation report with
policy fingerprint and validation epoch binding.

## Proof Surfaces

Process tests and container mocks cannot establish behavior against a Docker
daemon. Actual interrupted-container recovery requires a provisioned runtime and
checks receipt-bound cleanup while preserving unrelated files and containers.
Absent runtime evidence remains unavailable.

Container boundary regressions in `tests/unit/plugins/container_tests.rs` exercise
real temporary Git repositories for non-fast-forward rejection, checked-out-host
fast-forward behavior, hidden index flags, stash/side-branch/tag preservation and
unchanged inherited-ref cleanup. Side-effect spies exercise the production
finalization helper's failure, publication, final-foldback and push ordering.
A real common-publisher call rejects a clone missing governance before network
publication; target mismatches also fail closed. These tests do not claim a live
Docker run or a real external PR. Existing full publisher/readiness contracts
remain independently required. Full `decapod validate` executes the prescribed
suite; authored tests and static inspection alone are not passing receipts.

SQLite, Neon, Supabase HTTP fixtures, actual PostgreSQL transactions, and hosted
Propodus authorization are separate proof scopes. Cloud fixtures cannot establish
server-side tenant isolation, deployment membership policy, or event atomicity.
Missing host SQLite runtime is an explicit availability failure; it does not
activate a bundled fallback driver.

Cache regression coverage additionally requires one download across separately
verified single-link installations, corruption and unsafe-hardlink rejection,
cancelled-copy cleanup and retry, no-clobber publication, and failed installation
preserving the previous active identity and reusable cache. Init coverage checks
both entry points, read-only dry-run, idempotent migration of known retired
selectors, extension preservation and rejection of ambiguous provider settings.
The init policy selector additionally needs both command surfaces, explicit true
and false, preservation on omitted flags, read-only dry-run, unchanged authored
pins/extensions and proof that selection performs no inference or model download.
Portable execution needs real CPU inference and bounded accelerator-failure
recovery, platform-specific device selection, exact model/runtime binding after
fallback, and independent numerical checks for every claimed backend. Declared
support, a CPU enum variant or mocked accelerator errors do not prove that the
pinned backbone executes on CPU or CUDA.
The CPU/GPU worker source and v2 hardware-only retry protocol are now authored.
The scalar CPU recurrence fixture uses hand-calculated decayed state and query
scaling, and RoPE fixtures independently constrain zero-position/tail behavior.
These are not checkpoint parity or independently reviewed reference evidence.
The parent classification tests must reject retries for any answer (including
negative/abstaining answers), inference errors, malformed messages and CPU retries.
Init-refresh coverage also requires omitted portable defaults and an empty model
pin to remain omitted from legacy TOML; explicitly authored backend/fallback
settings must survive refresh. All current source changes need executed CI, while
native build/probe/inference remains a separate missing validation recipe.
The worker manifest scopes `parking_lot` to Metal and optional CUDA builds;
CPU-only targets do not need GPU loader synchronization. Dependency lint must
recognize this target/feature boundary without adding an unused-dependency waiver.
Local-binding regressions must exercise independent per-maintainer paths for one
shared model pin, private permissions, malformed/oversized/wrong-identity records,
failed registration preserving prior bytes, unsafe executable links/formats and
cancellation, and frozen-custody rejection before model access. Native-format
fixtures are never executed and provide no Metal/CPU inference evidence. Recheck
local custody after the independent deterministic freshness check; changed runtime
or assets must invalidate the model acceptance without relabelling passed source
proof as failed model inference.

Cross-machine custody is established separately from execution evidence. The
released v0.118.0 task-transfer importer preserves the original task identity and
source journal as provenance, but its successful import and fresh local claim
do not carry forward validation or native results. The Mac continuation must
regenerate complete candidate proof and record its actual Metal, independent
parity and readiness-calibration results before publication.

The standalone CI recipe regenerates release-bound entrypoints, the managed
Dockerfile and the spec manifest with its evaluating binary, then requires a
clean tracked projection. Commit supported regeneration after a controller
upgrade; passing project checks alongside projection drift is still failure.

Synthetic fixtures and source review are not native acceptance. Required measured
evidence remains Apple-arm64 compilation, genuine Metal numerical dispatch, real
pinned checkpoint loading, independent reference agreement through pooling,
engine and calibrated decision output, readiness-domain calibration evaluation,
latency/peak memory and repeated-request cleanup. Exercise 4,096/4,097-token
boundaries. The private numerical export records observations with parity
unverified and supplies no fabricated threshold or pass. No compatible native
executor or independent reference outputs are currently established for this work.
Ordinary governed full validation is still required; activation's one-time
bootstrap permission does not authorize feature builds/tests or publication.

### Native execution boundary

The smallest legitimate controller prerequisite is an independently accepted
and released supplemental host-proof recipe that retains the complete source-CI
profile. It must bind the exact candidate and accepted target, worker manifest
and lock, measured Rust/Xcode/Metal toolchain, immutable verified model inputs,
bounded child execution and private proof output. It must execute the separate
decoder tests, actual synchronized Metal probe and pinned-model requests, preserve
failed attempts and refuse completion when either source proof or native proof
is missing. An unmerged candidate must not authorize this new recipe. Hardware
availability alone does not close this execution-contract gap.

### Readiness evaluation protocol

Freeze the calibration and model/runtime pins before evaluating. The initial
acceptance criterion is zero false-ready outcomes, with a one-sided 95% exact
binomial upper bound at most 5% for false-ready among accepted decisions. With
zero errors this requires at least 59 independent accepted evaluation cases;
zero observations never establishes safety. Require at least 50% nonabstaining
coverage overall and at least 50% acceptance of independently labelled ready
cases, so rejecting or abstaining on everything cannot pass. These engineering
criteria do not reinterpret upstream conformal levels as readiness guarantees.

Evaluation must use real, independently adjudicated task/PR snapshots with
explicit requirements and validation evidence as available at the decision time.
Labels must be fixed without looking at this implementation's outputs. Include
ready cases and passing tests with unmet explicit requirements, incomplete work,
contradictions, stale/missing evidence and ambiguous cases requiring abstention.
Group related task revisions into one split; keep the evaluation holdout apart
from any fitting or threshold selection and record corpus provenance, split
digest, label adjudication and deduplication. No readiness fitting or real holdout
evaluation has yet occurred. If fitting becomes necessary, pin it separately and
evaluate on a new untouched holdout; do not silently replace upstream calibration.

Report total and per-category sample counts, false-ready counts and denominators,
coverage, abstention, malformed/error outcomes and confidence intervals. Separate
deterministic gate refusals from model assessments. Representative sampling and
independent-case assumptions must be disclosed; a convenience corpus cannot
prove population risk or out-of-distribution coverage. Synthetic fixtures only
test arithmetic and fail-closed gates. Independent parity additionally needs
authenticated pinned inputs and token/span, raw block 13/19, pooled feature,
normalization, engine and calibrated-output reference values, or a separately
reviewed Rust reference with stage-specific tolerances fixed before comparison.

Platform proof must state where it executed. Native, emulated, and cross-compiled
results are distinct. Packaging checks must build and run the promised artifact
on the applicable native or explicitly emulated target; evaluating metadata or
selecting an archive is insufficient. Unrun release targets remain unproven.

## Recovery and final acceptance

Validation failures leave the task open. Follow the report's supported repair,
preserve failed evidence, and rerun against the exact candidate. Stop for human
judgment when a decision is required, access is unavailable, authority conflicts,
or remediation is unsupported. Never weaken the profile, fabricate green checks,
edit machine-owned evidence, or use a local receipt to replace missing remote
proof.

Audit abandonment retains an unknown original mutation outcome. Integrity probes
do not authorize database repair, and failed runtime inspection does not authorize
resource deletion. Timeout diagnostics identify the active stage when available;
no universal short wall-time claim covers a full provider suite.

Final acceptance requires current complete evidence, satisfied execution and
publication gates, verified remote publication when requested, and proof-bound
completion. Report the exact scope and unresolved limitations. A successful
check of this specification, or a previous candidate, makes no claim that a later
candidate has passed.

## Release mutation ownership

Release ownership regression fixtures embed the workflow as a declared Bazel
compile input, so sandboxed test compilation and execution inspect the same
reviewed configuration. Synthetic checkout fixtures bind their own executor
root explicitly, including when a checkout-less shard inherits a different
provider root. A subprocess regression verifies this isolation without mutating
other tests' process environment. Release-policy assertions follow the actual
executor-aware entrypoint and its preparation/execution context propagation.

The release workflow (`.github/workflows/release.yml`) selects native GitHub
Actions as its sole mutation owner in a credential-free job. Ownership requires
`GITHUB_ACTIONS=true` and the protected `RUNNER_ENVIRONMENT=github-hosted` value
used by its `ubuntu-latest` owner job, with no Buildkite host markers. The
importer also exposes GitHub-compatible variables, so absent Buildkite variables
alone cannot establish ownership. Its sanitized `self-hosted` identity, missing
or unknown runner identity, and conflicting provider markers must all be rejected
by the exact-script regression. Future native self-hosted release runners require
an explicitly reviewed policy change. Every release job requires that positive
ownership result, including planning, artifact building,
container publication, hosting and announcement. Imported Buildkite execution
therefore skips those jobs before their ordinary secret retrieval. These skips
are not release or publication evidence. Provider resolver capability checks can
still run before the condition is evaluated.

Native pushes retain the release-plz action's release-PR creation followed by
release behavior. A manual dispatch may explicitly select `release_pr_only` to
create or update the release PR without invoking package publication; the default
remains the normal release sequence. This code does not repair a provider that
never receives a postmerge event. Missing external trigger delivery remains a
separate operational blocker, and validation does not authorize distribution.

## Intent Reconciliation Acceptance Boundary

Repeated hosted HTTP504 failures fetching pinned Bazel release archives are
repaired by `.config/build/downloader.cfg`: try Bazel's official mirror, then
the same official GitHub release URL with an explicit download query, retaining
the original URL as a fallback. `rules_cc`, `bazel_features`, `rules_shell`,
`protobuf` and `rules_java` mirror archives and `rules_rust` and `supply-chain`
query-route archives were verified against upstream pins. Version selection, integrity
checks, compiler requirements, source execution and the complete test graph
remain unchanged; fresh exact-candidate validation must exercise the repair.

The accepted v0.133.0 source inventory differs from the last reviewed profile
only by the package version in `Cargo.toml` and `Cargo.lock`. The explicit
required-profile review declaration is reconciled to that complete inventory;
the finite workflow graph, adapters, provider selection and proof requirements
remain unchanged. This review pin is configuration, not execution evidence.

## Acceptance observer capture boundaries

Official Decapod 0.153.1 reproduced a 175-event real capture and its truncated 173-event prefix both reporting observed_autonomous_admission=true with no findings. The added mandatory offline controls cover missing genuine closing events, legacy unknown coverage, dropped events, missing channels, index substitution, inconsistent windows, shared custody, missing agent end and failed/incomplete exit. Existing source/base drift coverage still preserves historical admission. GitHub Actions owns source verification for this delivery as requested by the maintainer. The published launch media is tabled.

## Matching release projections

The initial hosted standalone validation compiled the observer but rejected tracked generated drift from evaluating source version 0.153.2 with the previous 0.153.1 projection binary. The candidate now refreshes supported projections with the matching registry-installed 0.153.2 binary. The change preserves required checks and source policy.

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
The read-only reconciliation report must detect missing/orphaned/duplicate
requirement references, missing proof-spec mappings, stale code fingerprints,
and unresolved human questions. A matching manifest or reference is not
semantic proof; `intent.reconcile` cannot satisfy validation obligations,
approve spec changes, or authorize publication. Unit tests must exercise
these negative cases and preserve existing proof and publication gates.

## Immutable Impact Negative Acceptance

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
Unit and RPC tests must preserve existing empty-params compatibility while
proving exact-commit ancestry, deterministic NUL-safe add/modify/delete/rename
classification, authored-spec versus generated-projection distinctions, and
bounded uncertainty on malformed, non-UTF8 or excessive paths. A code change
without reviewed authored specs remains observable and explicitly unmapped.
Human-intent questions, requirement markers, stable hashes and changed-source
inventory are advisory: none may satisfy a proof gate or change
`semantic_alignment: not_proven`.

## Native inventory qualification

The required CLI suite includes seven inventory regressions: UTF-8/reuse and
redaction; sampled/budget-limited coverage; ordinary DDL and generated columns;
trigger/view binding without DDL disclosure; checkpoints outside the ordinary
event prefix; missing-store CLI/direct/stdin RPC without initialization; and
bounded framing before startup. The registry dependency/version/checksum guard
remains enabled and is advanced here to exact registry Dactyl v0.14.0. Read-only live captures use a CI-produced
binary with database trees mounted read-only and only existing locks writable.
Those captures bind their actual historical source/binary identity; they cannot
establish publication readiness for the current candidate.

The candidate preserves accepted managed-cloud, terminal onboarding,
first-proof admission, native inventory, Dactyl 0.14.0 and bounded reflection
changes through v0.141.0 at 3718f70e, with the four release-bound entrypoints,
managed image pin, and specification manifest regenerated by the installed
published 0.141.0 controller. Canonical spec reviews follow the final source
and authored-spec edits. Complete exact-candidate Buildkite proof and current
accepted-target verification remain required before publication; earlier passing
stages, GitHub diagnostics and checkpoint-history checks cannot replace them.

The CLI contract target runs exclusively in Bazel. Two hosted attempts of the
previous 74-case candidate exhausted its 300-second deadline during the final
existing workunit-recovery case after all seven inventory regressions passed;
the same suite previously completed in 252 seconds. Rust's single test thread
did not isolate real CLI fixtures from the concurrent entrypoint target.
Exclusive scheduling let that complete suite pass in 293.7 seconds, leaving
insufficient margin under the previous aggregate ceiling. The expanded target
therefore uses Bazel's existing finite 900-second class, like the other large
CLI suites. Every case, assertion and individual operation deadline remains
required. Fresh execution must establish complete current-suite success;
earlier partial runs do not prove it.

### Validation-owned candidate execution

For the supported remote source-CI profile, plain validation captures the
accepted executor contract before selecting the provider. Candidate helper
changes do not require a separate local compile or a shadow preflight when the
accepted R closure and unchanged carrier are supported by the controller.
Validation pushes or reuses the same governed work branch, requires the full
candidate build and applicable test graph, authenticates their artifacts, and
returns its own final report. Build failure, missing custody, altered R/H
identity, or a moved accepted target leaves readiness false.

The source executor’s dual-root preparation.v2 and execution.v3 envelopes bind
R, its tree and profile closure separately from the actual subject revision and
tree. Their schema is not authorization: the importer constructs expectations
from independent source capture and requires matching final closure. Legacy
preparation.v1/execution.v2 cannot carry an ignored dual-root field. Authenticated
integration workers retain their artifact-only mode and inherit the verified R
executor through the measured preparation dependency. No candidate receipt can
choose the expected recipe, required suite or readiness outcome.

The protocol is finite and provider-specific. Changed workflow/local-action
semantics, new providers and unsupported accepted executor contracts require a
reviewed controller adapter. Native validation retains its existing strict
contract. Initial rollout uses the strict legacy path until the accepted target
contains this protocol; automatic replacement of an already installed old
controller is not claimed.

The Linux source-policy regression also exercises a real, dependency-free Cargo
fixture through the unchanged production golden-vector recipe. Its accepted Git
executor R and edited candidate H are separate clean worktrees. R compiles H,
runs its assertions and executes the resulting ELF binary; the consumer then
applies the full preparation policy and verifies the actual artifact and logs.
Compile errors, failing assertions, changed executor material, and tampered
artifacts or receipts must be rejected. The fixture uses the installed declared
Rust toolchain and bounded child-process capture, with no successful skip path.
Fixture setup generates the dependency-free lockfile offline before the accepted
R commit, so compilation must leave the complete tracked candidate unchanged.

This regression runs in the required Supabase library-test recipe. It proves
local runner and independent custody behavior, not provider authentication or a
complete hosted dual-root rollout. Provider authentication remains covered by
its own checks. A hosted opt-in proof requires a subsequent ordinary work branch
whose accepted target already contains the protocol, followed by full plain
validation against that branch; release dispatch is unnecessary.

## Consumer local-proof regressions

The repository-local profile is tested in fresh non-Cargo Git repositories using
actual source-inspecting native commands, including success and nonzero failure.
Contract tests cover unknown IDs, stale workflow and command pins, full mapping,
requiredness, strict-profile downgrade rejection and immutable execution/target
boundaries. The native profile does not attest hosted CI or Nix daemon/deployment
behavior. Init regressions distinguish top-level and `init with`, preserve external
tracker/proof choices across forced refreshes, and verify generated workflow event
context and nonshallow history. Full Decapod validation still owns this project's
build/test/source-CI proof and publication gate.

The required `cli_contracts` target includes the consumer-readiness and both init
regression modules explicitly, with their source/compile-data inputs declared in
the Bazel target. Standalone Cargo test discovery is not evidence that the finite
source-CI graph executed a new suite. Provider logs must show those module tests
passing, alongside a separate genuine public CLI acceptance run.

Security rejection assertions inspect the complete structured validation failure
list, not the compact human summary's first issue. A placeholder-spec failure
must not hide or substitute for the specific protected-path traversal rejection.

## Reviewed importer compatibility maintenance

Importer upgrades require independent official source and executable-member
review, followed by negative tests for old, observed-but-unreviewed, unknown,
mixed and platform-swapped identities. The fixed reviewed tuple is 0.102.7.
A provider's own successful output cannot authorize its compiler bytes. When a
released controller cannot import newer provider evidence, that failed result
remains failed; a changed source pin alone does not repair the running binary.
Any explicit engineering selection of a replacement controller must independently
verify its accepted source-CI executor, exact candidate identity and downloaded
artifact digest, then rerun complete readiness. This is not automatic controller
adoption or acceptance of candidate-produced JSON as a proof verdict. No separate
local prerequisite build or reduced check graph is introduced.

## CI-attested activation regression coverage

The finite provider adapter is checked against projected public metadata from
Buildkite builds 1840 (successful commit `c444ff9b03ae569a268f348c898f0dccfa08b5e4`)
and 1870 (failed commit `bcb5842ff5d1dba6a8697b1cffe5deb1d135f701`). Fixture
provenance distinguishes retained identity/status fields from redacted command
content. Required keys originate in the reviewed workflow/catalog mapping, never
from whichever observed jobs passed. Tests must exercise the ordinary three-read
collector, branch-dependent concurrency identities, exactly one keyless bootstrap,
negative source/run/job mutations, and checked-in policy mapping completeness.
Only metadata routes are permitted by the fixture reader; no archives or logs are
used as evidence for this policy.

The Gatling completion regression exercises the public CLI's rejection of
`todo done --validated` when its minimal fixture lacks the mandatory full proof
profile, retaining the open task and unchanged revision. Existing completion-unit
fixtures separately cover successful prepared proof capture and atomic status
commit. The ignored positive public-CLI full-readiness fixture remains an explicit
coverage gap, not a passing acceptance test. Completion/runtime edits additionally
select Gatling so future changes cannot silently omit this regression.

## Separate CPU executable regression obligations

The CUDA-loader correction has authored, unexecuted regressions for strict
model-free capability envelopes, rejected outcome injection, contradictory
availability/error pairs, wrong backend/schema, malformed and oversized errors,
CPU-pin changes before model access, and atomic private binding retention after
failed CPU registration. Setup rejects an orphan CPU option before cache/network
access. Existing unsafe-hardlink and inference-negative/abstention retry rejection
coverage remains required. Rustfmt and whitespace checks pass; these are not
executed Rust tests. Actual loader fallback, native worker compilation, Metal
probe/model inference, independent backbone parity and readiness holdout proof
remain outstanding and must run through the accepted released controller.

## Supplemental Rust worker verification

Generic configured Rust prerequisites must execute through the bounded physical
adapter with locked inputs and recorded outcomes. A failure stops before Laya
assessment. This repository has no mandatory external Rust worker recipe.
Optional inference, numerical qualification and hosted authorization need their
own actual evidence; source tests do not establish them.

## Immutable CI and version-only release command proof (#1397)

`ci_event_context_policy` preserves its existing accepted-carrier tests, including
regular committed configuration custody. Its original local run exposed the
legacy carrier's unavailable `tomllib` on the host interpreter (52 passed, one
failed). That failure is preserved, not marked passed by the experimental Rust
parser's diagnostic run. The carrier migration is not part of this candidate;
complete hosted checks must execute under the existing declared CI environment.
`ci_checkout_policy` retains real private-checkout/containment and rustup-dispatch
coverage; owned macOS fixture paths use canonical filesystem identities rather
than weakening containment assertions.

Material-spec report notes distinguish `outcome=passed` with
`proof=authored_change` or `proof=version_only_exact_projections` from explicit
local `outcome=skipped` reasons. These notes report the executed gate branch;
they do not promote a project-check result into publication readiness. The
whole-command regression inspects this JSON record and the non-ready scope rather
than guessing proof execution from an aggregate green count or absent failure text.

## Released native acceptance boundary

The opt-in observer `tests/acceptance/native_readiness.py` invokes an externally
installed released CLI on agent-prepared governed native branches. Unlike the
runtime fixture, it cannot replace prerequisites or accepted-target transport.
It requires a successful unchanged-candidate external receipt/verdict pair,
actual failed native execution without success publication, and rejection of a
subsequently dirty candidate. It is not a provider or publication authority and
does not change plan acceptance or configured proof. Its failed-native and dirty
controls are distinct from runtime in-flight target-race tests. See
`docs/native-readiness-acceptance.md` for prerequisites and the outstanding real
execution requirement; authored acceptance code alone does not close #1474.

## File-claim reconciliation with native acceptance

The file-scope candidate preserves the accepted native-readiness and live-recovery
acceptance suites from PR 1499 alongside the bounded claim regressions. Explicit
path scopes still require ordinary category, dependency, lease and trust gates;
they neither replace native execution evidence nor establish isolated-worktree
custody. The 0.123.7 release changes the discovered build inventory only through
the accepted Cargo package version. The reviewed inventory declaration is measured
after this merge, before spec refresh; required proof checks are unchanged.

The earlier 0.123.4 source-CI run failed release-bound projection drift. Regenerating
the entrypoints, managed Dockerfile and manifest with official 0.123.7 repairs
those generated inputs but is not a passing test result. This merged candidate
requires fresh exact-head execution evidence. Trust administration remains outside
this PR and unresolved in issue 1480.

## Historical recovery regression contract

The unit history assessor and strict verifier share the same per-commit logic.
Negative fixtures cover multiple missing commits, late tip checkpoints, changed
parent evidence, malformed governance, unavailable refs and self-asserted
exception declarations. The real `workbranch_governance` CLI fixture creates
two actors through separate sessions and tasks in actual linked worktrees; it
reproduces missing history and `SPEC_REVIEW_REQUIRED`, follows the documented
recovery/review commands, and verifies the fresh publication material while
preserving original commits/custody. It denies foreign-checkout mutations and
resolution of an unarchived observation. Passing this gate is not full native,
Buildkite, hosted security or cloud-profile proof. Full exact-candidate validation
remains mandatory, and unsupported exception/maintenance capabilities stay absent.

### Workbranch fixture admission

`tests/workbranch_governance.rs` exercises custody through the actual actor-bound CLI. A manual Git worktree does not become authoring-admissible merely because `agent.init` observes it: `workspace ensure` must verify and bind the checkout first. A session release deliberately clears task assignments, so resumption requires a fresh task claim and authoring admission. Negative fixtures assert that observation preserves inherited governance and that foreign or nonexistent branch-task bindings cannot be admitted. These preconditions preserve the existing custody contract; passing fixtures do not replace independent exact-candidate publication proof.

The existing Laya post-proof judgment remains distinct from deterministic source evidence. This governance recovery change introduces no worker-runtime migration or new model-proof contract.

## Datastore revision 2 proof profiles

`tests/datastore_efficiency.rs` checks nonempty history preservation, additive
uniqueness without renumbering, unsafe-definition rejection, rollback, repeated
initialization and 64 competing event writers. Its ignored measurement case is
explicitly invoked for physical byte/WAL/tail evidence; it is not a timing
assertion in normal CI. Migration and event unit suites supplement these checks.
Disposable native PostgreSQL baseline evidence does not qualify hosted Supabase,
64 exclusive claimants or complete client/backend parity. Publication still
requires the full exact-candidate profile. The v0.134.0 proof-source inventory differs from the previously reviewed v0.133.2 inventory only in accepted Cargo package versions; workflow, action, helper and executor definitions are unchanged.

## Current assessment verification boundary

Laya is the sole post-validation model contract. Deterministic source, migration,
state-commit golden vectors and custody checks remain required. Retired model
research artifacts are removed rather than reclassified as Laya evidence.
Private runtime registration version 2 rejects older bindings explicitly and
allows re-registration without changing model or proof hashes.

## Reviewed journal repair, validation exceptions, and replay stability (#1581–#1583)

A task journal may contain multiple independently identified, uncheckpointed
domain-event suffixes. A repair request can name all reviewed task IDs in one
invocation, with a bounded human-authored reason. Before any acceptance is
written, the implementation must confirm that each selected ID has a
checkpointed predecessor and an uncheckpointed domain-event range, that no
selected ID repeats, and that no unselected task has an uncheckpointed suffix or
divergent live projection. One canonical database transaction records an
acceptance event and a new transition checkpoint for each selected task.
Failure anywhere in preflight or write must preserve the prior event count
and all prior checkpoint heads. No historical event is deleted or synthesized.

Exceptional repair and optional local-validation bypass require distinct
permissions in the **main repository** configuration. Roles and actor labels
are exact and default-denied. The authorization reader must reject an
unresolvable root, missing policy, symlinks, non-regular policy files,
oversized content, invalid TOML and unauthorized actors. A policy digest
binds the authorizing configuration to the audit event. Local actor labels
are cooperative claims and must not be represented as authenticated people.
Ordinary validation remains mandatory. An explicitly permitted bypass records
its reason, reports `bypassed` and leaves both proof readiness and publication
readiness false; it cannot mint accepted verification or native proof.

Verification self-execution must accept the canonical identity of the
currently running Decapod executable, including Nix's `.decapod-wrapped`,
but reject arbitrary executables even when named `decapod-*`.
Readiness replay normalization may mask per-run `request_id` and
`binding.evidence_digest` for each recognized readiness receipt, including
Laya. It must retain material input revisions, model and governance policy
bindings, mode, decision and blockers in the digest. The default Laya mode is
disabled; required mode needs explicit reviewed configuration and cannot
be disabled in violation of accepted policy. Two identical executions must
hash identically; changed material decisions must not.

Regression coverage exercises batch atomicity, unrelated-gap rejection,
wrong-actor denial, Nix self-identity and lookalikes, CLI exception
requirements, disabled Laya defaults, and replay identity stability.
These test definitions are falsifiable requirements; successful execution
is established only by CI receipts for the exact candidate commit.

## Release alignment and projection fixture scope

PR #1589 integrates accepted v0.134.2 journal/RBAC changes and preserves the active workstream. Entrypoints, managed image pin and spec attestations are regenerated by the installed published release. The production-projector replay test supplies a clearly synthetic artifact digest rather than submitting a large debug test binary to the registered-runtime verifier. Its four previously captured input hashes remain the compatibility oracle. Real model/runtime registration retains the existing 512 MiB bound and custody checks; replay vectors do not establish executable registration or model inference.

## CI fixture execution and review dispositions

Lifecycle tests execute their temporary shell runtime only after a bounded, side-effect-free listing proves the fixture inode has no open writers. The probe retries Linux `ETXTBSY` only, for at most two seconds; other failures remain fatal. A Linux regression holds an actual write descriptor, verifies the kernel refusal, then releases it and requires successful execution. Production container mutation and error handling are unchanged. Publication reviews classify SECURITY.md as unchanged relative to the accepted PR base when its semantic content is identical, regardless of accepted upstream additions; current code fingerprints still bind that explicit review.

The socket lifecycle fixture likewise permits a bounded `AddrInUse` interval after closing its own listeners: a parallel fork may retain the descriptor until exec. Its live-peer refusal and replacement-owner assertions still run first, and stale cleanup must succeed within two seconds. Production stale-socket removal continues refusing live peers; no retry policy is added to production operations.

Supabase HTTP fixtures explicitly restore blocking mode on accepted connections before applying the existing five-second read timeout. This is necessary when the platform inherits the nonblocking listener flag. A delayed-header regression verifies that the fixture waits for a valid request instead of failing immediately with `WouldBlock`; production HTTP transport is unchanged.

The delayed-header regression uses the approved Tokio networking API with a five-second overall deadline, while the fixture server deliberately uses blocking reads under its existing read timeout. No lint-policy exception or production networking change is required.

## Local relational compaction proof

`tests/datastore_relational_compaction.rs` proves historical index removal
preserves nonempty multi-table row digests and uniqueness, is idempotent,
rejects redefined historical indexes, atomically rolls back when a required
UNIQUE replacement is missing, and stays compact after subsystem
reinitialization. Any row/hash change, index recreation, partial mutation,
or incorrect upgrade-success claim falsifies completion. Full independent
candidate validation and CI remain required before publishing or merging.

## Aptitude initialization preserves existing prompt history

The existing `plugins_aptitude_tests` target in `tests/plugins/aptitude.rs`
exercises the public `initialize_aptitude_db` entrypoint against disposable
SQLite stores. Repeated initialization must preserve every persisted prompt
field, including stable IDs, disabled defaults, user-adjusted priorities,
usage counters, timestamps, custom prompts and historical duplicate defaults.
Equality of the complete ordered row snapshots is required; unchanged row count
alone cannot establish preservation.

When a default is absent, initialization must restore exactly its logical
`(context, prompt_text)` identity and authored priority, even if a custom prompt
already occupies that context. The restored row receives a fresh physical ID,
starts active with zero usage, and remains unchanged on the next initialization.
Every previously present row must remain identical. The two regressions are
included in the existing required `selective-plugins` source-CI target;
their presence is a test obligation, not an executed passing result. They add
no runtime seeding behavior, concurrency or hosted-storage qualification,
performance measurement, or publication-readiness claim.

Changes under `tests/plugins/` select that same complete plugin recipe, including
`//:plugins_aptitude_tests`, even when production source is unchanged.
`tests/source_ci_profile_policy.rs` checks the test-only selection, required job
inventory, identical production/test recipes, and preservation of unrelated-path
and release-only exclusions. A green `not_applicable` result cannot establish
that the prompt-history regressions executed.

## Datastore production qualification (v0.137.0 review)

`docs/datastore-v2/production-readiness-v0.137.0.md` records source findings
and open acceptance requirements, not newly executed proof. Qualification must
bind complete effective SQLite and native PostgreSQL catalogs to their reviewed
logical contract. Historical inventory provenance remains distinct from the
current package version, and legacy migration receipts cannot acquire invented
implementation attestations during an upgrade.

Acceptance requires actual released-client format compatibility, interrupted
migration recovery, revision and claim-generation fencing, mandatory atomic
mutation/event pairing on every retained route, and uncertain-commit retry
reconciliation. Bounded pagination and journal replay must preserve complete
history, canonical bytes and a coherent snapshot beyond the current aggregate
limits. The shared scenario corpus must exercise governance transitions and
proof bindings through both SQLite and the published native RPC client.

Existing compaction fixtures, compatibility-service mocks and earlier proof
ledger entries retain their stated scope. They cannot establish native cloud
deployment, hosted capacity, backup restoration, or this candidate's publication
readiness. Unavailable checks remain unavailable. The review records both the earlier unadmitted publication and the current
governed reconstruction. No missing validation receipt is claimed as passed,
and this documentation cannot weaken the configured runtime proof profile.

## Agent documentation index consistency

The Documentation Index in `docs/agent/api-index.md` is the canonical agent
document list, including the API index itself. The Markdown document links in
`docs/agent/README.md` and the agent section of `docs/agent/llms.txt` must name the
same files exactly once. The human book retains its separate section; a matching
basename under another directory does not satisfy an agent document link.

`tests/contract_conformance.rs` compares both indexes with the canonical table and
requires every indexed document to resolve through `assets::get_embedded_doc`.
The required `contracts` source-CI profile executes `//:contract_conformance`,
including this invariant. Its compile-time inputs are the three indexes; the
test calls the public embedded-document lookup for corpus membership. The
existing specification-refresh tests retain their independent coverage.
The Bazel test inputs include the inspected indexes, and the constitution
compression inputs include `repository-readiness.md`, so sandboxed builds can
embed that contract. This regression verifies documentation discovery and
packaging. It does not replace exact-candidate governance, runtime or publication
proof, and adding the regression does not establish that it has executed.

The real-libtest inventory fixture in `tests/integration_sharding_policy.rs`
copies its executable in a child process and waits for that process to finish
before invoking the supervisor. Parallel fixture processes must not inherit an
open writable descriptor for the relocated binary. The inventory still executes
the real copied libtest binary and checks its discovered test names; its separate
exit-126 regression continues to require immediate failure with captured
diagnostics and no retry.

The v0.138.1 rebase preserves the accepted historical-completion and Laya
research contracts while replaying this documentation and fixture repair onto
the current accepted target. The original task journal, earlier checkpoints and
archived pre-rebase tip retain their original provenance. Native APIs must
refresh release-bound projections, review the changed Bazel source inventory,
and record a distinct checkpoint for the replayed material. Earlier successful
v0.137.4 execution remains historical evidence; the rebased commit requires
fresh complete publication proof and execution of the documentation-index and
real-executable regressions against the captured accepted target.

## Onboarding integration proof boundaries

The required `selective-cli` profile executes `//:cli_contracts`. Its Unix-only
`terminal_onboarding` module contains the six PTY cases previously declared in
standalone `tests/init_cli.rs`, which the finite profile did not select. Cases
cover recommended setup, edit/cancel, terminal restoration, preview, redirected
output and cloud review cancellation without a service connection. The fixture
converts `TIOCSCTTY` to the platform ioctl request type, preserving its value
across the macOS constant/parameter type difference. The key test exercises an
arrow sequence followed by lone Escape with and without a controlling terminal.
When macOS revokes the slave at child
exit, the fixture reads the retained master and still asserts restoration of
canonical mode and echo.
Managed-session CLI cases separately cover authoritative access and recovery;
neither fixture suite establishes a hosted purchase or paid customer journey.

The required broker tests exercise an actual basic-trust archive denial.
They check the task's persisted status and revision, the typed non-commit
receipt, and same-ID replay across broker lifetimes with a single execution.
The broker crash test continues to check genuine uncertainty separately.

The reviewed source declaration for the 0.139.0 reconciliation measures all 105
build/workflow/helper inputs against accepted master19acd21. Every input matches
that accepted tree; only the package version in Cargo.toml and Cargo.lock changed
from the preceding accepted cloud mergec87e4f49. Renewing the inventory digest
retains the same source adapter, accepted executor, required graph and verdict
policy. A matching inventory is preparation, and full exact-candidate validation
remains mandatory after every source or target change.

## Validation proof lifecycle

Issue #1629 separates proof admission from proof consumption. A fresh governed docs task must reach its first reviewed source command through full `decapod validate`, without supplying an earlier receipt or an undeclared workunit. Terminal publication rules consume the live runtime's verified complete suite before readiness receipts or historical completion are written. Full candidate/accepted-target/material/governance bindings remain required. Explicit workunit contracts retain journal, proof-artifact, source, current epoch, living-spec and capsule lineage checks; an absent projection cannot erase a durable declaration. Regression coverage must observe real command execution, reject failed commands without readiness/receipts, and retain unknown, forged, stale and mismatched evidence failures. Component fixtures may substitute only accepted-target transport, not source checks or execution observations.

Review regressions cover expiry during terminal evaluation and native receipt staging, a distinct mandatory project workunit obligation without a declaration, and a declared current workunit reaching the actual rule evaluator with a generated capsule, bound living spec, current execution journal and retained proof bytes. Explicit epoch mismatches, corrupted capsule/source and deleted manifests remain failures.

The fresh governed docs regression has its actual `#[test]` entrypoint in the already required `tests/entrypoint_correctness.rs` target, calling `tests/support/fresh_docs_readiness.rs` with helper-only `tests/common/consumer_readiness_fixture.rs`. Real CLI commands create and claim the docs task, ensure its managed workspace, approve its plan, refresh/review specs, checkpoint and commit. The public runtime's prerequisite callback runs the real CLI with explicit `ci_local` scope and requires observed success, nonempty passed checks, zero failures, no publication readiness and an unchanged clean candidate. The same runtime invocation then executes the complete real source suite and terminal rules; only accepted-target transport is substituted with a fresh read of the actual local bare Git upstream. This is CLI-admission plus complete-runtime coverage, not an end-to-end CLI HTTPS transport test. The unchanged selective-CLI recipe executes the same entrypoint, CLI-contract and contract-enforcement targets. All 16 preexisting consumer API cases remain in `consumer_readiness.rs` under CLI contracts; helper support contains no test cases. The genuine fresh docs case runs once in the entrypoint target, preventing the measured 65-second case from overloading CLI contracts. A green `not_applicable` selection is not execution evidence.

The fixture starts without a prior prerequisite receipt or ignored policy cache and verifies clean committed candidates throughout. Before the first proof executes, `tests/support/fresh_docs_readiness.rs` exercises the public CLI `decapod todo done --validated --commit` path to confirm premature proof-backed completion is rejected without mutating task completion state or candidate cleanliness. After the positive first `docs_...` proof passes under canonical `task_type = Some("docs")` and `git_ref = Some("refs/heads/...")` without a workunit, the same fixture initializes and checkpoints an unverified `govern workunit` proof plan via the public CLI, verifies `govern workunit transition --to verified` fails without passing gates, and confirms subsequent runtime validation blocks readiness and withholds both `completion/native-readiness-receipt.json` and `historical-proof.json`. Unit regressions in `src/decapod/core/validation_rules.rs` separately cover `normalize_rule_task_type`, `canonical_git_ref`, `EvaluationPhase::Publication` `task_types`/`ref_patterns` filtering, and checkpointed unverified workunits failing `decapod.evidence.proof_backed_completion` (`UNVERIFIED_COMPLETION`). Canonical scaffold regressions ensure an absent runtime cache causes no source healing or manifest rewrite, while a missing tracked living spec still triggers repair. Existing policy validation continues rejecting malformed or unsupported policy contracts. CLI fixture calls have finite 30-second limits, additionally capped by the runtime's original remaining budget during admission; the accepted aggregate CI budget remains unchanged.

The actual accepted 0.139.0 release at `19acd21f` changes only the Decapod package versions in inventoried `Cargo.toml` and `Cargo.lock` relative to its cloud-client parent. Its 105-file source inventory is reviewed as `sha256:c8786efce425085ccb62996e1d586dd8dabd491adea74af4829e638ec26d6174`; the pre-extraction candidate inherits those source definitions exactly. Updating the declaration records that measured accepted release delta; it grants no execution or publication evidence and changes no controller, executor, workflow, helper or source-selection contract.

Shared fixture extraction adds only declared Rust source inputs for the existing Bazel consumer and entrypoint targets. At placement commit `cff3a3c3`, the reviewed 105-file inventory hashes to `sha256:50c5532ad4dca740a8216688204d2c0446459da2782a66de9377aacb30d360c8`; `.config/build/decapod.BUILD.bzl` is the sole changed definition relative to then-accepted `19acd21f`. The accepted executor, three selected target names, commands, serial test threads, finite budgets, selectors, workflows and compiled carriers remain unchanged. Before/after named-case inventories preserve every existing case and the single fresh docs case; actual configured CI must establish execution after this placement change.

The accepted target subsequently advanced through onboarding PR #1636 and the actual 0.139.1 release, ending at `bc0a71971bce8ee036a24562b7fc4a9b771f2f40`. Reconciliation retains incoming onboarding source, tests and whole-product specifications, the existing validation lifecycle work and every prior native checkpoint. All 140 carried baseline obligations are unchanged; the incoming current journal declares no additional claim or unresolved decision. Source inventory changes since the placement commit are exactly the accepted `Cargo.toml`/`Cargo.lock` package-version bytes. The actual 105-file candidate inventory is reviewed as `sha256:31fa30d0bbc53f76f4f09cbca80101965b71dc1c00c337b54c53607ab3df4a40`; its only definition difference from the accepted target is the already reviewed additive Rust-input wiring. In that reconciliation, independent controller, compiled carrier, required graph, selectors and finite budgets remained unchanged.

Standalone source validation compares the repository after native `validate --refresh-specs` and rejects release-bound drift. The actual 0.139.1 evaluator exposed 0.139.0 entrypoint, managed-image and manifest pins after the accepted release advanced; those projections must be regenerated by an independently released 0.139.1 controller. Registry installation through the supported native session-update path is environment bootstrap, never candidate proof. The project auto-update preference is restored after scoped bootstrap, and projection-only preparation remains nonpublication evidence. The source inventory remains the same reviewed 105-file `sha256:31fa30d0bbc53f76f4f09cbca80101965b71dc1c00c337b54c53607ab3df4a40` set.

Actual pre-refresh candidate `c626fa46` confirmed the unchanged selected target bounds: CLI contracts passed in 281.6 seconds and contract enforcement in 274.2 seconds; the entrypoint target completed in 207.4 seconds with only the prior-release template mismatch failing. Its genuine fresh docs proof passed in 75.2473 seconds. These source-CI diagnostics confirm placement and retain the failure honestly; the regenerated candidate still requires its own complete configured execution and independent full native validation.

## Managed project proof categories

The independent disposable native service consumer executed project selection, a shared task write/read from two separately custodied CLI processes, direct data denial after external assignment revocation, and next-command denial while the other account retained access on source `15f5237f`. Discovery/account envelope and session issuance were modeled; native authorization and PostgreSQL execution were real. This is exact development-source evidence, not hosted or released-client assurance. The subsequent accepted 0.140.0 version-only merge changes the candidate and requires renewed complete source and native consumer proof. Release-bound projections are generated by the registry 0.140.0 controller obtained through the supported session updater; a matching projection or reviewed inventory cannot stand in for that execution.

## Versioned cloud mutation receipts

Mixed-release cache regressions require legacy rows and unknown outcomes to survive migration, the released array-only reader shape to reject v2, unsupported future versions to remain untouched, and oversized encoded upgrades to preserve their original bytes. These are additional full-profile obligations; no manual build or standalone test supplies current publication readiness.

## Accepted inventory release reconciliation

The accepted target `1d04f232c46864eecfadb9c8b9b23069ca0e906e` adds bounded local storage inventory and releases Decapod 0.140.1. Its declared registry dependency is Dactyl 0.13.0, checksum `ba334815ca49f203b6a933d559b84b5aca5559686237be9791f48d71b3cb6292`; the later registry releases are not this consumer artifact. The actual 105-source inventory is reviewed as `sha256:be2b53094dcaddbe64037a722a82763ac837d7090dc2b7bf280f7c94452d5a57`. The accepted Bazel delta adds storage-inventory fixture inputs and preserves its finite CLI aggregate deadline and exclusive scheduling. The generated module lock changes the registry Dactyl references consistently. No required check, executor, workflow or selector is removed. The public managed-project branch preserves those accepted definitions.

The earlier full 0.140.0 attempt returned `SNAPSHOT_NOT_READY` after the accepted target advanced; it is not current readiness. The 0.140.1 registry controller is installed through supported `session acquire` with a temporary scoped auto-update opt-in, then restores that preference. Release projections are generated through the controller. Both the version-two receipt tests and native project consumer require fresh execution for the resulting exact source, accepted target and registry artifact. Prior source-specific native evidence remains historical and cannot authorize publication of this new candidate.

## Accepted patched registry reconciliation

Accepted target `df15c9b8dcaeff2e4cba7ddd4918a1e2e954d455` merges the published Dactyl 0.14.0 adoption and releases Decapod 0.140.2. The exact registry checksum is `d10824bf8882a26b92479f07edb26c1f6fde8da3019f4001befaf28699d7a990`; the Quinn protocol lock floor is 0.11.19. This supersedes the earlier 0.13.0 consumer state, while retaining its history. The reviewed 105-source inventory is `sha256:f9a15ead2fa48e1c4beb6341959b06e7cce9e05658cd66ce3685c8eea7e4386a`: accepted changes are registry package/lock definitions, generated module dependency mappings and the release dependency policy. Required workflows, source executors, checks, selectors and budgets remain intact.

A fresh supported session updater installed registry controller 0.140.2 into an isolated install root, restored its temporary auto-update preference and confirmed no pending Decapod migration. Its embedded documentation matches 0.140.1. All earlier readiness attempts and local service reports remain bound to their historical candidates; the newly reconciled source and registry artifact require fresh full validation and independent native consumer execution.

The PR1648 rebase uses the independently installed registry 0.140.2 controller.
All 105 inventoried build inputs match accepted `df15c9b8`, including its Dactyl
0.14.0 adoption and native inventory wiring. Only package-version bytes differ
from the reviewed SDK-adoption parent. Native commands regenerate release
projections and refresh the reviewed digest; this preparation is not execution
proof. The archived original PR tip retains its historical checkpoints, while
the rebased source requires its own current checkpoint and complete validation.

## Accepted v0.142.0 master reconciliation

Reconciling `origin/master` (`d3542c30`, `v0.142.0`, incorporating PR #1647 cloud sign-in/diagnostic updates, PR #1649 bounded reflection, and `release-plz` v0.142.0) preserves PR #1651's `cloud projects` / `cloud project use|create` routing and v2 operation cache alongside `CloudCommand::Login { restart }` and `authorization_url` recovery diagnostics. The 105-file reviewed source inventory hashes to `sha256:6d13f890e76d0c0b69e338bb571d2858166afbf30adeeb1e58313a6b12042c87` (adding `Cargo.toml`/`Cargo.lock` `0.142.0` package versions and `.config/build/decapod.BUILD.bzl` entrypoint target inputs without dropping any required check, workflow, selector, or budget). All prior historical validation receipts remain bound to earlier commits; the merged candidate carries `0.142.0` controller projections, `specs.refresh`, checkpointing, and requires complete `decapod validate` proof.

## Owned scratch mode-000 cleanup (#1643)

`assets/tools/build_lifecycle_tests.rs` covers rootless container overlay
permission recovery through `//:build_lifecycle_tests` and the Cargo binary test
suite. `supervisor_recovers_owned_mode_000_podman_overlay_scratch_without_touching_shared_caches`
spawns the real supervisor binary with a child that populates
`$HOME/.local/share/containers/storage/overlay/opaque-layer/work/work` (`0o000`
under `0o500`), a read-only `0o555` layer directory, and a symlink to an
external `0o555` shared cache alongside an unrelated `0o000` sibling directory,
requiring clean exit, zero `build cleanup:` stderr diagnostics, complete scratch
removal, and untouched external permissions and files.
`owned_permission_recovery_rejects_symlink_uid_device_and_bound_escapes`
directly exercises `Scratch::recover_owned_permissions` and
`Scratch::recover_owned_permissions_with_bounds` across nested `0o000`
directories (independent of host effective UID), symlink non-traversal,
foreign-UID and cross-device rejection before chmod/descent, and closed failure
on depth and entry bounds, while `cleanup_diagnostics` retains its read-only
contract.

Reconciled with accepted `master` (`3b6b31f8`, `v0.143.0` including `#1651` and
`#1656`), the installed `0.143.0` controller regenerates the release-bound
`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`,
`.decapod/managed/Dockerfile.decapod` and `.decapod/managed/specs/.manifest.json`
projections, and `.decapod/config.toml` updates
`proof.required_profile.reviewed_sources_digest` to
`sha256:ea92a8706fe811047b6ffbc92826d4ddeb008db851ab1b9c1853b024d65df768` for
the combined `v0.143.0`, `#1651`, `assets/tools/build_lifecycle.rs`, and
`assets/tools/build_lifecycle_tests.rs` inputs in the reviewed source inventory.
Source review and local projection refresh do not prove execution; full
configured `decapod validate` on the exact candidate commit against accepted
`master` remains mandatory before merge.

Reconciling `#1629` with accepted `master` (`64c4ff2e`, `v0.143.1` including
`#1655` and `#1664`) regenerates `0.143.1` release-bound entrypoints,
`.decapod/managed/Dockerfile.decapod`, and `.decapod/managed/specs/.manifest.json`
from the `0.143.1` controller and updates `reviewed_sources_digest` to
`sha256:50af27077a026878a187611c381226be611ae10f45bfec77fc205282258d3ccf`;
full CI and `decapod validate` execution on the exact merged commit remain
required before publication readiness.

## PR 1666: accepted v0.143.2 source integration

Run the current `github_quota` tests and the real automatic discovery pacing regression. Synthetic curl responses exercise transport/pacing behavior only. A current complete independent provider judgment remains required; prior project checks and pre-reset observations are not readiness.

### v0.143.2 fresh-docs workunit fixture initialization

`tests/support/fresh_docs_readiness.rs` initializes the workspace-local workunit runtime using the real `init --refresh --proof` CLI before declaring unverified completion. Root datastore initialization does not initialize this journal. The scenario still requires a clean unchanged candidate, rejects unverified completion and withholds readiness receipts; the production `WORKUNIT_RUNTIME_UNINITIALIZED` guard remains intact. The repair passed on the baseline Dactyl dependency before SDK-specific verification.

## PR1666 current-master integration

Current accepted source is master 9b900254; the source package version is 0.144.0. The release was compiling and assets were unavailable at setup. Use actual hosted CI results for this update, push each addressed root cause and do not run tests locally. Refresh official release-bound projections through supported tooling when that release becomes available; no cached earlier green result is treated as current proof.

### Reviewed 0.144.0 build-input binding

The accepted release changes the Decapod package version in Cargo.toml and Cargo.lock. Review the actual source inventory and bind its current digest without changing required checks, providers or toolchain requirements. Git symlink targets retain their accepted identity; no toolchain change is claimed from comparing a symlink blob with dereferenced contents. The original PR parent remains in the merge graph. No local tests are executed.

### Official 0.144.0 projection drift repair

The hosted standalone validator passed project prerequisites but rejected uncommitted release-bound regeneration: entrypoints and the managed Docker image/version still referenced0.143.2. The official installed0.144.0 kernel now regenerates those projections and their manifest through supported init/spec interfaces. Preserve the exact reviewed configuration and the drift gate; no local test execution or passing full-readiness receipt is claimed.

### Bounded complete CLI contract execution

Hosted Contract Conformance exhausted the300s aggregate while entering the schema-determinism case after16 completed cases in the22-case sequential suite; the matching push job passed. `tests/cli_contract_enforcement.rs` now uses a60s process-group deadline in its shared command helper, producing a command-specific failure instead of an unbounded child wait. The Bazel target uses the existing finite900s long class, retains exclusive isolation and executes all22 cases without removing assertions. The reviewed source-inventory update covers that explicit build-input budget change; required checks, providers and proof semantics are unchanged. No local tests are run.

## PR1663 integrated storage behavior

Current accepted target is5e23ad68 (0.144.1). Preserve all original regression sources and use hosted CI for the new integrated candidate; do not run local tests. The resolved source export conflict retains both custody/resource guards and the component-journal API. Source inventory, release projections and current-content reviews are refreshed without changing required provider policy or claiming independent full readiness.

### Blank-title write-admission regression

Hosted Gatling failed because its legacy EDGE-1 case expected an empty task title to be accepted. The storage contract rejects empty and whitespace-only new titles. The same existing real-CLI case now requires failure with the typed title-admission diagnostic and compares task identities/titles before and after each rejection, preserving the no-task-created invariant. Production admission is unchanged; historical read compatibility is not redefined. Current hosted execution must verify the repair; no local tests run.

### Complete bounded todo-plugin execution

Hosted plugin verification exhausted the300s moderate aggregate after more than30 completed cases in the40-case todo suite. That target now uses the finite900s long class and exclusive target scheduling, while its internal concurrent workers and all assertions remain intact. Its serial CLI helper keeps broker routing, uses a1s idle window and a120s child-process deadline with command diagnostics. Optional container behavior is preserved. The reviewed build-input binding records these execution-budget changes; no local tests or production latency assertions are removed.

## PR 1662: accepted v0.143.2 source integration

Exercise actual `sqlite_maintenance` CLI/RPC worktree cases and storage-lock regressions on the updated source. Policy substitution, unsupported cloud maintenance and owner-only recovery must remain rejected. Historical project checks do not establish full current-candidate publication readiness.

### v0.143.2 fresh-docs workunit fixture initialization

`tests/support/fresh_docs_readiness.rs` initializes the workspace-local workunit runtime using the real `init --refresh --proof` CLI before declaring unverified completion. Root datastore initialization does not initialize this journal. The scenario still requires a clean unchanged candidate, rejects unverified completion and withholds readiness receipts; the production `WORKUNIT_RUNTIME_UNINITIALIZED` guard remains intact. The repair passed on the baseline Dactyl dependency before SDK-specific verification.

## PR1662 current-master preparation

This checkpoint preserves original source and tests before integration with current master. Preserve failure history and all assertions; use hosted CI for verification, without local tests or running the maintenance candidate against the live machine datastore. Prior test observations do not prove the new integration.

## Bounded independent collection (#1573)

Existing `github_quota` tests cover discovery headroom/direct Buildkite polls,
exhaustion/unknown reset, malformed/duplicate/missing/overflow hints and deadline/
cancellation preservation. Adapter tests preserve bounded failure information.
The real complete1658 observation and five Buildkite workflow executions are
documented separately from mechanism tests and historical403 failures. This
dedicated closure candidate still requires its own hosted suite and full native
publication gate against current accepted master. No local tests were run.

## Required local maintenance acceptance (#1568)

The required `cli_contracts` target now includes `tests/sqlite_maintenance.rs`;
Bazel supplies its module source explicitly. The tests traverse real Dactyl
SQLite integrity/backup/recovery/lock behavior, the actual binary's operator
surface, governed CLI/RPC opt-in and anti-self-elevation, and two real claimed
worktrees with concurrent bounded retry and cross-custody rejection. No fake
adapter/error replaces the maintenance acceptance path. Missing native SQLite
fails the required cases instead of silently returning.

The genuine governed fixtures clear inherited outer-provider context and all
three diagnostic overrides only in their own children, retaining admission and
leaving the parent source-job identity intact. Older operator bootstrap cases
remain diagnostic; they are not claimed as full readiness or authenticated cloud
proof. Existing exclusive operation locks, timeouts, backup path restrictions and
cloud-denial assertions remain. The source suite's accepted provider graph,
profile and original cases are preserved. No local tests ran; current hosted
execution, case outcomes and full exact-candidate publication proof are pending.

## Current published SDK 0.16.1 adoption (#1667)

The current exact registry pin/checksum is `dactyl-db =0.16.1` /
`1b41dde2e8be3fc986f7f5a7eb4e5bfb9af49d4c39dabe98fd74a0ccb4ad7ac6`.
Official sparse-index discovery and downloaded crate bytes agree. The Rust
library sources match 0.16.0 byte for byte; its additive 0.15 resource accounting
and 0.16 logical connectors retain the existing application facade. Rust 1.85
remains below Decapod's unchanged 1.96.1 requirement. Cargo resolution updated
only this requested SDK; Bazel crate-universe regeneration changed twelve lock
lines after preserving the accepted one-space JSON representation. Nix derives
its package graph from Cargo.lock and needs no separate SDK input update.

`tests/release_dependency_policy.rs` pins the registry source, exact release and
checksum while retaining storage-feature and patched Quinn-floor checks. The
journal and transfer tests construct malformed historical payloads in disposable
SQLite so SDK parameter budgets do not prevent exercising the original bounded
reader rejection. No admission limit, proof assertion or existing check is
removed. The required source suite still includes offline Supabase/Neon/local
consumer behavior; this update cannot establish real hosted backend parity.

No local compilation or tests were run. Dependency resolution and Bazel module
regeneration are input-authoring operations, not proof. Hosted execution and the
full exact-candidate official Decapod0.145.1 publication gate remain pending;
provider requirements and accepted execution contracts are unchanged.

## Diagnostic override acceptance regressions (#1659)

`tests/consumer_readiness.rs` runs the actual full runtime in isolated child
processes for empty/zero Git skips, false tooling skips, truthy fingerprint skips
and a false fingerprint control. Denied claims must select/execute nothing and
produce no evidence directory; the false fingerprint control must execute real
native proof. No parent test process mutates its environment.

The existing genuine claimed-worktree scenario in
`tests/support/fresh_docs_readiness.rs` invokes full CLI validation, RPC validation,
workspace publication and historical validated completion under overrides. It
requires actual kernel rejection, no successful judgment or full receipt, and
unchanged source, then retains the normal positive proof flow. Broker unit tests
use real capture/apply subprocesses over different inherited worker values and
invoke the real environment gate, proving both retained denial and cleared scope.

The existing partial CLI admission checks `scope=ci_local` and
`publication_ready=false`; it remains a prerequisite observation rather than
complete proof. Current hosted execution and full remote readiness are pending.
No local tests, disabled required checks, fabricated receipts or workunit-forgery
completion claims are included in this checkpoint.

## Accepted local proof policy regressions (#1658)

The dedicated v0.144.3 branch preserves accepted command, program, argument,
directory, environment, toolchain, timeout and coverage declarations before
provider selection. `tests/consumer_readiness.rs`, included by the required
`cli_contracts` target, exercises real Git targets and native child processes.
Controls cover repinned success-printing replacements, removals, execution and
coverage edits, additive checks, unchanged accepted execution and stale source.

A further adversarial case keeps the command unchanged, corrupts source, rewrites
its accepted check script and repins the source-definition digest. This captures
a remaining source-admission question; it must be rejected before closure. A
positive control independently accepts a source-policy revision on the target
before validation. The genuine fresh-CLI scenario explicitly covers initial
adoption, rather than labeling its later workspace-specific environment as
already accepted policy.

No current hosted result or local test is claimed at this checkpoint. Push the
regressions to collect the actual judgment, preserve any observed false green,
repair the source boundary, and require fresh exact-candidate hosted proof.
Configured source-PR readiness remains strict and provider-backed. A local
consumer result cannot stand in for unavailable external assurance.

### Observed check-script false green and required repair

[Buildkite9472 CLI execution](https://buildkite.com/decapodlabs/decapod/builds/9472#01a1d6e5-1809-485a-b0dd-5acc62254ffe)
evaluated059dec1b against422e0b56. The registered source-script attack produced
`ready=true`, `proof_ready=true`, local-readiness scope, completed native evidence
and no blockers. The real regression failed; the suite recorded86 passed,
one failed and one ignored in189.18s. The full CLI job took640.61s including its
build and all three targets; those are separate runtime measurements.

The repair additionally preserves the accepted local source-definition digest
before provider selection. The same attack must now return the typed accepted-
contract blocker with zero selections/executions, while unchanged policy,
additive commands, first adoption and separately accepted target-policy evolution
still execute real proof. Current hosted revalidation remains pending; previous
failed evidence is retained and no local tests or full PR readiness are claimed.

## Accepted release projection refresh (v0.144.5)

After integrating accepted master `9767ae9a3ff1c7452101482218a17c55cf569b71`, regenerate the agent entrypoints and managed release projections with the official v0.144.5 binary. Accepted verification requirements remain unchanged. Projection integrity is a prerequisite; fresh hosted GitHub Actions must evaluate this candidate. No local tests or full publication-readiness claim accompany this refresh.

## Repeated validation runtime fixture

`tests/validate_termination.rs::validate_smoke_runtime_is_bounded_without_contention` establishes a named session before timing and asserts that repeated prerequisite checks reuse it. The existing per-call 10-second and aggregate 20-second limits remain unchanged. This measures steady-state prerequisite latency; it does not establish full publication readiness. Hosted verification is required for this candidate.

## Observed workunit regression evidence (#1660)

`validation_history_workunit_tests` executes two real repository checks through
the existing sealed runtime, archives their actual observation and exercises
workunit acceptance, stale input rejection, selector substitution, interrupted
projection recovery and archival races. Portable completion tests reuse genuine
observed proof. Legacy CLI/publication cases reject pass-only declarations;
journal tests preserve file-integrity and lease/recovery coverage without
mislabeling resumability as VERIFIED proof. The local accepted-target transport
is a real bare Git fixture, not full authenticated forge or native agent proof.
Hosted execution and final exact-candidate publication validation remain pending.
No local tests or CI are run during authoring.

## PR1677 integration with accepted v0.145.0

Integrate accepted master `706569c7ac2f156def00a331134fee52fb444042`, including the canonical observed-workunit
proof producer from #1681. Diagnostic overrides must still be rejected before
full readiness, publication and validated completion; the new canonical archive
cannot convert a diagnostic result into full proof. Preserve both contracts and
the broker's durable-outcome precedence and bounded request windows.

Official Decapod v0.145.0 regenerates release-bound entrypoints, the managed image
pin and spec projections. Reconcile the reviewed source inventory for this
accepted release without changing required checks or provider policy. Earlier
success for `65da34111d87d70792b59e8fb02ac067d15451ce` against `e8f8848d19a02ad8e2a580cf7722fe4ff9342221` remains historical.
Fresh GitHub Actions must evaluate the integrated candidate. No local tests run;
Buildkite is excluded at the user's direction and full kernel publication
readiness is not asserted by this integration checkpoint.

## PR1678 integration with accepted v0.145.1

Integrate accepted master `1c1fa5dcfc2d31cc2c09dcf5d18470dc62900dd5` with the published Dactyl 0.16.1 registry
pin and checksum. Retain the accepted canonical observed-proof and diagnostic
admission boundaries from #1681 and #1677. The SDK remains transport/storage
infrastructure; it cannot declare task completion or substitute local authority
for a rejected cloud principal. Keep the patched Quinn floor and existing
cell, parameter, result-accounting and wire limits distinct.

Regenerate entrypoints, the managed image pin and projections through official
Decapod v0.145.1. Reviewed inventory changes identify the accepted release and
SDK inputs without changing required checks or provider policy. Source and
proof metadata from `fbe399289d1f24011476312e16653e8d830f490a` against
`e8f8848d19a02ad8e2a580cf7722fe4ff9342221` remain historical. Fresh GitHub Actions
must verify this integrated candidate. No local tests run; Buildkite is excluded
at the user's direction and full native publication readiness is not asserted.

## PR1679 integration with accepted v0.145.2

Integrate accepted master `0677f9906d7e70651a9fc2337b2c27899bd331fa`, including
the published Dactyl 0.16.1 SDK and the canonical observed-proof and diagnostic
admission boundaries. Required CLI/Bazel execution still includes the real
SQLite maintenance module: missing SQLite must fail, and repository-local
fixtures must establish their own task custody and accepted maintenance policy.
Candidate opt-in, cross-task claims and forged cloud principals cannot establish
authority. SDK transport and same-user local execution do not add independent
identity or isolation guarantees.

Official Decapod v0.145.2 refreshes release-bound entrypoints, the managed image
pin and spec projections. The reviewed inventory identifies these inputs without
changing required checks or provider policy. Prior success for
`331f6d748a44e91a4fd31c7af6d59372acd17eaa` against
`e8f8848d19a02ad8e2a580cf7722fe4ff9342221` is historical evidence only. Fresh
GitHub Actions must evaluate the integrated candidate. No local tests run;
Buildkite is excluded at the user's direction and full native publication
readiness remains unproven.

## PR1680 integration with accepted v0.145.4

Integrate accepted master `5002f1db286f4d41c92ce4d4b4a58468bf55aa3b`, preserving
the accepted canonical workunit producer, diagnostic admission rules, published
SDK and required SQLite/CI selection coverage. Collector quota hints remain
bounded scheduling inputs; denied public requests cannot acquire authority from
a diagnostic client, a header or an unrelated successful job.

Official Decapod v0.145.4 regenerates release-bound entrypoints, the managed image
and projections. Reconcile the reviewed inventory without changing required
checks or provider policy. The documented v0.144.3 provider collection and the
prior documentation candidate `725b4114595bf17a36568afbe164ce5c9f7b9cf4` against
`e8f8848d19a02ad8e2a580cf7722fe4ff9342221` remain historical observations. Fresh
GitHub Actions must evaluate this integrated candidate. No local tests run;
Buildkite is excluded at the user's direction and full native publication
readiness remains unproven.

## Versioned physical legacy recovery coverage (#1657)

`tests/sqlite_maintenance.rs` exercises the actual Decapod binary with isolated
repositories, canonical policy and claimed workspaces. Regressions cover exact
canonical-copy archival, unchanged canonical bytes, bounded receipt size,
idempotent replay, prepared-state startup fencing, resumption after physical
archival, separate-grant denial, stale source bytes/modes, corrupt input,
symlinks and reappeared source. The existing reconstructed verification consumer
must reject the operation report as task completion proof. Interrupted physical
states are deterministic fixtures built from a real kernel-issued preparation,
not ten native agents or proof of a killed process at every instruction.

`tests/unit/cli_tests.rs` checks typed command parsing and permission separation.
Required CLI aggregate already includes SQLite maintenance tests. Hosted execution
for this candidate remains pending; no local tests run. Buildkite is excluded at
the user's direction while its accepted requirement remains enabled, so full
native publication readiness is unproven. Empty/exact-copy retirement is a
bounded #1657 slice: unmatched legacy schemas, arbitrary historical equivalence,
cloud owner repair and independent security isolation remain unresolved.

## Squash release packaging regression (#1626)

`tests/release_metadata_policy.rs::nix_gates_recognize_squash_linear_and_ordinary_release_commits`
extracts and executes both actual workflow scripts against real Git repositories.
It creates a real squash merge, a linear release and an ordinary release merge;
non-release, root and octopus histories remain ineligible. The owner performs its
real depth-two fetch through a process-local URL rewrite to the fixture. Imported
runner and PR/manual event cases stay denied; manual packaging stays supported.
No Nix, Cachix credential or hosted publication call is simulated as success.

Fresh GitHub Actions must verify the exact candidate; no local tests run.
Buildkite remains excluded at the user's direction while its accepted policy
requirement stays enabled, so full native publication readiness is unproven.
The original missing v0.138.0 cache entry is historical and is not claimed filled
by this code change. Actual protected-master packaging/publication must establish
cache availability separately after acceptance.

## Canonical local Git proof (#1069)

The new required actual-binary entrypoint regression creates an operator-reviewed canonical local contract, claims a task, creates its governed workspace and invokes ordinary full validation with provider environment removed. It checks real executed observations, durable history inspection, unchanged repeat execution, source changes, forged diagnostic claims for altered source, accepted-base movement, authority revocation and forge publication refusal. Existing strict/external and adversarial suites remain required. GitHub Actions runs the checks; no local tests/builds or Buildkite evidence collection. Exact hosted observations are pending and the complete ten-worker A–H/native-runtime mission remains unproven.

### v0.147.0 acceptance target

PR #1702 now integrates accepted release master
`26196764ec1dfa95ce366c08fc3e104b8a5bfec1`, including #1697's bounded physical
recovery. Final authoring projections and checkpoint observations must use the
installed v0.147.0 kernel. The release changes Cargo package identity, so review
the measured verification inventory against that exact accepted source before
reconciling its pin; preserve all required commands and strict provider policy.
Earlier v0.146.0 source observations stay historical. Queued checks that reject
a moved target before execution establish no test result. Fresh GitHub checks
must evaluate this combined candidate, including normal local CLI proof and
the unchanged common publication-admission denials. No local project test/build
execution or Buildkite collection is authorized by this toolchain transition;
full native publication proof remains unclaimed.

## PR #1699 accepted-target integration

The escalation branch now integrates accepted master `8e435969a8c21269aa277301b3e6492d780a9bdb` with installed Decapod 0.145.6. Preserve both the escalation contract and the accepted Nix release-gate/finite-adapter changes, including both owning Bazel fixtures. The earlier full native observation for `d52943740cac03fa45046317ea4af090f919fec6` against `f5da0e6183bf31cfbef3d5a53c52553ee8b6fbb5` remains historical and cannot authorize a new integration. Fresh checks must bind the new source/tree/target and custody generation. This integration uses GitHub verification without local test/build execution or Buildkite evidence collection; accepted strict provider requirements remain enabled, and no automatic merge authority is introduced.

## Documentation brand and media acceptance (#1707)

Publication of the Decapod mdBook brand refresh requires source-bound
verification that the book still builds with its existing mdBook source-CI
profile and unmodified workflow inventory. The first book page must show
the Decapod Labs identity and mascot, embed the website-owned video above
the documentation navigation, and link to the accepted quickstart and source.

The shared stylesheet must retain readable, distinguishable light and dark
themes, keyboard-focus affordances, responsive layouts and legible code blocks.
Media URLs must resolve from the separately published Decapod Labs website.
A docs build alone is not proof of a successful public GitHub Pages deployment;
runtime domain availability remains a separate observation.

The showcase kernel PR must not introduce a new GitHub Actions workflow, shadow
the website's renderer or suppress the existing living-spec material gate.
Independent native source validation and complete candidate readiness remain
necessary before merging.

## Readiness Gate-Skip Override Hardening (#1659)

Diagnostic gate-skip overrides (`DECAPOD_VALIDATE_SKIP_GIT_GATES`,
`DECAPOD_VALIDATE_SKIP_TOOLING_GATES`, and
`DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES`) are strictly non-authoritative and
must never establish full validation, publication readiness, or validated task
completion:

- **Full-readiness rejection boundary:** `core::validation_runtime::require_full_readiness_environment()` rejects any presence (`var_os().is_some()`) of `DECAPOD_VALIDATE_SKIP_GIT_GATES` or `DECAPOD_VALIDATE_SKIP_TOOLING_GATES`, and any truthy value of `DECAPOD_VALIDATE_SKIP_FINGERPRINT_GATES` (`core::validate::skip_fingerprint_gates()` matching `1`, `true`, `TRUE`, `yes`, `YES`), failing with `READINESS_GATE_OVERRIDE_UNSUPPORTED` across `decapod validate`, `decapod v`, `decapod rpc --op validate.run`, `decapod workspace publish`, `decapod rpc --op workspace.publish`, and `decapod todo done --validated` (live and `--commit` historical paths).
- **Pre-side-effect enforcement on `WorkspaceCommand::Publish` (`src/decapod/lib.rs`):** When `proof.validation_buildkite_preflights = false`, `WorkspaceCommand::Publish` enforces `require_full_readiness_environment()?` before calling `run_validation_bounded_with_receipt(..., refresh_specs = true, ...)`, ensuring gate-skip overrides reject immediately with `READINESS_GATE_OVERRIDE_UNSUPPORTED` without mutating `.decapod/managed/specs/` or surfacing masked downstream failures.
- **Verification and recovery isolation (`src/decapod/plugins/verify.rs`):** `run_validate_and_hash` calls `crate::core::validation_runtime::require_full_readiness_environment()?` prior to spawning child `decapod validate --format json`, and `safe_validation_failure_diagnostic` includes `READINESS_GATE_OVERRIDE_UNSUPPORTED` in `MARKERS`. This closes the bypass where `qa verify`, `qa verify todo`, `qa verify recover`, or `todo done --validated` under `GITHUB_ACTIONS=true` or `BUILDKITE=true` with active skip overrides could otherwise enter `CiLocal` and mark tasks verified.
- **Partial diagnostics and non-reusable receipts:** `ValidationScope::CiLocal` (`GITHUB_ACTIONS=true` / `BUILDKITE=true`) and `ValidationScope::ProjectPrerequisites` (`decapod govern project-check`) preserve `publication_ready = false` and never record `validation.completed` history (`core::validation_history::finish_archive`) or full native readiness artifacts (`observations.json`, `completion/`, `historical-proof.json`). Removing an override after an overridden or partial `govern project-check` receipt write (`sections.validation` in `.decapod/governance.json`) does not authorize `validate`, `workspace publish`, `qa verify commit`, or `todo done --validated [--commit]`.
- **Group broker isolation (`src/decapod/core/group_broker.rs`):** `CALLER_ENV_KEYS` clears/replaces all three `DECAPOD_VALIDATE_SKIP_*` variables per caller request so an override-tainted broker leader cannot taint a clean `todo done --validated` client, while override-tainted clients fail in `lib.rs::run()` before mutating todo state.
- **Regression suite and verification scope:** `tests/gate_skip_override_readiness_1659.rs` exercises all of the above end-to-end against the compiled `decapod` binary in isolated canonical local-Git repositories (`repository_profile::REQUIRED_PROFILE`, `canonical_local_target = true`). Full hosted Buildkite/GitHub source-CI acceptance for this branch remains governed by the repository's configured proof profile and is not substituted by local test execution.

## Split-store convergence and all-6 #1660 VERIFIED workunit falsifiers

`src/decapod/core/workunit.rs` (`converge_split_workunit_store`,
`workunit_journal_status`, `validate_verified_manifest`, `transition_status`,
`resume_workunit`, `init_workunit`, `checkpoint_workunit`, `mutate_workunit_with_proof_gate`)
enforces single-domain canonical custody and kernel-observed proof for all
`VERIFIED` workunit transitions and publication gates:

- Any separately initialized workspace-local datastore (`<worktree>/.decapod/data/todo.db`
  or `decapod.db` containing policy tables `agents`/`risk_zones`, `workunit`
  events, or corrupt/unreadable state, while permitting benign `traces`-only
  RPC logs) returns `WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED` on `VERIFIED`
  transitions, `validate_verified_manifest`, `resume_workunit`, partial or
  uninitialized split-store writes that would otherwise leak into the canonical
  main store, and `journal-status` (`split_store: true`,
  `atomic_domain_supported: false`, `proof_status: "not_proven"`), while
  keeping explicitly initialized non-`VERIFIED` local declarations isolated
  inside the workspace-local datastore until converged.
- `decapod govern workunit converge [--task-id <id>] [--dry-run]` inspects or
  archives workspace-local database files and split-only projections into
  `.decapod/data/split-store-audit/<archive_id>/audit.json` as `not_proven` audit
  history (`classification: "WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED"`), never
  promoting or importing unattested or self-declared `sha256:abc` rows into the
  canonical store. After convergence, `journal-status` continues retaining
  `audit_records` as `not_proven` while allowing current tasks in the worktree
  to execute canonical `decapod validate` and transition atomically to `VERIFIED`.
- Actual-binary suite `tests/workunit_verified_proof_1660.rs` (zero
  `DECAPOD_VALIDATE_SKIP_*` gate-skip variables) exercises all 6 `#1660`
  falsifiers: (1) self-authored `record-proof` pass + `sha256:abc` never mints
  `VERIFIED` or publication; (2) quick-scope/skipped, failed, or unavailable-tool
  attempts stay `not_proven`; (3) source mutation, accepted-base movement,
  changed `proof_plan`, and newer failed validation downgrade/mark receipts
  `stale`; (4) authentic proof copied across tasks, custody generations (including
  ABA lease reclaim), or repositories is rejected; (5) split-store records stay
  `not_proven` audit history with `WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`;
  and (6) repaired canonical tasks complete `VERIFIED` atomically and pass
  `verify_workunit_gate_for_publish`.

## Accepted release inventory review

Release target34f837d4 changed only the decapod package version from0.148.9 to0.148.10 in Cargo.toml/Cargo.lock and changelog. All105 verification source definitions were compared to the prior reviewed candidate; only those two version lines changed. No required check, workflow, helper, adapter, environment or timeout changes. The candidate source inventory is explicitly reconciled tosha256:e1a242381f7411a8facfe285e2945040dafa93c46dd48c11a86ebd55d9a95b3f; fresh native source-CI validation still decides admission. Validation09 target-ancestry failure and validation10 stale-inventory rejection remain preserved.

## Native0.148.10 projection refresh

The d4269e35 source-built CI health step actually rejected current generated drift: its0.148.10 binary refreshed the managed Docker pin and specs manifest from0.148.9. Official checksum-verified0.148.10 is now installed. Native workspace/agent/spec refresh and the reported project-check repair regenerated those projections; no AGENTS file was manually edited and no source check was weakened. The local prerequisite-only repair reported three failures (first container_workspace_required) while applying two repairs, so it is not claimed as a project or publication pass. Fresh complete Buildkite-owned validation is required for the checkpointed candidate. The actual captured Pi export replay at d4269e35 passed and reproduced4180e2b8... byte-for-byte; no fixture actions occurred.

## Accepted conformance and0.148.11 integration

Full native validation13 and renewed publication passed for45ae1bff/base34f837d4 using complete Buildkite12165 and standalone12167. Both protected app805657 contexts passed; actual library1813pass/8ignored included13 observer cases. Publication independently read back that exact head/base. Master then advanced through accepted interop PR1787 and release0.148.11 to352d892958e1cdf692299927f7c2f3c0c36fc639. That admission is historical and PR1784 returned to draft.

The accepted changes separate source conformance coverage from executed results, reject unregistered RPCs before startup and malformed JSON before negotiation, advertise actual session gates, and tighten the actual competing-mutation timing assertion. Observer source is unchanged and needs no parallel registry or authority. All accepted source/test changes are retained. Authored living specs were merged separately from generated blocks, preserving both the accepted conformance contract and observer/recovery sections. Native commands refresh generated projections.

The105 verification-source definitions were reviewed; only Cargo.toml/Cargo.lock version lines differ in that declared inventory, while the actual accepted Rust/fixture changes are covered by full source/tree binding and required execution. The reconciled inventory issha256:2acf1af72443f49b41967205535a4a0a4eff38a100e3b11ab930a010a3437db0. Official macOS release pipeline44286433522 artifact14419543607 was inspected and checksum-verified (archive5412a6ba1637a188306cf4fce3759d369e8edec78248b7c5a9e1de23ca3ab873); its14e5d8d8 tag and352d8929 master share tree4a9469b32f9c3c9992864b9e5b04995bd48449d0. Installed0.148.11 replaces backed-up0.148.10 without modifying the pinned Pi0.148.8 binary. No new current proof is assumed.

## Release11 tag currency and native pin repair

Validation14 stopped before pushing b413bb64 because the new remote release tag was absent from frozen local release history (SNAPSHOT_UPLOAD_INCOMPLETE); none of its required CI observations is claimed authenticated. Read-only git fetch origin --tags obtained v0.148.11 without publishing or rewriting any tag. Official0.148.11 native govern project-check --refresh-specs then regenerated the managed Docker image/version pin from0.148.10 to0.148.11. Its prerequisite-only result remains partial on the host and is not full readiness. This exact repair is checkpointed before fresh complete validation15. Accepted source,105 inventory2acf1af7 and all required checks remain unchanged.

## Recovery archive disposition
The selected-source reauthoring and present-time history observation are committed together at fafe6138db68a0d450cc01513b628c17cc40aa3e. The subsequent recovery disposition references that immutable archive. It does not establish the missing historical checkpoint or replace full current-candidate validation.

## Preserved failed source history
The prior source branch at dad525d46feb9daf09f498c3d2934cd6560fc87b contains integration merge c31d039f without an exact-material checkpoint. That history remains preserved and unproven. The current workstream records the native present-time recovery observation, reauthors the reviewed source in a fresh claimed workspace, and requires independent full validation of its new commits. No retrospective checkpoint or inherited readiness is asserted.

## Acceptance observer delivery checks
The mandatory offline core suite exercises actual source attribution, failed-check feedback followed by a new model repair action, receipt subject matching and later source/base invalidation. The ignored live export remains separate from those deterministic tests. A passing code suite does not supersede current-content publication-spec review: CI rejects a stale review even when a final lint-only edit preserves behavior. Refresh the affected reviews and obtain a fresh full exact-candidate validation before publication.

## Current accepted policy integration

Publication at102c638e against6ab6de31 was valid at its observed admission. Master subsequently advanced to c47137ad through accepted policy-boundary work. Preserve those new initializer, operator-consent and recovery boundaries, checkpoint the exact integration merge before committing, and collect fresh whole-profile proof; the earlier green result grants no integration authority for this new context.

## Current audit verification scope (#1069)

This candidate changes documentation and release-bound governance projections.
The implementation map cites actual #1697 source executions at
`d0b24f6f926583047be0fbe3f51f5686247d1de2` against
`f5da0e6183bf31cfbef3d5a53c52553ee8b6fbb5`; they are not this document candidate's
receipt. Native source-CI records retain their incomplete-profile and
unauthenticated scope fields. Aggregate CLI/core timing and a bounded physical
receipt assertion do not establish reconstruction microbenchmarks, five-minute
sharding or complete storage-growth proof.

Fresh hosted checks must judge this candidate's own applicable scope. Conditional
or not-applicable jobs cannot be counted as executed tests or performance proof.
No local tests or Buildkite evidence collection is performed. Accepted required
checks and provider policy remain enabled; full native publication readiness is
unproven. The complete original mission remains open rather than being closed
by a documentation update.

## Accepted v0.146.0 audit reconciliation

This map now uses accepted master `e9c40e929ee33aa5443c38b57276f96111db0b92` and the installed 0.146.0 kernel. The accepted #1699 bounded escalation CLI/RPC reference is implemented and covered by its exact-source execution; it is not complete external runtime adoption, semantic reasoning proof or the ten-worker A–H demonstration. Prior baseline paragraphs remain historical audit context. Pending #1697 physical recovery and #1702 canonical local-target admission are kept separate from accepted enforcement until individually verified and merged. Source-CI diagnostics do not substitute for the unchanged full native provider contract.

## Accepted bounded recovery audit update

The current map baseline advances to human merge `7ada4f0755b7d0b63b835c25280dd36f6bbfc525` of #1697. Its exact candidate/base/tree CLI observation is linked and stays scoped to that source; it does not prove this updated map. Bounded physical retirement is implemented, while general row migration, prepared-input supersession and full native publication proof remain unresolved. Earlier baseline paragraphs are historical. This integration preserves both authored spec sets and requires new exact-candidate GitHub verification.

## v0.147.0 map baseline

The current audit baseline is accepted release master `26196764ec1dfa95ce366c08fc3e104b8a5bfec1`, authored under installed v0.147.0. Earlier baseline statements and executed observations are historical. The accepted recovery mechanism, pending canonical-local target proposal and remaining whole-mission requirements stay distinct. The release changes package identity; all 105 discovered verification inputs match this accepted target, and only the stale reviewed inventory pin is reconciled. Strict requirements and provider policy remain enabled; fresh exact-candidate GitHub proof is required. No local project tests or Buildkite collection establish publication readiness here.

## Bazel dependency delivery boundary

The separate archive-cache carrier in `.github/actions/source-ci-profile/action.yml`
restores only content-addressed SHA-256 downloads. It preserves pinned versions,
lockfiles, independently accepted executors and required checks. Cache hits are
inputs, never proof. `tests/bazel_repository_cache_test.py` exercises real pinned
Bazel reuse during a fixture outage and rejects corrupt entries. Hosted cache
persistence and complete accepted-baseline execution still need independent
evidence; controller 0.133.0 cannot admit changed carrier bytes. See
`docs/architecture/bazel-dependency-delivery.md` for the acceptance transition.
Laya policy and PR #1575 remain separate from this infrastructure change.

## Derived release research reconciliation (#1613)

The historical candidate failed dependency denial on Tokio 1.52.3
(`RUSTSEC-2026-0386`) and governance validation on `PR_MERGE_CONFLICT`.
Reconciliation retains accepted v0.137.4 master and its Tokio 1.53.3
root lockfile, current release workflow, collector policy and controller
allowlist. Genuine native task/workspace custody, current-source spec reviews,
checkpoint history and final-candidate validation remain necessary; source
reconciliation cannot retrospectively prove the original development history.

An upstream advance invalidates the old publication base even when a previous
candidate has successful source-CI receipts. Current integration preserves the
accepted test-only plugin selection and its material validation contract. Old
receipts retain their original base/head bindings; reconciliation onto the new
accepted base requires native custody, checkpoints and fresh exact-candidate
publication checks. `PUBLICATION_MATERIAL_BASE_CHANGED` is not a test waiver.

The inactive `tests/fixtures/release_pinned_preparation.yml` retains isolated
release-plz preparation after removal of retired Vega worker-lock commands.
`core::readiness_profile::tests::preparation_research_preserves_release_boundaries_without_self_authorization`
requires unchanged release-owner/admission boundaries and byte-identical
downstream artifact jobs/matrices. It also requires the controller to reject
the unaccepted fixture even with a refreshed candidate inventory digest or an
accepted executor. The active workflow and accepted contract are not widened.

This regression is a library unit test. For the actual changed source paths,
the existing required, always-selected `supabase-consumer` source check runs
its unfiltered `cargo test --locked --features supabase-cloud --lib` command
alongside its listed integration targets. Its complete execution record and
named unit-test result must establish execution. The selected `selective-core`
check runs `entrypoint_correctness` for these paths; that job is not evidence
that this library regression ran. A skipped, ignored, cached without required
custody, or `not_applicable` result is not a fresh successful execution.

Source regressions do not complete issue #1399. The preserved release-plz
0.3.171 diagnostic belongs to its historical source. Current nonpublishing
preparation, all four configured cargo-dist archives and the complete bound
execution/collection graph remain separate obligations. No fixture, digest
update, source-CI subset or generated checkpoint grants release authority or
converts missing platform evidence into success.

## Governed repository proof-policy migration (#1738)

The strict-profile regression compares the candidate configuration already
committed by the consumer fixture with the accepted Git target. It binds
approval to those two commits and checks that the runtime still rejects the
downgrade before executor selection. See #1871.

`tests/unit/core/repository_profile_tests.rs` and `tests/consumer_readiness.rs`
cover the `repo.pr-readiness.v1` transition contract across source-inventory
updates (`reviewed_sources_digest`) and command/execution upgrades
(`reviewed_commands_digest` + `executions`), verifying full native execution and
readiness receipt generation after `policy.control`-authorized approval in the
canonical main `.decapod/data` ledger as well as fail-closed rejection for
missing approvals (`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`), unauthorized callers
(`ordinary-agent`, `"unknown"`, and `--actor unknown`), stale approvals (moved
base SHA, moved candidate SHA, or changed policy digest), revoked/expired
approvals, and unconditional `REQUIRED_PROFILE_DOWNGRADE` enforcement. Local
unit and integration tests exercise real Git targets, SQLite policy ledgers, and
native execution; full repository readiness requires Buildkite validation on the
committed candidate.

## Target-scoped repository proof verification (#1740)

Unit tests in `tests/unit/core/repository_profile_tests.rs` and `src/decapod/core/validation_history.rs` verify `command_contract_digest` compatibility, invalid/cyclic `DeploymentTargetContract` rejection, `resolve_target_closure` dependency and `shared_inputs` expansion, `preserve_accepted` scope-weakening rejection, and `CompletedReadinessEvidence` target-scope isolation. Integration tests in `tests/consumer_readiness.rs` verify selected-target execution without unrelated host executables, shared-input fanout, unknown/cyclic/incompatible-executor fail-closed guards, `verify_native_receipt_scope` rejections for full-repo/mismatched-target/stale-SHA receipts, and candidate target weakening refusal.

## Acceptance of combined instruction contracts

Use actual failing and repaired candidates for origin continuity, contradictory requests, pending spec decisions, selected roots and required behavior. Record plan, instruction, policy, candidate, target and custody identities with current observations. Verify that scope, lease or target movement invalidates the affected evidence. A green test without an accepted criterion leaves semantic alignment unresolved. The archived source and fixture receipts remain tied to their original subjects and must be refreshed for this integration.

## Release and recorded instruction checks

Version-only release verification checks canonical projections against the classified release head. The recorded-instruction readiness predicate continues to govern approval, execution checkpoints, validation, and publication. Each result retains its own measured evidence.

The legacy scope reconciliation test initializes a valid explicit path declaration
and then seeds the historical comma-split projection in its temporary journal.
It checks that repair requires a recoverable journal, preserves previous evidence,
revokes plan acceptance, and refuses independently changed constraints.

The preserved-source release regression uses an older entrypoint version
and a baseline manifest whose template hash and fingerprint come from
the manifest's release renderer. Its content hash comes from the committed
entrypoint. The test verifies the real manifest shape and repeats validation
after a second projection refresh.

## Portable encoding execution plan

The current encoding change has an actual released-binary counterexample and a source-level correction in the independent test consumer. Its existing unfiltered interoperability target will execute every pinned negative and positive vector through the real CLI and the independent Python process, retaining exact consumer versions/hashes and raw-input hashes in a versioned diagnostic report. Formatting and native checkpoint history are separate from runtime test execution. GitHub Actions supplies source verification under the maintainer's delivery instruction; pending results must not be called passing native admission or full campaign completion.

The accepted v0.154.0 package epoch (`0f44f780`) is integrated before encoding validation. The conformance harness obtains executable identity through the documented read-only `decapod system version` command; unsupported top-level version flags are not execution evidence. Native regeneration supplies the current release projections, while the independent decoder and all pinned raw inputs remain separate from authorization and full native admission.

Run encoding conformance against the v0.155.1 release base. The release metadata merge preserves all 22 raw inputs and corresponding positive controls. Retain the full core suite and the timing assertions.

The previous PR run on `08f76bff` against `2e206d26` passed both broker probes and all eleven core targets in GitHub Actions job 136580615071. The reconciliation with accepted contract recovery at `d96bf819` preserves that run as historical evidence and retains the full test suite for a fresh run. The reviewed source inventory has 105 inputs and differs from the accepted base only in the existing Bazel test data declaration.

The v0.156.0 cut at `ba21c8f1` changes only release metadata. The combined candidate also repairs the inherited strict-profile downgrade fixture. The constructor already commits the candidate repository profile while preserving the surrounding config. GitHub Actions job 136580080529 failed on a second empty commit before reaching the refusal assertion. The test now uses the committed candidate and still requires `REQUIRED_PROFILE_DOWNGRADE` after transition approval, with zero native execution selections. Keep the existing CLI and core suites, test filters, and deadlines for the fresh run.

On `7b7d83e4`, Actions jobs 136614741062 and 136614841197 both executed and passed the repaired downgrade test. The CLI suites still exceeded their aggregate budgets, and the branch core job passed encoding conformance while failing three adjacent timing gates. Reconcile the accepted approval-verification changes at `033977e2` and rerun the complete GitHub Actions suite for that base.

The v0.156.1 metadata cut at `73026d84` follows the accepted approval-verification changes. It preserves the encoding fixtures and downgrade assertion. Use the v0.156.1 evaluator and execute fresh checks on the new exact base. The inspected source inventory still differs only in the Bazel data declaration.

The accepted downgrade fixture at `9408bec1` also verifies that the committed candidate config differs from the accepted config. The encoding branch now uses those exact accepted fixture bytes. The two earlier downgrade executions remain historical evidence. The v0.156.1 PR offline suite passed 1,925 library cases on head `61a57f71` before the base advanced.

The v0.156.2 metadata cut at `31f19a70` advances the evaluator release for this branch. The encoding inputs, canonical downgrade fixture, and complete check graph carry forward. The v0.156.1 core result belongs to `61a57f71` and its accepted base. Run fresh GitHub Actions against the v0.156.2 target.

The v0.156.2 PR job 136687113651 passed both broker probes and all eleven unfiltered core targets on `351115238`. The paired offline job passed 1,925 library cases. The separate CLI job still hit two existing aggregate deadlines. With accepted context and outcome changes at `d62e30ad`, preserve native interop and authority bytes from the target and run fresh checks on the reconciled encoding candidate.

GitHub Actions run 44861445770 passed all checks on `aa0497d91`, including both CLI suites that had timed out and 1,956 offline library cases. The v0.157.0 cut at `146a00f41` changes only release metadata. Refresh the release projections and rerun the full check graph against that exact base with all 22 encoding inputs intact.

On `91d8e8361`, GitHub Actions run 44868785435 passed the two broker probes, all eleven core targets, all three CLI targets, and 1,956 offline library cases. The accepted broker and onboarding changes at `f0cbb701` then advanced the base. Preserve those accepted source and test changes and execute the full check graph again on the reconciled encoding candidate.

Standalone validation passed on `59526e792` before the v0.158.0 cut at `1bdb1e6e` advanced the base. The cut changes only release metadata. Reconcile it into the encoding candidate and run fresh GitHub Actions with all 22 encoding inputs and the accepted broker and onboarding fixes intact.

## Scoped proof recovery diagnostics and lease advisory isolation (#1892)

- **Advisory isolation**: `src/decapod/plugins/verify.rs` (`extract_from_validation_json`, `collect_structured_stderr_labels`, `collect_tokens_bounded_excluding_advisories`) excludes non-fatal `VALIDATION_LEASE_TTL_LOW:` stderr advisories from `SAFE_BLOCKER_LABELS` extraction so their warning prose never injects false-positive `VALIDATION_HISTORY_TASK_STALE` labels during `qa verify recover` / `qa verify regen`.
- **Stage/scope accuracy and wrapper peeling**: When `project_gates` pass and `src/decapod/lib.rs` mirrors readiness blockers as `"publication_readiness: ..."` lines in `report.failures`, `extract_from_validation_json` preserves `stage = "readiness"` (or `"post_validation_readiness"`) and the readiness scope rather than misclassifying the failure as `project_gates`, while `strip_error_wrappers` peels nested `DecapodError` Display wrappers and up to 6 `<ALLOWLISTED_CODE>:` layers.
- **Concrete precondition discrimination**: `src/decapod/core/trajectory/execution.rs::verify_execution_binding`, `src/decapod/core/task_journal.rs`, `src/decapod/core/todo.rs`, and `src/decapod/plugins/verify.rs` expose canonical subcodes for `EXECUTION_STATE_CHANGED` (`EXECUTION_POLICY_CHANGED`, `EXECUTION_TASK_CUSTODY_CHANGED`, `EXECUTION_MATERIAL_CHANGED`, `EXECUTION_PLAN_OR_INTENT_CHANGED`, `EXECUTION_GOVERNANCE_OR_TRAJECTORY_CHANGED`), all `VALIDATION_HISTORY_TASK_STALE` (`TASK_*`), all `VALIDATION_HISTORY_TASK_UNBOUND` (`TASK_*`), and `TASK_JOURNAL_INCOMPLETE_HISTORY` without weakening single-task custody fences or altering historical completion timestamps.
- **Regressions**: `tests/unit/plugins/verify_tests.rs` (`scoped_validation_diagnostics_ignore_ttl_low_advisory_and_expose_execution_state_change`, `scoped_validation_diagnostics_distinguish_all_task_history_and_unbound_preconditions`) and `tests/unit/core/workunit_journal_tests.rs` (`lease_renewal_exposes_custody_subcode_and_workunit_checkpoint_refreshes_without_changing_head`).

## Release-only inventory qualification, privacy preflight, stale-target exit, and headless Podman regressions (#1838, #1417, #1023)

- **Release-only `repo.pr-readiness.v1` transitions (`tests/unit/core/repository_profile_tests.rs`)**: Positive regressions cover `dactyl`-style `0.17.0 -> 0.18.0` root crate version bumps in both `ci_local = false` and `ci_local = true` modes without `policy.db` or signed approval, plus exact Git-tree vs filesystem `SourceInventory` digest parity including the `rust-toolchain.toml` symlink alias. Negative regressions confirm fail-closed `ACCEPTED_REPOSITORY_CONTRACT_CHANGED` on workflow edits, `Cargo.toml`/`Cargo.lock` dependency diffs, candidate-authored `Cargo.toml` edits, version downgrades, and missing ancestors.
- **Publication privacy & stale target movement (`tests/unit/core/publication_privacy_tests.rs`, `src/decapod/core/snapshot_provider/automatic/tests.rs`)**: Unit tests verify literal redacting matches across local `.decapod/data/publication-deny.txt` and `[governance].publication_deny_identifiers` (without self-flagging `.decapod/config.toml`) and verify `verify_pending_accepted_target` returns `STALE_ACCEPTED_BASE` when the remote target advances.
- **Linux `ETXTBSY` fixture isolation & headless macOS Podman (`tests/unit/plugins/container_tests.rs`)**: Shared `write_executable_fixture` child-process writer covers all fake runtime shell fixtures in `container_tests.rs`; `verify_headless_podman_machine_args` and `verify_headless_vfkit_args` regressions reject `--log-level=debug`, `--gui`, and `virtio-gpu`/`virtio-input` display/input devices while verifying env scrubbing and headless diagnostics.
- **README badge (`tests/unit/lib_init_prompt_tests.rs`)**: Unit test verifies `[init].readme_badge` default `true`, config roundtrip opt-out, `protected_paths` respect, and idempotent version upgrades.

## Wave 7 Shard A Verification Matrix (#1079, #1122, #1047, #1134, #1388, #1626, #1672, #1879, #1890, #1573)

- **Manifest migration & content-bumping refresh (`tests/unit/core/project_specs_tests.rs`, `tests/projection_consistency.rs`, `tests/init_validate_green_field.rs`)**: Verifies backward-compatible read fallback from `.decapod/managed/specs/.manifest.json`, migration to `.decapod/managed/.manifest.json` with legacy cleanup, deterministic `codebase-contract` updates on module additions, authored-prose preservation, and idempotence.
- **Pre-build structural & SHA-256 gate (`tests/analyze_readonly.rs`, `assets/tools/source_ci/pre-build-validate.sh`)**: Verifies `decapod validate --pre-build` fails fast on missing governance, missing manifest, or missing/invalid 64-hex entrypoint `decapod-fingerprint` SHAs without mutating the repository.
- **CI workflow, sharding, release-cut, and Nix tag policies (`tests/source_ci_profile_policy.rs`, `tests/release_metadata_policy.rs`, `src/decapod/core/readiness_profile/tests.rs`)**: Verifies all 44 finite PR job instances, balanced 3-way core and CLI shard partitioning covering every original Bazel target with `Tests (core)` / `Tests (cli)` aggregators, release-only scheduling for `test-core`/`test-core-status`/`supabase-consumer`, and native Cachix publication gating for squash/linear/merge releases and SemVer `refs/tags/v*` tags.
- **Governed docs auto-sync (`tests/docs_sync_checkpoint_boundary.rs`)**: Exercises all six lifecycle scenarios against a real bare Git remote and compiled `decapod` binary (no-drift, first PR, repeated open-PR updates, post-merge reset, validation failure fail-closed, concurrent/diverged branch fail-closed).
- **ETag conditional polling & GHA carrier policy (`src/decapod/core/snapshot_provider/github_quota.rs`, `src/decapod/core/snapshot_provider/public_buildkite/ci_attested/real_metadata_tests.rs`)**: Verifies `If-None-Match` / HTTP 304 zero-quota revalidation, fail-closed unexpected 304 handling, and `.decapod/config.toml` GitHub Actions carrier selection without Buildkite `ci_attested`.

<!-- decapod:capability-overlay:background-processing:start -->

Generated capability obligations: `background-processing` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Background Processing Validation Overlay

### Duplicate Delivery Tests
- Test repeated delivery against the declared duplicate-handling semantics, including side effects and limitations
- Idempotency key verification
- Verify the declared delivery guarantee; do not claim exactly-once behavior without proof

### Retry Tests
- Configured retry/backoff policy verified
- Configured retry bound or unbounded policy verified
- Poison-work handling verified when the project declares it

### Shutdown Tests
- Graceful drain on signal
- In-flight job completion or safe requeue
- Verify the declared durability and loss boundary on forced termination; record unsupported guarantees
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Validation Overlay

### Migration Proof Command
- Configure `repo.migration_validation.command` and its arguments as the executable migration proof; file presence is not proof
- The configured command MUST define its working directory, timeout, expected exit code, and evidence output

### Migration Tests
- All migrations MUST have integration tests
- Rollback procedures MUST be tested
- Data integrity checks post-migration

### Persistence Integration Tests
- Repository abstraction tested against real database
- Transaction boundary tests
- Concurrency conflict tests
- Data integrity validation after recovery
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-contract:start -->

## Codebase Contract Snapshot

- CI workflows (9): `.github/workflows/ci.yml`, `.github/workflows/constitution-sync.yml`, `.github/workflows/decapod-validate.yml`, `.github/workflows/docs.yml`, `.github/workflows/docs_autosync.yml`, `.github/workflows/docs_sync.yml`, `.github/workflows/nix-flake.yml`, `.github/workflows/release.yml`, `.github/workflows/security-audit.yml`
- Integration test targets (180): `tests/acceptance/publication_admission.rs`, `tests/activation_conformance.rs`, `tests/agent_eval.rs`, `tests/agent_interop_conformance.rs`, `tests/agent_rpc_suite.rs`, `tests/analyze_readonly.rs`, `tests/assets_schema_contract.rs`, `tests/assurance_harness.rs`, `tests/build_layout_policy.rs`, `tests/canonical_evidence_gate.rs`, `tests/capability_contract_drift.rs`, `tests/chaos_replay.rs`, `tests/ci_checkout_policy.rs`, `tests/ci_event_context_policy.rs`, `tests/cli_contract_enforcement.rs`, `tests/cli_contracts.rs`, (+164 more)
<!-- decapod:codebase-contract:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `307ac4647f236b100d139c0c51ab31205678f6303929749ad6230df18802a088`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (41 files), `bindings/` (9 files), `docs/` (3 files), `packages/` (4 files), `src/` (286 files), `tests/` (297 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
- Repository signal fingerprint: `f5b26099b3c76eff512a47775e0a5500ba3ed7c5bac4e4f69fd30a1358fd6205`

## End-to-End Propodus `037` Cloud Delegated-Grant Admission Across `data db` CLI & RPC (`#1568`)

Wires Propodus migration `037` Cloud repository-access (`propodus_repository_access_v1`) and delegated-grant (`propodus_delegated_grants_v1`) resolution into every live `decapod data db` CLI and `data.database.*` RPC discovery/execution path when `[repo].backend = "cloud"`, while preserving fail-closed unauthenticated refusal (`CLOUD_PRINCIPAL_UNVERIFIED`) and prohibiting any local `.decapod/data/decapod.db` SQLite open/create fallback (`CLOUD_LOCAL_SQLITE_UNSUPPORTED`):

- **End-to-end Cloud authority admission (`src/decapod/core/db_maintenance.rs`, `src/decapod/lib.rs`)**: Introduces `resolve_cloud_maintenance_authority`, `evaluate_capability_decision_with_cloud_authority`, `build_capabilities_report_with_cloud_authority`, and `execute_governed_capability_with_grant`, resolving DPoP-authenticated `connect_control` checks during both discovery (`capabilities`) and execution (`verify`, `maintain.*`, `lock.*`, `backup`, `recover`), failing closed on missing sessions (`CLOUD_PRINCIPAL_UNVERIFIED`), missing grants (`CLOUD_DELEGATED_GRANT_REQUIRED`), ungranted operations (`CLOUD_DELEGATED_OPERATION_NOT_GRANTED`), revoked/expired grants (`CLOUD_DELEGATED_GRANT_REVOKED` / `CLOUD_DELEGATED_GRANT_EXPIRED`), stale generations (`CLOUD_DELEGATED_GRANT_GENERATION_STALE`), or `grant_policy_digest` drift (`POLICY_SNAPSHOT_STALE`).
- **Cloud CLI/RPC `--delegated-grant-id` & SQLite guard (`src/decapod/cli.rs`, `src/decapod/core/rpc.rs`, `src/decapod/core/docs_cli.rs`, `docs/agent/command-contracts.md`)**: Threads `--delegated-grant-id` / `delegated_grant_id` across CLI subcommands and JSON-RPC `Database*Params` structs, guards every local SQLite maintenance runner (`run_maintain_{inspect,verify,lock_diagnose,lock_release_own,wal_checkpoint,backup}`, bounded-lock helpers, and `run_maintain_lock_retry_with_grant`) against `[repo].backend = "cloud"`, and validates the full CLI + RPC DPoP contract in `tests/cloud_wave_parity.rs`.

## Container Custody Root-UID Preflight, Supabase Connection Modes & Local PG17 Native Proof (`#1379`, `#1378`, `#1369`)

Hardens Linux container storage custody and documents the three Supabase/PostgreSQL 17 operational boundaries without weakening fail-closed checks:

- **Privileged-UID (`uid=0`) read-only store rejection & Podman custody (`#1379`, `src/decapod/core/db.rs::storage_health_preflight`, `tests/sqlite_hardening.rs`)**: Rejects directories with `mode & 0o222 == 0` under `#[cfg(unix)]` before executing `write_probe` (closing the `CAP_DAC_OVERRIDE` bypass in `uid=0` Podman/Docker containers), serializes `DECAPOD_STORAGE_FAULT_STAGE` env mutation via `RwLock<()>` in `tests/sqlite_hardening.rs`, and passes `sqlite_hardening`, `datastore_contract`, `shared_state_semantics_1247`, `storage_filesystem_tests`, `container_signer`, and `plugins::container` (`95/95`) inside Linux Podman `uid=0` `fuse`/`virtiofs` while documenting container SELinux `recovery_metadata_unsupported` and macOS `vfkit`/`AF_UNIX` signer boundaries (`docs/datastore-v2/cloud-parity-and-boundaries.md`).
- **Supabase connection modes & PG17/PostgREST native contract (`#1378`, `#1369`, `docs/book/src/reference/supabase-validation.md`, `tests/supabase_service.rs`)**: Documents `direct :5432`, `Supavisor session pooler :5432`, and `Supavisor transaction pooler :6543` (`PGRST_DB_PREPARED_STATEMENTS=false`, transaction-local `set_config(..., true)` GUCs) alongside the fail-closed hosted `MANAGED_CLI_FIXTURE_UNAVAILABLE` boundary, aligns `local_native_managed_project_revocation` with `#1896` seat-revocation diagnostics, and passes `TestPostgresNativeManagedProjectConsumer` against disposable PG 17.11 + PostgREST 16.5.

## Keep workspace creation, policy approval, container relay, and validation usable on Cloud repositories (#1886)

Completes end-to-end workspace creation and validation parity when an accepted repository selects `[repo].backend = "cloud"`, failing closed on custody/scope mismatches without falling back to `.decapod/data/decapod.db`:

- **Cloud `govern policy` admission (`src/decapod/lib.rs`)**: Admits `GovernCommand::Policy(_)` inside `is_cloud_file_only_command` so `decapod govern policy approve --scope repo.pr-readiness.v1` routes directly to Cloud `propodus_governance_v1` approvals without failing on `PUBLIC_CLOUD_BACKEND_UNAVAILABLE`.
- **Cloud workspace destination, container-relay, and nested-container guards (`src/decapod/core/workspace.rs`, `src/decapod/core/cloud_workspace.rs`)**: Enforces `WORKSPACE_DESTINATION_MISMATCH` in `ensure_cloud_workspace` when `--branch` differs from the current isolated worktree branch, admits active Cloud container relays (`is_container_relay_active()`) inside `ensure_cloud_workspace` via `check_cloud_authoring(..., true)` without host credentials or nested container launches, guards `config.use_container && !status.container.in_container` in both `ensure_workspace` and `ensure_cloud_workspace`, and rejects nonexistent branch-embedded task IDs in `admit_cloud_checkout` with `TODO_CUSTODY_REQUIRED` instead of binding an unrelated claimed task.
- **Offline integration proof (`tests/cloud_workspace_validation_1886.rs`)**: Verifies nonexistent branch task refusal, auto-discovered hosted task `workspace ensure`, `WORKSPACE_DESTINATION_MISMATCH`, non-CI `--pre-build` + `canonical_local_target = true` fail-closed `LOCAL_TARGET_POLICY_UNACCEPTED` + `ci_local` validation, Cloud `govern policy approve`, and hostless Cloud container-relay `workspace ensure --container` + `validate` without mutating or creating SQLite stores.

## Section 10 storage bounds, dogfood compaction benchmarks, Cloud parity & durability (`#1630`, `#1933`–`#1936`, `#1110`, `#1118`, `#1120`, `#1132`, `#1220`, `#1221`, `#1369`, `#1378`)

Hardens Decapod's single-datastore local SQLite guarantees, non-task store write bounds, real dogfood lossless compaction, and fail-closed Cloud boundary isolation without introducing competing storage engines:

- **Unified non-task write-admission bounds (`#1630`, `src/decapod/core/storage_contracts.rs`, `src/decapod/plugins/*.rs`, `docs/datastore-v2/logical_contract_v1.json`)**: Extends UTF-8 byte, NUL-byte (`0x00`), JSON depth (`MAX_JSON_DEPTH = 32`), and evidence-ref collection (`MAX_EVIDENCE_REFS_PER_RECORD = 128`) validation across all 10 persisted non-task plugins (`knowledge`, `federation`, `health`, `decide`, `cron`, `reflex`, `feedback`, `archive`, `aptitude`, `lcm`) and aligns `docs/datastore-v2/logical_contract_v1.json` (`36` active runtime tables including `repository_scope` plus `sql_boundary_parity`) with `storage_contracts::contract_manifest()`.
- **Real dogfood snapshot compaction & $O(N)$ event-scan fix (`#1630`, `src/decapod/core/storage_compaction.rs`, `src/decapod/core/houseboat.rs`, `docs/datastore-v2/compaction-benchmarks-v0.166.md`)**: Eliminates an $O(N^2)$ full-table-scan pagination bottleneck (`(seq > ?1 OR ...)` without a `(seq, event_id)` index) across `compact_historical_events`, `collect_event_embedded_component_keys`, and `logical_events_inventory` by paging over SQLite `rowid > ?1 ORDER BY rowid ASC LIMIT 1024`, then adds `decapod houseboat storage-snapshot-benchmark` (`DactylBridge::open_canonical(..., AccessMode::ReadOnly)` + `backup`) and measures `85,512,192` bytes saved (`16.82%`) across real snapshots of `dactyl`, `propodus`, and `decapod` (`508,391,424 -> 422,879,232` bytes) with `100%` `lossless_verified`, `todo_stream_parity`, and `task_journal_parity`.
- **Single-DB & schema reconciliation (`#1118`, `#1132`, `tests/single_datastore.rs`, `docs/datastore-v2/table-inventory-reconciliation.md`)**: Verifies all 13 subsystem initializers persist exclusively to `.decapod/data/decapod.db` with zero per-subsystem `*.db` or `*.events.jsonl` writes and documents the `46` DDL constants in `schemas.rs` vs `36` active runtime tables.
- **Cloud SQLite fallback fix, fail-closed non-todo matrix, `1933`–`1936` Cloud coverage & conflict disclosure (`#1110`, `#1120`, `#1221`, `#1369`, `#1378`, `#1220`)**: Fixes `lib.rs::ensure_session_valid` so expired cloud session renewal skips `todo::cleanup_stale_agent_assignments` on `&store_root`, threads `predecoded_rpc_request` into `is_cloud_file_only_command` (`decapod rpc --stdin`), adds `is_cloud_project` guards to `handle_store_upsert`/`handle_store_query`, enumerates 17 unsupported non-todo store operations in `cloud_parity_matrix.rs`, adds Cloud crash/clock-skew/derived-parity tests to `1934`–`1936`, preserves fail-closed `MANAGED_CLI_FIXTURE_UNAVAILABLE` in `tests/supabase_service.rs`, and records `#1220` truthfully as an irreducible conflict with `dactyl-db` SQLite custody (`docs/datastore-v2/cloud-parity-and-boundaries.md`).

## Accurate agent-facing failure recovery context and next operations (#1948)

Replaces `core::rpc::error_response`'s hardcoded `"Session may be invalid or expired"` / `agent.init` / `context_capsule: None` fallback with deterministic, state-derived recovery metadata across CLI, JSON RPC, HTTP RPC, MCP, and `interop.project`:

- **Nine-class failure taxonomy (`src/decapod/core/rpc.rs::derive_failure_recovery`)**: Distinguishes retryable read/preflight faults (`retryable_preflight_fault`), changed/stale state (`stale_state_reread_required`), repairable proof failures (`failed_proof_repair_required`), missing workspace/protected-branch custody (`workspace_custody_required`), missing/stale session or constitutional awareness (`session_custody_required`), principal-only/controller authority (`authority_required` with empty `allowed_next_ops`), uncertain post-send writes (`unknown_outcome_readback_required` directing readback before replay), invalid parameters/entities (`invalid_input`), and unsupported commands (`unsupported_no_recovery` with empty `allowed_next_ops`).
- **Pre-dispatch & handler RPC JSON envelopes (`src/decapod/lib.rs`)**: Emits structured `RpcResponse` JSON (plus trace redaction and non-zero exit status) when worktree, session, constitutional awareness, cloud admin, or handler errors fail, without inventing human approval, suggesting bypass flags, or emitting speculative `agent.init` calls.
- **Projection & MCP parity (`src/decapod/core/projection.rs`, `src/decapod/core/agent_interop/mcp.rs`, `src/decapod/core/http_transport.rs`)**: Preserves `context_capsule`, `error.details`, `blocked_by`, and `allowed_next_ops` in `ToolOperationProjection`, `mcp_denied_result`, and `http_transport::error_code_json` while keeping existing golden vectors and redaction invariants intact.

## Align Decapod Cloud CLI diagnostics, automatic repository reconciliation, and `dactyl-db 0.21.0` `OperationalDisposition` (issues #1896, #1859, #1895)

- **Automatic originating-repository reconciliation (`src/decapod/core/cloud_project.rs`, `docs/book/src/reference/cloud-projects.md`)**: Eliminates the manual post-login `decapod cloud projects` + `decapod cloud project use <owner/repo>` step when `record.selection` is unset on a valid managed session (`!is_dpop_access_token(bearer)`). `cloud_project::route` queries `AccountOperation::Status` for the originating repository slug, registers cleanly when `registration_required`, resolves the server-assigned `<org_uuid>/<repo_uuid>` project ID directly (`route_from_status`), and falls back to `reconcile_originating_repository` across the project directory only when ambiguous or unenrolled (`signup_required` for empty orgs, admin enroll/reservation CTA when `admin == Some(true)`, or collaborator administrator-contact guidance without duplicate purchase prompts).
- **Reservation, teammate policy, and `dactyl-db 0.21.0` `OperationalDisposition` handling (`Cargo.toml`, `Cargo.lock`, `src/decapod/core/cloud_client.rs`, `src/decapod/core/error.rs`)**: Bumps `dactyl-db = "=0.21.0"` from crates.io (zero path overrides), wires `DactylError::operational_disposition(OperationKind)` and `OperationalAction::{Retry, InspectAndDecide, Reconcile, Stop}` into `transport_error_for_operation`, `transport_error_for_repository`, and `admin_transport_error` (preserving `OutcomeUnknown` for post-send mutation uncertainty without false positives on pre-send/read timeouts), maps Propodus `access_disposition` and all 11 Cloud onboarding/access states, suppresses purchase CTAs when `admin_contact_required == true`, and filters `t_<32hex>` / URI credentials via `is_safe_public_org_id`.

# Intent

## CI shard unbundling, bounded-parallel GHA custody download, and next-hop release admission (`#1672` Hop 1, `#1400`, `#1573`)

Advances three linked throughput and quota reliability tracks off `origin/master` without altering `.github/workflows/*.yml` in Hop 1:

- **Bounded-parallel GHA custody artifact collection (`#1400`, `#1573`, `src/decapod/core/snapshot_provider/mod.rs`, `src/decapod/core/snapshot_provider/github_artifacts.rs`, `src/decapod/core/validation_runtime/snapshot/source_import.rs`)**: Preserves single-pass `discover_workflow` authentication and existing ETag `304` zero-quota revalidation (`src/decapod/core/snapshot_provider/github_quota.rs`), and executes distinct missing artifact ZIP fetches concurrently across up to `6` bounded workers via `Runner::execute_batch` / `OfficialCli::execute_batch` and `GithubWorkflowInventory::download_artifacts_batch` while keeping per-artifact size, `sha256:` verification, and `artifact_snapshot` parity deterministic and fail-closed.
- **Core and CLI test unbundling & Bazel parallelization (`#1672` Hop 1, `.config/build/decapod.BUILD.bzl`, `assets/tools/source_ci_profile.rs`, `tests/source_ci_profile_policy.rs`, `tests/*.rs`)**: Splits overloaded aggregates by introducing `tests/support/cli_bin.rs`, extracting `tests/effective_grant_cli_contracts.rs` from `tests/cli_contracts.rs` (removing duplicated `mod consumer_readiness`, `mod init_ci_scaffolding`, `mod init_config_behavior`, and `mod sqlite_maintenance`), extracting `tests/entrypoint_validation_contracts.rs` from `tests/entrypoint_correctness.rs` (removing duplicated `mod infer_precision`), adding missing `#[test]` / `#[cfg_attr(..., test)]` attributes to the `6` standalone tests in `tests/consumer_readiness.rs`, removing redundant `//:group_broker --exact` reruns and unnecessary `--test-threads=1` / `"exclusive"` serialization on `TempDir`-isolated targets, and balancing `selective-core-1..3` and `selective-cli-1..3`.
- **Two-hop release workflow digest admission (`src/decapod/core/readiness_profile.rs`, `src/decapod/core/init_reconciliation/mod.rs`, `tests/governance_fault_detection_1795.rs`)**: Keeps `.github/workflows/ci.yml` and `.github/workflows/release.yml` untouched in Hop 1 while admitting `reviewed_next_hop_workflow_digests()` alongside `reviewed_workflow_digests()` via `is_reviewed_workflow_digest` for the reviewed GitHub Release tarball replacement in `.github/workflows/release.yml` (`sha256:6cb40c461a58e84aa507fad13fe328d91ecb49dfca36ce4759809691f11c9310`), avoiding `axodotdev` HTTP 429 failures in Hop 2 without loosening unknown workflow rejection.

## Terminal reflection recovery & machine-only completed-task proof boundary (`#1956`, `#1957`)

Documents two non-destructive recovery workflows grounded directly in the current Decapod binary and adds real-binary integration coverage in `tests/recovery_workflows_1956_1957.rs`:

- **Terminal reflection (`REFLECTION_BLOCKED`) with uncommitted governed work (`#1956`, `docs/agent/error-recovery.md`, `docs/agent/state-model.md`)**: Clarifies that `begin-pr` rejects uncommitted work (`GOVERNANCE_UNARCHIVED_WORK`) as well as committed unmerged PR records (`GOVERNANCE_PR_NOT_ACCEPTED`), and that subsequent operator authority neither resets the stopped branch's three-round reflection budget nor substitutes for validation proof. Preserves the terminal receipt and uncommitted authored edits via archival checkpoint+commit on the stopped branch, starts a fresh workstream off the accepted target via `decapod todo add` -> `claim` -> `decapod workspace ensure`, ports only non-governance authored paths, and honors operator authority through fresh plan/execution/checkpoint/validation gates.
- **Completed machine-only operational tasks without a repository artifact (`#1957`, `docs/agent/error-recovery.md`)**: Documents that Decapod does not admit external or user-confirmed evidence without a repository artifact as `VERIFIED`, rejects fabricating `--artifact <id>=<unrelated-file>` or using `qa verify prune` (which wipes `completed_at` and reopens the task to `open`), and specifies recording operational context via `decapod todo comment` followed by policy-approved `decapod todo archive --id <ops-id>` (`task.archive`), keeping `completed_at` and unverified `task_verification` intact while clearing `PROOF_HOOK_FAILED` for unrelated code candidates.

## Section 9 CI/Publication latency, Cloud transition approval, and collector hardening (`#1400`, `#1573`, `#1833`)

Reduces GitHub Actions custody-collection latency and request volume while preserving full cryptographic proof, and extends Cloud policy-transition approvals to `ci_local` execution:

- **Inventory-backed GHA artifact collection (`src/decapod/core/snapshot_provider/github_artifacts.rs`, `src/decapod/core/validation_runtime/snapshot/source_import.rs`)**: Eliminates redundant per-artifact `verify_run` and `metadata` API subprocesses during `github_sources` collection (`#1400`, `#1573`) by authenticating the workflow run, jobs, and artifact `Metadata` (ID, size, and `sha256:` digest) once in `discover_workflow`, verifying each downloaded ZIP against those bounds via `GithubWorkflowInventory::download_artifact`, and re-verifying exact `artifact_snapshot` + `Run`/`Jobs` parity via post-loop `discover_workflow`.
- **Cloud policy-transition approval in `ci_local` (`src/decapod/core/repository_policy_approval.rs`, `src/decapod/core/repository_profile.rs`)**: Carries `DECAPOD_REPO_TRANSITION_DECISION_ID` on `ApprovalInput` and evaluates `verify_cloud` whenever `cloud_backend` is active—including `ci_local` remote CI runs—while still blocking local SQLite fallback in `ci_local` (`#1833`).

## Section 11 governance, provenance knowledge convergence, constitution integrity, phase-correct admission, and Cloud delegated grants (#657, #1774, #1772, #1763, #1367, #1568, #1735, #1944)

Completes remaining Section 11 governance and context integrity gaps across provenance-aware project knowledge retrieval/supersession, constitutional non-self-promotion enforcement, phase-correct PR checkpoint admission, Propodus migration `037` delegated-grant wire compatibility, and cross-repo trust/egress and ADR reconciliation:

- **Provenance-aware contradiction surfacing, explicit supersession, and negative-bias exclusion (`#657`, `src/decapod/plugins/knowledge.rs`, `src/decapod/plugins/federation_ext.rs`, `src/decapod/cli.rs`, `src/decapod/lib.rs`)**: Surfacing equally authoritative contradictions for legacy entries lacking `merge_key` via normalized scope (`canonical_conflict_scope`) and title/claim grouping, projecting `node_edges` (`edge_type = 'supersedes'`) for typed federation records, exposing explicit `--merge-key` / `--on-conflict` / `--supersedes` / `--scope` options on `decapod data knowledge add` and `store.upsert`, excluding persistently harmful records (`hurt >= 2 && hurt > helped`) as `negative_retrieval_bias` while ranking remaining candidates by net helpfulness, and tightening `url:` provenance validation to HTTPS or loopback HTTP without whitespace/control characters.
- **Constitutional non-self-promotion & `core/SPECS.sections.apply` integrity (`#1774`, `assets/build/constitution_admission.rs`, `tests/constitution_scaffolding.rs`)**: Rejects `authority_review` on non-binding `advisory` nodes (`CONSTITUTION_AUTHORITY_REVIEW`), rejects `sections.enforcement` / `sections.claims` assertions (`CONSTITUTION_UNSUPPORTED_ENFORCEMENT`), and enforces that `core/SPECS.sections.apply` retains the non-self-promotion admission-registry rule (`CONSTITUTION_BINDING_CONTROL_REMOVED`).
- **Phase-correct gate matrix, repair admission, and bypass classification (`#1763`, `#1367`, `tests/gate_admission_and_resilience.rs`)**: Verifies orthogonal `EvaluationPhase` x `RuleAuthority` evaluation, refusal of caller-supplied `evidence_artifacts` at Publication when opaque `TerminalEvidence` is absent, actual-binary denied -> soft-reset repair -> admitted `govern artifacts verify-checkpoints` transition, and non-admission of external unmediated commits.
- **Propodus `037` Cloud delegated-grant wire parity (`#1568`, `src/decapod/core/db_maintenance.rs`, `tests/cloud_wave_parity.rs`)**: Aligns `parse_cloud_delegated_grant_status` with Propodus migration `037` `get_grant` fields (`expires_at` RFC3339 expiry evaluation when boolean `expired` is absent, plus `policy_digest` fallback alongside `grant_policy_digest`) and qualifies `fetch_cloud_delegated_grant_status` -> `evaluate_cloud_capability_with_delegated_grant` over loopback HTTP + DPoP (`propodus_delegated_grants_v1`), failing closed on ungranted, revoked, and expired grants.

## Decapod Cloud pricing & Organization Admin documentation update (`docs/book/`, `docs_01m8bj4zfm0sm21e`)

Updates the human-facing mdBook documentation (`docs/book/`, published at `https://decapodlabs.ai/decapod`) to replace stale `"Coming soon"` / `offerings.html` copy and `https://decapodlabs.github.io/` brand URLs with live **Decapod Cloud** positioning (`$50/contributor/repo/mo`, `$0` surprise overages, multi-repo reservations) and direct navigation to `https://decapodlabs.ai/pricing` and `https://decapodlabs.ai/admin` without changing any CLI or runtime authorization boundaries:

- **Landing page, theme shell & styling (`docs/book/src/index.md`, `docs/book/theme/index.hbs`, `docs/book/src/custom.css`)**: Replaces the stale `"Coming soon"` badge and `offerings.html` links in the Decapod Local & Decapod Cloud section with `.decapod-offering-badge.badge-cloud` (warm coral/espresso) and `.decapod-offering-actions` flex layout linking to `https://decapodlabs.ai/pricing` ("Compare Local & Cloud ↗", "View Cloud pricing ↗") and `https://decapodlabs.ai/admin` ("Organization Admin ↗"), updates brand/asset links to `https://decapodlabs.ai/`, and adds compact `Pricing` and `Admin` links to `.decapod-top-links`, `.decapod-footer-links`, and `.decapod-rail-links`.
- **Guide & reference prose (`docs/book/src/introduction.md`, `quickstart.md`, `workflows/multi-agent.md`, `concepts/faq.md`, `reference/propodus.md`, `reference/cloud-projects.md`)**: Documents Decapod Cloud's `$50/contributor/repo/mo` pricing, `$0` surprise overages, multi-repo reservations, and Organization Admin management scope (subscriptions, repository enrollment, seat assignments, teammate access policies, and billing) while preserving local-first defaults and service-owned authorization boundaries.

## Scope `selective-core-2` and `selective-core-3` selection to `core_full(paths)` (`assets/tools/source_ci_profile.rs`)

Fixes a `source-ci-profile` custody failure (`required source-CI recipe is empty`) on PRs touching `src/decapod/core/*` files outside `core_full(paths)`:

- **Shard-selection alignment (`assets/tools/source_ci_profile.rs`, `tests/source_ci_profile_policy.rs`)**: Restricts `Selection::Core` selection for `"selective-core-2"` and `"selective-core-3"` in `profile::selected` to `core_full(paths)`, matching `profile::commands` (which already returns an empty recipe for shards `2` and `3` when `!core_full(paths)` while routing partial `src/decapod/core/` tests through `"selective-core-1"` and `"selective-core-status"`). Preserves full `core_full` 3-shard coverage for `core_test_inputs` and shared manifests while preventing empty-recipe aborts in `source-ci-custody` and CI shard status reporting.

## Section 12 Agent-Host Native Bindings, Executable Harness Packaging, `Arm::Static` Outcome Support, and Empirical Host Studies (#875, #1277, #1095, #880, #1888, #1711, #873)

Advances Decapod's host-integration plane while preserving one authoritative Rust governance kernel and truthful external-blocker reporting:

- **Executable harness-native discovery & packaging (`#875`, `src/decapod/core/agent_interop/registry.rs`, `src/decapod/cli.rs`, `src/decapod/lib.rs`)**: Exposes `decapod integration harness {matrix,generate,install,remove}` to write executable stdio MCP server definitions, `SKILL.md` skills (`render_harness_skill_markdown`), Claude Code `hooks.PreToolUse` entries (`decapodManaged = true`), and OpenCode `.opencode/plugins/decapod.json` hooks across supported harnesses while preserving unrelated user configs/hooks, enforcing `version_skew`, and refusing symlink targets.
- **Library-safe Rust RPC seam, `decapod-napi` native TypeScript addon & automatic Pi hooks (`#1277`, `src/decapod/lib.rs`, `bindings/typescript/**`, `packages/pi-decapod/**`)**: Exports `execute_library_rpc`, `evaluate_prompt_library`, `validate_darkmatter_snapshot_library`, and `library_version` with explicit per-caller `cwd`/`agent_id`/`session_password` (`allow_process_session_cache = false`), zero `process::exit`, zero global `SESSION_P_VAL` mutation, and cooperative `AtomicBool` cancellation (`CANCELLED`). Adds the standalone `bindings/typescript/native` (`decapod-napi`) crate compiled to `.node`, routes `@decapodlabs/decapod-client` through `native_napi` with explicit `cli_process` fallback visibility, and wires `registerPiDecapodHooks` in `@decapodlabs/pi-decapod` (`input`, `before_agent_start`, `tool_call`, `tool_result`, `session_shutdown`).
- **`Arm::Static` 3-arm outcome comparison & empirical Section 12 study (`#1095`, `#880`, `#1888`)**: Extends `src/decapod/core/flight_recorder/outcomes.rs` with `Arm::Static` alongside `Baseline` and `Governed` (`expected_arms = 3` when static runs appear) and documents the 2026-11-26 paired Antigravity `1.5.18` study alongside external credential/quota blockers for `claude` (`loggedIn: false`, `#1888`), `codex` (ChatGPT usage limit), and `opencode` (`No authenticated integrations`) without purchasing subscriptions or fabricating human attention.
- **Read-only Darkmatter & bounded A2A boundary parity (`#1711`, `#873`)**: Re-verifies Decapod's read-only Darkmatter snapshot contract (`houseboat_engine`) and bounded A2A Agent Card/Handoff validator (`agent_interop_conformance`) while keeping `DecapodLabs/darkmatter` external and preserving owner-priority deferral on HTTP/network A2A server transport.

## Admit `100755` Git mode for tracked `.sh` definition files in `executor_contract::capture`

Fixes a false `CONTROLLER_CONTRACT_UNSUPPORTED` (`accepted executor mode: assets/tools/source_ci/pre-build-validate.sh`) failure when the accepted base tracks `.sh` executor definition scripts with executable (`100755`) Git mode:

- **Targeted `.sh` blob-mode admission (`src/decapod/core/validation_runtime/executor_contract.rs`)**: Allows both `100644 blob ` and `100755 blob ` in `executor_contract::capture` strictly for `.sh` paths returned by `profile::definition_files()` while preserving strict `100644 blob ` enforcement on `.rs` executor definitions, `assets/tools/source_ci_custody.rs` protocol marker, and `verify_carrier_modes`.
- **Stale accepted-base `reviewed_sources_digest` reconciliation (`src/decapod/core/repository_profile.rs`)**: Eliminates false-positive `ACCEPTED_REPOSITORY_CONTRACT_CHANGED` (`changed_paths=[]`) failures on consumer repos (such as Dactyl `origin/main` `fa4a10b7`) when the accepted base carries a stale `reviewed_sources_digest` in `.decapod/config.toml` while `commands_preserved && candidate.reviewed_commands_digest == accepted.reviewed_commands_digest`, `base_tree.inventory.digest() == candidate_sources_digest`, `head_tree.inventory.digest() == candidate_sources_digest`, and `changed_paths.is_empty()` (`qualifies_stale_accepted_base_inventory_reconciliation`). Any candidate modification to workflows, scripts, manifests, file modes, or proof commands still fails closed.

## Scope `repo.pr-readiness.v1` inventory transitions to proof-relevant inputs, surface transition diagnostics, and route governed WAL checkpoint via Dactyl write path (#1949)

Eliminates dead-ends on routine packaging/version script changes, surfaces actionable transition diagnostics and accurate human-authorization guidance, and restores governed `db.wal.checkpoint` execution under `dactyl-db 0.19.1`:

- **Proof-relevant inventory transition scoping (`src/decapod/core/repository_profile.rs`)**: Allows `repo.pr-readiness.v1` transitions (`qualifies_non_proof_source_inventory_transition`) without out-of-band operator CLI approvals when `accepted_commands` and `accepted_contract` are preserved verbatim and every added, modified, or removed inventoried file is either an unreferenced non-workflow helper under `scripts/`, `assets/scripts/`, `assets/tools/`, or `assets/build/` (checked across multi-hop and same-directory transitive helper chains) or a routine package/container/release manifest or lockfile (`Cargo.toml`, `Cargo.lock`, `package.json`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `pyproject.toml`, `requirements.txt`, `uv.lock`, `poetry.lock`, `go.mod`, `go.sum`, `Dockerfile`, and release-automation configs) whose ecosystem is not coupled to `accepted_commands`, `accepted_contract`, or any proof-relevant inventoried file (while still failing closed on coupled manifests or directly/transitively proof-referenced helpers).
- **Pre-execution transition diagnostics & untruncated rendering (`src/decapod/core/repository_profile.rs`, `src/decapod/core/validation_rules.rs`, `src/decapod/core/validate.rs`)**: Removes manual `decapod govern policy approve` CLI instructions from `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`, preserves `verify_accepted_transition` diagnostics in `evaluate_policy_transition_with_approval` so `decapod.policy.accepted_repository_contract` emits the exact 64-hex `transition_id`, `scope`, old-to-new digests, and `changed_paths`, and prints `ACCEPTED_REPOSITORY_CONTRACT_CHANGED` failures untruncated in `render_validation_report` across both default text and `-v` modes.
- **Truthful `agent_guidance` for human-authorization-required blockers (`src/decapod/core/validate.rs`, `src/decapod/core/validation_runtime.rs`, `src/decapod/lib.rs`)**: Emits `ValidationGuidance::for_human_authorization_required()` (`owner = "human"`, `next_action = "request_human_authorization"`) instead of `"owner": "agent"` / `"repair_and_revalidate"` whenever a non-autonomously-remediable human-authorization rule or `ACCEPTED_REPOSITORY_CONTRACT_CHANGED` blocker fails validation.
- **Governed WAL checkpoint write-path execution (`src/decapod/core/db_maintenance.rs`, `tests/sqlite_maintenance.rs`)**: Routes `PRAGMA wal_checkpoint(PASSIVE)` through Dactyl's write path (`Connection::execute`) on an `AccessMode::ReadWrite` handle so `dactyl-db 0.19.1`'s `sqlite3_stmt_readonly` guard remains intact on read operations, reporting `null` for unavailable SQLite frame counters rather than fabricating zeros.

## Reconcile `dactyl-db = "=0.19.1"` release dependency & advisory floors

Reconciles Decapod against the latest released `dactyl-db 0.19.1` crate (keeping `quinn-proto >= 0.11.19`, `quinn-udp >= 0.5.16`, `rustix >= 1.1.6`, clean `cargo audit`, and synced Bazel `MODULE.bazel.lock` via `//:cargo_inputs_policy`) so `decapod` releases maintain latest-always Dactyl compatibility and security.

## Repository-shaped kernel benchmarks, bounded diagnostic telemetry, and `--device-id` hyphen parsing (#1938, #1095)

Hardens Decapod's operational observability and performance regression discipline without introducing background daemons, paid telemetry dependencies, or speculative storage engine replacement:

- **Repository-shaped whole-kernel benchmark suite & regression budgets (`#1938`)**: Adds `docs/kernel-workload-baseline.md` and `tests/kernel_workload_1938.rs` (wired into `tests/core/core.rs` and `.config/build/decapod.BUILD.bzl`) exercising fixed-seed (`0xDECA_90D4_1938_0001`) small/medium/large skewed datasets across concurrent `claim`/`renew`/`release`/conflict-retry lifecycles, cold vs warm reads, full-stream replay + `task_journal_verify`, lossless `_ce_v1` event compaction + GC + explicit page reclamation, `db_maintenance` probes, and `pre_build_validate::evaluate` timing. Documents measured SQLite growth, write amplification, lock waits, RSS, and explicit scaling activation thresholds instead of speculative engine replacement.
- **Bounded privacy-preserving execution telemetry (`#1095`)**: Adds `src/decapod/core/telemetry.rs` (`TelemetryRingBuffer`, `TelemetryRetentionPolicy`, `DiagnosticTelemetryReport`, `collect_workspace_diagnostic_telemetry`, `anonymize_identifier_with_salt`) and integrates `operational_diagnostics` into `src/decapod/core/observability_projections/telemetry.rs` (`ExecutionTelemetryProjection`). Reuses structured receipts (`BrokerEvent`, `TraceEvent`, timeline/governance receipts, and SQLite workload counts) to emit `p50`/`p95`/`p99` quantiles for `queue_wait`, `execution`, and `lock_wait`, proof freshness, denied/unknown outcome counts, and strict ring-buffer/cardinality/retention accounting while redacting prompts, secrets, raw evidence, paths, URLs, emails, and customer/repo identifiers, failing closed (`allow_identifier_observation = false`, dropping identifiers rather than hashing/retaining them unless explicitly authorized with a per-instance ephemeral salt) and unconditionally setting `can_grant_authority = false`.
- **RFC 7638 base64url `--device-id` hyphen fix (`src/decapod/cli.rs`)**: Sets `allow_hyphen_values = true` on `CloudCommand::Revoke::device_id` so valid JWK thumbprints beginning with `'-'` parse cleanly instead of failing intermittently as unknown flags.

## Wave 3 derived projection replay, compaction growth safety, and knowledge convergence (#1936, #1630, #657)

Hardens Decapod's derived-data replay, lossless storage compaction, and provenance-aware knowledge convergence without adding a second memory store or parallel event broker:

- **Canonical stream vs. derived-view inventory & replay contracts (`#1936`, `docs/derived-data-and-replay.md`, `tests/derived_replay_1936.rs`)**: Inventories canonical `decapod.db:events` streams (`events::STREAMS`) against derived task, workunit/workspace, knowledge/context, and validation-proof views; proves clean snapshot + suffix parity against resumable budgeted cursors (`task_journal::verify_with_budget`), idempotent overlapping batch replay, fail-closed gap/out-of-order/mid-read-snapshot-drift rejection (`TASK_JOURNAL_INCOMPLETE_HISTORY`, `TASK_JOURNAL_CONFLICT`, `TASK_JOURNAL_SNAPSHOT_CHANGED`), torn/corrupt `workunits/<task_id>.json` recovery (`workunit::recover_workunit`) without granting custody or publication, and uniform `task.delegation.{policy,reserve}` suffix exemptions across `src/decapod/core/task_journal.rs`.
- **Compaction growth safety, concurrency & multi-stream pagination (`#1630`, `src/decapod/core/storage_compaction.rs`, `src/decapod/core/events.rs`, `src/decapod/core/todo.rs`)**: Fixes composite `(seq, event_id)` keyset pagination in `compact_historical_events`, `collect_event_embedded_component_keys`, and `logical_events_inventory` (preventing multi-stream tied-`seq` skips at 256-row page boundaries), routes `events::{query,query_all_paged,actors}` and `todo::task_journal_verify` through `pool::global_pool().with_read` (eliminating concurrent reader/writer `SQLITE_IOERR_LOCK` races), reports candidate `unreferenced_components` in `compact_datastore_lossless` dry-run previews, and verifies skewed multi-stream compaction, concurrent reader/writer/compactor safety, pre/post-activation rollback, downgrade refusal, and index/freelist amplification bounds.
- **Knowledge convergence, pagination & procedural-promotion firewall (`#657`, `src/decapod/plugins/knowledge.rs`)**: Proves full rebuild vs. incremental `KnowledgeContext` equality across added/merged/superseded/revoked/duplicated entries, replaces the 2048-row cutoff in `read_attestations_and_feedback_on_conn` with `(seq, event_id)` keyset pagination and `events::reconstruct_stored_payload`, excludes malformed provenance (`malformed_provenance`) in `resolve_context`, and enforces `ensure_promotion_source_active` (`PROMOTION_FIREWALL_BLOCKED`) across `record_promotion_event` and `procedural/` additions so obsolete/revoked/expired knowledge cannot promote.

## Datastore and evidence encoding compatibility corpus, rollback fix, and qualification (#1937; Refs DecapodLabs/dactyl#146, DecapodLabs/propodus#227)

Qualifies Decapod's local SQLite datastore and evidence encoding compatibility across historical upgrades, floor refusals, malformed inputs, duplicate/interrupted migrations, and local-hosted logical parity:

- **Versioned fixture corpus & policy (`tests/fixtures/migration/corpus_manifest.json`, `docs/encoding-compatibility.md`)**: Adds SHA-256-pinned fixtures covering historical `v1` pre-codec stores (`v1_pre_codec_store.sql` with `schema_version: 1` inline checkpoints, verbose envelopes, duplicate `agent_prompts`, plain cold LCM summaries), stale-floor `v2` stores stamped `0.141.0` (`v2_stale_floor_0_141_0.sql`), current accepted `v2` stores (`v2_current_accepted_store.sql` with `_ce_v1`, `_ce_raw`, and `codec_version` `0`/`1` `storage_components`), `decapod.task-transfer.v1` bundles (`v2_task_transfer_bundle.json`), additive optional-field compatibility vectors (`additive_compatible.json`), version-floor policy vectors (`version_policy_vectors.json`), and fail-closed malformed vectors (`malformed_vectors.json`).
- **Atomic migration ledger rollback fix (`src/decapod/core/migration.rs`)**: Fixes `check_and_migrate_with_backup_report` so pending compiled migrations on existing datastores acquire exclusive maintenance custody + backup, snapshot `.decapod/managed/migrations/{applied,catalog}.json` and `.decapod/managed/version_counter.json`, roll back both `.decapod/data/` and those managed migration files upon failure, and skip stale `.migration_backup_*` directories during backup/restore so interrupted runs remain cleanly recoverable.
- **Logical parity without engine byte equality**: Documents and qualifies local SQLite vs hosted PostgreSQL logical schema parity (`storage_contracts`, `cloud_parity_matrix`) linked to `DecapodLabs/dactyl#146` and `DecapodLabs/propodus#227` without lowering compatibility floors (`0.144.2`) or imposing physical DDL equality.

## Live Podman container-custody & Propodus Cloud Admin wire alignment (#1379, #1859, #1886, #1896)

Resolves four defects uncovered during live Podman container-custody execution and live hosted Cloud Pilot verification against `goyxpcmxetztjlrrcwew` (`039`) on `decapod v0.163.0`:

- **Podman container-signer argument splice, socket-dir mount & `/run/.containerenv` detection (`#1379`, `#1886`)**: Fixes `prepare_container_signer_server` (`src/decapod/plugins/container.rs`) so `-v` and `-e DECAPOD_CONTAINER_SIGNER_SOCKET=...` flags splice immediately before the positional container image argument (`spec.args.len().saturating_sub(4)` instead of `saturating_sub(3)`, which previously passed `-v` as the container command and crashed `crun` with exit 127), canonicalizes `/tmp` -> `/private/tmp` and bind-mounts the dedicated private socket directory for macOS Podman virtio-fs compatibility, emits `DECAPOD_WORKSPACE_IMAGE` and `DECAPOD_CONTAINER=1` from `build_docker_spec`, and recognizes Podman's standard `/run/.containerenv` marker alongside `/.dockerenv` in `src/decapod/core/workspace.rs` and `src/decapod/core/cloud_workspace.rs`.
- **Live Propodus Cloud Admin wire contract & session-custody preservation (`#1859`, `#1896`)**: Aligns `src/decapod/core/cloud_admin.rs` (`OrgAdmins`, `OrgRecovery`, `PolicyTemplates`, `OrgUpgrades`) to live Propodus `accounts.js` `"operation"` / `"recovery_request_id"` request envelopes, gates `MutationOutcome::Uncertain -> OutcomeUnknown` in `cloud_client::admin_transport_error` strictly on mutating requests so read-only `decapod cloud orgs` billing probes gracefully handle `503 billing_not_configured`, and maps `"invalid_request"` (`CloudAuthErrorCode::InvalidRequest`) to `CloudAuthStatus::ActionRequired` instead of `AuthRequired` so HTTP 400 parameter errors never wipe a valid live human Cloud session.

## Execution telemetry projection, MCP worktree/session gate parity, observed interop receipts, real-host activation, and empirical studies (#870, #871, #872, #873, #875, #880, #1888, #1733, #1735, #1763, #1772, #1774, #1796, #1095, #1367, #1024)

Extends Decapod's local-first, daemonless observability and interoperability planes with deterministic execution telemetry, real worktree/session gate parity on stdio MCP mutations, polyglot initialization tests, multi-process cold-context handoff and TOCTOU gate-denial controls, and empirically observed host/reviewer studies:

- **Deterministic execution telemetry projection (`#1095`)**: Exposes strictly read-only, bounded, secret-redacted time-series execution telemetry (`decapod observe telemetry`, RPC `observe.telemetry`, and loopback HTTP `GET /observe/v1/telemetry`) over existing governance, trajectory, and timeline records without adding a telemetry database or default network upload. Separates independently verified useful work (`verified_useful_work_numerator`) from unreviewed human attention (`human_attention_denominator = null` rather than fabricated zero), preserves partial/cancelled runs, deduplicates duplicate event IDs, flags mismatched task/source joins, and tags evidence origin classes (`DeterministicMock`, `InstrumentedRealAgent`, `OperatorReported`, `IndependentVerification`, `RecordedKernelFact`).
- **Real stdio MCP mutation gate parity & observed conformance receipts (`#870`, `#871`, `#872`, `#873`, `#875`, `#1733`, `#1735`)**: Adds opt-in `decapod mcp serve --allow-mutations` wired to canonical local session verification (`verify_strict_local_agent_session`) and real Git worktree custody checks (`workspace::get_workspace_status`), keeping default `decapod mcp serve` fail-closed read-only (`read_only_session_denied`) while verifying `session_required`, `worktree_isolation_required`, and isolated-worktree `admitted_for_kernel_dispatch` parity end-to-end. Records real `v0.163.0` observed stdio MCP (`Content-Length` + `JSONL`), multi-process A2A Agent Card + 9-case handoff matrix, standalone Python reference consumer (`26/26` vectors), and live `@decapodlabs/pi-decapod` Node `v24.18.0` receipts in `docs/conformance/observed-interop-receipts-v0.163.md` and updates `docs/architecture/agent-interop-adr.md` honestly.
- **Real host activation, independent read-only review benchmark, gate admission, cold-context handoff, constitution controls, and polyglot init (`#880`, `#1888`, `#1796`, `#1763`, `#1367`, `#1772`, `#1774`, `#1024`)**: Captures real authenticated `codex-cli 0.169.0` baseline vs governed trials (cold `master` fail-closed refusal + prepared isolated worktree completion with 10x rejected-B negative control) and truthful unauthenticated `claude 2.1.311` (`loggedIn: false`) deferral (`docs/research/host-outcome-pilot/wave7-host-activation-2026-11-22.md`); executes a 4-task real fresh-context read-only reviewer benchmark (`docs/studies/independent-task-first-review-1796.md`, `1.00` precision / `1.00` recall vs `0.00` baseline recall); codifies the end-state enforcement table and multi-process TOCTOU/cold-context/constitution negative controls (`tests/gate_admission_and_resilience.rs`, `docs/studies/phase-gate-admission-audit-1763.md`); and adds Cargo/Node/Go/Python/mixed-monorepo polyglot `init` reconciliation and non-mutating `--dry-run` coverage (`tests/init_reconciliation.rs`).

## End-to-end DPoP device gateway lifecycle and `dactyl-db = "0.19.0"` Cloud Admin wiring (#1878, #1835, #1836, #1896, #1895, #1859, #1369)

Completes sender-constrained RFC 9449 DPoP operational transport end-to-end across Decapod Cloud and upgrades the storage/onboarding control plane to `dactyl-db = "=0.19.0"` without breaking human-bearer admin custody or touching local SQLite:

- **End-to-end DPoP device enrollment, sidecar session rotation, gateway routing, and container signing (`#1878`)**: Upgrades operational `connect` / `connect_control` calls from human Bearer sessions to repository-scoped `dpc_at_`/`dpc_rt_` DPoP device sessions backed by locally custodied Ed25519 keys, handles `401 use_dpop_nonce` challenges and proactive `REFRESH_LEAD_SECONDS` rotation over `/device-refresh`, routes data and proof-control RPCs through `/functions/v1/decapod-onboarding/cloud-{query,batch,control}`, extends the host container signer socket allowlist to `/cloud-control` while continuing to deny all device/billing/admin endpoints, and preserves human Bearer authentication on `/account-status` and Cloud Admin endpoints.
- **Full `dactyl-db 0.19.0` Cloud Admin & transport integration**: Wires all six new `AccountOperation` variants (`RepositoryEnroll`, `ResourceLimits` with graceful legacy 404 fallback, `OrgAdmins`, `OrgRecovery`, `PolicyTemplates`, `OrgUpgrades`), propagates `dactyl-db 0.19.0` problem codes (`#132`), DPoP nonce/key/audience lifecycle codes (`#133`), org session/grant narrowing codes (`#134`), paid repository seat enforcement (`#136`), explicit opt-in staged `FIRST_PARTY_CLOUD_ORIGIN` (`https://api.decapodlabs.ai`, `#138`) with verified `canonical_auth_url` / `canonical_repository_url` helpers, tenant upgrade/lifecycle diagnostics (`#139`), and validated `EffectProvenance` metadata (`#140`).

## Agent interoperability, bounded MCP/A2A adapters, offline capability/skill/rule registries, and independent conformance (#870, #871, #872, #873, #875, #878, #1715, #1716, #1717, #1718, #1724, #1725, #1733, #1735)

- **Versioned four-plane governance envelope & independent-consumer conformance (`#870`, `#871`, `#1733`, `#1735`)**: Introduces a transport-neutral `decapod.governance-envelope:v1` contract (`src/decapod/core/agent_interop.rs`, `assets/schemas/agent-interop-envelope-v1.schema.json`, `docs/conformance/governance-envelope-v1.md`) composing four independently versioned planes (`intent`, `authority`, `evidence`, `outcome`) with strict authority isolation (`imported_authority_granted = false`), replay/tenant/recipient fencing, delegation narrowing, decision-time 4-disposition projection (`admit`, `repair_required`, `authority_required`, `reject`), two-way dispatcher-vs-manifest-vs-session-guard drift detection, and observed-conformance receipts bound to binary SHA, source commit, schema/profile version, transport, accepted producer trust class, freshness TTL, and verified raw receipt bytes. Twenty-six pinned vectors (`tests/fixtures/agent_interop_vectors.v1.json`) — including summary-fiction rejection (claimed 64-hex digests without raw receipt bytes), tampered execution environments, stale commits/bases/leases, and process-vs-outcome integrity — execute across Decapod, an independent raw-JSON Rust verifier, and a standalone Python standard-library consumer (`tests/fixtures/agent_interop_reference_consumer.py`) with zero verdict or reason-class drift.
- **Bounded local stdio MCP & A2A adapters (`#872`, `#873`)**: Ships daemonless `decapod mcp {serve,schema,config}` (JSON-RPC 2.0 over JSONL and `Content-Length` stdio framing with frame/response caps, duplicate-key refusal, secret/hidden-rubric redaction, CLI/RPC gate parity on mutating tools, and conflict-safe host config merge/remove) and `decapod a2a {card,handoff}` (truthful Agent Card advertising only executable governance skills with separate core-A2A vs Decapod-extension conformance reporting, plus fail-closed handoff validation rejecting stale proof, contradictory ownership, or imported authority claims), updating `transport:mcp` and `transport:a2a` in `interop::builtin_manifest()` from `intentionally_not_used` to `implemented` while retaining honest `evidence.state = not_run`.
- **Offline capability/skill/manifest/composition/rule/overlap engines (`#875`, `#878`, `#1715`–`#1718`, `#1724`, `#1725`)**: Delivers a repo-native offline capability registry with license/provenance quarantine and cycle detection (`#1715`), deterministic token-budgeted skill discovery and A/B baseline evaluation (`#1716`), canonical manifest compilation (`AGENTS.md`/`CLAUDE.md`/`GEMINI.md`/`CODEX.md`) with source maps and policy-conflict detection (`#1717`), safe markdown fragment composition denying prompt injection, invisible Unicode smuggling, and unreviewed privilege escalation (`#1718`), conflict-safe harness packaging and an honest 6-host compatibility matrix (`#875`, keeping `live_host_verified = false` until external real-host runs exist), a machine-readable 8-domain coding-rule taxonomy/gap matrix with zero third-party product names (`#1724`), a canonical rule metadata/precedence evaluator supporting no-rule selection (`#1725`), and 6-state semantic task-scope overlap classification with separate task-local vs combined-integration proof (`#878`).

## Read-only observability projections and hardened authenticated loopback HTTP transport (#860, #874, #1712-#1714, #1719-#1720, #1723, #1727-#1728, #1730-#1732)

Provides strictly read-only, bounded, versioned observability projections (`decapod.observe.v1`) and hardens Decapod's optional authenticated HTTP RPC transport without introducing any UI, frontend state machine, telemetry database, or resident daemon (`INV-DAEMONLESS`):

- **Repository-bounded specs, governance, timeline, and live workspace projections (`#860`, `#1712`, `#1713`, `#1714`, `#1723`, `#1727`, `#1728`)**: Exposes structured, deterministic projections over existing canonical Git, `.decapod/managed/specs/*`, `.decapod/governance.json`, witnessed native path (`observed-path.json`), and read-only SQLite records (`decapod observe <specs|timeline|workspace|validation-plan|pr-manifest|context-provenance>` and read-only RPC ops `observe.*`). Double-read cross-store snapshots explicitly report non-atomicity and concurrent-edit inconsistencies (`SnapshotConsistency`), separate human-origin `original_intent` from agent-authored `refined_intent`, distinguish `Witnessed` vs `Declared` vs `RecordedFact` timeline provenance with tamper/missing-attempt detection, provide canonical `#860` workunit activity categories (`active`, `waiting`, `blocked`, `drifting`, `completed`, `abandoned`) with typed transition rules and a shared human/machine renderer, and tie bounded `SourceAnchor` file slices to exact SHA-256 hashes that turn stale immediately on edit.
- **Validation-plan applicability, prospective PR correspondence, and redacted context provenance (`#1730`, `#1731`, `#1732`)**: Surfaces applicable rules across all four authorities (`KernelInvariant`, `Constitution`, `ProjectOverride`, `TaskConstraints`), requirement decomposition coverage (catching dropped requirements such as `R3`), local vs joint/integration checks, global equality/inequality constraint-relation satisfiability (`A=B, B=C, A!=C`), prospective PR agent-intent vs published provider state correspondence and drift flags (never labeling auto-approved plans `human_approved`), and secret/prompt/reasoning-stripped context consultation provenance linked to canonical intent lineage.
- **Hardened authenticated HTTP RPC & strictly read-only loopback observability server (`#874`, `#1719`, `#1720`)**: Fixes `#874`'s reopened HTTP transport defects by binding `Idempotency-Key` to the canonical SHA-256 digest of `(method, path, query, op, body, repository_context, caller/session/tenant/repo headers, hashed token)`, refusing same-key conflicts (`409 idempotency_conflict`), bounding replay tables by both entry count and bytes with fail-closed capacity enforcement (`503`), reconciling uncertain outcomes (`409 unknown_outcome_reconcile_required`) instead of blind replay, and executing child RPC via `bounded_process::BoundedCommand::bounded_capture_with_stdin`. Adds `decapod observe serve` with constant-time Bearer auth, `--token-stdin`, loopback-only fail-closed binding, foreign `Origin`/`Host` rejection, tenant/repo/revocation checks, and strict refusal of all mutating operations (`403 read_only_enforced` / `405 method_not_allowed`).

## Obligation ledger, clarity/STE validation, escalation admissibility, offline historical exceptions, and opt-in formal obligations (#1083, #1084, #1093, #1097, #1365, #1468, #1569, #1691, #1726, #1729, #1761, #1762, #1768, #1907; Refs #1095, #1772, #1774, #1796)

- **Escalation admissibility in canonical entrypoints and embedded constitution (`#1691`)**: Aligns `src/decapod/core/assets.rs` (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `CODEX.md`) and `assets/constitution.json` (all 16 routing-pass sites and `interfaces/PROCEDURAL_NORMS`) so freshly initialized agents investigate ordinary uncertainty locally via bounded reflection (`decapod govern plan reflect`) and escalate to a human only when escalation admission confirms irreducible ambiguity, conflicting requirements, or missing authority.
- **Opt-in accepted-policy `governance.main_merge = true` (`#1468`)**: Supports explicit human-authored `governance.main_merge = true` in `.decapod/config.toml` while keeping the default `false` (omitted when serialized), rejecting candidate-branch self-authorization (`POLICY_SELF_AUTHORIZATION_REJECTED`), and failing closed on target or candidate movement after validation.
- **Offline verifiable historical-exception authority and semantic governance integrity (`#1569`, `#1093`)**: Replaces hardcoded `"exception_authorized": false` in `src/decapod/core/governance_document.rs` with offline Ed25519 verification against accepted-base `.decapod/historical-exception-signers.json` while strictly preserving `history_proven = false` for all exceptions, and enforces semantic `plan`/`claims` integrity checks (`todo_ids`, substantive intent/purpose, executable `proof_hooks`, governing `authority.source_issue`, and whitespace/timestamp theater-diff detection).
- **Derived obligation ledger, stagnation detection, outcome claims, 4D completion, canonical approvals, trusted grader, root conservation, clarity/STE validator, and opt-in formal obligations**: Adds `src/decapod/core/obligation_ledger.rs`, `src/decapod/core/artifact_clarity.rs` (`decapod validate --clarity --profile ste-lite|ste-strict|custom`, explicitly disclaiming formal ASD-STE100 certification), and `src/decapod/core/formal_obligations.rs` (disabled by default; Lean/Kani/Verus transcript & hole verification plus finite-capacity conservation state machine), alongside deterministic offline benchmark/telemetry harnesses referencing `#1095`, `#1772`, `#1774`, and `#1796` whose live paired real-agent or maintainer reviews remain external.

## Maintenance RBAC extensions, non-empty legacy TODO recovery, read-only analysis, and workspace/pre-build diagnostics (#1568, #1657, #1279, #1071, #1064, #1047)

- **Project-opted local database maintenance RBAC completion (`Refs #1568`)**: Extends `[governance.db_maintenance]` (and equivalent single-source top-level `[maintenance]` shorthand, failing closed with `AMBIGUOUS_MAINTENANCE_POLICY` when both are configured) with bounded non-destructive `db.wal.checkpoint` (`PRAGMA wal_checkpoint(PASSIVE)` under explicit allowlisting) and operator-ergonomic CLI aliases (`diagnose`, `lock-inspect`, `integrity-check`, `wal-checkpoint`) routed through the centralized RBAC gate, backed by regression tests for `BACKEND_MISMATCH`, `db.backup` mutator-retry idempotency, and unrelated-datastore lock isolation. Cloud delegated maintenance remains gated on Propodus `#88` and is not claimed here.
- **Explicit non-destructive recovery for non-empty legacy TODO stores (`#1657`)**: Extends `decapod todo journal reconcile-legacy` beyond zero-byte/byte-identical retirement to inspect non-empty legacy `todo.db` snapshots (`logically_equivalent` vs `divergent` with `legacy_only_task_ids` and `conflicting_task_ids`), export deterministic JSON bundles (`--export <PATH>`), explicitly quarantine any reviewed legacy store (`--quarantine --expected-review <SHA>`), and opt-in import only missing non-conflicting legacy tasks into canonical `decapod.db` with verified task-journal checkpoints (`--import-missing --expected-review <SHA>`), failing closed (`LEGACY_TODO_CONFLICT_BLOCKED`) on any conflict while preserving `#1683`'s inert zero-byte behavior.
- **Lower-level zero-mutation read-only analysis mode (`#1279`)**: Introduces `OperationClass::AnalyzeReadOnly` and `decapod analyze [--read-only] [--format text|json] [--max-files N] [--max-bytes N] [--timeout-ms N] [--draft-spec]` dispatched before startup migrations, sessions, or storage locks. Guarantees zero repository/Git/SQLite mutations, bounded traversal with symlink/sensitive-file exclusion, untrusted instruction prompt-safety scanning, strict fact-vs-draft-inference separation, and non-blocking governance observations.
- **Workspace effective-state diagnostic & publication planning (`#1071`, `#1064`)**: Exposes `decapod workspace effective-state --format json` (aliasing `workspace diagnose`) with top-level `dirty_files` and ordered `next_actions`, and extends `decapod workspace publish` with `--pr <N>`, `--branch <NAME>`, `--mode full|governance-only`, and zero-mutation `--dry-run` planning with explicit `REMOTE_BRANCH_DIVERGED` decision gates.
- **Local pre-build structural validation (`Refs #1047`)**: Adds `decapod validate --pre-build [--format text|json]` to catch missing/malformed `.decapod/config.toml`, `.decapod/governance.json`, living specs, agent entrypoints, and feature-branch material spec drift before build/test cost while always reporting `scope=pre_build` and `publication_ready=false` (CI workflow edits remain outside this PR).

## Multi-organization Cloud parity (#1895) and organization constitution overlays (#877)

- **Multi-organization Cloud parity without local SQLite fallback (`#1895`)**: Proves through an explicit versioned operation and evidence parity matrix (`decapod.cloud-parity-matrix.v1` / `decapod.cloud-parity-report.v1`) and a 2-organization / 2-database / 2-repo-per-org / 2-human integration harness (`tests/cloud_tenancy_parity_1895.rs`) that `backend = "cloud"` preserves Decapod logical semantics (ordered task/event/CAS claims, cross-repo `operation_id` replay rejection, stale `task_revision` rejection, commit-and-policy-bound evidence capture, and `#215` separation of org-admin views from paid per-repo seats) across organization switches, mid-session seat revocation, and network cuts without exposing physical database identifiers to Decapod or creating `.decapod/data/decapod.db`. Blanket Cloud parity is never claimed: unsupported Cloud operations (`todo.lease.renew`, `todo.claim.shared`, `todo.edit`, `todo.comment`, `todo.handoff`, `todo.archive`, `todo.transfer.export_import`, `todo.journal.reconcile_legacy`, `qa.verify.commit`, `data.database.maintenance`) are explicitly enumerated and fail closed (`CLOUD_LEASE_RENEW_UNSUPPORTED`, etc.) rather than falling back to SQLite.
- **Organization constitution overlays with explicit 4-layer precedence (`#877`)**: Establishes a deterministic, inspectable governance hierarchy linking Decapod's binary-embedded doctrine (`binary_embedded`, layer 1), organization-level policy (`organization_overlay`, layer 2, loaded from `[context.organization_overlay]` or imported cache `.decapod/managed/organization-overlay.json`), repository-local `.decapod/OVERRIDE.md` (`repository_project_override`, layer 3), and non-authoritative task context projection (`task_context_projection`, layer 4). Organizations can classify controls as `overridable`, `extend_only`, or `non_overridable`, enforce `shared` vs `organization_private` visibility, verify SHA-256 digests and Ed25519 signatures, support signed offline bundles, and fail closed when repository overrides or candidate config changes attempt to weaken or bypass higher-precedence controls.

## First-class Decapod Cloud onboarding & administrative CLI/RPC over Dactyl 0.18.0 (#1896, #1836, #1835, #1877)

Extends `decapod cloud` and JSON RPC with first-class organization, member/seat, invitation, OAuth provider identity, and billing control-plane operations strictly over `dactyl_db::AccountOperation::{Register, Status, Manage, Portal}` without touching local SQLite or waiting on an unreleased `dactyl-db` version:

- **Orthogonal org-admin vs paid-repo-seat visibility (`#215`, `#1896`)**: `decapod cloud orgs` / `cloud.orgs` list verified organizations (`organization_id`, `cloud_org_id`, `slug`, `admin`, `paid_repo_count`, `paid_repo_seat_assigned`, `org_admin_without_paid_seat`) and explicitly surface that organization governance privileges grant zero repository data access without an assigned paid repository seat.
- **Full organization custody, invite, identity, and billing control (`#1877`, `#1835`, `#1836`)**: Exposes `decapod cloud members` (`list`, `assign`, `revoke-seat`, `revoke-member`, `transfer-ownership`, `project-settings`), `decapod cloud invites` (`list`, `create`, `cancel`, `accept`), `decapod cloud identity` (`prepare-link`, `confirm-link`, `resolve-quarantine`, `revoke-link`), and `decapod cloud billing` (`portal`, `limits`), plus advancing `#1836` in `cloud_client::status_live` and `cloud_admin::billing` so `overage_cap_exhausted` sets `spend_cap_warning = true` and `CloudResponseMetadata` deprecation/sunset/diagnostic headers propagate cleanly.
- **Fail-closed secret custody, URL validation & non-retry diagnostics**: Enforces stdin-only one-time secrets (`--secret-stdin` CLI flag, zero secret CLI argv flags, `--stdin`-only JSON for RPC secret operations, `RedactedSecret` zeroization + `[REDACTED]` `Debug`/`Display`/trace/error handling), strict `https://` zero-credential portal URL validation, typed AAL2 `mfa_step_up_required` (`MfaRequired`) diagnostics, and single-send `outcome_unknown` (`OutcomeUnknown`) surfacing on post-send mutation uncertainty without `.decapod/data/decapod.db` creation.

## Narrowing effective operation grants and agent guidance hygiene (#1905, #1893)

- **Monotonically narrowing effective operation grants (`#1905`)**: Ensures every governed operation evaluates a deterministic, non-persisted `EffectiveOperationGrant` (`decapod.effective-grant.v1`) formed by intersecting accepted repository policy, server-verified Cloud access (`VerifiedCloudScope`), actor trust and active task custody, live scoped policy approvals, parent-to-child delegation bounds, and exact operation/parameter/path/resource/prestate bindings. Authority can only narrow across delegation: child requests, copied parent grants, candidate-branch policy edits, forged approval tokens, or `PlanState::Approved` can never widen scope, override explicit denials, or bypass human approval or cumulative shared-parent budget accounting across sibling subtasks. Request admission (`GrantDecision`) remains distinct from observed consequence (`ExecutionOutcomeKind::DecapodMediatedApplied` vs `WitnessedExternal`).
- **Agent-facing guidance hygiene (`#1893`)**: Removes maintainer-only development entrypoint-version-drift override settings from agent-facing managed specs, CLI help, embedded docs, and generated context capsules so agents continue treating managed entrypoints as tool-managed while keeping internal `.decapod/config.toml` validation and default drift rejection in `src/decapod/core/entrypoint_integrity/history.rs` intact.

## Composed service capability contract drift detection (#1906)

Makes cross-service capability contracts independently inspectable before governed admission so agents never rely on stale prose, guessed transports, self-reported badges, or HTTP 200-shaped client responses:

- **Focused `Decapod -> Dactyl -> Propodus` contract slice**: Models authorized Cloud task claim + readback (`cloud.task.claim_and_readback.v1`) as a versioned repo-native contract (`decapod.capability-contract.v1`) binding producer, consumer, owner, accepted source revision, input/output/error schemas, transport binding, auth scope, preconditions, side effects, idempotency, uncertain-outcome rules, dependencies, required negative cases, and evidence provenance (`src/decapod/core/capability_contract.rs`, `assets/schemas/capability-contract.schema.json`).
- **Four explicit composite consequence states**: Separates `prepared` (eligible, no effect yet), `observed` (owner poststate + independent readback confirmed), `held` (containment with `goal_completed = false` on prestate drift, shared-capacity collision, pending async work, severed ack, or blocked blind replay), and `compensated` (supported corrective operation verified with original fault retained in historical evidence).
- **Local offline repository isolation**: Repositories without Cloud binding (`backend = "local"`) evaluate offline contracts (`local.task.offline.v1`) with zero Cloud or network dependency (`local_offline_usable = true`), reporting Cloud-only contracts as `unsupported` without impairing local governance or backlog workflows.

## Operational reliability fixes: snapshot policy precheck, custody-preserving heartbeat, shared tool measurement, and Podman fixture isolation (#1881, #1837, #1884, #1838)

Integrates four targeted operational and CI-stability fixes on `v0.160.0` (`295f0af39`):

- **Container snapshot policy precheck & private staging (#1881)**: `decapod workspace ensure --container` validates canonical `db.backup` maintenance authorization before invoking container image builds (`WORKSPACE_CONTAINER_SNAPSHOT_DENIED` pending workspace when denied) and stages authorized backup snapshots under mode-`700` `.decapod/backups/container-store.XXXXXX` instead of `target/`.
- **Custody-preserving lease heartbeat (#1837)**: Decouples same-holder task lease deadline extension (`decapod todo renew --heartbeat`, `DECAPOD_VALIDATE_LEASE_TTL=renew`) from `lease_generation` rotation so long validation runs extend active deadlines without invalidating pre-existing workunit execution custody bindings (`capture_execution_custody_binding`). Plain `decapod todo renew` without `--heartbeat` retains custody-rotating semantics, and Cloud workspaces preserve `CLOUD_LEASE_RENEW_UNSUPPORTED`.
- **Bounded repeated-toolchain freshness measurement (#1884)**: Reuses per-path cryptographic measurements across multiple checks within a single `native_proof::capture_readonly_identity` freshness pass so multi-check repositories stay within strict byte budgets while preserving full check identity digests, deadlines, and cancellation checks.
- **Podman fixture writer isolation (`Refs #1838`)**: Isolates executable fixture creation in `podman_machine_start_captures_output_without_reading_terminal_input` to a waited child `/bin/sh` process so parallel test forks cannot inherit writable descriptors (`ETXTBSY`).

## Portable repository policy transition approvals and deployment-target admission (#1833, #1832)

Extends `repo.pr-readiness.v1` governed policy transitions so reviewed source-inventory and command-contract upgrades verify across local checkouts, fresh CI clones (`ci_local`), and Decapod Cloud (`dactyl-db`), while closing target-map self-authorization and incomplete target-proof diagnostics:

- **Portable CI & Cloud transition custody (#1833)**: Keeps `.decapod/data/policy.db` gitignored while admitting exact `(accepted_base_sha, candidate_head_sha, accepted_sources_digest, candidate_sources_digest, accepted_commands_digest, candidate_commands_digest)` transitions through either (1) maintainer Ed25519 signatures (`DECAPOD_REPO_TRANSITION_APPROVAL`) verified exclusively against `.decapod/repository-policy-signers.json` committed on the accepted base commit, or (2) authenticated Cloud `propodus_governance_v1` decisions (`DECAPOD_REPO_TRANSITION_DECISION_ID`) bound to the routed project, current authority generation, base/candidate commits, and exact candidate tree. Also enables toolchain upgrades that change source-inventory digests on unchanged files to be cleanly reviewed and approved across local and `ci_local` verification without permitting candidate-committed files to self-authorize.
- **Unified deployment-target & platform transitions (#1832)**: Consolidates platform-key validation helpers, requires an exact `repo.pr-readiness.v1` policy transition approval for any change to `RepositoryContract.targets` (including additions and `shared_inputs` edits), and emits `TARGET_SCOPED_PROOF_REJECTED` with unscoped `decapod validate` guidance when `workspace publish` or `qa verify` finds only target-scoped proof for `HEAD`.
- **v0.160.0 source inventory alignment**: Reconciles `.decapod/config.toml` `[proof.required_profile].reviewed_sources_digest` to `sha256:96d47fcfe781090fd7945ddb1e42a9b82ec8e7cf6498c2323d364d4a3c512975` for the `v0.160.0` `Cargo.toml`/`Cargo.lock` release metadata without relaxing `decapod.pr-readiness.v1` (`REQUIRED_PROFILE_DOWNGRADE` remains unconditionally blocked).

## Cloud Wave 2: quota/spend-cap visibility, cross-platform DPoP device custody, and SQLite-free Cloud workspaces (#1836, #1835, #1886)

Completes Decapod Cloud operational parity across error taxonomy, device lifecycle,
and workspace/validation workflows while preserving Decapod Local isolation:

- **Quota, spend-cap & provider status mapping + live policy inspection (#1836)**:
  Distinguishes `quota_exceeded`, `overage_cap_exhausted`, and `provider_disabled`
  from generic availability failures in `cloud_client` and surfaces authenticated
  included-usage and spend-cap state via `decapod cloud status --live`
  (`AccountOperation::Status` with `operation = "resource_limits"`).
- **Cross-platform DPoP keystore, signed revoke & device inventory (#1835)**:
  Attaches the enrolled Ed25519 DPoP proof signer during `decapod cloud revoke`
  (`--session-logout` DPoP session logout vs management device revocation),
  abstracts OS keystore backends across macOS Keychain, Linux Secret Service
  (`secret-tool`), and Windows Credential Manager with strict mode-`0600` private
  file fallback, exposes non-secret custody inventory via
  `decapod cloud devices [--live]`, and forwards redacted
  `Deprecation`/`Sunset`/`Decapod-Cloud-*` diagnostic warnings from
  `CloudResponseMetadata`.
- **SQLite-free Cloud workspace provisioning & validation (#1886)**: Keeps
  `decapod workspace ensure`, `decapod validate`, container isolation
  (`HostContainerSignerServer` + `HostCloudRelayHandler`), and governed plan
  checks usable when accepted repositories select `backend = "cloud"`, binding
  hosted task custody (`custody_bind_or_reconcile`) and enforcing fail-closed
  boundaries without creating, reading, or mutating `.decapod/data/decapod.db`.

## Pay-and-use Cloud onboarding and org-scoped project routing (#1877, #1896)

Decapod Cloud enables a streamlined "select Cloud, sign in, pay, and use" flow without requiring a mandatory Admin setup wizard before repository enrollment:

- **Direct repository enrollment via pricing URL (#1896)**: When a repository has not yet been subscribed (`payment_required` or `entitlement_required` / seat-required variants), Decapod surfaces `https://decapodlabs.ai/pricing?repo=<RFC3986-encoded-owner%2Frepo>` (falling back to `https://decapodlabs.ai/pricing` when no valid `<owner>/<repo>` slug is known) across CLI diagnostics, setup summaries, and JSON `enrollment_url` fields while preserving any active authenticated session. Once checkout succeeds and `backend_authorized = true` with `status = "active"`, governed operations resume automatically without an Admin setup wizard.
- **Explicit non-error-loop onboarding states (#1896)**: `pending_fulfillment` / `payment_pending` preserves the saved session and instructs callers to retry rather than restarting sign-in; `admin_approval_required` distinguishes organization admin privileges from an assigned paid repository seat; `organization_already_enrolled` surfaces the immutable Cloud organization identifier so callers ask an existing administrator for an invite or seat rather than purchasing a duplicate subscription.
- **Account auto-registration & organization disambiguation (#1877)**: `decapod cloud projects` / `project` operations register unregistered authenticated accounts (`registration_required`) via one-time account registration, surface `provider_link_stepup_required` explicitly when provider step-up is needed, and support `--org` on `cloud project use|create` to disambiguate duplicate project names across organizations.
- **Zero silent SQLite fallback**: Cloud init (both initial and refresh paths) and Cloud onboarding blocks never create or fall back to `.decapod/data/decapod.db`.

## Applicable context and verifiable outcomes

Decapod keeps the global Constitution and repository decisions durable. An agent requests a bounded projection for the current task. Explicit policy and current intent determine authority, while provenance distinguishes guidance, replaced decisions, and observed execution. The pilot records retain failed attempts and distinguish a functional result from admitted completion. (#1772, #1774, #657, #880, #1095, #879)

## Reporting claims and current proof (#1726, #1735)

Decapod remains the repo-native governance kernel; consumers must distinguish
reported outcomes from its canonical current validation and admission decisions.
Official0.148.11 reproduced a reporting false accept: `interop.project` returned
`proven` and publication-ready from fabricated pass/head/digest fields, even when
the caller supplied its own matching verifier and rule labels. No validation or
actual publication occurred. Pure report projection must instead retain the
reported status while refusing current proof, explain the missing canonical
authority, preserve negative/stale diagnostics, and permit ordinary operation.
This correction adds no validator, report generator or receipt store.

A second actual control showed approval formatting claiming that policy had
recorded a caller-supplied boolean despite no policy operation. All pure
projection inputs must be explicitly caller-owned and not canonically verified.
Reported approval/operation statuses remain display data; their formatting
cannot authenticate a principal, grant or observed effect.

## Single-header docs book layout and viewport page spacing (#1849)

The public documentation book (`docs/book/src/index.md` and `docs/book/src/custom.css`) maintains the shared mdBook navigation bar as the single site header across all pages and viewports:

- Removes the duplicate Welcome-page `.decapod-docs-brand` header strip and its theme-specific `.decapod-docs-brand` CSS overrides so navigation remains unified in the shared `#menu-bar`.
- Increases top content spacing (`#content main`) and aligns the right reading rail (`.decapod-reading-rail`) padding (`clamp(44px, 4.8vw, 80px)` on desktop, `47px` on tablet, `37px` on mobile) for balanced layout breathing room across viewports.
- This layout fix modifies human-facing mdBook content and stylesheet rules only; it introduces no runtime, CLI, RPC, schema, or security-boundary changes.

## Agent-agnostic, daemonless kernel application positioning in the docs book (#1847)

The public documentation book (`docs/book/src/index.md`, `docs/book/src/introduction.md`, and `docs/book/src/mental-model.md`) explicitly frames Decapod as an **agent-agnostic, daemonless metacognitive governance kernel application** rather than an agent harness, prompt framework, or background daemon:

- Coding agents across harnesses (Codex, Claude Code, Cursor, Antigravity, and other compatible tools) invoke Decapod on demand through its CLI and structured RPC (`decapod rpc`) interface whenever work crosses an orientation, task-custody, workspace-isolation, boundary, or exact-commit validation gate.
- Each invocation terminates cleanly without leaving a resident background daemon (`INV-DAEMONLESS`), while the repository (`.decapod/` and Git history) preserves durable governance state, intent, constraints, and proof across agents, models, processes, and sessions.
- This clarification updates human-facing documentation prose only; it introduces no runtime, CLI, RPC, schema, or security-boundary changes.

## Cloud control-plane, Dactyl 0.18.0 adoption, and Decapod Local/Cloud unification (#1809, #1808, #1769, #1667, #1777, #1778, #1590, #1033, #1034, #1038, #1040, #1066, #1158, #1369, #1376, #1377, #1380)

Decapod adopts published registry crate `dactyl-db = "=0.18.0"` (SHA-256
`6c26eaf28272a50517d7fe300b56940ac03627477b48579293b231c33de905b4`, preserving
`quinn-proto >= 0.11.19` and synchronized `Cargo.lock` + Bazel `MODULE.bazel.lock`)
without path or Git overrides, and unifies all user- and agent-facing CLI,
capabilities, embedded docs, and human book documentation around **Decapod Local**
(`local`, free, offline, no account required) and **Decapod Cloud** (`cloud`,
account required):

- **Decapod Local/Cloud branding & docs (#1808, #1769)**: Retires the legacy
  Vercel preview URL default so unconfigured explicit `neon`/`supabase`
  datastores fail closed with `DECAPOD_CLOUD_API_URL` guidance. Introduces
  canonical `DECAPOD_CLOUD_*` env vars while retaining `DECAPOD_PROPODUS_API_URL`
  as a backwards-compatible alias emitting structured `decapod.deprecation.v1`
  notices, recognizes `decapod.cloud.store` (`v1`) envelopes and `Decapod-Cloud-*`
  diagnostic headers, and adds a responsive **Decapod Local** vs **Decapod Cloud**
  offerings section on `docs/book/src/index.md` linking to
  `https://decapodlabs.github.io/offerings.html` without public pricing or
  checkout routes.
- **Mixed-worktree routing & inline cloud parity (#1777, #1034, #1158)**: CLI
  and inline todo helpers resolve backend selection (`local` vs `cloud`) and
  canonical repository identity strictly from the invoking worktree rather than
  falling back to the root checkout's SQLite store in either direction
  (local-root/cloud-worktree or cloud-root/local-worktree), while honoring
  `#1776` accepted-cloud fail-closed enforcement against unaccepted local downgrades.
- **Device-bound DPoP custody & container signer broker (#1778, #1033, #1040, #1066)**:
  Dedicated per-origin/per-repository Ed25519 device keys are persisted outside
  Git (OS Keychain where available, otherwise mode-0600 `O_NOFOLLOW` external
  private file), wired into `dactyl-db 0.18.0` `RequestProofSigner` (RFC 9449
  `DPoP`) and `AccountOperation::{DeviceChallenge, DeviceEnroll, DeviceStepUp, DeviceRevoke}`,
  plus `decapod cloud logout|revoke` and a bounded host-side Unix-socket
  container request-signer broker (`[workspace.credentials]`) so container
  workspaces never receive private keys or raw bearer/refresh secrets.
- **Repository access tiers & owner repair (#1590)**: Local `db.recover`
  requires merged canonical `owner_repair_enabled = true` and RBAC `owner`
  authority; cloud repair queries `propodus_repository_access_v1` (`Read` +
  audited `Write`) via `CloudConnector::control_request` and executes
  `owner_repair_v1` atomically via `DactylBridge::atomic` with zero local
  SQLite fallback.
- **Hosted lease fencing & 5-state proof acceptance (#1809, #1038)**: Removes
  the `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED` placeholder in favor of hosted
  exclusive lease fencing/renewal/release and wires `propodus_proof_acceptance_v1`
  / `propodus_proof_acceptance_get_v1` (`captured -> independently_verified ->
  currently_admissible -> published -> historically_accepted`) with safe
  `outcome_unknown` reconciliation and distinct `403`/`413`/`429` error types.
  Offline/loopback and local PG17 + PostgREST runs prove client wire and native
  database contracts without claiming hosted production acceptance or unrun Nix
  builds on hosts where `nix` is absent.

## Accepted release12 continuity

The accepted target advanced again to `7bd0a915` (0.148.12), which changes only the root package versions and changelog after9c. The prior9c merge remains a real parent with its own checkpoint; this release integration gets a new native checkpoint. All105 verification-source inputs were independently remeasured: Cargo.toml/Cargo.lock version bytes changed and103 remained identical. Required helpers, graph, commands and toolchain remain unchanged. Official session acquire installed a private0.148.12 controller (SHA256 `1fb9e592076c937f0be049f2329b287e22f2e6a346b8f351ec863bfc92a6d7d3`) and restored the temporary config opt-in; old/shared binaries remain intact. Native projection refresh does not transfer earlier Actions success or establish current admission.

## Current accepted-target integration

The active currentness task integrates accepted master `9c7e6cff` through a real two-parent merge after the external acceptance of PR1791. Its strict JSON source, test compile inputs and authored contracts are retained alongside this task's current proof obligations. Earlier352-based source results remain historical; the combined candidate requires new execution against9c. The native active journal stays with this task; Git preserves both parents and all accepted history. No merge admission is inferred from the external ref update.

## Accepted interoperability workstream context

## Unambiguous client requirements

Interop discovery must not silently erase a caller's required semantics.
Malformed JSON and duplicate decoded keys are refused before RPC startup on
both stdin and flag-parameter routes. Ordinary unique-key requests continue.
The existing unique-key decoder is reused from release-manifest validation;
focused cross-language parsing controls make no new conformance-receipt,
independent-executor, authority or publication claim.

This follow-up starts from accepted release `352d8929` (0.148.11), after
PR #1787 was merged by another actor while the parser extension was prepared.
The earlier unaccepted source and checkpoints remain in their original Git
history. This new governed task reauthors only the reviewed duplicate-key
correction. All 105 verification-source inputs were independently measured;
only root package versions in Cargo.toml/Cargo.lock changed from 0.148.10.
The remaining 103, workflow graph and declared toolchain are unchanged.
Source-inventory reconciliation is not execution or publication proof.

The first Actions evaluation built the candidate as 0.148.11 and passed its
191 project checks, then correctly refused release-projection drift: the
managed Docker header and manifest still described controller 0.148.10.
The official session updater installed a private 0.148.11 controller; native
refresh/project-check now owns those regenerated release surfaces. The
configured proof graph is unchanged. Host-only container and shared-task
proof blockers remain separate from portable source-CI execution and prevent
any claim of native admission. Metadata repair is not a passing runtime test.

## Checkpoint history recovery

This fresh governed workstream preserves PR #1788 and immutable source
`d53835ad2ee573996edcc31ef1ce2ef577a9a7cc`. Exact native inspection relative
to current accepted `352d8929` found one missing checkpoint, after the agent
committed a release merge before its asynchronous checkpoint completed. Eight
earlier authored commits passed that same inspection. The native assessment
of the older original baseline is retained separately with its complete
findings. Neither assessment is relabelled as a passing history.

After the present-time recovery archive was committed, the native resolver
accepted only its current archive/reauthoring disposition. The original gap
remains historical and unproven. Official 0.148.11 refreshes the current managed
Docker pin and manifest release metadata; its installation used the supported
session updater in a private location. Shared-host prerequisites are not
publication proof, and fresh applicable Actions execution is still required.

The source branch and failures remain preserved. This task reauthors only
reviewed workunit code, meaningful execution tests, source inventory and
semantic prose from the accepted target, with a present-time native history
recovery observation. It imports no prior governance, custody, success or
publication authority. A late checkpoint cannot repair the old commit; fresh
current checkpoints and independent execution must establish this candidate.

## Accepted release source review

This currentness workstream starts from accepted `352d8929` (0.148.11).
The preserved original branch integrated that release through a real
two-parent merge. All 105 verification-source inputs were
independently measured, including declared symlink target bytes and their
separately inventoried regular target. Only the root package versions in
Cargo.toml/Cargo.lock changed from 0.148.10; the remaining 103, required
workflow/executor graph and declared toolchain are unchanged. Reconciliation
retains the full configured proof policy and does not establish execution.

## Observed proof remains current only within its execution contract

A genuine prior success must remain inspectable as history while a newer full
not-ready judgment withdraws current workunit verification. Safe repair must
remain available: fresh complete proof for the exact current task and renewed
custody can restore VERIFIED. This correction uses the existing validation
history and workunit journal; it adds no alternative proof producer. The CLI
regression executes real required Python checks and an explicitly declared
external failure control. It proves bounded currentness and recovery, without
claiming complete external-input closure, forge publication admission, or an
independent agent/host.

## Actual-boundary workunit proof controls (#1660, #1735)

The combined candidate incorporates accepted release `34f837d4` (0.148.10).
All 105 verification-source inputs were independently remeasured: only the
root package versions in Cargo.toml and Cargo.lock changed from 0.148.9;
the remaining 103 inputs, workflows, required graph and toolchain are intact.
The reviewed inventory is reconciled accordingly. A scoped 0.148.10 controller,
installed through the official session updater, owns current projection refresh
and validation. Its actual binary digest is retained separately from candidate
evidence. Controller installation and release integration are not test proof.

The ordinary recovery path must retain the workunit's genuine task-bound
context capsule and policy lineage. The source-file counterexample prepares
that capsule through the public query and attach-state commands before
requesting proof, so missing context cannot substitute for the producer test.
Full validation uses its current runtime observation while the workunit is
CLAIMED; it does not require a prior VERIFIED projection. A generic terminal
rule refusal is not evidence of a circular proof dependency.

An agent-declared passing result and a real source file's digest must remain
diagnostic declarations. The existing canonical full-validation producer owns
VERIFIED admission. The campaign's actual-binary control reaches an admitted
task worktree and current execution checkpoint without gate-skip variables;
recording a source-file pass succeeds, while VERIFIED is refused without the
canonical producer record. The journal retains the declaration and CLAIMED
status. This confirms the bounded guard, not the issue's entire proof matrix.
Fresh trusted proof, split-store recovery, replay and publication remain
separate acceptance obligations.

## Autonomous safe config recovery and privilege custody (#1771, #1776)

The cloud boundary suite treats embedded schema discovery as file-only recovery,
including CLI and RPC reads. `tests/common/cloud_managed_scenarios.rs` compares
the complete project and private machine file inventories before and after each
read, and requires zero HTTP requests. Unsupported operational `store.query`
still exercises the cloud startup guard and must refuse local SQLite fallback;
schema availability does not authorize operational persistence.

First adoption on a verified accepted Git tree that predates repository policy
uses the compiled global approval defaults for ordinary operations. The resolver
confirms absence in that exact accepted tree, never an older ref or candidate
fallback. Candidate RBAC, shared-claim and maintenance grants remain denied.
Failure to read or verify accepted policy remains an error. Test fixtures must
establish their accepted baseline before executing candidate-branch operations.

PR #1779 previously used an explicit user exception for draft creation before
full validation on 2026-11-17. That historical exception does not authorize
another workstream, establish current admission or grant merge authority.

Agents must be able to discover the embedded configuration contract even when
their proposal is malformed, repair it in governed isolation, and resume
ordinary work. They cannot use schema validity, a repinned candidate policy,
or a changed base selector to acquire new exceptional authority. This change
reuses existing parser, branch-safe maintenance policy and publication owners.
It preserves accepted-policy rotation and revocation without repetitive human
confirmation. Real-host activation (#880), unified diagnostic (#1071), and
real-forge current admission (#1763) remain separate executable acceptance
obligations; parser/RBAC regression results cannot stand in for those results.

The proof-source inventory is reconciled against its last reviewed source
`edbfc00d`: only the package version in `Cargo.toml` and `Cargo.lock` advanced
from 0.148.7 to 0.148.9. The candidate preserves every workflow, command,
coverage obligation and configured execution provider while acknowledging
those release metadata bytes. Reconciliation does not establish test success.

## Truthful conformance discovery (#871, #1735)

Compatibility discovery must preserve required semantics at the parser boundary.
Discovery also reports the existing session requirements for context bundle
export/import, mentor obligations and assurance evaluation. Automatic session
acquisition does not make an operation session-free. Every RPC descriptor is
checked against the existing entry guards; read-only database inspection retains
its earlier bounded session-free route. No enforcement permission is changed.

The official 0.148.10 counterexample refused a valid unsupported extension but
accepted its trailing-comma flag form after silently replacing the parameters
with an empty object. Flag parameters now fail on invalid JSON before startup;
omitted parameters still mean an empty object. Ordinary supported negotiation
and valid unsupported-extension refusal remain available. This is an input
interpretation correction, not authority or executed-conformance attestation.

The combined candidate incorporates accepted release `34f837d4` (0.148.10).
All 105 verification-source inputs were independently remeasured: only the
root package versions in Cargo.toml and Cargo.lock changed from 0.148.9;
the remaining 103 inputs, workflows, required graph and toolchain are intact.
The reviewed inventory is reconciled accordingly. A scoped 0.148.10 controller,
installed through the official session updater, owns current projection refresh
and validation. Its actual binary digest is retained separately from candidate
evidence. Controller installation and release integration are not test proof.

The current RPC surface must also advertise `intent.reconcile`, `state.resolve`
and `state.diff`. The real entry boundary rejects unregistered operation names
with `unknown_op` before project/session startup. Registration and metadata
come from one declaration in the existing interop catalog; registration grants
no authority and preserves every registered operation's own gates. This bounds
undiscoverable additions without claiming an authenticated conformance producer.

The source-inventory review compared all 105 discovered inputs against the last
accepted review `edbfc00d`. Only the root package version in Cargo.toml and
Cargo.lock changed, from 0.148.7 to 0.148.9. The reviewed digest acknowledges
those release inputs; workflows, helpers, toolchains and required coverage are
unchanged. This review is not evidence that any required check executed.

Harnesses may discover implemented operations and their source test coverage
without an execution claim. The built-in interoperability manifest reports
`not_run` even where a registered conformance test and source location exist.
`core::interop::validate_manifest` rejects declared `verified` and `failed`
results with `unobserved_conformance`: the current manifest format supplies no
authenticated producer receipt. Ordinary negotiation remains available.
This closes the static-label counterexample only. Current binary-bound observed
conformance, independent consumers and the rest of the campaign remain required
before #871 or #1735 can be considered complete.

## Complete scoped source currency (#1753)

The current follow-up also repairs the source-version policy's currency boundary.
Canonical source headers explicitly permitted by config must not be forced into
a version-only rewrite by another gate. The exact canonical repository and real
source lineage remain mandatory; generated manifest/Docker provenance and all
execution checks remain current. No user edits of managed entrypoints are used.

## Post-integration proof boundary

The implementation was accepted externally in merge 41e7e152 with the exact
c35eb608 Git tree, then released as 0.148.7 at 9d1aed27. That forge transition
does not turn the stale prior native validation into admission. The current
proposal must validate its own head against the newly accepted base;
runtime changes are limited to the scoped currency correction above; required
checks remain enforced. Strict live
acceptance 07 additionally repeats the complete sequence on evaluating release
0.148.7; its receipts retain their own exact controller and fixture subjects. The release-only Cargo.toml/Cargo.lock version changes require a
reviewed source-inventory update; no workflow, command or coverage is removed. See docs/publication-admission-acceptance.md for exact identities.

## Current dependency policy (#1755)

A current required dependency failure reopens delivery even after historical
readiness passed. Decapod replaces the unmaintained `chrono` dependency with
`time`, without an advisory exception. Fresh source and live-forge proof must
cover this candidate before delivery issues are closed. The reviewed source
inventory changes only for Cargo.toml, Cargo.lock and MODULE.bazel.lock; accepted
commands, workflow adapters, requiredness and coverage remain intact.

During development of Decapod itself, an explicitly configured internal policy
can separate entrypoint release-version churn from the source package version.
Its scope is the exact canonical Decapod repository and source lineage; valid
canonical template bytes and fingerprints remain mandatory. It does not renew
historical execution evidence or weaken exact-artifact publication proof.

## Measured strict live acceptance

The current real GitHub acceptance uses the repaired timestamp dependency and
requires actual source-policy rejection, excluding a timeout as proof. Its
immutable receipts, exact source/head/base identities, quarantine and stale-target
controls are recorded in docs/publication-admission-acceptance.md. The final
source delivery independently requires fresh full exact-candidate validation.
Historical controller and fixture successes do not authorize a later artifact.

## Evidence-bound PR admission (#1747, #1748)

An agent earns PR admission only through complete live proof for the actual
candidate, accepted target and required policy. `PublicationReadiness` carries
that existing proof through the publisher; a saved historical success cannot
authorize a different present artifact. A target advance or changed source,
governance, execution tools or task custody requires fresh applicable proof.
PR admission is an observation for exact head/base identities, not authorization
to merge later. Failed post-creation admission preserves a separate rejection
and verifies that the matching PR is quarantined as a draft or already closed.

## Do What Must Be Done (#1690–#1692)

Maximize useful autonomous engineering per human intervention. Agents must
investigate discoverable information, make defensible engineering decisions and
verify uncertainty before delegating cognitive work to the human. The existing
orientation/mentor reflection protocol now accepts typed escalation proposals,
observes source predicates and policy/approval state, and challenges inadequate
proposals within its lifetime three-round budget. Human intent, explicit
authorization and exceptional decisions retain authority. The kernel neither
reasons as an LLM nor directly questions the human. Semantic exhaustion and
completed engineering work remain unproven by deterministic reflection receipts.

## Required test dependency coverage (#1672, PR #1684)

Test-only repairs must select the existing core or CLI lane that executes them.
`assets/tools/source_ci_profile.rs` includes the runtime and CLI recipe inputs,
shared consumer fixtures and Bazel declarations in that selection. An absent
selection is not successful test execution. This delivery retains the accepted
finite check IDs and complete recipes. Independently bounded shards, compilation
reuse and the five-minute timing requirement remain open under #1672; this
coverage prerequisite does not claim them. Exact-candidate validation remains
required after integrating accepted v0.145.3 source.

## Concurrent custody and diagnostic scope (#1469, #1661)

Independent tasks can author concurrently in distinct admitted worktrees. A task
lease has one live checkout binding; transfer, retirement and renewal preserve
fencing. Explicit shared-resource reservations govern integration and release
operations rather than excluding unrelated source work. Non-Git projects can
run bounded prerequisite diagnostics without being admitted for authoring or
claiming publication readiness. This repair retains the accepted master policy
and Decapod 0.143.1 release contract; it does not complete the broader ten-worker
or autonomous-runtime acceptance mandate.

## Evidence-grounded reflection before consequential action

Decapod helps agents reconsider a proposed action through optional bounded
reflection in orientation and mentor. It challenges a conflict with the current
plan or a literal ADR match, then recognizes an inspectable scope correction or
separately resolved governing gap. The ordinary orientation path remains simple.
Reflection is a reasoning aid: it establishes neither human authorization nor
semantic correctness, executed proof or completion. The kernel remains local,
daemonless and deterministic, with no model or new memory service.
See `core/mentor/reflection.rs` and `docs/architecture/bounded-reflection.md`.

## Published SDK adoption (#1638)

The client pins the normally published registry `dactyl-db =0.16.1` release
without changing local/cloud composition, session custody, role authority or
transport selection. Its patched `quinn-proto >=0.11.19` requirement must hold
for every resolved lock entry, including inactive dependencies. This adoption
does not enable HTTP/3 or establish hosted service acceptance.

## Governed cloud client and private custody (#1628)

The human runs `decapod init` once per repository and then talks to the coding
agent normally. The agent owns every subsequent Decapod invocation, including
sharing a browser sign-in link, bounded continuation, private session reuse,
refresh and explicit safe recovery. The user completes provider/account actions
without operating the CLI again. Pending sign-in is an honest incomplete
connection, not a reason to lose the user's task or create a duplicate task.

The agent carries the engineering request through Decapod's intent, task,
workspace, execution-history, validation and publication interfaces. It owns
sequencing and supported recovery, and returns an inspectable proof reference
bound to the checked source. The user supplies the outcome and required human
decisions, without manually orchestrating the command lifecycle. Decapod's
governance and configured validation remain the authority for admissibility
and readiness; an agent's completion message is not evidence.

Decapod's local-first governance kernel remains the system interface for agent
work. Optional cloud selection changes the operational task backend without
moving authorization policy into the agent or turning credentials into Git
state. `repo.backend = local|cloud` is the only project-level composition
choice. Dactyl owns discovery, physical connector selection, authentication
transport, account-completion transport, and cloud data I/O; Decapod owns
machine-private session custody and the bounded human-facing continuation.

The client must reuse valid sessions, serialize refresh, preserve interrupted
one-time outcomes, and expose pending/denied/action-required outcomes honestly.
Only service-confirmed completion plus an authenticated repository read permits
the init-ready message. This establishes current read access, not future write
or privileged authority. Local mode requires no cloud account. The implemented
task slice does not add full remote governance parity, pricing policy, hosted
PostgreSQL acceptance, or a release exception for an unpublished dependency.
Source: `core/cloud_client.rs`, `core/cloud_session.rs`, `core/todo.rs`, and
`lib.rs`. Fixture evidence and protected real-service evidence stay distinct.

## Current bounded-reader repair (#1571)

Restore ordinary local validation liveness on macOS by enumerating pinned
filesystem handles directly. The change must preserve digest equivalence,
streaming bounds, no-follow identity checks, and genuine failure results. It adds
no authorization exception, publication waiver, new datastore or cloud behavior.

## Rust consumer source-spelling coverage

## Product Outcome

Decapod keeps AI coding work tied to the human request, the repository's rules,
and evidence of what was actually checked. Its product outcome is work that
people can inspect, agents can continue, and the project can accept through
explicit governance gates.

Nix packaging proof must exercise the built Decapod binary independently of
checkout-local governance state. Shared CI checkout permissions must not require
loosening runtime storage security to smoke-check a package.

The harness chooses what to attempt and the agent performs the work. Decapod
resolves canonical state, decides whether the attempt is admissible, and judges
whether the resulting claim is proven. It does not choose the next action. A
passing verifier is not, by itself, an independently constrained verifier.
Each isolation axis needs its own usable evidence reference. A native-trace
label, a self-report, or a harness name does not prove that axis, and evidence
for one axis does not prove another. The verifier must also have reviewed the
same input digest as the completion evidence. `not_proven` stays distinct from
`failed`, and `unknown` stays distinct from `proven`. A capability is `stale`
only when a usable evidence reference is bound to a different governing
identity. An unknown judgment is not automatically a state blocker.

Concurrent local agents and worktrees must retain their own caller identity in
results, task ownership and audit evidence while sharing canonical storage.
The foreground broker's optional reusable connection must make bounded progress,
end with its owning foreground invocation, and hold transaction locks only for
actual database work. Migration and restore retain exclusive file-lifecycle
protection. An uncertain acknowledgement must never silently repeat a mutation.
Long repository paths must route through a real confined endpoint on Linux and
macOS; unavailable transport is an explicit error. Required broker acceptance
checks returned and persisted ownership, physical opens, lifecycle failures and
normal CI execution. This work does not merge, release, or close #1476.

The current follow-up repairs a demonstrated public Buildkite artifact-origin
compatibility defect in the released v0.121.0 controller. Accept the documented
exact HTTPS artifact host alongside the legacy bucket, preserving producer,
path, checksum, output and deadline checks. This independently qualified
controller prerequisite retains accepted master's existing source-bound CI API
attestation policy and every required source/native check. The original stricter
derived-release branch and its failed collection remain preserved until an
independently supported release can resume them; this fix does not close the
broader #1070/#1398/#779 or four-platform packaging requirements.

The maintainer delegated the functional safe-abstention decision after seeing two
meaningful ready controls approved and the third validly abstaining. Functional
delivery requires genuine approval on each tested backend and every adverse or
protocol control; a ready control may safely abstain, with its frozen expected-
accept mismatch preserved explicitly. This is no claim of perfect oracle agreement
or domain accuracy. Abstention still blocks readiness for that work. No production
threshold, model, prompt, numerical tolerance or deterministic gate changes.

The record must preserve what was asked, how the agent understood it, what
scope was accepted, what changed, and what remains unproven. That record stays
with the project when a conversation, process, model, or agent harness changes.
It connects human intent to agent work, tool execution, produced artifacts, and
the evidence and decisions needed for delivery.

## Human and agent experience

The human installs and initializes Decapod in the project:

```bash
cargo binstall decapod && decapod init
```

They then give work to their coding agent, review its understanding and results,
and decide questions that require human judgment. The agent owns operational
use of Decapod: orientation, Constitution retrieval, task coordination,
workspace setup, specification authoring, evidence collection, validation,
and supported recovery.

The agent also takes on the development work needed to deliver the request:
understanding the project, planning, configuring build and test tools and CI,
implementing changes, repairing failures, and preparing work for review within
the agreed scope and authority. Decapod steers that lifecycle through project
rules, durable state, checks, and actionable feedback. Validation is one phase
of the work.

The human should not need to operate the governance CLI for each coding task.
Unclear intent, conflicting requirements, missing permission, and unavailable
proof must become clear questions or blockers rather than hidden assumptions.

When an agent changes machines, Git history alone does not establish local task
ownership. Explicit task-scoped transfer preserves the original identity, intent,
revision and relevant source history after the source yields and seals custody.
The receiver accepts a private artifact through a trusted operator handoff and
must regenerate machine-specific evidence. This bounded local mechanism covers
standalone tasks; connected/shared work and global exclusivity across disconnected
stores require further authority rather than an invented replacement claim.

## User and Actor Contract

- The human supplies intent, constraints, authorization, and judgment
- The agent interprets the request and repository, plans and performs the
  work, maintains specifications, and records evidence through Decapod
- Decapod maintains governance and coordination state, resolves supported
  project authority, checks invariants, and gates supported transitions
- The repository preserves source, specifications, accepted history, and
  durable records needed for later review and continuation

An agent-authored specification exposes the agent's understanding. Its presence
does not establish that the interpretation is correct. Likewise, a passing
check supports the behavior that check covers; it does not prove all possible
properties of the product.

## Scope

### Intent and context before implementation

Agents consult Decapod before committing to a design when intent, boundaries,
interfaces, or proof requirements are uncertain. The embedded Constitution,
project overrides, orientation, and scoped context help the agent identify
the next useful question or action. Resolved authority and context sources
remain inspectable.

Agents can draft work while a governing instruction is still unresolved.
Approval, execution, validation, and publication require a clear recorded plan.
When a recorded source resolves a question or contradiction, the revised plan
requires reapproval and affected proof must be run again. Pending spec decisions
require the explicit review-resolution transition.

An accepted change to selected-work policy requires a new approval before work
can continue. The same plan may be approved again after the repaired policy is
accepted. A decision that genuinely requires human authority still follows the
existing decision process.

### Bounded, coordinated work

Tasks, claims, sessions, and isolated workspaces establish who owns the work
and where it may run. Concurrent agents must not silently acquire one another's
assignments or overwrite the main checkout. Existing issue trackers may remain
the organizational record; Decapod coordinates accepted repository execution.
When agents publish a GitHub PR, the change title and description come from
explicit input or the branch's authored commits. Ambiguous commit history
requires an explicit title so reviewers can identify the intended change.

Completed work must remain tied to the task that was actually verified.
Changes to task intent or custody cannot inherit a prior passing result; the
kernel exposes stale evidence and requires supported revalidation before that
work can again authorize completion or dependent execution.

### Durable understanding and continuity

Living specifications describe the product. Plans and trajectories describe
the current work. Claims state conditions that evidence can support or falsify.
Knowledge and context records preserve useful project information across
sessions. These records must retain unresolved assumptions and obligations so
the next agent can continue without inventing a clean history.

Checkpoint creation must preserve that history at the mutation boundary. A reused
identifier cannot replace earlier evidence with revised work. Identical retries
remain safe; changed work gets a new checkpoint and independent validation.

Interrupted execution must distinguish recoverable state from current permission
to continue. A journal can restore the exact bytes and custody that it recorded;
it cannot turn missing history into observed history, or refresh old proof merely
by replaying it. Checkpoints of pre-journal state are explicitly migration
baselines. Resume compares current source, intent, plan, and applicable authority
with the recorded binding and fails closed on drift. This continuity is an
on-demand kernel operation, not a scheduler or background retry service.

### Evidence and accepted work

Agents define proof expectations, run the required checks, preserve results,
and repair recoverable failures. Decapod binds supported evidence to the task,
repository state, policy, and validation epoch. Governed publication remains
blocked while required checks, evidence, or approvals are unsatisfied.

Completion after integration is a separate decision from publishing new work.
An agent may finish its original task using durable full validation evidence
for the exact candidate commit, provided the task's intent and custody remain
continuous and that candidate's integration is independently established.
Advancing the accepted branch does not erase that historical execution.
Historical completion must not require a new pending PR or another agent's
workspace, and it must never authorize publication of a different candidate.
Missing historical proof remains missing; a green hosting-provider badge or
the legacy tracked prerequisite receipt cannot reconstruct it.

Obligation completion must remain true when inspected from a fresh session:
current transitive dependencies, proof outcomes, proof freshness, and a nonempty
state-commit root determine readiness. Cached status is advisory. Verification
must expose missing or corrupt evidence as a failed outcome. Publication checks
explicit completion attempts and their dependencies without requiring unrelated
unattempted backlog to be complete (`core::obligation`, `core::validate`).

### Local operation with explicit integrations

Release automation may create an ordinary branch at the accepted target before
authoring a change. That event must retain its verified identity without becoming
a candidate, a release exemption, or publication proof. Source changes on that
branch still require the normal immutable-range checks.

## Acceptance Criteria

The implementation and its tests must establish that:

1. Project intent, local authority, and the agent's interpretation are available
   for review before publication
2. Required session, claim, workspace, and boundary checks reject invalid
   transitions with actionable diagnostics
3. Durable state and required lifecycle evidence commit together; stale
   updates and ownership conflicts remain visible
4. A later invocation can recover current work, its evidence, and unresolved
   obligations from supported repository records
5. Required proof cannot be replaced by a completion claim, generated
   fingerprint, provider probability, or unrelated successful check
6. Validation distinguishes failure, missing evidence, partial proof, and
   readiness for the exact candidate and accepted target
7. Recovery preserves user work and historical evidence, stays bounded, and
   escalates conditions that the supported interface cannot resolve

For container execution, recovery includes retaining failed or interrupted
workspaces and successful local-only work whose commits were not returned.
Failed commands cannot trigger publication. Successful PR requests use the
same governed publication proof as other workspaces, and host foldback cannot
replace divergent commits. Automatic cleanup protects dirty work, tracked work
hidden by index flags, and added or changed refs omitted by branch foldback.
Ignored successful build outputs remain intentionally disposable. A reported
retention reason is recovery information, not evidence that the command passed.

The concrete proof surfaces are defined in [VALIDATION.md](VALIDATION.md).
These expectations specify behavior; this document is not a test receipt.

## Constraints

Decapod does not supply a model's intelligence, write the implementation on
the agent's behalf, or replace the coding harness and human review. It governs
operations routed through its supported interfaces; it cannot prevent an
agent from issuing unrelated shell, Git, or service commands outside them.

Deterministic records and checks do not certify arbitrary natural-language
meaning. Local session custody does not authenticate an external model
provider. Local fixtures do not establish hosted tenancy, deployment, or
live-container behavior. No fixed full-validation latency or service-availability
SLO is asserted here.

Requests that change consequential scope or contradict the accepted contract
require a decision. A recoverable check failure requires more work. Neither
condition is completion.

### External engine tooling

Decapod's proof boundary does not operate Dagger engines. Agent skills and
project CI may own that tooling outside the kernel, while native receipt
custody, receipt-independent prerequisites and required Buildkite execution
remain governed by their existing contracts. Retired Dagger configuration must
fail explicitly rather than silently discarding a required check.

## Continuous Intent Traceability

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
The agent must preserve the human-originated change intent while proposing
updates to living specifications and implementation. Explicit requirement
identities may be attached to authored specs across changes. The read-only
reconciliation report exposes missing or orphaned references, unresolved
questions, and outdated source/spec snapshots without declaring semantic
alignment or granting publication authority.

## Immutable Change Impact

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
The agent must be able to inspect what implementation and authored living-spec
material changed between an exact accepted Git base commit and the current HEAD
commit, without permitting source observations to rewrite human intent. An
unmapped code change remains explicitly unmapped, incomplete or unsupported
inputs remain unproven, and the harness owns any proposed code/spec repair.
The kernel retains independent review, validation and publication authority.

## Bounded native storage inventory (#1630)

Measure Decapod, Dactyl and Propodus dogfooding stores before selecting a compact
representation. Native CLI/RPC inspection exposes exact counts, explicit bounded
samples and redacted size/reuse observations. Retain all stored history and the
accepted managed-cloud behavior. This PR supplies the measurement prerequisite;
field contracts, representation, migration, retention and full domain parity
remain coordinated work requiring measured budgets and further proof.

## Laya readiness scope

The sole model evaluator is optional Laya assessment after deterministic proof.
The kernel remains daemonless and local-first; model installation and inference
are not required for ordinary disabled-mode operation. No retired scientific
corpus, calibration threshold or old runtime implementation is current policy.

## Optional team publication gate

Teams opt into `[laya] mode = "required"` with the reviewed model manifest and
private runtime registration. Current deterministic proof remains mandatory;
only an affirmative Laya decision may permit an already-authorized PR step.
Rejection, abstention and missing/stale evidence retain repair/revalidation
instructions. Assessment never grants merge or release authority.

## Supplemental Rust worker verification

### Diagnostic collection and unresolved acceptance (#1445)

Native diagnostics must be executable before independent parity and readiness evaluation are available. Missing reference/holdout inputs are publication acceptance gaps, not dependencies of a device tensor probe or model forward pass. The agent must produce public, reproducible Rust verification inputs and auditable agent-curated repository cases; these labels are not human-reviewed ground truth. Source/native diagnostic success alone must not trigger feature publication or a completion claim.

## Validation Rules System

Decapod enforces governance through a unified, rule-driven validation system that
extends the Constitution and OVERRIDE.md authority model. Rules translate
applicable principles into mechanically checkable requirements.

The intent is to make governance mechanically enforceable while preserving the
existing authority hierarchy. Rules translate constitutional principles into
mechanically checkable requirements with structured remediation contracts.

Key intent:
- **Mechanical enforcement**: Governance violations are detected and reported
  automatically, not reliant on human review of natural language.
- **Structured remediation**: Every blocking rule provides actionable repair
  instructions distinguishing autonomous repair, human authorization, and
  evidence production.
- **Anti-bypass**: Kernel invariants cannot be waived by candidate-controlled
  policy changes. Override eligibility (NonOverridable, TightenOnly) is enforced.
- **Context awareness**: Rules declare applicability conditions; custody rules
  only apply in governed agent workspaces with active task claims.
- **Structured remediation**: Every blocking result provides actionable repair
  instructions with stable error codes.
- **Three-verdict system**: PASS/FAIL/UNVERIFIABLE with UNVERIFIABLE never
  treated as PASS.

The system extends the existing Constitution/OVERRIDE.md authority model:
Constitution (built-in defaults) → OVERRIDE.md (project overrides) → Task
constraints (governed plan) → Resolved effective rule set → Validation.

Seven built-in rules cover the existing governance responsibilities:
scope enforcement, protected paths, task ownership, workspace isolation,
evidence freshness, proof-backed completion, and file-touch budgets.

Overrides are declared in OVERRIDE.md under a dedicated VALIDATION_RULES section
with TOML-formatted fenced blocks. Override eligibility (NonOverridable,
TightenOnly, ConfigurableOnly, FullyOverridable) is enforced per-rule.

The validation rules gate runs as part of `decapod validate` after override
authority resolution and before project specs validation. Results include
policy fingerprint and validation epoch binding.

The system is accessible via `decapod validate`, RPC (`validation.rules.list`,
`validation.rules.evaluate`, `validation.rules.get`), and OVERRIDE.md.

## Validation Rules System Implementation Status

The validation rules system has been implemented with the following components:

- **Rule Registry**: Manages built-in rules, project overrides, and task constraints with proper precedence
- **Seven Built-in Rules**: Covering scope enforcement, protected paths, task ownership, workspace isolation, evidence freshness, proof-backed completion, and file-touch budgets
- **Actual Evaluators**: Connected to existing Decapod evaluators (scope validator, gatekeeper, workspace, evidence validator, workunit validator)
- **Three-Verdict System**: PASS/FAIL/UNVERIFIABLE with UNVERIFIABLE never treated as PASS
- **Structured Remediation**: Every blocking rule provides actionable repair instructions with stable error codes
- **Override Integration**: OVERRIDE.md support with TOML-formatted fenced blocks and override eligibility enforcement
- **Context Awareness**: Rules declare applicability conditions; custody rules only apply in governed agent workspaces

### Implementation Quality

- All validation rules unit tests pass (8/8)
- Clippy clean with `-D warnings`
- Cargo fmt clean
- Actual evaluators connected (no placeholder Pass returns)
- Override eligibility enforced (NonOverridable, TightenOnly, ConfigurableOnly, FullyOverridable)
- Anti-bypass protections: kernel invariants non-waivable, candidate-controlled policy changes rejected

The validation rules gate runs as part of `decapod validate` after override authority resolution and before project specs validation. Results include policy fingerprint and validation epoch binding.

## Immutable CI context and release projection continuity

Readiness must use the candidate's committed target configuration and immutable
provider change range. A genuine version-only release may refresh exact generated
projections without inventing an authored requirement change; every other source
change retains the authored-spec obligation. Product implementation and new regression coverage remain Rust-native;
the accepted CI carrier is unchanged. Repeated header refresh must preserve project-authored
Dockerfile bytes and must not introduce whitespace that makes the independent
projection check disagree with the actual validator. This focused #1397 delivery
contributes to #1052/#1069/#1070; it does not by itself prove their full lifecycle,
platform, hosted-custody or derived-release acceptance scope.

## Independent task execution and kernel bootstrap

Independent tasks must acquire custody before their worktrees exist, including
basic-trust agents modifying overlapping logical source files. Task identity is
not title similarity, a category, or a source directory. `core/todo.rs` keeps
claims independent while `core/authoring.rs` admits the actual actor's observed
checkout against a current lease. A named mutable resource still requires its
own exclusive custody. Source overlap requires integration reconciliation and
combined validation; it never proves completion by itself.

A correctable prompt-evaluator invocation error must not become a policy denial
or an allowance. The complete original prompt still needs successful evaluation.
Recovery and publication retain trust, exact approval, fencing and independent
proof requirements. Unsupported cloud profiles remain explicit and cannot fall
back to local authority. See `docs/architecture/concurrency-custody.md` for the
implemented boundaries and remaining hosted/native integration limits.

## Governed recovery without retrospective proof

Agents must be able to continue legitimate work when earlier immutable commits
lack their own checkpoints. The supported outcome is a new, properly admitted
workstream with a present-time gap observation and fresh proof, while the
original source history remains unproven. This change adds no trust elevation,
human-approval assertion, database maintenance privilege or publication waiver.

## Datastore efficiency mission

Preserve the minimum durable operational authority and evidence required for
local-first governed work. The first storage increment removes a structurally
redundant event index while preserving exact historical records and uniqueness.
Its benchmark reports disk allocation, fixture-only reclamation, WAL and latency
separately. It does not establish full cloud parity, hosted capacity, new retention
permissions or the overall 30% storage goal. Subsequent changes require independent
measurements and compatibility proof, as sequenced in `docs/datastore-v2/architecture.md`.

## Laya-only source cleanup

Remove retired model implementation, compatibility fields, fixtures and stale
documentation from the current tree. Preserve canonical proof/event bytes and
historical Git commits. Resolve obsolete carried obligations through the
supported governance CLI; historical resolution identifiers remain auditable
until their accepted merge allows normal workstream compaction.

## Relational index compaction intent (v0.136.4)

Preserve all Decapod domain rows, stable IDs, proof bindings, journal history,
and local isolation while eliminating nonunique indexes duplicated by existing
UNIQUE btrees' leftmost-key coverage. Do not delete rows, rewrite events, alter
WAL settings, run VACUUM, weaken integrity, or silently fall back from cloud.
This is a bounded local SQLite optimization. Full semantic parity with
Propodus Supabase remains a separately gated migration (Propodus #71).

## Refined terminal onboarding and managed cloud setup

Human setup follows Project, Preferences and Review, with recommended defaults,
focused customization and an explicit apply/edit/cancel decision. Interactive
setup creates no project, Git, governance or cloud sign-in state before approval.
People using an inherited terminal should still be able to cancel with Escape
when a process cannot open its controlling-terminal device.
Existing settings and full proof definitions survive refresh, with private
backups before changed config replacement. Local operation needs no account.

Cloud setup uses the accepted Dactyl-managed session flow after apply. The service
owns pricing, account requirements, assignments and repository access; the TUI
does not infer authority from a local roster. A pending or denied handoff is not
setup success. Managed-project selection and the protected paid customer journey
remain separate work and are not claimed by this presentation change.

## Explicit managed project routing

Managed cloud commands must distinguish the local Git checkout from a service-issued project reference. `cloud projects` lists authenticated project names; `cloud project use` selects an exact name or a previously shown entry; `cloud project create` explicitly materializes a project through the existing idempotent account contract. Selection does not grant access. Existing session custody and local operation remain independent of this account-routing cache.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## PR 1666: accepted v0.143.2 source integration

Reduce anonymous GitHub discovery consumption while preserving exact-source proof, immediate denial behavior and the original invocation deadline. Quota hints govern waiting, not acceptance; this proposal does not establish current provider execution.

## PR1666 current-master integration

Incorporate accepted master 9b900254, including custody, protected-path/workunit fixes and the 0.144.0 source release. Preserve the quota-pacing feature and its original PR history, then repair hosted CI failures one at a time. No local test execution or independent full readiness is claimed.

## PR1663 integrated storage behavior

Integrate PR1663 on accepted master5e23ad68 and0.144.1 while preserving storage admission, lossless history and format obligations. Keep accepted custody, quota and runtime-owner changes. Historical benchmark claims remain historical; only fresh hosted execution can establish this candidate's checks.

## PR 1662: accepted v0.143.2 source integration

Expose only project-opted local database maintenance capabilities to the agent. Repository policy and exact operation authority govern diagnostics and safe maintenance; a project flag does not confer recovery, raw SQL, cloud privilege or task/checkout ownership.

## PR1662 current-master preparation

Preserve prepared PR1662 capability-scoped database maintenance before integrating accepted master3afa9df9 and the0.144.2 source release. Default-deny accepted policy, owner-only repair, bounded safe operations and non-destructive lock custody remain obligations. Hosted CI must establish the exact integrated candidate; no local tests are run.

## Independent collector closure (#1573)

Merged1666 supplies bounded anonymous GitHub quota pacing. Preserve required
provider/source/repository identities, explicit access denial and the invocation
deadline while reconciling actual complete collection evidence. Official0.144.3
completed the complete1658 provider contract without identity substitution. This
follow-up records the actual observation and separately validates its own task
and current candidate; a prior task's proof never completes this task by itself.
No new credential path, provider fallback, receipt authority or local tests.

## Required maintenance evidence completion (#1568)

The local-only capability engine merged in1662. Its standalone
`tests/sqlite_maintenance.rs` target was not explicitly part of the existing
required source recipe. Register those actual storage/CLI/RPC controls in the
required CLI contract target without substituting an adapter mock or changing
provider authority. Native SQLite absence must fail these required cases.
Cloud policy/delegation remains separately enforced by Propodus; local opt-in
and forged principal environment never establish authenticated cloud authority.
Hosted execution and exact-candidate readiness remain pending.

## Published SDK convergence (#1667)

Adopt the current published `dactyl-db =0.16.1` registry artifact, preserving
SQLite/Neon/default Supabase features and application-owned governance decisions.
The official sparse index reports non-yanked checksum
`1b41dde2e8be3fc986f7f5a7eb4e5bfb9af49d4c39dabe98fd74a0ccb4ad7ac6`;
the downloaded crate matches it. The 0.16.1 patch has identical `src/*.rs` bytes
to 0.16.0 and repairs upstream test compatibility. Version 0.15 adds bounded
resource accounting and scans; 0.16 adds logical connectors/session transport.
These additions do not confer application authority or independently prove a
hosted deployment. Exact candidate compatibility remains a hosted verification
obligation, with no local test execution.

## Diagnostic overrides cannot authorize readiness (#1659)

Git, tooling and fingerprint gate-skip variables support diagnostic fixtures;
their presence must never authorize full readiness, publication or verified
completion. The shared runtime must reject a full claim before prerequisites or
provider selection, and CLI/RPC/publication paths must preserve that boundary.
Existing CI-local and project-prerequisite diagnostics retain their explicit
partial scope and `publication_ready=false` behavior.

Use the existing judgment, validation-history and receipt owners. Keep historical
observations intact and require a fresh applicable validation after removing an
override. Real child-process regressions isolate environment changes and must
verify both rejected full claims and truthful diagnostics. No local test result,
current hosted success or completed repair is claimed by this plan.

## Observed workunit proof convergence (#1660)

The existing `record_proof_result` pass row is a declaration: it initializes both
proof epochs to None, and `ensure_verified_ready` currently checks only pass rows.
Durable execution binding and referenced-byte hashes preserve custody/integrity
but do not authenticate who observed a successful check. Whole publication has
additional gates, so misleading VERIFIED state is not by itself a proven forge
merge bypass. Preserve that distinction in real binary adversarial controls.

Converge VERIFIED with the existing canonical `validation_history` producer.
A current canonical observation must bind this exact task, workunit contract,
source, accepted target, policy and tool closure. Add only bounded references and
read-only applicability inputs to existing proof records, not a second datastore
or evaluator. Runtime terminal judgment must consume actual completed checks;
opaque additional gates remain unproven. Legacy declarations retain history and
require fresh observed proof, with explicit safe downgrade/recovery. No new
cryptographic trust root or OS-isolation claim is inferred from shared UID access.
No local tests run; supported real CLI/runtime and hosted proof remain required.

## Mixed-store recovery (#1657)

Provide explicit, versioned and idempotent recovery for a nonempty legacy TODO
store beside valid canonical task-journal state. Reuse datastore lifecycle,
maintenance authority and journal verification. Preserve original files, task
identities, chronology, custody/proof bindings and unknown outcomes. Diagnose
equivalence, divergence and corruption independently; do not implicitly merge
contradictory state, delete legacy input, reconstruct missing proof, or replay an
unknown mutation. Ordinary operation resumes only after an inspectable safe
recovery result. The empty-stub path is a bounded first repair; general nonempty reconciliation
remains unimplemented. Hosted regressions must exercise real store and recovery boundaries.

## Storage convergence follow-through (#1630)

Audit the current merged compact journal/event/component and bounded inspection
implementation before adding representation changes. Preserve canonical logical
hashes, custody, snapshot-consistent continuation, historical oversized values
and explicit physical-maintenance authority. Use supported native read-only
inspection for the Decapod, Dactyl and Propodus dogfood stores. Publish aggregate
measurements with binary/schema/codec identity and sampled/complete coverage;
these observations are not before/after savings or hosted PostgreSQL proof.

Hosted tests must cover any repaired inspection or reconstruction boundary.
Existing historical fixture benchmarks retain their original release/environment
scope. Cross-backend, scale, migration and deployment requirements remain open
until supported executable evidence establishes them. No live store repair or
physical compaction, policy weakening, local CI or payload publication is part of
the measurement pass.

## Accepted local verification requirements (#1658)

Decapod must preserve the repository-local verification contract already accepted
on the target. A candidate may add required checks, but replacing a command,
arguments, measured toolchain, environment, timeout or coverage declaration must
not become authoritative merely because the candidate recomputes a review digest.
`repository_profile` owns this preservation check; `validation_runtime` must apply
it before selecting an execution provider. A previously accepted strict profile
continues to require its existing external assurance.

The dedicated change restores the preserved repair against v0.144.3 and adds
real-process consumer regressions for repinning, additive checks and stale source.
Initial adoption remains supported. Requirement replacement needs a separately
accepted target-policy transition; this work introduces no candidate-side override
or new authority store. Hosted tests and exact-candidate validation remain pending
until their actual observations are collected; no local test result is claimed.

## Versioned mixed TODO-store recovery (#1657)

Add a supported explicit review/reconciliation path that can run despite the
mixed-store migration guard. Preserve canonical journal, both source histories,
ownership, proof bindings and unknown operation outcomes. Retire legacy input
only when the kernel independently proves that its authority is already present
in canonical state. Never replay a guessed task mutation or manufacture missing
history. Corrupt, changed, unsupported or contradictory inputs remain blocked.

Use existing physical coordination, task-journal verification, bounded evidence
and authoritative project policy. Local cooperative role labels remain distinct
from authenticated cloud authority and OS isolation. Recovery must be versioned,
inspectable and safely resumable after interruption. This work does not grant
historical exceptions, cloud maintenance rights or verified task completion.
Hosted checks provide execution evidence; no local tests or Buildkite runs are
part of authoring, and full native publication readiness remains unproven.

## Nix publication after squash releases (#1626)

Preserve Decapod's governed engineering and independent publication boundaries
while repairing Nix packaging eligibility. Both existing Nix gates must recognize
protected-master squash/linear release commits by their own release subject and
ordinary two-parent release merges by the second parent's release subject.
Unknown/root/octopus shapes and non-release subjects must not publish. Keep the
native GitHub-hosted ownership, protected ref/event restrictions, credential-free
selector, locked packaging proof and separate Cachix publisher unchanged.

Use actual Git histories to test the workflow scripts through Rust policy tests.
Hosted-only verification is required; no local tests, Buildkite queries or
automatic merging. This change does not claim that a historical missing Nix cache
entry has been populated: actual native packaging and authenticated publication
remain separate observations after acceptance.

## Canonical local-target proof (#1069)

Extend the existing repository-local verifier so an explicitly accepted local contract can read the canonical repository's accepted Git branch without a forge. The normal CLI must execute its reviewed checks through the sealed host and preserve exact source, accepted base, integration, tools, policy, and task custody. Candidate-only target opt-in and strict-profile or cloud fallback must fail closed. Local shared-user authority and separate remote publication remain explicit limits; the complete ten-worker A–H demonstration is still unproven. Hosted regression execution, rather than local test runs, will establish this slice's evidence.

## Decapod Labs media and documentation stewardship (#1707)

The independent Decapod Labs website owns the canonical visual identity and
original website marketing-media renderer. The Decapod kernel repository carries
only reviewed, generated copies of the approved mascot (SVG/PNG), animated
README GIF, product MP4 and poster. It must not add a marketing rendering
workflow to the kernel's source-CI workflow inventory.

The kernel's mdBook documentation is a separate technical product surface,
published at `https://decapodlabs.github.io/decapod/`. Its landing page uses
the approved Decapod Labs wordmark, official kawaii mascot, coral/cream visual
palette and the existing product video, with a direct link to the company
homepage. The technical manuals remain technical and preserve precise
definitions of scope, authority, proof and unsupported guarantees.

The landing page must not claim an unconditional sandbox, semantic correctness
oracle or hosted-cloud acceptance. No docs theme change, image, video or
successful media build satisfies Decapod's native validation/readiness gates.

## Documentation identity across the technical manual (PR #1741)

The documentation shell must present the canonical Decapod Labs identity on
all mdBook pages, in addition to the existing introduction content. The
persistent sidebar brand area uses the website-owned `assets/logo-a.png`
wordmark (dark Decapod text with strong-orange Labs text) beside the official
crab mascot when the light theme is selected. Dark mdBook themes substitute
the website-owned `assets/logo-c.png` wordmark for contrast, retaining the
same mascot. The landing-page company link uses `logo-c.png` and remains
accessible by its Decapod Labs name. The docs favicon remains the reviewed
mascot icon.

The public asset paths belong to `DecapodLabs/decapodlabs.github.io` and are
referenced by the technical book without duplicating or regenerating brand
artwork inside the kernel. Both palette variants must remain visually legible
at narrow viewport widths. A missing remote image must not prevent access to
the manual, its links, search, or code examples. Branding changes do not modify
authority, readiness, proof guarantees, or the scope of documented commands.

Acceptance requires inspecting the generated sidebar and landing page in both
light and dark palettes, confirming the correct asset references and accessible
names, and independently checking the publicly served Pages result. Passing a
source build alone does not establish that the public deployment occurred.

## Rust-native public documentation shell redesign

The public documentation remains generated with the reviewed mdBook v0.4.40
Rust binary and the canonical Markdown in `docs/book/src/`. Replace the default
mdBook visual shell through its supported `docs/book/theme/index.hbs` template
and project CSS without introducing an Astro, Node, npm, or external application
build system. Preserve the existing chapter filenames, document links,
search implementation, theme selection, print output and keyboard navigation.

Create a distinct Decapod Labs interface with a full-height navigational
chapter rail, branded sticky top navigation, roomy editorial reading surface,
contextual quick-reference rail at wide viewports, and a contrasting company
footer. The official mascot appears in the sidebar and reading rail. The
approved `logo-a.png` serves light navigation, `logo-c.png` serves dark
navigation, and `logo-b.png` serves the dark footer. Warm cream, the light
coral `#F19C76`, vivid orange `#F05B37`, restrained plum, generous rounded
surfaces and tactile card treatment form the visual identity. Maintain proper
focus indicators, legible code and tables, narrow-screen sidebar behavior,
contrast and reduced-motion preferences. The original product film remains
on the landing page without introducing autoplay with audible sound.

The browser presentation is explanatory documentation for humans and external
readers; the installed Rust binary retains authority over agent-facing,
versioned operational instructions. Theme assets and layout never authorize
changes to scope, governance policy, completion or evidence. Success requires
an actual mdBook build, structural checks on the rendered navigation and links,
responsive review and independent observation of the published Pages site. A
source build alone does not establish publication.

The report's manifest and managed Docker pin must be generated by the actual
0.148.7 evaluating binary. An earlier CI-local pass with 0.148.6 projections was
not full readiness. Only supported CLI regeneration repairs that metadata;
the current candidate still requires complete fresh proof before publication.

## First-class BYOA and agent-invoked metacognition documentation (#1789)

Make the public mdBook and README landing experience accurately express how
engineers and coding agents use Decapod in practice: humans prompt their chosen
coding agents (Codex, Claude Code, Cursor, Antigravity, or custom harnesses);
coding agents invoke Decapod at decision, custody, workspace, and validation
boundaries; parallel work coordinates claims, leases, and worktrees while
preserving Git/review/CI conflict boundaries; the embedded Constitution acts as a
subordinate reference canon inside the governed loop; and both `backend = "local"`
and `backend = "cloud"` keep governance state repo-native.

Three concrete reader journeys (switching agents mid-repo, two engineers working
with different agents in parallel, and recovering from failed validation on an
exact revised commit) plus explicit negative cases (conflicting claims, expired
leases, unregistered/malformed RPC, stale proof, and Cloud denial) are grounded
in the `0.148.14` binary CLI/RPC help and existing integration tests.

## Independently observed autonomous engineering (#1757, #1758)
An external coding agent's statement of completion does not prove its actions.
The existing flight recorder gains a read-only, digest-bound acceptance export
over actual host, kernel, Git and forge observations. The objective is a real
unattended failure, kernel rejection, agent diagnosis/correction, fresh required
proof and exact-subject publication. Scripted coverage, kernel authority,
cooperative observation and independent GitHub policy remain distinct. Safe
exports can drive website-owned launch media; the kernel never renders video,
authenticates arbitrary capture labels or imports publication authority.

## Real-agent acceptance and release-bound projections
The issue1758 Pi experiment is actual inference, not a deterministic repair imitation. The read-only observer must preserve its real required-check failure, new model-requested repair after returned feedback, fresh exact-subject native proof and independent GitHub readback. Subsequent source/base controls retain historical admission while clearing current applicability. Operator probes, PR creation, observed admission and merge authorization remain distinct. Source implementation readiness is separately required; a successful fixture export cannot excuse a stale managed container pin or failed source CI. Regenerate release-bound surfaces through native Decapod, checkpoint and rerun full configured validation after release movement.

## Current continuous-governance audit (#1069)

Refresh the existing implementation map against accepted Decapod0.145.6 master
without changing the kernel, validation policy or publication authority. Separate
accepted code from pending PRs, and source-bound executed observations from
unexecuted requirements. Cover exact proof, observed workunit acceptance,
diagnostic overrides, workspace/custody, local readiness and cloud independence.

Keep missing native runtime integration, complete ten-worker A-H proof,
independent exception/cloud authority, unsupported workspace-store convergence
and five-minute sharding explicit. A green source-CI record does not become a
full native publication receipt. Preserve the existing architecture and SQLite
foundation; do not implement a competing engine to satisfy an older ticket.
Documentation-only changes use hosted verification and no local tests or
Buildkite evidence collection. Previously merged fixes need accurate tracking,
not duplicate implementation or unsupported issue closure.

## Accepted v0.146.0 audit reconciliation

This map now uses accepted master `e9c40e929ee33aa5443c38b57276f96111db0b92` and the installed 0.146.0 kernel. The accepted #1699 bounded escalation CLI/RPC reference is implemented and covered by its exact-source execution; it is not complete external runtime adoption, semantic reasoning proof or the ten-worker A–H demonstration. Prior baseline paragraphs remain historical audit context. Pending #1697 physical recovery and #1702 canonical local-target admission are kept separate from accepted enforcement until individually verified and merged. Source-CI diagnostics do not substitute for the unchanged full native provider contract.

## Read-only workspace diagnosis (#1071)

Operators and agents need one command to identify the next recovery step
when a workspace is blocked. The diagnostic shows accepted authority and
worktree proposals separately and continues to explain malformed config
without bootstrapping or repairing state. Verification and admission stay
with the existing governed operations.

## Recorded instructions and independently accepted requirements

Keep the original request and ordered clarification history available across a cold handoff. A recorded revision identifies its observed or declared source. A provider account observation supports provenance without authenticating human approval. Pending decisions remain visible after reinitializing the same plan. Resolving a decision requires a fresh plan acceptance before the existing execution evidence can be used.

## Independently accepted GitHub Actions preflight (#1890)

A strict `decapod.pr-readiness.v1` source publication can use GitHub Actions (`[proof].validation_github_actions_preflights = true`) only when the accepted target independently selects that carrier and retains the reviewed dual-root executor. The carrier enforces the same complete finite source-CI and candidate-build graph (`readiness_profile::required_source_catalog_for_provider` with `evidence::Provider::GithubActions`) and source/target freshness checks, while CI-local `validate` remains diagnostic (`scope=ci_local`, `publication_ready=false`). Candidate configuration changes cannot opt a branch into a different strict source-CI carrier (`GITHUB_SOURCE_CI_POLICY_NOT_ACCEPTED`, `SOURCE_CI_PROVIDER_NOT_ACCEPTED`); live strict-carrier activation awaits separate maintainer target-policy acceptance.

## Version-only release proof

Decapod release metadata must remain independently distinguishable from source, policy, dependency and authored-spec changes. Its existing immutable classifier and exact projection verifier own this boundary. A canonical Decapod source checkout may retain version-bound agent entrypoints under its already accepted source-development policy. Release proof must preserve those exact committed bytes when that policy verifies origin, immutable source ancestry, canonical template payload and each version-bound fingerprint. This does not permit a caller-selected header or a source/configuration change to masquerade as release metadata. Required Actions will exercise this correction; implementation and registered test locations alone are not passing evidence.

The current candidate integrates accepted release0.153.2 at34b98ab1. Its first Actions attempt was rejected before execution after the accepted target advanced; that refusal remains historical. Release projections are regenerated through official0.153.2, and the complete105-file source inventory is independently remeasured. The reviewed regression fixture is the only inventory input changed from the accepted base. GitHub Actions for the integrated exact candidate remain required; old passes and source registration do not establish current execution.

Actions exposed a standalone-tool integration error in the first current-base candidate: the shared verifier could not call its crate-private source reader. The existing read-only predicate is now exported as an internal documented-hidden helper for that wrapper. Source-lineage regression setup stays in the library-only harness; the standalone wrapper retains its existing complete shared regression suite. No test is removed from the required library execution.

The full Supabase-enabled library execution caught a test-fixture identity omission: the release-version helper must retain the canonical Cargo package repository field when it reuses the existing published-root source fixture. The correction changes fixture construction only; origin, ungrafted ancestry, committed template and immutable entrypoint checks remain strict. Prior failed candidate eee794ba and actual job136182283538 remain preserved.

Release projection integrity and retained observer-run completeness are separate proof surfaces. A successful version-only comparison cannot turn missing run boundaries or incomplete tool observation into an autonomous-admission claim; the existing acceptance observer retains that judgment. Preserving source-entrypoint versions grants no observation coverage.

## Portable encoding interpretation assurance

The existing governance interoperability work now includes a narrowly versioned raw-JSON decoding contract at `docs/conformance/json-v1.md`. Its purpose is to make the independent standard-library Python consumer agree with the real native RPC parser on malformed input and legitimate Unicode, finite-number and nesting boundaries. The 22 pinned inputs protect rejection and ordinary read-only operation. This is one encoding-plane correction; it does not complete the independent governance-envelope profile or authenticate imported proof, approval or custody.

The encoding profile and independently retained agent-run coverage remain separate. Matching decoder reasons proves only the declared raw-input interpretation scope; it cannot substitute for required run boundaries, tool observations or the actual agent closing event. Missing observer coverage remains unknown even after every encoding vector passes.

The independent JSON encoding vectors remain a bounded parser compatibility check alongside accepted recorded-instruction readiness in `src/decapod/plan_governance.rs`. Agreement on Unicode, finite numbers, duplicate keys and nesting cannot converge unresolved instructions, satisfy spec review, or confer current publication authority.

The accepted v0.154.0 package epoch (`0f44f780`) is integrated before encoding validation. The conformance harness obtains executable identity through the documented read-only `decapod system version` command; unsupported top-level version flags are not execution evidence. Native regeneration supplies the current release projections, while the independent decoder and all pinned raw inputs remain separate from authorization and full native admission.

## Houseboat Core Engine, Darkmatter Read-Only Contract, TS/Pi SDK, and Storage/Recovery Completeness (#852-#859, #1711, #1277, #1630, #657, #1024, #1367, #880, #1763)

Decapod's local-first governance kernel now exposes decoupled Houseboat primitives in `src/decapod/core/houseboat.rs` and `src/decapod/core/mooring.rs` so multi-agent fleets, workspace lifecycles, drift policies, collaboration boundaries, durable journals, motion adjudication, degraded resilience modes, and shared principal-level resource budgets can be evaluated deterministically without weakening custody or adding background daemons.

- **Fleet coordination & 3-worktree reconciliation (`#852`, `#1763`)**: `FleetCoordGraph` models typed edges (`depends-on`, `conflicts-with`, `supersedes`, `derives-from`, `integration-required`), rejects cyclic dependency/derivation graphs, blocks superseded or conflicting workunits, and gates multi-workunit merges (including 3-worktree combined-candidate negative controls in `tests/houseboat_engine.rs`) on an exact `CombinedReconciliationProof` bound to participating heads and accepted target SHA.
- **Workspace lifecycle, drift seaworthiness, mooring contracts, journals, motion & resilience (`#853`-`#858`)**: Explicit workspace lifecycle transitions preserve dirty worktrees and surface leaked containers/mounts/temp credentials; configurable `DriftPolicyConfig` avoids hardcoded blocking-only projection severity while detecting unsupported toolchains and deprecated APIs with bounded, uncertainty-preserving remediation proposals; `CollaborationContract` and `MooringRegistry` fail closed on expiry, revocation, boundary escape, or tampered artifact wrappers; `DurableJournal` and `reconcile_machine_journals` preserve incomplete transitions and refuse silent last-writer-wins; `MotionAdjudicator` flags premature/out-of-scope/stalled work and dual-path mismatches; dependency classification separates critical stops from typed degraded modes with explicit exit-degraded evidence contracts and resumable partial-write recovery.
- **Principal-level resource budgets & uncertain-effect holds (`#859`)**: `ResourceBudgetLedger` enforces atomic principal ceilings and per-child limits (`6+6 vs 10`, `3x4 vs 10`, `5+5` exact fit) and keeps `UncertainHold` allocations occupied past expiry until reconciled by an authoritative owner with a matching fence token.
- **Darkmatter read-only boundary (`#1711`) & TS/Pi SDK (`#1277`)**: `docs/architecture/darkmatter-readonly-contract.md` and `tests/fixtures/darkmatter_readonly_snapshot.v1.json` define the repository-bounded read-only snapshot contract (keeping UI assets strictly out of `DecapodLabs/decapod`); `bindings/typescript/` and `packages/pi-decapod/` provide a zero-dependency library-safe TypeScript client (`AbortSignal`, isolated env, zero `process.exit`) and Pi lifecycle extension that keep raw observed human prompts distinct from agent interpretation and classify direct host writes honestly as `outside_native_enforcement`.
- **Storage, knowledge attestation, init matrix & crash recovery (`#1630`, `#657`, `#1024`, `#1367`)**: Local SQLite compaction benchmarking (`run_local_storage_compaction_benchmark` + `docs/datastore-v2/compaction-benchmarks-v0.162.0.md`) explicitly separates measured local SQLite metrics from unavailable hosted PostgreSQL coverage; `KnowledgeAttestation` enforces write-time source attestation, secret rejection, and human+evidence promotion gates; `reconcile_after_crash` refuses stale repository HEADs, changed instruction revisions, and pending unknown outcomes. Live multi-host/hosted/external-repo arms remain explicitly open and tracked via `Refs`.

## CI Workflows, Dual-Root Readiness Profile, Manifest Relocation, and Governed Automation (#1079, #1122, #1047, #1134, #1388, #1626, #1672, #1879, #1890, #1573)

- **Specs manifest relocation (`#1079`)**: `.decapod/managed/.manifest.json` is now the canonical living-specs manifest path across `src/decapod/core/project_specs.rs` (`LOCAL_PROJECT_SPECS_MANIFEST`), scaffolding, `.gitignore`, `validate.rs`, `pre_build_validate.rs`, `release_projection_drift.rs`, `integration_removal.rs`, and CI scripts (`health.sh`, `standalone-validation.sh`), with backward-compatible read fallback to legacy `.decapod/managed/specs/.manifest.json` and automatic cleanup on refresh.
- **Content-bumping `specs.refresh` (`#1122`)**: `project_specs::refresh_specs_from_codebase` deterministically updates a bounded source-derived `codebase-contract` marker block in each living spec (modules, CLI/RPC surfaces, workflows, integration test targets, state/ops/security surfaces) while preserving 100% of authored semantic prose and keeping generated blocks excluded from `material_spec_body()` so `#1183` still requires authored prose.
- **Fast pre-build CI gate (`#1047`, `#1134`)**: `.github/workflows/ci.yml` executes `pre-build-validate` (`assets/tools/source_ci/pre-build-validate.sh`) immediately after `release-metadata` and before heavy build/test/lint jobs, failing fast when `.decapod/config.toml`, `.decapod/governance.json`, `.decapod/managed/.manifest.json`, living-spec hashes, or 64-hex `decapod-fingerprint` entrypoint SHAs are missing or stale; `src/decapod/core/pre_build_validate.rs` enforces the same structural and SHA-256 checks locally.
- **Parallel core/CLI CI sharding & release-cut ruleset alignment (`#1672`, `#1879`)**: `Tests (core)` and `Tests (cli)` split into three parallel shards each (`test-core` `1/3..3/3`, `test-cli` `1/3..3/3`) with fail-closed aggregator jobs retaining the exact `Tests (core)` and `Tests (cli)` check names; `test-core`, `test-core-status`, and `supabase-consumer` remain scheduled on version-only release cuts so ruleset `12701816` receives both required named checks.
- **Governed scheduled docs auto-sync (`#1388`) & Nix tag publication (`#1626`)**: `.github/workflows/docs_autosync.yml` replaces raw `create-pull-request` with `assets/tools/docs_autosync_governed.sh` (`begin-pr`, trajectory init/record, `specs.refresh`, exact-material `checkpoint`, `verify-checkpoints --publication-material`, non-force push, fail-closed drift report); `.github/workflows/nix-flake.yml` covers squash-merged commits and SemVer `v*` release tags under native-only publication ownership.
- **Non-Buildkite GitHub Actions preflight activation (`#1890`) & ETag conditional polling (`#1573`)**: `.decapod/config.toml` transitions from Buildkite (`validation_buildkite_preflights = false`, `[proof.ci_attested]` retired) to the independently reviewed GitHub Actions carrier (`validation_github_actions_preflights = true`); `PublicGithubReads` (`src/decapod/core/snapshot_provider/mod.rs`) and `github_quota.rs` support bounded in-process `If-None-Match` ETag revalidation so HTTP 304 polls preserve anonymous rate-limit quota without caching across runs.

## Crash Durability, Consistency Model Checking, Derived-Index Atomicity, and Security Boundary Enforcement (#1934, #1935, #1936, #1964)

- **Hardening & assurance closure (`#1934`–`#1936`, `#1964`)**: Closes real boundary gaps identified during post-cutover verification without altering authority boundaries: (1) enforces exact origin authority and canonical path allowlisting in `container_signer::extract_origin_relative_path` (`src/decapod/core/container_signer.rs`) so suffix/port/userinfo/fragment/path-traversal smuggling cannot mint host-signed DPoP proofs (`RULE-SIGNER-ORIGIN-010`), paired with `RULE-HTTP-AUTH-011` (`http_transport::verify_bearer_token` inside `handle_single_connection`); (2) makes `lcm::rebuild_index` and `lcm::rebuild_index_from_ledger` (`src/decapod/plugins/lcm.rs`) atomic under `BEGIN IMMEDIATE ... COMMIT` while preserving existing `originals_index` rows on content-hash validation failure; and (3) proves `workspace_lifecycle::reserve` staging-to-publish `SIGKILL` recovery + CLI `govern workunit recover`, a deterministic 48-step multi-agent + clock-anomaly lease model checker (`tests/lease_fencing_consistency_1935.rs`), and `federation` + `lcm` + `context_capsule` rebuild convergence (`tests/derived_replay_1936.rs`).

## Whole-Repo Egress Scanner & Cloud-to-Local Revocation Gate (#1944)

- **Whole-source egress inventory enforcement (`src/decapod/core/model_assets.rs`, `tests/trust_egress_gate.rs`, `docs/trust-and-egress-inventory.md`)**: Closes the gap between Decapod and Dactyl/Propodus static egress gates by (1) validating model asset download URLs through `trust_egress::validate_egress_url(url, &[])` prior to file creation or `/usr/bin/curl` execution; (2) scanning all production `.rs` files under `src/decapod/` (`#[cfg(test)]` stripped) so any newly added outbound `curl`/`gh`/socket/HTTP sink or production `https://` host fails CI unless registered in `REVIEWED_OUTBOUND_SOURCE_SINKS`, `DECLARED_EGRESS_CHANNELS`, and `docs/trust-and-egress-inventory.md`; and (3) exercising runtime `DECAPOD_CHECK_LATEST` and `backend = "cloud"` -> `cloud logout` + `backend = "local"` revocation across restarts under a live loopback trap (`Clack#10` stays outside scope and `#1944` remains open).

<!-- decapod:declared-capabilities:start -->

## Declared Capability Surfaces

These declarations select authoring and proof obligations; they do not attest implemented runtime guarantees.

- `agent-helper`
- `authentication`
- `authorization`
- `background-processing`
- `control-plane`
- `event-driven`
- `external-integrations`
- `governance-kernel`
- `persistent-state`
- `scheduled-jobs`

<!-- decapod:declared-capabilities:end -->

<!-- decapod:codebase-contract:start -->

## Codebase Contract Snapshot

- Tracked significant codebase files: 655 across `.config/` (1), `.github/` (11), `Cargo.lock/` (1), `Cargo.toml/` (1), `README.md/` (1), `assets/` (41), `bindings/` (9), `docs/` (3), `packages/` (4), `src/` (286), `tests/` (297)
- Core modules: 149 | Workflows: 9 | Integration targets: 180
<!-- decapod:codebase-contract:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `e6f364a29983cb8bdc9433c161c92f3ff446d83612d7665c701bb88094190f9f`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (41 files), `bindings/` (9 files), `docs/` (3 files), `packages/` (4 files), `src/` (286 files), `tests/` (297 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

# Architecture

## Modular agent interoperability engine (`src/decapod/core/agent_interop.rs`, #870–#878, #1715–#1718, #1724, #1725, #1733, #1735)

- **Four-plane envelope, replay/authority isolation, drift & observed receipts (`src/decapod/core/agent_interop.rs`)**:
  - `GovernanceEnvelopeV1` (`PlaneVersions`, `IntentPlane`, `AuthorityPlane`, `EvidencePlane`, `OutcomePlane`) is evaluated by `import_external_envelope` against `LocalTrustPolicy` and `ReplayLedger`, enforcing major plane version checks, unknown mandatory extension refusal, safe sorted omission of unknown optional extensions, envelope SHA-256 verification, URL/path ambiguity rejection, cross-repo/tenant/recipient isolation, replay/retired-run protection, raw receipt byte verification (rejecting summary fiction and mutated bytes), producer trust checks, freshness (head/base/tree/epoch/lease/expiry/newer failure), delegation narrowing, exact approval parameter binding, environment-tamper & contiguous history checks, and process-vs-outcome separation. `imported_authority_granted` is unconditionally `false`.
  - `project_decision_disposition` provides a read-only projection onto `DecisionDisposition::{Admit, RepairRequired, AuthorityRequired, Reject}` while preserving `EvaluationTransportState::{Invalid, Unknown, Unavailable}` distinct from admission and refusing scope-widening repairs.
  - `check_dispatcher_authorization_drift` audits `interop::builtin_manifest()` against `interop::RPC_OPS`, actual `rpc_op_bypasses_session` entry guards, and read-only profile allowlists in both directions. `project_manifest_with_receipt` binds `ObservedConformanceReceipt` to binary SHA, source commit, schema/profile version, transport, producer trust class, raw evidence digests, and TTL before promoting capabilities from `NotRun` to `Verified`/`Failed` (or `Stale` on mismatch/expiry).
- **Bounded transport adapters (`src/decapod/core/agent_interop/mcp.rs`, `src/decapod/core/agent_interop/a2a.rs`)**:
  - `mcp::McpServer` runs a finite stdio JSON-RPC 2.0 loop (`serve_stdio`) supporting JSONL and `Content-Length` framing, unique-key JSON parsing (`core::json::unique_value`), strict frame/response byte caps, MCP protocol version negotiation (`2025-03-26`, `2024-11-05`), secret/rubric/traversal denial on `resources/read`, read-only vs session/worktree-gated `tools/call`, and conflict-safe host config merge/removal (`generate_mcp_host_config`, `merge_mcp_host_config`, `remove_mcp_host_config`).
  - `a2a::{builtin_agent_card, validate_a2a_handoff}` advertises only executable local governance skills with explicit `A2aConformanceLayers` separation and validates `A2aHandoffEnvelope` against `LocalHandoffContext` without importing remote authority.
- **Offline registry, skill compiler, manifest compiler, composition, rule evaluator & overlap classifier (`src/decapod/core/agent_interop/registry.rs`, `src/decapod/core/agent_interop/rules.rs`)**:
  - `registry::CapabilityRegistry` validates SPDX licenses, verified provenance, SHA-256 content digests, prompt-injection cleanliness (`scan_markdown_for_injection`), and acyclic dependency/conflict graphs offline. `select_skills_and_compile_bundle` enforces relevance thresholds, conflict precedence, and `token_budget`, paired with `evaluate_skill_ab_fixture`. `compile_harness_manifests` runs `detect_contract_policy_conflicts` before emitting reproducible entrypoints and `SourceMapEntry` records. `compose_fragments` enforces `PrecedenceTier` ordering, blocks invisible bidi/zero-width and injection patterns, and requires human review on authority/tool expansion. `harness_compatibility_matrix` and `generate_harness_package` cover all 6 harnesses honestly with `live_host_verified: false`.
  - `rules::coding_rule_taxonomy_matrix` covers all 8 required domains plus documented rejected proposals with zero third-party product names. `evaluate_rule_applicability` enforces `RuleArtifactKind::CanonicalSource`, `RuleAuthorityTier` precedence, policy immutability, expiry, injection/escalation rejection, and honest `no_rule_justified` empty selection. `classify_task_overlap` and `evaluate_combined_integration` implement all 6 semantic overlap states and deny publication when a clean textual merge fails combined integration proof.

## Read-only observability projections and hardened HTTP transport (#860, #874, #1712-#1714, #1719-#1720, #1723, #1727-#1728, #1730-#1732)

- **Observability projection core (`src/decapod/core/observability_projections.rs` and submodules)**:
  - `snapshot.rs` (`#1714`, `#1727`, `#1728`): `capture_observation_with` takes pre-read and post-read `CaptureMark`s across Git HEAD (`git --no-optional-locks rev-parse`), Git index SHA-256, `.decapod/governance.json` SHA-256, and SQLite (`db::db_connect_readonly_bounded` with `PRAGMA query_only = ON`, `PRAGMA data_version`, `MAX(seq)`, `COUNT(*)`), producing `ObservationSnapshot` with honest `atomicity = "best_effort_double_read"` and `SnapshotConsistency` flags. `enumerate_live_workspace_inventory` enumerates staged/unstaged/untracked/renamed/deleted changes with exact SHA-256 hashes and secret/symlink/binary/case-collision flags; `read_source_anchor` and `verify_anchor_freshness` enforce component-wise `O_NOFOLLOW` repo containment and bind slices to file SHA-256.
  - `specs.rs` (`#1712`, `#1720`): `project_specs_and_governance` projects `.decapod/managed/specs/*`, `.manifest.json`, `.decapod/governance.json` intent lineage (`original_intent` vs `refined_intent`), active claims, `ValidationReceipt::validate_integrity()`, read-only SQLite objectives, `intent -> task -> spec -> implementation -> receipt` traversal edges, and `inspect_spec_text_security` flags.
  - `timeline.rs` (`#1713`): `project_activity_timeline` composes trajectory loops (`TrustClass::Declared`), witnessed native runner records from `trajectory::path::Judgment` (`TrustClass::Witnessed`), bounded SQLite event pages via `projection_budget::project_events_read_only`, Git commits, and validation receipts. `verify_trajectory_integrity` and `verify_observed_path_chain` inspect raw `.decapod/governance.json` even on tampered hashes to detect `TRAJECTORY_HASH_MISMATCH`, `MISSING_INITIAL_LOOP_ATTEMPT`, `NON_CONTIGUOUS_LOOP_ATTEMPTS`, `FORGED_OR_DANGLING_CUSTODY_LINK`, `WITNESSED_PATH_BROKEN_CHAIN`, and `FORGED_VALIDATION_RECEIPT`.
  - `workspace.rs` (`#860`, `#1723`): `project_workspace_observation` and `classify_workunit_activity` compute deterministic `WorkunitActivityCategory` states (`Active`, `Waiting`, `Blocked`, `Drifting`, `Completed`, `Abandoned`) with typed `allowed_transitions()`, `fleet_coord::detect_fleet_overlaps` contention, `StatusAuthorityClass` (`Authoritative` vs `StaleInput` vs `Inferred`), `WorkspaceEvolutionRecord`, bounded worktrees via `projection_budget::project_worktrees_read_only`, and shared human rendering via `render_workspace_human`.
  - `validation_plan.rs` (`#1730`), `pr_manifest.rs` (`#1731`), `provenance.rs` (`#1732`): `project_validation_plan` evaluates rules across all four `RuleAuthority` classes, surfaces missing requirement mappings (`R3` dropped in decomposition), separates `local_checks` from `joint_integration_checks` (`clean_merge_does_not_prove = true`), and runs union-find `evaluate_constraint_relations` to detect global contradictions (`A=B, B=C, A!=C`). `project_pr_manifest` separates prospective PR `agent_intent` from `provider_state` and detects `PrDriftFlag` conditions. `project_context_provenance` reads `.decapod/data/traces.jsonl` strictly read-only, links `intent_id`/`intent_revision`, strips raw prompts and private reasoning keys, and rejects cross-scope repo queries.
  - `server.rs` (`#1719`, `#1720`): `serve_observe_http` / `serve_observe_on_listener` expose `GET /observe/v1/*` and allowlisted read-only `POST /rpc/v1` (`ALLOWLISTED_OBSERVE_RPC_OPS`) over `http_transport::serve_on_listener` with `read_only_only = true` and `ReadProjectionAccessPolicy`.
- **Hardened HTTP RPC transport & bounded subprocess stdin (`src/decapod/core/http_transport.rs`, `src/decapod/core/bounded_process.rs`, `src/decapod/lib.rs`)**:
  - `bounded_process::BoundedCommand::bounded_capture_with_stdin` adds nonblocking stdin delivery inside the Unix process-group poll loop so `lib.rs::http_rpc_handler_with_limits` terminates stalled (`504 rpc_timeout_unknown_outcome`) or oversized (`502 rpc_output_limit_exceeded`) child RPC processes cleanly and reaps owned descendants.
  - `http_transport::serve_on_listener` enforces constant-time Bearer comparison (`verify_bearer_token`), `Origin`/`Host` loopback validation (`validate_origin_and_host`), request-bound idempotency (`compute_request_binding_digest` -> `409 idempotency_conflict`), finite entry and byte bounds (`max_cache_entries`, `max_cache_bytes` -> `503`), explicit `ReplayContract`, uncertain-outcome non-replay (`409 unknown_outcome_reconcile_required`), and `max_requests` (`INV-DAEMONLESS`).

## Governance ledger, clarity validator, historical exceptions, and formal obligation cores (#1083, #1084, #1093, #1097, #1365, #1468, #1569, #1691, #1726, #1729, #1761, #1762, #1768, #1907)

- **Escalation-admissibility templates & constitution (`src/decapod/core/assets.rs`, `assets/constitution.json`)**: `template_agents()` and `agent_entrypoint_body()` align Golden Rules 1, 3, and 9, Pre-Inference Governance, and Operating Notes with bounded reflection/escalation admission (`#1691`), and `assets/constitution.json` updates all 16 routing-pass directives plus `interfaces/PROCEDURAL_NORMS` while leaving historical entrypoint fixtures intact.
- **Accepted-policy `main_merge` & anti-widening gates (`src/decapod/cli.rs`, `src/decapod/core/effective_grant.rs`, `src/decapod/core/validate.rs`, `src/decapod/lib.rs`)**: `GovernanceConfig.main_merge` defaults to `false` (`skip_serializing_if = "is_false"`), is preserved during init refresh (`existing_main_merge`), and is guarded against candidate widening in `effective_grant::check_candidate_policy_widening`, `validate::validate_project_config_downgrade_check`, and `lib.rs::run_validate_command` (`POLICY_SELF_AUTHORIZATION_REJECTED`).
- **Offline historical exceptions & semantic integrity (`src/decapod/core/governance_document/historical_exception.rs`, `src/decapod/core/governance_document/semantic_integrity.rs`, `src/decapod/core/governance_document.rs`)**: `verify_historical_exceptions` verifies Ed25519 signatures via `ring::signature::ED25519` strictly against accepted-base `.decapod/historical-exception-signers.json` (rejecting candidate trust roots, replay, revocation, expiry, commit/tree/obligation drift, and conflicting authority) while keeping `history_proven = false`. `validate_semantic_governance_integrity` and `detect_governance_theater_diff` enforce substantive `plan`/`claims` bindings beyond `todo_ids`.
- **Modular obligation ledger, clarity validator, and formal obligation adapter (`src/decapod/core/obligation_ledger.rs`, `src/decapod/core/artifact_clarity.rs`, `src/decapod/core/formal_obligations.rs`)**:
  - `obligation_ledger.rs`: `derive_obligation_ledger` (`#1084`, conservation/consistency/proof + async effect states), `StagnationTracker` (`#1083`, prose-immune governed fingerprint + compaction serialization + provider-wait distinction), `verify_outcome_claim_anchor` (`#1729`), `build_completion_report_4d` (`#1726`), `ApprovalLedger::authorize_and_consume` (`#1761`), `evaluate_trusted_grader` (`#1762`), `evaluate_root_conservation` (`#1768`), `evaluate_main_merge_authorization` (`#1468`), and offline harnesses for `#1095`, `#1772`, `#1774`, and `#1796`.
  - `artifact_clarity.rs`: Zero-LLM deterministic mechanical clarity & controlled-English validator (`ste-lite`, `ste-strict`, `custom`) with canonical Decapod terminology table (`#1097`, `#1365`), skipping code spans/fences and human prompts while setting `formal_asd_ste100_compliance = false`.
  - `formal_obligations.rs`: Opt-in (`enabled = false` default) Lean/Kani/Verus transcript and hole verifier (`verify_formal_obligation`, `compute_runner_attestation`) plus `CapacityAccountingState` proving conservation across allocate/arm/settle/abort/indeterminate/timeout/restart/reconcile transitions (`#1907`).

## Maintenance RBAC, logical legacy TODO recovery, read-only analysis, and pre-build/workspace diagnostics (#1568, #1657, #1279, #1071, #1064, #1047)

- **Governed WAL checkpoint & config resolution (`src/decapod/core/db_maintenance.rs`, `src/decapod/cli.rs`)**: Adds `DbMaintenanceCapability::WalCheckpoint` (`db.wal.checkpoint`, `MaintenanceAuthorityTier::ProjectMaintenanceOptIn`, opt-in outside `DEFAULT_SAFE_ALLOWLIST`) and `run_maintain_wal_checkpoint`, acquiring a bounded `StorageLock::Shared` and executing `PRAGMA wal_checkpoint(PASSIVE)` without deleting WAL/SHM sidecars. `DecapodProjectConfig::effective_db_maintenance` resolves `[governance.db_maintenance]` or top-level `[maintenance]` and fails closed with `AMBIGUOUS_MAINTENANCE_POLICY` if both are set.
- **Non-empty legacy TODO inspection, export, quarantine & journaled import (`src/decapod/core/legacy_todo_recovery.rs`, `src/decapod/core/todo.rs`)**: `inspect_logical_state` reads bounded temporary snapshots (`MAX_LOGICAL_ROWS = 10_000`) to classify `logically_equivalent` vs `divergent` stores against canonical `tasks`. `write_export_bundle` writes private `create_new` JSON exports; `--quarantine` hard-links to `todo.db.legacy-recovery-v1.archive` and retires `todo.db` without mutating `decapod.db`; `--import-missing` archives `todo.db` first, rejects any conflicting ID (`LEGACY_TODO_CONFLICT_BLOCKED`), inserts missing rows + `task.add` events + `task_journal::record_changes` inside one atomic `BEGIN IMMEDIATE` transaction, and verifies `task_journal::verify_with_budget` before unlinking `todo.db`.
- **Early-dispatch read-only repository analyzer (`src/decapod/core/analyze.rs`)**: `OperationClass::{AnalyzeReadOnly, GovernedChange}` separates epistemic reads from governed mutations. Dispatched directly alongside `Command::Eval` in `src/decapod/lib.rs` before store/session/migration startup, `analyze_repository` walks directories deterministically under `(max_files, max_bytes, timeout_ms)`, skips symlinks and sensitive filenames, scans instruction files via `agent_eval::evaluate_prompt`, observes git via `GIT_OPTIONAL_LOCKS=0`, infers strictly evidence-backed `draft` capabilities, and redacts output via `core::trace::redact`.
- **Workspace effective-state, publish dry-run/modes, and pre-build structural checks (`src/decapod/core/workspace_diagnostic.rs`, `src/decapod/core/workspace.rs`, `src/decapod/core/workspace_publication.rs`, `src/decapod/core/pre_build_validate.rs`)**: `workspace_diagnostic::inspect` surfaces top-level `dirty_files` and deduplicated `next_actions` and detects candidate `[maintenance]` edits as proposals; `workspace::plan_workspace_publish` and `workspace_publication::{classify_remote_divergence, is_governance_only_path, ensure_and_verify_pr_for_target}` enforce `--pr`, `--branch`, `--mode governance-only`, `--dry-run`, and non-forced divergence decision gates; `pre_build_validate::evaluate` performs early read-only schema/governance/specs/entrypoint checks with `scope = "pre_build"` and `publication_ready = false`.

## Multi-organization Cloud parity matrix and organization overlay stack (#1895, #877)

- **Parity matrix & tenant isolation verification (`#1895`)**: `src/decapod/core/cloud_parity_matrix.rs` defines the versioned `decapod.cloud-parity-matrix.v1` inventory comparing `local` SQLite and `cloud` storage/governance behavior across task/event/CAS ordering, custody and leases, authorization-scoped transitions, commit/policy-bound evidence, and Cloud-unsupported operations. It exposes CLI (`decapod capabilities parity`) and read-only RPC (`interop.cloud_parity`) projections without touching local SQLite. `tests/cloud_tenancy_parity_1895.rs` exercises these invariants against a local simulated Cloud gateway backed by two isolated backend stores; live hosted Dactyl/Supabase/Propodus clusters remain outside offline cargo test execution.
- **Four-layer constitution resolution (`#877`)**: `src/decapod/core/org_overlay.rs` resolves `OrganizationOverlayConfig` (`DeclaredContextConfig.organization_overlay` in `src/decapod/cli.rs`) and `OrganizationOverlayBundle` (`decapod.org-constitution-overlay.v1`), computing `OverlayTrustState`, `OverlayFreshnessState`, and `AuthorityChainProvenance`. `src/decapod/core/assets.rs` (`canonical_override_id`, `raw_project_override_entries`, `resolved_override_evidence`, `get_merged_doc`) blends layers 1–3; `src/decapod/core/docs.rs` (`resolve_scoped_context`) projects non-authoritative Layer 4 context; `src/decapod/core/context_capsule.rs` (`DeterministicContextCapsule.authority_chain`) binds provenance hashes while preserving legacy hash stability when `authority_chain` is `None`; and `src/decapod/core/context_bundle.rs` (`build_bundle`, `validate_bundle`, `contains_private_policy_leak`) strips and rejects `organization_private` policy text leaks (`ORG_OVERLAY_PRIVATE_LEAK_REJECTED`).

## First-class Decapod Cloud administrative core (`src/decapod/core/cloud_admin.rs`, #1896, #1836, #1835, #1877)

- **Admin control-plane boundary (`src/decapod/core/cloud_admin.rs`, `src/decapod/core/cloud_client.rs`)**:
  `cloud_client::with_admin_session` acquires `CloudConnector` (via `acquire_with_metadata` using `discover_with_metadata`), enrolled DPoP signer, and repository-scoped `SessionStore` bearer without initializing `.decapod/data/decapod.db`, reconciles `AuthRequired | Revoked` via `account_result`, and threads `CloudMetadataWarning`s from both discovery and per-request `CloudResponseMetadata`. All five handlers (`cloud_admin::{orgs, members, invites, identity, billing}`) execute exclusively over `dactyl_db::AccountOperation::{Register, Status, Manage, Portal}` (`--no-default-features` builds keep compile-time feature parity and fail closed via `selected_origin()`).
- **Slot-to-UUID seat resolution & orthogonal billing probing (`cloud_admin::{orgs, members}`)**:
  `orgs` merges `AccountOperation::Status` (`organizations`, `projects`) with admin-gated `AccountOperation::Manage` (`action = "status"`) to obtain `active_paid_repositories` / `paid_repo_count` while treating unconfigured billing probes benignly and preserving `OutcomeUnknown`/`AuthRequired`/`Revoked`/`MfaRequired` failures. `members assign` and project-scoped `members revoke-seat` resolve `--slot <n>` across cursor-paginated `AccountOperation::Register` (`operation = "members"`, integer `after` cursor) to an active verified `account_id`, failing closed on pending slots before dispatching `AccountOperation::Manage` (`action = "assign" | "revoke"`).
- **Compact diagnostic & error taxonomy (`src/decapod/core/error.rs`, `src/decapod/core/cloud_client.rs`, `src/decapod/core/cloud_todo_operation.rs`)**:
  Adds `CloudAuthStatus::OutcomeUnknown` (mapped to `FailureKind::Transport`) and 1-byte `CloudAuthErrorCode` inside `CloudAuthDiagnostic` (keeping `DecapodError` under the 128-byte `clippy::result_large_err` budget while emitting snake_case `error_code`). `cloud_client::admin_transport_error` separates read transport failures (`Offline`) from mutating post-send uncertainty (`MutationOutcome::Uncertain`, `outcome_unknown`, transport/timeout/unavailable -> `OutcomeUnknown`) and handles `dactyl-db 0.18.0`'s redacted HTTP 403 `Authorization` errors on AAL2-gated operations via `bearer_aal2_claim` (`MfaStepUpRequired` when bearer JWT lacks `"aal":"aal2"` or is opaque; `AdminRequired` when `"aal":"aal2"` is present). `status_live` and `cloud_admin::billing` both classify `"overage_cap_exhausted"` in `spend_cap_warning`.

## Effective operation grants and cumulative delegation accounting (#1905, #1893)

- **Deterministic composition & canonical path algebra (`src/decapod/core/effective_grant.rs`)**:
  `PathPattern` and `ScopeAllowanceSet` normalize repository-relative paths/globs (`RepoWide`, `DirPrefix`, `DirImmediate`, `Exact`) with strict ambiguity rejection in `parse_scope_spec` (no silent fallback on malformed clauses, `..`, backslashes, mid-path `**`, or unsupported wildcards). `compose_layers_order_independent` performs commutative, associative intersection over allowances/operations/resources, union over denials (`reduce_subsumed_patterns`), `min` over budgets and expirations, and exact recipient/policy/repo bindings. `evaluate_effective_grant` reuses `db_maintenance::resolve_repository_policy`, `plugins::policy` (`eval_risk`, `actor_trust_level_on_conn`, `required_trust_for_operation`, `trust_level_to_int`, `check_approval_on_conn`, `has_live_scoped_approval_on_conn`, `exact_target_approval_ids_on_conn`), `fleet_coord::lease_state`, `GatekeeperConfig::from_repo_config`, and `plan_governance::load_plan` without persisting any flattened mutable permission cache.
- **SQLite event-backed delegation & cumulative shared-parent budgets (`src/decapod/core/effective_grant.rs`, `src/decapod/core/todo.rs`, `src/decapod/core/task_journal.rs`)**:
  Delegation policies (`task.delegation.policy`) and child reservations (`task.delegation.reserve`) are recorded in `events::TODO` without changing SQLite table DDL (`tests/datastore_contract.rs`). `record_delegation_policy_on_conn` rejects widening relative to ancestor tasks (`GRANT_DELEGATION_WIDENING`, `GRANT_DELEGATION_DEPTH_EXCEEDED`), `reserve_delegation_budget_on_conn` inside `claim_task_with_lease_local` enforces cumulative sibling budgets atomically (`CUMULATIVE_DELEGATION_BUDGET_EXCEEDED`), and `transition_child_reservations_on_conn` marks reservations `Consumed` on `todo done` and `Released` on `todo release`/`archive`.
- **Boundary enforcement & guidance hygiene (`src/decapod/core/workspace.rs`, `src/decapod/plan_governance.rs`, `src/decapod/core/scaffold.rs`)**:
  `enforce_task_operation_if_delegated` and `enforce_workspace_publish_grant` bind `todo.done`, `todo.handoff`, `workspace.ensure`, `plan.execute`, and `workspace.publish` (`collect_candidate_changed_files`) to `EffectiveOperationGrant` and candidate-policy widening detection (`detect_candidate_policy_widening`). Non-filesystem task policy targets (`task.*`/`todo.*` with `--path <task_id> --task-id <task_id>`) resolve exact `derive_fingerprint` approvals without misclassifying task IDs as repository paths. Maintainer-only development entrypoint-version-drift override text is excluded from `.decapod/managed/specs/OPERATIONS.md` while runtime config validation remains preserved in `entrypoint_integrity/history.rs`.

## Composed service capability contracts & validation boundary (#1906)

- **Versioned schema & core evaluator (`assets/schemas/capability-contract.schema.json`, `src/decapod/core/capability_contract.rs`)**:
  Defines `CapabilityContract` (`decapod.capability-contract.v1`), `ServiceInterfaceDescriptor`, `AcceptedCapabilityPolicy`, `EvidenceObservation`, `CompositeExecutionTrace`, and `CompatibilityReport` (`decapod.capability-compatibility-report.v1`). `validate_contract_schema` and `evaluate_composed_capability` enforce required schema/error/auth/idempotency/negative-case/provenance invariants, compare Dactyl and Propodus interface descriptors against pinned Decapod expectations, and verify candidate contracts against `AcceptedCapabilityPolicy` (`repo_accepted_policy` from main/host baseline).
- **Composite consequence evaluator (`evaluate_consequence_trace`)**:
  Distinguishes `CompositeConsequenceState::{Prepared, Observed, Held, Compensated}`, enforces prestate CAS checks (`PrestateDriftAfterPreparation`), aggregate shared-capacity checks (`SharedInvariantViolation`), async settlement (`UnresolvedAsyncConsequence`), severed-ack containment (`SeveredAckBlindReplayAttempt`), and admissible compensation (`task.release` with `FaultErasedByCompensation` / `UnsupportedCompensation` guards).
- **Multi-repository fixture & validation gate (`run_compatibility_fixture_suite`, `src/decapod/core/validate.rs::validate_composed_capability_contract`)**:
  `run_compatibility_fixture_suite` exercises negative and repaired traces for every falsifier in `#1906`. `validate_composed_capability_contract` leaves local repositories without `.decapod/contracts/capability_contract.json` unaffected while failing closed (`CAPABILITY_CONTRACT_DRIFT`) when an explicit contract file drifts from accepted host policy, drops required negative cases, or self-certifies.

## Operational reliability fixes (#1881, #1837, #1884, #1838)

- **Pre-build backup policy gate & `.decapod/backups` staging (`src/decapod/core/workspace.rs`)**:
  Local `ensure_workspace` invokes `container_snapshot_policy_blocker` (`db_maintenance::resolve_canonical_policy` + `evaluate_capability_decision(DbMaintenanceCapability::Backup)`) before `acquire_registered` / `build_workspace_image`, returning a `pending` `WorkspaceStatus` with `BlockerKind::Unauthorized` (`WORKSPACE_CONTAINER_SNAPSHOT_DENIED`) when denied. Authorized `container_workspace_launch_command` creates `.decapod/backups/container-store.XXXXXX` with symlink refusal (`test ! -L .decapod && test ! -L .decapod/backups`) and `mkdir -m 700`, cleaning the temporary directory on exit/signals.
- **Same-generation task heartbeat (`src/decapod/core/todo.rs`, `src/decapod/core/task_journal.rs`, `src/decapod/core/trajectory/execution.rs`, `src/decapod/core/mentor/reflection.rs`, `src/decapod/core/validation_history.rs`)**:
  `todo::heartbeat_claim_lease` journals `"todo.heartbeat"` (`task.lease_heartbeat`) without incrementing `revision` or `lease_generation` when the current active holder extends `lease_expires_at` (`now + lease_secs > previous_secs`), and `ensure_validation_lease_ttl` (`ValidationLeaseTtlMode::Renew`) delegates to `heartbeat_claim_lease` bound to the observed `lease_generation`. `task_journal::check_heartbeat_step` validates strict monotonic extension (`at < old && old < next`) with identical remaining projection fields, records `heartbeat_anchors`, and permits `"todo.heartbeat"` transitions inside `check_validation_binding` while preserving `#1892` stale subcodes (`TASK_ADVANCED_DURING_VALIDATION`), for all non-heartbeat transitions. `todo::capture_execution_custody_binding` (used by `trajectory/execution.rs::capture_once` and `mentor/reflection.rs::custody`) binds to the pre-heartbeat `heartbeat_anchor` expiry while still verifying live expiration.
- **Shared read-only toolchain hashing (`src/decapod/core/native_proof.rs`)**:
  `measure_readonly_toolchains` parameterizes `measured_toolchain_inputs` over a `FnMut(&Path) -> io::Result<String>` closure and caches `PathBuf -> digest` within a single `capture_readonly_identity` invocation while keeping independent execution checks live-measured.
- **Child-isolated Podman fixture (`tests/unit/plugins/container_tests.rs`)**:
  Writes the executable fixture in a waited `/bin/sh` child before `chmod 0o755`, preventing inherited writable file descriptors across parallel test threads.

## Portable policy-transition carriers and unified target-map checks (#1833, #1832)

- **Portable approval verification (`src/decapod/core/repository_policy_approval.rs`, `src/decapod/core/repository_profile.rs`)**:
  `transition_approval::{ApprovalInput, verify_signed, verify_cloud, approve_cloud, cloud_backend, git_object}` provides independent transition carriers under `repository_profile::verify_accepted_transition`. `verify_signed` reads `.decapod/repository-policy-signers.json` exclusively from the accepted base Git commit (`{base}:.decapod/repository-policy-signers.json`) and verifies an Ed25519 signature (`ring::signature::ED25519`) over `["decapod.repository-policy-approval.v1", key_id, transition_id, issued_at, expires_at]` within a 24-hour maximum lifetime (`MAX_APPROVAL_SECONDS = 86_400`). Under Cloud backends (`supabase-cloud`), `cloud::approve` and `cloud::verify` call `propodus_governance_v1` (`authorize_exception` / `verify_decision`) via `cloud_client::connect_control` and `repo_identity::resolve_repository_identity`, checking `project_id`, `decision_id`, `capability = "governance.exception.authorize"`, `obligation_id`, `base_commit`, `candidate_commit`, `affected_commit`, `tree_oid`, `current_generation_matches`, `authority_basis`, and expiration timestamps. Local checkouts continue reading the canonical main `.decapod/data` ledger while `has_active_policy_transition_approval` rejects any tracked `.decapod/data` (`POLICY_STORE_UNSAFE`).
- **CI base/candidate enforcement (`src/decapod/core/validation_rules.rs`, `src/decapod/core/validation_runtime.rs`, `src/decapod/core/workspace.rs`, `src/decapod/plugins/policy.rs`, `src/decapod/lib.rs`)**:
  `evaluate_policy_transition_with_approval` resolves the CI base ref strictly from provider metadata (`GITHUB_BASE_REF`, `BUILDKITE_PULL_REQUEST_BASE_BRANCH`) or remote-tracking `origin/HEAD` / unambiguous `origin/{main,master}`, verifies that working-tree `.decapod/config.toml` matches committed `{head}:.decapod/config.toml` when `is_ci` holds, and shares `ApprovalInput::from_environment` across `validation_rules`, `validation_runtime::load_project`, and `workspace::canonical_local_readiness_repository`. `lib.rs::run_govern_command` routes `GovernCommand::Policy` to `policy::run_policy_cli_in_workspace` to handle Cloud `repo.pr-readiness.v1` approvals.
- **Target-map transitions & scoped-proof diagnostics (`src/decapod/core/repository_profile.rs`, `src/decapod/core/validation_history.rs`, `src/decapod/plugins/verify.rs`, `src/decapod/lib.rs`)**:
  `repository_profile::preserve_accepted` enforces `accepted.targets == candidate.targets` (requiring exact transition approval for target additions, removals, or edits) and consolidates platform-key validation around canonical helpers. `validation_history::{FullHistoryLookup, current_target_only_diagnostic}` preserves target-scoped completion evidence isolation and feeds `TARGET_SCOPED_PROOF_REJECTED` diagnostics into `lib.rs::enforce_workbranch_readiness_before_push` and `verify::run_validate_and_hash_detailed`.

## Cloud Wave 2 control-plane, keystore custody, and Cloud workspace runtime (#1836, #1835, #1886)

- **Quota/cap taxonomy, spend inspection & metadata warnings (`src/decapod/core/error.rs`, `src/decapod/core/cloud_client.rs`, `src/decapod/core/cloud_todo_operation.rs`, `src/decapod/core/propodus.rs`)**:
  `CloudAuthStatus` adds `QuotaExceeded`, `OverageCapExhausted`, and
  `ProviderDisabled`, mapped cleanly in `cloud_client::diagnostic_for_status_code`
  and `AdapterErrorKind::Quota` in `transport_error_for_repository`, and classified
  under `FailureKind::Authorization` in `cloud_todo_operation`.
  `cloud_client::status_live` sends `AccountOperation::Status`
  (`operation: "resource_limits"`) with DPoP signing when enrolled and projects
  `resource_limits`, `spend_cap_status`, `spend_cap_warning`, and
  `propodus::warnings_from_cloud_metadata(&CloudResponseMetadata)` (`CloudMetadataWarning`).
- **Cross-platform OS keystore, DPoP revoke & device inventory (`src/decapod/core/cloud_dpop.rs`, `src/decapod/core/cloud_client.rs`, `src/decapod/cli.rs`, `src/decapod/lib.rs`)**:
  `DeviceKeyStore` supports macOS Keychain, Linux Secret Service (`secret-tool`),
  and Windows DPAPI/Credential Manager behind `DeviceKeyStorageBackend::OsKeychain`
  with strict non-symlink mode-`0600` (`PrivateFile0600`) fallback, plus
  non-secret `DeviceKeyInventory` (`inspect`) and `.meta.json` `device_key_id`
  persistence (`record_device_key_id`, `registered_device_key_id`,
  `clear_registered_device_key_id`). `cloud_client::revoke` separates
  `dpc_at_` / `--session-logout` DPoP-signed `action=logout` from management
  Bearer `device_revoke` / `--device-id` / `--all`, reconciles local state, and
  returns `OperationOutcome`; `cloud_client::devices` lists local device custody
  and optional `--live` readiness + header warnings.
- **SQLite-free Cloud workspace custody & validation (`src/decapod/core/cloud_workspace.rs`, `src/decapod/core/workspace.rs`, `src/decapod/core/container_runtime.rs`, `src/decapod/core/container_signer.rs`, `src/decapod/core/validate.rs`, `src/decapod/plan_governance.rs`, `src/decapod/lib.rs`)**:
  `cloud_workspace::{admit_cloud_checkout, check_cloud_authoring, custody_bind_or_reconcile, run_with_hosted_todo_store}`
  route Cloud `workspace ensure`, `validate`, and workbranch governance seeding
  through `DactylProofControlTransport` and `CloudCustodyStore` outside the repo.
  For `--container` workflows, `cloud_workspace::{HostCloudRelayHandler, LiveHostCloudRelayExecutor, RelayProofControlTransport}`
  and `container_runtime::verify_running_container_for_invocation` relay only
  bounded custody operations through `HostContainerSignerServer::bind_with_relay`
  while verifying immutable container `Id`, `State.Running`, workspace path, and
  `org.decapod.invocation` labels. `lib.rs`, `validate.rs`, `plan_governance.rs`,
  `trace.rs`, `proof.rs`, `governed_transition.rs`, and `assurance.rs` guard
  SQLite migrations, broker/archive/mandate/watcher checks, and auxiliary trace
  writes when `is_cloud_project` or `DbBroker::is_cloud()` holds. `todo.rs`
  fails unsupported `TodoCommand::Renew` early via `CLOUD_LEASE_RENEW_UNSUPPORTED`.

## Pay-and-use Cloud onboarding, diagnostics, and project routing (#1877, #1896)

- **Error & diagnostic taxonomy (`src/decapod/core/error.rs`, `src/decapod/core/cloud_todo_operation.rs`)**:
  `CloudAuthStatus` adds `RegistrationRequired`, `ProviderLinkStepupRequired`, `PaymentRequired`, `PendingFulfillment`, `AdminApprovalRequired`, and `OrganizationAlreadyEnrolled`. Both `CloudAuthDiagnostic` and `cloud_todo_operation::Diagnostic` add `enrollment_url: Option<Box<str>>` (keeping `DecapodError` under the 128-byte `clippy::result_large_err` ceiling), emit `enrollment_url` conditionally in JSON, and append `; enroll: <url>` in `Display`. `cloud_todo_operation::failure_kind` classifies `PendingFulfillment` as `FailureKind::OnboardingPending` (retryable), `PaymentRequired | AdminApprovalRequired | OrganizationAlreadyEnrolled` as `FailureKind::Authorization`, and `RegistrationRequired | ProviderLinkStepupRequired` as `FailureKind::Authentication`.
- **Pricing URL builder & repository-scoped error mapping (`src/decapod/core/cloud_client.rs`, `src/decapod/core/dactyl.rs`, `src/decapod/core/todo.rs`)**:
  `CLOUD_PRICING_URL` (`https://decapodlabs.ai/pricing`), `is_valid_repo_slug`, and `pricing_enrollment_url` validate `<owner>/<repo>` segments and RFC 3986 encode the query parameter. Shared `diagnostic_for_status_code` backs both `transport_error_for_repository` (delegated from `transport_error`) and `cloud_project::project_auth_diagnostic`, wiring repository slugs through `cloud_client::{acquire, one_time_error, connect, projects, enroll_device_with_connector, request_mfa_step_up}`, `dactyl::CloudConnector::discover`, and `todo::{cloud_todo_diagnostic_with_auth, cloud_error, cloud_adapter_error}`.
- **Account registration & organization-scoped routing (`src/decapod/core/cloud_project.rs`, `src/decapod/cli.rs`, `src/decapod/lib.rs`)**:
  `CloudProjectCommand::{Use, Create}` accept `--org` (`organization_id: Option<String>`), checked against `Project.organization_id`, `Creation.organization_id`, and `check_organization`. `identity()` auto-registers `registration_required` accounts via `AccountOperation::Register` (`operation: "account"`). `Response` accepts optional additive fields (`authorization_state`, `pending_fulfillment`, `cloud_org_id`, `admin`, `repo_assigned`, `organizations`, `projects[].organization_id`); `route()` requires `backend_authorized && status == "active" && pending_fulfillment != Some(true)` and invokes `project_auth_diagnostic` / `enrich_diagnostic` on failure.
- **Onboarding & init boundary (`src/decapod/onboarding.rs`, `src/decapod/lib.rs`)**:
  `setup_review_rows`, `customize_setup`, non-interactive Cloud init output, and `onboarding::ready` describe `/pricing` enrollment and automatic resume with no Admin setup wizard while preserving existing CLI contract assertions (`service confirms repository access` and omission of seat/billing pricing copy). `run_init_apply` guards `migration::check_and_migrate_with_backup_report` with `!init_with.backend.is_cloud() && !repo_ctx.effective_backend().is_cloud()` so Cloud refresh never initializes local SQLite.

## Constitution selection, knowledge, and evidence

`core::docs::resolve_scoped_context` selects applicable operation, path, and tag bindings before unique lexical matches. `core::context_capsule` and the RPC resolver use that selection without filling empty advisory results with unrelated text. The existing knowledge and federation owners retain inactive history and project current applicable advisory entries. Corrections commit the new entry and prior-state event in one broker transaction.

`assets/build/compress_constitution.rs` calls the shared source admission check before compression. The check reserves explicit binding and router identities, checks references, and requires review declarations for binding guidance. `assets/tools/constitution_history.rs` produces a reproducible chronology from immutable accepted source and a dated public release capture.

The native boundary adapter delegates through literal arguments to the current kernel. The flight recorder keeps the merged acceptance observer and adds an opt-in comparison over compact observations. Each operation uses the existing governance and proof owners.

## Pure report projection does not authenticate proof (#1726)

`core::governed_transition::project_report_publication` reads caller-owned JSON.
Its only production consumer is `core::projection::project_validation_report`;
it has no canonical producer, current task, candidate/base or authority lookup.
A digest and mutually matching caller labels therefore cannot produce PROVEN.
The existing pure helper retains failed/stale/unknown interpretations but leaves
passing reports not_proven and ineligible. The projection exposes an additive
optional `proof_reason` explaining the missing authority and current-validation
remedy. Report identifiers and source remain display data. Actual native
validation, canonical history/workunit proof and consequential publication retain
their existing owners and are not redirected through this projection.

The RPC result also labels all projection inputs as caller-supplied and not
canonically verified. `lib.rs::handle_interop_project` writes those constant
provenance fields after parsing the request; caller-provided origin/verification
labels cannot become authority. Approval formatting preserves the reported
value but does not assert that a policy transition or principal decision ran.

## Reversible integration lifecycle (`core::integration_removal`)

`src/decapod/core/integration_removal.rs` separates repository integration
scaffolding from project-authored instructions and retained governance/datastore
history:

- **Ownership provenance (`integration_ownership.json`):** `decapod init`
  (`lib.rs::run_init_apply` + `record_init_ownership`) and `decapod setup hook`
  (`lib.rs::run_hook_install` + `record_hook_ownership`) record SHA-256 digests,
  file modes, and pre-existing exclusions in
  `.decapod/managed/integration_ownership.json`. Uninitialized or legacy repos
  without that manifest require cryptographic release/fingerprint header proof
  on entrypoints and template/manifest hashes on specs; byte-identical authored
  files without ownership evidence (such as unowned `AGENTS.md` or hooks) are
  classified as `authored` and preserved.
- **Non-mutating inspection:** `Command::Integration` dispatches in `lib.rs::run`
  before session acquisition, migrations, or root-isolation guards, using
  `StorageLock::diagnose` and lock-free `dactyl_db::AccessMode::ReadOnly`
  probes alongside `fs::symlink_metadata` traversal.
- **Crash-safe restoration manifest:** `remove_integration` atomically persists
  `.decapod/integration-restore-manifest.json` (`status: "in_progress"`) with
  exact file bytes and `.gitignore` delta before unlinking any owned file, then
  transitions to `"applied"` and prunes only empty `.decapod/managed/`
  directories. Re-init (`decapod init`) detects absent
  `.decapod/managed/specs/.manifest.json` and regenerates missing scaffold specs
  without overwriting authored spec prose.

## Cloud control-plane, DPoP custody, Dactyl 0.18.0, and Local/Cloud branding (#1809, #1808, #1769, #1667, #1777, #1778, #1590, #1066, #1158)

- **Registry & Bazel lock alignment**: `Cargo.toml` pins
  `dactyl-db = { version = "=0.18.0", features = ["sqlite", "neon"] }` and
  direct `ring = "0.17"` (locked at `0.17.14`); `Cargo.lock` and
  `MODULE.bazel.lock` lock `dactyl-db 0.18.0` and `quinn-proto 0.11.19`.
  `flake.nix` reads `./Cargo.lock` dynamically.
- **Unified Local/Cloud CLI, capabilities, and store envelopes (`src/decapod/cli.rs`, `src/decapod/core/rpc.rs`, `src/decapod/core/propodus.rs`, `src/decapod/core/docs_cli.rs`, `docs/book/`)**:
  `resolve_cloud_env_var_with` prefers canonical `DECAPOD_CLOUD_*` env vars and
  emits once-per-process `CloudEnvDeprecationNotice` for legacy
  `DECAPOD_PROPODUS_API_URL`; `CloudRuntimeConfig::validate_datastore` fails
  closed without the retired Vercel preview default. `CapabilitiesReport` adds
  `backends` (`local`/`cloud`) and the `cloud` subsystem. `propodus.rs`
  recognizes `decapod.cloud.store` (`v1`) envelopes and `Decapod-Cloud-*`
  diagnostic headers alongside legacy compatibility envelopes.
- **Worktree-scoped todo routing (`src/decapod/lib.rs`, `src/decapod/core/todo.rs`)**:
  `Command::Todo` dispatches through `todo::run_todo_cli_in_workspace(&project_store, &workspace_root, todo_cli)`.
  Inline helpers (`add_task`, `get_task`, `list_tasks`, `claim_task_with_lease`,
  `release_task_as_actor`, `get_work_claim`) inspect `cloud_runtime(root)`
  (defaulting absent `config.toml` to local) and route through
  `inline_cloud_todo_factory` when `backend = "cloud"` while preserving
  `*_local` entrypoints for explicit local worktrees. `is_cloud_file_only_command`
  admits observational `workspace status` under `backend = "cloud"` without
  initializing local SQLite.
- **Ed25519 DPoP & host container signer (`src/decapod/core/cloud_dpop.rs`, `src/decapod/core/container_signer.rs`, `src/decapod/core/cloud_client.rs`, `src/decapod/core/dactyl.rs`, `src/decapod/plugins/container.rs`)**:
  `DeviceKeyMaterial` (zeroize-on-drop Ed25519 PKCS#8 via `ring`) and
  `DeviceKeyStore` store device keys outside repository paths (OS Keychain or
  strict mode-0600 `O_NOFOLLOW` file). `DeviceDpopSigner` and
  `ContainerBrokerDpopSigner` implement `dactyl_db::RequestProofSigner` with
  `observe_nonce` tracking and plug into `CloudConnector::with_proof_signer`.
  `HostContainerSignerServer` serves bounded Unix-socket signing for
  `--container` workspaces (`DECAPOD_CONTAINER_SIGNER_SOCKET`), enforcing
  origin, repository, agent ID, operation allowlist, capability separation, and
  immediate revocation while `filter_container_environment` strips raw secrets.
- **Repository access tiers & owner repair (`src/decapod/core/db_maintenance.rs`)**:
  Adds `RepoAccessTier` (`read`/`write`/`owner`), `CloudRepositoryAccessStatus`,
  verbatim `OWNER_REPAIR_V1_SQL`, `execute_cloud_owner_repair_batch`
  (`DactylBridge::atomic`), `fetch_cloud_repository_access_status` (`Read`),
  `evaluate_cloud_repair_authority` (`Write`), `mutate_cloud_repository_tier`
  (`Write`), `execute_cloud_owner_repair`, and local `run_maintain_recover`
  wired into all three `db.recover` CLI/RPC entrypoints.
- **Lease fencing & 5-state proof acceptance (`src/decapod/core/cloud_proof.rs`, `src/decapod/core/dactyl_todo.rs`)**:
  `DactylTodoStore` replaces `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED` with
  exclusive lease claim/renew/release (`claim_task_with_lease_blocking`,
  `renew_task_lease_blocking`) while preserving `#1247` static `atomic_mutation`
  post-condition guards and monotonic nanosecond markers. `cloud_proof.rs` owns
  `ProofLifecycleState`, `proof_capture_fingerprint`, `CloudControlError`
  (`403`/`413`/`429`/`outcome_unknown`), `DactylProofControlTransport` over
  `CloudConnector::control_request`, `custody_bind`/`custody_revoke`,
  `submit_capture`/`submit_verify`/`submit_admit_or_publish`/`submit_retire_historical`,
  `reconcile_operation_after_unknown` (`get_operation`), and
  `reconstruct_attestation`, wired into cloud `todo done --validated`.

## Current workunit validation judgments

`core::validation_history::finish_archive` retains immutable successful
observations and separately withdraws current workunit proof after a completed
task-bound full not-ready judgment. `core::workunit` appends the withdrawal to
its existing canonical journal, checking the observed generation and current
task/custody inside the same database transaction before publishing the cache
projection. The generation advances even when no proof had been promoted, so
an earlier success cannot later authorize that subject. Recovery restores the
journal's narrowed state; it does not reconstruct a success from diagnostics.

Current workunit selection filters canonical history by exact current task
binding before applying the existing proof and currentness checks. It never
falls back to older custody or skips a newer matching stale observation.
General historical lookup preserves its ambiguity contract. Separate stores
remain explicitly unsupported for atomic proof acceptance or withdrawal.

## Workunit declarations retain their proof owner (#1660)

`validation_rules::evaluate_workunit_rule` accepts the opaque, complete current
runtime result before canonical append, with a CLAIMED workunit, supported
full-validation plans, bound living specs and genuine capsule policy lineage.
`workunit::verify_capsule_policy_lineage_for_task` separately requires the
task's context state reference and internally consistent capsule. These
prerequisites stay enforced when recovering from a diagnostic declaration.

`core::workunit::transition_status` obtains current applicability from
`validation_history::current_workunit_proof` and fences accepted append in the
canonical task/proof transaction. File integrity and a declared pass cannot
replace that producer. `tests/consumer_readiness.rs` exercises this through
real CLI task/workspace admission and a current checkpoint using the existing
consumer fixture. It introduces no proof engine or alternate datastore.
Separately initialized workunit stores retain their explicit unsupported
atomic-domain boundary; this regression does not claim convergence.

## Source coverage and execution observations (#871)

The recursive unique-key JSON decoder previously local to
`core::derived_release_profile` now lives in shared internal `core::json`.
Existing release-manifest readers and RPC framing reuse it. Callers retain
their framing budgets and typed schema/diagnostic owners. The extraction adds
no proof authority. Actual-binary controls refuse nested and escaped duplicate
keys on both RPC transports, then recover ordinary negotiation; a separate
Python implementation checks the focused JSON meaning on the same host.

The Bazel interoperability target declares the included Python source as
compile data, matching Cargo's direct include dependency. Required test targets,
commands and budgets are unchanged. The first Bazel run correctly failed to
build when that input was undeclared; no interoperability execution is claimed
for that failed build.

The required broker preparation probe measures the competing mutation's elapsed
time before releasing the mock preparation and awaiting its client cleanup.
It retains the strict ten-second bound and explicitly checks that preparation
was still blocked when the competing mutation completed. Cleanup time cannot
impersonate a measured broker serialization failure; product behavior and the
required native recipe remain unchanged.

The library regression compares every RPC session descriptor with the existing
session bypass predicate and the earlier read-only database inspection route.
Four stale false descriptors are corrected without changing those guards.
The actual-binary manifest scenario checks their emitted values separately.

`lib.rs::parse_rpc_request` propagates malformed flag-parameter JSON through
the existing validation error boundary, just as malformed stdin framing does.
Only absent parameters default to an empty object. Existing specialized
inventory and maintenance diagnostics remain in their own branches. The
black-box regression uses a fresh private Git project, removes ambient gate and
provider overrides, proves rejection before project/session files exist, then
initializes a real project and executes valid ordinary negotiation controls.
It also replays the malformed required extension on that initialized project
before the valid controls, exercising the original false accept and recovery.

`interop::rpc_catalog` emits registered names and manifest metadata from the
same declarations. The public `lib.rs::run` entrypoint builds unknown-name
refusals with `core::rpc::error_response` before resolving project state, opening
sessions or entering specialized schema/database routing. Registered operations
continue through their existing handlers and authority gates. Black-box controls
cover the formerly omitted reconciliation capability and confirm that unknown
names return the typed envelope in a fresh Git fixture without project/session
initialization.

`src/decapod/core/interop.rs` describes implemented operations and known test
locations without executing tests during discovery. Static coverage tables
populate references with `NotRun`; `validate_manifest` refuses execution-state
assertions lacking an authenticated observation surface. Existing full-validation
and canonical workunit producers remain the owners of their own proof domains;
their mere existence cannot prove that a particular conformance test executed.
Adding current observed conformance still requires an explicit binding to those
owners and independently verified test execution, rather than another registry.

## Source-policy currency continuity (#1753)

`entrypoint_integrity::preserves_source_entrypoint_versions` reuses the existing
bounded source-policy reader and requires all four regular entrypoints to match
the committed canonical bodies and version fingerprints. Publication currency
uses generated-manifest release provenance only under that authorization, with
conservative fallback to the base header when provenance is missing/invalid.
Evaluating-release upgrades still refresh Docker and manifest pins. The ordinary
repository rule, required-file checks and sibling integrity gates remain intact.

## UTC timestamp dependency (#1755)

`core/time.rs` owns UTC RFC3339 parsing and established output precision using
`time::OffsetDateTime`. Storage models use explicit string serde; Dactyl retains
nine-digit transaction markers and accepts legacy SQL UTC timestamps. Cloud
session and policy expiry still fail closed on invalid input. Epoch-Z command
envelopes retain their separate existing representation. Cargo and Bazel resolve
the same maintained dependency, without disabling dependency-policy checks.

An explicitly configured internal Decapod-development policy can tolerate
entrypoint release/package version drift only for the exact canonical Decapod
repository identity and verified source lineage. It still reconstructs each
immutable commit's source template and validates its release-bound fingerprint.
Consumer repositories retain strict alignment. This policy does not waive
execution, custody, source, target or publication evidence and is omitted from
normal generated configuration and agent discovery.

## Publication proof continuity (#1747, #1748)

`core::validation_history::PublicationReadiness` owns the runtime's opaque
completed evidence, original task binding and external evidence directory. It
is constructed from the live result, never from an agent-supplied receipt.
Publication reuses `CurrentEvidenceInputs` to measure source/history, policy and
native tool inputs, and rechecks the existing task/workunit/epoch binding.
`workspace_publication` compares remote repository, branch, head and base to
that same proof before creation and during final readback. Its final callback
owns the failure/quarantine boundary, preventing a later unchecked error path
from returning admission. The existing provider and native proof engines remain
the authority; no additional proof store or reusable admission token is added.

## Agent-facing escalation convergence (#1690–#1692)

`core/mentor/reflection/escalation.rs` composes source observations with the
existing `reflection` continuation, `plan_governance` journal and
`plugins::policy` authority owner. It adds no tables or reasoning runtime.
Typed proposals bind canonical intent and the missing subject; receipts retain
digests, source identities, typed reasons, progress and dispositions. Prose and
literal source bodies remain transient. The existing lock, custody checks,
freshness checks, idempotent submissions, 64 KiB journal limit and three-round
budget remain authoritative. Literal byte predicates do not establish semantic
relevance, source authenticity, actual test execution or publication readiness.
`governed_transition::CapabilityGap` distinguishes agent-directed inference
escalation from a separately admitted human dependency. Cooperating harnesses
invoke the protocol; arbitrary external conversation cannot be intercepted.

## Core and CLI selection dependency closure (#1672)

`assets/tools/source_ci_profile.rs::{core_test_inputs,cli_test_inputs}` composes
the existing source selectors with their recipe test inputs. Shared consumer
fixtures and Bazel target declarations select both existing lanes; named runtime
fixtures select the full core recipe. CLI aggregate inputs select the unchanged
three-target recipe. `selected`, `commands` and `inventory` remain the existing
authority surfaces, covered together by
`required_test_source_changes_retain_the_complete_owning_recipe`. This changes
selection, not check IDs, execution commands, release-only disposition or the
accepted controller's authority. Shard activation and performance evidence remain
separate obligations under #1672.

## Checkout custody and non-Git diagnostics (#1661)

`core::authoring` enforces one live checkout per task lease transactionally with
the existing task journal and workspace lifecycle events. Renew advances the
binding generation; yield, release, reclaim, handoff, terminal status and prune
retire it. `core::fleet_coord` treats `resource:<name>` as a resource reservation,
not a repository path. Release and publication consume the corresponding gate.

`workspace::get_workspace_status` remains fail closed for non-worktrees and
unavailable custody evidence. `lib::run_validate_command_local` permits non-Git
projects to execute only explicitly partial diagnostic scopes without workspace
admission. A successful project check has no publication judgment. Full validation
still requires a registered, governed candidate. Host custody remains cooperative
and is not an OS sandbox against direct filesystem or Git access.

## Composed reflection continuation

`core/mentor/reflection.rs` composes the existing mentor ADR detector, canonical
plan constraints, execution-custody capture and governance-document lock/atomic
writer. Orientation and `mentor.obligations` share this implementation. One
optional journal in `GovernedPlan` preserves at most three compact receipts and
a terminal stop; it adds no database table, index, event stream or transcript.
Current plan/accepted-base, policy, ADR and supplied source identities invalidate
dependent conclusions. The existing plan execution, trajectory checkpoint/resume
and validation boundaries reject binding unresolved or stale reflection.
The unchanged approval and exact-candidate publication gates remain necessary.
The reflection contract hash reuses the plan's existing acceptance/progress
reset on a clone. Approval and measured phase/gate progress do not consume
reflection rounds; changes to the actual phase/gate definitions still invalidate
convergence. The original execution and proof gates remain authoritative.
Mentor text matches stay advisory. `state::project_federation_decision` now
honors lifecycle before labels and preserves confidence, scope and provenance;
an inferred/imported accepted tag cannot become a human-confirmed decision.
ADR context identity and potential-conflict detection consume the same bounded
captured bytes, followed by current-source revalidation before the atomic write.
Case-folded proposed paths and challenge identities prevent spelling-only
aliases from being counted as progress on case-insensitive workspaces.

## Registry dependency closure (#1638)

`Cargo.toml` pins Dactyl 0.16.1 while retaining `sqlite` and `neon`, and the
existing default `supabase-cloud` feature still forwards `dactyl-db/supabase`.
Cargo resolves the registry checksum and patched Quinn protocol floor; pinned
Bazel regenerates its crate-universe metadata from those same committed Cargo
inputs. Nix consumes `Cargo.lock` directly and needs no Git output hash or
separate package pin. The SDK requires Rust 1.85, below the existing Decapod
1.96.1 toolchain. HTTP/3 stays disabled. No cloud/session/authorization code
is changed by this dependency alignment.

## Managed cloud connector and crash-safe custody (#1628)

The agent-facing handoff carries a validated `authorization_url` separately from
recovery text. `core::cloud_client::pending` directs the caller to poll
`decapod cloud login` at two-second intervals for at most five minutes and then
resume its original task. Individual invocations stay bounded and daemonless.
`cloud login --restart` is the explicit agent recovery entrypoint for uncertain
one-time state; it never repeats the interrupted exchange. `core::todo` retains
the original add operation ID and browser URL when connection preflight fails.

`core::cloud_client::connect` composes the public Dactyl `CloudConnector` with
the private `core::cloud_session::SessionStore`. Dactyl resolves discovery and
the physical route; the active init/session/TODO path does not use the legacy
Propodus HTTP client or ambient provider/token selectors. Decapod supplies the
Git-origin repository target and an opaque bearer to the managed bridge.

The session binding contains the validated origin, repository, and canonical
main-checkout identity. Linked worktrees share a binding; distinct clones and
origins do not. Private Unix files, no-follow opened descriptors, ownership and
link checks, bounded records, a stable exclusive lock, and synchronized atomic
replacement protect durable custody. Credentials and pending bootstrap state
are never stored in the project. Every persisted phase validates its required
session/bootstrap/code shape before orchestration uses it.

Preparing, code exchange, session exchange, and refresh persist an in-flight
marker before dispatch. Reopening an interrupted one-time phase yields
`RestartRequired`; no uncertain mutation is replayed. Status polling and
same-session account completion resume safely. Received codes and rotations
are durable before later requests. Explicit bearer rejection invalidates that
same bearer under the refresh lock without discarding a newer concurrent
rotation. Denied or unresolved entitlement retains a safely retryable account
completion. `DactylTodoStore::check_read_access` uses an existing read-only,
service-authorized task query; health checks never stand in for authorization.

Unsupported cloud repository-datastore commands fail before local migration,
presence, group-broker, or SQLite startup. Explicit file-only controls retain
their own path; this is a capability boundary, not remote kernel parity.
`tests/cloud_managed_session.rs` exercises the binary through local fixtures;
it cannot establish server RLS, PostgreSQL atomicity, or protected hosted policy.

## Agent-invoked authority across local and cloud repositories

The repository's agent remains Decapod's operational caller. Solo/local work
uses its existing cooperative session and workspace custody; no external
account is required. In cloud mode, Decapod carries a managed session bearer
to Propodus through Dactyl's opaque storage context. Propodus independently
resolves the authenticated GitHub-linked principal, live membership,
repository grant and entitlement. A client-provided agent ID or local RBAC
role label can identify the executing agent but cannot elevate cloud authority.

The coordinating backend maintains `read`, `write`, `maintainer`, and
`owner` tiers; a contractor is a user with a bounded repository-scoped
grant, rather than a special class of agent. Cloud task custody couples an
agent label with its authenticated principal, so the same label supplied
under a different principal cannot release or complete the claim. Reviewed
cross-principal repair remains a separate privileged operation. These
backend guarantees are developed and proved in the companion Propodus
migration; this repository's client fixtures prove only request routing and
denial propagation, not remote PostgreSQL authorization or Git merge rights.

## Storage-independent version dispatch

`src/decapod/lib.rs::run` dispatches `system version` alongside the bootstrap
commands before project discovery, session acquisition and migration. Binary
version metadata has no project-store dependency. Normal project commands still
initialize storage and report a detected migration once; version reporting
neither consumes that notice nor repairs unsafe datastore permissions.

## Bounded macOS epoch enumeration

`core::validation_epoch::input::mac_directory` enumerates a pinned directory
through `fdopendir`, using a new `openat` description for an independent cursor.
Entry classification uses `fstatat` with `AT_SYMLINK_NOFOLLOW` relative to that
same stream. Owned names and type flags feed the existing repository-signal
walker; file selection, hashes, entry/depth/byte/time bounds and safe reads remain
owned by their existing functions. macOS never reopens an observed pathname or
uses `/dev/fd` for enumeration. Linux retains its `/proc/self/fd` reader. This
internal iterator adjustment adds no public command, authorization or datastore
contract. Regression coverage replaces an enumerated path with a hostile symlink
and drops the original handle while verifying the stream remains pinned.

## Governed transition judgment

`src/decapod/core/governed_transition.rs` is the kernel that decides whether an
attempted change is admissible and whether a completion or publication claim is
proven. It does not choose the next unit of work. Canonical head, rules
fingerprint, and verifier come from `state::canonical_snapshot`, which is the
same revision and provenance resolver used by `state.resolve`. The judgment
does not keep a second definition of that identity.

`plan_governance::instruction_readiness` evaluates recorded instructions from
the current governance plan. Orientation projects those same blockers and
remediation, and approval, execution checkpoints, validation, and publication
enforce them. Its source revision binds the recorded inputs independently of
approval or phase progress. The existing validation epoch binds proof to the
applicable plan after a correction.

For selected work, plan approval also binds the accepted policy's semantic
revision. The shared readiness gate compares that revision before progress or
publication. Refreshing a target with unchanged selected-work policy preserves
plan approval. Reapproval after policy changes resets old phase progress.

A task verification row is an index. `judge_stored_verification` re-hashes the
bound files and checks that the proof digest matches that reconstruction under
the canonical snapshot. A pass status or a digest written only into SQLite is
not `proven`. Material decisions are appended to the existing `traces` event
stream and rendered by the flight recorder.

Canonical order is Git identity, then task revision, lease, and claim scope in
the datastore, then evidence bound to that identity. Projections such as
`project_validation_report` derive a judgment from those records. Task comments,
verification notes, `passes: true`, and an agent-written `publication_ready`
boolean are not authority.

`effective_verification_status` returns `unproven` when a passing status is
bound to a task revision but `proof_plan_results` has no output digest.
`work_claim::from_todo` then stays blocked. `state_commit::verify` rejects an
empty scope record or a missing expected root. `state_commit::freshness`
reports `stale` when the committed head, rules fingerprint, or verifier differs
from the current identity, and `unknown` when either identity is incomplete.

Validation rules no longer drop an Error or Critical rule because a forbidden
path changed. `evaluate_workunit_rule` does not pass because a manifest key
exists. Project and task overlays cannot replace a `NonOverridable` rule body.
The publication-phase rule `decapod.publication.fresh_proof` runs only when the
caller sets `include_publication`.

Verifier integrity, runtime capability attestation, and blocker classification
live in the same kernel. Each isolation axis on `VerifierEnvelope` is an
`IsolationAttestation` with its own observation, source, and evidence
reference. A reference on one axis cannot establish another. A native-trace or
external-observation label without a usable reference stays `unknown`.
Integrity also compares `canonical_input_hash` with the completion evidence
`inputs_hash`. `CapabilityAttestation` keeps observed support only when a
native or external evidence reference is usable and the `StateIdentity` is
current. It is not `capabilities::CapabilityRegistry`. `classify_blocker` uses
`AdmissionCause` and optional `ProofCause`. It ignores reason prose.
`transition_trace` adds `blocker_class` and `required_capability` only when a
class is known. `capability_gap` is a pure record: runtime for context, state,
tool semantics, coordination, policy, verification, and mechanical work, and
frontier only for inference. No second state machine, harness adapter, or
capability router was added.

## Claim scope command coverage

The `claim_boundaries` module in `tests/cli_contracts.rs` runs distinct CLI
processes from a main checkout and a separate Git worktree. A barrier starts
two disjoint explicit-file claims together, then read commands from both
worktrees verify canonical-store identity and persisted assignment. The test
also submits an overlapping claim and verifies that rejection leaves it
unassigned. No database state is injected to construct either scenario.

## Historical proof replay identity

`core::validation_history` preserves successful full-readiness evidence in the
existing local canonical verification event stream, indexed by the validated commit.
It adds no parallel datastore or background service. `lib.rs` captures the
task binding before proof starts and rechecks it at the durable append boundary.
Native, repository-local and provider execution share that boundary; partial
CI checks, snapshot preparation and failed proof cannot create successful
history. An append failure revokes the returned publication readiness.
Cloud-backed validation keeps its existing authority path; this local history
implementation does not claim cloud task completion support.

The archived record retains the complete bounded check plan, execution results,
material identity and optional decision assessment. It survives deletion of the
temporary execution directory without promising permanent raw build logs.
Full proof remains outside `.decapod/governance.json` and the frozen candidate,
so recording it does not change the commit it proves. The tracked validation
section retains its existing prerequisite-only meaning.

`plugins::verify` uses a distinct historical completion proof. It checks the
original task contract, owner and lease lineage against the canonical task
journal, then verifies integration into the accepted branch. The validated
candidate, its original accepted base and the observed integration are separate
identities. Task completion and proof repair use the existing atomic revision,
dependency, verification and journal transaction. A completed but unverified
task retains its original completion timestamp and revision when repaired.

Historical replay does not run current-workspace validation or bind unrelated
later work to the old task. It cannot satisfy the full current-candidate gate
used by `workspace publish`. Missing, corrupt, ambiguous or mismatched evidence
fails closed rather than selecting a convenient successful run.

`workspace_publication` derives omitted GitHub PR metadata only on the creation
path. It reads non-merge commits between the verified remote base and head, and
then rechecks admission immediately before creation. Existing matching PRs
continue through readback without changing their metadata.

## Candidate and accepted-target custody

Source-CI validation binds execution evidence to the immutable candidate and
accepted target revisions. When master advances, the worktree must incorporate
that accepted revision and generate fresh candidate-bound evidence; a push proof
that names an older base is a custody failure even when the Rust tests passed.
The #1524 validation failures were this target mismatch, not evidence of a test
failure. Release projection refreshes remain required when the installed
Decapod version changes.
The validation trajectory remains bound to the original claimed PR task while
supporting reconciliation work is attached to the same governed plan; auxiliary
work items must not replace the identity used to authorize publication.
The v0.126.2 master update changed only the Decapod package version in the
reviewed `Cargo.toml`/`Cargo.lock` inventory. The workflow, action and adapter
contracts were unchanged, so the required-profile source digest is updated to
the complete current inventory reported by `decapod init --proof --dry-run`.

## Consumer inventory test boundary

`tests/support/justfile_inventory.rs` copies the Rust fixture under
`tests/fixtures/justfile_consumer/` into an isolated Git repository and runs the
actual initialization CLI. It compares the inventory's `Justfile` digest with
exact index bytes. The macOS test checks APFS and inode aliasing before exercising
the same path, failing instead of skipping incompatible filesystems. Fixture
module tests are compiled into `cli_contracts`, avoiding nested builds.
`source_ci_profile::commands` runs that module in the existing required macOS
job as well as the Linux CLI target; normal custody and candidate binding apply.

## Broker caller custody

The finite group broker in `src/decapod/core/group_broker.rs` serializes local
mutations without adopting the elected leader's identity. Protocol version 3
requires an explicit caller context: canonical working directory and a closed
allowlist of agent, session, task, correlation, idempotency and claim-autorun
fields. Each child command clears those inherited fields before installing the
request values; absent caller metadata cannot fall back to the leader's values.
The ordinary session and operation-policy checks still run in that child.
Context conveys the caller's execution inputs, not a grant of trust or approval.

The local `todo archive` command identifies a typed refusal from its policy
gate before entering the task transaction. The broker's own child conveys that
outcome through a reserved exit status. The parent records the denial using
the existing durable dedupe envelope. Other failing child exits continue to
leave the requested mutation's outcome uncertain.

The active session password is transported only on the existing restricted local
socket, without a debug representation, response echo or durable request storage.
Dedupe stores only the result and a structured digest of arguments and non-secret
caller context. That digest binds identity and worktree while permitting password
rotation for an already authenticated same-caller retry. Unknown context fields
and inconsistent payload hashes fail closed; old context-free broker clients
cannot silently execute under a leader's identity. Caller isolation remains process-based. Election and storage ownership have finite
foreground lifetimes; this adds no daemon or service.

`tests/group_broker.rs` exercises two callers claiming distinct tasks through a
third caller's live broker and checks returned and persisted ownership; the
unit suite covers actor/workspace replay separation, credential rotation,
rejected context keys and removal of absent leader metadata. These are required
regressions, not a claim that unexecuted candidate tests have passed.

## Native completion custody

`validation_runtime::write_native_completion` writes and syncs native receipt
and verdict into a private temporary directory outside the frozen checkout and
Git metadata, then publishes the pair with one directory rename. No fallible
operation follows that publication point. Creation, partial-write or sync
failure leaves no published successful pair and the returned report fails closed.
`workspace::ensure_required_governance_artifacts_in_pr` checks required intent
and trajectory material independently of legacy preparation receipts; the full
readiness boundary remains responsible for all selected proof and final custody.

## Architecture Overview

Decapod is a Rust governance kernel invoked by coding agents through a CLI and
structured RPC. It connects human intent, project authority, coordinated
execution, and evidence in repository-owned state. Each invocation is finite;
the repository carries the work between invocations.

Source adoption inventory in `src/decapod/core/init_reconciliation/mod.rs` binds
actual directory-entry paths, rather than successful filesystem probe spellings.
The `source_entry_path` resolver checks source components and the existing
reviewed symlink policy first, then resolves each existing discovery root to its
exact entry spelling before `Discovery::visit` hashes it. This keeps a single
`Justfile` from producing a phantom `justfile` on case-insensitive filesystems;
distinct entries on case-sensitive filesystems remain separate even when their
contents or underlying inode are identical. Discovery still includes untracked
files and works before Git initialization, so a Git-only listing cannot hide new
build inputs. Snapshot binding remains exact and does not forgive case-only
working-tree drift from the tracked candidate.

Spelling resolution reads at most 8,192 entries per containing directory and
rejects ambiguous or unresolved aliases, while the existing source entry,
depth, per-file and total-byte bounds remain in force. Failure returns an
incomplete-discovery blocker rather than a partial accepted inventory. Tests in
`init_reconciliation/tests.rs` cover both Justfile spellings without Git,
same-content and hardlinked distinct names, nested spelling resolution and the
lookup bound; their filesystem-sensitive branches exercise the host's actual
case behavior and do not claim emulated APFS or Windows execution on Linux.

`snapshot_provider::public_buildkite::CurlPublicReader` retains its typed,
anonymous GET authorization and bounded redirect loop. Artifact redirects admit
the exact documented `https://buildkiteartifacts.com/` origin and legacy owned
S3 bucket, then apply the same four-UUID layout, producer-job and safe object-path
checks before following. Provider metadata and content hashes still bind the
download. No arbitrary host, authentication, alternate downloader or new backend
is introduced. This controller prerequisite keeps the accepted active evidence
policy; the separate stricter rollout waits for independent controller release.

Functional qualification and product permission remain distinct. Following the
maintainer's delegated decision, the native reducer allows valid conservative
abstention on a ready control while retaining the original expected-accept
mismatch. Both actual backends must demonstrate genuine ready-work approval and
every adverse/protocol control must pass. Raw response and numerical verification
remain mandatory. The shared live predicate continues denying abstaining work;
qualification cannot convert uncertainty into publication permission or claim
all oracle labels matched.

## Direction

The d3ebad07 eight-run recipe actually compiled and passed40native tests and
real Metal probe/32recurrence comparisons. Pinned download failures56/28
prevented all three numerical model comparisons and most evaluation phases.
The last CPU shard executed22cases, with4false-ready proxy outcomes,
11abstentions and1unsupported-length case; this is incomplete evaluation.
The final aggregate remains strict, but now retains independently collected
phase diagnostics even if an earlier missing numeric export stops scientific
verification. An error report is not acceptance or new model-equivalence proof.

The product is a local-first command-line runtime with a structured process RPC
surface. Agents invoke it at decision and state-transition boundaries. Durable
repository records provide continuity instead of a resident orchestration service.

## Current Facts

- Rust implements the `decapod` binary and its supporting library
- The binary embeds Constitution data and agent documentation
- Local runtime state uses Dactyl-backed SQLite; cloud routing is optional. The
  SQLite shared library is loaded dynamically from the host, and the Nix flake
  package carries it in its runtime closure (`packages.sqlite-runtime`, wired into
  `packages.default` via `LD_LIBRARY_PATH`) so `nix run` needs no host libsqlite3 (#1536)
- `packages.default` is reproducible on `x86_64-linux`: two builds from the same
  locked inputs are byte-identical (`nix build --rebuild`). On `aarch64-darwin`,
  upstream Nix rebuilds can still differ in the Mach-O `LC_UUID` and its one
  code-signature page hash; Lix rebuilds are byte-identical. The Nix flake
  workflow checks the `x86_64-linux` rebuild on each release merge. The
  constitution build script
  (`assets/build/compress_constitution.rs`) serializes only `BTreeMap`s, so the
  embedded JSON keeps one key order, and the flake maps the build directory to
  `/build`, because on Darwin it is random and rustc embeds it (#1176)
- The Nix flake workflow has credential-free release identification and build
  proof, followed by a separate native GitHub Actions-only Cachix publisher.
  The publisher executes native locked builds, smoke checks and Linux rebuild
  proof again before sending `packages.default` for `x86_64-linux` and
  `aarch64-darwin` to `decapod`. It requires a valid `CACHIX_AUTH_TOKEN` and
  fails closed if missing; Buildkite import, manual runs, ordinary merges and
  pull requests cannot publish (#1177). The reviewed workflow hash in the
  compiled readiness profile and the source-inventory digest must both match
  this reviewed source definition.
- Git worktrees and supported containers provide task execution isolation
- CLI and Decapod-specific process RPC are the current integration surfaces

## Topology

The runtime is organized by responsibility:

```text
Agent or harness process
  decapod CLI / structured RPC invocation
    Authority and context
    Task custody and workspace lifecycle
    Governance records and proof gates
    Storage facade and bounded provider adapters
Repository
  Source and Git history
  Authored specifications and tracked governance
  Runtime datastore and managed evidence
```

## Component Responsibility Matrix

`src/main.rs` enters the library runtime. `src/decapod/cli.rs` defines command
surfaces; `src/decapod/lib.rs` dispatches them. The core implements governance,
state, task custody, workspaces, context, and proof. Plugin modules provide
capabilities such as knowledge, federation, policy, health, and bounded
automation. Embedded Constitution data and agent documentation ship with the
binary.

The agent interprets the request, plans and performs work, configures the
project's build, test and CI paths as needed, writes living specifications, and
gathers evidence. Decapod steers those responsibilities through project rules,
state and actionable reports, evaluates supported transitions, and persists
their state. Validation is one phase in that lifecycle. The runtime does not
own a model loop or keep a background service running for a task.

## Native command sequences

`core::native_proof::Invocation` retains the existing reviewed direct command
and may carry up to 63 ordered `InvocationStep` successors. This extends the
same bounded runner rather than introducing a workflow interpreter. All commands
share one owned scratch directory, the original check deadline and cumulative
output allowance. Each executable joins the measured toolchain closure; the
invocation digest binds the complete ordered argument, directory and environment
declarations. Failed or unexecuted successors cannot satisfy the parent check.
The source adapter uses this for finite Cargo recipes when their existing
platform and toolchain prerequisites are met. mdbook still needs its independent
version-resolution adapter. Shell, Docker, generated executable,
foreign-platform and derivative adapters remain explicit obligations; sequence
support alone does not establish complete native/Buildkite equivalence.

`assets/tools/build_lifecycle.rs` supervises Bazel (`--run_under=//:build_lifecycle`)
and Cargo build/test invocations inside an atomically allocated per-invocation
scratch directory. After child exit and bounded process-group teardown,
`Scratch::clean` verifies parent and scratch directory identity before removing
the owned scratch tree. On Unix `PermissionDenied` (including mode-`0o000`
rootless container overlay directories left under a disposable `HOME`),
`Scratch::recover_owned_permissions` restores owner `0o700` traversal only on
verified same-UID, same-device non-symlink directories within bounded depth and
entry limits, leaving `cleanup_diagnostics` read-only and external caches or
symlink targets untouched.

## Execution Path

An invocation parses a command or RPC request and resolves its project context.
The applicable session, authority, claim, workspace, and policy checks run before
the governed operation. Stateful paths reconcile supported migrations and use
the canonical storage boundary. The operation records its required evidence and
returns a scoped result or actionable blocker. Read-only discovery remains
available for diagnosing missing or damaged state.

## Intent, authority, and context

The human request and clarifications establish meaning. The agent records its
accepted interpretation, scope, assumptions, and proof expectations in governed
project artifacts. This context remains valid when execution moves into an
isolated workspace.

The Constitution provides structured engineering guidance. `core/DECAPOD`
routes broad intent toward the smallest relevant doctrine surface. Project
configuration and recognized directive bodies in `.decapod/OVERRIDE.md`
specialize that authority. Resolution records the applied directive, source,
hashes, byte count, and precedence; a malformed or conflicting overlay fails
visibly.

Orientation and context capsules bind selected sources and policy to the
task. Capsules use canonical serialization and hashes so consumers can verify
their recorded inputs. A hash establishes content identity, not the truth or
completeness of the selected context. Repository mapping respects tracked and
non-ignored inputs rather than traversing dependency and build trees without
bounds.

## Concurrency and Runtime Model

Sessions identify local agent custody. Todos and their claims coordinate
assignments, leases, dependencies, and handoff. Exclusive claims reject active
ownership conflicts; supported shared ownership remains explicit.

Workspace creation requires the applicable claim and builds an isolated Git
worktree or supported container-backed workspace from an accepted committed
base. The root checkout's unrelated staged, tracked, untracked, and ignored
changes remain in place. Branch names and directory names alone never establish
ownership.

The canonical governance root resolves shared runtime coordination for managed
worktrees. Authored specs, generated workspace projections, and the active
tracked governance document belong to the work branch. This separates shared
coordination from the candidate files being reviewed.

Container and workspace lifecycle receipts bind the invocation, path, directory
identity, and creating runtime where applicable. Bounded subprocess helpers
capture output and terminate and reap owned children on failure. Cleanup acts
only on established ownership and preserves uncertain resources with a recovery
diagnostic.

`plugins/container.rs::finalize_container_branch` separates child success,
publication, foldback and disposal. Failed commands may return their branch for
recovery but never invoke push or PR publication. Successful PR requests call
`workspace::publish_workspace` on the owned clone, using the same governance,
exact-candidate validation, normal push and verified draft-PR path as the public
workspace command. Publication may create a checkpointed commit; foldback occurs
afterward so the host receives the final candidate. Push-only execution returns
the branch first. A local-only/publication option conflict fails before runtime
startup, and an inconsistent explicit/configured publication base fails with
retained-workspace recovery instructions.

Foldback uses Git's ordinary non-forcing ref update, including its atomic rejection
of non-fast-forward host divergence. A checked-out host branch uses the existing
fast-forward-only path. Neither recovery nor success authorizes overwriting
independently advanced host history.

`cleanup_workspace_clone` separates runtime removal from workspace disposal.
Failed or interrupted commands keep the clone even if foldback succeeded.
Successful disposal requires foldback, the expected attached branch, an ordinary
visible index, no added or changed refs outside the returned branch, and the
bounded shared `workspace::worktree_is_dirty` check. `prepare_workspace_clone`
captures inherited refs after initialization; comparing raw ref OIDs preserves
new stash, side-branch and annotated-tag custody without treating unchanged
inherited refs as new work. Index inspection is NUL-delimited and never clears
assume-unchanged or skip-worktree flags. Unknown Git state fails closed with the
retained path; ownership is rechecked before removal. These are cooperative,
post-execution checks, not an atomic filesystem snapshot or reflog preservation.

## Deployment Topology

The installed product is an on-demand executable on the host or in a managed
execution container. Its managed Dockerfile supplies the agent's execution
environment; an application's own Dockerfile remains a separate product artifact.
Git, the selected container runtime, optional hosted storage, and optional remote
providers have independent availability and access requirements. Selecting an
adapter does not establish that its external service was provisioned or verified.

## Data and Contracts

Decapod separates several kinds of state:

- `.decapod/config.toml` records project configuration
- `.decapod/OVERRIDE.md` records human-editable project authority
- `.decapod/managed/specs/` holds the agent-authored product contract plus
  explicitly marked generated projections
- `.decapod/governance.json` is the single tracked governance document for
  the accepted baseline and active change, including claims, checkpoints,
  plan, trajectory, validation, and optional observations
- `.decapod/data/decapod.db` is the canonical local runtime datastore for
  coordination and event evidence
- Ignored workunits, context capsules, and managed artifacts hold current
  execution state and evidence whose custody is governed by their contracts
- Git preserves accepted source, specifications, and governance history

A fresh work branch receives a fresh active-change journal while carrying
accepted policy and unresolved obligations forward. Ordinary repeated calls,
renames, and rebases preserve the active workstream. An accepted merge and a
published release are separate facts.

Starting another trajectory run is a guarded journal transition. Under the same
governance-document lock, `trajectory::init_trajectory` proves that the complete
predecessor document is recoverable from Git before replacing trajectory and
intent custody. Historical Jev observations remain read-only and preserved. An uncommitted failed or interrupted run
cannot be discarded by choosing another run ID; the agent continues that run
or checkpoints and commits it before starting another. Git remains the history
store rather than a second snapshot database.

`governance_document::checkpoint` checks unstaged material and ID reuse inside
the existing document lock, before and after loading current authority and
measuring material respectively. This prevents a cooperating writer from
classifying another checkpoint’s temporary publication file as authored input. It preserves the
first record, rejects conflicting content before saving, and treats an identical
request as a no-op. Proof references are sorted and deduplicated before comparison.
The material digest includes staged source and governed semantic inputs, but
excludes the checkpoint list itself. Cooperating document writers serialize;
this does not claim isolation from an unrelated process mutating the Git index.

Governance document writers lock the complete read-modify-write operation and
write atomically. Related sections share intent and epoch references instead
of creating competing copies. Generated views remain projections of their
inputs and never become a second authority.

Plan acceptance is bound to its execution contract through the existing locked
mutation boundary in `src/decapod/plan_governance.rs`. Material plan, phase, or
gate definition edits return the plan to `DRAFT` and clear every phase/gate
progress flag and timestamp. Exact no-op updates preserve progress. The
conservative all-phase reset prevents later phases from retaining evidence that
depended on an earlier contract. Reapproval is a separate declared transition;
phase entry/completion cannot manufacture approval from draft state.
`validate_plan_progress` is shared by explicit transitions, execution checks,
and the publication validation gate to reject inconsistent completed state.
This adds no separate storage, authority service, or harness-specific runtime.

## Persistence boundary

Decapod owns domain schemas, transition rules, migration order, identity,
retry policy, and the meaning of events. All application storage access crosses
the Dactyl facade in `core::dactyl_db`. Dactyl owns physical execution, access
mode, atomicity, normalized results, and its supported integrity and recovery
operations.

Local physical connections hold shared advisory lifecycle guards, preventing
file replacement while SQLite owns transaction locking. Maintenance takes an
exclusive guard on the same canonical, stable sidecar inode. Same-thread
maintenance connections can nest; unrelated threads cannot inherit exclusive
access, and shared-to-exclusive upgrades fail until handles close. No persistent
lock filename is a stale lease to delete.

Material task and agent-state mutations commit with required event evidence.
Task revisions and conditional updates reject stale transitions. Canonical
events use named streams with unique event identity and per-stream sequence.
Consumers use the semantic event interface; legacy JSONL and split stores are
migration inputs rather than live alternate stores.

Event-table initialization reads one Dactyl schema snapshot inside its migration
transaction to locate legacy streams, their columns, and the canonical event
sequence indexes. Import preserves the journal interlock and restricts index
replacement to the event table.

Optional cloud routing is repository-scoped. Decapod derives repository identity
from the supported Git origin and accepts an authenticated opaque datastore
route. The hosted service owns its authentication and tenancy boundary; Dactyl
owns transport and physical execution. A local transaction test does not prove
that a deployed service implements the same contract.

Task proof keeps its subject in the existing verification-artifact JSON: the
stable `task_id` and the exact `task_revision` accepted by the completion
transaction. `PreparedTodoVerification::persist_on_connection` binds the final
revision after `done`; explicit recapture binds the current completed revision.
`todo::effective_verification_status` is the shared eligibility boundary for
work claims, dependency admission, exclusive completion, aggregate verification,
and read-only diagnostics. Historical proof remains visible when its binding is
missing or stale, but cannot authorize current work.

Verification and batch recovery carry the task and baseline they actually
inspected. Under one broker transaction they compare status, revision, proof-plan
bytes, artifact bytes, and the previous verification outcome before recording
new results and canonical events. Batch refresh returns its exact written
snapshots rather than selecting newly appeared tasks into an earlier validation.
Recovery keeps every refreshed row pending while aggregate validation runs with
exactly those task-proof admissions scoped to the recovery. The final transaction
checks each admission against current task, epoch, output, and file evidence,
then promotes the complete batch atomically; an error cannot leave partial passes.
The exemption affects only selected proof admission; completed-task counting
still enforces the required governed plan. A scope guard restores temporary
verification exemptions on every return, including recapture and completion's
nested validator calls.
Prune deletes proof, reopens the task, increments revision, and records its event
atomically. The legacy replay helper lacks complete revision history and removes
revision authority from its projection while retaining the original events.
This uses existing task revisions and SQLite/Dactyl transactions, not another
state store or execution runtime.

## Runtime reconstruction and execution continuity

`core::todo::task_journal` extends the existing canonical TODO event stream with
versioned projection checkpoints. Task scalars, revision, lease generation,
ownership, dependencies, and proof bytes are captured by the task transaction;
legacy event logs are not assumed to contain those complete preimages. An
explicit baseline records existing authority without inventing earlier custody.
Chain validation checks identity, predecessor and projection digests before
restoring a projection. Divergent live authority is not overwritten.
A surviving mutation beyond the last checkpoint blocks reconstruction. Complete
journal-tail removal cannot be detected without independent evidence; this is
not an externally sealed tamper-proof log. Post-baseline legacy imports and
sequence renumbering require an explicit migration instead of silent repair.
The optional cloud task schema has no reconstruction adapter in this contract.

`core::todo::task_journal::transfer` (implemented in
`src/decapod/core/task_transfer.rs`) adds explicit bounded custody transfer for
standalone yielded local tasks. A source transaction records the exact transfer
artifact and increments its revision; the shared task mutation boundary refuses
later mutations on that sealed subject. Artifact publication follows the durable
record and supports explicit byte-identical recovery. Import validates the source
checkpoint chain through the same pure journal verifier, binds GitHub repository,
receiver and source commit, then atomically creates the original task identity and
a local observed baseline. Foreign events remain nested source provenance inside
the canonical TODO stream, preserving their sequence numbers without pretending
they executed locally. No new datastore or lease service is added.

Only task records/history cross this boundary. Current verification artifacts and
pass timestamps are cleared; exact old bytes remain in the provenance record.
Destination revision advances, requiring fresh claim, orientation and proof.
Connected/shared tasks and any preexisting destination task/hash/history fail
closed. Operator acceptance of an independently obtained digest supplies source
trust. A content digest does not authenticate a sender; local source sealing cannot
prevent divergent clones or privileged writers from continuing. Global exclusive
custody would require an additional trusted authority.

`core::workunit` journals complete manifest snapshots in the canonical `workunit`
event stream. The JSON file remains the ignored, recoverable projection. Journal
commit precedes file publication; interruption can leave the projection behind
its durable head, which explicit recovery reconciles without changing proof.
Read-modify-write helpers serialize the whole operation; external prepared writes
require the expected journal generation. An identical operation retry is a no-op;
conflicting identity or predecessor reuse is an error.

`core::trajectory::execution` binds checkpoints to current repository material,
accepted intent and plan authority using existing material and validation-epoch
owners. Recovery and resume are separate operations. Restoring a manifest does
not authorize resumed execution or establish publication readiness. The full
validation and publisher remain the only source of their own readiness verdicts.

Atomic artifact publication syncs file contents before rename. On Unix it also
syncs the parent directory. A directory-sync error is an unconfirmed durability
outcome even if the complete new file is visible; recovery inspects the journal
and file instead of treating that error as proof of rollback. Other platforms
retain atomic visibility without a claimed directory-fsync guarantee.

## Current obligation evidence

`core::obligation::Derivation` walks dependencies iteratively on one canonical
broker-held read connection, with one captured time and memoized node results.
The existing Dactyl/SQLite storage lock keeps cooperating local writers outside
that read. Dependency readiness uses current derived results rather than stored
`obligations.status`; deep chains and diamonds need no recursive call stack or
cache mutation. Direct edge reads retain missing endpoints and cycles as
verification failures. Whole-graph inspection counts every stored edge, including
orphan sources. New obligations validate dependencies before transactional insert.

`plugins::health::derive_health_from_connection` reads claims and proof events
on that same connection. It never reads or updates `health_cache`. Health
inspection uses the same derivation before refreshing that advisory cache.
Exact claim IDs take precedence; subject aliases must be unique. Missing proof
state is unsatisfied, while malformed storage and ambiguous aliases remain
errors. No additional state store, daemon, or completion ledger is introduced.

## Proof and publication

Plans, claims, trajectories, workunits, obligations, context bindings, validation
epochs, and completion evidence describe different parts of the same work.
The proof layer checks their consistency and required evidence instead of
equating one successful command with completion.

Validation is bounded and returns explicit failures and supported recovery.
Full workbranch readiness binds the exact candidate, accepted target, applicable
profile, policy, and execution evidence. CI-local validation has a narrower
scope and cannot authorize publication. Candidate or authority changes can
invalidate earlier proof.

The strict source profile (`decapod.pr-readiness.v1`) retains the same required
check graph across native execution, Buildkite (`validation_buildkite_preflights`),
and independently accepted GitHub Actions execution
(`validation_github_actions_preflights`). In `src/decapod/core/validation_runtime.rs`,
`ProofConfiguration::verify_source_ci_authority` verifies candidate against accepted
target policy before `required_source_catalog_for_provider` selects
`Provider::Buildkite` or `Provider::GithubActions`, and `SnapshotHost` /
`snapshot::source_import` bind the accepted dual-root executor and recollect
measured workflow attempts and authenticated archives from the immutable source
branch. Provider choice changes only the evidence carrier while explicit source,
target, and complete-check requirements remain authoritative.
Provider details belong in [VALIDATION.md](VALIDATION.md).

The controller's finite source adapters are separate from the configured
project's reviewed inventory. `readiness_profile::required_source_catalog`
checks both before execution selection. A matching declaration cannot authorize
different compiled workflow or helper semantics; this case returns a typed
controller-support blocker rather than asking the agent to repin and repeat.
There is no independently pinned remote controller-bootstrap harness today.

Governed publication requires current readiness and uses a normal non-forced
push. Remote divergence requires reconciliation and fresh proof. Branch push,
pull-request creation, merge, packaging, release, and deployment remain
distinct external outcomes. The human supplies intent and reviews and merges
the prepared PR; the agent owns the engineering work and validation phase. The
checked validation push uses narrowly bound authorization rather than a separate
local operator-approval ceremony.

The dependency-free `assets/tools/ci_event_context.rs` distinguishes a real
change range from a verified ordinary branch creation at the captured accepted
tip. Only the latter emits `no_change=true`; workspace/pre-PR and ordinary update
contexts retain distinct immutable endpoints. The constitution workflow rechecks
the same accepted commit and tree before and after running its schema command.
This no-delta execution produces no measured candidate custody receipt.

## Optional Laya readiness assessment

`core/laya_readiness.rs` runs only after required deterministic proof succeeds.
`core/readiness_assessment.rs` owns the bounded production projection and common
binding types; `core/laya_protocol.rs` defines the external worker protocol.
Disabled mode is the default. Required mode needs the reviewed manifest and a
private, verified runtime registration. Only a current affirmative decision can
permit the already-authorized publication step; abstention and rejection block.
No replay fixture or model response establishes build, custody or merge authority.

## Architectural constraints

Decapod remains daemonless, local-first, and agent-operated. Supported mutations
require the applicable session, ownership, policy, and isolation gates.
Machine-owned state changes through the CLI or RPC; authored living-spec prose
is the explicit direct-edit exception.

Recovery preserves uncertainty and original evidence. A timeout, missing
projection, unreachable service, or interrupted client cannot establish that
an operation never happened. Unsupported repair and unresolved ownership stop
the affected transition for review.

## Delivery Plan

Changes to Decapod begin with the affected user outcome and contract. The agent
records scope and proof expectations, implements a bounded change in its claimed
workspace, updates the owning specifications, and checks the actual behavior.
Compatibility and migration consequences are reviewed before exact-candidate
validation and governed publication. Release and deployment require their own
evidence and authority after source readiness.

## Risks and Mitigations

- Misunderstood intent remains possible. Authored specifications and explicit
  unresolved questions make it reviewable before publication
- Coordination races can lose custody. Conditional mutations, leases, canonical
  events, and isolated workspaces reject or expose conflicts
- Crashes and remote timeouts can leave unknown outcomes. Durable operation
  identity and conservative recovery preserve that uncertainty
- Provider drift can invalidate proof. Reviewed profiles, immutable source and
  attempt bindings, and fresh final checks reject mismatched evidence
- Direct commands can bypass Decapod. The agent contract and human review remain
  necessary outside the runtime's enforceable interface boundary

## Foreground storage ownership and caller isolation

The caller-isolation requirement in issue #1476 applies to returned claim results,
persisted task assignees, and the metadata attached to each queued operation.
`group_broker::CallerContext` continues to capture the originating caller and
replace inherited identity/session fields in isolated command children. The
storage-owner transition does not dispatch CLI handlers inside the leader or
swap its process-global environment between callers.

The opt-in `DECAPOD_STORAGE_OWNER=1` path starts a supervised storage child owned by
the elected foreground broker. `storage_owner::Owner` opens the canonical
Dactyl connection once and retains it across successive proxy leases. The
connection is created, used, and dropped on the worker main thread because its
facade contains thread-local `Rc` state. A lifetime pipe ends the worker on
parent loss, even during SQL. Normal shutdown stops new leases and gives an
established lease up to five seconds to finish before killing/reaping a
nonresponsive worker.
There is no detached process or independently managed daemon.
Its lifetime is the current foreground election, not an authentication session.
The broker stops admission after 120 seconds or the configured idle period,
finishes the bounded active request, and drains storage for at most five seconds.
Callers in every worktree discover the canonical datastore's endpoint. An
initialized owner lets its isolated children omit redundant startup migrations;
the optimization is bound to the exact canonical database, and a lost required
owner is an error rather than permission to reopen locally.

All four canonical `db` factories discover the owner before acquiring a physical
sidecar lock. When the storage parent does not exist, discovery leaves secure
first-open directory creation to the normal factory rather than rejecting a new
database. Existing parents and storage files still undergo permission checks.
An isolated child retains its caller context and executes the
normal policy and audit code, while its facade forwards storage operations.
Reads of task backlog/status can use that same physical connection. One logical
stream lease owns transaction affinity until its last facade handle closes.
Same-thread, same-context nested handles reuse that lease; differing context or
access mode is rejected rather than silently inherited. Other callers queue.
Abandoned transactions are rolled back before another caller is admitted.

The shared sidecar guard protects file lifetime even while the owner is idle;
it holds no SQLite write lock. Election uses the separate stable broker.election
inode. SQLite transactions and the active stream lease provide mutation isolation.
Claim completion releases its lease before optional container preparation; a
claiming leader also releases its election before preparation. Maintenance bridges
and migration rollback hold exclusive lifecycle access across backup/restore.
Initial migrations precede owner creation. Unfinished transactions roll back and
temporary schema objects are removed before the next lease. SQL configuration,
attachment, connection-local counters in reads or writes (including stored view
definitions), and unsupported transaction controls are rejected; read-only leases
cannot disable query-only mode. Typed SQL failures survive transport unchanged.

Group requests have one 120-second queue/execution budget, a bounded kernel
admission backlog, bounded frame size, and a one-second request-read deadline.
Execution is serial in accepted order, without FIFO guarantees across concurrent
connectors or elections. A failed acknowledgement is activity, not an idle timeout.
Connection failure before any request bytes may retry admission; after send,
transport loss is UNKNOWN and does not automatically elect, fall back or replay.
Before launch, a durable UNKNOWN receipt binds request ID to arguments and caller.
A final receipt replaces it after completion. Same-ID retrieval returns a saved
result or uncertainty; a crash in the commit/receipt gap never causes replay.
This is conservative at-most-one launch per recorded request, not exactly-once
mutation. Operators must inspect uncertain persisted state before deciding on
new work. Command failure may follow partial commit and is also uncertain.

The owner remains opt-in through DECAPOD_STORAGE_OWNER=1 because its deliberately
restricted SQL facade and cooperative local scope should not silently change all
consumers. Normal CI exercises this path unconditionally in owner unit tests and
the eight-worktree, twenty-mutator and exclusive-claim broker regressions. Socket
probe failure is failure in these regressions. Physical-open instrumentation sits
at the underlying Dactyl open, not logical proxy creation. Both Linux and Darwin
normalize accepted sockets to blocking I/O before applying bounded timeouts.

core/local_socket.rs preserves short historical addresses and maps overlong
canonical paths to a SHA-256-derived directory under sticky /tmp. Private UID or
explicit shared datastore GID scopes that directory. It rejects unsafe existing
permissions, symlink/non-socket endpoints and live peers; cleanup checks endpoint
inode identity. Lock files remain beside canonical storage and are never unlinked.
This avoids both Linux and Darwin sun_path limits without shortening test roots
or masking permission/protocol failures. Empty runtime directories are reusable.

## Intent Traceability Read Model

## Acceptance observer capture boundaries

The read-only acceptance observer in src/decapod/core/flight_recorder/acceptance.rs correlates historical admissions and separately assesses retained capture completeness. Optional coverage binds the declared channels, operator custody, elapsed window, counts, indexes, actual agent end and process exit. Missing or incomplete coverage keeps historical observations while preventing a completed autonomous-run claim.

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`core/intent_reconcile.rs` derives a read-only reconciliation view from
`plan_governance`, `core/project_specs`, the spec manifest, and current
repository code fingerprints. It stores no secondary project authority and
requires no persistent database migration or agent scheduler. The calling
harness remains responsible for interpreting findings and proposing repairs.

## Bounded Immutable Impact Read

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
`core/intent_reconcile.rs` optionally reads an immutable ancestor commit and
the current HEAD through bounded local Git calls. It reuses
`core/project_specs::material_specs_change_between_refs` for authored versus
generated-only living-spec deltas, with the same canonical specification list
and material-body stripping rules. NUL-delimited change paths preserve renames
and whitespace. Oversized, unsupported or unavailable observations fail closed
or explicitly mark the impact incomplete. No new persistence, scheduler, proof
record or cross-document authority is introduced.

<!-- decapod:capability-overlay:persistent-state:start -->

Generated capability obligations: `persistent-state` is declared for this project. These are decisions and proof requirements, not claims of implemented behavior. Resolve each against project source and tests; record unsupported or non-applicable mechanisms explicitly.

## Persistent State Architecture Overlay

### State Ownership
- Each entity type MUST have a designated state owner
- State ownership boundaries MUST be explicitly documented
- Cross-boundary state access MUST go through defined interfaces

### Transaction Boundaries
- Multi-entity mutation atomicity and partial-failure treatment MUST be documented and proven for the selected store
- Transaction boundaries MUST be documented in ARCHITECTURE.md
- Document compensation or other partial-failure treatment if distributed operations exist

### Storage Abstraction
- Storage ownership, consistency behavior, and access boundaries MUST be explicit
- Portability or swappable implementations are project decisions, not universal requirements
- Migration and rollback treatment MUST match the selected storage technology
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `7903b0b6afa925ddf05a3ba6de92d11610e0fb77e9bf39de5003e1916a0e31d4`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `docs/` (3 files), `src/` (281 files), `tests/` (280 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

## Native inventory boundary

`core::storage_inventory` owns semantic measurement, sampling and redaction;
`DactylBridge::inspect_snapshot` delegates physical reads to released Dactyl's
bounded read-only transaction. CLI/RPC inspection dispatches before mutating
startup. Filesystem observations remain separate from SQL snapshot consistency.
Exact checkpoint/other totals and a distinct checkpoint sample avoid prefix
blindness, with shared scan/time/VM budgets. No retention or authority changes.

## Accepted executor and candidate source

The remote validation adapter separates the accepted executor revision R from
candidate source H. `validation_runtime/executor_contract.rs` captures R from
the accepted target’s immutable Git objects before provider dispatch. Its entire
helper, selector, aggregation and recipe closure must match the installed
controller. The candidate’s workflow, local-action and carrier-script bytes stay
independently pinned. Updating a reviewed inventory does not admit a new carrier
or remove a required check.

The existing source-CI carrier stages R separately and compiles its executor.
Fixed R commands build candidate H with H’s configured build inputs and execute
its tests. The integration preparation job packages the measured R helpers with
H’s built test artifacts; downstream workers authenticate that producer chain
rather than compiling a different verifier. Versioned custody binds executor and
subject identities before and after execution. The original controller imports
provider-authenticated results and rechecks the current source, target and full
expected graph before reporting readiness.

This is a reviewed-transitive-code boundary. Separate directories and closure
measurements prevent accidental authority substitution; they are not an OS
sandbox against hostile build scripts sharing the provider account. The trusted
provider importer compiles the pinned workflow, and its authenticated producer
records remain part of the trust model. Validation does not accept a candidate
CLI’s JSON as its verdict or install that candidate as its controller.

The protocol preserves strict single-root operation for the initial rollout
when the accepted base predates dual-root support. That path still requires H’s
helper closure to match the controller. Unknown carrier semantics, unsupported
R, and native-controller mismatches remain explicit blockers. A released binary
cannot acquire new protocol code merely because its repository source changed.

## Repository-declared local proof

Consumer repositories select `repo.pr-readiness.v1` through the canonical proof
configuration. `core/repository_profile.rs` checks complete discovered workflow
mapping, required commands and explicit execution declarations against reviewed
source and command digests, including `RepositoryExecution::platforms`
(`PlatformExecution` entries keyed by `CANONICAL_PLATFORM_KEYS`) and
`RepositoryExecution::resolve_for_platform`. `validation_runtime/repository_host.rs`
translates that reviewed local contract into the existing bounded native runner via
`command_invocation_for_platform` / `select_commands_for_platform`, resolving and
byte-measuring only the current host platform's reviewed executable and
`toolchain_files` and failing closed (`MISSING_PLATFORM_EXECUTION` /
`UNSUPPORTED_PLATFORM`) without fallback if the host platform is absent or
unsupported; it does not interpret workflow YAML or introduce an orchestration
engine. Every command runs in the frozen source snapshot, with byte-measured
declared tool inputs before and after execution and fresh accepted-target queries.
Source/configuration/tool/target drift invalidates proof.

The resulting `repository_local_readiness` scope attests those local commands,
not hosted workflow jobs, platform matrices or release/deployment execution. The
repository authors the semantic coverage and transitive toolchain declaration;
the kernel enforces freshness, execution and custody mechanically. Existing
Decapod source-CI adapters retain their stricter authority and cannot be downgraded
by a candidate switching to local proof. Unsupported remote/daemon operations
remain explicit limitations, rather than fabricated native evidence.

### External engine tooling

The repository-local proof host executes reviewed local commands only. Decapod
has no Dagger-specific engine adapter, authorization map, engine inspection, or
lifecycle integration. Agents may use Dagger through their own skills or project
CI outside this kernel. Such execution does not itself satisfy a Decapod proof
gate or replace independently required Buildkite execution evidence. Existing
native receipt custody and receipt-independent prerequisites remain unchanged.

### Optional CI-attested validation boundary

Validation establishes whether the agent's work has converged on the user's
declared intent and satisfies the conditions for integration. Assurance remains
bounded by the requirements and checks actually established. A later readiness
decision evaluates that result; the agent publishes through the applicable gate,
and a human merges.

`validation_runtime::snapshot::ci_attested` implements an opt-in alternative to
archive custody. The policy must equal the accepted target's policy, with
Buildkite validation enabled on both target and candidate. The existing compiled
check catalog remains authoritative. `snapshot_provider::public_buildkite` binds
complete successful native-job metadata to the exact repository, pipeline,
workflow, branch and commit. `readiness_evidence::buildkite_ci` verifies distinct
CI-attested observations bound to material, intent and check definitions.

The trust boundary is the configured CI executor and provider-owned HTTPS
metadata, rather than independently downloaded measurements of remote outputs.
No artifact, log, toolchain or binary measurement is invented. Projects without
an accepted CI-attested policy retain archive custody. This repository selects
the CI-attested policy in `.decapod/config.toml`; its activation still requires
maintainer acceptance and a controller containing the corrected adapter.

## CPU startup across separate executable pins

Laya installation metadata may include a separately hashed CPU companion.
`laya_runtime` checks canonical native executable identity and private bindings;
backend fallback follows the configured policy without granting readiness on an
unusable response. Local runtime-binding schema version 2 requires explicit
registration. Older registrations fail with re-registration instructions; model
manifests and historical proof bodies are unchanged.

## Supplemental Rust worker verification

`validation_runtime/rust_worker.rs` provides generic, explicitly declared Rust
proof prerequisites. It retains locked dependency execution, bounded processes,
source custody and recorded outcomes. These are deterministic project checks,
not an alternate post-validation model evaluator. This repository declares no
mandatory supplemental worker. Laya remains separately configured and optional.

## Immutable target configuration in source CI

The accepted CI context action continues to bind workbranch pushes to the
regular committed configuration blob, exact checkout and accepted ancestry.
Its carrier bytes remain unchanged because the installed controller independently
pins them. A prepared Rust-only bootstrap migration is retained privately and is
not an active feature or proof of this PR. No Python implementation or runtime is
added. Actual candidate-command regressions remain Rust and invoke the existing
validator/projection interfaces. Neither context diagnostics nor CI-local checks
claim provider freshness or publication readiness.

The managed Dockerfile release refresh in `plugins/container.rs` changes only
the two release-bound ARG values. Its preservation of line endings, trailing
newline state, authored commands and blank lines keeps the projection verifier's
expected bytes consistent with actual command-level release validation. Existing
historical generator verification continues to bind earlier entrypoints to their
own immutable generator source rather than retroactively applying this template.

## Precise claim paths and unchanged trust

The bounded file-scope change addresses the disjoint-work portion of issue 1480.
Trust-grant administration remains unresolved and is not added by this change.
There is no new config trust mapping, elevation API, trust-row mutation or human
CLI requirement. Existing caller names and local approvals remain cooperative
assertions, not authenticated principals or independent proof of human consent.

`fleet_coord::declared_paths` recognizes bounded ASCII literal JSON file-set scopes.
Explicit footprint matching conservatively folds ASCII case on every platform;
Unicode scopes are rejected rather than assuming filesystem alias semantics.
`todo::add_task` rejects traversal/symlink scopes and avoids fuzzy consolidation
of explicitly scoped tasks. Claim/handoff overlap checks, fleet diagnostics and
`work_claim::from_todo_with_lease` use the same footprints. Legacy/invalid scopes
remain conservative. File sets do not bypass independent category ownership,
dependency, lease, shared-claim trust or approval gates and do not establish
verified isolated-worktree custody. Tests cover disjoint/intersecting/parent paths,
malformed declarations, unchanged shared-claim trust and journaled scopes;
source declarations are not a claim that tests have run.

## Task custody and observed checkout admission

The local claim/journal transaction in `core/todo.rs` retains exclusive task
ownership, dependencies, revision and lease generation while treating source
path/category overlap as metadata. Workspace creation selects the destination
branch's task before examining another actor's source branch. The version-one
`authoring_bindings` table lives in the existing canonical datastore, accessed
through `DbBroker` and Dactyl. `core/authoring.rs` observes registered Git and
filesystem identity, checks the live actor/lease, and commits binding plus audit
atomically. The same physical checkout cannot acquire incompatible active task
bindings, including under the same actor. Admission is rechecked at governed
CLI/RPC authoring entrypoints; it is not an OS write sandbox.

Repository-scoped cloud task state stays behind Dactyl and Propodus's trusted
identity/authorization boundary. Host-local checkout identities are never sent
as cloud authorization. The released compatibility task/event profile remains;
resource reservation, filesystem bindings, lease handoff and validated remote
completion are unsupported there. Local labels cannot grant cloud roles or
entitlements. Native RPC and hosted integration need their separate proofs.

## Immutable history recovery assessment

`core/governance_document/checkpoint_history.rs` contains the per-commit checker
shared by strict checkpoint admission and read-only JSON assessment. It binds
resolved base/head identities, classifies affected obligations and retains the
strict material, identity, parent-record and imported-lineage requirements.
`governance_document::record_history_recovery` records an observation in the
fresh workstream's carried obligations, under the existing document lock.
The source commit, tree and governance blob remain immutable; no database,
scheduler or second history authority is introduced. Source reauthoring and
current independent proof are distinct from reconstruction of past evidence.

## Datastore physical revision 2

Decapod retains one local SQLite authority and routes physical execution through
Dactyl. `core/schemas.rs::EVENTS_TABLE_SCHEMA_V2` retains event columns and proof
encodings while avoiding the duplicate nonunique `(stream,seq)` tree. The forward
`events.compact_sequence_index.v002` migration verifies both index definitions
under `BEGIN IMMEDIATE` and removes only the redundant tree. Missing uniqueness
is established additively; duplicate historical sequences reject the upgrade
without normalization. No historical payload, identity or hash is rewritten.
Freed pages are reusable; file shrink requires separately controlled maintenance.
The measured catalog and unresolved cross-backend contracts live in
`docs/datastore-v2/`; native Propodus RPC supports only a partial task model.

## Current evaluator boundary

Laya is the sole optional post-validation evaluator. Shared projection and asset
helpers contain no evaluator compatibility layer. `ValidationReport` exposes one
assessment field, `laya_readiness`; generic historical replay normalization is
unchanged. Retired model code and research corpora are absent from the current
source tree; their immutable history remains in Git.

## Guarded relational index compaction

`core::migration::migrate_compact_redundant_relational_indexes` runs one
`BEGIN IMMEDIATE` transaction over local `decapod.db`. For each of five
nonunique indexes it verifies exact index shape and the full ascending/BINARY
UNIQUE replacement before DROP. Category/risk-zone names already have UNIQUE
constraints; task-owner/dependency keys have matching composite UNIQUE
indexes; preferences use `UNIQUE(category,key)`. Runtime initializers stop
recreating redundant trees. Reverse-key indexes remain. No data or proof
encodings change; cloud PostgreSQL migration ownership stays with Propodus.

## Terminal setup over the managed cloud boundary

`src/decapod/onboarding.rs` owns terminal sizing, presentation, keyboard input and
terminal restoration. When both standard streams are terminals and opening
`/dev/tty` fails, setup reads from a duplicate of the inherited input terminal
and restores its saved mode on exit. Setup uses the terminal driver's timed
read for key-sequence continuations, so an incomplete sequence cannot block
after a PTY readiness notification.
`lib.rs::{enrich_repo_context_interactive,customize_setup,
review_setup}` collects a configuration draft; `prepare_init_directory_and_git`
and `run_init_apply` execute only after interactive approval. Changed configuration
is backed up privately before replacement. Preview and cancellation do not create
project state. Unattended initialization retains its direct execution path.

Cloud initialization then calls the accepted `bootstrap_cloud_session` and
`core::cloud_client::connect` path. Session custody and Dactyl transport retain
their existing owners. The terminal does not implement billing, duplicate account
policy, or grant authority from local settings. Errors leave setup non-successful;
the ready receipt follows successful service-confirmed access only. Real PTY
coverage verifies that reviewing/canceling cloud setup does not contact even the
loopback fixture service before consent. The six Unix PTY cases live in the
private `terminal_onboarding` module of `tests/cli_contracts.rs`, which the
finite source-CI CLI selector schedules. Declaring a standalone integration
target alone does not establish required execution.

## Validation proof lifecycle

Full validation has separate admission and terminal evidence boundaries. `core/validation_rules.rs` checks source/governance inputs before execution; rules requiring proof outputs run in `validation_runtime::run` only after the complete required suite has passed. The runtime passes its owned `CompletedReadinessEvidence`, bound to exact candidate, accepted target, source, configuration, plan, governance and trajectory. No prior prerequisite receipt or string marker can manufacture that completion. Terminal failures precede external readiness receipts and canonical completion history.

Both pre-execution (`core/validate.rs::get_current_task_type`) and terminal publication (`core/validation_rules.rs::evaluate_terminal_rules`) normalize task categories and task ID prefixes through `validation_rules::normalize_rule_task_type` (`feature`, `bugfix`, `refactor`, `docs`, `test`) and populate canonical Git references through `validation_rules::canonical_git_ref` (`refs/heads/<branch>`). Store lookups inspect only an existing `decapod.db` without creating sidecar databases or tripping unmigrated `todo.db` stubs, falling back to branch task-ID prefixes when local store rows are absent. Administrative categories (`code`, `cicd`, `secu`, `task`) map to `None` so built-in `task_types` rules remain applicable rather than exempted, while task-scoped and ref-scoped `EvaluationPhase::Publication` overrides filter against actual branch and task identity.

The terminal boundary shares the original invocation deadline. It checks the remaining budget around evidence reads and before issuing proof; native success files remain private until an atomic bundle publication that rechecks that deadline. Remote ready verdicts likewise stage privately before publication. A deadline expiry cannot turn completed checks into late publication authority.

A fresh worktree may lack the ignored context-capsule policy cache. The policy loader materializes and validates that cache when needed; its absence is not a missing source scaffold and must not trigger a rewrite of the committed specs manifest before proof. Missing tracked scaffold surfaces retain their repair path.

## Explicit managed project routing

`core/cloud_project.rs` owns bounded project-directory selection and the pending creation receipt. `SessionGuard` stores this auxiliary record under the same stable lock and exact checkout/service custody binding, without changing the credential record format or refresh state machine. `cloud_client::acquire` reuses accepted authentication; `connect` resolves the selected project after refresh. The todo factory uses that resolved data reference when opening the consumer connection. No data or authentication transport is implemented outside Dactyl.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## Versioned cloud mutation receipts

The machine-private cloud todo receipt file uses a version-two envelope around its bounded records. Legacy flat arrays migrate under the existing stable file lock; array-only older clients cannot parse and rewrite the new envelope, so they cannot silently remove resolved-project bindings. This changes neither credential custody nor the server data protocol.

## PR 1666: accepted v0.143.2 source integration

`snapshot_provider::github_quota::GithubQuota` is shared by the existing Budget. Validated successful HTTP response hints pace cold GitHub locator discovery; known Buildkite envelopes retain their normal polling interval. The final binding and fresh-registry recheck remain authoritative after removal of a redundant earlier binding read.

## PR1666 current-master integration

The shared GithubQuota/Budget mechanism continues to pace only anonymous discovery, preserve final verification capacity, and honor bounded deadlines. Accepted custody and workunit runtime ownership changes remain in place; this integration adds no competing verifier or provider policy.

## PR1663 integrated storage behavior

The integrated `todo` module exports both accepted authoring/resource custody functions and component-aware journal verification. Keep initialized runtime ownership and task binding retirement alongside storage format guards, compact event reconstruction, content-addressed components and snapshot-consistent verification. The coordination kernel is the published0.144.1 source with only the previously scoped inert-placeholder repair; it does not contain or execute this proposed storage migration.

## PR 1662: accepted v0.143.2 source integration

`core::db_maintenance` centralizes capability admission against canonical accepted project configuration. `storage_lock` retains OS-backed descriptor custody, and Dactyl remains the physical backup/verification boundary. CLI and RPC dispatch this narrow surface before startup migration without adding a second datastore authority.

## PR1662 current-master preparation

The db_maintenance module resolves accepted canonical-repository policy and uses existing Dactyl and StorageLock primitives. It does not replace the datastore or migration authority. Retain current-master storage format guards, task custody, provider verification and runtime ownership while integrating this operation boundary.

## Bounded independent collection (#1573)

`snapshot_provider::github_quota` is advisory scheduling inside the existing
independent reader. It changes neither provider selection nor evidence authority.
The shared budget carries hints into custody/archive reads; actual source suites
and final exact identities remain owned by their existing verifiers. See
`docs/architecture/public-proof-collection.md` for the measured supported run.

## Required maintenance execution (#1568)

The existing `db_maintenance` owner still decides backend, canonical policy,
capability and task/worktree custody before the Dactyl operation. This follow-up
adds no policy engine. `tests/cli_contracts.rs` includes the standalone maintenance
module, and `.config/build/decapod.BUILD.bzl` supplies its source to the existing
required Bazel target. Consequently source-CI selects and executes those actual
boundaries instead of inferring coverage from unrelated green jobs.

## Published bounded SDK (#1667)

`Cargo.lock` owns the registry 0.16.1 SDK and `MODULE.bazel.lock` derives its
crate-universe mappings. Nix reads that same Cargo.lock; no separate package hash
or Nix input update is necessary. SQLite, Neon and default Supabase features
remain selected by the existing application facade. SDK resource budgets are
retained: SQL, parameters, decoded rows, wire bodies and deadlines fail closed.
No new connector policy interpreter, daemon, HTTP/3 or hosted runner is added.

## Diagnostic overrides and full readiness (#1659)

`validation_runtime::require_full_readiness_environment` is the shared admission
for full claims. The runtime invokes it before prerequisites or selection; CLI
and RPC full dispatch, workspace publication and current/historical validated
completion invoke the same owner. The originating local completion process checks
before broker handoff, and for `todo done --validated` prepares verification
evidence outside the exclusive group-broker lock (`todo::prepare_validated_done_handoff`
-> mode-`0600` request-bound handoff under `broker-runtime/` -> `verify::consume_prepared_done_handoff`
re-verified against canonical `validation_history` / local Git objects before the
transactional status update). `group_broker::CallerContext` carries and clears
diagnostic flags per request so worker scope cannot come from another caller, and
brokered child execution honors the caller's bounded `deadline_ms` rather than a
15s control clamp. `bounded_process::configure_child_nofile_limit` elevates soft
`RLIMIT_NOFILE` on Unix across `__project-check-worker`, migration validation,
tooling gates, and `CaptureWithCeiling` subprocesses. Partial project-prerequisite
and CI-local routes do not gain publication authority.

## Preserve accepted local verification policy (#1658)

`validation_runtime::load_project` reads the target's committed proof configuration
and calls `repository_profile::preserve_accepted` before provider selection when
that target uses repository-local proof. The existing contract owner compares
required commands, arguments, execution declarations and workflow coverage.
Additive required checks remain possible; replacement and profile substitution
fail with `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`. Candidate review digests still
establish internal consistency, while the accepted Git target supplies policy
authority. No new policy interpreter, receipt store or service is introduced.

### Accepted source-definition binding

The local-profile branch also requires the candidate source-definition review
digest to equal the accepted target's digest. Discovery still independently
checks the actual bytes. This closes check-script replacement with an unchanged
command. The existing discovered validation inventory is the boundary; it is not
a claim to infer the semantic or transitive completeness of arbitrary commands.
Ordinary subject changes still require fresh source-bound execution. Changes to
inventoried validation definitions require a separately accepted policy revision.

## Observed workunit acceptance (#1660)

`validation_runtime` remains the complete proof producer and
`validation_history::finish_archive` owns canonical verification-journal append.
`workunit::transition_status` consumes that observed record and remeasures current
inputs rather than treating declared pass rows, file hashes or resumable custody
as completion proof. The existing workunit journal retains original generations;
its projection, execution binding and independently checked proof must all be
current at public consumption. Optional bounded, deduplicated input references
reuse the canonical record; there is no second receipt store.
See `docs/architecture/workunit-observed-proof.md` for migration and scope.

## Legacy TODO bootstrap boundary (#1657)

The existing migration owner classifies only a zero-byte regular TODO file with
no WAL/SHM/rollback sidecar as inert. It never opens, imports or removes that
placeholder, including when another subsystem triggers reconciliation. The
classification is captured once for source import/cleanup. Every uncertain TODO
artifact beside canonical task history remains guarded. General nonempty
versioned reconciliation is still unresolved; no second journal authority or
implicit projection rebuild is introduced.

## Explicit physical legacy reconciliation (#1657)

`core/todo.rs` exposes the versioned `journal reconcile-legacy` CLI contract;
`core/legacy_todo_recovery.rs` owns bounded review and physical archival. The
pre-migration dispatcher in `lib.rs` makes recovery reachable while normal
startup remains guarded. V1 accepts only an empty input or a byte-identical
complete canonical datastore copy, after verifying the copied canonical task
journal. It never imports rows, rebuilds task projections, replays mutations or
changes canonical SQLite bytes. Other historical layouts remain unresolved.

One bounded `legacy-todo-recovery-v1.json` record tracks prepared/committed
physical retirement, with a preserved `todo.db.legacy-recovery-v1.archive`.
This operation record is not a second task authority or completion receipt.
The review binds actual content, Unix file identities and modes, canonical
policy fingerprint and actor. Supported storage locks exclude cooperating
writers; SQLite sidecars require quiescence. Prepared recovery fences ordinary
startup even after the legacy filename disappears. Resume remeasures the same
inputs; changed policy or canonical bytes leave preparation unresolved rather
than guessing an outcome. Only one retirement per store is supported in V1.

## Nix release eligibility (#1626)

The existing `.github/workflows/nix-flake.yml` owner and packaging gates inspect
immutable Git parents and subjects. A one-parent squash/linear release uses its
own subject; a two-parent release merge uses parent two's subject. Both retain
the existing release prefix and reject root/octopus or non-release histories.
This is packaging eligibility under the accepted workflow, not a replacement for
kernel validation or an independently authenticated completion judgment.

The credential-free publication owner still requires a native GitHub-hosted
protected-master push. Nix checks the locked toolchain, builds and smoke-checks
both supported native systems, with an additional Linux reproducibility check.
Only the separate authenticated publisher can write Cachix after those jobs
succeed. No new runtime, service, policy interpreter or release authority exists.

The finite adapter in `core/readiness_profile.rs::reviewed_workflow_digests`
explicitly pins the reviewed Nix workflow bytes. The squash-gate change updates
that single compiled pin after reviewing triggers, jobs, matrices, actions and
source predicates; no required job or provider is removed. A declaration digest
cannot override the compiled contract. An older installed controller that does
not support these bytes cannot gain authority merely from candidate repinning;
independently accepted controller support is still required for full native
readiness. Existing accepted-executor tamper tests remain unchanged. The initial
hosted consumer run rejected eleven fixture setups at the old Nix pin, rather
than establishing the later admission assertions; fresh execution must verify
the reconciled candidate.

## Canonical local Git proof (#1069)

The existing `repository_profile::RepositoryContract` review digest optionally binds canonical local Git target authority. `workspace::resolve_readiness_repository` keeps that identity distinct from cloud GitHub identity, requiring a reviewed local contract already committed on the canonical accepted branch. `RepositoryHost::canonical_local` reuses the sealed snapshot verifier and fresh ref observations; no daemon, parallel policy engine or datastore is added. `validation_history` preserves exact local scope and checks target movement. Native strict source proof and provider requirements remain intact.

## Concurrent isolated authoring and combined-candidate admission (#1469)

Concurrent authoring across isolated worktrees separates task custody, isolated editing, and shared integration/release admission without introducing a global source lock:

1. **Advisory source overlap vs. enforced custody/resource fencing:** `core/fleet_coord.rs::detect_overlaps` emits `enforced = false` (`advisory_category_overlap`, `advisory_scope_overlap`, `advisory_path_overlap`) for source categories and repository path declarations, while setting `enforced = true` (`resource_reservation`) only for named shared resources (`resource:release`, `resource:integration`, `resource:integration:<branch>`). `core/todo.rs::claim_task_with_lease` admits distinct tasks across overlapping source footprints with `integration_readiness = "combined_candidate_reconciliation_required"` while rejecting duplicate exclusive claims on an active task lease (`status = "conflict"`, `lease_state = "active"`) and competing shared-resource reservations (`conflict_class = "shared_resource"`, `resolution = "wait_for_resource_custody"`).
2. **Checkout custody without transcript trust:** `core/workspace.rs::claim_branch_scoped_open_tasks` and `core/authoring.rs::{admit_authoring, check_authoring}` bind each active lease to at most one registered worktree (`AUTHORING_CHECKOUT_OCCUPIED`, `AUTHORING_TASK_ALREADY_BOUND`, `AUTHORING_CUSTODY_STALE`, `WORKSPACE_SECONDARY_CUSTODY`). A fresh actor without the first agent's transcript observes live claims via `decapod todo fleet` / `decapod todo list`, cannot hijack A's task or worktree, and either claims a separate permitted task or yields with a typed blocker. `core/authoring.rs::enforce_shared_resource_available` fences `workspace.publish` and `release.lineage_sync` (`SHARED_RESOURCE_OCCUPIED`) without blocking unrelated isolated worktree authoring.
3. **Combined-candidate execution and target freshness:** Locally green isolated worktrees and clean textual Git merges never discharge joint proof. `core/validation_runtime.rs` and `core/validation_history.rs` execute required source checks against the exact combined head and accepted target, withholding `native-readiness-receipt.json` and blocking `todo done --validated` / `workspace publish` on incompatible producer/consumer interfaces or contradictory shared behavior until repaired and revalidated on the exact combined commit. When another branch lands first and advances the accepted target, prior receipts become stale (`STALE_ACCEPTED_BASE`, `WORKUNIT_PROOF_STALE`) until rebased/merged onto the new accepted target and revalidated. `tests/concurrent_worktree_integration_1469.rs` exercises all three scenarios end-to-end through the actual `decapod` binary and isolated Git worktrees.

## Public BYOA, metacognition, and coordination book narrative (#1789)

The public mdBook documentation under `docs/book/src/` (`index.md`,
`introduction.md`, `mental-model.md`, `quickstart.md`, `concepts/agent-first.md`,
`concepts/interoperability.md`, `concepts/constitution.md`, `concepts/mcp.md`,
`workflows/single-agent.md`, `workflows/multi-agent.md`, `SUMMARY.md`) and the
corresponding pointer in `README.md` document Decapod's existing repo-native
governance kernel architecture without adding runtime code or altering
`docs/agent/*` contracts.

Every documented boundary maps directly to existing binary surfaces and tests:
BYOA discovery and negotiation via `decapod rpc --op interop.manifest`,
`interop.negotiate`, and `interop.project` (`src/decapod/core/interop.rs`,
`tests/interop_conformance.rs`); Human -> Agent -> Kernel custody via
`eval`, `docs ingest`, `session acquire`, `session handshake`, `todo`,
`workspace`, `govern`, and `validate` (`tests/doc_alignment.rs`,
`tests/contract_conformance.rs`); lease/overlap coordination via `todo claim`,
`todo renew`, `todo handoff`, `todo yield`, and `todo fleet`
(`tests/unit/core/fleet_coord_tests.rs`); decision-time orientation and
bounded reflection via `infer orientation --reflection` and `mentor.obligations`
(`tests/infer_precision.rs`); subordinate reference usage of the embedded
Constitution (`decapod constitution get/search`, `decapod govern capsule query`);
and repo-native Local (`decapod.db`) vs Cloud (Dactyl-routed task custody,
`tests/cloud_managed_session.rs`) parity. No automatic merge arbitration,
always-on daemon, or unimplemented MCP server capability is claimed.

## Integrity-preserving merged journal reconciliation (#1780)

`src/decapod/core/governance_document/merge_reconcile.rs` implements
`core::governance_document::reconcile_merge`, exposed via
`ArtifactsCommand::ReconcileMerge` in `cli.rs` and `lib.rs`. Instead of parsing
potentially mixed working-tree JSON after a line-based Git merge, it reads
canonical `.decapod/governance.json` blobs directly from immutable `HEAD`,
`MERGE_HEAD`, and `merge-base --all` commits under `with_lock` after checking
`ensure_ungrafted`.

Two ancestry modes are supported:
- **Same PR workstream** (matching `change.id`, `base_commit`, and normalized
  `target_base`): 3-way merges `baseline`, `claims`, `resolutions`, `epochs`,
  `recursive_passes`, `legacy_*`, `plan`, `checkpoints`, `trajectory`, and
  `custody` events/trajectories, re-validates loops, clears changed or
  trajectory-invalidated `validation` receipts, and recomputes canonical
  `proof_status`, `verdicts`, and `artifact_hash`.
- **Accepted target merge** (distinct `change.id` or no `change` where
  `MERGE_HEAD` is an ancestor of the accepted `target_base` ref): retains
  `ours`'s complete active workstream journal and proof posture while 3-way
  merging newly accepted `baseline.policy` and unretired `baseline.unresolved`
  entries and clearing stale `validation` receipts when trees differ.
Any conflicting unaccepted PR identity, mismatched `base_commit` / `target_base`,
deleted active journal, or conflicting check status fails closed without
mutating `.decapod/governance.json`.

## Projection performance budgets, degraded mode, and retention-stable receipts (#1734)

Read-only source, event, and worktree projections remain strictly decoupled from visualization concerns (zero UI dependencies or display-specific state machines in the Decapod kernel) and never disrupt concurrent governed writers:

1. **Covering SQLite indexes and bounded read access:** `core/schemas.rs::EVENTS_TABLE_STREAM_SUBJECT_SEQ_INDEX` (`idx_events_stream_subject_seq` on `events(stream, subject_kind, subject_id, seq) WHERE subject_kind IS NOT NULL AND subject_id IS NOT NULL`) installed by `core/events.rs::ensure_tables` eliminates SQLite temp B-tree sorts and cross-subject stream scans for subject-scoped cursor replay while omitting NULL-subject streams from the B-tree, complementing `idx_events_stream_seq_unique(stream, seq)`. `core/events.rs::actors` executes `SELECT DISTINCT actor FROM events WHERE stream = ?1 ORDER BY actor ASC` directly instead of materializing full stream payloads. `core/db.rs::db_connect_readonly_bounded` opens strictly read-only (`AccessMode::ReadOnly`, `StorageLockMode::Shared`) fail-fast observer connections without long retry loops so read clients never queue behind or starve governed writers.
2. **Measured-baseline budgets and explicit degraded mode:** `core/projection_budget.rs` (re-exported via `core/projection.rs`) enforces `ProjectionBudget` (`max_rows`, `max_response_bytes`, `max_scan_rows`, `max_worktrees`, `max_files_per_worktree`, `max_wall_time_ms`, `reader_lock_timeout_ms`) with hard kernel safety ceilings (`HARD_MAX_*`) and defaults calibrated from real measured runs (`tests/projection_performance_budgets_1734.rs::projection_performance_baseline_benchmark_1734`: 10,000-event store bounded to 179 KiB / 146 KiB peak batch at p50 = 3.93–4.99 ms vs 7.04 MiB / 33–44 ms unbounded; indexed subject query p50 = 743 us vs 1210 us unindexed; 4 concurrent WAL writers shifted p50 58–62 us -> 70–77 us with zero writer errors alongside 4 continuous read clients; 25-worktree dirty sweep hit the 500 ms wall budget at 19 worktrees / 517 ms -> default `max_worktrees = 24`). Exceeding any budget, encountering reader lock contention, or presenting a cursor behind the retention floor transitions the page to `ProjectionStatus::Degraded` with typed `DegradedReason` diagnostics (`ResponseByteBudgetExceeded`, `ScanBudgetExceeded`, `WorktreeBudgetExceeded`, `FileEnumerationBudgetExceeded`, `WallTimeBudgetExceeded`, `CursorBehindRetentionFloor`, `ReaderContentionDeferred`) without silent truncation or false completeness.
3. **Retention-stable logical receipts and lock-free worktree enumeration:** `core/projection_budget.rs::{project_events_read_only, project_events_on_conn}` hashes recursively key-sorted canonical reconstructed logical events (`canonical_json_bytes`), making `content_sha256` and `window_receipt_sha256` bit-for-bit invariant across `core/storage_compaction.rs::{apply_v2_compaction_migration, gc_unreferenced_components, reclaim_pages_explicit}` and across `apply_bounded_event_retention` for retained windows. Explicit cold-window retention writes a chained cryptographic `EventRetentionAnchor` under `meta(namespace='event_retention', key=<stream>)` without renumbering surviving sequences, and `core/events.rs::append_on_conn` consults that floor only when a stream is empty so post-retention appends stay strictly monotonic with zero hot-path query overhead. `project_worktrees_read_only` runs all Git commands with `GIT_OPTIONAL_LOCKS=0` and bounded timeouts so multi-worktree observation never acquires index locks against concurrent agents.

## Compact storage v2 floor reconciliation (#1739)

`core::storage_compaction::{v2_format_metadata_needs_floor_upgrade,reconcile_v2_storage_format_metadata,stamp_storage_format_metadata}` compute `max_semver_floor` so `storage.min_reader_version` and `storage.min_writer_version` never fall below `0.144.2` on v2 stores (`storage.format_version = 2`), while preserving higher valid floors and leaving unmigrated v1 stores untouched until migration `1200` runs. `core::migration::schema_upgrade_pending` checks `v2_format_metadata_needs_floor_upgrade` so `check_and_migrate_with_backup_report` holds the exclusive storage lock and backup across legacy `0.141.0 -> 0.144.2` restamping, and `core::events::ensure_tables` reconciles on write-side initialization. Backwards-compatible reads and writes for `>= 0.144.2` binaries (`STORAGE_FORMAT_VERSION = 2`, `TODO_SCHEMA_VERSION = 18`) remain intact with no incompatible DDL or trigger additions.

## Broker writer incarnation provenance (#1783)

`DbBroker::build_event` (`src/decapod/core/broker.rs`) captures `WriterIncarnation::current()` on `pending` write audit events without adding external crate dependencies. On macOS, `RealIncarnationProbe` reads `libc::proc_pidinfo` (`PROC_PIDTBSDINFO`) process start timestamps (`pbi_start_tvsec`/`pbi_start_tvusec`), `kern.bootsessionuuid` via `libc::sysctlbyname`, and `libc::gethostuuid`. On Linux, it reads `/proc/<pid>/stat` field 22 (`starttime`), `/proc/sys/kernel/random/boot_id`, `/etc/machine-id` (or `/var/lib/dbus/machine-id`), and `/proc/self/ns/pid`, falling back safely to `None` / `ProcessProbe::Unverifiable` when unavailable.

`assess_writer_liveness` establishes `WriterLiveness::Stopped` only when provenance is complete and either (a) the current boot incarnation (plus host ID and PID namespace when applicable) matches and the PID is absent/zombie (`ESRCH` / `SZOMB` / `Z`) or running with a different `writer_start_id` (proving PID reuse), or (b) `writer_host_id` matches the current host and `writer_boot_id` proves a different prior boot incarnation. Neither event age alone nor a bare PID check without start-identity and boot/host correlation ever proves writer termination.

## Claim and workspace actor gating (#1737)

`core::authoring` owns `is_missing_or_unknown_actor`,
`authoring_actor_required_error`, and `has_real_actor_checkout_custody`, shared
across `core::todo` and `core::workspace::ensure_workspace` so claim admission,
branch-scoped workspace creation, and checkout binding enforce one uniform actor
contract. `core::todo::claim_task_with_lease` routes eligible legacy
`"unknown"` claims through `force_release_assignment` (`unknown_actor_adopted`)
inside the existing `with_task_journal` transaction, preserving monotonic lease
generations and journal checkpoints.

Unit coverage in `tests/unit/core/todo_tests.rs` and black-box CLI coverage in
`tests/todo_enforcement.rs` exercise `init --proof --no-ci --isolation-mode worktree`,
read-only command preservation without `DECAPOD_AGENT_ID`, `"unknown"` / empty
rejection across `todo claim` and `workspace ensure`, legacy `"unknown"`
recovery with and without `authoring_bindings`, and cross-owner isolation
between real actors.

## Read-only acceptance correlation (#1757)
`core::flight_recorder::acceptance` extends the current flight recorder rather
than adding a governor or agent runtime. Bounded sibling-file reads verify
artifact digests and event ordering. Source-specific reducers correlate actual
Pi calls, native plan/observations/receipt material, task/workspace custody and
separate Git/GitHub subject snapshots. The existing readiness-evidence verifier
checks captured suite consistency. Trusted host provenance remains explicit;
source labels and hashes do not authenticate a producer.
Essential kernel calls join actual in-flight Pi bash calls, and actual tool IDs
must match provider tool requests. Repair requires a new model request and tool
start after the failed tool's feedback returned; controller work and prewritten
same-call repair cannot establish autonomous correction. New validation clears
prior publisher applicability; later source/base movement preserves history
while requiring new applicable publication evidence. Output grants no authority.

## Source-grounded current enforcement map (#1069)

`docs/architecture/continuous-governance-implementation-map.md` now distinguishes
the accepted0.145.6 kernel from pending recovery and separately accepted Nix
source, and binds hosted
observations to their actual head, accepted base and declared scope. Canonical
validation, observed workunit proof, request-local diagnostic denial, task custody
and historical completion remain existing authority owners. The map introduces
no new engine, store, daemon or authority interpreter.

The complete native runtime integration, ten-worker A-H demonstration,
independently authenticated exception/cloud authority and compatible split-store
workunit convergence remain missing. Native GitHub source diagnostics cannot
replace configured provider proof or authorize a different candidate. Existing
SQLite semantics remain the foundation; a facade or registry upgrade does not
satisfy an older no-SQLite-engine proposal by assertion.

## Accepted v0.146.0 audit reconciliation

This map now uses accepted master `e9c40e929ee33aa5443c38b57276f96111db0b92` and the installed 0.146.0 kernel. The accepted #1699 bounded escalation CLI/RPC reference is implemented and covered by its exact-source execution; it is not complete external runtime adoption, semantic reasoning proof or the ten-worker A–H demonstration. Prior baseline paragraphs remain historical audit context. Pending #1697 physical recovery and #1702 canonical local-target admission are kept separate from accepted enforcement until individually verified and merged. Source-CI diagnostics do not substitute for the unchanged full native provider contract.

## Source-CI archive delivery

The existing preparation carrier may restore only Bazel's content-addressed
SHA-256 archive subtree. Accepted-master pushes populate provider-scoped entries;
other events restore only. Extracted repositories, build outputs and evidence
remain outside that cache. Bazel verifies bytes on reuse; a cache hit does not
authorize a test result. Changed carrier acceptance requires an independently
released controller, with complete baseline/provider proof still outstanding.
This infrastructure work preserves PR #1575 and does not reconcile Laya policy
inside a candidate-controlled validation definition.

## Target-scoped deployment closure (#1740)

`repository_profile::resolve_target_closure` resolves a selected target's transitive `depends_on` closure plus any additional targets whose `shared_inputs` prefixes/globs (`matches_shared_input`) intersect `<accepted_base>..HEAD` changed paths (`ApplicabilityFacts.changed_paths`), failing closed on unknown targets (`UNKNOWN_DEPLOYMENT_TARGET`), dependency cycles (`DEPLOYMENT_TARGET_CYCLE`), invalid patterns (`REPOSITORY_CONTRACT_INVALID`), or missing diff context (`MISSING_SHARED_INPUT_DIFF_CONTEXT`). `repository_profile::required_catalog_for_target` builds `Obligation`s solely for commands in the resolved closure while recording excluded commands as `StageDispositionKind::NonApplicable` (`out_of_target_scope_no_assurance`) and binding the resolved scope into `definition_digest` and `RequiredCatalog.source_inventory_digest`. `RepositoryHost::select_commands` verifies platform compatibility (`INCOMPATIBLE_TARGET_EXECUTOR`) and installed executables only for targets in `resolved_targets`, allowing a Linux-only slice to run without unrelated macOS executables while full validation (`options.target == None`) remains unchanged.

## Workspace diagnostic composition (#1071)

`src/decapod/core/workspace_diagnostic.rs` composes the canonical repository
policy resolver, checkout and task custody readers, governance artifact
inventory, historical validation reader, and optional GitHub readback.
It projects local operation eligibility and proof applicability from those
owners without introducing another admission authority.

## Selected work and actual behavior

The selected-work model in `core/intent_reconcile/selected.rs` reads explicit accepted Git roots and reconciles them to existing task obligations. Root mapping, the bounded equality and inequality consistency check, and actual required application checks give separate results. Independent checks cover missing audit behavior and incompatibility in a combined source tree. Unrelated backlog remains outside the selected work.

Use the canonical task-scope parser upstream. Legacy scope repair is limited to the known projection and preserves the prior journal before reauthorization.

## Release projections and source template authority

`core::release_projection_drift::verify` reuses `entrypoint_integrity::preserves_source_entrypoint_versions` for Decapod source development rather than implementing another version exception or template reader. In that mode the expected entrypoint is its immutable release-head blob, and the working entrypoint must equal it byte for byte. The existing predicate independently validates canonical committed source templates, repository origin, ungrafted ancestry, policy opt-in and version-bound fingerprints. Other repositories retain the binary-release rendering path. Immutable release classification, staged/working change inventory, exact Dockerfile/spec/manifest hashes and all other-field comparisons remain required.

The standalone `assets/tools/release_projection_drift.rs` wrapper links the ordinary Decapod library while compiling the shared verifier. Its source-version predicate must therefore be available across that crate boundary; it remains a read-only helper, not a grant. The shared projection fixture accepts an existing source root, while library-only lineage tests supply the existing published-root fixture from `entrypoint_integrity/history/tests.rs`.

The full Supabase-enabled library execution caught a test-fixture identity omission: the release-version helper must retain the canonical Cargo package repository field when it reuses the existing published-root source fixture. The correction changes fixture construction only; origin, ungrafted ancestry, committed template and immutable entrypoint checks remain strict. Prior failed candidate eee794ba and actual job136182283538 remain preserved.

Release projection integrity and retained observer-run completeness are separate proof surfaces. A successful version-only comparison cannot turn missing run boundaries or incomplete tool observation into an autonomous-admission claim; the existing acceptance observer retains that judgment. Preserving source-entrypoint versions grants no observation coverage.

## Independent raw-input decoding boundary

`tests/fixtures/interop_strict_json.py` remains an independent, same-host Python decoder with no Decapod import or storage access. It now checks Unicode scalar strings/keys, finite integer overflow and the native 127-container depth limit after unique-key parsing. `tests/fixtures/interop_json_vectors.v1.json` preserves raw parameter strings; the existing unfiltered `tests/interop_conformance.rs` target compares actual native rejection reasons and legitimate projection execution. The existing Bazel target declares both compile-time data inputs explicitly. No production parser, policy engine, proof producer, workflow or required test recipe is replaced.

The encoding profile and independently retained agent-run coverage remain separate. Matching decoder reasons proves only the declared raw-input interpretation scope; it cannot substitute for required run boundaries, tool observations or the actual agent closing event. Missing observer coverage remains unknown even after every encoding vector passes.

The independent JSON encoding vectors remain a bounded parser compatibility check alongside accepted recorded-instruction readiness in `src/decapod/plan_governance.rs`. Agreement on Unicode, finite numbers, duplicate keys and nesting cannot converge unresolved instructions, satisfy spec review, or confer current publication authority.

The accepted v0.154.0 package epoch (`0f44f780`) is integrated before encoding validation. The conformance harness obtains executable identity through the documented read-only `decapod system version` command; unsupported top-level version flags are not execution evidence. Native regeneration supplies the current release projections, while the independent decoder and all pinned raw inputs remain separate from authorization and full native admission.

## Release-only inventory transitions, CI ergonomics, headless Podman, and badge (#1838, #1417, #1023)

- **Release-only `repo.pr-readiness.v1` qualification (`src/decapod/core/repository_profile.rs`)**: `verify_accepted_transition` invokes `qualifies_release_only_inventory_transition` when `commands_preserved == true` and `candidate.reviewed_commands_digest == accepted.reviewed_commands_digest`. It computes the exact Git-tree `SourceInventory` digest at `accepted_base_sha` using the shared discovery roots and bounds from `src/decapod/core/init_reconciliation/mod.rs` (including the `rust-toolchain.toml` symlink alias rule and `MAX_ENTRIES`/`MAX_DEPTH`/`MAX_FILE_BYTES`/`MAX_TOTAL_BYTES`), verifies zero reviewed-source changes on the candidate relative to `accepted_base_sha`, walks up to 16 first-parent ancestors (`git rev-list --first-parent --max-count 16`), and qualifies only when the sole reviewed-source change between the matching ancestor and `accepted_base_sha` is a strict SemVer `X.Y.Z` bump of root `[package].version` in `Cargo.toml` and the matching root `[[package]]` entry in `Cargo.lock` (when present).
- **Pending target-movement preflight check (`src/decapod/core/snapshot_provider/automatic.rs`)**: `wait_for_source_runs` periodically calls `verify_pending_accepted_target` (`ACCEPTED_TARGET_RECHECK_INTERVAL = 60s`) to fail fast with `STALE_ACCEPTED_BASE` when the remote accepted branch advances mid-run.
- **Public-boundary privacy preflight (`src/decapod/core/validation_runtime/publication_privacy.rs`)**: Wired into `validation_runtime::run` before adapter selection, scanning authored commit messages and diffs (`<accepted_base>..HEAD` and `<accepted_base>...HEAD`) against optional local private deny rules (`.decapod/data/publication-deny.txt`) and repo-configured `[governance].publication_deny_identifiers`.
- **Headless macOS Podman VM startup (`src/decapod/plugins/container.rs`)**: `verify_headless_podman_machine_args` and `start_podman_machine_quietly` enforce that automatic/diagnostic startup paths never pass `--log-level=debug`/`trace` or `--gui` and scrub `PODMAN_LOG_LEVEL`/`CONTAINERS_LOG_LEVEL`.
- **Decapod README badge (`src/decapod/cli.rs`, `src/decapod/lib.rs`, `assets/badge/decapod-governed.svg`, `README.md`)**: `InitConfigSection.readme_badge` defaults to `true` (`decapod init` idempotently syncs the versioned badge into unprotected `README.md` files unless opted out via `readme_badge = false`), and `assets/badge/decapod-governed.svg` provides a static accessible SVG badge linked from `README.md`.

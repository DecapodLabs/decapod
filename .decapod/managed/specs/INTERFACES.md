# Interfaces

## Wave 7 datastore compaction, `[agent_maintenance]` config, knowledge attestation, and TypeScript/Pi contracts (#1630, #1568, #657, #1277, #1711, #1118)

- **Lossless compaction CLI & RPC (`src/decapod/cli.rs::DataCommand::Compact`, `src/decapod/core/rpc.rs::StorageCompactParams`, `src/decapod/core/interop.rs`, `src/decapod/lib.rs`)**:
  - CLI: `decapod data compact [--dry-run] [--reclaim-pages] [--format json|text]` outputs `DataCompactionReport` (`decapod.data-compaction-report.v1`) with exact pre/post byte & page counts, `events_compacted`, `prompts_deduplicated`, `components_collected`, `reclaim_executed`, `lossless_verified`, and `hosted_backend_status: "unavailable_local_only"`.
  - RPC: registers `data.compact` (`read_only = false`, `session_required = true`, aligned with `INV-SESSION-AUTH` entry enforcement) accepting `{ dry_run?: bool, reclaim_pages?: bool }`.
- **Maintenance policy & Cloud delegated grant contract (`src/decapod/cli.rs`, `src/decapod/core/db_maintenance.rs`)**:
  - `.decapod/config.toml` accepts `[agent_maintenance]` as an exclusive shorthand alongside `[governance.db_maintenance]` and `[maintenance]`; configuring more than one fails closed with `AMBIGUOUS_MAINTENANCE_POLICY`.
  - Adds `DELEGATED_GRANTS_CONTROL_OP = "propodus_delegated_grants_v1"`, `CloudDelegatedGrantStatus`, `fetch_cloud_delegated_grant_status`, `parse_cloud_delegated_grant_status`, and `evaluate_cloud_capability_with_delegated_grant`.
- **Knowledge provenance & feedback projection (`src/decapod/plugins/knowledge.rs`)**:
  - Adds `add_attested_knowledge(store, repo_root, args, scope)` and extends `KnowledgeContextRecord` with optional `supersedes_id` and `feedback_summary` (`KnowledgeFeedbackSummary`), emitting `freshness = "source_attested_current" | "source_changed_since_write" | "source_unavailable" | "not_captured_at_write"`.
- **TypeScript client & Pi extension packages (`bindings/typescript/**`, `packages/pi-decapod/**`, `tests/fixtures/darkmatter_readonly_snapshot.v1.json`)**:
  - `@decapodlabs/decapod-client@0.163.0` exports `DecapodTransport`, `DecapodClient`, `probeTransport`, `classifyDecapodError`, `createPromptProvenance`, `validateDarkmatterSnapshot`, and `DARKMATTER_SNAPSHOT_SCHEMA_VERSION`.
  - `@decapodlabs/pi-decapod@0.163.0` exports `createPiDecapodExtension` with `onPromptReceived`, `onContextResolve`, `onToolResult`, `beforeMutation`, `onSettlement`, `onResume`, `beforeValidationAndAdmission`, `switchModelOrHarness`, and `classifyDirectHostWrite`.
  - `tests/fixtures/darkmatter_readonly_snapshot.v1.json` provides the canonical offline read-only consumer snapshot verified by both Rust (`tests/houseboat_engine.rs`) and TypeScript (`bindings/typescript/test/client.test.ts`).

## Podman container-custody & Propodus Cloud Admin wire contract updates (#1379, #1859, #1886, #1896)

- **Propodus Edge JSON request contracts (`src/decapod/core/cloud_admin.rs`)**:
  - `/org-admins` (`AccountOperation::OrgAdmins`): sends `"operation": "list"|"grant"|"revoke"`.
  - `/org-recovery` (`AccountOperation::OrgRecovery`): sends `"operation": "status"|"initiate"|"confirm"|"cancel"` and `"recovery_request_id"`, reading either `recovery_request_id` or `recovery_id` from server responses.
  - `/policy-templates` (`AccountOperation::PolicyTemplates`): sends `"operation": "audit_list"|"export"|"import"|"delete"`.
  - `/org-upgrades` (`AccountOperation::OrgUpgrades`): sends `"operation": "status"|"schedule"|"approve"|"cancel"`.
- **Error taxonomy & container status contracts (`src/decapod/core/error.rs`, `src/decapod/core/cloud_client.rs`, `src/decapod/core/workspace.rs`, `src/decapod/plugins/container.rs`)**:
  - Adds `CloudAuthErrorCode::InvalidRequest` (`"invalid_request"`) mapped to `CloudAuthStatus::ActionRequired`, preserving session custody on HTTP 400 parameter errors.
  - Preserves structured `billing_not_configured` on non-mutating `decapod cloud orgs` billing probes rather than flattening into `OutcomeUnknown`.
  - `decapod workspace status` recognizes `/run/.containerenv` and `DECAPOD_CONTAINER=1` for Podman `in_container: true`, and `decapod auto container run` exports `DECAPOD_WORKSPACE_IMAGE` and `DECAPOD_CONTAINER=1` while splicing `DECAPOD_CONTAINER_SIGNER_SOCKET` flags before the image positional argument.

## Execution telemetry CLI/RPC/HTTP and MCP `--allow-mutations` interfaces (#1095, #872, #871)

- **Read-only telemetry CLI (`src/decapod/cli.rs::ObserveCommand::Telemetry`, `src/decapod/lib.rs::run_observe_command`)**:
  - `decapod observe telemetry [--dir <PATH>] [--after-seq <N>] [--limit <N>] [--bucket-seconds <N>] [--task-id <ID>] [--repo-id <ID>] [--expected-head-sha <SHA>] [--format json|text]`
  - Emits `ExecutionTelemetryProjection` (`schema_version = "decapod.observe.v1"`, `telemetry_schema_version = "decapod.execution-telemetry.v1"`, `read_only = true`, `authoritative = false`) or human-readable text via `render_telemetry_human`.
- **Read-only RPC & loopback HTTP (`src/decapod/core/interop.rs::RPC_OPS`, `src/decapod/core/rpc.rs`, `src/decapod/core/observability_projections/server.rs`, `src/decapod/lib.rs`)**:
  - Registers `observe.telemetry` (`read_only = true`, `session_required = false`) across `RPC_OPS`, `builtin_manifest()`, `rpc::Capability`, `ALLOWLISTED_OBSERVE_RPC_OPS`, `lib.rs` dispatch/bypass arms, and `GET /observe/v1/telemetry` on `decapod observe serve`, preserving zero two-way dispatcher/authorization drift (`check_dispatcher_authorization_drift`).
- **MCP stdio mutation gate opt-in (`src/decapod/cli.rs::McpCommand::Serve`)**:
  - Adds `--allow-mutations` (default `false`) to `decapod mcp serve`. Without `--allow-mutations`, `decapod_governed_mutate` returns `read_only_session_denied`; with `--allow-mutations`, missing/invalid session credentials return `session_required`, protected/root checkouts return `worktree_isolation_required`, and authenticated isolated `.decapod/workspaces/*` worktrees return `admitted_for_kernel_dispatch`.

## DPoP device gateway & `dactyl-db 0.19.0` Cloud Admin CLI/RPC contracts (#1878)

- **Expanded `decapod cloud` CLI subcommands (`src/decapod/cli.rs`, `src/decapod/lib.rs`)**:
  - `decapod cloud project enroll [--project-id <id>] [--organization-id <id>] [--plan-tier <tier>]` (`decapod.cloud.enroll.v1`, `AccountOperation::RepositoryEnroll`).
  - `decapod cloud billing limits [--project-id <id>]` routes through `AccountOperation::ResourceLimits` first (falling back on 404 to legacy `AccountOperation::Status`).
  - `decapod cloud admins {list|grant|revoke}` (`decapod.cloud.admins.v1`, `AccountOperation::OrgAdmins`, explicitly reiterating `#215` admin-vs-paid-seat orthogonality).
  - `decapod cloud recovery {status|initiate|approve|cancel}` (`decapod.cloud.recovery.v1`, `AccountOperation::OrgRecovery`).
  - `decapod cloud policies {list|get|apply|delete}` (`decapod.cloud.policies.v1`, `AccountOperation::PolicyTemplates`).
  - `decapod cloud upgrades {status|request|apply|cancel}` (`decapod.cloud.upgrades.v1`, `AccountOperation::OrgUpgrades`).
  - `decapod cloud status` adds non-secret boolean `first_party_origin_staged` without exposing physical origin URIs; explicit opt-in `DECAPOD_CLOUD_FIRST_PARTY_ORIGIN=1` selects `FIRST_PARTY_CLOUD_ORIGIN` (`https://api.decapodlabs.ai`).
- **Expanded JSON RPC & capabilities contracts (`src/decapod/core/rpc.rs`, `src/decapod/core/interop.rs::RPC_OPS`, `src/decapod/core/docs_cli.rs`, `docs/agent/command-contracts.md`)**:
  Registers `cloud.{enroll,admins,recovery,policies,upgrades}` with `session_required = false`, typed `Cloud{Enroll,Admins,Recovery,Policies,Upgrades}Params`, matching `Capability` and `cloud` subsystem `ops` (`project.enroll`, `enroll`, `admins`, `recovery`, `policies`, `upgrades`), and aligned early-dispatch guards without touching local SQLite.
- **Typed `CloudAuthErrorCode` expansion (`src/decapod/core/error.rs`, `src/decapod/core/cloud_client.rs`)**:
  Adds typed `snake_case` error codes for DPoP (`use_dpop_nonce`, `invalid_dpop_proof`, `dpop_aud_mismatch`, `dpop_replay`, `dpop_nonce_mismatch`, `dpop_clock_skew`, `dpop_ath_mismatch`, `dpop_htu_mismatch`, `dpop_htm_mismatch`, `dpop_jkt_mismatch`, `invalid_binding`, `dpop_proof_required`, `missing_dpop_proof`, `stolen_bearer`, `device_revoked`, `device_key_expired`, `device_not_enrolled`, `refresh_reuse_detected`, `expired_credential`), org session/grant narrowing (`grant_revoked`, `grant_expired`, `scope_denied`, `repo_access_denied`), paid seats (`paid_seat_required`, `seat_capacity_exceeded`, `organization_already_enrolled`, `admin_approval_required`), tenant lifecycle (`tenant_upgrade_required`, `tenant_upgrading`, `tenant_suspended`, `tenant_archived`, `org_recovery_required`, `policy_template_conflict`), and provenance/concurrency (`provenance_origin_mismatch`, `idempotency_conflict`, `idempotency_in_progress`, `version_conflict`, `transaction_aborted`).

## Agent interoperability CLI, RPC, schema, and independent conformance surfaces (#870–#878, #1715–#1718, #1724, #1725, #1733, #1735)

- **Bounded transport CLI subcommands (`src/decapod/cli.rs::{McpCli, A2aCli}`, `src/decapod/lib.rs::{run_mcp_command, run_a2a_command}`)**:
  - `decapod mcp serve [--one-shot] [--max-requests <N>] [--max-frame-bytes <N>] [--framing auto|jsonl|content-length]`: finite, daemonless local stdio JSON-RPC 2.0 server handling `initialize`, `notifications/initialized`, `notifications/cancelled`, `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, and `shutdown`.
  - `decapod mcp schema [--format json|text]`: exports `McpSurfaceSchema` (tools, resources, supported protocol versions, authority boundary).
  - `decapod mcp config [--root <PATH>] [--executable <PATH>]`: emits scoped `mcpServers.decapod` JSON configuration.
  - `decapod a2a card [--format json|text]`: prints `A2aAgentCard` advertising only executable local skills and separate `conformance_layers`.
  - `decapod a2a handoff (--input <FILE> | --json <JSON>)`: validates `{ "envelope", "local_context" }` via unique-key JSON parsing, exiting non-zero on stale proof, contradictory ownership, tampered digests, or `claims_local_authority = true`.
- **Read-only JSON RPC ops (`src/decapod/core/interop.rs::RPC_OPS`, `src/decapod/core/rpc.rs`, `src/decapod/lib.rs`)**:
  - Registers 7 read-only, session-free, storage-independent RPC operations: `interop.envelope.verify`, `interop.drift.check`, `interop.registry.query`, `interop.skills.select`, `interop.rules.evaluate`, `interop.rules.taxonomy`, and `interop.overlap.classify`.
  - Updates `transport:mcp` and `transport:a2a` in `interop::builtin_manifest()` to `implementation_status = "implemented"` with truthful `evidence.state = "not_run"`, while `transport:acp` remains `intentionally_not_used`.
- **Normative schema, fixtures, docs & conformance suite**:
  - `assets/schemas/agent-interop-envelope-v1.schema.json`: Draft 2020-12 JSON Schema for `decapod.governance-envelope:v1`.
  - `tests/fixtures/agent_interop_vectors.v1.json` + `tests/fixtures/agent_interop_reference_consumer.py` + `tests/agent_interop_conformance.rs`: 26 pinned adversarial and positive-twin vectors verified across Decapod, an independent raw-JSON Rust verifier, and an independent Python stdlib consumer.
  - `docs/conformance/governance-envelope-v1.md` and `docs/architecture/agent-interop-adr.md`: normative profile specification and `#1735`-aligned `proven / partial / target` ADR cross-reference.

## Read-only observability CLI, RPC, and loopback HTTP interfaces (#860, #874, #1712-#1714, #1719-#1720, #1723, #1727-#1728, #1730-#1732)

- **Read-only CLI (`src/decapod/cli.rs::ObserveCli`, `src/decapod/lib.rs::run_observe_command`)**:
  - `decapod observe specs [--dir <PATH>] [--offset <N>] [--limit <N>] [--include-excerpts] [--anchor-path <REL>] [--start-line <N>] [--end-line <N>] [--format json|text]`
  - `decapod observe timeline [--dir <PATH>] [--after-seq <N>] [--limit <N>] [--stream <NAME>] [--format json|text]`
  - `decapod observe workspace [--dir <PATH>] [--offset <N>] [--limit <N>] [--anchor-path <REL>] [--start-line <N>] [--end-line <N>] [--format json|text]`
  - `decapod observe validation-plan [--dir <PATH>] [--require <R_ID>...] [--format json|text]`
  - `decapod observe pr-manifest [--dir <PATH>] [--planned-file <REL>...] [--format json|text]`
  - `decapod observe context-provenance [--dir <PATH>] [--offset <N>] [--limit <N>] [--task-id <ID>] [--repo-id <ID>] [--format json|text]`
  - `decapod observe serve [--dir <PATH>] [--bind 127.0.0.1:0] [--token-stdin | --auth-token <TOK>] [--allow-remote] [--max-requests <N>] [--max-body-bytes <N>] [--tenant-id <ID>] [--repo-id <ID>...] [--revoked-token-id <ID>...]`
  - All `decapod observe` commands are strictly read-only, sessionless, offline-first, and emit `schema_version = "decapod.observe.v1"`, `read_only = true`, `authoritative = false`.
- **Read-only JSON RPC ops (`src/decapod/core/rpc.rs`, `src/decapod/core/interop.rs`, `src/decapod/lib.rs::rpc_handlers::handle_observe`)**:
  - Registers `observe.specs`, `observe.timeline`, `observe.workspace`, `observe.validation_plan`, `observe.pr_manifest`, and `observe.context_provenance` (`read_only = true`, `session_required = false`, worktree/mandate bypass aligned in `lib.rs`).
- **Loopback HTTP endpoints (`src/decapod/core/http_transport.rs`, `src/decapod/core/observability_projections/server.rs`)**:
  - `decapod observe serve`: accepts `GET /observe/v1/{specs,timeline,workspace,validation-plan,pr-manifest,context-provenance}` and `POST /rpc/v1` restricted exclusively to `ALLOWLISTED_OBSERVE_RPC_OPS`, setting `X-Decapod-Read-Only: true`, `X-Decapod-Snapshot-Id`, `Cache-Control: no-store`, and `X-Content-Type-Options: nosniff`. Mutating RPC operations return `403 read_only_enforced`; non-`GET`/`POST` HTTP methods return `405 method_not_allowed`.
  - `decapod rpc --http-server`: retains `POST /rpc/v1` while enforcing request-bound `Idempotency-Key` (`409 idempotency_conflict`), `X-Decapod-Request-Id` (`409 replayed_request`), bounded entry/byte cache (`503 idempotency_capacity_exceeded`), uncertain-outcome non-replay (`409 unknown_outcome_reconcile_required`), and bounded child timeouts/output caps (`504`/`502`).

## Mechanical clarity CLI, accepted-policy `main_merge`, historical exceptions, and ledger APIs (#1083, #1084, #1093, #1097, #1365, #1468, #1569, #1691, #1726, #1729, #1761, #1762, #1768, #1907)

- **Clarity validation CLI (`src/decapod/cli.rs::ValidateCli`, `src/decapod/lib.rs::run_validate_command`, `src/decapod/core/artifact_clarity.rs`)**:
  - `decapod validate --clarity [--profile ste-lite|ste-strict|custom] [--format text|json]` evaluates durable governance artifacts (`.decapod/governance.json` excluding human prompt fields and `.decapod/managed/specs/*.md`) deterministically and outputs `decapod.artifact-clarity.v1` (`formal_asd_ste100_compliance: false`, `deterministic: true`, `findings[]`). Exits non-zero (`CLARITY_VALIDATION_FAILED`) on error-severity findings.
- **Configuration contract (`src/decapod/cli.rs::GovernanceConfig`, `src/decapod/core/docs_cli.rs`)**:
  - Adds `[governance].main_merge` (`bool`, default `false`, omitted when false). Candidate-branch opt-in beyond accepted base policy fails closed with `POLICY_SELF_AUTHORIZATION_REJECTED`.
- **Offline historical exception carrier (`src/decapod/core/governance_document/historical_exception.rs`)**:
  - Accepted base trust root `.decapod/historical-exception-signers.json` (`decapod.historical-exception-signers.v1`) and `DECAPOD_HISTORICAL_EXCEPTION_DECISION` (`decapod.historical-exception.v1`), producing `decapod.historical-exception-judgment.v1` with `history_proven: false`.
- **Rust core interfaces (`src/decapod/core/obligation_ledger.rs`, `src/decapod/core/formal_obligations.rs`, `src/decapod/core/governance_document/semantic_integrity.rs`)**:
  - Exposes typed deterministic evaluators `derive_obligation_ledger` (`decapod.obligation-ledger.v1`), `StagnationTracker`, `verify_outcome_claim_anchor`, `build_completion_report_4d` (`decapod.completion-report-4d.v1`), `ApprovalLedger`, `evaluate_trusted_grader` (`decapod.trusted-grader-report.v1`), `evaluate_root_conservation`, `evaluate_main_merge_authorization`, `validate_semantic_governance_integrity`, `detect_governance_theater_diff`, `verify_formal_obligation` (`decapod.formal-obligation.v1` / `decapod.formal-obligation-verdict.v1`), and `CapacityAccountingState`.

## Maintenance WAL checkpoint, legacy TODO logical recovery, read-only analyze, workspace effective-state/publish planning, and pre-build validate (#1568, #1657, #1279, #1071, #1064, #1047)

- **Database maintenance CLI & RPC (`src/decapod/cli.rs::DatabaseMaintainCommand`, `src/decapod/core/rpc.rs`, `src/decapod/core/interop.rs`)**: Adds `decapod data db maintain wal-checkpoint [--timeout-ms <0..=5000>]` (and RPC `data.database.wal_checkpoint` with `DatabaseWalCheckpointParams`), `lock-inspect` (`db.lock.diagnose`), plus visible aliases `diagnose` (`inspect`) and `integrity-check` (`verify`). Accepts `[maintenance]` alongside `[governance.db_maintenance]` in `.decapod/config.toml`.
- **Legacy TODO recovery CLI (`src/decapod/core/todo.rs::TodoJournalCommand::ReconcileLegacy`)**: Extends `decapod todo journal reconcile-legacy` with `--export <PATH>`, `--quarantine`, and `--import-missing` (mutually exclusive with `--prepare-only` and requiring `--expected-review <sha256>` for mutating actions). Review responses expose `quarantine_review_sha256`, `logical_summary`, `can_quarantine`, and `can_import_missing`.
- **Read-only analysis CLI (`src/decapod/cli.rs::Command::Analyze`, `AnalyzeCli`)**: Adds `decapod analyze [--read-only] [--format text|json] [--max-files N] [--max-bytes N] [--timeout-ms N] [--draft-spec] [PATH]` returning `decapod.analyze-readonly.v1` JSON or concise text with zero repository/SQLite/session mutations.
- **Workspace effective-state & publication planning CLI (`src/decapod/cli.rs::WorkspaceCommand`, `WorkspacePublishMode`)**: Adds `visible_alias = "effective-state"` and `--format text|json` to `WorkspaceCommand::Diagnose`, plus `--pr <N>`, `--branch <NAME>`, `--mode full|governance-only`, and `--dry-run` (`decapod.workspace-publish-plan.v1`) on `WorkspaceCommand::Publish` (and `pr_reused` + `mode` on `PublishResult`).
- **Pre-build validate CLI (`src/decapod/cli.rs::ValidateCli::pre_build`)**: Adds `decapod validate --pre-build [--format text|json]` returning `decapod.pre-build-validate.v1` with `scope: "pre_build"`, `read_only: true`, `publication_ready: false`, and `proof_ready: false`.

## Cloud parity inspection, constitution overlays, and precedence RPCs (#1895, #877)

- **Parity CLI and RPC (`#1895`)**: Adds `decapod capabilities parity [--dir <PATH>] [--format text|json]` and read-only RPC `interop.cloud_parity` returning `decapod.cloud-parity-report.v1`, plus explicit unsupported Cloud lease renewal refusal (`CLOUD_LEASE_RENEW_UNSUPPORTED`) in `src/decapod/core/cloud_parity_matrix.rs` and `src/decapod/core/interop.rs`.
- **Constitution overlay CLI, RPC, and manifest (`#877`)**: Adds `decapod constitution get --resolved`, `decapod constitution search` provenance metadata, and `decapod constitution overlay {status|verify|import|clear}` (`src/decapod/core/constitution_cli.rs`, documented in `src/decapod/core/docs_cli.rs` and `docs/agent/command-contracts.md`). Adds read-only RPC `constitution.overlay.status` (`handle_constitution_overlay_status` in `src/decapod/lib.rs`, schema types `ConstitutionOverlayStatusParams`/`ConstitutionOverlayStatusResult` and extended `ContextResolveResult`/`ContextCapsule`/`ConstitutionGetParams`/`ConstitutionGetResult` in `src/decapod/core/rpc.rs`), plus `authority_hierarchy` (`AuthorityHierarchyEntry`) on `InteropManifest` in `src/decapod/core/interop.rs`.

## First-class `decapod cloud` CLI & JSON RPC admin contracts (#1896, #1836, #1835, #1877)

- **CLI subcommands (`src/decapod/cli.rs::CloudCommand`, `src/decapod/lib.rs::run_cloud_command`)**:
  - `decapod cloud orgs [--project-id <id>]`: outputs `decapod.cloud.orgs.v1` with `organization_id`, `cloud_org_id`, `slug`/`display_name`, `admin`, `paid_repo_seat_assigned`, `paid_repo_count`, `org_admin_without_paid_seat`, `access_model_note`, and response header `warnings`.
  - `decapod cloud members {list|assign|revoke-seat|revoke-member|transfer-ownership|project-settings}`: outputs `decapod.cloud.members.v1`; `list` supports `--organization-id`/`--org`, `--project-id` (`access_details`), and integer `--after`; `assign` takes `--project-id` + `--slot`; `revoke-seat` supports project-scoped `Manage` (`--project-id` + `--slot` or `--member-account-id`) and org-slot `Register` (`--slot`); `revoke-member` and `transfer-ownership [--expected-revision]` take `--member-account-id` UUID; `project-settings` updates `--project-id` + `--project-name`.
  - `decapod cloud invites {list|create|cancel|accept}`: outputs `decapod.cloud.invites.v1` with `membership_grants_paid_seats: false` and explicit zero-paid-seats notice; `accept --invitation-id <uuid> --secret-stdin` reads the one-time secret strictly from stdin (zero secret flags exist on CLI).
  - `decapod cloud identity {prepare-link|confirm-link|resolve-quarantine|revoke-link}`: uses `CloudIdentityProvider::{Google,Github,Gitlab}` and `CloudQuarantineDecision::{Confirm,Revoke}`; `confirm-link --intent-id <uuid> --secret-stdin` reads `stepup_secret` strictly from stdin.
  - `decapod cloud billing {portal|limits}`: outputs `decapod.cloud.billing.v1`; `portal [--organization-id]` validates strict `https://` origin + zero credentials; `limits [--project-id]` projects `resource_limits`, `spend_cap_status`, and `spend_cap_warning` (true on `warning | overage_cap_exhausted | exhausted | cap_reached`).
- **JSON RPC ops (`src/decapod/core/rpc.rs`, `src/decapod/core/interop.rs::RPC_OPS`, `src/decapod/lib.rs`)**:
  Registers `cloud.{orgs,members,invites,identity,billing}` with `session_required = false`, typed `Cloud{Orgs,Members,Invites,Identity,Billing}Params`, and `SubsystemInfo` entries; dispatches early via `run_cloud_admin_rpc` (with `execute_rpc` parity and `is_cloud_file_only_command` / `rpc_op_requires_worktree` / `rpc_op_bypasses_session` / `rpc_op_skips_mandate_enforcement` alignment) without creating `.decapod/data/decapod.db`. Rejects `--params` argv secrets in `parse_rpc_request` and redacts secrets from receipts/errors/traces (`cloud_auth_diagnostic_code` attaches serialized `CloudAuthDiagnostic` in `RpcError.details`). Regenerated `docs/agent/command-contracts.md` via `decapod docs build` (`src/decapod/core/docs_cli.rs`).

## Effective operation grant explanation, delegation CLI, and guidance hygiene (#1905, #1893)

- **Read-only CLI grant explanation (`src/decapod/plugins/policy.rs::PolicyCommand::Grant`)**:
  `decapod govern policy grant --command <OP> [--path <PATH>] [--task-id <ID>] [--resource <RES>] [--recipient <AGENT>] [--budget-units <N>] [--expected-prestate <DIGEST>] [--harness-id <ID>] [--format text|json]` evaluates `EffectiveOperationGrant` (`decapod.effective-grant.v1`) read-only (`observing = true`, `is_read_only_operation` admits `policy.grant` and `*.explain`), printing bounded text or JSON (`decision`: `allowed | denied | unproven | stale`, `reasons`, `effective_allowances`, `effective_denials`, `effective_max_budget_units`, `remaining_delegation_depth`, `cumulative_budget`, `target_fingerprint`, `approval_ids_matched`, `prestate`, `layers`, `execution_outcome`).
- **Read-only RPC interface (`src/decapod/core/rpc.rs::PolicyGrantExplainParams`, `src/decapod/core/interop.rs::RPC_OPS`, `src/decapod/lib.rs::rpc_handlers::handle_policy_grant_explain`)**:
  Registers `policy.grant.explain` (`read_only: true`, `session_required: false`) in `RPC_OPS`, generates `### Operation: PolicyGrantExplain` in `docs/agent/command-contracts.md`, binds actor identity strictly from `current_agent_id()`, and returns the computed `EffectiveOperationGrant` JSON payload.
- **Task delegation subcommand (`src/decapod/core/todo.rs::TodoCommand::Delegate`)**:
  `decapod todo delegate --parent <TASK_ID> [--child <TASK_ID>] [--recipient <AGENT>] [--allow <PATTERN>...] [--deny <PATTERN>...] [--op <OP>...] [--resource <RES>] [--budget-total <N>] [--budget-child <N>] [--depth <N>]` records monotonic parent-to-child delegation bounds atomically in `events::TODO` (`todo::schema()` exposes `"delegate"`).
- **Guidance & context hygiene (`#1893`)**:
  CLI help (`--help`, `validate --help`, `init --help`, `govern --help`), `decapod docs ingest`, `context.resolve`, `local_project_specs_context`, and `.decapod/managed/specs/*` exclude maintainer-only development entrypoint-version-drift override guidance while keeping `.decapod/config.toml` parsing backward-compatible.

## Composed capability contract CLI & RPC interfaces (#1906)

- **Read-only CLI (`decapod capabilities contracts`, `src/decapod/cli.rs::CapabilitiesCommand::Contracts`, `src/decapod/lib.rs::run_capabilities_command`)**:
  `decapod capabilities contracts [--fixture] [--contract <PATH>] [--dir <PATH>] [--format text|json]` inspects composed service capability contracts without startup migrations, SQLite initialization, session acquisition, or network calls. `--fixture` executes the deterministic `Decapod -> Dactyl -> Propodus` compatibility suite (`decapod.capability-compatibility-fixture.v1`), and `--contract <PATH>` evaluates a candidate `decapod.capability-contract.v1` JSON file against accepted project policy (`decapod.capability-inspection.v1`). Existing `decapod capabilities [--format json|text]` output remains unchanged.
- **Read-only RPC (`interop.capability_contracts`, `src/decapod/core/interop.rs`, `src/decapod/core/rpc.rs::InteropCapabilityContractsParams`)**:
  Registered in `interop::RPC_OPS` with `read_only = true`, `session_required = false`, and bypassed worktree/mandate mutations in `src/decapod/lib.rs`. Accepts `{ fixture?: bool, contract?: object }`, returning `InteropCapabilityContractsResult` with `input_origin` (`"repository_pinned_contracts"` or `"caller_supplied"`) and `canonical_state_verified = false`.
- **Schema contract (`assets/schemas/capability-contract.schema.json`)**:
  Draft 2020-12 strict schema (`additionalProperties = false`, `$id = https://decapod.dev/schemas/capability-contract-1.0.0.schema.json`) enforced by `tests/assets_schema_contract.rs`.

## Container snapshot precheck, `--heartbeat` lease extension, and shared measurement contracts (#1881, #1837, #1884, #1838)

- **Interactive container workspace precheck (`src/decapod/core/workspace.rs`)**:
  - `decapod workspace ensure --container` fails early into `status: "pending"` with `BlockerKind::Unauthorized` (`WORKSPACE_CONTAINER_SNAPSHOT_DENIED (<code>): <detail>`) when canonical `db.backup` policy is disabled/denied, without invoking `docker`/`podman build`.
  - When backup is permitted, the emitted launch command stages `decapod.db` inside `.decapod/backups/container-store.XXXXXX` (`mkdir -p -m 700`) and mounts it read-only to `/tmp/decapod-store.db`.
- **`decapod todo renew --heartbeat` (`src/decapod/core/todo.rs::TodoCommand::Renew`)**:
  - Adds `--heartbeat` boolean flag to `decapod todo renew --id <ID> [--lease-seconds <N>]`. Extends `lease_expires_at` only if `now + lease_secs > previous_secs` while keeping `lease_generation` and `revision` unchanged; returns `LEASE_HEARTBEAT_STALE` if inactive, wrong holder, or generation mismatched.
  - `DECAPOD_VALIDATE_LEASE_TTL=warn` emits `VALIDATION_LEASE_TTL_LOW` hinting `decapod todo renew --heartbeat --id <task_id> --lease-seconds <secs> before running validation`; `DECAPOD_VALIDATE_LEASE_TTL=renew` triggers `heartbeat_claim_lease`.
  - Cloud-mode `TodoCommand::Renew` (including `--heartbeat`) continues rejecting early in `check_cloud_todo_preflight` with `CLOUD_LEASE_RENEW_UNSUPPORTED`.
- **Read-only freshness identity (`src/decapod/core/native_proof.rs::capture_readonly_identity`)**:
  Preserves identical digest format (`measured_toolchain_inputs`), byte budget enforcement, deadline, and cancellation behavior while hashing each distinct file path once per capture call.

## Portable policy-transition carriers and target-scoped diagnostics (#1833, #1832)

- **Signed CI approval carrier (`docs/agent/repository-readiness.md`, `src/decapod/core/repository_policy_approval.rs`)**:
  - Accepted base signer manifest at `.decapod/repository-policy-signers.json` (`schema = "decapod.repository-policy-signers.v1"`, `keys` mapping key IDs to 64-hex raw 32-byte Ed25519 public keys).
  - Environment carrier `DECAPOD_REPO_TRANSITION_APPROVAL` holding JSON `schema = "decapod.repository-policy-approval.v1"`, `key_id`, `transition_id`, `issued_at`, `expires_at` (`<= 86_400s` window), and 128-hex `signature` over `["decapod.repository-policy-approval.v1", key_id, transition_id, issued_at, expires_at]`.
- **Cloud policy approval interface (`src/decapod/plugins/policy.rs::run_policy_cli_in_workspace`, `src/decapod/core/repository_policy_approval.rs::approve_cloud`)**:
  - `decapod govern policy approve --id <transition-id> --scope repo.pr-readiness.v1` on a Cloud-configured repository records a `propodus_governance_v1` decision (`authorize_exception`) and prints `DECAPOD_REPO_TRANSITION_DECISION_ID=<uuid>` for Cloud validation (`verify_decision`). Without `supabase-cloud`, Cloud approval/verification fails closed with `CLOUD_POLICY_APPROVAL_UNSUPPORTED`.
- **Target-scoped readiness rejection diagnostic (`src/decapod/core/validation_history.rs::current_target_only_diagnostic`, `src/decapod/lib.rs`, `src/decapod/plugins/verify.rs`)**:
  - Adding, removing, or modifying `proof.repository_contract.targets` triggers `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`. When `HEAD` holds only target-scoped validation proof (`--target <name>`), `workspace publish` (`enforce_workbranch_readiness_before_push`) and `qa verify` (`run_validate_and_hash_detailed`) emit `TARGET_SCOPED_PROOF_REJECTED` instructing the caller to run unscoped `decapod validate`.

## Cloud Wave 2 CLI, error diagnostics, and hosted workspace interfaces (#1836, #1835, #1886)

- **`decapod cloud` subcommands (`src/decapod/cli.rs::CloudCommand`)**:
  - `decapod cloud status [--live]`: Without `--live`, emits private local
    session/device state without network calls (`server_readiness: "not_checked"`).
    With `--live`, queries `AccountOperation::Status` (`operation: "resource_limits"`)
    and includes `resource_limits`, `spend_cap_status`, `spend_cap_warning`, and
    redacted header `warnings`.
  - `decapod cloud devices [--live]`: Reports non-secret `DeviceKeyInventory`
    records (`origin`, `repository`, `jkt`, optional `device_key_id`, `backend`,
    `scope`, `keystore_policy`, `os_keychain_available`) without key material;
    `--live` checks project authorization and surfaces `CloudMetadataWarning`s.
  - `decapod cloud revoke [--all] [--device-id <jkt-or-id>] [--session-logout] [--local-only]`:
    Supports offline `--local-only` cleanup, explicit DPoP-signed
    `--session-logout` (`action = "logout"`, mandatory enrolled device signer),
    or management revocation (`--all` / `--device-id`).
- **Typed cloud error mapping (`src/decapod/core/error.rs::CloudAuthStatus`)**:
  Adds `QuotaExceeded` (`quota_exceeded`, `AdapterErrorKind::Quota`),
  `OverageCapExhausted` (`overage_cap_exhausted`), and `ProviderDisabled`
  (`provider_disabled`), all mapped to `FailureKind::Authorization` in
  `cloud_todo_operation` and carrying zero upstream error strings.
- **Hosted workspace & validation boundary (`src/decapod/core/cloud_workspace.rs`)**:
  When accepted `.decapod/config.toml` selects `backend = "cloud"`,
  `decapod workspace ensure` (and RPC `workspace.ensure`) bind or reconcile
  `CloudCustodyRecord` via `get_operation`, seed branch governance from hosted
  tasks (`hosted_tasks_for_governance_seed`), reject direct unsupported
  `todo renew` with `CLOUD_LEASE_RENEW_UNSUPPORTED`, and support host-relay
  container execution (`DECAPOD_CLOUD_CONTAINER_RELAY=1`) via
  `RelayProofControlTransport` without exposing tokens or creating `decapod.db`.

## Pay-and-use Cloud CLI flags, auth diagnostics, and pricing URL contract (#1877, #1896)

- **Managed project commands (`src/decapod/cli.rs`, `src/decapod/lib.rs`, `src/decapod/core/cloud_project.rs`)**:
  `decapod cloud project use <CHOICE> [--org <ORGANIZATION_ID>]` and `decapod cloud project create <NAME> [--org <ORGANIZATION_ID>]` confirm organization ownership across `cloud_project::execute`. `ProjectView` includes `organization_id: Option<String>`, and `decapod cloud projects` appends `({organization})` in text mode when present.
- **Cloud auth & todo operation diagnostics (`decapod.cloud.auth.v1`, `decapod.cloud.todo-operation.v1`)**:
  `CloudAuthStatus` serializes as snake_case JSON values `registration_required`, `provider_link_stepup_required`, `payment_required`, `pending_fulfillment`, `admin_approval_required`, and `organization_already_enrolled`. When repository enrollment is actionable (`payment_required`, `entitlement_required`, and seat-required aliases), diagnostics attach `"enrollment_url": "https://decapodlabs.ai/pricing?repo=<owner%2Frepo>"` (or `"https://decapodlabs.ai/pricing"` when no valid slug is present) and render `; enroll: <url>` in human error text. `cloud_todo_operation::Diagnostic` propagates `enrollment_url` alongside `operation_id` and `authorization_url` without altering `OperationRecord` schema compatibility.
- **Additive server account-status response fields (`src/decapod/core/cloud_project.rs::Response`)**:
  Supports optional `authorization_state`, `pending_fulfillment`, `cloud_org_id`, `admin`, `repo_assigned`, `organizations`, and `projects[].organization_id` with `#[serde(default)]`, preserving full backward compatibility with legacy payloads.
- **Verified offline & unit proof coverage (`tests/unit/core/cloud_project_tests.rs`, `tests/common/cloud_managed_scenarios.rs`, `tests/unit/core/propodus_tests.rs`, `tests/cloud_backend_storage.rs`)**:
  Covers URL encoding/validation, all P0 status code & `FailureKind` mappings, `admin=true, repo_assigned=false` orthogonality, immutable `cloud_org_id` surfacing, `payment_required` / `pending_fulfillment` session preservation and automatic resume, `--org` selection and request-ID isolation, headless JSON secret-redaction checks, and zero local `decapod.db` creation on Cloud init refresh.

## Bounded context and host observations

`context.resolve` retains its fragment contract and adds selection reasons, stable source identities, a scoped advisory knowledge projection, and current governed-plan context with explicit task binding. It returns an explicit no-match result for advisory queries and retains mandatory obligations. New capsule and bundle fields are versioned. Legacy hashes stay stable when the optional fields are absent. The complete context envelope is capped at 262,144 bytes, with at most twelve fragments and 8,192 excerpt bytes.

`infer boundary --input` accepts a bounded `decapod.native-boundary.v1` request with a host agent ID. `trace flight-recorder compare --input` reads compact outcome observations and preserves missing metrics and incomplete runs. `trace flight-recorder acceptance --capture` continues to correlate the independently captured observer inputs.

`mentor.obligations` accepts authenticated queries before source authoring. The reflection engine verifies workspace and task custody before it updates a continuation. Its result does not grant publication authority.

## Caller-supplied validation report projections (#1726)

Every `interop.project` result adds optional `input_origin=caller_supplied` and
`canonical_state_verified=false`. These describe the operation/approval/report
input, not the receipt proving that the transformation itself ran. Caller input
cannot override these output fields. The original per-kind `source` remains an
input-type label, not authenticated producer provenance. Reported approval and
denial statuses remain display data; their remediation no longer claims that
the corresponding canonical policy operation occurred. Only the existing
policy owner can establish a real grant or denial.

`interop.project` with `kind=validation_report` remains a successful read-only
transformation; outer RPC success does not verify its input. The reported
`status`, candidate, epoch and identifiers remain visible. `proof_judgment` for
a well-formed reported PASS is `not_proven` and `publication_ready` is false,
including plausible receipt digests and caller-matched verifier/rule identities.
Reported failed, stale and unknown inputs retain their existing classifications.
The optional additive `proof_reason` explains that these are caller-supplied
reports and directs the reader to canonical validation for the exact candidate
and accepted base. A copied genuine report also cannot prove present currency
through a pure transformation. Current proof must come from the canonical owner.
Existing actual-binary controls execute counterfeit, negative/stale and ordinary
negotiation recovery without provider, custody or gate-skip overrides.

## Reversible repository integration removal (#1704)

`decapod integration {preview, remove, restore}` exposes a repository-level
removal lifecycle grounded in `src/decapod/core/integration_removal.rs` and
dispatched before session/migration startup in `src/decapod/lib.rs`:

- **Read-only preview (`decapod integration preview [--dir <path>] [--format text|json]`):**
  Performs zero filesystem, Git, SQLite, session, or network mutations. Returns
  `IntegrationPreviewReport` listing `generated`, `edited`, `authored`,
  `unknown`, and `active` resources separately, plus actionable `blockers` and
  `authority_boundaries`.
- **Bounded removal (`decapod integration remove --authorize [--dir <path>] [--format text|json]`):**
  Requires explicit `--authorize` (and refuses `--dry-run` with guidance to use
  `preview`). Fails closed (`exit != 0`) on symlink replacement
  (`INTEGRATION_SYMLINK_REFUSED`), active custody (`INTEGRATION_ACTIVE_CUSTODY`:
  todo leases, `authoring_bindings`, registered/non-empty `.decapod/workspaces/`,
  held workspace/storage locks), or uncertain ownership
  (`INTEGRATION_UNCERTAIN_OWNERSHIP`). Removes only positively owned,
  unchanged generated files (via `.decapod/managed/integration_ownership.json`
  or verified release/fingerprint markers on legacy repos) and owned
  `DECAPOD_GITIGNORE_RULES` lines in `.gitignore`. Preserves edited/authored/unknown
  files, `*.bak` backups, `.decapod/data/`, `.decapod/governance.json`, global
  config, and credentials. Writes `.decapod/integration-restore-manifest.json`
  atomically (`status = "in_progress"` -> `"applied"`) before deleting files so
  interrupted or repeated removals remain idempotent.
- **Restore (`decapod integration restore [--dir <path>] [--manifest <path>] [--format text|json]`):**
  Losslessly restores captured files (including Unix file modes) and
  `.gitignore` entries from `.decapod/integration-restore-manifest.json`, while
  failing closed if the manifest is missing/unsupported or if a destination
  became a symlink or holds conflicting authored content.
- **Separate authority boundaries:** `--purge-state`
  (`PERMANENT_STATE_DELETION_BOUNDARY`) and `--remove-protected-governance`
  (`PROTECTED_GOVERNANCE_REMOVAL_BLOCKED`) are refused by integration removal.

## Delegated cgroup parent selection and cgroup diagnostics (#1820)

`decapod auto container run` (`src/decapod/plugins/container.rs::ContainerCommand::Run`)
accepts an optional `--cgroup-parent <path-or-slice>` flag and honors the
`DECAPOD_CONTAINER_CGROUP_PARENT` environment variable (CLI flag takes precedence;
`container::schema()` exposes both `"cgroup_parent"` and `"cgroup_parent_env"`).
All explicit and detected values pass `validate_cgroup_parent` fail-closed before
any runtime probe or workspace clone: only canonical absolute cgroup hierarchy
paths (`/delegated/parent`, without `/sys/fs/cgroup` mount prefixes, root `/`,
trailing slashes, empty segments, `.` or `..` traversal, whitespace, control
characters, leading `-`, or metacharacters) and systemd slice names (`*.slice`)
are accepted; invalid values fail with `container_invalid_cgroup_parent`.

When no explicit parent is supplied and local `podman info` reports rootless
`cgroupfs` on cgroup v2 (`serviceIsRemote: false`), `detect_delegated_cgroup_v2_parent_from`
reads `/proc/self/cgroup` (`0::<path>`) and walks upward from the caller's cgroup
to the deepest non-root ancestor under `/sys/fs/cgroup` that is a real directory
(no symlink traversal), writable by the current user, and enables `cpu`, `memory`,
and `pids` in `cgroup.subtree_control`. `build_docker_spec` injects
`--cgroup-parent <validated-parent>` while preserving `--pids-limit 512`,
`--memory`, and `--cpus` unchanged; cgroups are never disabled.

`classify_container_failure` distinguishes OCI/cgroupfs permission errors
(`crun`/`runc` `cannot set limits without cgroups`, `/sys/fs/cgroup` permission
denied, `unable to apply cgroup configuration`, `cgroup.subtree_control`/`cgroup.procs`
errors) as `container_cgroup_permission_denied` with delegated cgroup parent
remediation, leaving actual workspace ownership and Git lockfile failures under
`container_workspace_permission_denied`. `RunSummary` exposes the resolved
`cgroup_parent` field for observability.

## Explicit path scope preservation and reconciliation (#1760)

Explicit task path declarations (`--scope 'paths:[...]'`) are parsed once via
`core::fleet_coord::parse_scope_entries` (delegating `paths:` prefixes to
`core::fleet_coord::declared_paths`) rather than naive comma splitting. That
single bounded JSON-array parser preserves both JSON array separators and commas
embedded inside literal relative paths (such as `docs/a,b.md`) across
`infer orientation` (`allowed_scope`), `workspace::initialize_workbranch_governance_at`
(`BranchWorkContext.scope`), `trajectory::fresh_trajectory` / `record_trajectory`
(`repo_scope`), and `custody::bootstrap_intent` (`intent.constraints`), keeping
`todo get`, `infer orientation`, `govern trajectory get`, and `govern state resolve`
consistent without weakening `declared_paths` validation or modifying legacy
comma-separated scopes.

Already-generated fragmented scope projections in `.decapod/governance.json`
are repaired through `decapod govern trajectory reconcile-scope [--task-id <id>] [--run-id <id>]`
(`core::governance_document::reconcile_scope_projections`). Before mutating any
projection, reconciliation resolves a single unambiguous task binding
(`SCOPE_RECONCILIATION_AMBIGUOUS_TASK` / `SCOPE_RECONCILIATION_TASK_REQUIRED`),
verifies that existing fragments match the task's legacy comma-split projection
(`SCOPE_RECONCILIATION_AMBIGUOUS`), and requires the prior governance document to
be committed in Git (`SCOPE_RECONCILIATION_ARCHIVE_REQUIRED` via `recoverable_in_git`).
Repair restores canonical sorted paths in both `sections.trajectory.repo_scope`
and `intents.<id>.constraints`, recomputes `trajectory.artifact_hash`, invalidates
`sections.plan.state` back to `DRAFT` when changes occur, preserves original
recorded intent, prior custody events, checks, loops, and checkpoints, and is
idempotent (`SCOPE_ALREADY_CANONICAL`) on repeat invocations.

## Unified Local/Cloud CLI, auth, container credentials, repository tiers, and proof acceptance (#1809, #1808, #1769, #1778, #1590, #1033, #1040, #1066)

- **Unified Local/Cloud CLI, env, and store envelopes (#1808, #1769)**:
  `src/decapod/cli.rs` exposes `DECAPOD_CLOUD_API_URL`, `DECAPOD_CLOUD_PROVIDER`,
  `DECAPOD_CLOUD_DATASTORE`, and legacy `DECAPOD_PROPODUS_API_URL` (emitting
  structured `decapod.deprecation.v1` notices on stderr). `CapabilitiesReport`
  exposes `backends` (`local` / `Decapod Local`, `cloud` / `Decapod Cloud`),
  `backend.local`/`backend.cloud`, and the `cloud` subsystem. `propodus.rs`
  accepts `decapod.cloud.store` (`v1`) envelopes and `Decapod-Cloud-*` headers
  alongside legacy compatibility envelopes. `docs/book/src/index.md` and
  `docs/book/src/custom.css` present **Decapod Local** vs **Decapod Cloud**
  linking to `https://decapodlabs.github.io/offerings.html` without public
  prices or checkout routes.
- **CLI `decapod cloud`**: Adds `logout` (clears session state while preserving
  the enrolled device key) and `revoke [--all] [--device-id <jkt>]` (calls
  `AccountOperation::DeviceRevoke` when an active session is present and wipes
  local device key + session). `decapod cloud status` reports
  `device_key_present`, `device_jkt`, and `device_keystore` without exposing
  private key bytes or tokens. `CloudAuthStatus` adds `DeviceRevoked`,
  `KeyLost`, `MfaRequired`, `PaymentPending`, and `EntitlementDenied`.
- **Container credential config (`[workspace.credentials]`)**: Adds optional
  `git_transport`, `github_api`, and `propodus_cloud` fields on
  `DecapodProjectConfig`. `decapod workspace status` works under `backend = "cloud"`
  and emits redacted `credentials` capability readiness (`available`,
  `unconfigured`, `absent`, `expired`, `unauthorized`) without touching SQLite.
- **Database maintenance & owner repair (`[governance.db_maintenance]`)**:
  Adds optional `owner_repair_enabled` (default `false`) and `required_tier`
  (`read` | `write` | `owner`). `db.recover` requires merged canonical policy
  and `owner` authority on local stores (dump/reload via Dactyl on Linux,
  explicit typed `Capability` refusal on non-Linux hosts) and
  `propodus_repository_access_v1` + `owner_repair_v1` on cloud stores, where
  `OWNER_REPAIR_V1_SQL` transmits strictly the registered wire marker
  `SELECT propodus.owner_repair_v1($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`
  inside `DactylBridge::atomic` (never server-internal SQL expansion).
- **Cloud todo & proof capture**: `todo claim`, `todo renew`, `todo yield`, and
  `todo done --validated` route through Dactyl without `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED`.
  `todo done --validated` fails closed locally with `"proof-capture"` when
  candidate commit/evidence bounds are absent; when valid evidence is supplied,
  it submits `propodus_proof_acceptance_v1` `capture` (`status = "captured"`,
  `acceptance_supported = false`) and reconciles `outcome_unknown` through
  read-only `propodus_proof_acceptance_get_v1` `get_operation` rather than
  auto-replaying mutations. Typed HTTP/control denials surface distinct
  `CLOUD_ENTITLEMENT_DENIED` (`403`), `CLOUD_PAYLOAD_BOUND_EXCEEDED` (`413`),
  `CLOUD_QUOTA_EXCEEDED` (`429`), and `CLOUD_OUTCOME_UNKNOWN`.

## Workunit proof and continuity semantics (#1660)

Before full proof, use `govern capsule query --task-id <id> --workunit-id <id>
--scope interfaces --topic <topic> --write`, then attach its emitted task
capsule path using `workunit attach-state`. Creating a capsule does not itself
prove execution. A fresh workunit checkpoint binds the resulting context;
full validation and the canonical VERIFIED transition remain separate steps.

`record-proof --status pass` may retain an agent declaration about an existing
artifact. Success does not authorize `transition --to verified`; that operation
requires a current canonical full-validation record. Readback preserves the
unproven declaration without a validation epoch or producer reference.
After relevant trajectory changes, resume refuses the old execution binding.
Discarding obsolete passing declarations allows a fresh checkpoint and resume;
the explicit diagnostic fail marker used to discard a claim is not an
independently executed failed test. Historical journal entries remain intact.

A completed task-bound full validation with a not-ready judgment withdraws
current workunit verification through the existing journal. It clears the
current proof results, validation epoch and producer selector, downgrades
VERIFIED to CLAIMED, and advances the journal even if promotion had not yet
occurred. The event is `validation_not_ready`: it does not assert that every
check executed or failed. The journal reader recognizes this event kind while
retaining the same identity, manifest hash, record hash and predecessor checks;
the resulting CLAIMED snapshot must remain readable for repair. Partial CI-local
and preparation scopes do not have this authority. Earlier successful
observations remain historical records.

The executable negative control selects the native capture by its full check
identity (`inspect-source`), then requires an exited process with code 1 and the
expected assertion diagnostic. Canonical ordering places the independent
`inspect-second` check first; its successful result cannot substitute for the
required failure or be mistaken for evidence that the control failed to execute.

Fresh full proof under a renewed lease can restore VERIFIED. The implicit
workunit selector chooses the newest canonical success for the exact current
task contract and custody; every epoch, source, execution, input and workunit
subject check still applies. Generic `qa verify commit` inspection retains its
mixed-custody ambiguity error and requires explicit historical selection.

The split-store refusal now gives a bounded recovery path: preserve both stores,
claim a fresh task from the repository root, use a managed workspace sharing
the canonical runtime, reauthor its obligations and obtain new full proof.
This is not in-place store convergence or historical-proof authentication;
that migration remains unsupported.

## Config repair and accepted authority (#1771, #1776)

Configuration errors use `INVALID_CONFIG_SCHEMA` with the original parser
location/type/expected-value diagnostic and embedded repair commands. Supported
schema version is 1.0.0; root extension compatibility remains unchanged.
`data schema` and `rpc --op schema.get` expose their existing schemas without
config parsing or datastore/session mutation, including on malformed projects.
Ordinary mutations resume after a valid correction. Valid syntax alone never
grants exceptional permission. RBAC bypass keeps `proof_ready=false` and
`publication_ready=false`; unaccepted root policy proposals return
`RBAC_PERMISSION_DENIED`, and unresolved accepted sources fail closed.

## Conformance evidence boundary (#871)

The existing broker responsiveness assertion retains its ten-second limit for
the competing operation while optional preparation is blocked. Claim completion
after the explicit release is separately bounded and must still succeed; its
cleanup duration is not part of that competing-operation measurement.

`context.bundle.export`, `context.bundle.import`, `mentor.obligations` and
`assurance.evaluate` advertise `session_required=true`, matching existing
entry enforcement. This includes automatic acquisition; compatibility metadata
does not issue a session. Bounded `data.database.inspect` remains session-free.

Malformed `rpc --op ... --params ...` JSON returns nonzero with
`Invalid JSON parameters`, before project/session startup; it is never converted
to an empty compatibility request. Omitted parameters and explicit empty objects
retain their existing meaning. Valid requests requiring an unsupported extension
retain the `negotiation_refused` envelope; successful compatibility discovery
still grants no mutation, completion or publication permission.

Both RPC stdin framing and flag parameters reject duplicate decoded object
keys at every nesting level before startup. Escaped spellings of the same key
are duplicates; later fields cannot erase an earlier required extension.
Existing 32 MiB framing bounds and redacted inventory/maintenance diagnostics
remain. JSON syntax acceptance does not waive the strict negotiation schema:
unknown fields still return `invalid_params`. Supported optional operation and
extension fields can carry unknown identifiers; negotiation reports them as
omitted, preserving decoded Unicode identifiers. A focused
Python stdlib decoder and the actual Rust CLI execute the same malformed and
ordinary inputs; this is same-host differential parsing, not independent
producer trust or full foreign-client governance conformance.

The v1 manifest adds the existing reconciliation and state-read operations.
Reconciliation remains advisory and session-free; state reads retain their
existing session/workspace gates. Unknown RPC names now receive the existing
`unknown_op` envelope before startup, including in an uninitialized Git project.
Both stdin RPC and the HTTP adapter use that same local entry boundary. No
negotiation result or catalog entry supplies mutation permission.

`interop.manifest` retains its v1 wire shape and source coverage references.
`evidence.conformance_test` and `evidence.evidence_ref` identify source tests;
they do not attest execution, transport, binary identity or current applicability.
Built-in evidence is `not_run`. The Rust manifest validator refuses authored
`verified`/`failed` with `unobserved_conformance`, including known tests and
plausible references. Historical JSON remains deserializable without gaining
current assurance. No new execution producer, receipt import or authorization
path is introduced; actual observed conformance remains an unresolved contract.

## Internal source currency agreement (#1753)

The existing private config-only source policy now also governs version-only
entrypoint participation in the publication diff. No new option or public wire
field is introduced. Authorized canonical source headers remain unchanged when
generated metadata is already current. A generator upgrade still requires Docker
and manifest updates; disabled, foreign, ambiguous or invalid source policy keeps
the ordinary release-bound refresh rule and blocking diagnostics.

## Timestamp compatibility (#1755)

Task, KnowledgeEntry and Decision timestamps remain RFC3339 strings normalized
to UTC with Z and 0/3/6/9 fractional digits. Explicit observation timestamps keep
+00:00; Dactyl operation markers keep nine fractional digits and Z. Legacy SQL
UTC timestamps remain accepted at the Dactyl read boundary only. Expiry inputs
remain RFC3339 and invalid values still expire or require refresh; the separate
policy epoch-Z compatibility and command envelope wire format are retained.
The internal Rust model type is now `time::OffsetDateTime`.

## Admission and quarantine (#1747, #1748)

`workspace publish` and RPC publication carry completed live validation through
their existing readiness boundary. `PUBLICATION_PROOF_SUBJECT_CHANGED` rejects
a different repository, work branch, candidate or accepted base. Missing live
completion is `PUBLICATION_PROOF_MISSING`; changed current inputs or custody
remain blocking. A PR observed after creation can still fail admission:
`PR_CREATED_NOT_ADMITTED` identifies its URL and confirmed draft/closed quarantine
or an explicit unconfirmed quarantine. The publisher never reports successful
publication on this path. Rejection JSON is appended to the live external
evidence directory without rewriting historical successful proof. An admitted
result's existing warnings field states that admission does not authorize merge.
The wire shape of the existing publication result is unchanged.

When creating a GitHub PR, `workspace publish` and RPC publication accept
explicit titles and descriptions. For an omitted title, the publisher reads
the exact verified base-to-head range and requires one Conventional Commit
subject among its non-merge commits. A missing or ambiguous subject produces
`PR_TITLE_REQUIRED` with `--title` guidance before PR creation. An omitted
description uses that commit's body when present or a list of branch-only
commit subjects. A matching existing PR is reused without deriving metadata.

## Escalation admissibility (#1690–#1692)

Continuations may supply an omitted authority command and narrow an omitted
target; established scope cannot be replaced or dropped. This uses the current
token and budget. A newly admitted targeted human dependency reports
`progress: human_dependency`; repeated admitted questions do not gain progress.

Orientation's existing `--reflection` input and `mentor.obligations.reflection`
accept optional `escalation`: canonical plan intent, question, missing subject,
uncertainty class, investigation, optional resolution with exact source facts,
risks, constraints and optional authority command/target. Typed escalation uses
`decapod.reflection.v2` with `decapod.escalation.v1`; ordinary reflection retains
v1. Responses expose typed reasons, source identities, progress, targeted
challenges, token and remaining rounds. `human_escalation_permitted` is distinct
from a challenge and never supplies an approval. `completion_proven` and
`semantic_exhaustion_proven` stay false. Native completion and publication still
require independent current proof. `escalation_required` on an inference
capability gap is accompanied by `escalation_target: agent`.

## Required source-CI test selection (#1672)

`assets/tools/source_ci_profile.rs::{selected,commands,inventory}` includes
the existing recipe test inputs as selection dependencies: named core runtime
fixtures, CLI aggregate inputs including `tests/sqlite_maintenance.rs`, shared
`tests/common/*` and `tests/support/*` fixtures, and Bazel declarations. The
regression `required_test_source_changes_retain_the_complete_owning_recipe`
checks selection, unchanged complete command recipes and applicable required
inventory together. Unrelated documents stay unselected, and release-only
dispositions retain their existing contract. No new shard protocol or timing
guarantee is introduced by this coverage change.

## Custody and prerequisite diagnostics (#1661)

`workspace status` reports `can_work=false` when checkout custody is unproven.
`govern project-check --format json` remains usable after `init --no-git` and
reports `scope=project_prerequisites`, `publication_ready=false`, and no readiness
result. Full `validate` rejects that same non-Git directory with
`gate=workspace_protection` / `AUTHORING_CHECKOUT_UNVERIFIED`.
Task renewal and lifecycle operations update or retire their checkout binding
atomically; a second live binding is `AUTHORING_TASK_ALREADY_BOUND`.
`resource:<name>` overlap diagnostics identify the affected shared operation,
and occupied release/integration reservations yield `SHARED_RESOURCE_OCCUPIED`.

## Optional reflection on existing interfaces

`infer orientation --task-id <id> --reflection '<JSON>'` and
`mentor.obligations`' optional `reflection` parameter accept the same typed
request: task/submission ID, operation (`change|modify|update`), literal proposed
paths, optional expected token, explanation and evidence references. Existing
calls omit the extension and preserve their response contract. Responses add
`reflection` with `decapod.reflection.v1`, disposition
(`proceed|reflect|human_decision|blocked`), reason, one optional challenge,
token, context digest, round/remaining budget, terminal/execution-blocked flags,
next action and always-false completion proof. Challenges expose a stable ID,
bounded question, trigger, source references, investigation and binding flag.
`state.resolve` decision records add optional `knowledge` provenance.
The default round limit is three; one plan supports one current local task
continuation. Source/path/input limits fail explicitly. No new RPC operation,
authority hierarchy or persistence format is introduced.
The CLI requires an explicit matching outer `--task-id` for reflection. This
keeps its legacy orientation gates and the continuation on the same governing
task intent. An omitted or conflicting binding fails before recording a round;
the RPC continues to use the task ID in its typed reflection request.
The plan source hash identifies its contract projection: reflection receipts,
timestamps, approval state and measured phase/gate progress are normalized out.
Governing definitions remain bound, and execution checks still require current
approval and proof independently of a reflection disposition.

## Cloud initialization, continuation, and readiness (#1628)

The human's sole setup command is init; subsequent use belongs to the agent.
`decapod cloud login` starts/resumes managed sign-in and checks current access;
`--restart` explicitly replaces an expired, revoked or uncertain flow. The
existing init restart flag remains compatible. `CloudAuthDiagnostic` and cloud
TODO operation diagnostics add optional `authorization_url` (omitted when
absent) without changing their v1 schema identifiers. Agents share the link,
poll for up to five minutes and continue without waiting for a second chat
message. A pending `todo add` retains its exact original operation identifier
and arguments across login; authentication never certifies task creation.

`decapod init --backend cloud`, `decapod session acquire`, and ordinary cloud
TODO operations share `core::cloud_client::connect`. Missing custody starts a
bounded sign-in and returns non-success `onboarding_pending` with a validated
browser URL and headless resume instruction. A subsequent invocation polls once,
exchanges only when ready, persists the issued session, and asks the service to
confirm account completion. Init reports ready only after an authenticated
read-only task lookup confirms current repository access.

`action_required` describes a service-reported unresolved entitlement without
inferring price, payment, membership, or seat authority. `repository_denied`
propagates explicit access denial. `restart_required` preserves uncertainty
after an interrupted one-time exchange or rotation. The explicit
`--restart-cloud-signin` flag starts a fresh flow for this binding without
replaying its old exchange. Normal data failures do not trigger unauthenticated
retry, mutation replay, or local fallback.

`decapod cloud status` reports only private local phase, credential presence,
and expiry; its `server_readiness` remains `not_checked`. `--proof` on cloud
initialization prepares configuration only and does not synthesize sign-in or
claim readiness. The agent-owned `cloud login` command uses the same managed flow.
No new project provider, deployment, credential, price, or roster field exists.
The fixture-only origin requires explicit test mode and literal loopback HTTP.

The connector currently accepts a canonical repository routing assertion, not
an existing paid project ID. Selecting an account-directory project and binding
this checkout to that server ID needs a supported upstream attachment contract;
this client does not invent one or claim that SCM ownership or paid-project
attachment has been verified. Browser directory registration remains distinct.

The client consumes the exact published `dactyl-db 0.16.1` registry API;
current-candidate fixture execution remains required for the complete composition. Pending
secrets round-trip through its public Serde contract; authentication status and
code exchange return strings, and completion confirms the session handoff.
The caller retains its exact origin/repository/checkout binding and saved flow
deadline, and an authorized repository read remains the readiness requirement.
Lost responses to one-time mutations require explicit restart; a lost idempotent
completion response keeps the issued session and can be checked again.
The release-policy test binds the registry version and actual crate checksum.
Source-overlay integration results cannot be reported as package publication,
real PostgreSQL, hosted account enforcement, or full remote governance proof.

## Agent delegation and cloud authorization interface

Decapod is invoked by each contributor's coding agent in the repository.
Local `DECAPOD_AGENT_ID` labels and `[governance.rbac]` describe cooperative
identity and exceptional local grants only. Cloud `todo` calls use a
managed-Auth bearer, canonical repository binding, and backend-owned checks.
Client role labels, task-assignee strings and `actor_roles` cannot substitute
for that bearer or select a cloud privilege. The same agent command interface
serves owners, maintainers, contributors and contractors; no contractor-specific
human command is added.

Propodus alone resolves the effective `read`, `write`, `maintainer`, or
`owner` tier and enforces expiring direct user grants (including explicit
revocation over inherited group/team/org grants). Cloud claim/release/complete
must bind the acting authenticated principal independently from the
caller-supplied agent label. The protected two-principal service proof checks
both ordinary lifecycle concurrency and label impersonation. HTTP fixtures
show client composition only; local mode never falls back from a failed cloud
authorization. Merge readiness remains governed by the independently
configured publication and proof gates.

## Version reporting and migration inspection

`decapod system version` reports the installed binary version and existing
update guidance without reading configuration, acquiring a session, initializing
a datastore or migrating project state. It works in uninitialized, damaged and
unsafe-permission checkouts. Stateful commands retain storage permission
rejection and the first normal command still reports a pending migration.

`decapod system migrations --format {json,text}` (dispatched early in
`src/decapod/lib.rs::run_system_migrations_command` via `SystemCommand::Migrations`
in `src/decapod/cli.rs`) provides read-only, non-initializing, session-free
inspection backed by `src/decapod/core/migration.rs::inspect_migration_state` and
`MigrationInspectionReport`. It resolves `workspace_root`, `governance_root`, and
the active `decapod_root` (following the main repository `.decapod` from linked
worktrees via `find_governance_root`) and exposes `version_counter.json`,
`applied.json`, the compiled and on-disk `catalog.json`, `pending_migrations`,
`pending_legacy_databases`, `action_required`, `reason_codes`, and
`instructions` without creating `.decapod/data/`, mutating storage permissions,
opening SQLite locks, or consuming pending migration notices.

`MigrationReport::agent_instruction` in `src/decapod/core/migration.rs` carries the
resolved `decapod_root`, directs agents to `decapod system migrations --format json`
instead of raw disk reads, and prints resolved `applied.json` and `catalog.json`
paths so notices stay accurate inside governed worktrees. `decapod system doctor
check` in `src/decapod/plugins/doctor.rs` (`check_decapod_dir_for_store`,
`check_migrations`) likewise executes in non-initializing mode across worktrees
and reports pending or corrupt migration artifacts.

## Publication projection

`interop.project` validation reports expose `proof_judgment` alongside
`publication_ready`. Eligibility comes from `project_report_publication`.
A report that only sets `publication_ready` or `passes` stays ineligible.
A fresh pass with a non-empty head and a receipt digest can be `proven`.
Stale rules or verifier identity, a failed status, missing canonical state,
and a corrupt status stay distinct and are not publishable.

`state commit verify` requires the scope-record bytes and an expected root.
Omitting the root is an error, not a successful verification.

`state.resolve` remains the canonical snapshot. `judge_stored_verification`
re-hashes the files named by a verification row and binds that digest to the
snapshot. The row's status field is not accepted as proof. Governed-transition
decisions are appended to the `traces` stream that `decapod flight-recorder`
already renders. When the transition is not admissible and proven, that payload
includes `blocker_class` and `required_capability`. Those fields describe the
blocker. They are not an instruction to the harness.

`judge_completion_with_verifier` is the completion judgment plus an optional
list of isolation axes. An empty list preserves `judge_completion` and ignores
the envelope. `judge_verifier_integrity` checks each required axis against that
axis's evidence reference and checks `canonical_input_hash` against the
completion `inputs_hash`. `attest_capability` keeps observed support only when
the attestation carries a usable evidence reference and a current identity.
`admit_operation_surface` answers whether an operation class may write the
given paths. None of these functions is a new RPC, and none selects work. A
missing blocker class means the cause is unclassified. It does not mean
`state`.

## Claim scope acceptance

`todo add --scope 'paths:["src/a.rs"]'` declares the files an exclusive claim
coordinates. Distinct callers can claim disjoint files in the same canonical
repository; logical source overlap is coordination metadata rather than an authoring lock. Task custody and observed writable-checkout custody remain exclusive. The real CLI
regressions check both claim acknowledgements and subsequent `todo get` data.
Shared-claim acknowledgement preserves the existing primary assignment and
adds only secondary membership. Enabling the option solely in a worktree
must not affect the canonical datastore policy.

## Recovery replay comparison

### Commit-bound completion after merge

`decapod qa verify commit <full-candidate-sha>` inspects durable full validation
history in a local canonical store without creating a datastore, acquiring a session, changing task state
or refreshing the current workspace. Its historical result always declares
`publication_ready=false`; inspection is not a new validation run.
Cloud-backed historical task lookup and completion are explicitly unsupported;
ordinary cloud validation retains its existing behavior.

`decapod todo done <task-id> --validated --commit <full-candidate-sha>` requests
completion from that task's recorded full proof. The caller must retain the
original ownership lineage, the task contract must be unchanged, and the
candidate must have verifiable integration into the configured accepted branch.
This explicit mode uses the original plan and task binding instead of requiring
a new current-workspace plan. It never starts a current-workspace validation
child. Ordinary `todo done --validated`, batch recapture and publication retain
their separate fresh-proof contracts.

If the task is already done but lacks accepted verification, the same historical
command repairs its proof atomically while preserving completion time and task
revision. Missing proof, intent edits including edits later reverted, a new
owner or lease generation, incomplete journal history, unsatisfied dependencies,
and concurrent task changes leave task and proof state unchanged. The recorded
candidate and the observed integration are not interchangeable SHAs; validation
of the candidate is never reported as execution on the merge result.

The first integration observer reads the original accepted branch through public
GitHub HTTPS and requires its immutable objects to be available locally. Private
repository integration is explicitly unsupported by this transport; status
lookup remains local. Ordinary merges and fast-forwards use candidate ancestry.
Squash/rebase recovery requires an exact complete candidate tree in at most 256
accepted first-parent commits after the original base; patch similarity and
unrelated tree matches are insufficient. Renewing or reacquiring the task with a
new lease generation requires new proof. Repeated successful executions of the
same SHA may be selected only within one task contract and custody binding;
stored completion references retain the exact original record identity.

`qa verify recover` and per-task verification use the same normalized validation
report. Fresh execution paths and receipt IDs do not constitute material drift;
changed scope, policy, artifact content, epoch, target or verdict still does.
Persisted output hashes remain SHA-256 values. Baselines captured with the old
comparison can require one explicit supported recovery to refresh them. No
command flag bypasses proof, and atomic batch finalization is unchanged.

## Exact consumer source spelling

The `agent.init` reconciliation result must contain only `Justfile` when that
is the sole tracked spelling, even when APFS resolves `justfile` to the same
inode. The Rust CLI regression checks `discovered_sources` against the Git
index and retains `proof_status=not_run` and `merge_ready=false`: initialization
does not certify native readiness. No CLI flags or response schemas change.

## Optional basic-trust secondary claims

`[governance].allow_basic_shared_claims` is false by default. When enabled in
the main repository config, local shared claims by basic agents can join an
existing different primary owner. The operation records the config fingerprint
and asserted labels atomically with the ownership event. It does not grant
verified trust, authenticate an identity, authorize primary claims or waive
other gates. This option does not apply to cloud-backed claims.

## Historical verification recovery acceptance

## Local broker caller protocol

The internal Unix-socket broker request in
`src/decapod/core/group_broker.rs` uses protocol version 3. Alongside
request ID, arguments and an absolute queue/execution deadline, each request carries a canonical absolute working
directory and an explicitly allowlisted caller context. The broker leader honors
the caller's bounded `request.deadline_ms` (capped by `BUILD_TIMEOUT`) instead of
clamping `todo claim` or `workspace status|prune` control mutations to 15s
`CONTROL_TIMEOUT`. For `todo done --validated`, slow historical/QA verification
executes outside the exclusive group-broker lock via
`todo::prepare_validated_done_handoff` before broker dispatch, handing off a
mode-`0600` request-bound token that the brokered worker re-verifies against
canonical storage (`verify::consume_prepared_done_handoff`) before the short
transactional status commit. Agent identity, session,
task, correlation, causation, idempotency and claim-autorun inputs replace—not
merge with—the elected leader's corresponding environment. The child executable
is resolved independently of either working directory.

This internal protocol is not a new public task ownership or authorization API.
The ordinary CLI/session/policy checks still execute; request fields cannot grant
trust or approval. Missing or unrecognized caller fields fail validation rather
than inheriting the leader identity. The existing restricted local socket alone
transports the ephemeral session password; durable dedupe stores no credential.
Structured replay binding includes arguments, actor, working directory and
non-secret context, but excludes password rotation. Protocol mismatch fails
closed; no persistent daemon or alternate IPC transport is introduced.

The regression in `tests/group_broker.rs` requires two concurrent claim callers
to traverse the same elected leader and checks both returned and stored owners.
Unit coverage in `tests/unit/core/group_broker_tests.rs` checks replay separation,
rejected arbitrary environment keys and removal of absent leader credentials.
Current full publication proof remains required; these declarations are not a
substitute for the controller's authenticated test result.

## Native readiness completion artifacts

## Interface Overview

Humans supply intent, review results and authorize merge/release. Agents perform
work and invoke Decapod through finite CLI/RPC operations at planning, custody,
validation and publication boundaries. Repository state preserves continuity.
The public Buildkite reader admits only its source-bound artifact routes and
verified producer/content identities; diagnostics do not create execution proof.

Optional Laya input uses `validation-results.v3` through the shared production
projector. Intent, proof hooks, unresolved questions, contradictions, stop
conditions, phase gates and actual observations remain distinct. The complete
input must fit its existing 32 KiB and declared model bounds; inputs are never
selectively clipped to obtain an approval. Private replay validates deterministic
evidence before using that same builder but establishes no live custody.

`readiness_assessment::permits_decision` retains failed-proof dominance and
requires an affirmative, nonabstaining decision in required mode. Live callers
also establish freshness, accepted policy, runtime identity and existing human
authority. `ValidationReport` contains only the optional `laya_readiness` model
field; inactive evaluator aliases and compatibility DTOs are not public API.

## Reviewed native sequence interface

`native_proof::Invocation.following` is an ordered vector of explicit native
programs, arguments, working directories and environments, not a deserializable
command authority. It is omitted from serialization when empty, preserving
existing single-command invocation digests. The complete check is bounded to
64 commands. Every successor undergoes the same executable, argument, environment,
platform and recursion checks as the first command. Artifacts are verified only
after all commands succeed in their shared owned scratch directory.
`CheckDiagnostic.steps` retains each attempted step's exit/termination, duration,
output hashes and preparation/spawn failure. A changed later tool preserves
earlier execution diagnostics while making the check fail. These observations
remain execution evidence; they neither select the required graph nor override
missing platform or policy obligations.

`toolchain_digest_bounded` accepts the caller's remaining time and cancellation
while retaining the original 15-second/1-GiB measurement ceiling. Repository
proof seals retain the originating deadline and cancellation token; expired or
cancelled seals cannot authorize another measurement. The CLI subtracts native
discovery time before entering the readiness runtime. Declared invocation limits
remain stable across final re-selection; execution still uses the lesser of
that limit and the original remaining budget.

## Human and agent entrypoints

The separate scientific worker validates its own ephemeral nonce format locally (sha256: plus64lowercase hexadecimal digits). It does not call or expose the main crate's private model-installation digest helper. Candidate4cdcd25d native compilation rejected that cross-crate call with E0603 before any new test or model execution; repair preserves the abstraction boundary and leaves all receipt/acceptance checks intact. A canonical nonce detects malformed bookkeeping, not authenticated global ownership.

### CI-attested execution declaration

`proof.ci_attested.workflows` maps each supported workflow's native step keys to
the compiled required checks. Its optional `pipeline_trigger` declaration names
the finite `buildkite-gha-0.102.7` contract, exact workflow name and, when present,
the supported authored concurrency expression. The configured executor is trusted
to supply one keyless importer; metadata verifies its terminal success and actual
job UUID, not the plugin implementation. Derived concurrency gates and the reserved
bootstrap evidence identity are distinct from required logical validation checks.
Collection reads only build, complete steps and complete jobs metadata. It rejects
failed, retried, missing, duplicate or unexpected jobs and mismatched source/run
identity. Optional configured workflows are collected only when the compiled
catalog requires them. Exact accepted-target policy equality remains mandatory.

The activation PR uses an explicitly approved one-time direct-check and exact-CI
bootstrap review. It cannot report released-controller publication readiness for
its own new policy. Subsequent changes use the accepted policy and released
controller through ordinary validation.

Human setup is `cargo binstall decapod && decapod init` in the project, followed
by a task given to the person's coding agent. The README keeps this path short.
Operational commands belong in the agent contract and agent documentation.

The human-facing README is a brief product overview: its existing visual layout
and section order make capabilities easy to scan, while the introduction
explains agent-owned engineering work. It distinguishes parallel owned tasks
from continuing one pull request in another tool. Validation stays in prose as
an agent responsibility; its optional remote execution applies to that phase,
not to Decapod as a whole. The README does not add operating instructions or
historical proof reports to the single-command setup path.

`CONTRIBUTING.md` gives human contributors the review expectations and project
layout. The agent contract and linked technical references carry the operational
workflow. Contribution guidance assigns build orchestration, verification and
repair to the agent, without requiring a human to expand credential permissions
or run a manual prerequisite suite.

Agents discover the installed interface with `capabilities`, ingest its command
contracts with `docs ingest`, and retrieve engineering guidance with
`constitution get` or constitution search. `docs` serves `docs/agent/`; the
Constitution supplies broader engineering doctrine. Version, capabilities,
Constitution lookup, and embedded documentation remain available when project
state needs recovery. Documentation ingestion can update an existing local
awareness marker; it is not an implementation or completion proof.

The project-check machine-contract gate validates the serialized capability
names and interlock codes from the declarations shared with the CLI. The
gate does not depend on container or registry availability. The public
`capabilities` command includes these runtime observations.

`eval --stdin --format json` accepts the complete incoming prompt. The agent
proceeds only after exit 0 and `status: allow`. A blocked result or nonzero exit
requires human review. This gate applies at startup and to each new prompt.

## Inbound Contracts

The CLI is the operational surface. `decapod rpc` provides Decapod-specific
JSON request and response envelopes over process stdin/stdout. The binary does
not currently provide an MCP server, MCP lifecycle, or MCP discovery. Future
adapters are separate contracts.

An RPC request contains `op`, `params`, a correlation `id`, and an optional
`session`; omitted parameters default to JSON null and omitted IDs are generated. The response includes `id`, `success`, a `receipt`, and
`allowed_next_ops`. Depending on the operation it also carries `result`,
`mandates`, `context_capsule`, `blocked_by`, `interlock`, `advisory`,
`attestation`, or `error`. A receipt records the operation, time, input/output
hashes, touched paths, and governing anchors. An operation receipt establishes
what that call recorded, not the correctness of the whole task.

## Failure Semantics

Blockers distinguish `missing_answer`, `missing_proof`, `unauthorized`,
`conflict`, `validation_failed`, `workspace_required`, and `protected_branch`.
Errors carry a code, message, and optional details. Callers inspect these fields
rather than infer domain error kinds from exit status:

- `0`: the requested operation succeeded within its reported scope
- `1`: a domain, validation, configuration, session, I/O, or storage failure
- `2`: CLI syntax rejected before execution
- `127`: the calling shell could not locate a command; this is not a Decapod
  domain error

The CLI, aliases, and RPC share their underlying execution gates. In particular,
a failed `validate.run` retains its structured response and returns nonzero.
Agents use the installed command contracts and schemas rather than inventing
operations or treating a listed next operation as approval to bypass its gates.

## Intent, authority, and context

The prompt, conversation, and explicit human clarifications establish request
meaning. An isolated workspace supplies the execution filesystem; a todo
supplies ownership and coordination. Entering a workspace does not erase the
agent's request context.

`infer orientation` exposes the goal, task binding, governed plan, allowed and
forbidden scope, relevant areas, proof requirements, known unknowns, decision
gates, and next action. `context.resolve`, capsule queries, and context bundles
carry selected project specifications and Constitution material with their
sources. A retrieved recommendation or advisory observation is not permission
to cross an execution boundary.

Project authority comes from `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
Each registered override directive owns a fenced source body. Resolution
preserves nested Markdown and examples, removes the outer wrapper, and exposes
`directive_id`, `source`, `source_hash`, `body_hash`, `byte_count`, and
`precedence`. Duplicate exact directives, unclosed wrappers, and non-empty
unknown Decapod-namespaced directives invalidate the whole overlay. Empty
retired generated directives remain upgrade-compatible. Partial application
must not conceal ambiguous authority.

Project authority comes from `.decapod/config.toml` and `.decapod/OVERRIDE.md`.
Each registered override directive owns a fenced source body. Resolution
preserves nested Markdown and examples, removes the outer wrapper, and exposes
`directive_id`, `source`, `source_hash`, `body_hash`, `byte_count`, and
`precedence`. Duplicate exact directives, unclosed wrappers, and non-empty
unknown Decapod-namespaced directives invalidate the whole overlay. Empty
retired generated directives remain upgrade-compatible. Partial application
must not conceal ambiguous authority.

## Validation Rules System Interface

Decapod exposes the validation rules system through CLI, RPC, and configuration
interfaces.

### Configuration Interface

The validation rules system is configured through `.decapod/OVERRIDE.md` under a
dedicated `

## VALIDATION_RULES Overrides section. Each rule override uses a
fenced block with the rule ID as heading:

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

Supported override fields:
- `severity`: `advisory`, `warning`, `error`, `critical`
- `applicability`: `required_changed_paths`, `forbidden_changed_paths`, `task_types`, `ref_patterns`, `ci_only`, `local_only`
- `evidence_max_age_seconds`: Maximum age of evidence in seconds (0 = no limit)
- `requirements`: `required_evidence`, `required_evaluators`, `required_context`
- `remediation`: Override the remediation contract

Override eligibility is enforced per-rule:
- `NonOverridable`: Cannot be overridden (kernel invariants)
- `TightenOnly`: Can only increase severity or add constraints
- `ConfigurableOnly`: Can configure parameters but not disable
- `FullyOverridable`: Full override permitted

Invalid overrides (unknown rule IDs, duplicate definitions, malformed content,
unauthorized weakening of kernel invariants) cause validation to fail closed.

### RPC Interface

The validation rules system exposes the following RPC operations:

#### `validation.rules.list`
List all registered validation rules with their effective configuration.

**Request:**
```json
{"op": "validation.rules.list", "params": {}}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rules": [
      {
        "id": "decapod.scope.authorized_paths",
        "description": "All changed files must be within authorized scope...",
        "authority": "constitution",
        "severity": "error",
        "evaluator": "decapod.scope.validator",
        "effective_severity": "error",
        "override_eligibility": "tighten_only",
        "applicability": {...},
        "requirements": {...},
        "remediation": {...},
        "provenance": {...}
      }
    ]
  }
}
```

#### `validation.rules.evaluate`
Evaluate all applicable rules against the current context.

**Request:**
```json
{
  "op": "validation.rules.evaluate",
  "params": {
    "context": {
      "changed_paths": ["src/main.rs", "tests/test.rs"],
      "task_type": "feature",
      "git_ref": "refs/heads/feature/xyz",
      "is_ci": false,
      "evidence_artifacts": {
        "governed_plan": "sha256:...",
        "workspace_status": "sha256:..."
      },
      "validation_epoch": {...},
      "policy_fingerprint": "sha256:..."
    }
  }
}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rules_report": {
      "results": [
        {
          "rule_id": "decapod.scope.authorized_paths",
          "verdict": "pass",
          "severity": "error",
          "evaluated_at": "2026-11-05T18:23:31Z",
          "evidence_refs": ["governed_plan", "workspace_status"],
          "message": "Rule passed",
          "remediation": null,
          "policy_fingerprint": "sha256:...",
          "validation_epoch": {...}
        }
      ],
      "passed": 7,
      "failed": 0,
      "unverifiable": 0,
      "overall_verdict": "pass",
      "has_blocking_failures": false,
      "has_unverifiable_mandatory": false
    }
  }
}
```

#### `validation.rules.get`
Get a specific rule by ID with its effective configuration.

**Request:**
```json
{"op": "validation.rules.get", "params": {"rule_id": "decapod.scope.authorized_paths"}}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "rule": {...},
    "override_values": null,
    "effective_severity": "error",
    "effective_applicability": {...}
  }
}
```

### CLI Interface

The validation rules are evaluated as part of `decapod validate`:

```bash
decapod validate --format json
```

The validation output includes a `rules_report` section in the JSON output:

```json
{
  "status": "failed",
  "rules_report": {
    "results": [...],
    "passed": 6,
    "failed": 1,
    "unverifiable": 0,
    "overall_verdict": "fail",
    "has_blocking_failures": true,
    "has_unverifiable_mandatory": false
  }
}
```

Verbose output (`-v`) includes per-rule details with remediation contracts.

### OVERRIDE.md Integration

Project-specific rule overrides are declared in `.decapod/OVERRIDE.md` under a
dedicated `

## VALIDATION_RULES Overrides section. Each rule override uses a
fenced block with the rule ID as heading:

```markdown

## VALIDATION_RULES Overrides

### decapod.scope.authorized_paths
````markdown
severity: error
applicability:
  required_changed_paths: ["src/**"]
  forbidden_changed_paths: ["**/secrets/**"]
evidence_max_age_seconds: 43200
````
```

The OVERRIDE.md parser validates:
- Known rule IDs only (unknown IDs fail closed)
- No duplicate rule definitions
- Well-formed TOML within fenced blocks
- Override eligibility respected (NonOverridable, TightenOnly enforced)

### Applicability and Context Awareness

Rules declare applicability conditions. The registry only evaluates applicable
rules based on:
- Changed paths (required/forbidden patterns)
- Task type (feature, bugfix, refactor, docs, test)
- Git ref patterns
- CI vs local execution context

Custody rules (`task_ownership`, `workspace_isolation`) only apply to governed
agent workspaces with active task claims. They do not apply to:
- Detached/post-merge validation contexts
- Consumer repository validation
- Release/post-merge validation contexts
- Fixture/test validation contexts

### Remediation Contract

Every blocking rule result (FAIL or mandatory UNVERIFIABLE) includes a structured
remediation contract:

```json
{
  "error_code": "SCOPE_VIOLATION",
  "violation_description": "Changed files exceed authorized scope",
  "expected_condition": "All changed files within declared scope",
  "observed_condition": "Files outside authorized scope modified",
  "affected_paths": ["src/secret.rs"],
  "required_steps": [
    "Revert changes to files outside authorized scope",
    "Request human approval for scope expansion"
  ],
  "autonomous_remediation_permitted": true,
  "human_authorization_required": false,
  "evidence_for_revalidation": ["governed_plan"],
  "related_rules": ["decapod.scope.file_touch_budget"]
}
```

Agents must follow the remediation contract and revalidate. Kernel invariants
cannot be waived by candidate-controlled policy changes.

## Current-evidence recovery for historical completion claims

`plugins::verify::VerifyCommand::Recover` accepts repeated `--artifact TODO_ID=FILE`
inputs when an unverified completed task lacks a usable file baseline. Inputs
must name tasks in the recovery batch and readable current files; no implicit
AGENTS.md substitute or historical CI receipt is imported by this batch path.
`prepare_recovery_captures` collects file hashes and the active validation epoch
before validation, rejecting missing per-task inputs and additional proof
obligations that cannot be established by the validation-only capture contract.
Relative artifact paths use the same task-aware lookup during capture and replay:
the invoking checkout takes precedence, then the owning task workspace is
consulted. Replay still requires the stored content hash to match; a missing or
changed artifact is not accepted as proof.

`refresh_batch_baselines_with_captures` requires a genuinely passing current
snapshot, checks the original task and proof projection under the write
transaction, and rechecks captured file bytes and epoch before inserting pending
proof. It preserves completed task status, timestamp and revision, and records
previous proof fields plus the explicit fresh-current-material provenance in the
append-only recovery event. `finalize_recovered_batch` remains the sole batch
promotion boundary after final validation and every task's artifact checks.
A pending or failed capture is not verified completion; a newer task, baseline,
file or epoch invalidates the attempt. Existing proof plans containing additional
obligations are not downgraded by supplying new files.

The required regression surface in `tests/unit/plugins/verify_completion_tests.rs`
includes multiple completed tasks without proof rows, missing/foreign artifact
inputs, extra proof obligations, failed snapshots, changed files, concurrent task
revision changes, pending final failure, retained completion history and atomic
successful promotion. These are authored tests until exact-candidate validation
executes them; source review alone is not passing proof.

The candidate-binary regression in `tests/verify_deadlock_regression.rs` also
invokes the real recovery CLI for two completed tasks lacking proof rows. With
explicit current artifacts, an independently unapproved plan still fails actual
child validation; recovery must preserve task status, completion timestamp and
revision without creating evidence or pruning either task. This failure-path
coverage does not establish successful full-native CLI recovery or publication
readiness. The Tokio lock repair changes only the runtime package version and
checksum, not this interface or its evidence-admission obligations.

Recovery guidance is an agent/harness error contract, not a human command
checklist. Epoch drift and aggregate missing-proof errors direct the agent to
non-reopening batch recovery with explicitly selected task-bound current files.
A failed scoped child validation emits `VALIDATION_CHILD_FAILED` via
`src/decapod/plugins/verify.rs` (`extract_safe_validation_diagnostic` and
`SafeValidationDiagnostic`). Rather than relying solely on a 1 MiB raw-byte
substring scan, `extract_from_validation_json` inspects structured JSON child
reports up to 16 MiB (`gate`, `readiness`, `report`, and `scope`) to surface
bounded canonical `stage` and `scope` fields plus compile-time allowlisted
labels from `SAFE_BLOCKER_LABELS`, unwrapping nested `READINESS_BLOCKED` /
`Cannot publish:` / `AUTOREMEDIABLE_VALIDATION_ERROR` / `Rule` wrappers via
`extract_labels_from_structured_text` while prioritizing specific blocker codes
over generic `READINESS_BLOCKED` within the `< 1024`-byte ceiling. Unstructured
output and `stderr` fall back to token-boundary matching capped at 1 MiB per
stream so prefix collisions (such as `LAYA_READINESS_BLOCKED`) and untrusted
`notes` or arbitrary message suffixes never emit spurious labels. When `--json`
is passed to `qa verify recover` (`run_recover_cli`) or `qa verify regen`
(`VerifyCommand::Regen`), `emit_recovery_failure_json` prints the structured
error payload (`phase`, `stage`, `scope`, `labels`, `output_hash`, and bounded
`diagnostic`) before returning the existing fail-closed error, noting that
unscoped `decapod validate --format json` may stop earlier at
`PROOF_HOOK_FAILED`. Unknown failures retain the supported
`decapod validate --format json` diagnostic and original-command retry route. Raw
child messages, paths, URLs, environment values, task titles and credentials are
never forwarded by this diagnostic. Labels and JSON error fields are diagnostic
observations only, never acceptance, approval, or proof. Initial/final recovery
and `regen` failures preserve historical/unverified or pending states without
writing `task_verification` passes or altering task revisions.
Humans may initialize Decapod; execution and recovery instructions target agents.

## Sessions, tasks, and workspaces

Most mutations require an active repository-local session. Local session
credentials establish custody and correlation; they do not authenticate a model
provider or human principal. Cloud authentication credentials are separate,
machine-local material. Session records and credentials must not become tracked
project content.

Todos have durable IDs, lifecycle state, and explicit ownership. Exclusive
claim mode takes the assignment; shared mode joins as a secondary owner. Claim,
yield, handoff, heartbeat, and completion use governed operations. Conditional
status changes distinguish success, not-found, and conflict, and emit their event
only when the state transition wins. External trackers may remain the
organizational system of record; they do not replace Decapod execution claims.

Captured task verification artifacts include `task_id` and `task_revision`.
A passing legacy record without this binding, a malformed binding, or a different
task/revision projects `validation_status: stale`; a done task then projects
`status: blocked`. Dependency admission and completion reject that evidence.
Proof references remain historical references rather than renewed permission.
`qa diagnose` recommends `decapod qa verify regen <id>` for stale binding;
`qa verify --stale` includes revision drift as well as age. The `--stale` flag
is global and applies to every `qa verify` subcommand, including
`qa verify recover --stale`: it lists stale done tasks (with
`last_verified_at` and `verification_policy_days`) and returns without
running any child validation (#1527). Closing a task via `todo done` without
`--validated` records `verification_status: "CLAIMED"` and actionable `remediation`
pointing to `todo done <id> --validated --commit <sha>` (when a recorded candidate
exists via `validation_history::latest_candidate_for_task`), `qa verify regen <id>`,
or `qa verify recover --artifact <id>=<file>`. `PROOF_HOOK_FAILED` and
`verify_target` surface the same self-service recovery paths, and `qa verify regen <id>`
enters `VerifyingTodosGuard` around its baseline capture so regenerating proof for
an unverified `done` task cannot self-deadlock on its own `CLAIMED` entry. Explicit
recapture runs real validation before binding new evidence. Batch recovery first
leaves its refreshed evidence `CLAIMED` and requires per-task and aggregate proof.

`STALE_TASK_VERIFICATION` reports a task or verification baseline changed while
verification was running. The stale result is discarded without replacing the
newer evidence. Inspect the current task and rerun verification or recapture.
Strict revision binding also invalidates proof after custody changes such as
release or owner edits; comments alone do not change task revision.

`workspace ensure` requires a claimed task and creates or reuses a matching
isolated workspace. It starts from committed source, preserves dirty root files
and the root index, and never imports uncommitted root changes implicitly.
Existing destinations require verified ownership, Git registration, and branch
identity. Unknown or symlinked destinations fail with preservation guidance.
`root_isolation` metadata describes this separation; it is not permission to
copy root material. `workspace status` is bounded observation and does not run
validation or refresh specifications.

A committed claim is acknowledged before optional container preparation. The
preparation runs outside broker/datastore locks; its failure is a warning on the
successful claim, not a rollback of ownership. A durable request-bound receipt
prevents automatic duplicate launches. `DECAPOD_CLAIM_AUTORUN=0` disables this
optional preparation. Interrupted preparation requires supported workspace
recovery before an explicit retry.

`TODO_PROJECTION_MISSING` means the selected control plane lacks the expected
coordination projection. It reports `kind: coordination_projection_missing` and
`execution_ready: false`; it is distinct from missing human intent and from a
storage read failure. Preserve the request, inspect the original claim and
workspace through Decapod, resolve ownership or store selection, and rerun the
execution gate. Never manufacture a duplicate task or copy a database to hide
this discrepancy.

The explicit local transfer commands preserve an original standalone task:
`todo transfer export --id <id> --to <receiver> --output <absolute-private-file>`
requires an actual yield by the source actor and seals source mutation.
`transfer recover --id <id> --transfer-id <id> --output <new-private-file>`
recovers identical bytes after interrupted publication. Recovery selects the
immutable export event by task and transfer identity instead of recapturing the
entire subject journal. The canonical event read enforces its existing 8 MiB
record bound before payload materialization, and recovery revalidates the
repository, exporter, task, transfer, envelope and artifact digest bindings.
Successful exports at the 4,095/4,096-event boundary remain recoverable after
the export/checkpoint append and after later comments. This does not relax the
4 MiB artifact or 4,096-event historical-bundle limits.
`transfer import --input <absolute-private-file> --expected-digest <sha256:digest>
--accept-source-transfer` requires a trusted source handoff, the same GitHub
origin and intended receiver, fetched source commit and absent local authority.
The `decapod.task-transfer.v1` artifact is bounded to 4 MiB and 4,096 subject
events. Sessions and unrelated tasks are excluded. Export supports only yielded
tasks without shared owners, parent/dependency edges or incoming relationships.
Import returns `execution_ready: false`; it neither claims the task nor imports
current-machine proof. `TODO_PROJECTION_MISSING` remains blocking until the real
import, claim and applicable execution gates succeed.
The source seal assumes all cooperating writers use a supporting controller.
An older executable, privileged database writer or disconnected copy can ignore
that fence; the artifact does not establish an online exclusive-ownership lease.

Cleanup requires ownership evidence. Task-like names, old timestamps, and an
empty inventory from a different container engine do not establish permission
to delete. Managed invocation receipts bind the exact path, directory generation,
lease, and creating runtime; uncertain resources are preserved for review.
Workspace/control subprocesses have a 15-second deadline and image builds a
600-second deadline, with bounded output and owned-child termination/reaping.
A timeout does not prove that a task, branch, or container is absent.

`container run` reports `workspace_cleanup` in successful summaries: `removed`,
`preserved_by_request`, `preserved_unsynced_branch`, `preserved_unexpected_head`,
`preserved_hidden_index`, `preserved_unreturned_refs`, or
`preserved_dirty_workspace`. Failed executions retain their failure status and
workspace path; runtime termination retains files after invocation-owned runtime
cleanup. `--keep-worktree` adds an unconditional retention request. Git inspection
errors report `container_workspace_status_unavailable`; finalization failures
report `container_workspace_finalization_failed` with the underlying diagnostic
and retained path. Foldback rejects divergent host history without forcing it.

`--push` publishes the returned branch only after successful execution. `--pr`
uses the common governed workspace publisher on the clone, including full
readiness and verified draft-PR creation/reuse; its result appears as
`publication` in the summary. Combining `--push --pr` does not cause a separate
unvalidated push. Publication precedes final foldback because the publisher may
commit checkpointed material. Publication failure retains the clone; repair the
reported gate, then run `decapod validate` and `decapod workspace publish` there.
A failed command never invokes either publication action.

`--local-only` cannot combine with `--push` or `--pr`; the shared boundary rejects
that conflict before starting a runtime or creating a clone. Omit the publication
flags to retain local work, or omit `--local-only` to return the actual candidate.
An explicit `--pr-base` differing from the configured governed publication target
returns `container_publication_base_mismatch`. Align the intended configuration,
review the accepted target and revalidate from the retained workspace rather
than silently publishing against another base.

## Governance and living specifications

`.decapod/governance.json` is the sole tracked governance document. Its plan,
trajectory, claims, validation, and optional decision-provider sections share
normalized intent and epoch state. Agents update them through CLI/RPC in
work-sized batches: intent and scope, claims and proof hooks, work and evidence,
then validation and checkpoints. Unchanged semantic updates are no-ops.

Fresh governed branches receive a task-bound DRAFT plan without inherited
completion claims or validation receipts. Repeated initialization and branch
renames preserve the active journal. Explicit `begin-pr` verifies acceptance of
the previous work before resetting it; unarchived work, ambiguous task/base
identity, and conflicting authorities block replacement. Unresolved obligations
survive until explicit evidence-bound resolution. Accepted history stays in Git.

A current-PR claim has a statement, falsifier, status, and proof references.
Health Engine runtime claims remain a separate datastore surface. A trajectory
records intent, boundaries, assumptions, inspected/modified files, commands,
checks, and typed loop attempts. Verification passes require non-empty proof
references; failed verification requires feedback. Attempts start at 1 and are
contiguous. Event or improvement records do not apply their mutation proposals.

`govern artifacts checkpoint` binds staged material and semantic inputs before
a commit. `verify-checkpoints` checks the immutable range, including merges and
the exact head; `--target-base` can identify the accepted target separately.
An existing checkpoint ID is immutable within the active PR. The checkpoint
mutation returns `GOVERNANCE_CHECKPOINT_ID_CONFLICT` without saving if its summary,
normalized proof references, or current material digest differ. An exact retry
returns the current document without a write, including after committing the
original checkpoint. Changed material requires a new ID; historical verification
remains an independent publication gate.
Checkpoints do not substitute for tests, human decisions, or missing earlier
checkpoints. `review-spec` binds contract review to current code and authored
INTERFACES, ARCHITECTURE, and SECURITY hashes. `requires-decision` remains open
until the explicit resolution surface records a human decision reference and
rationale; that record does not authenticate the human or prove semantic truth.

Agents directly author semantic prose in `.decapod/managed/specs/*.md` inside
their isolated workspace. Generated attestations, capability blocks, overlays,
fingerprints, and manifests remain CLI-owned, as does all other machine-owned
control-plane state. Supported refresh is `rpc --op specs.refresh` or
`validate --refresh-specs`; there is no top-level `specs.refresh` command.
Refresh preserves authored content and refuses `SPEC_REFRESH_AUTHORED_CONTENT_LOSS`
before writing any specification or manifest. A material authored change remains
required where the publication profile requires living-spec proof.

## Data Ownership

Application persistence crosses the Dactyl-backed facade. Decapod owns domain
schemas, conditional mutations, event identities, migrations, and policy;
Dactyl owns physical execution, access mode, typed results, and maintenance.
No raw SQLite connection or alternate driver is an application interface.
Read-only callers do not perform schema writes or migration repair.

The local backend uses the canonical `.decapod/data/decapod.db`. Shared workspace
lookup resolves its owning control plane rather than creating a divergent task
store. Local physical connection factories hold shared lifecycle guards;
maintenance takes exclusive access, and contention reports `STORAGE_LOCK_TIMEOUT`.
An attempted shared-to-exclusive upgrade reports `STORAGE_LOCK_UPGRADE_BLOCKED`.
Locks are descriptor-owned and
released by the operating system, never deleted based on age. This coordinates
cooperating Decapod processes, not arbitrary external SQLite clients.

State and its canonical event commit together at the local transaction boundary.
Event replay with the same identity and content is idempotent; divergent content
fails with `EVENT_ID_CONFLICT`. Canonical streams and projections live in the
datastore. Legacy JSONL and split databases are migration inputs, not parallel
runtime authority. Migration is lossless, bounded, and idempotent; conflicting or
malformed evidence blocks rather than being silently discarded.

Runtime continuity is exposed through the existing domain commands. Task journal
commands require the local backend; cloud selection fails closed without a local
fallback. Workunit mutations require an initialized local runtime; missing or
partial policy tables fail before journal mutation. Verification returns a nonzero
exit status for projection conflict:

- `todo journal checkpoint` captures an explicit current-state baseline;
  `todo journal verify` checks the journal without restoring state;
  `todo journal restore --expected-current-digest <digest>` requires the observed
  live projection precondition and refuses divergent authority
- `todo journal checkpoint --accept-historical-gap <task-id> --accept-historical-gap <other-id> --reason "<reviewed rationale>"` accepts multiple reviewed historical uncheckpointed suffixes (#1534, #1581). Every selected task must have a checkpoint head and a real suffix. Duplicate IDs, unrelated gaps and unrelated divergent projections block the operation before mutation. One broker transaction records a `task.journal.gap_accepted` event and a new transition checkpoint for every accepted task; on failure nothing commits. Historical events remain intact; the reviewed reason and `todo.journal.repair` permission in the main repository's `[governance.rbac]` are mandatory.
- `validate --bypass-validation --bypass-reason "<reviewed exception>"` requires a separate `validate.bypass` role, records `validation.bypassed` with `proof_ready=false` and `publication_ready=false`, and never satisfies QA replay, CI, or publication. Ordinary `validate` still executes mandatory checks. `[laya] mode = "disabled"` is the default, with required mode acting only after deterministic proof.
- `govern workunit checkpoint --task-id <id> --operation-id <id>` records a
  versioned checkpoint; optional `--expected-generation` fences a known head
- `govern workunit journal-status --task-id <id>` reports integrity and projection
  consistency; `recover` restores exact journaled manifest bytes
- `govern workunit resume --task-id <id> --operation-id <id>` verifies current
  execution authority before recording continuation; it does not run work or
  reissue proof, and accepts the same generation precondition as checkpoint
- `govern trajectory checkpoint --run-id <id> --checkpoint-id <id>` captures
  current source, task custody and policy, with repeated `--approval-id` references
- `govern trajectory verify-resume --run-id <id> --checkpoint-id <id>` checks
  continuation without changing the recorded checkpoint or granting approvals

Operation identifiers bind immutable requests. Identical replay is idempotent;
a changed request must use a new identifier and a valid predecessor. Missing,
unsupported, corrupt or contradictory history is a typed blocker. A migration
baseline states that earlier history is incomplete. Neither projection recovery
nor a baseline grants a lease, approval, successful check, or current proof.

`data db verify` reports healthy, unavailable, locked, corrupt, or unsupported
integrity results without repairing them. Backup produces a verified standalone
snapshot. Recovery is an explicit operator-selected archive-and-reload action
requiring writer quiescence. `data broker verify` checks audit lifecycle, not
physical database integrity. Orphan audit repair previews the exact event,
requires a stopped writer and a reason, rejects recent/future/completed targets,
and appends an idempotent abandonment acknowledgment while retaining
`original_outcome: unknown`. It neither retries the original mutation nor proves
the database healthy.

## Validation, publication, and completion

`validate` outside CI is the complete current-workbranch readiness boundary.
It binds the exact candidate, accepted target, source and governance material,
reviewed workflow definitions, toolchain, and complete applicable check matrix.
`govern project-check` is a bounded prerequisite check and cannot establish PR
readiness. Under `GITHUB_ACTIONS=true` or `BUILDKITE=true`, public validation runs
bounded local gates, reports `scope=ci_local` and `publication_ready=false`, and
never submits or waits for its own preflight.

Readiness reports expose `agent_guidance` for the validation phase: agent
ownership, the plain validation entry command, the next action, and the
conditions that require human input. Reconciliation repair tasks expose `owner`
and `resume_command`. These fields steer the agent; they do not change the
readiness verdict, grant authorization, or replace execution evidence. Missing
build/test configuration and unsupported adapters remain explicit blockers.

For supported remote execution, validation captures an accepted executor R
independently from candidate source H. Versioned preparation and execution
custody bind R’s revision, tree and profile closure separately from H’s actual
execution revision and tree. The required graph and final verdict remain in the
installed controller. Authenticated artifact-only workers inherit the measured
R executor through their verified preparation dependency.

`CONTROLLER_CONTRACT_UNSUPPORTED` distinguishes a reviewed inventory from a
carrier, protocol or accepted executor the running controller cannot admit. Its
`next_action` is `obtain_supported_controller`, and readiness remains false.
Repeating validation with that controller, changing the declaration or building
manually cannot repair this incompatibility. The current runtime does not
bootstrap a replacement controller or accept a candidate's own readiness claim.

Without either remote preflight flag, strict source preflight selects native
execution (`ProjectProofConfig::uses_remote_preflight() == false`).
`proof.validation_buildkite_preflights=true` selects Buildkite, and
`proof.validation_github_actions_preflights=true` selects authenticated GitHub
Actions execution when that strict carrier is independently accepted on the
target (`ProofConfiguration::verify_source_ci_authority`). Setting both remote
flags together, combining GitHub Actions preflight with `ci_attested` or
`repo.pr-readiness.v1`, missing accepted dual-root support
(`GITHUB_SOURCE_CI_EXECUTOR_UNAVAILABLE`), or candidate-selected carrier drift
(`GITHUB_SOURCE_CI_POLICY_NOT_ACCEPTED`, `SOURCE_CI_PROVIDER_NOT_ACCEPTED`) fails
closed. GitHub Actions collection uses configured official GitHub access and
binds workflow path, logical job, matrix, run, attempt, and archive identity
before admitting measured execution; `init_reconciliation::inspect` and
`intent_reconcile::selected::admission` treat both remote flags uniformly via
`uses_remote_preflight()`. Unknown profiles, unavailable adapters, and unreconciled
source contracts fail explicitly.
The remote route pushes or reuses the exact existing workspace branch, executes
its project checks remotely, and verifies authenticated build, attempt, artifact,
and final source/target identities. It does not run duplicate local prerequisites,
create a temporary branch, or treat GitHub Actions results as Buildkite proof.
Configured validation does not ask the human for a separate local approval to
push that checked work branch. The bounded validation push authorization does
not grant permission for unrelated Git writes, merges, releases or deployments.
Existing authorized provider access is required; denial does not authorize
credential installation or fallback.

Snapshot preparation/resume is a private evidence handoff on that same branch.
Preparation remains non-ready. Resume reconstructs source/tree/target identity
and rechecks actual provider evidence. Adoption cannot assert a different source
commit into validity; a changed host object identity requires checkpointed source
and a fresh handoff. Publishing rejects adoption arguments. The transport does
not claim compatibility with a separate `bk preflight` command.

Branch pushes may precede readiness. PR creation, including draft creation, waits
for converged intent, mergeability, and full exact-candidate proof. Material,
history, target, policy, workflow, or toolchain changes invalidate earlier proof.
`workspace publish` independently enforces this boundary, performs a normal
non-forced push, and verifies remote head/base and the complete PR material.
It reports `remote_verified`, `pr_verified`, and warnings separately. Confirmed
push with incomplete PR verification returns `PUBLICATION_INCOMPLETE`; an
ambiguous push acknowledgment remains an unknown outcome. Reconcile and retry
idempotently without destructive rollback.

Workunit states are `DRAFT`, `EXECUTING`, `CLAIMED`, and `VERIFIED`. A `VERIFIED`
workunit requires passed proof-plan gates. Verified completion binds task, intent,
specifications, proof plan/results,
context capsule, repository identity, and validation epoch. `passed`, `failed`,
`partial`, `unavailable`, and `no_checks_run` remain distinct. A claim of completion
is not evidence, and no checks produce an unsupported verdict. Portable evidence
retains source identity and artifact digests; the receiving repository makes its
own acceptance decision.

The CI context action exposes a required boolean `no_change` disposition. It is
true only after an explicit created push with an exact zero before value names
an ordinary branch whose head equals the captured accepted target. Missing or
malformed event data, unavailable base history, dirty or hidden source, and
unrelated heads fail verification. Consumers must not infer this disposition
from a release-looking branch name or equal caller-supplied IDs. Constitution
schema validation runs for both dispositions; only real ranges use candidate
custody. Full validation and measured receipt APIs still reject equal endpoints.

## Obligation and HealthEngine verification

`obligation verify --id` prints its structured current derivation and returns a
nonzero exit when the obligation is unmet. `obligation validate-graph` prints the
whole graph result and exits nonzero for any unmet node, cycle, or missing
endpoint. `dependency_errors` exposes dangling edges; `total_edges` counts raw
stored edges. Malformed requirement arrays are errors, never empty proof lists.
`get` and `list` retain the stored status field for compatibility; that projection
cannot authorize readiness.

`obligation complete --id --commit` records a completion attempt using a nonempty
root and prints current readiness. It does not assert `Met`; missing IDs or empty
roots fail. Full validation rechecks recorded roots and legacy `met` projections,
including their transitive evidence, while unattempted open obligations remain
backlog. A public whole-graph check intentionally asks about every node.

Health claim references resolve an exact ID first, then a unique subject alias.
Duplicate subjects return `HEALTH_CLAIM_AMBIGUOUS`; a storage failure is not
reported as a missing claim. `govern health get` may refresh the advisory cache,
but obligation verification does not depend on that refresh. These behaviors are
implemented in `core/obligation.rs` and `plugins/health.rs`.

## Outbound Dependencies

Physical SQL execution remains behind Dactyl. Optional cloud storage uses the
selected Propodus/Supabase contract with server-derived authorization and no
fallback to local authority. Git supplies repository identities and versioned
artifacts. External executors supply source-bound proof only under the configured
provider policy; an unavailable provider remains an explicit blocker.

Laya model assets are immutable, pinned external artifacts. Explicit setup
verifies the private cache and optionally registers a native runtime and CPU
companion. Merely enabling `--laya-decisions` does not download assets or prove
that inference works. Required Laya policy and installation bindings are checked
again before final readiness. Runtime-registration version 2 requires explicit
re-registration of older local bindings, preserving files on refusal.

## Compatibility Matrix

Compatibility is established by the exposed operation and versioned data
contract, rather than by an agent's chosen provider or harness:

- CLI and process JSON RPC use the installed command schemas and the same gates
- Storage context version 1 separates local and authenticated cloud targets;
  unknown versions fail before I/O
- Native and Buildkite readiness implement the reviewed required profile;
  missing platform or provider capabilities remain blockers
- Local handshake declarations provide tamper evidence without authenticated
  provider identity; MCP and other planned adapters are not current binary APIs
- Cloud task completion and local verified completion remain distinct while the
  cloud contract lacks proof-capture support

## Local storage-owner transport (opt-in)

`DECAPOD_STORAGE_OWNER=1` enables the foreground owner for an elected group
broker. Default routing remains unchanged while this transition is validated.
The owner uses a confined local Unix socket derived from the canonical datastore,
with a versioned, bounded-frame protocol. Long paths use a hashed private UID or
explicit shared GID directory under /tmp; the same endpoint serves all worktrees. This is an internal cooperative-local
interface, not a new remote database service or authentication boundary.

An Open frame binds the canonical database path, access mode, and immutable
caller metadata to one lease. Session passwords are excluded. SQL parameters
retain their original types, including integer versus real and blob values.
The owner serializes leases, not individual statements, so BEGIN through
COMMIT/ROLLBACK cannot interleave across callers. Read-only leases are enforced
by the server and cannot use Read messages to disable query-only protection.
Temporary objects are removed at lease release, and connection configuration,
attachment, connection-local result counters and unsupported controls are rejected.

No operation is replayed after an uncertain transport outcome. Failure before
any owner connection exists may use the existing local path; accepted but
unanswered protocol traffic does not authorize local fallback. Closing or
losing a lease rolls back unfinished work. Normal foreground shutdown sends a
drain signal while retaining its lifetime pipe, stops new leases, and allows an
established lease up to five seconds to finish. It then reaps or kills/reaps the
supervised worker. Actual pipe EOF terminates the worker immediately, including
during SQL. The endpoint is removed after worker exit.

The #1476 regression retains the original two-agent legacy case and adds eight
owner-path agents in distinct Git worktrees. It must assert each caller's returned
and persisted assignee and traversal through one elected leader with one physical
owner opening. Additional fixtures cover twenty concurrent mutations, eight-way
exclusive-claim collision, disconnect after execution with another queued request,
unknown outcomes without replay, long paths, and parent loss during SQL with
rollback recovery. Socket-probe skips cannot pass owner acceptance. These tests
and required configured validation remain evidence obligations, not assertions
of completed platform qualification.

Group-broker protocol 3 persists an UNKNOWN receipt before command launch.
Same-ID retrieval is bound to the original caller and never relaunches a started
request; missing acknowledgement requires inspection, not a new request ID.
A command exit failure does not prove that no partial mutation committed.
Broker audit envelope 1.1.0 adds caller_agent_id while retaining actor/actor_id
as subsystem labels. request_id uses the originating broker request when present;
event_id stays unique. Historical 1.0 events deserialize with an empty caller
field rather than inventing prior attribution. Policy continues to run in each
isolated caller subprocess, with task-specific checks using the actual claimant.

## Intent Reconciliation RPC

## Acceptance observer capture boundaries

CaptureCoverage is an optional backward-compatible input to the existing decapod.agent-acceptance.v1 capture. Older captures remain readable with capture_coverage.state=unknown. Complete coverage requires the host, kernel, Git and GitHub channels, no dropped events, exact index digests and a retained terminal position. Post-exit independent Git/forge readback is supported. Shared or unknown witness custody is classified incomplete.

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`decapod rpc --op intent.reconcile --params '{}'` returns an advisory,
versioned reconciliation report. It exposes the governed-plan intent digest,
unresolved questions, current code fingerprint and last spec refresh identity,
canonical authored spec hashes, explicit `REQ-*` references, and observable
gaps. It accepts an empty parameter object, requires no session or worktree
for inspection, and does not mutate spec or governance records. Its
`semantic_alignment: not_proven` field is deliberately never a success proof.

## Immutable Intent Change-Impact RPC

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
`intent.reconcile` accepts an optional `base_commit` string containing an
exact 40/64-hex Git commit ID. When present, report schema 1.1.0 adds
`change_impact` with immutable base/head identities, sorted changed files
and rename predecessors, authored-material versus generated-only spec deltas,
REQ identifiers explicitly cited in changed authored specs, unmapped
implementation paths, completeness, advisory review/evidence flags and
structured findings. Uncommitted bytes are excluded from this Git comparison.
Invalid symbolic, absent or non-ancestor bases return a validation error.
The original empty-parameter report stays version 1.0.0 without an impact
field. Nothing in this operation authorizes a mutation, approval or completion.

## Bounded local storage inspection

CLI `data database inspect` and RPC `data.database.inspect` expose
`decapod.storage-inventory.v1`. Options bound rows, decoded scan bytes, report
bytes and duration; reads beyond five seconds require an explicit option.
Missing stores remain missing; sampled, unavailable and partial metrics are
labeled. Incomplete inspection has a typed limitation and nonzero exit.
The effective catalog digest covers tables, indexes, triggers and views without
emitting DDL. Checkpoint and prefix metadata samples retain separate coverage.
RPC framing is capped at 32 MiB before JSON decoding. See
`docs/storage-inventory.md` for interpretation and bounds.

## Plan contract revision boundary

`govern plan update`, `phase-add`, `phase-update`, `add-entry-gate`,
`add-exit-gate`, and `update-gate` revoke acceptance and all phase/gate progress
when their proposed contract actually differs. Repeating identical values leaves
acceptance and progress intact. Review the revised contract and explicitly
`govern plan approve`, then satisfy and replay its phases; `enter-phase` and
`exit-phase` return `NEEDS_PLAN_APPROVAL` for draft or annotating plans.

`plan_governance::patch_plan` rejects a combined contract change and accepted
state with `PLAN_CHANGED_REQUIRES_APPROVAL`, preserving the previous document.
Final proposed state, rather than the pre-patch phase list, determines whether
`DONE` is structurally valid. `validate_plan_progress` enforces incomplete-phase
and unsatisfied-completed-gate rejection for execution and publication as well.
The `governed_plan.instruction_readiness` object in inference orientation
reports the current plan's source revision, verdict, source pointers and
remediation. Approval, execution checks and checkpoints, validation, and
publication enforce the same recorded blockers for missing intent, unknowns,
questions, contradictions, and pending spec decisions. The report describes
recorded constraints. Explicit approval records declared authority.
Plan initialization preserves pending spec decisions. Recording or resolving
one of those decisions requires new plan approval.

A plan with selected work records `accepted_selected_work_revision` on approval.
If the accepted policy changes, or the plan predates the revision field,
readiness reports `NEEDS_PLAN_APPROVAL` once current instruction blockers are
resolved. Reapprove the plan and rerun affected proof. Plans without selected
work continue to omit this field.

The public plan CLI regressions live in the existing branch-enforced
`tests/cli_contract_enforcement.rs::plan_governed_execution` module. Their
standalone predecessor was not selected by the source-PR graph. This
executed target makes CLI routing, reapproval, blocker parity, and phase
replay behavior part of the normal required proof without changing its adapter.

## Consumer readiness and repeatable initialization

`proof.required_profile.id = "repo.pr-readiness.v1"` is the supported local consumer
profile. `proof.repository_contract` declares a reviewed command digest,
`source_coverage` rows and per-command `executions`. Every discovered workflow has
exactly one explicit mapping to real required commands; all configured commands
execute. Absolute installed programs, declared runtime/tool files, exact environment,
working directory and timeout are configuration-bound. Each execution entry declares
either single-platform top-level `program` + `toolchain_files` or a mutually
exclusive `platforms` map (`PlatformExecution`) keyed by canonical host platform
(`linux-x86_64`, `linux-aarch64`, `macos-x86_64`, `macos-aarch64`), preserving
byte-identical `command_contract_digest` values for single-platform declarations
while binding every declared platform alternative into `reviewed_commands_digest`
and `preserve_accepted` (`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`). Unknown IDs,
invalid/unknown/mixed platform declarations, stale pins, empty/optional commands,
silent binary substitutions and missing or duplicate coverage fail closed. Init
exposes both discovered digests for review without updating them or implying proof
success.

A `source_coverage` row may instead declare a reviewed CI-only limitation (#1531):
`{ source = ".github/workflows/ci-only.yml", ci_only = true, reason = "..." }` (`commands`
may be omitted or empty). Such a row pins the limitation through `reviewed_commands_digest`,
requires a nonempty `reason` and no local commands (a `reason` without `ci_only`, or
`ci_only` with mapped commands, fails closed as a false coverage claim), and is reported in
`validation-results.v3` as a `NonApplicable` limitation stage for that workflow —
never counted as a passed or skipped check. When every discovered workflow is
`ci_only`, no row maps commands by design: the required local commands still execute
normally as the repository's local build/test contract and the usual
every-command-mapped invariant does not apply. Rows without the new fields keep their
pre-#1531 serialized identity and digests.

`init` and `init with` share option-provenance-aware preservation: omitted defaults
retain authored configuration even under `--force`; explicit arguments still change
it. The generated GitHub validation workflow fetches complete history and supplies
immutable `CHANGE_EVENT`, `CHANGE_BASE` and `CHANGE_HEAD` to CI-local validation.

## Local CPU companion and model-free startup protocol

`setup laya --runtime <path> --cpu-runtime <path>` registers explicit local native
executable pins. Startup probes and fallback use the existing Laya protocol and
bounded process adapter. A successful probe is not model inference evidence.

## Supplemental Rust worker verification

Generic Rust proof recipes remain physical prerequisites, not model policy.
Configured manifests, features, arguments, dependency locks and timeouts are
bounded and source-bound. Worker outcomes cannot authorize publication, and
this repository declares no required worker recipe.

## Material-spec gate execution records

The validation JSON report's existing `notes` array records material-spec
pass/skip disposition. A valid immutable release emits
`material_specs: outcome=passed proof=version_only_exact_projections`;
a local early exit instead identifies its explicit skipped reason. Required
CI context errors remain failures, never these local skip dispositions.

## Handoff caller and recovery contract

The `todo handoff` interface captures the actual request caller before broker
execution. `--from` asserts the expected recorded owner and `--to` selects the
receiver; neither changes caller identity or trust. The result and task event
preserve actor, source owner, receiver and recovery authority separately.
`plugins::policy::eval_effective_policy` reports the enforced handoff zone,
including verified trust and approval, from the same canonical risk-zone data.

`core::todo::handoff_task_as` supports an explicit in-process request caller and
shares the broker's existing cooperative operator trust semantics for handoff.
It never promotes stored agent trust. Cross-owner recovery additionally requires
an explicit matching source owner and live exact task-bound approval. Successful
recovery consumes the approval records atomically with custody mutation and
records their IDs; failed preconditions retain both state and authority. General,
wrong-target, expired and consumed approvals cannot authorize that recovery.
The reserved local operator identity is not operating-system authentication.

The scoped tests in `tests/plugins/todo.rs` and `tests/plugins/policy.rs` cover
caller/source separation, CLI default caller, operator recovery, policy parity
and rejection boundaries. These tests are authored obligations until full
candidate validation executes; no test or PR-readiness success is asserted here.

The `eval_effective_policy` helper shares `is_read_only_operation` with broker
mutation enforcement and return LOW with no mutation requirements or HITL gate
before reading mutation policy or initializing a datastore. In particular,
`todo.get`, `todo.list`, `todo.status`, and `policy.list` cannot seed TODO state
or inherit mutation-zone trust and approval requirements, including for paths
whose write-risk classification is critical. The read-only side-effect regression
in `tests/plugins/policy.rs` remains authored and unexecuted pending validation.
This helper-level guarantee does not make the entire CLI side-effect-free:
preexisting policy CLI initialization can still initialize policy storage.

The first executed plugin suite on `fe0b7b77` passed policy tests and 24 of
27 TODO tests, but three TODO fixtures failed before exercising their intended
assertions: missing subprocess Git author identity, identical source/recipient
input, and a legacy CLI fixture omitting actual caller identity. The fixtures
now supply explicit caller context and deterministic local test Git identity,
and separately check source-owner and recipient spoof rejection. These repairs
remain unverified until the exact new candidate executes; no runtime policy
requirements were relaxed to accommodate the fixtures.

### Agent/harness execution boundary

Users execute only `decapod init`. All other Decapod interfaces, including
handoff evaluation, approval recording and recovery execution, are invoked by
the AI agent/harness. A human provides intent and explicit approval through
that agent/harness, never a required human operator CLI session. The harness
preserves the actual established caller independently of approval attribution,
expected owner and recipient, and reports the resulting audit. Missing caller
authority remains a blocker; the execution boundary grants no role elevation.

The invoking agent/harness discovers handoff recovery in actual failure output:
`ValidationError` preserves the denial reason and appends `HANDOFF_RECOVERY:`
plus JSON schema `decapod.todo.handoff-recovery.v1`; non-success JSON responses
carry the same object as `recovery`. Fields identify task, actual actor, expected
owner and recipient. `inspect_argv` and `evaluate_argv` provide bounded read and
policy-evaluation actions; `next_action` requires established authority and
conversational human approval when necessary, never self-authorization. It
warns that reconciliation can fail after custody commits, requiring inspection
before retry. Diagnostic construction itself does not read or write state.
Regression assertions cover both early validation errors and JSON precondition
failures without creating storage; these new assertions await exact CI execution.

Other typed failures, including post-transfer reconciliation I/O, emit the same
versioned recovery context to stderr on a best-effort basis before returning the
original typed error unchanged. Failure to write diagnostic output cannot mask
the original error. The context uses a bounded generic reason for these failures
and does not duplicate their potentially sensitive raw cause into recovery JSON;
the ordinary error retains its original cause. Unit regressions cover preserved
I/O classification and diagnostic broken-pipe behavior.

### Integration with request-scoped broker identity

The v0.123.1 accepted broker carries each request's established caller and working
directory into its subprocess. Handoff reads that request identity independently
of expected owner and receiver; merging the broker fix does not promote caller
trust or remove exact recovery approval. Native-proof interfaces and their agent-directed recovery remain intact
alongside this handoff contract.
The earlier fixture-repair candidate `67b166d9` passed both push and PR GitHub
plugin suites; this integrated candidate still requires fresh complete proof.

The concurrent caller regression uses distinct seeded `features` and `docs`
categories while retaining independent caller identities, directories, scopes,
leader traversal and response/persisted-owner assertions. Normal category
validation remains enforced: caller names do not create categories. Buildkite
build 2931 exposed the invalid-category fixture setup before either claim;
the corrected fixture requires fresh official execution evidence.

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

## Regenerated handoff guidance

`docs_cli::todo_handoff_contract` is the canonical generator body for the existing
agent-only caller and ownership recovery contract. `build_docs` emits it between
governance artifacts and explicit claim scopes, preventing Docs Sync from deleting
the tracked handoff section. The focused `trajectory_docs` regression regenerates
command contracts and checks exact inclusion, single emission, required approval
and unchanged trust boundaries. This repair changes documentation generation, not
handoff authorization or task custody. Its runtime regression still requires
execution through the configured validation workflow.

### Native validation recovery diagnostics

Each executed native check may produce `native-N.diagnostic.json` alongside its
private stdout, stderr, and step records in the reported external evidence
directory. The versioned diagnostic identifies the complete check key, elapsed
milliseconds, termination, check-level error and the three related filenames.
This metadata is an agent recovery aid, not an imported receipt or readiness
authority. The independent evidence, candidate and target binding and all
required gates remain necessary; incomplete or failed execution cannot publish a
successful completion bundle.

## Concurrency and recoverable evaluation

Task claims expose advisory source overlaps and `custody_pending` authoring
state. `workspace ensure` verifies a claimed task's observed checkout before
admitting authoring. Secondary ownership has an explicit independent-task
recovery diagnostic, rather than borrowing the primary lease. `resource:<name>`
is an explicit local mutable-resource reservation; the cloud task/event profile
returns `RESOURCE_CUSTODY_PROFILE_UNSUPPORTED` for that capability.

Cross-owner handoff approvals bind task, current owner, lease generation and
receiver, expire within the supported 24-hour window and are consumed only by
successful transfer. Failed diagnostics supply current generation and the exact
policy evaluation target without fabricating approval. The policy CLI's
approving `--actor` is separate from its executing caller.

Prompt evaluation returns `invalid_input` or `error` with `evaluated=false` for
unevaluated requests and nonzero exit. Recovery requests complete-original-prompt
evaluation without human approval for invocation correction. `allow` and `block`
retain their policy meaning and exit semantics. Compatibility `/query` / `/batch`
transport remains unchanged; native RPC/hosted proof is not inferred from it.

## Checkpoint history inspection and recovery

`govern artifacts verify-checkpoints --format json` returns the versioned
`decapod.checkpoint-history.v1` assessment. `scope=checkpoint_history` and
`publication_ready=false` prevent confusing this gate with complete readiness.
`history_proven`, `inspection_complete`, exact base/head commits, verified
commits and per-commit findings expose proven, unproven-history, invalid-history
and unavailable outcomes. Each finding names its commit, tree, obligation,
recovery and material digest when knowable. An incomplete assessment is explicit.

`govern artifacts record-history-recovery --source-ref <ref> --reason <text>`
requires a fresh admitted workstream at its accepted base. It records current
actor/custody, immutable source identities and gap assessment, with
`historical_proof=not_proven` and `exception_authorized=false`. It neither copies
source nor changes its ownership/history. Identical calls are idempotent;
conflicting context is rejected. `resolve-obligation` requires the recovery
observation to be archived in Git before retiring its active obligation.
Publication still independently verifies every commit in the new range.

## Event storage compatibility

Event physical layout revision 2 adds no CLI/RPC fields and changes no proof
encoding. Historical DDL remains available to migration fixtures; current
initialization no longer creates `idx_events_stream_seq`. The full unique
`idx_events_stream_seq_unique` continues enforcing stream-local order identity.
`EVENT_INDEX_COMPACTION_UNSAFE` identifies altered prerequisite definitions.
Older stores missing the unique tree receive it additively; duplicate sequences
fail with the original typed storage error and rollback, without normalization.
Native PostgreSQL uses Propodus's own migrations; the SQLite index migration is
never sent through cloud RPC.

## Laya-only optional setup and initialization

`setup laya` supports cache-directory, runtime, CPU companion, offline and
update-check options. `init --laya-decisions[=true|false]` explicitly selects the
optional assessment policy; omission preserves it. Init preserves current Laya
settings and opaque user extension tables. Only `[laya]` has model-policy
semantics. No retired model configuration or receipt type is an API alias.

## Relational compaction compatibility

The `sqlite.compact_redundant_relational_indexes.v001` forward migration
changes only redundant local SQLite btree structures. No CLI/RPC, Dactyl
operation, task revision, event sequence, transaction semantics, or cloud
schema change. An unexpected historical index or missing unique enforcement
raises `RELATIONAL_INDEX_COMPACTION_UNSAFE` and rolls back all index removal.
Freed SQLite pages are reusable but are not auto-vacuumed.

## Human setup and cloud handoff presentation

Interactive `decapod init` presents Project, Preferences and Review with
Recommended/Customize and Apply/Edit/Cancel choices. Arrow keys, typed choices,
Enter and Escape work in supported terminals; redirected output and `--proof`
retain unattended behavior. Preview writes nothing; cancellation before apply
does not create a new directory or initiate cloud authentication. Reinitialization
preserves existing backend, isolation and structured proof settings.
An inherited terminal supports the same keys when `/dev/tty` is unavailable.

The cloud review describes account sign-in and service-confirmed repository
access. It accepts no seat roster, provider credential or local price declaration.
After apply, the accepted managed-session flow may return `onboarding_pending`,
`action_required` or a denial; these remain non-success and cannot produce the
terminal ready receipt. Existing headless continuation, private custody, explicit
restart and server-readiness contracts are preserved. No managed-project selector
or protected hosted-payment assurance is added here.

## Validation proof lifecycle

Validation evidence rules execute at the terminal publication boundary. `validation_proof` identifies the actual current invocation's complete-suite proof, not the legacy prerequisite receipt in `governance.json#/sections/validation`. That legacy receipt keeps its direct `receipt_hash` shape and integrity check. Workunit completion applies only to a current task with an explicit durable workunit declaration; journal lookup failures and missing, invalid or stale declared projections block instead of becoming an absent contract.

Only the kernel optional workunit-completion rule can become inapplicable when the authoritative journal reports no declaration. A separate mandatory project or task rule still evaluates and fails when required workunit evidence is absent. Optional manifest epoch fields constrain evidence when provided; the supported current execution binding positively binds the actual epoch when those legacy fields are unset.

Task binding on a managed `agent/<actor>/<topic>` branch reads task identifiers from the topic. A task-shaped actor name does not declare another task. Legacy unnamespaced and `agent/<task>` branches and multiple task identifiers in a topic retain their existing matching and mismatch checks.

## Explicit managed project routing

`cloud projects` and `cloud project use|create` consume the existing version-one account status/register channel. A saved numbered choice references the exact prior directory for its managed account. Creation persists an immutable UUID/name before the request and retains the response reference after success; ambiguous outcomes reconcile the same UUID. Selected project status must echo its project and managed account before use. Text output shows names/status; credentials and routing records remain outside repository configuration.

`tests/supabase_service.rs` declares the separate `local-native-projects-v1` consumer profile. An independent fixture owner supplies two ordinary managed bearers, disposable loopback PostgREST, and private client/receipt paths. The first phase selects a real account project and observes a shared native task through two CLI processes; the second phase requires the owner to revoke one account before checking its next operation. Missing provisioned services are unavailable. Modeled session issuance, HTTP fixtures, real local PostgreSQL, hosted authentication and released artifacts remain distinct proof categories.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## Versioned cloud mutation receipts

Cloud todo receipt cache v2 is an object with version=2 and records. The reader accepts legacy flat record arrays, preserves their uncertainty and original repository semantics, and rewrites them as v2 under the stable lock. Unsupported future versions fail without replacement. Server request/result contracts and the cloud-session credential record remain unchanged.

## PR 1666: accepted v0.143.2 source integration

The existing public provider and Budget interfaces retain HTTP403/429 failures and cancellation/deadline semantics. Missing or malformed quota reset information cannot authorize another request or a ready judgment. No alternate credential, provider, receipt or daemon is introduced.

## PR1666 current-master integration

Existing snapshot-provider calls and typed failures remain unchanged. HTTP403/429 denial remains immediate and no authentication fallback is added. Integration preserves the current source-CI event contract and its exact base/head binding.

## PR1663 integrated storage behavior

Preserve versioned journal/transfer and inventory outputs, checked write admission and explicit-only page reclamation. Existing custody and resource APIs remain exported. No provider/check requirement is removed; current-source inventory is reviewed after accepted-master integration.

## PR 1662: accepted v0.143.2 source integration

Existing `data database` maintenance/capability/lock/audit CLI and RPC operations report allowed, denied or unsupported capability decisions. Candidate-only opt-in remains `DB_MAINTENANCE_POLICY_UNACCEPTED`; owner repair is separate. Configured local authority cannot imply authenticated cloud access.

## PR1662 current-master preparation

Preserve symmetric data.database CLI/RPC capability discovery, diagnostics, bounded maintenance and audit interfaces. Policy admission must happen before startup migrations for maintenance commands; raw SQL, destructive replacement and arbitrary destinations remain outside the delegated contract.

## Bounded independent collection (#1573)

Public403 failures retain operation/method/endpoint/status and bounded numeric
reset hints, without treating an authenticated diagnostic client's quota as the
public reader's grant. A safely replayable collection starts only after the
reported window and actual workflow production/completion, with source/target
rechecked. Unknown, missing and unsupported proof cannot become readiness.

## Maintenance proof scope (#1568)

The supported `data database` / `data db maintain` CLI and `data.database.*` RPC
contracts are unchanged. Required cases exercise default denial, canonical local
opt-in, worktree self-elevation rejection, policy revocation, scoped capabilities,
backup restrictions, unsupported destructive operations and concurrent retry.
Two deterministic workers exercise actual claims/workspaces and custody; this is
not a demonstration of two native LLM agent integrations. Existing operator
recovery fixtures remain diagnostic and do not grant agent recovery authority.

## SDK resource and uncertainty compatibility (#1667)

The shipped 0.15/0.16 API adds `ResourceBudget`, `open_with_budget`, `visit_rows`,
`has_table`, `has_column`, failure-origin/mutation-outcome classifiers and logical
connector/session types without replacing the existing open/read/write facade.
The 0.16.1 patch retains identical Rust library bytes. This dependency adoption
does not claim that every caller has migrated to the additive interfaces or that
a remote capability error grants local fallback or replay permission. Existing
backend owners remain authoritative for those decisions.

### Local SDK value and parameter ceilings

The physical local bridge retains an 8 MiB per-value compatibility ceiling and
allows 16 MiB per bound parameter set so a ceiling-sized value can coexist with
its primary key and metadata. These are separate resource limits. SDK v1 charges
BLOB result bytes at 32 times their raw length, so the local decoded-result
accounting ceiling is 272 MiB: one 8 MiB component plus 16 MiB of accounted
headroom. This is an accounting bound, not a buffer allocation or a new logical
field allowance. Row counts and cloud budgets retain SDK defaults; application
field admission remains unchanged. The real SQLite regression tests the exact 8 MiB boundary with a
bound key, lossless re-read, unrelated update and rejection before a write of
a value exceeding the cell ceiling. A codec decoding guard does not authorize
larger application records.

The journal regression persists and independently reconstructs a retained
codec-0 component at the full cell ceiling, checking its exact byte length and
SHA-256 through the production component owner. It does not rewrite historical
bytes or establish task completion. New text values above the cell ceiling and
aggregate parameter sets above their separate budget still fail before mutation.

## Full claims with diagnostic gate overrides (#1659)

`READINESS_GATE_OVERRIDE_UNSUPPORTED` identifies an active Git/tooling skip or a
truthy fingerprint skip. Remove the override and obtain fresh validation in the
claimed worktree. Presence-only Git/tooling flags remain active even with empty,
zero or false strings; the existing fingerprint truth-value parser is preserved.
Explicit `ci_local` and `project_prerequisites` diagnostics retain their output
scopes and cannot yield `publication_ready=true`. An audited validation bypass
also remains non-proof. No new command, adapter policy or verdict taxonomy is
introduced. Older brokers reject unfamiliar context fields; inspect uncertain
operation outcomes rather than blindly replaying a write.

## Accepted repository-local contract changes (#1658)

The existing validation runtime returns the actionable
`ACCEPTED_REPOSITORY_CONTRACT_CHANGED` blocker when a candidate removes or replaces
accepted local obligations, even with internally consistent new review digests.
The check precedes execution-provider selection. Remove the weakening and retain
the accepted checks, or first establish a separately accepted policy transition
on the target. Additional required checks are allowed. Initial local adoption and
unchanged accepted contracts continue through the existing runtime and receipt
interfaces; no new command or bypass is exposed.

### Source-definition changes

`ACCEPTED_REPOSITORY_CONTRACT_CHANGED` also identifies replacement of the accepted
local source-definition digest. Recomputing a candidate digest cannot authorize
changed inventoried check scripts, build/dependency definitions or provider
inputs. Retain the accepted inventory or establish the reviewed revision on the
target before revalidation. Additive commands under that inventory and first
local adoption remain supported; no override or second policy store is added.

## Group broker workload budget

`src/decapod/core/group_broker.rs` accepts the optional
`DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS` workload input on the Unix broker path.
The default is 120000 milliseconds; valid explicit values are 1 through 300000.
Malformed, zero and oversized client values fail before mutation admission.
Native peers also face the hard maximum before a new dedupe receipt can be created.
A matching durable outcome is resolved first: expired or overlong retry deadlines
cannot replace an existing UNKNOWN or committed result with NOT_COMMITTED. The
real execution-boundary regression checks both outcomes and both retry timings,
including the unchanged stored result and commit marker. No new operation runs
when the same-ID outcome is already recorded.
An explicit policy refusal before a local `todo archive` transaction returns
`NOT_COMMITTED` with `BROKER_POLICY_DENIED`, the policy diagnostic, and no
commit marker or retry delay. A same-ID request returns the stored denial.
The foreground acceptance window follows the originating request budget, and
existing per-command limits and UNKNOWN/replay handling remain in force.

`tests/group_broker.rs` declares its existing 300-second twenty-client workload
at both the outer process boundary and broker request boundary, retains the
all-twenty exact-once assertions, and exercises invalid inputs through the actual
CLI with no task or receipt mutation. The native oversized-deadline regression
traverses the real execution boundary. Fresh hosted verification is required;
these declarations are not a successful validation observation.

## Canonical workunit validation contract (#1660)

Existing workunit CLI operations remain the request surface, extended with
`decapod govern workunit converge [--task-id <id>] [--dry-run]` and typed
`proof_status` (`proven` | `not_proven` | `stale` | `absent`), `split_store`,
`atomic_domain_supported`, `audit_records`, and `convergence_hint` fields on
`WorkUnitJournalStatus` (and `proof_status`/`split_store`/`atomic_domain_supported`
on `govern workunit status`). `record-proof` records a declaration; requesting
VERIFIED requires a current canonical full observation for exactly one supported
`validate_passes`/`validated_commit` obligation. The kernel derives the result,
epoch and optional validation-record selector. Caller-supplied selectors cannot
import producer success. File artifact paths retain byte-integrity semantics.
Separately initialized workspace-local datastores return
`WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED`; explicit `converge` archives
workspace-local records into `.decapod/data/split-store-audit/` as `not_proven`
audit history without promoting split records to `VERIFIED`, restoring atomic
canonical-store execution for fresh validation. Historical JSON omits the
optional field; Rust manifest literals add `validation_record: None`, and
verification consumers pass repository context to
`validate_verified_manifest(root, manifest)`. Publication and human merge
authorization remain independent operations.

## Scoped container-runtime recovery during prerequisites

`src/decapod/plugins/container.rs::heal_container_runtime_override` first uses
the existing disabled-runtime marker predicate. A missing or unrelated override
returns unchanged without probing or starting a container engine. An existing
disabled-runtime override retains the existing availability probe and scoped
recovery behavior. Required validation checks and publication proof requirements
remain independent of this optional preparation step.

The isolated subprocess regression in `tests/unit/plugins/container_tests.rs`
observes runtime invocations for missing, unrelated and disabled override cases,
and checks preservation of unrelated content. It does not claim live container
isolation or publication readiness. The actual CLI runtime probe retains its
10-second per-call and 20-second aggregate requirements; fresh hosted execution
must establish the resulting candidate's behavior and timing.

## Legacy input recovery scope (#1657)

Normal supported startup/workspace/task interfaces reuse the existing migration
owner's inert-placeholder handling. There is no new slash command or agent
prompt requirement. TASK_JOURNAL_MIGRATION_REQUIRED continues to block non-inert
TODO inputs beside canonical journal state. Explicit V1 recovery below covers
only empty input or exact canonical copies; general historical reconciliation
remains required. See
`docs/architecture/legacy-todo-recovery.md`.

## Versioned legacy retirement interface (#1657)

`decapod todo journal reconcile-legacy` reviews the canonical main repository's
local store before normal migration/session-presence writes. JSON distinguishes
empty/canonical_copy/unmatched/corrupt/unverifiable inputs and returns a review
SHA only for supported inputs under an explicit `todo.legacy.reconcile` grant.
`--expected-review sha256:<digest>` requires an active local session password
and rechecks exact identities, bytes and canonical policy. `--prepare-only`
requires that review token, persists intent and preserves the source for
interruption/recovery. The same apply request resumes a prepared operation.
No arbitrary source path, destination or SQL input is accepted.

`governance.rbac.legacy_todo_reconcile_roles` defaults to empty; neither journal
repair nor ordinary maintenance grants this permission. The receipt records
physical archival only: every response leaves `publication_ready=false` and
`completion_proven=false`. A committed retry reports a historical outcome,
without asserting current canonical validation; a reappeared legacy source
blocks reuse. Unsupported nonmatching historical stores stay blocked. See
`docs/architecture/legacy-todo-recovery.md` for the exact scope and recovery
limits; this is not completion of all #1657 migration requirements.

## Nix release gate outputs (#1626)

`Select Cachix publisher` retains `publish=true|false`; packaging retains
`run=true|false`. Both recognize ordinary two-parent release merges and
one-parent release commits such as `chore: release v0.138.0 (#1624)`. Parent
shape must be supported and the selected subject must retain the existing
`chore: release v` prefix. The credential-free owner fetches the exact protected
pushed SHA at depth two, sufficient for the selected commit/parent subject.

Manual `workflow_dispatch` still requests packaging proof only. The native-owner
conditions and publisher dependency/ref/event restrictions remain unchanged;
manual or imported/self-hosted execution cannot grant Cachix publication.

## Canonical local Git proof (#1069)

An accepted `proof.repository_contract.canonical_local_target = true` uses ordinary CLI/RPC validation against the canonical local branch. `readiness.ready` refers to that exact local subject with `scope=repository_local_git_readiness`; `publication_ready` stays false. `qa verify commit` inspects canonical historical proof. `workspace publish` and the independent final PR gate reject local-only target authority before remote publication. Omitted/false mode retains existing digests and GitHub behavior. Candidate-only mode, changed branch, strict profile and cloud selection cannot activate the local host.

## Merged governance journal reconciliation (#1780)

`decapod govern artifacts reconcile-merge` provides a supported recovery path
when a Git 3-way merge (`MERGE_HEAD` present, e.g. `git merge --no-commit -X
ours`) leaves `.decapod/governance.json` conflicted or failing with
`Trajectory artifact hash mismatch`. The command requires an active merge
(`GOVERNANCE_MERGE_NOT_IN_PROGRESS` otherwise), reads only immutable Git commit
objects (`HEAD`, `MERGE_HEAD`, and `merge-base --all`) rather than trusting
corrupted working-tree bytes, and fails closed on conflicting unaccepted PR
authority (`GOVERNANCE_MERGE_AUTHORITY_CONFLICT`), ambiguous merge bases
(`GOVERNANCE_MERGE_AMBIGUOUS_BASE`), or contradictory check proof statuses
(`GOVERNANCE_MERGE_PROOF_CONFLICT`).

Same-workstream merges reconcile additive `trajectory` sets, merge `claims`,
`resolutions`, `checkpoints`, `plan`, and `custody` state (re-sequencing
divergent custody events/steps and remapping `loop.custody_event_id`), clear any
stale `validation` receipt when trajectory or validation changed, and recompute
`proof_status`, `verdicts`, and `artifact_hash` via `with_recomputed_hash()` so
failed or missing proof never becomes passing. For a shared intent, the
reconciler preserves the original request, its source provenance, and the
common instruction history. It retains a one-sided append and rejects
conflicting revisions or a truncated history. Accepted-target merges
(`origin/<target_base>`) retain the complete active workstream journal while
carrying forward newly accepted baseline policy and unresolved obligations.
`tests/merged_governance_journal_1780.rs` exercises the real binary across
`git merge --no-commit -X ours` recovery, checkpointed merge commit
verification (`verify-checkpoints --publication-material`), and negative proof
and authority controls.

## Todo-scoped /tmp cleanup guidance (#1798)

`src/decapod/core/validate.rs` exposes `resolve_validation_task_id`, `validation_temp_root`, `todo_tmp_cleanup_instruction_for_task`, and `todo_tmp_cleanup_instruction`, resolving the active todo scope (`DECAPOD_TASK_ID` or branch-encoded task ID) to `<temp_dir>/decapod_validate/<task-id>` without modifying internal `create_validation_temp_path` or `cleanup_validation_temp_paths` ownership semantics.

When validation succeeds (`ValidationGuidance::for_outcome(true)`, `ValidationGuidance::for_readiness_outcome(true, Mode::Required, Decision::Accept)`, `decapod validate`, and `validate.run` remote readiness payloads) and when PR creation completes (`workspace::publish_workspace_with_readiness` warnings via `workspace::pr_admission_warnings` and `decapod qa diagnose` `cleanup_guidance` when `next_command == "decapod workspace publish"`), Decapod instructs the agent to flush ONLY that todo's `/tmp` build and scratch directories (such as `<temp_dir>/decapod_validate/<task-id>` and any todo-scoped `CARGO_TARGET_DIR` under `/tmp`). The instruction explicitly forbids wildcard or root-level deletion of `/tmp`, deleting another concurrent agent's or sibling todo's `/tmp` path, and touching shared `/tmp/decapod` controller sockets/state or active sibling directories. The existing `PublishResult`, `WorkspacePublishResult`, and `ValidationGuidance` wire shapes remain unchanged. `docs/agent/command-contracts.md` (`docs_cli::readiness_contract` and the `decapod workspace` contract) and `docs/agent/state-model.md` document the same todo-scoped `/tmp` cleanup rule, verified by `tests/tmp_build_cleanup_guidance_1798.rs` and `tests/unit/plugins/verify_tests.rs`.

## Read-only budgeted projection and retention contract (#1734)

- **Schema & Authority Boundary (`decapod.projection-budget.v1`):** `core/projection_budget.rs` (re-exported through `core/projection.rs`) defines strictly read-only, non-authoritative projection envelopes (`EventProjectionPage`, `WorktreeProjectionPage`) carrying `read_only = true`, `authoritative = false`, `status` (`complete` | `partial_has_more` | `degraded`), `degraded_reasons`, `metrics`, `budget`, and deterministic SHA-256 receipts (`content_sha256`, `window_receipt_sha256`, `continuity_receipt_sha256`, `receipt_sha256`). Independent observability clients (such as Darkmatter) consume these contracts without embedding visualization state machines or UI dependencies into Decapod.
- **Event Cursor & Retention Interface:** `EventProjectionQuery { stream, after_seq, subject_kind, subject_id, event_type, budget }` executes via `project_events_read_only` (using `core/db.rs::db_connect_readonly_bounded`) or `project_events_on_conn`. `EventProjectionCursor` exposes `after_seq`, `next_seq`, `last_event_id`, `retention_floor_seq`, `retention_anchor_sha256`, `disposition` (`continuous` | `starts_at_retention_floor` | `behind_retention_floor`), and `has_more`. `apply_bounded_event_retention` and `read_retention_anchor` manage `decapod.event-retention-anchor.v1` watermarks in `meta(namespace='event_retention')`.
- **Worktree Inventory Interface:** `WorktreeProjectionQuery { after_worktree_path, include_dirty_summary, budget }` executes via `project_worktrees_read_only` using `GIT_OPTIONAL_LOCKS=0`, returning deterministically sorted `WorktreeProjectionEntry` items with bounded `dirty_file_count`, `dirty_truncated`, `sample_dirty_paths`, and `next_cursor` pagination.

## Risk-map warning CLI contract (#1782)

`src/decapod/cli.rs` mounts `PolicyCli` (`src/decapod/plugins/policy.rs`) exclusively under `Command::Govern(GovernCommand::Policy)`, exposing risk-map initialization and verification at `decapod govern policy riskmap init` and `decapod govern policy riskmap verify`. Both missing-map diagnostics—`src/decapod/core/validate.rs::validate_risk_map` and `RiskmapSubcommand::Verify` in `src/decapod/plugins/policy.rs`—derive their warning guidance from `plugins::policy::missing_riskmap_warning` and `plugins::policy::RISKMAP_INIT_COMMAND` (`decapod govern policy riskmap init`), removing the previously emitted unsupported `decapod riskmap init` and `decapod policy riskmap init` paths. Read-only `RiskmapSubcommand::Verify` remains unauthenticated by `policy.control`, while `RiskmapSubcommand::Init` retains its existing `enforce_broker_mutation_policy(..., "policy.control")` (`core` trust) gate.

Scoped regression coverage in `tests/unit/core/validate_tests.rs::risk_map_warning_recommends_supported_cli_command_and_clears_when_present`, `tests/plugins/policy.rs::riskmap_verify_recommends_supported_cli_command_when_missing`, and `tests/cli_contract_enforcement.rs::documented_bootstrap::risk_map_warnings_recommend_supported_cli_command_paths` checks `Cli::try_parse_from` and real CLI binary execution across missing-map warning emission, `--help` resolution, unsupported-path rejection, `core`-trust enforcement, and warning clearance once `RISKMAP.json` exists; complete exact-candidate validation remains required before PR publication.

## Compact storage v2 compatibility floor (#1739)

`core::schemas::STORAGE_MIN_READER_VERSION` and `core::schemas::STORAGE_MIN_WRITER_VERSION` are `"0.144.2"`, and `core::migration::all_migrations()` sets `min_version = "0.144.2"` and `target_version = "0.144.2"` on `sqlite.compact_lossless_storage.v002` (`sequence: 1200`), matching the first release that implements `_ce_v1` envelope reconstruction (`core::events::reconstruct_stored_payload`) and component-backed journals. `core::events::{query,query_all_on_conn,query_subject_on_conn,query_subject_event_on_conn}` enforce `check_read_compatibility`, and `core::events::{ensure_tables,append_on_conn,check_subject_append_capacity}` enforce `check_storage_format_compatibility` (`STORAGE_FORMAT_INCOMPATIBLE`). Existing v2 datastores stamped `0.141.0` prior to #1739 are restamped to `0.144.2` on `activate`/migration and write-side table initialization (`reconcile_v2_storage_format_metadata`) without mutating event/task rows, downgrading higher valid floors, touching unmigrated v1 stores, or writing on read-only connections.

## Verifiable broker writer incarnation and orphan recovery (#1783)

`BrokerEvent` in `src/decapod/core/broker.rs` extends new `pending` audit events with backward-compatible optional fields `writer_pid`, `writer_start_id`, `writer_boot_id`, `writer_host_id`, and `writer_pid_ns` populated by `WriterIncarnation::current()`, while terminal and acknowledgment events omit them when unset. `DbBroker::verify_replay` and `decapod data broker verify` populate each `Divergence` with `writer_liveness` (`active` | `stopped` | `unknown`), `human_review_required`, `liveness_detail`, and any captured `writer_*` provenance fields.

`BrokerCommand::Repair` (`src/decapod/cli.rs`, routed to `DbBroker::repair_with_options` in `src/decapod/lib.rs`) adds `--acknowledge-unknown-writer` and reports `writer_liveness`, `human_review_required`, `liveness_detail`, `acknowledgment_required`, and `acknowledged_unknown_writer` in `RepairReport`. Active writers (`writer_liveness = "active"`) are unconditionally refused in both preview and `--apply`. Proven stopped writers (`writer_liveness = "stopped"`) repair directly with `--apply` without waiting on age thresholds or requiring overrides. Legacy provenance-free, partial, cross-host, or unverifiable pending events report `writer_liveness = "unknown"` and `human_review_required = true` in `verify` and `repair` preview, and refuse silent `--apply` unless `--acknowledge-unknown-writer` is explicitly provided after human review. See `docs/agent/error-recovery.md` and `docs/agent/command-contracts.md`.

## Explicit actor identity on claim and workspace boundaries (#1737)

`todo::claim_task_with_lease` (covering CLI `decapod todo claim`, cloud
`TodoCommand::Claim`, `todo renew`, `todo yield`, `todo heartbeat --autoclaim`,
`todo worker-run`, `todo add-owner`, and explicit `--owner` on `todo add` /
`todo edit`), `workspace::ensure_workspace` (CLI `decapod workspace ensure`
and RPC `workspace.ensure`), and `core::mentor::reflection::custody` reject
missing, whitespace-only, or `"unknown"` actors via
`core::authoring::is_missing_or_unknown_actor` before mutating task custody or
creating a branch/worktree, failing closed with `AUTHORING_ACTOR_REQUIRED` and
explicit `DECAPOD_AGENT_ID=<actor>` guidance.

Read-only inspection and intake surfaces (`todo list`, `todo get`, `todo show`,
`workspace status`, `docs ingest`, `eval`) and ownerless `todo add` remain
usable when `DECAPOD_AGENT_ID` is unset. For pre-existing tasks whose
`assigned_to` is `"unknown"`, an explicit non-`"unknown"` actor can adopt the
task via exclusive `todo claim` or `workspace ensure --branch` when
`authoring_bindings` holds no real-actor checkout custody and `task_owners` holds
no other real primary owner; adoption emits `task.release` with reason
`unknown_actor_adopted` and advances `lease_generation` (`lease_lifecycle = "reclaimed"`).

## Acceptance observation export (#1757)
`decapod trace flight-recorder acceptance --capture <private-index>` reads
`decapod.agent-acceptance.v1` capture data and prints a redacted, versioned event
manifest. It reports observed autonomous admission, historical PR identity,
current applicability, interventions, source classes and unresolved findings.
Both publication_ready and merge_authorized are always false. Captured original
objective identity, executable/repository binding and evidence hashes are
mandatory; agent declarations cannot stand in for tool/kernel observations.
Consumers must preserve private-capture and cooperative-host assurance limits.

## Audit evidence and invocation scope (#1069)

The refreshed implementation map changes no CLI/RPC interface. Existing native
context, scope/admission, observed state, validation and publication surfaces
remain the integration contract. A runtime must invoke them; instruction files
and hooks cannot intercept arbitrary conversation or shell mutation.

The map distinguishes actual CLI prerequisite admission from complete runtime
proof in a fixture transport, source-CI execution from a full native receipt,
physical recovery from task completion and publication eligibility from merge
authority. It preserves the pending native reference integration and independent
service/forge enforcement boundaries. Source-bound diagnostic links do not waive
any required command, provider, custody or current-target requirement.

## Accepted v0.146.0 audit reconciliation

This map now uses accepted master `e9c40e929ee33aa5443c38b57276f96111db0b92` and the installed 0.146.0 kernel. The accepted #1699 bounded escalation CLI/RPC reference is implemented and covered by its exact-source execution; it is not complete external runtime adoption, semantic reasoning proof or the ten-worker A–H demonstration. Prior baseline paragraphs remain historical audit context. Pending #1697 physical recovery and #1702 canonical local-target admission are kept separate from accepted enforcement until individually verified and merged. Source-CI diagnostics do not substitute for the unchanged full native provider contract.

## Accepted repository-local contract changes (#1658, #1738)

The existing validation runtime returns the actionable
`ACCEPTED_REPOSITORY_CONTRACT_CHANGED` blocker when a candidate removes or replaces
accepted local obligations or updates `reviewed_sources_digest`, even with
internally consistent new review digests. The check precedes execution-provider
selection. When a legitimate transition is required, `verify_accepted_transition`
in `src/decapod/core/repository_profile.rs` computes the deterministic
64-hex-character `repo_policy_transition_id` bound to
`(accepted_base_sha, candidate_head_sha, accepted_sources_digest,
candidate_sources_digest, accepted_commands_digest, candidate_commands_digest)`
under scope `"repo.pr-readiness.v1"` (`POLICY_TRANSITION_SCOPE`), and checks the
canonical main `.decapod/data` policy ledger (`policy::policy_db_path`,
checked read-only via `has_live_scoped_approval_on_conn`) for an active,
bounded-lifetime, non-revoked approval granted via `PolicyCommand::Approve`
(`"policy.control"`).

When no matching active approval exists, `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`
includes the old-to-new digests and the exact
`decapod govern policy approve --id <transition-id> --scope repo.pr-readiness.v1`
command. Once approved by a `policy.control`-authorized maintainer,
`validation_runtime` and `workspace::canonical_local_readiness_repository`
admit the reviewed `repo.pr-readiness.v1` contract into the standard
`required_catalog` and execution pipeline so all transitioned required checks
run against the exact candidate before `workspace publish`. Additional required
checks under an unchanged source inventory and initial local adoption continue
without transition approval; `REQUIRED_PROFILE_DOWNGRADE` from
`decapod.pr-readiness.v1` remains unconditionally blocked.

### Source-definition changes

`ACCEPTED_REPOSITORY_CONTRACT_CHANGED` also identifies unapproved replacement of
the accepted local source-definition digest. Recomputing a candidate digest
cannot self-authorize changed inventoried check scripts, build/dependency
definitions, or provider inputs. Retain the accepted inventory or obtain a
`policy.control` transition approval bound to the exact base, head, and source/command
digests before revalidation; no override or second policy store is added.

## Target-scoped repository proof (#1740)

- `ValidateCli` (`src/decapod/cli.rs`) accepts `--target <TARGET>` and `ValidateRunParams` (`src/decapod/core/rpc.rs`) accepts optional `target: Option<String>` for `repo.pr-readiness.v1` (`validation_buildkite_preflights = false`). `--target` is rejected for `govern project-check`, `--bypass-validation`, Buildkite snapshot handoff flags, and `decapod.pr-readiness.v1` (`TARGET_SCOPE_UNSUPPORTED` / `READINESS_INVOCATION_INVALID` / `PROJECT_CHECK_SCOPE`).
- `RepositoryContract.targets` (`BTreeMap<String, DeploymentTargetContract>` in `src/decapod/core/repository_profile.rs`) declares reviewed `commands`, `depends_on`, `shared_inputs`, and optional `platforms` (`CANONICAL_PLATFORM_KEYS`). Empty `targets` preserves pre-#1740 `command_contract_digest` values byte-for-byte; non-empty `targets` is included in `command_contract_digest`.
- `ValidationReport` (`src/decapod/core/validation_runtime.rs`) exposes `target: Option<String>` and `resolved_targets: Vec<String>`, emits `scope = "repository_target_readiness"` (or `"repository_local_git_target_readiness"`), and forces `publication_ready() == false` whenever target-scoped. `completion/native-readiness-receipt.json` binds `"scope"`, `"target"`, `"resolved_targets"`, and `material.head_revision` inside `"receipt_hash"`.

## Workspace diagnosis (#1071)

`decapod workspace diagnose --json` returns
`decapod.workspace-diagnostic.v1` and a deterministic exit status.
`--operation workspace.authoring` selects a local eligibility check.
`--offline` reports remote dimensions unavailable without contacting the
forge. The default publication diagnosis lists blockers and sends agents
to the live publisher for exact validation and readback.

## Readiness and evidence scope

Cold state readers expose instruction identity and revision. Preparation, approval, execution checkpointing, validation and publication apply the same recorded blocker and remediation. An actual native-call observation binds its terminal result to the current candidate, accepted base and policy, instructions, task custody and measured producer. Retained denial evidence remains available after a legitimate repair. Publication verifies final head and live target independently. Hosted or unobserved effects retain unresolved coverage.

## Exact release projection interpretation

A source-development manifest binds the immutable entrypoint bytes in
`content_hash` and the renderer at `decapod_release` in `template_hash`
and `fingerprint`. The release verifier recomputes both bindings and checks
the refreshed entrypoint bytes against the classified release head.

The release projection checker remains read-only and returns a failure for any unexpected projection. Under verified source-development policy, entrypoint versions may differ from the package or spec-manifest release while their committed bytes and fingerprints remain canonical. Even another well-formed, correctly fingerprinted header is rejected if it differs from that immutable release head. Foreign origin, altered payload, configuration/source/dependency changes and unknown manifest drift do not obtain an exception. Ordinary consumers continue to require the evaluating binary's exact release templates. No RPC schema, producer authority or required CI recipe is added or relaxed.

## Versioned encoding-plane artifacts

`docs/conformance/json-v1.md` defines encoding profile1.0.0, with a machine-readable vector schema at `assets/schemas/interop-json-vectors.schema.json` and pinned raw inputs at `tests/fixtures/interop_json_vectors.v1.json`. Both consumers receive the same original text, never expected answers or a reserialized input. `accepted` and `invalid_json` describe decoding only. The existing `interop.project` result still labels caller-supplied data and `canonical_state_verified=false`. Numeric decoding does not promise arbitrary precision, canonical numeric hashes or authorization.

The accepted v0.154.0 package epoch (`0f44f780`) is integrated before encoding validation. The conformance harness obtains executable identity through the documented read-only `decapod system version` command; unsupported top-level version flags are not execution evidence. Native regeneration supplies the current release projections, while the independent decoder and all pinned raw inputs remain separate from authorization and full native admission.

## Scoped proof recovery diagnostics and workunit checkpoint refresh (#1892)

- **`qa verify recover` / `qa verify regen` failure JSON & stderr (`src/decapod/plugins/verify.rs`)**:
  - `SAFE_VALIDATION_SCOPES` includes `"repository_target_readiness"` and `"repository_local_git_target_readiness"`.
  - `SAFE_BLOCKER_LABELS` and `strip_error_wrappers` expose concrete `EXECUTION_STATE_CHANGED`, `EXECUTION_*_CHANGED`, `VALIDATION_HISTORY_TASK_STALE`, `VALIDATION_HISTORY_TASK_UNBOUND`, `TASK_*`, `TASK_JOURNAL_INCOMPLETE_HISTORY`, `WORKUNIT_*`, and `VALIDATION_LEASE_TTL_INSUFFICIENT` codes ahead of `READINESS_BLOCKED`, while filtering `VALIDATION_LEASE_TTL_LOW:` advisory lines on `stderr`.
  - Readiness blockers mirrored into `report.failures` (`"publication_readiness: ..."`) retain `stage = "readiness"` / `"post_validation_readiness"` and readiness scope when no non-readiness project gate failed.
- **Actionable execution-binding recovery (`src/decapod/core/trajectory/execution.rs`, `src/decapod/core/todo.rs`, `docs/agent/error-recovery.md`)**:
  - `verify_execution_binding` classifies `EXECUTION_STATE_CHANGED` into the changed dimension (`EXECUTION_POLICY_CHANGED`, `EXECUTION_TASK_CUSTODY_CHANGED`, `EXECUTION_MATERIAL_CHANGED`, `EXECUTION_PLAN_OR_INTENT_CHANGED`, `EXECUTION_GOVERNANCE_OR_TRAJECTORY_CHANGED`) and surfaces `decapod govern workunit checkpoint --task-id <id> --operation-id <id>` / `decapod govern trajectory checkpoint` so lease renewals or independent policy approvals can refresh active workunit custody without altering Git `HEAD` or historical `done` task completion timestamps.

## Configuration and badge surfaces (#1838, #1023)

- **`[governance].publication_deny_identifiers` (`src/decapod/cli.rs`)**: Optional string array in `.decapod/config.toml` (omitted when empty) listing literal identifiers blocked from authored commit messages and non-config diffs during `decapod validate`; private operator strings remain in untracked `.decapod/data/publication-deny.txt`.
- **`[init].readme_badge` (`src/decapod/cli.rs`, `src/decapod/lib.rs`)**: Boolean flag defaulting to `true`; when enabled, `decapod init` idempotently writes/updates the installed-version Decapod badge in `README.md` (unless `README.md` is missing, listed in `[governance].protected_paths`, or already links `assets/badge/decapod-governed.svg`).
- **Static SVG badge (`assets/badge/decapod-governed.svg`, `README.md`)**: Accessible flat SVG badge (`role="img"`, `aria-label="Decapod: governed"`) linked and documented in `README.md`.

## Houseboat CLI, Darkmatter Read-Only Snapshot, and TypeScript/Pi Interfaces

- **CLI (`decapod houseboat ...` wired in `src/decapod/cli.rs` & `src/decapod/lib.rs`)**:
  - `decapod houseboat status`: emits `decapod.houseboat.v1` JSON listing active Houseboat engine subsystems and issue mappings.
  - `decapod houseboat verify-darkmatter-snapshot --file <path>`: validates a `decapod.darkmatter-readonly-snapshot.v1` fixture for read-only flags, repository-bounded relative paths, cryptographic evidence digests, and intent-to-evidence traceability.
  - `decapod houseboat storage-benchmark --tasks <n>`: runs a local SQLite task/lease/compaction/GC/reclaim workload in an isolated temp directory and emits `decapod.storage-compaction-benchmark.v1` JSON with explicit `hosted_backend_status = "unavailable_local_only"` and `unknown_fields`.
  - `decapod houseboat verify-mooring [--file <path>] [--now-epoch <u64>]`: evaluates a `decapod.mooring-contract.v1` contract at a read boundary and prints the structured `MooringVerdict`.
- **Darkmatter Read-Only Contract (`docs/architecture/darkmatter-readonly-contract.md`, `tests/fixtures/darkmatter_readonly_snapshot.v1.json`)**:
  - Schema `decapod.darkmatter-readonly-snapshot.v1` mandates `read_only = true`, `mutation_endpoints_exposed = false`, `ui_generated_proof_allowed = false`, and repository-relative paths only.
- **TypeScript Binding & Pi Extension (`bindings/typescript/`, `packages/pi-decapod/`)**:
  - `DecapodClient` (`bindings/typescript/src/index.ts`): concurrent, `AbortSignal`-cancellable wrapper around `decapod` CLI/RPC with per-instance `cwd`, `agentId`, and `sessionPassword` isolation and zero `process.exit` calls.
  - `createPiDecapodExtension` (`packages/pi-decapod/src/index.ts`): captures raw human prompts separately from agent interpretation across model/harness switches, enforces preparation/pre-mutation/validation-admission gates without equating plan drafting to human approval, and classifies unobserved direct host writes as `outside_native_enforcement`.

## Wave 7 Shard A Interface & Contract Updates (#1079, #1122, #1047, #1134, #1388, #1672, #1879, #1890)

- **Canonical manifest path**: `.decapod/managed/.manifest.json` (`LOCAL_PROJECT_SPECS_MANIFEST`); legacy `.decapod/managed/specs/.manifest.json` remains readable via `project_specs::resolve_specs_manifest_path` until refreshed (`#1079`).
- **Pre-build validation surface (`decapod validate --pre-build` & CI `pre-build-validate`)**: Validates `.decapod/config.toml`, `.decapod/governance.json` (`claims`, `sections.plan`, `sections.trajectory`), `.decapod/managed/.manifest.json` JSON + 64-hex SHA-256 `repo_signal_fingerprint`/`config_input_hash`/`spec_input_hash`/file `content_hash`, and configured agent entrypoint `decapod-release` + 64-hex `decapod-fingerprint` markers (`#1047`, `#1134`).
- **Source-CI check IDs**: `pre-build-validate`, `selective-core-1`, `selective-core-2`, `selective-core-3`, `selective-core-status` (`Tests (core)`), `selective-cli-1`, `selective-cli-2`, `selective-cli-3`, `selective-cli-status` (`Tests (cli)`), and `selective-plugins` (`Tests (plugins)`).
- **Configured strict preflight carrier (`.decapod/config.toml`)**: `[proof].validation_buildkite_preflights = false` and `[proof].validation_github_actions_preflights = true` with `[proof.required_profile].id = "decapod.pr-readiness.v1"` and reconciled `reviewed_sources_digest` (`#1890`).

<!-- decapod:codebase-contract:start -->

## Codebase Contract Snapshot

- CLI/RPC/protocol surfaces (23): `assets/tools/laya-native/src/lib.rs`, `assets/tools/laya-native/src/main.rs`, `bindings/typescript/README.md`, `bindings/typescript/package.json`, `bindings/typescript/src/index.ts`, `bindings/typescript/src/node-env.d.ts`, `bindings/typescript/test/client.test.ts`, `packages/pi-decapod/README.md`, `packages/pi-decapod/package.json`, `packages/pi-decapod/src/index.ts`, `packages/pi-decapod/test/pi_extension.test.ts`, `src/decapod/cli.rs`, `src/decapod/core/agent_interop/a2a.rs`, `src/decapod/core/agent_interop/mcp.rs`, `src/decapod/core/interop.rs`, `src/decapod/core/observability_projections/server.rs`, (+7 more)
- Manifest authority path: `.decapod/managed/.manifest.json`
<!-- decapod:codebase-contract:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `4e955f6c41bf1868484fbc86a8bfcd78c8601c5ca1064830adf69363000273e3`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `bindings/` (5 files), `docs/` (3 files), `packages/` (4 files), `src/` (284 files), `tests/` (283 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

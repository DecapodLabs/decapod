# Security

## Section 12 Host Integration, Caller Isolation, and Privacy Boundaries (#875, #1277, #1095, #880, #1888)

- **Harness packaging isolation (`src/decapod/core/agent_interop/registry.rs`)**: `install_harness_package` and `remove_harness_package` refuse symlink targets (`symlink_metadata`), verify version skew (`version_skew`), preserve unrelated user MCP/hook entries via `decapodManaged = true` tagging, and construct harness config directory literals safely without bypassing repository namespace gates.
- **Per-caller Rust/N-API isolation (`src/decapod/lib.rs`, `bindings/typescript/native/src/lib.rs`)**: `execute_library_rpc` requires an explicit non-empty `agent_id`, keeps `allow_process_session_cache = false` so concurrent in-process callers cannot share `SESSION_P_VAL` or mutate process-wide CWD/env state, enforces `RPC_OPS` membership and `session_required` / worktree isolation gates identically to the CLI, and polls `AtomicBool` cancellation without terminating the host process.
- **Pi lifecycle enforcement (`packages/pi-decapod/src/index.ts`)**: `registerPiDecapodHooks` runs prompt safety `evalPrompt` on `input`, injects governed context on `before_agent_start`, and denies protected-branch writes in `tool_call` before tool execution.
- **Empirical telemetry & credential restraint (`#1095`, `#880`, `#1888`)**: Paired Antigravity study artifacts under `docs/research/host-outcome-pilot/` remain local-only redacted summaries (no raw transcripts, no secret scraping, no default uploads, and `reviewed_active_human_minutes = null` rather than fabricated human-attention claims); unauthenticated/quota-limited hosts (`claude` `loggedIn: false`, `codex-cli` usage limit, `opencode` unauthenticated) fail closed cleanly without credential harvesting.

## Source-CI executor mode & stale accepted-base reconciliation boundaries

- **Executor definition vs. carrier Git mode boundary (`src/decapod/core/validation_runtime/executor_contract.rs`)**: `is_valid_definition_entry` admits `100755 blob ` strictly for `.sh` paths in `profile::definition_files()`, while non-`.sh` definitions, `assets/tools/source_ci_custody.rs` protocol markers, and `verify_carrier_modes` strictly require `100644 blob `, rejecting symlinks (`120000`), arbitrary modes, and unauthorized byte mutations.
- **Stale accepted-base inventory reconciliation boundary (`src/decapod/core/repository_profile.rs`)**: `qualifies_stale_accepted_base_inventory_reconciliation` admits `repo.pr-readiness.v1` transitions with a stale `accepted_base` `reviewed_sources_digest` only when `base_oid != head_oid`, `commands_preserved && candidate.reviewed_commands_digest == accepted.reviewed_commands_digest`, both `base_tree.inventory.digest()` and `head_tree.inventory.digest()` equal `candidate_sources_digest`, and `compute_inventory_diff_paths(&base_tree, &head_tree)` is empty, failing closed on any candidate modification to workflows, scripts, manifests, Git file modes, or proof commands.
- **Selective shard-status effective context boundary (`src/decapod/core/validation_runtime/snapshot/source_policy.rs`)**: `verify_non_integration_effective_context` enforces exact 7-key binding (`SOURCE_CI_*` plus `CUSTODY_PREPARE_RESULT="success"` and `CUSTODY_WORKER_RESULT="success"`) strictly for `selective-core-status` and `selective-cli-status`, while all other non-integration checks remain locked to the exact 5 `SOURCE_CI_*` keys.

## Fail-closed proof-reference boundary for `repo.pr-readiness.v1` transitions (`#1949`)

- **Conservative reference, ecosystem coupling & unreadable-blob fail-closed guards (`src/decapod/core/repository_profile.rs`)**:
  - `qualifies_non_proof_source_inventory_transition` never waives `accepted_commands`, `accepted_contract` (including `reviewed_commands_digest`, `executions`, `source_coverage`, `targets`), workflows, build-system inputs (`Makefile`, `Justfile`, `CMakeLists.txt`, Bazel, Nix, `rust-toolchain*`, `tox.ini`), or `catalog_source_paths()`.
  - `collect_proof_relevant_blobs` performs a bounded BFS over `accepted_base_sha` seeded by all non-exempt or contract-referenced files, failing closed (`None`) on non-UTF-8 / oversized blobs and propagating exact path matches, directory globs (`dir/*`, `dir/**`), directory token matches, `same_dir_helper_referenced_in_text`, and `ecosystem_is_coupled_to_text` (`Rust`, `Node`, `Python`, `Go`, `Container`) across multi-hop helper and manifest chains.
- **No agent privilege elevation or manual CLI dead-end**: `decapod.policy.accepted_repository_contract` retains `OverrideEligibility::NonOverridable`, `autonomous_remediation_permitted = false`, and `human_authorization_required = true` when proof-relevant inputs or command contracts change, shifting `agent_guidance.owner` to `"human"` and directing PR review or cryptographic/Cloud approval envs without weakening RBAC/policy gates or prompting humans for manual `decapod govern policy approve` commands.

## Dactyl `0.19.1` dependency security & transitive floor enforcement (`#1943`, `#1944`)

- **Zero-advisory lockfile reconciliation (`dactyl-db 0.19.1`)**: Upgrades `dactyl-db` to `0.19.1` while asserting `quinn-proto >= 0.11.19`, `quinn-udp >= 0.5.16` (RUSTSEC-2026-0538), and `rustix >= 1.1.6` (RUSTSEC-2026-0486) across all `Cargo.lock` entries (including inactive feature edges) and `cargo audit` zero-vulnerability gates.

## Diagnostic telemetry privacy, redaction, and non-authority invariants (`src/decapod/core/telemetry.rs`, `#1095`, `#1944`)

- **Strict non-authority**: `DiagnosticTelemetryReport` sets `read_only = true`, `authoritative = false`, `can_grant_authority = false`, and `diagnostic_only = true`; telemetry counters, quantiles, and freshness summaries never authorize validation, completion, or publication.
- **Fail-closed identifier suppression & per-instance salted anonymization**: Hashing an identifier does not exempt its observation, retention, or correlation from authorization requirements. `TelemetryRetentionPolicy::allow_identifier_observation` defaults to `false`, dropping caller-supplied `repo_identifier` values (`repo_digest = None`) and skipping workspace directory name reads in `collect_workspace_diagnostic_telemetry` and `aggregate_telemetry_samples`. Even when explicitly authorized (`allow_identifier_observation = true`), `TelemetryRingBuffer` computes `anonymize_identifier_with_salt` using a fresh per-instance 32-byte CSPRNG `ephemeral_salt` (`decapod.telemetry.anon.v2:`), preventing deterministic cross-session fingerprinting or static-salt correlation under Trust Invariant #2 (`No identity fingerprinting`).
- **Negative text redaction**: `sanitize_telemetry_text` layers over `core::trace::redact_string` to strip multi-line blobs, prompts/raw evidence (`prompt`, `system_prompt`, `user_prompt`, `raw_evidence`, `transcript`, `stdout`, `stderr`, `diff --git`), filesystem paths, URLs, email addresses, key-value tenant/customer/repo tags, and `owner/repo` slugs, bounding label length to `64` bytes.
- **Cardinality & retention bounds**: `TelemetryRetentionPolicy::clamped` enforces hard ceilings (`512` samples, `32` distinct stages, `128` time buckets, `7`-day retention) to prevent unbounded memory or label-cardinality exhaustion.

## Ecosystem Permanent Trust & Egress Gate (#1944, #1933)

- **Trust & Egress Policy Contract (`docs/trust-and-egress-inventory.md`, `src/decapod/core/trust_egress.rs`)**:
  - Establishes permanent trust enforcement across nine invariants: `Identity sovereignty`, `No identity fingerprinting`, `Credential isolation`, `Consent persistence`, `Effective revocation`, `Relay transparency`, `Distribution integrity`, `Privacy contract integrity`, and `Incident accountability`.
  - Enforces a zero default outbound network egress baseline for Decapod CLI core initialization, idle state, session acquisition, capabilities inspection, todo workflows, RPC dispatch, and pre-build validation (`decapod validate --pre-build`): `decapod session acquire` (`src/decapod/lib.rs`) only runs crates.io update checks when `project_config.repo.session_auto_update` is explicitly `true`, and `generate_capabilities()` (`src/decapod/core/rpc.rs`) only queries crates.io when `DECAPOD_CHECK_LATEST=1` is explicitly set.
  - Strictly isolates local observability projections (`ExecutionTelemetryProjection` in `src/decapod/core/observability_projections/telemetry.rs`, `read_only = true`, `authoritative = false`, no default upload) as local-only diagnostics; outbound transmission of derived or hashed machine/credential identifiers is prohibited under the `No identity fingerprinting` rule.
  - Evaluates and enforces persisted consent and immediate revocation via `trust_egress::ConsentRevocationState` loading `<repo>/.decapod/config.toml` via `DecapodProjectConfig::load` (`backend` and `repo.session_auto_update`), so revoking optional egress stops outbound traffic, clears queued outbound state immediately, and remains halted across schema/state reloads and process restarts.
  - Enforces declared HTTPS/loopback allowlisting (`trust_egress::validate_egress_url`) with strict host/port validation across runtime outbound URL boundaries (`CurlTransport::request_with_diagnostics` in `src/decapod/core/propodus.rs`, `Context::curl` in `src/decapod/core/derived_release_driver.rs`, `get_latest_version` in `src/decapod/core/rpc.rs`, `fetch_latest_crates_version` in `src/decapod/lib.rs`, and `diagnostic_read_pr` URL construction in `src/decapod/core/workspace_publication.rs`).
  - Rejects credential/secret ingestion or leakage (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `STRIPE_SECRET_KEY`, `GITHUB_TOKEN`, Codex CLI `auth.json`, `~/.claude.json`, `~/.config/gh/hosts.yml`), undeclared outbound hosts, and non-HTTPS remotes via `tests/trust_egress_gate.rs` (including negative-control counter-tests and static inventory completeness verification).

## Dependency security audit remediation (`quinn-udp` RUSTSEC-2026-0538 & `rustix` RUSTSEC-2026-0486, #1943)

- **Lockfile-only minimal remediation (`Cargo.lock`, `MODULE.bazel.lock`)**:
  - Updated transitive `quinn-udp` from `0.5.14` to `0.5.16` (`>=0.5.16, <0.6.0`, resolving [RUSTSEC-2026-0538](https://rustsec.org/advisories/RUSTSEC-2026-0538)) and `rustix` from `1.1.4` to `1.1.6` (`>=1.1.6`, resolving [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486)) without relaxing or suppressing any `cargo audit` checks.
  - Verified `quinn-udp` is an inactive optional dependency of `reqwest 0.12.28` (`http3` feature disabled in `dactyl-db 0.19.0` / `decapod`) while keeping `Cargo.lock` and Bazel `MODULE.bazel.lock` (`//:cargo_inputs_policy`) clean (`cargo audit`: 0 vulnerabilities, 0 warnings).
  - `tests/release_dependency_policy.rs::locked_quinn_udp_and_rustix_preserve_patched_floors_including_inactive_entries` enforces the patched floors across future lockfile updates while preserving `dactyl-db = "=0.19.0"`.

## Wave 7 compaction, delegated RBAC generation checks, knowledge attestation, and TypeScript/Pi trust boundaries (#1630, #1568, #657, #1277, #1711)

- **Fail-closed compaction & explicit-only page reclamation (`src/decapod/core/storage_compaction.rs`)**:
  - `compact_datastore_lossless` refuses `backend = "cloud"` (`DATA_COMPACT_CLOUD_UNSUPPORTED`) rather than touching local SQLite, runs zero mutations under `--dry-run`, executes `VACUUM` strictly when `--reclaim-pages` is explicitly requested, and aborts/rolls back if pre/post logical event count or SHA-256 digests diverge.
- **Ambiguity refusal & Propodus `037` delegated grant revocation/generation checks (`src/decapod/cli.rs`, `src/decapod/core/db_maintenance.rs`)**:
  - Configuring multiple maintenance policy tables (`[governance.db_maintenance]`, `[maintenance]`, `[agent_maintenance]`) fails closed with `AMBIGUOUS_MAINTENANCE_POLICY`.
  - `evaluate_cloud_capability_with_delegated_grant` fails closed on missing server grant (`CLOUD_DELEGATED_GRANT_REQUIRED`), cross-repository mismatch (`CROSS_REPO_AUTHORITY_DENIED`), revocation (`CLOUD_DELEGATED_GRANT_REVOKED`), expiry (`CLOUD_DELEGATED_GRANT_EXPIRED`), zero or mismatched `revocation_generation`/`policy_generation`/`session_generation` (`CLOUD_DELEGATED_GRANT_GENERATION_STALE`), or operations outside the monotonically narrowed grant (`CLOUD_DELEGATED_OPERATION_NOT_GRANTED`).
- **Knowledge provenance integrity & anti-self-promotion (`src/decapod/plugins/knowledge.rs`)**:
  - Write-time `knowledge.write_attestation` events bind repository-relative `file:` digests without widening advisory records into binding policy (`authority = "advisory_below_current_intent_and_policy"`). Superseded bodies never enter operative context; `hurt` outcome feedback surfaces visible caution without allowing `helped` counts to bypass the procedural promotion firewall.
- **TypeScript/Pi & Darkmatter containment (`bindings/typescript/src/index.ts`, `packages/pi-decapod/src/index.ts`)**:
  - `@decapodlabs/decapod-client` keeps per-client isolated env/cwd with zero `process.exit` calls and explicitly reports `cli_process` fallback rather than silently claiming native N-API loading; `validateDarkmatterSnapshot` rejects `read_only != true`, `mutation_endpoints_exposed != false`, `ui_generated_proof_allowed != false`, `..`/absolute path escapes, and non-`sha256:` evidence digests.
  - `@decapodlabs/pi-decapod` refuses autonomous mutation, resume, and completion settlement while a Decision Gate or unresolved conflict is open or repository proof is unverified, and labels unobserved direct host writes `outside_native_enforcement`.

## DPoP sender-constrained gateway custody, container signer isolation & `dactyl-db 0.19.0` fail-closed boundaries (#1878)

- **Sidecar DPoP session binding & fail-closed revocation (`src/decapod/core/cloud_session.rs`, `src/decapod/core/cloud_client.rs`)**:
  `device_session.json` shares the checkout's `SessionStore` binding and atomic private-file permissions; copying a device session across different `(origin, repository, checkout)` bindings fails closed. Refresh reuse (`refresh_reuse_detected`), `device_revoked`, `device_key_expired`, and `record_rejected_credential` clear the sidecar device session without falling back to SQLite or downgrading to unsigned Bearer in strict `DECAPOD_CLOUD_AUTH_SCHEME=dpop` mode. `cloud_client::revoke` separates DPoP-signed `action=logout` (using the active `dpc_at_` sidecar + enrolled device signer) from human-bearer management revocation (`revoke_all` / `revoke_key`).
- **Container signer boundary (`src/decapod/core/container_signer.rs`)**:
  Admits `/cloud-control` alongside `/cloud-query` and `/cloud-batch` via the host-mounted Unix domain socket so containerized workspaces sign operational Cloud control RPCs without ever receiving private key bytes or raw credentials, while explicitly denying all six new `dactyl-db 0.19.0` human-bearer admin operations (`/repository-enroll`, `/resource-limits`, `/org-admins`, `/org-recovery`, `/policy-templates`, `/org-upgrades`) as well as `/device-*` and `/billing-*`.
- **Direct wire error-code forwarding, strict URL validation & privacy (`dactyl-db 0.19.0`, `cloud_client.rs`, `cloud_admin.rs`)**:
  Upgrading to `dactyl-db 0.19.0` closes the prior `0.18.0` gap by directly forwarding allowlisted HTTP error codes (`mfa_step_up_required`, `custody_conflict`, `owner_protected`, `member_not_found`, `invitation_expired`, `invitation_not_found`, `project_not_found`, `seat_revoked`, `grant_revoked`, `grant_expired`, `scope_denied`, `dpop_aud_mismatch`, `version_conflict`, `idempotency_conflict`, `idempotency_in_progress`, `transaction_aborted`) and validating origin-bound `EffectProvenance` headers. `enroll_repository` validates returned `checkout_url`/`enrollment_url`/`portal_url` via `validate_https_portal_url`, `local_status` never leaks physical origin URLs, and `DECAPOD_CLOUD_FIRST_PARTY_ORIGIN=1` remains explicit opt-in until custom-domain cutover is verified.

## Read-only observability containment, redaction, and authenticated HTTP transport hardening (#874, #1719, #1720, #1728, #1732)

- **Repository containment & adversarial file defense (`src/decapod/core/observability_projections/snapshot.rs`, `specs.rs`)**:
  - `validate_repo_bounded_path` rejects absolute paths, `..` traversal, NUL bytes, and symlinks at every ancestor and leaf component (`SYMLINK_BOUNDARY_VIOLATION`), and opens regular files with `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC` on Unix.
  - `is_secret_sensitive_path` blocks `.git/*`, `.decapod/managed/sessions/*`, `.env*`, `*.pem`, `*.key`, `id_rsa`, `credentials*`, and similar sensitive paths (`SECRET_PATH_DENIED`); `read_source_anchor` rejects binary files, enforces hard byte ceilings, redacts secret patterns and absolute paths, and post-checks file hashes for concurrent edits.
  - `inspect_spec_text_security` flags control characters, prompt-injection markers, and embedded secrets in untrusted spec prose without mutating files.
- **Provenance redaction & cross-scope isolation (`src/decapod/core/observability_projections/provenance.rs`)**:
  - Strips `raw_prompt`, `user_prompt`, `system_prompt`, `thinking`, `chain_of_thought`, `private_reasoning`, `api_key`, `token`, `password`, `authorization`, and related keys before emitting consultation metadata, preserves `agent_reported` vs `trusted_host` origin trust (preventing consultations from self-authenticating human prompts), and rejects mismatched repository scope (`CROSS_SCOPE_DENIED`).
- **Transport authentication, replay binding, and mutation refusal (`src/decapod/core/http_transport.rs`, `src/decapod/core/observability_projections/server.rs`, `src/decapod/lib.rs`)**:
  - Constant-time Bearer verification (`verify_bearer_token`) over SHA-256 digests and exact byte slices; `--token-stdin` avoids leaking credentials into process argv; loopback-only default binding fails closed on non-loopback addresses without `--allow-remote`.
  - Rejects foreign `Origin` (`403 forbidden_origin`) and non-loopback `Host` (`403 forbidden_host`) headers, cross-tenant (`403 cross_tenant_denied`), cross-repository (`403 cross_repo_denied`), and revoked token IDs (`401 token_revoked`), and enforces read-only credentials cannot execute any mutating operation (`403 read_only_enforced` / `405 method_not_allowed`).
  - Fixes `#874` idempotency confusion by binding `Idempotency-Key` to the SHA-256 digest of method, path, query, RPC op, raw body bytes, repository context, caller/session/tenant/repo headers, and hashed token, capping replay storage by both entry count and byte weight (`503` on capacity exhaustion within window), preventing blind replay of uncertain outcomes (`409 unknown_outcome_reconcile_required`), and executing child RPC through Unix process-group `bounded_capture_with_stdin` so stalled or oversized children are killed and reaped within bounds.

## Maintenance RBAC boundaries, non-destructive legacy recovery, read-only analysis isolation, and non-forced publish guards (#1568, #1657, #1279, #1071, #1064, #1047)

- **Maintenance capability scoping & conflict rejection (`src/decapod/core/db_maintenance.rs`)**: `db.wal.checkpoint` is excluded from `DEFAULT_SAFE_ALLOWLIST`, requires explicit opt-in from canonical main-repository config, executes `PRAGMA wal_checkpoint(PASSIVE)` on Dactyl's write path (`Connection::execute`) with a 5000ms ceiling (never `TRUNCATE`/`RESTART` or sidecar deletion, preserving Dactyl's `sqlite3_stmt_readonly` read guard), and refuses dual `[governance.db_maintenance]` + `[maintenance]` declarations (`AMBIGUOUS_MAINTENANCE_POLICY`). Worktree backend tampering continues failing closed with `BACKEND_MISMATCH`; Cloud backends remain denied (`CLOUD_PRINCIPAL_UNVERIFIED`) pending Propodus `#88`.
- **Fail-closed legacy TODO recovery (`src/decapod/core/legacy_todo_recovery.rs`)**: Live `todo.db` is never opened directly by SQLite; exports refuse overwriting existing files (`create_new(true)` with private permissions); `--import-missing` refuses any conflicting task ID (`LEGACY_TODO_CONFLICT_BLOCKED`) and archives original bytes before transactional journaled insertion; `--quarantine` never touches `decapod.db`; both require canonical `todo.legacy.reconcile` RBAC and active local session authentication.
- **Zero-mutation read-only analysis & hostile input containment (`src/decapod/core/analyze.rs`)**: Dispatched before store/session/lock code so `.decapod/data/decapod.db` SHA-256 and Git status remain byte-identical. Never follows symlinks, excludes sensitive files (`.env*`, `id_rsa`, `*.pem`, `*.key`, credentials) without reading them, scans untrusted instruction files with `agent_eval::evaluate_prompt` without executing repository code or hooks, and redacts all output via `core::trace::redact`.
- **Non-forced publication & pre-build non-substitution (`src/decapod/core/workspace.rs`, `src/decapod/core/pre_build_validate.rs`)**: `workspace publish` rejects `--branch`/`--pr` mismatches, mixed implementation diffs under `--mode governance-only`, and diverged remote heads (`REMOTE_BRANCH_DIVERGED` decision gate — never force-pushes). `validate --pre-build` performs zero writes and unconditionally reports `publication_ready = false` and `proof_ready = false`.

## Multi-tenant Cloud isolation and non-overridable organization policy enforcement (#1895, #877)

- **Tenant & seat isolation without SQLite fallback (`#1895`)**: Cloud sessions remain bound to explicit `(origin, repository, project/org)` context and verified server access. Decapod never receives backend DB names, URLs, or credentials, fails closed on cross-org/stale scopes, seat revocation, paid-seat absence (`#215`), cross-repo `operation_id` replay, or network cuts, and never initializes or modifies `.decapod/data/decapod.db` as a fallback.
- **Overlay integrity, non-weakening, and private-policy redaction (`#877`)**: `src/decapod/core/org_overlay.rs` verifies Ed25519 signatures via `ring::signature::ED25519` and SHA-256 digests, marking invalid bundles `OverlayTrustState::Tampered` and rejecting stale or conflicting overlays fail-closed. `.decapod/OVERRIDE.md` cannot override `non_overridable` controls (`ORG_OVERLAY_NON_OVERRIDABLE_CONFLICT`) or weaken `extend_only` controls (`ORG_OVERLAY_EXTEND_ONLY_WEAKENING_REJECTED`). Candidate config downgrades against accepted HEAD are blocked during validation (`ORG_OVERLAY_POLICY_DOWNGRADE` in `src/decapod/core/validate.rs`), and `organization_private` directive text is omitted from portable context bundles with fail-closed validation (`ORG_OVERLAY_PRIVATE_LEAK_REJECTED` in `src/decapod/core/context_bundle.rs`).

## Decapod Cloud admin secret custody, URL validation, and SQLite isolation (#1896, #1836, #1835, #1877)

- **Zero secret leakage & volatile zeroization (`src/decapod/core/cloud_admin.rs::RedactedSecret`)**:
  `RedactedSecret` trims trailing `\r\n`, rejects empty/whitespace-only or `> 4096`-byte inputs without echoing values, prints `[REDACTED]` in `Debug`/`Display`, omits `Serialize`, and overwrites heap buffers via `cloud_dpop::zeroize_bytes` on drop. `redact_url_fragment` and `redact_json_value` scrub secret fields (`*secret*`, `*token*`, `*password*`, `*credential*`, `authorization`, `apikey`) and `#...` fragments from errors, stderr warnings, and RPC trace/receipt records.
- **Stdin-only secret transport (`cli.rs`, `lib.rs::parse_rpc_request`)**:
  `CloudInvitesCommand::Accept` and `CloudIdentityCommand::ConfirmLink` define only `--secret-stdin` (no secret argv flags exist on CLI). `reject_argv_cloud_admin_secrets` blocks secrets passed via `decapod rpc --op cloud.{invites,identity} --params` CLI arguments before dispatch or tracing, requiring `decapod rpc --stdin`.
- **Strict HTTPS billing portal & invitation URL checks (`validate_https_portal_url`, `validate_https_url_basic`)**:
  `billing portal` enforces explicit `https://` scheme, valid DNS/IPv6 authority, and absence of control/whitespace characters, `@` userinfo, `#` fragments, and credential query keys (`token`, `access_token`, `refresh_token`, `id_token`, `secret`, `client_secret`, `invitation_secret`, `stepup_secret`, `session_secret`, `password`, `pwd`, `api_key`, `apikey`, `private_key`, `bearer`, `auth`, `signature`). `invites create` and `identity prepare-link` validate returned `invitation_url` / authorization URLs as valid `https://` origins without userinfo while redacting fragments on rejection.
- **Single-send uncertain outcome containment & zero SQLite touch**:
  `cloud_admin::account_call` never automatically replays failed mutations; post-send `MutationOutcome::Uncertain` / transport/timeout/unavailable errors on mutating calls emit `CloudAuthStatus::OutcomeUnknown`. Early CLI (`Command::Cloud`) and RPC (`run_cloud_admin_rpc`) paths bypass SQLite startup and local `.decapod/data/decapod.db` / `todo.db` creation. Gap noted: under `dactyl-db 0.18.0`, HTTP 403 error codes outside Dactyl's `public_error_code` allowlist (`mfa_step_up_required` vs `admin_required`) rely on `bearer_aal2_claim` inspection for disambiguation until Dactyl `0.19.0` forwards those explicit codes.

## Effective operation grant anti-widening and TOCTOU invariants (#1905, #1893)

- **Strict monotonic narrowing & denial precedence (`src/decapod/core/effective_grant.rs`)**:
  `EffectiveOperationGrant` never widens past the accepted repository policy (`resolve_repository_policy`), server-verified Cloud scope (`VerifiedCloudScope`), actor trust floor (`required_trust_for_operation`), or parent delegation limits. Explicit denials (`denials`, `denied_operations`, `denied_resources`) compose by union, allowances compose by exact order-independent intersection, budgets compose by `min`, and delegation depth decrements across every parent->child edge.
- **Anti-spoofing & non-self-authorization (`evaluate_effective_grant`, `detect_candidate_policy_widening`)**:
  Rejects `"unknown"`/blank actors (`ActorSpoofingRejected`), copied parent grant references (`CopiedParentGrantRejected`), unrecognized/non-live approval tokens (`ForgedOrUnknownApproval`), `PlanState::Approved` substitution (`PlanApprovalNotAuthorization`), trust-insufficient actors (`TrustLevelInsufficient`), and candidate-branch `.decapod/config.toml` weakening (`CandidatePolicyWideningRejected`).
- **Prestate binding, lease expiry & cumulative sibling capacity**:
  Binds grants via `GrantPrestateBinding` (`repo_fingerprint`, `accepted_base_oid`, `head_oid`, `policy_revision`, `working_tree_digest`, `task_revision`, `lease_generation`, `lease_expires_at`, `session_id`, `harness_id`) so state drift, lease expiry, or harness/session changes invalidate old grants (`GrantDecision::Stale`). Atomic SQLite transactions in `reserve_delegation_budget_on_conn` prevent concurrent or sequential sibling subtasks from exceeding parent cumulative capacity (`CUMULATIVE_DELEGATION_BUDGET_EXCEEDED`), and `Consumed` reservations remain counted after `todo done`.
- **Execution-outcome separation & maintainer-setting isolation**:
  `OperationConsequenceRecord` separates admission (`GrantDecision`) from external effects (`ExecutionOutcomeKind::WitnessedExternal` vs `DecapodMediatedApplied`), while `#1893` regression tests assert maintainer-only development entrypoint-version-drift override settings never surface in agent-facing specs, embedded docs, CLI help, or context capsules.

## Capability contract trust boundaries & tamper resistance (#1906)

- **Accepted policy precedence (`AcceptedCapabilityPolicy`)**:
  Agent-edited local contract copies cannot weaken `auth_scope`, drop `required_error_classes` or `required_negative_case_ids`, disable `blind_replay_forbidden`, or disable `readback_required`; `check_against_accepted_policy` and `validate_repo_contract_boundary` compare against the main/host baseline (`repo_accepted_policy`) and fail closed with `AgentEditedContractCopyDrift` / `DroppedNegativeCase`.
- **Evidence provenance & no blind replay**:
  Evidence from untrusted producers, stale revisions, or `uncertain` / `pending` prior attempts cannot be reused as current proof (`EvidenceProducerUntrusted`, `EvidenceStale`, `EvidenceUncertainNotProof`). Blind replay after a severed acknowledgement is rejected (`SeveredAckBlindReplayAttempt`) and compensation is blocked until authoritative idempotent readback resolves uncertainty or a mutually supported corrective op runs while retaining the original fault.
- **Read-only inspection isolation**:
  `decapod capabilities contracts` and `interop.capability_contracts` never trust caller-supplied contracts as canonical authority (`canonical_state_verified = false`), perform zero network calls, and never write `.decapod/data/decapod.db` or session state.

## Snapshot policy enforcement, heartbeat custody invariants, and freshness measurement bounds (#1881, #1837, #1884)

- **Snapshot authorization & symlink guards (`src/decapod/core/workspace.rs`)**:
  `container_snapshot_policy_blocker` consults `db_maintenance::resolve_canonical_policy` before any container image build so unauthorized actors cannot build containers or receive snapshot launch hints (`test_workspace_ensure_container_reports_default_backup_denial_before_build`). Authorized launch commands verify `! -L .decapod` and `! -L .decapod/backups` before creating mode-`700` `.decapod/backups/container-store.XXXXXX` and clean up the temporary directory on `EXIT`/`HUP`/`INT`/`TERM`.
- **Non-widening lease heartbeat (`src/decapod/core/todo.rs`, `src/decapod/core/task_journal.rs`)**:
  `heartbeat_claim_lease` enforces `todo.claim.exclusive` policy, active exclusive holder equality (`assigned_to == current_agent_id`), expected `lease_generation`, and non-expired lease (`previous_secs > now`), and never shortens an existing deadline. `check_heartbeat_step` verifies `at < old && old < next` and exact projection equality except `lease_expires_at`; any non-heartbeat task/custody/status change still invalidates `check_validation_binding` (`TASK_ADVANCED_DURING_VALIDATION`), and explicit `todo renew` without `--heartbeat` still increments `lease_generation` and invalidates prior proof (`real_lease_renewal_invalidates_proof_from_the_previous_generation`).
- **Single-capture tool digest reuse (`src/decapod/core/native_proof.rs`)**:
  `measure_readonly_toolchains` restricts path-hash reuse strictly to a single `capture_readonly_identity` call after verifying canonical path identity and `native_executable`, while checking `budget.remaining()` for every binding; execution checks and subsequent captures continue reading live tool bytes (`readonly_identity_reuses_shared_tool_bytes_within_one_capture`).

## Portable transition approval verification and target-map non-self-authorization (#1833, #1832)

- **Untrusted candidate isolation (`src/decapod/core/repository_policy_approval.rs`, `src/decapod/core/validation_rules.rs`)**:
  In `ci_local` mode, `evaluate_policy_transition_with_approval` derives the accepted base solely from provider base-branch variables (`GITHUB_BASE_REF`, `BUILDKITE_PULL_REQUEST_BASE_BRANCH`) or `origin/HEAD` / unambiguous `origin/{main,master}` — never candidate `.decapod/config.toml` — and requires candidate working-tree `.decapod/config.toml` to equal committed `{head}:.decapod/config.toml`. `verify_signed` reads `.decapod/repository-policy-signers.json` strictly from `{base}:.decapod/repository-policy-signers.json` using `--no-replace-objects` and `GIT_CONFIG_COUNT=0`, preventing candidate-committed signer lists or ambient Git config from authorizing transitions (`fresh_ci_checkout_requires_independent_approval_after_inventory_upgrade`).
- **Tracked store refusal & Cloud decision binding (`src/decapod/core/repository_profile.rs`, `src/decapod/core/repository_policy_approval.rs::cloud`)**:
  `has_active_policy_transition_approval` fails closed (`POLICY_STORE_UNSAFE`) if `.decapod/data` is tracked in Git on either checkout or main root. Cloud decision verification enforces exact `project_id`, `decision_id`, `capability = "governance.exception.authorize"`, `obligation_id`, `base_commit`, `candidate_commit`, `affected_commit`, `tree_oid`, `current_generation_matches == true`, `authority_basis` (`verified_repo_admin` or `delegated_repo_lead`), and live `expires_at` / `authority_expires_at` bounds (`cloud_decision_requires_exact_live_repository_authority`).
- **Deployment-target preservation & strict-profile lock (`src/decapod/core/repository_profile.rs`, `tests/consumer_readiness.rs`)**:
  `preserve_accepted` requires exact equality on `RepositoryContract.targets` so target additions, dependency edits, or `shared_inputs` expansions cannot alter validation scope without transition approval, while downgrading `decapod.pr-readiness.v1` remains unconditionally blocked (`REQUIRED_PROFILE_DOWNGRADE`) even when a candidate carries an approved transition ID (`decapod_strict_profile_downgrade_remains_unconditionally_blocked_even_with_approval`).

## Cloud Wave 2 keystore custody, DPoP logout isolation, and fail-closed container relay (#1836, #1835, #1886)

- **Cross-platform OS keystore & non-secret inventory (`src/decapod/core/cloud_dpop.rs`)**:
  Private Ed25519 keys remain external to the repository across macOS Keychain,
  Linux `secret-tool`, Windows DPAPI Credential Manager, and non-symlink
  mode-`0600` private-file fallback. `DeviceKeyInventory` (`decapod cloud devices`)
  and `.meta.json` `device_key_id` files store only public `jkt`/ID metadata and
  never emit PKCS#8 bytes or tokens.
- **DPoP vs management revoke separation & header redaction (`src/decapod/core/cloud_client.rs`, `src/decapod/core/propodus.rs`)**:
  `--session-logout` (or `dpc_at_` sessions) requires an enrolled local device
  DPoP signer and refuses to fall back to unsigned logout (`CloudAuthStatus::KeyLost`).
  `warnings_from_cloud_metadata` bounds and redacts `Deprecation`, `Sunset`, and
  `Decapod-Cloud-*` header notices so upstream secrets never reach stderr/JSON.
- **Fail-closed container relay & SQLite non-creation (`src/decapod/core/cloud_workspace.rs`, `src/decapod/core/container_runtime.rs`, `src/decapod/core/validate.rs`)**:
  `HostCloudRelayHandler` denies host-relay custody requests unless
  `verify_running_container_for_invocation` verifies the running container's
  immutable 64-hex `Id`, workspace path label, and `org.decapod.invocation` label.
  Cloud `init`, `workspace ensure`, `govern`, `validate`, and validation-worker
  paths fail closed on missing/revoked/drifted/expired hosted custody and never
  create, read, or mutate `.decapod/data/decapod.db` (even if pre-existing legacy
  SQLite bytes exist).

## Pay-and-use Cloud enrollment URLs, seat orthogonality, and zero SQLite fallback (#1877, #1896)

- **Validated repository enrollment URLs (`src/decapod/core/cloud_client.rs::pricing_enrollment_url`, `is_valid_repo_slug`)**:
  Rejects empty segments, `.` / `..` traversal, extra slashes, query/fragment characters, and opaque UUIDs before appending `?repo=<RFC3986-encoded-owner%2Frepo>`, falling back to static `https://decapodlabs.ai/pricing`. Enrollment links carry zero bearer tokens, refresh secrets, or DPoP state.
- **Fail-closed authorization & non-destructive pending states (`src/decapod/core/cloud_project.rs`, `src/decapod/core/cloud_client.rs`)**:
  `route()` admits Cloud data operations only when `backend_authorized == true && status == "active" && pending_fulfillment != Some(true)`. `pending_fulfillment` / `payment_pending` preserves saved private credentials rather than triggering destructive sign-in restarts; `admin_approval_required` (`admin=true, repo_assigned=false`) enforces that organization admin privileges never substitute for an assigned paid repository seat; `organization_already_enrolled` surfaces immutable `cloud_org_id` rather than inviting duplicate enrollment.
- **No Cloud-to-SQLite fallback (`src/decapod/lib.rs::run_init_apply`)**:
  Cloud-mode `decapod init` refresh skips `migration::check_and_migrate_with_backup_report` so `.decapod/data/decapod.db` is never initialized in Cloud repositories. All headless JSON auth/todo error outputs redact secrets and echo only bounded non-secret status guidance.

## Admission and observation limits

The Constitution build rejects metadata-only self-promotion and unsupported enforcement assertions. Binding review declarations identify the required review process. They provide no authenticated reviewer identity. Repository overrides and runtime mandates retain their native owners. Superseded, expired, and unrelated knowledge is excluded from operative context and can be inspected through the appropriate audit path.

The sandboxed scaffolding tests use the checked-in corpus and schema to check promotion, declared enforcement, and reference validity.
The activation and capsule fixtures resolve the candidate executable before entering temporary repositories so their child commands exercise the build under test.

The cooperative adapter accepts bounded regular files and delegates fixed operation arguments to the current executable. It grants neither host sandbox enforcement nor approval. Compact comparison inputs distinguish reported, captured, and independently checked evidence. Context return, actual model delivery, functional correctness, and current native completion require separate observations.

## Device key custody, container signing isolation, and fail-closed cloud boundaries (#1778, #1066, #1590, #1809, #1808)

- **Key custody outside repositories**: `DeviceKeyStore` rejects any key path
  that resolves inside the repository checkout (including through non-existent
  path tails on macOS `/var` -> `/private/var`), rejects symlinks, enforces
  mode `0600` (`O_NOFOLLOW`), and zeroizes PKCS#8 key material on drop. DPoP
  verifiers reject private JWK parameters (`d`, `p`, `q`, `dp`, `dq`, `qi`,
  `oth`, `k`) and bind `htm`, canonical `htu`, `iat`, `ath`, `nonce`, and `aud`.
- **Container isolation**: `filter_container_environment` strips raw tokens,
  service-role keys, refresh secrets, and `SSH_AUTH_SOCK` before container
  launch. `HostContainerSignerServer` mounts only a mode-0600 Unix socket and
  denies wrong repository, mismatched agent ID, foreign origin, non-allowlisted
  operations (including `device-enroll`, `device-revoke`, billing, and owner
  tier mutations), cross-capability misuse, expired sessions, and revoked keys.
- **No silent local fallback or unmerged privilege escalation**: Cloud auth,
  entitlement (`403`), payload bound (`413`), quota (`429`), or owner-repair
  denials never create, read, or mutate `.decapod/data/decapod.db`. Unconfigured
  explicit `neon`/`supabase` datastores fail closed without falling back to the
  retired legacy preview endpoint. Unmerged worktree config never enables
  `owner` repair locally or remotely, and `#1776` accepted-cloud enforcement
  continues to block unaccepted local downgrades.
- **Public repository hygiene**: All code, tests, fixtures, docs, and specs in
  this wave use synthetic `example.invalid` / loopback fixtures and public
  registered wire markers (`OWNER_REPAIR_V1_SQL = "SELECT propodus.owner_repair_v1($1, ..., $11)"`)
  only — zero live hostnames, merchant identifiers, private repository source
  filenames, private database schemas/CTE expansions, or raw secrets.

## Ambiguous RPC JSON

Duplicate decoded keys are invalid at every nesting level, including Unicode
escapes that spell an existing key. A later field cannot erase an unsupported
required extension before negotiation. Recursive uniqueness uses the existing
release-manifest decoder through a shared internal utility, preserving caller
size bounds and specialized redaction. This controls interpretation at the
CLI RPC boundary only; it does not authenticate an imported proof or establish
host-wide observation. Same-host Python differential parsing is distinct from
independent trusted execution.

## Withdrawal and fresh custody

A full validator not-ready judgment only narrows current workunit authority;
it never creates a successful observation or relabels unexecuted checks as
failed. Withdrawal checks the captured workunit generation and task custody
atomically in the existing canonical database. A concurrent journal or lease
change refuses the append. Historical successful records retain their original
identity. Renewal does not transfer their authority: promotion requires fresh
matching current proof, including the existing tool/input/source and epoch
checks. Split-store atomicity remains unsupported and fails explicitly.

## Configuration validity and accepted privilege proposals (#1771, #1776)

`cli::DecapodProjectConfig` rejects unsupported schema versions during typed
deserialization, including consumers that deserialize accepted policy directly.
Its canonical parser returns `INVALID_CONFIG_SCHEMA` with an embedded reference
and a retry path. `lib.rs` serves CLI schema and RPC `schema.get` before config,
session and migration startup so malformed configuration cannot block repair.

`core::db_maintenance::resolve_repository_policy` is the shared branch-safe
source for maintenance, exceptional RBAC, shared-claim opt-in and Git-backed
approval categories. A proposed main-root feature checkout cannot supply its
own grant or protect itself by changing `repo.base_branch`. Accepted grants
remain prospective and revocable. A validation bypass remains explicitly
non-proof and cannot authorize publication. Local protected-root/Git administrator
access and explicitly mounted host policy retain their cooperative host trust
boundary; this does not authenticate a human or intercept unrestricted shell.

The original v0.148.9 binary admitted an unsupported config version and an
unaccepted root RBAC bypass; it denied malformed TOML but blocked schema repair
access. Public-command regression controls cover denial, corrected ordinary
operations, independently accepted bounded grants and revocation. Candidate
test execution and exact-head full validation remain required before delivery.

### Cross-surface effective-authority enforcement and #1776 repairs

Cross-surface red-team verification against the compiled `decapod` binary (`0.148.11`)
reproduced four candidate / unaccepted-branch self-authorization vectors and verified
canonical enforcement across the remaining privilege and schema surfaces:

1. **Base branch self-selection (`workspace::resolve_base_branch`, `validate::validate_project_config_toml`)**:
   Candidate worktrees and unaccepted branches could previously set `repo.base_branch`
   to their own branch, collapsing diff comparisons and target-policy baselines to self
   under `govern project-check` and `validate`. `workspace::resolve_base_branch` now
   prefers `core::db_maintenance::resolve_accepted_project_config` (falling back to
   `detect_base_branch(&main_root)` for worktrees rather than candidate TOML), and
   `validate_project_config_toml` explicitly rejects candidate/unmerged divergence from
   the accepted base branch (`BASE_BRANCH_SELF_AUTHORIZATION_DENIED`) as well as
   candidate weakening of `[laya] mode = "required"` (`LAYA_POLICY_DOWNGRADE`),
   `proof.required_profile = "decapod.pr-readiness.v1"` (`REQUIRED_PROFILE_DOWNGRADE`),
   and `repo.backend = "cloud"` (`BACKEND_POLICY_DOWNGRADE`). `lib.rs` likewise
   preserves canonical `BackendType::Cloud` before entering local storage startup.
2. **Protected paths erasure (`core::gatekeeper::GatekeeperConfig::from_repo_config`, `validate_gatekeeper_gate`)**:
   Removing `governance.protected_paths` entries in a candidate worktree previously
   bypassed `govern gatekeeper check` and `decapod.scope.protected_paths` rule evaluation.
   `GatekeeperConfig::from_repo_config` now computes the deduplicated union of canonical
   accepted `protected_paths` and task-local paths, preserving stricter task-local guards
   while forbidding candidate removal of accepted protections. `validate_gatekeeper_gate`
   also reports `ViolationKind::ProtectedPath` counts cleanly.
3. **Container custody & isolation downgrade (`validate::is_container_workspaces_disabled`, `validate_git_workspace_context`)**:
   Raw substring scanning (`.contains("container_workspaces = false")`) allowed comments
   and unaccepted main-root checkouts to bypass container isolation, and candidate
   worktrees could downgrade `repo.container_workspaces` or `custody.isolation` to
   `"worktree"`. `is_container_workspaces_disabled` now evaluates parsed canonical
   accepted config via `resolve_accepted_project_config` (refusing unmerged branches and
   comment strings), and `validate_git_workspace_context` rejects candidate or unmerged
   downgrades (`CUSTODY_ISOLATION_DOWNGRADE`).
4. **`.decapod/OVERRIDE.md` rule and HITL boundaries (`core::db_maintenance::resolve_repository_override`, `core::validation_rules`, `plugins::policy::is_hitl_disabled_by_override`)**:
   `resolve_repository_override` binds `OVERRIDE.md` resolution to the accepted base
   commit (`:accepted-ref` SHA blob) whenever `main_root` is off a protected branch—even
   if `.decapod/config.toml` itself was not modified—closing `HITL_DISABLE` bypasses on
   unaccepted root checkouts as well as candidate worktrees. `validate_validation_rules`
   loads accepted rules via `resolve_repository_override` while separately validating any
   candidate/unmerged `OVERRIDE.md` against `RuleRegistry::load_project_rules_from_content`.
   `apply_rule_override`, `build_and_cache_rules`, and `extend_requirements` enforce
   `OverrideEligibility::NonOverridable` rejection across all fields (`OVERRIDE_NOT_PERMITTED`)
   and forbid `TightenOnly` / `ConfigurableOnly` rules (`decapod.evidence.required_and_fresh`)
   from lowering `severity` or relaxing (increasing) `evidence_max_age_seconds`.

Already-enforced controls covered by actual-binary regression in `tests/policy_self_authorization_1776.rs`
include default-deny `[governance.db_maintenance]`, `[governance.rbac]`, default-deny
`[governance].main_merge` (`POLICY_SELF_AUTHORIZATION_REJECTED` when candidate-authored
beyond accepted base policy, `#1468`), unknown-key `INVALID_CONFIG_SCHEMA`,
`[governance].allow_basic_shared_claims`,
offline Ed25519 historical-exception verification preserving `history_proven = false` (`#1569`),
and opt-in pinned formal-checker attestation rejecting admitted holes and fabricated transcripts (`#1907`),
`POLICY_SNAPSHOT_STALE` TOCTOU rejection across base-policy rotation, stale-target/proof
invalidation on `validate` + `todo done --validated`, bounded accepted rotation and
revocation with intact `data db audit` history, and pre-config read-only repair via
`decapod docs show` and `decapod data schema --deterministic`. Cooperative host-local
root/Git admin access and remote forge/Buildkite acceptance remain external trust
boundaries and are never fabricated by local kernel checks.

## Scoped currency authorization (#1753)

Currency preservation requires the same explicit config, sole exact canonical
push slug, ungrafted immutable source ancestry and committed template identity,
plus valid bodies/fingerprints for all four current regular bounded entrypoints.
A failed inspection grants no exception. Generated-release provenance is read
from the accepted Git blob, not an agent-supplied verdict. Missing/invalid data
retains the conservative refresh condition. Docker/manifest provenance, required
artifact presence and every execution obligation remain separate enforced gates.

## Timestamp conversion boundary (#1755)

Untrusted RFC3339 offset normalization uses checked UTC conversion. A timestamp
beyond the supported UTC range returns an error, including during storage JSON
serialization, instead of panicking or accepting an expiry. Regression controls
exercise this boundary alongside invalid dates and expired/refreshing sessions.

## Escalation admission preserves authority (#1690–#1692)

Discovering an omitted operation or target cannot reset the reflection budget.
Once established, a command and concrete target cannot be removed or replaced
on a continuation. A policy-required human dependency takes priority over
structural regression detection, and reflection never grants that authority.

`core/mentor/reflection/escalation.rs` observes existing policy and approval
state; it issues no grant, changes no role and never promotes inferred intent
into authority. A source predicate proves only current bytes, not provenance,
semantic truth, test execution or authorization. Unrelated approvals and human
questions cannot resolve another missing requirement. Task/custody bindings,
source freshness, continuation tokens, immutable subjects and the existing
three-round/64 KiB journal bounds protect the invoked protocol. Proposed question,
investigation, decision and quote bodies remain transient, with digests retained
for retry integrity; no private reasoning transcript is requested or stored.
Actual mutation, verifier, validation and publication guards remain independent.

Absence of `.git` does not establish checkout admission, including outside
`.decapod/workspaces`. `init --no-git` enables prerequisite diagnostics only.
The partial project-check route handles that compatibility explicitly; workspace
status and full validation retain their independent fail-closed boundaries.

## Diagnostic success grants no checkout authority (#1661)

Non-Git prerequisite diagnostics may inspect project configuration without a
task-bound Git workspace. They do not change `can_work=false`, mint a full
readiness judgment, or admit authoring/publication. Full validation continues to
reject that directory. CLI regression coverage exercises all three observations
using the real binary without Git-gate overrides. The custody transaction also
rejects second live bindings and rolls back binding persistence if event evidence
cannot be committed. Local actor labels and Git filesystem identities remain
host-local cooperative authority, not authenticated cloud or OS isolation.

## Agent-owned browser handoff

`authorization_url` exposes only Dactyl's validated, short-lived browser handoff.
It never includes the polling secret, verifier, exchanged code, access token or
refresh token. A task-add diagnostic preserves that handoff and its non-secret
operation ID without exposing task text or credentials. Recovery instructions
are fixed; raw authentication messages and recovery
text do not cross into task-operation diagnostics. Only the explicit validated
browser handoff field is carried for an onboarding-pending result. The agent performs an
explicit `cloud login --restart` after uncertain one-time outcomes; polling and
idempotent account confirmation do not authorize replaying those mutations.
The service remains authoritative for account requirements and repository access.

## Managed credential custody and server authority (#1628)

`core::cloud_session::SessionStore` is the only durable custody owner in the
managed CLI path. Its binding includes the validated service origin, exact
repository spelling used by the current service contract, and canonical main
checkout identity. The store rejects unsafe machine roots (including parent
traversal), paths within any Git checkout, symlinks/hard links, non-private
owners/modes, oversized records, invalid phase shapes, and binding mismatches.
Files use no-follow opened descriptors and synchronized atomic replacement;
refresh and rejection updates share a stable exclusive lock with a bounded
wait. Credential material and pending one-time secrets never enter Git.

`core::cloud_client` saves uncertainty before every one-time exchange or refresh
and never infers successful rotation after response loss. Explicit restart is
required for ambiguous one-time state. A rejection may invalidate only the
exact bearer used by that request, preserving a newer concurrent rotation.
Raw transport/service text and credentials are excluded from diagnostics.
The deliberately displayed browser handoff is validated by Dactyl and remains
a short-lived human sign-in capability, not repository authorization.

Dactyl alone performs discovery/auth/account/data transport in the ordinary
cloud path, with no redirect or ambient legacy token/provider adoption.
Decapod binds the private flow to the requested repository and validates the
handoff/session lifetimes. The published connector returns status/code strings
and a completion acknowledgement, without returning repository metadata to the
caller. Current access to the bound repository is therefore checked by the
authorized read rather than inferred from the sign-in result. Decapod does not
evaluate price, membership, seat/payment authority, or privileged grants. Service-confirmed
completion plus current authorized read is required before reporting setup
ready; every subsequent request still requires server authorization. Offline
fixtures cannot establish hosted PostgreSQL or managed-account enforcement.

## Historical validation trust boundary

`core::validation_history` accepts full execution evidence only from the
controller's completed runtime path. There is no command that imports a caller's
success JSON as validation authority. Records bind repository, immutable
candidate and original base, exact check plan and results, original task
contract, owner and lease lineage. SHA lookup is a bounded read-only operation;
malformed, oversized, ambiguous or digest-inconsistent records fail closed.

Historical completion independently checks integration and the complete supported
task lineage. Reassignment, renewed custody, intent edits later reverted,
missing history and concurrent task mutations cannot inherit an older pass.
It does not trust commit messages, branch names, a provider badge, or the legacy
tracked prerequisite receipt as proof of integration and successful execution.
When unauthenticated public HTTPS branch-tip inspection is rejected for a private
repository, `src/decapod/core/validation_history.rs::accepted_target_for_repository`
may fall back to reviewed host-user authenticated read-only transport — first via
`snapshot_provider::OfficialCli` (`gh api` GET `repos/{owner}/{repo}/git/ref/heads/...`
authorized through `external_action::authorize_official_github_read`), and then via
an isolated host-user `git ls-remote` in an empty scratch directory with
`GIT_TERMINAL_PROMPT=0` and disabled redirects. No credentials or raw transport
diagnostics are persisted in `.decapod/`, logs, or worktrees, and local no-graft /
no-alternate ancestry verification (`reject_history_overrides`) remains mandatory.
Historical authority is limited to completing its original task; new publication
still requires current full validation. These local records retain Decapod's
cooperative same-user trust model: hashes detect inconsistent evidence but do
not protect against an unrestricted owner rewriting all state or impersonating
another local actor.

## macOS directory stream ownership

The bounded epoch reader opens an independent directory description relative to
a held descriptor, transfers it to `fdopendir` only on success, and closes it
through `closedir` on drop. Entries are classified with descriptor-relative
`fstatat(AT_SYMLINK_NOFOLLOW)`; observed paths are not reopened. Existing root and
ancestor no-follow checks, safe in-root file-link resolution, and resource bounds
remain enforced. The private stream owns its cursor and copies each entry before
the next `readdir`; EOF and read errors remain distinct. No trust, task custody,
cloud authorization or validation gate changes.

## Advisory intent reconciliation access

## Acceptance observer capture boundaries

The observer never authenticates a producer from an agent declaration or a JSON hash. OperatorControlled witness custody is an explicit host assertion. Shared local OS authority can forge it. Coverage is scoped to its measured channels and retained window. Truncation, missing endpoints and mismatched counts or hashes fail the completion assessment without erasing the admission history.

<!-- decapod:requirement REQ-INTENT-TRACEABILITY -->
`intent.reconcile` is a sessionless local inspection RPC over the existing
repository plan, canonical specs and source fingerprints. It bypasses mutation
custody and startup datastore writes, accepts an empty object or an optional
full immutable Git base commit ID, and adds no network access, credential
access or persistence. Filesystem read
permissions remain the access boundary. Reports expose repository paths,
requirement IDs, hashes and unresolved plan questions; callers must treat those
questions as repository content when sharing output. A digest is provenance for
observed bytes, not authenticated human approval. Markers, matching hashes and
review records cannot establish semantic correctness, resolve a pending human
decision or authorize validation/publication; `semantic_alignment` remains
`not_proven`. The existing mutation and publication gates remain authoritative.

## Immutable impact Git trust boundary

<!-- decapod:requirement REQ-INTENT-CHANGE-IMPACT -->
The optional base-to-HEAD impact reader invokes bounded, repo-local Git
inspection on immutable commit objects with source replacement and inherited
Git directory/worktree/index overrides removed. It does not fetch network
data, execute repository hooks, infer code-to-requirement authority, disclose
source contents, or write repository state. Exact ancestry and complete
path accounting are required for complete observations; unsupported paths and
truncated results remain explicitly unproven. An uncommitted working tree is
outside the immutable comparison, and unresolved questions remain visible.

## Shared policy command regression

The Bazel release-download retry configuration uses only the official
`mirror.bazel.build` and `github.com` hosts, adds no credentials, and preserves
every repository rule's original pinned archive digest. Mirror samples were
checked against registry metadata bound by the committed Bazel lock. An alternate URL or
successful transfer cannot authorize different bytes or replace execution
evidence. Existing lockfile enforcement and publication gates remain required.

`tests/cli_contracts.rs::claim_boundaries` verifies that candidate-only policy
changes cannot enable shared claims, canonical opt-in preserves basic trust,
and the actual policy audit records the secondary caller, primary label and
config digest. Read-only datastore observation checks the stored trust and
audit event. Disabling canonical policy prevents new shared members without
changing primary ownership. These tests do not turn cooperative caller labels
into authenticated principals or authorize a trust-grant API.

## Execution identity and replay safety

Generic replay normalization masks only execution identity in named readiness
receipts: request IDs and binding evidence digests, plus the report evidence
path. Material revisions, model/governance/proof bindings, modes, decisions,
blockers and unknown fields remain significant. The normalizer is unchanged by
model retirement, preserving historical replay behavior without model-specific
code. Complete receipts remain intact in persisted historical evidence.

## Rust source-spelling fixture isolation

## Cooperative shared-claim opt-in

The optional basic-trust shared-claim setting is a cooperative policy, not an
authenticated trust grant. Same-user agents that can edit main config can
enable it, and caller labels can be impersonated. The runtime refuses the
opt-in if main-root resolution fails and does not use the candidate config for
the shared datastore. A bounded regular-file config snapshot is taken before
the write transaction; an in-flight operation can use its earlier snapshot.
Its transactional audit records the consumed config fingerprint, not proof of
human editing or verified identity. All other trust and approval gates remain.

## Broker caller context custody

`group_broker::CallerContext` transports only a fixed set of request-scoped
fields across the existing permission-restricted local socket. It is not a
general environment-forwarding facility. The subprocess clears inherited
identity/session fields before applying the caller values, retains ordinary
session/policy validation, and does not grant authorization based on request
metadata. Existing cooperative local-agent authentication and auto-acquisition
remain unchanged; this fix does not authenticate hostile same-user processes.

Session passwords remain ephemeral transport inputs, excluded from request
Debug output, replay hashes and durable request records. Dedupe binds actor and
working directory with non-secret context to prevent another caller's command
from accidentally reusing the leader's result. Restricted socket access remains
the existing trust boundary, not cryptographic isolation between local peers.

The required dependency check exposed RUSTSEC-2026-0386 in the accepted Tokio
1.52.3 lock entry. The minimal lock-only update to patched 1.53.3 changes its
version and registry checksum without relaxing advisory policy or changing
dependency edges. Fresh exact-candidate provider proof is still required.

## Native receipt and dependency repair boundaries

Native success receipts are produced only after the complete selected proof and
required readiness decision, bound to exact candidate/target and governance.
They cannot be imported to authorize publication or replace execution. Atomic
private completion staging prevents partial receipt/verdict writes from exposing
a successful pair; no tracked candidate bytes are changed by receipt output.
The root Cargo.lock updates Tokio1.52.3 to1.53.3 for RUSTSEC-2026-0386, changing
only that package's version and registry checksum. This addresses the actual
required dependency-check failure without suppressing advisories or modifying
other dependency edges. The pinned Bazel9.3.0 module dependency generator also
refreshes `MODULE.bazel.lock` from that Cargo input; byte review permits only
Tokio references/version/checksum and the Cargo input digest. Generation changes
no trust configuration, proof profile, registry or unrelated dependency graph.
It is not a claim that full validation has passed.

Public Buildkite artifact redirects admit only the exact documented HTTPS
`buildkiteartifacts.com` host and legacy `s3.amazonaws.com/buildkiteartifacts.com`
bucket prefix, with unchanged four-UUID layout, exact producer-job binding,
content hashes, byte limits and original deadline. HTTP, explicit ports, userinfo,
lookalike hosts, malformed producer identities, fragments and encoded traversal
reject before following. Signed query strings remain opaque and never enter logs,
evidence or Git. Buildkite documents the current host in its
[network requirements](https://buildkite.com/docs/agent/self-hosted/security/network-requirements).
The fix grants no arbitrary storage, credential, publication or candidate
self-authorization capability. The original strict-collection failure remains
failed until an independently supported controller can execute that route.

The verification-only scalar F64 reference verifies immutable model/config hashes and calculates expected values without candidate tensor/model math, while sharing generic infrastructure, weights and authorship. Candidate `30015be0` executed three fixed 138/314/4,096-token Metal/CPU comparisons, the exact token boundary and all 16 memory slots; this does not establish all-input equivalence or independent human review. Raw tensors stay in bounded private exports. The final required scientific command rechecks referenced bytes and comparisons without importing earlier-run artifacts. The complete v2 collection nevertheless produced 23 false-ready proxy outcomes per backend. Proxy labels, incomplete supported coverage and 23 source lineages retain the unsatisfied readiness-calibration gate.

The new bounded plan projection preserves explicit obligations without treating
plan gate flags as proof. Its overflow path abstains; final governance identity
checks reject changed intent or obligations. The verification-only public review
capture contains source facts, not private sessions or controller evidence, and
is excluded from the published crate. Its Rust audit pins bytes and checks exact
review/commit binding, timestamps, edits and complete pagination. Selected review
verdicts and rationale are excluded from prospective model input; future CI is
not backfilled. Public `User`/maintainer identity is not proof of unaided human
authorship, and owner grouping is not a guarantee of statistical independence.
Structural eligibility alone never grants a calibration or publication pass.

## Explicit local task-transfer trust

The separate worker's vendored Metal subpackage preserves its exact upstream base/tree and explicit three-barrier source modification. Patched shader identity is separate from the upstream revision; the standard Apache text corrects a prior project-specific appendix without inventing an upstream holder. aa66c8e4 actually passed its repeated primitive and two model fixtures after earlier measured Metal failures. The new scientific collection context is ephemeral evidence bookkeeping, not a task control plane or authentication against a malicious worker/same-user forgery. Exclusive files, current-directory/runtime/model/corpus binding, no-follow single-link reads and strict final aggregation reject accidental replay/mixing, altered labels, inconsistent decisions and missing coverage. A collection-only successful exit cannot waive the last required command or any kernel gate.

`src/decapod/core/task_transfer.rs` accepts source custody only with an independently
supplied artifact digest and explicit operator acceptance. Git origin and agent
names are local bindings, not authenticated remote principals. The operator must
verify source yield/export through a trusted channel and ensure cooperating source
writers use the supporting controller. No signature, source liveness check or
global exclusivity across disconnected clones is implied.

The source task is sealed under the normal journal transaction before private
artifact publication. Only its previous yielding actor can export. Import refuses
existing identity/hash/history and replay, validates source chain and receiver,
and retains source proof solely as history. Current proof and pass timestamps are
cleared. The artifact excludes session tables and unrelated tasks, but task text
can itself be sensitive and must remain outside Git. Older binaries and privileged
writers are outside this cooperative local fence.

## Agent-delegated authority, local and cloud

The agent is the only operational caller. Human owners, maintainers,
contributors, employees, and contractors express intent to their agent; the
agent calls Decapod within the same repository-native work lifecycle.
Local developer mode retains cooperative actor labels and requires no cloud
authentication. These labels provide audit attribution and task custody
within the local user trust boundary, not OS principal isolation.

Cloud authority instead belongs to a verified Propodus principal and is
evaluated on the server against a live managed session, organization
membership, repository grant, and entitlement. The agent ID, local RBAC
mapping, task owner field, and asserted role can never authorize a cloud
operation. Access tiers distinguish read, contributor/write, maintainer,
and owner. A contractor uses an expiring direct user grant within one
repository, including an explicit expired-deny fence over group/team/org
inheritance. Only the schema owner can issue or revoke that delegation.

Cloud task custody binds the executing authenticated principal separately
from the agent's freely chosen collaboration label. Release/complete of
another principal's claim must fail even if the caller copies the exact
label. Reviewed maintainer/owner repair is an explicit, bounded exception.
An agent's self-asserted success or authority cannot become proof or
publication permission. Live service assertions are required; synthetic
client fixtures establish routing only.

## Threat Model

Decapod governs accepted agent work inside a repository. Its security boundary
covers task custody, project policy, isolated execution, durable state and the
proof required for governed publication. The human supplies intent and decisions;
the agent interprets and performs the work. Decapod checks the configured
invariants and records their outcomes.

### Provider-attested validation trust

The accepted CI-attested policy trusts the configured Buildkite executor and its
HTTPS metadata for the required validation results. Pipeline Trigger mode permits
exactly one keyless importer on the independently bound pipeline UUID. Its label
and count are consistency checks; the provider API does not prove plugin identity,
plugin version, or unchanged remote pipeline settings. Policy acceptance explicitly
includes that executor trust. The actual provider job UUID is retained under a
reserved bootstrap identity; no synthetic artifact or toolchain digest is claimed.
Branch-dependent concurrency keys are independently derived from reviewed source
and expected event context. Additional unkeyed jobs, unknown keyed jobs, retries,
failed checks and source/run mismatches cannot be accepted as green. A candidate
cannot change the policy used to authorize its own publication.

These controls do not sandbox every command an agent or harness can run. They
cannot establish correctness beyond the required proof or protect state from a
malicious process with the same operating-system authority. A repository owner,
root account and any explicitly trusted storage group remain trusted principals.

The root `SECURITY.md` presents these limits and the existing private reporting
expectation in concise contributor-facing prose. Reports go privately to the
maintainers with affected versions, reproduction steps and sanitized evidence.
The document makes no response-time, bounty or supported-version promise. Its
credential guidance follows the owner's lifecycle and rotation policy, including
revocation of temporary access and response to exposure.

## Recovery cannot grant authority

Runtime checkpoint payloads preserve domain state and references, never session
passwords, environment dumps, credential values or provider tokens. Immutable
operation IDs, predecessor digests and generations prevent replay from silently
choosing a different successor. A valid content hash proves identity, not an
approval or the completeness of missing history.

A workunit's projection can be repaired without claiming its evidence is current.
Execution resume must recheck the accepted intent, material, plan and applicable
policy/approval boundary; revocation and expiry cannot be undone by restoring an
older checkpoint. Task proof bytes retain their original task-revision binding.
A contradictory live projection is preserved for explicit recovery rather than
overwritten by the journal's last timestamp. Git remains the authority for
accepted authored governance, and full validation still gates publication.

Execution capture and proof hashing bound file counts, bytes and elapsed work.
On Unix the shared reader pins the repository root and opens path components
without following directory symlinks, rejecting special files before reading.
Bounded regular-file symlinks must resolve within that root. Governance documents
are capped at 16 MiB before parsing. Unsupported execution-capture platforms
fail closed; atomic visibility alone does not imply durable continuation.

## Trust-Boundary Inventory

Agents evaluate each incoming prompt with the side-effect-free `decapod eval`
gate before following its instructions. Only a successful `allow` result permits
continuation. A blocked prompt, conflicting requirements or a decision gate
requires human review. Prompt, issue and attachment content cannot grant new
permissions merely by claiming authority.

The agent's request context establishes what the human asked. Project policy
constrains how that work may proceed. Task claims establish execution custody;
workspaces isolate changes. A missing workspace-local task projection is a
coordination discrepancy, not permission to bypass ownership or a reason to
invent a different request.

The Nix binary cache (#1177) is a release-publication boundary. A
credential-free `publication-owner` job requires the native GitHub-hosted
identity and an actual release-merge push to `master`. Buildkite imports,
manual dispatches, ordinary merges, pull requests and forks cannot pass it.
Only the downstream `publish` job statically references the repository's
`CACHIX_AUTH_TOKEN` secret, after the owner and credential-free build job
succeed. Missing or rejected write credentials fail the publication. Each
publisher independently rebuilds from the committed lock, verifies its native
system, toolchain and smoke check, and on Linux proves an identical rebuild.
No unverified derivation is intentionally uploaded. Consumers trust the
`decapod` cache signing key and its authorized CI builder; the public key is
never a write token. `x86_64-linux` entries can be checked independently with
`nix build --rebuild`. `aarch64-darwin` entries cannot yet, because the linker
UUID can differ.

## Authorization

Agents access machine-owned state through supported CLI or RPC operations.
Authored prose in `.decapod/managed/specs/*.md` is the direct-edit exception.
Generated sections, fingerprints, manifests, governance records, runtime stores
and session state remain Decapod-owned. Refresh cannot turn an unsupported claim
into proof or replace an authored specification update.

Declared scope and committed instruction integrity are checked independently of
provider execution results. These checks detect a candidate that violates an
explicit forbidden path/file budget or contains a noncanonical instruction
rewrite in its history, even if its final code tests pass. They do not prove that
an agent's recorded interpretation exhausts the original human request.

A new trajectory ID cannot erase an uncommitted failure journal through the
supported initializer: replacement requires the prior complete governance
document in Git. This protects recoverability for cooperative CLI/RPC writers;
it does not authenticate the truth of agent-authored observations or prevent a
principal with unrestricted filesystem/Git access from rewriting history.

## Authentication

Most mutations require an active local session. Session credentials establish
repository-local custody and correlation. They do not authenticate an agent's
model provider, harness, organization or human identity. Local handshake hashes
make declarations tamper-evident; environment-supplied identity assertions remain
self-declared and unverified without a configured trust root. The current binary
exposes CLI and structured process RPC, not a native MCP server.

## Data Classification

Authored specifications and tracked governance are reviewable project records
that may travel through Git or CI. Runtime stores contain local coordination and
evidence; session and provider credentials require separate private storage.
Project content may itself be sensitive. A generated artifact or projection does
not establish that its contents are public, encrypted or safe to disclose.

## State and filesystem boundary

On Unix, new private state directories use at most `0700`; files use at most
`0600`, set before writing bytes. Datastore checks reject unsafe ancestry,
state-file symlinks, hard links, non-regular files and unintended writable modes,
including SQLite and coordination sidecars. Existing installations are never
silently repaired with chmod. Atomic writes remove only their own temporaries.

Explicit `DECAPOD_STORAGE_SHARED_GROUP=1` trusts the datastore directory's group
and requests `0770` directories and `0660` storage files, subject to umask and
configured ownership. It never permits world-write access or shared machine
credentials, sessions or runtime configuration. The group broker's runtime socket
directory follows the same private or explicitly shared boundary.

POSIX mode checks do not inspect extended ACLs or mount policy. On Windows,
inherited ACLs remain the owner's responsibility; Unix checks do not establish
Windows DACL security. Filesystems must provide the required permission, locking,
sync and atomic-rename behavior. There is no blanket FUSE or virtiofs ban.

The adjacent database lock coordinates cooperating Decapod processes. It is not
an authorization mechanism or a guarantee against external SQLite writers. The
kernel releases advisory locks on process exit; deleting a lock file is not
recovery. Broker election uses its stable lock inode, never a persisted PID as
authority to signal a process.

## Workspace ownership and cleanup

Task work stays outside the main checkout. Tracked governance and specifications
remain branch-local; governance files aliased to the host by symlink, shared inode
or direct bind mount are rejected. Containers may share the approved datastore
and migration ledger while mounting accepted host policy read-only.

Cleanup requires ownership evidence for the exact resource. `workspace prune
--force` relaxes lifecycle selection, not ownership. Unregistered directories
require a durable invocation receipt, matching directory identity and an inactive
lease. Names, task-like paths and an empty inventory from a different container
engine prove nothing. Unknown resources, active claims and unverifiable ownership
remain preserved with diagnostics. Container removal checks ownership labels and
immutable IDs using the recorded runtime backend.

Automatic container cleanup does not equate directory ownership with permission
to destroy unfinished work. Failed executions retain all clone files, including
ignored diagnostics, and cannot invoke push or PR creation. Successful PR requests
use the common exact-candidate governed publisher; child success and CI-local
validation cannot substitute for publication readiness. Local-only execution
cannot select a stale host branch for publication. Foldback never force-updates
a divergent host branch.

Successful disposal requires branch foldback, the expected attached HEAD, an
index without hidden entries, unchanged inherited refs outside the returned
branch, and no dirty tracked or non-ignored untracked files. New or changed
stash, side-branch and tag refs preserve the clone. Raw OID comparison includes
annotated-tag objects; index inspection preserves flags rather than clearing them.
The shared status check forces untracked visibility and submodule inspection.
Ignored build outputs remain disposable after successful otherwise clean runs.
Reflog-only/dangling objects, deleted ref names, arbitrary external writers and
races after inspection are outside this bounded guarantee. Retention does not
copy files into Git or proof. Review retained work before explicit prune; these
automatic-clone checks do not broaden the separate explicit-prune contract.

Build/test cleanup in `assets/tools/build_lifecycle.rs` is restricted to
invocation-owned scratch with its original filesystem identity (`Scratch::verify_root`).
When Unix `fs::remove_dir_all` fails with `PermissionDenied`—such as rootless
Podman overlay `work/work` mode-`0o000` or read-only layer directories created
under a test's disposable `HOME`—`Scratch::clean` runs bounded permission
recovery (`Scratch::recover_owned_permissions_with_bounds`) before a single
re-verified removal retry. Recovery locks the verified scratch root to `0o700`,
inspects entries via `fs::symlink_metadata` without following symlinks, rejects
replaced directories, inode drift, foreign owners (`uid != identity.uid()`),
cross-device mounts (`dev != identity.dev()`), and trees exceeding 128 levels
(`MAX_PERMISSION_RECOVERY_DEPTH`) or 65,536 entries (`MAX_PERMISSION_RECOVERY_ENTRIES`),
and restores `0o700` only on verified owned directories. `cleanup_diagnostics`
remains strictly read-only. Replaced roots, unconfirmed live process groups,
shared caches, symlink targets and proof artifacts remain outside chmod and
deletion authority. Process supervision bounds ordinary owned descendants; it is
not a sandbox against deliberate process-group escape. Unsupported containment
requirements fail explicitly.

## Evidence and publication

Plans, claims, trajectories and hashes preserve interpretation and custody; none
proves that the work succeeded. Required evidence must bind to the exact candidate,
accepted target, toolchain and applicable checks. `passed`, `failed`, `partial`,
`unavailable` and `no_checks_run` remain distinct. No executed checks means an
unsupported completion verdict. CI-local success never establishes publication
readiness.

Persisted task proof is scoped to an exact task identity and revision. A stored
`pass` label without matching subject custody cannot unlock dependencies,
exclusive completion, work-claim completion, or aggregate readiness. Verification
result writers compare their original task and baseline inside the mutation
transaction; an old verifier cannot bless or invalidate a newer recapture.
Missing historical bindings require actual recapture, not a migration that
labels old evidence current. Revision binding is a repository-local integrity
check, not authentication of an external producer or a claim of cloud parity.

Governed branch publication uses ordinary non-forced creation or fast-forward,
or exact-tip reuse. Divergence and changed accepted targets block publication.
Configured validation owns its bounded, checked workspace-branch push without
a separate local operator approval. That exception is limited to the sealed
validation request; it does not relax unrelated external-write policy or the
provider's access checks. Validation does not authorize merges, releases, package
uploads, deployments or new credentials. A service denial cannot be repaired by changing credentials,
weakening gates or substituting fabricated evidence.

Release-only exceptions require immutable event history and exact material
classification. Names, labels and mutable refs provide no authority. Staged edits,
malformed events, unrelated base ancestry and synthetic merges with additional
source changes cannot obtain the exception. Generated hashes cannot hide changed
prose, templates, configuration or dependencies. Historical source-entrypoint
proof requires published Decapod lineage and the same immutable commit's package,
release and literal template; copied files in a consumer repository do not qualify.
Current candidate entrypoints still match the evaluating binary's template.

Persisted governance paths are project-relative with forward slashes, or
`<external-path>` for external locations. This is redaction, not encryption. The
active operation retains the actual path it needs.

## Storage maintenance and recovery

Integrity verification is read-only. Backup and recovery use the supported Dactyl
contract; ordinary opens, writes, migration notices and validation never authorize
raw SQLite edits, `REINDEX` or ad hoc database replacement.

Explicit recovery requires quiesced connections and filesystem writers. On
supported Linux/Android paths, Dactyl rebuilds in private same-filesystem staging,
preserves the original and sidecars at an unused sibling archive, retains owner,
group and safe mode, and activates only after verification with no-replace rename.
Decapod does not change the shared parent or chmod the active replacement.

Unsupported platform, ownership or metadata conditions are capability refusals,
distinct from a recovery that executed and failed. Codes such as
`secure_recovery_unsupported`, `recovery_ownership_unavailable` and
`recovery_metadata_unsupported` retain that distinction. `recovery_rollback_failed`
may leave the original at its archive path. Inspect that evidence before resuming; an error is never proof that
restoration succeeded. See `docs/storage-permissions.md` for the supported modes,
platforms and failure contract.

## Secret and dangerous-pattern checks

Gatekeeper's scanners are bounded heuristics, not complete parsing or dataflow
analysis. Explicit bearer syntax remains a finding even for short, dictionary or
fixture values. It recognizes supported header, assignment, quoted, backticked,
comment and standalone-line forms. Ordinary prose containing “bearer” is not
scheme syntax; concatenated credentials and schemes split from values across
lines remain outside this recognizer.

The Rust password classifier permits a narrowly proven environment-sourced named
format field only for an absolute standard formatting macro, a literal-key
`::std::env::var` call, optional parentheses and at most one `?` or argument-free
`unwrap`. Other matches on the line remain scanned. Literal defaults, unknown
calls, unproven captures and unresolved fields do not become exemptions.
Unsupported syntax, ambiguous namespaces, transforming attributes, malformed
source or uncertain byte mapping retain findings. The file-local model does not
expand macros or resolve parent-module and dependency namespace changes.

`govern gatekeeper check --paths` accepts one explicit file per repeated option;
missing, non-file or unreadable UTF-8 inputs fail closed. Staged-file discovery
checks Git's status and uses NUL-delimited paths, retaining removals for path
policy. Literal Rust `include_str!` dependencies are scanned inside the repository
with a 1,024-file bound; dynamic, missing, unreadable or escaping dependencies
cannot establish success. Classifiers discharge only the exact supported span.
SQL positional parameters require a terminal Dactyl connection sink or a
source-verified strict forwarding wrapper; constructing an operation or letting
its SQL escape is insufficient. Credential analysis can recognize supported
immutable runtime-generator and local Option/tuple flows, but literal alternatives,
mutation, ambiguous bindings and unavailable evidence retain findings. Primitive
comparison results can be inert text consumers; native shell and Dockerfile
rules still detect actual substitution and dangerous operations. Source analysis
cannot authenticate dependency downloads or infer unsupported whole-program flow.

Connection strings, keys and dangerous shell patterns are independent checks.
A SQL-looking string or a test filename cannot prove shell input harmless.
Passing a scanner cannot replace session, workspace, evidence or publication gates.

## Optional external execution

When selected, a validation provider supplies evidence for the same required
profile. Buildkite is one such provider. Check links locate runs; successful JSON
or a green result from another service does not authenticate execution. Reviewed
workflow, producer, source revision, native attempt, recipe and command outcomes
must match, with fresh final candidate and provider checks. Compiler and runtime
digests are independent pins; observed green jobs cannot authorize an upgrade.
Expected workflow filenames are joined under `.github/workflows/` and compared
exactly to the producer path, allowing only one optional `./` prefix. Observed
directories, traversal or green provider data cannot redefine that identity.

The public reader permits finite, reviewed GET routes. Artifact redirects are
bounded; job logs allow one HTTPS redirect to the exact verified provider object.
Foreign hosts, changed identities and extra hops fail closed. Signed query values
and arbitrary response headers stay out of diagnostics. Only identical artifact
GET timeouts may retry, at most twice within the original deadline; authorization,
HTTP, TLS, cancellation and integrity failures do not retry through another route.

Requests are authorized before concurrent collection. Only successful exact-route
responses are reused within bounded per-collection storage; verdicts, polls,
freshness checks, ZIP bodies and logs are not cached. Custody batches cap the sum
of per-stream allowances at 128 MiB, not total process memory. Larger supported
archives choose serial acquisition in advance. Denial or cancellation aborts
without later fallback or accepted partial proof. Archives are checked in memory
under size, count, type, path and expansion limits, one expansion at a time;
evidence members are never extracted or executed. Downloaded executable tools
require independently reviewed official digests before named-member extraction.

Source CI uses read-only repository access. Private scratch checkouts preserve the
exact committed tree without sharing object hard links, runtime databases or
relaxing source permissions. Reviewed build/test recipes must not execute arbitrary
imported YAML, inherit publication credentials or silently install missing tools.
Release and deployment authority remains separate.

## Reusable local connection custody

The opt-in storage owner retains the existing cooperative private/shared-group
socket boundary; caller metadata is attribution and is not an authenticated
claim of OS isolation. The existing #1476/#1485 command-child context replacement
continues to determine actor/session values used by policy and task writes.
The physical worker receives no session password or inherited actor identity.
Its protocol accepts only the canonical datastore path and bounded frames.
Read-only leases are enforced on the server, unsupported savepoint controls
are rejected, and every lease boundary attempts physical rollback. Known SQL
errors preserve their category; transport ambiguity poisons the proxy instead
of authorizing replay or another local execution path.

A mandatory parent-owned lifetime pipe and supervised worker shutdown bound
ownership; parent loss ends the worker even during SQL. Normal shutdown drains
an established lease for at most five seconds without admitting new leases,
then kills/reaps a worker that has not exited. Migration/restore and
physical ownership use the same stable sidecar lock inode. Backups must neither
copy nor replace runtime lock/election files or broker runtime directories.
No credential, daemon registration, new external audience, or remote service
is introduced. These are candidate invariants with regression coverage authored;
execution evidence remains required before publication readiness.

## Inventory privacy and authority

Native inspection uses caller OS authority, the existing lifecycle lock and a
Dactyl read-only snapshot. It bypasses initialization and does not replay
migrations or grant hosted access/paid capacity. Raw values, task IDs, principals
and content fingerprints stay private. Event classes are fixed public labels.
Unknown formats, skipped JSON, unsupported virtual tables and exhausted budgets
remain explicit. Header recognition does not verify compressed bodies, proof
chains or safe deletion. Accepted managed-cloud authority remains server-owned.

## Quinn transitive dependency remediation

The exact published Dactyl 0.14.0 registry dependency removes the older SDK
constraint that retained quinn-proto 0.11.9 in the inactive HTTP/3 lock graph.
Repository and fresh registry-consumer metadata now resolve quinn-proto 0.11.19;
the release-policy regression rejects any older locked entry, including inactive
ones. The configured dependency audit must still evaluate the exact candidate
without ignored advisories. Metadata resolution and an upstream release do not
establish current-source or hosted security proof. The generated Bazel lock must
agree with Cargo, and release-bound projections must match the evaluating release.
HTTP/3 remains disabled under the existing SDK feature configuration. Inactive
entries retain the same dependency audit requirement. This adoption changes no
transport policy, CI trigger, provider permissions or accepted-executor helper.

The reviewed provider importer identity is buildkite-gha 0.102.7. The validator
accepts exactly its independently reviewed official compiler and platform-specific
executable digests in `ReviewedBuildkiteImporter`; observed green jobs do not
select or authorize versions. The 0.102.5-to-0.102.7 source review covers diagnostic
redaction and error reporting, including the workflow-concurrency/output-derived
runs-on classification change from `E_EXPRESSION_INVALID` to `E_GRAPH_INVALID`.
Production runtime execution, serialized plan models and v2 schemas are unchanged.
Linux x86_64 release asset 642891563 and Darwin arm64 asset 642891564 were checked
against official archive digests before hashing their named executable members.
Old 0.102.5, observed 0.102.6, unknown versions, mixed tuples and swapped platform
identities remain rejected. Source, plan, runtime, artifact and accepted-executor
verification remain required. There is no provider-setting change, dynamic latest
acceptance or automatic controller bootstrap in this repair.

## Reviewed repository command boundary

Repository-local proof is admitted only for the exact supported profile after both
source and command/execution contracts match their reviewed digests. Its capability
permits reviewed native language/build executables without relaxing the existing
compiled Decapod profile. Program arguments are passed separately; workflow YAML
and shell text are never imported as commands. Direct shell, privilege/container
and recursive validator entrypoints remain denied. Cleared environment and owned
scratch avoid ambient credentials; declared installed bytes are measured around
execution. This is bounded execution and custody, not an OS sandbox or a proof that
arbitrary repository code has no external side effects. Review must exclude daemon,
network, external writes and publication operations. Unsupported behavior blocks
rather than producing a local substitute for required provider evidence.

Repository command authorization is a sealed exact-invocation operation minted
by the validated host, distinct from the generic executable allowlist. Existing
authored proof policy continues to deny disallowed programs; missing relevant
permissions in a present policy and malformed policy fail closed. No policy
file is rewritten as a repair, and generic callers cannot request the sealed
capability without the reviewed contract and measured invocation.

### External engines remain outside local proof

The native local-command boundary has no Dagger engine exception. Direct Dagger
invocations are rejected; retired Dagger execution and authorization fields are
unknown configuration errors, not silently ignored checks. This does not make
the local runner an OS sandbox: reviewed commands remain subject to its existing
external-access and non-daemon constraints. Agents own external engine tooling
and its permissions separately; arbitrary external CI results are not imported
as native proof. Buildkite custody and final readiness remain unchanged.

The root Cargo lock remediation changes only Tokio 1.52.3 to 1.53.3 and its
registry checksum. The derived Bazel lock must be regenerated under the pinned
Bazel toolchain before publication so dependency identities agree. Dependency
resolution is not audit/test success; required hosted checks remain mandatory,
with no ignored advisory or weakened validation requirement.

## CPU companion and pre-inference startup custody

Both local executable pins are frozen and verified with native-format, owner,
mode, single-link, bounded-size and SHA-256 checks. Setup verifies the optional
CPU companion before replacing its private binding; no shared policy path or
implicit executable discovery selects it. An absent ELF interpreter or empty
exit 127 can select only an explicitly pinned CPU companion, before a model
request is supplied. Typed hardware-only startup failures may select the same
worker's CPU path. Malformed capability reports, other exits/signals, timeout,
compute failure and negative/abstaining/malformed inference remain non-ready.
Attempt phase/runtime hashes distinguish startup from inference. A registered
native executable remains operator-trusted code; headers and self-reports are
not independent build provenance, dispatch, numerical or calibration evidence.

## Supplemental Rust worker verification

Supplemental Rust-worker proof preserves source-CI authority. It cannot execute
before a complete strict source verdict, replace that graph, import caller receipts
or authorize its own controller adoption. Current-source configuration, committed
manifest/lock, immutable snapshot, measured installed tools and fresh target reads
bind execution. Accepted recipes cannot be omitted or rewritten by candidate
policy. Typed host authorization remains required for both tool probes and checks.
Only fixed read-only xcrun/xcode-select probes are added; compiler execution,
developer-directory changes, arbitrary tools and extra probe arguments are rejected
by that capability. Actual Cargo execution uses the existing proof-exec boundary.

Native execution uses fresh private tool state and no ambient credentials or PATH
inheritance. This remains the existing reviewed-source model, not an OS sandbox
against malicious build scripts sharing the account. SDK metadata/tool hashes do
not attest all dynamic libraries, driver state or GPU behavior. Private output
objects retain existing permission/link defenses and byte bounds. A worker's
successful exit or self-reported backend cannot by itself establish model parity,
calibration, physical dispatch or publication authority.

### Fixture alias custody (#1457)

The reviewed fixture exception permits only a fixed source path and literal `target.txt` link payload with a separately measured regular file under the same fixture directory. It does not follow a caller-selected path, permit chains, accept a symlink target, skip sensitivity scanning or bypass denied paths. Committed, staged and working material must each satisfy its relevant alias/target checks, preventing a valid working link from laundering a malicious staged or committed link. Existing independent identity remeasurement, source immutability, private scratch, process and byte budgets remain required. This follows the existing reviewed toolchain-alias trust model; it is not an OS sandbox against concurrent malicious filesystem mutation or candidate code.

GPU one-shot ownership consumes the entire model before inference and discards it after synchronized completion, including failures, avoiding unsupported CUDA-only global reset and any uncleared-state reuse. The worker does not expose a reusable cross-request cache. The independent collector preserves every phase error and returns nonzero unless all required phases pass; malformed or missing private receipts fail closed. Sharing the strict typed CPU retry classifier with diagnostics adds no gate authority: inference errors, ordinary abstention, malformed IPC, stale assets or disabled fallback still cannot authorize another attempt. Real alternate-API unavailability in a diagnostic is not evidence that a working preferred GPU failed or that the full kernel publication/custody protocol ran.

### Public diagnostics and semantic evidence limits

The historical completed76 validation exposed false-ready outcomes against proxy labels and unexecuted/token-overflow cases on both backends. Those failed results remain preserved under their original criterion, which was superseded by the explicitly approved functional delivery scope in #1445/#1460; they are not passing accuracy evidence or a revived statistical publication prerequisite. The reference's zero-memory omission is repaired from independently pinned specification/config and own scalar block outputs, with analytic anchors rather than candidate-derived expected values. Shared authorship/parser/weights and512-token scope still limit independence. Actual CUDA-unavailable-to-CPU component execution does not establish preferred-Metal failure or full production custody/fallback routing. All raw numeric inputs and receipts remain private outside Git.

Public provider metadata and logs may diagnose a failed bootstrap without credentials. They remain untrusted diagnostic content: reading a sibling success or downloading a failure log cannot import a passed observation, alter the required graph or grant publication. A pre-parser helper launch failure establishes no source-test or native execution. Provider recovery must preserve failed history and bind the next real execution to the exact ordinary branch candidate. No authentication state, raw machine logs or model inputs belong in Git.

## Immutable configuration bootstrap trust boundary

The installed controller independently pins accepted CI carrier bytes. This PR
does not change those actions or replace their execution with an unmerged
controller. A diagnostic Rust parser migration was prepared privately but is
not activated or counted as current production proof. Existing immutable source
context and accepted executor checks remain authoritative. New actual-command
release tests execute the Rust candidate in owned temporary repositories, keep
session credentials outside Git, and distinguish bounded project prerequisites
from full publication readiness.

## Dependency advisory remediation

The separate native worker pins Parquet 59.3.1, whose reader no longer depends
on Thrift. This removes the Thrift 0.17.0 dependency reported by Dependabot #4
(GHSA-2f9f-gq7v-9h6m), rather than suppressing or dismissing the advisory.
Maintained Parquet 57/58 releases still require the affected dependency. The
existing pinned corpus size/hash checks and reader remain the execution boundary;
dependency resolution alone does not prove successful corpus decoding or model
execution. The model, tokenizer, engine, calibration and Git runtime pins and
numerical tolerances are preserved.

Scientific collection test fixtures explicitly request owner-only directory
permissions at creation instead of depending on the invoking shell's umask.
Production collection ownership/permission and unsafe-link checks are unchanged;
a public-directory regression verifies both creation and loading fail closed.

Compatible rustls-webpki 0.103.16 and webpki-roots 1.0.10 updates in the root and
worker locks address RUSTSEC-2026-0382 and RUSTSEC-2026-0381. Bazel metadata is
regenerated from the normal root Cargo lock and checked in strict lockfile mode.
Required hosted dependency checks and full validation still govern publication;
neither a lock update nor a public CI log is a passing receipt. Prior failed
validation and provider bootstrap/metadata rate-limit errors remain recorded.

## Approved ownership recovery

Handoff resolves policy from the actual caller, never from the optional expected
owner or intended receiver. A basic caller cannot acquire operator authority by
putting an operator label in either field. The transactional handoff gate uses
the same existing local cooperative operator trust interpretation as the broker;
other task operations and stored trust levels are unchanged. These role labels
are self-declared local authority, not OS authentication or remote credentials.

Cross-owner recovery requires explicit expected owner and an exact task-bound
live approval, consumed atomically on successful custody transfer. Prior owner,
actual actor, receiver and consumed approval IDs remain in the task event. Wrong,
expired, broad and already-consumed grants cannot authorize this recovery, and
failed preconditions do not consume authority. Dependency, lease, overlap and
completion-proof gates remain independently required. The new regressions are
unexecuted until official candidate validation succeeds.

Recovery never requires a human to execute Decapod beyond `decapod init`.
The AI agent/harness obtains the human decision and records its exact bounded
approval before acting under its already established authorized request identity.
Approval attribution does not elevate the executing caller; a harness must not
self-select a reserved role, promote its trust, or impersonate the previous owner
to evade a denial. Existing trust, exact-target single-use approval and custody
checks remain unchanged; this is an execution-contract correction, not new authority.

## Prompt context and HTTP diagnostics

The side-effect-free evaluator in `src/decapod/core/agent_eval.rs` distinguishes
an ordinary HTTP client reference from unsafe execution context: a read-only
public request in a diagnostic, API example or restored command history is not
itself a remote-execution finding. The original bytes remain the hashed subject,
and CLI responses never echo the prompt. Allowing inspection does not authorize
network access or command execution; those actions retain their normal gates.

This is a conservative common-marker detector, not a shell interpreter or a
semantic proof of benign intent. It inspects the entire prompt, including quoted,
fenced and historical material. Such labels grant no authority or exemption.
Known destructive/privileged actions, instruction overrides and credential
requests remain blocked. HTTP clients combined with shell execution, pipelines,
chained commands, process/command substitution, upload/authentication options,
credential headers and non-read-only request methods require review. File-output options are conservatively review-gated too: writing a fetched
program and invoking it on a later line must not escape detection merely because
the executable has an unfamiliar name. Ambiguous combinations may still require
review rather than silently suppressing evidence.

HTTP-specific context is bounded by inline-code and unquoted line boundaries;
every span is inspected, and adjacent shell operators and multiline substitutions
remain relevant across those boundaries. Quoted shell argument newlines remain
within the argument context. This prevents a prose semicolon after a closed
inline diagnostic, or an unrelated mention of Python/source, from inventing an
execution edge. It is deliberately not a general prose/shell grammar: unmarked
same-line prose and ambiguous shell syntax can still require review. Dynamic request arguments (shell command/process substitution, parameter
expansion and embedded backticks) are review-gated before lexical normalization
can erase their delimiters. This does not classify an arbitrary variable or
literal public query value as a known secret; it preserves uncertainty about
values that may be computed from local data. Dollar signs in prose outside a
closed inline diagnostic do not become request arguments. Existing whole-prompt
instruction/authority/destructive/credential markers remain intact.

HTTP options are classified through a narrow read-only allowlist, with
client-specific short flag meanings and existing method/header checks. Unknown
options request review rather than inheriting a benign classification: client
versions can add file-reading and opaque configuration features. Explicit static
`--url-query name=value` pairs are classified, but file-backed query arguments
are not. The [curl option contract](https://curl.se/docs/manpage.html#--url-query)
defines `name@filename` as file-backed input; attached header-file values and
variable/expanded-option mechanisms likewise remain review-gated. This bounded
classification deliberately does not promise a complete curl/wget interpreter.

`tests/unit/core/agent_eval_tests.rs` and `tests/agent_eval.rs` specify safe
report/history and adversarial combinations, including spacing, quoting and
escape variants. The public sanitized report is covered; the reporter's private
historical summary was not supplied and has not been reproduced. These authored
regressions require execution under full candidate validation before this change
can claim publication readiness.

The shared CI repair updates root Tokio from 1.52.3 to 1.53.3 for
RUSTSEC-2026-0386 and synchronizes its derived Bazel lock metadata. Both deltas
were reviewed and their exact before/after hashes verified locally; pinned Bazel
9.3 generated the shared metadata with the existing system JDK and normal TLS.
No required graph, trust, model, or execution policy changes. This dependency
repair is not runtime proof: the prompt-gate candidate still requires fresh full
validation against its exact published commit.

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

## Historical gap observations do not confer authority

History inspection is read-only and remains reachable without claiming a task.
Recording recovery is a governed mutation in the current actor's verified
checkout, not an administrator capability. It cannot authorize an exception,
replace a prior checkpoint, alter another actor's task, or certify earlier
history. It requires a distinct workstream with source ancestry rooted in the
accepted target. Ordinary resolution retains a digest/reference to the archived
observation; it does not make the old missing checkpoint pass verification.
A human-intent reference or local core label is not independently verified
maintainer authority. Offline exception authorization and project-scoped database
maintenance need separate accepted contracts; Propodus #88 owns future connected
authority. No such capability is silently enabled here.

## Exceptional RBAC and self-verification custody

The main repository's `[governance.rbac]` configuration declares separate
default-deny permissions for reviewed task-journal repair and explicit local
validation exceptions. Invalid, oversized, absent, symlinked or ambiguously
resolved main policy fails closed. Audited operations record actor label,
reviewed rationale and configuration digest. `DECAPOD_AGENT_ID` and local
reserved actor labels remain cooperative assertions, not cryptographic human
or cloud authentication; protect policy edits through human review.

A validation exception leaves proof and publication readiness false, and
cannot supply `validate_passes`, bypass CI or authorize publication. The
Nix-wrapped verification path permits only the exact canonical currently
running executable, never arbitrary similarly named executables. Readiness
hash normalization strips only per-run IDs and evidence digests, preserving
material, policy, decision and blocker semantics. Laya requires explicit
post-proof configuration and cannot waive deterministic validation.

## Current model policy and private registration

`laya_readiness::verify_policy_acceptance` rejects candidate-side disablement or
repinning of accepted required Laya policy. Private registration version 2 uses
`decapod.laya-local-runtime.v2`; unsupported versions produce explicit setup
instructions and leave the record untouched. Native format, canonical path,
ownership, single-link, byte-size and digest checks remain enforced. Model setup
uses bounded credential-free public asset transport. Opaque user configuration
extensions survive refresh but cannot enable evaluation or grant authority.

## Explicit managed project routing

The private project record is a routing cache, never authorization. Every selected route checks current server account/project status before a native operation, and the native operation independently authorizes membership/grant/assignment and current session. Forged metadata, account switches and denied references cannot silently select a different project or SQLite. The auxiliary file uses private no-follow regular-file checks, exact custody binding, bounded decoding and atomic durable replacement under the credential lock. A rejected data bearer is compared under the original Git checkout custody scope, even when the data route uses a managed project reference.

The resolved native project also supplies the repository field of newly created tasks. Before the first possible write dispatch, the existing machine-private todo operation journal atomically binds the operation to a digest of its resolved service/project route. Preflight login and initial project selection retain the same operation ID; subsequent selection changes cannot replay an uncertain or successful write into another project. Older unknown records remain bound to their original canonical repository semantics. The real PostgreSQL consumer exposed the former Git-label write mismatch; that failed candidate supplies no successful native proof.

## Versioned cloud mutation receipts

Operation-route binding must survive clients of different releases. Versioned receipt envelopes reject older array-only writers before dispatch, while the stable lock serializes migration with older in-flight writes. Both record count and actual encoded bytes are bounded before atomic replacement; an upgrade that would exceed the byte cap preserves the original evidence instead of truncating it.

## Canonical terminal rule context and proof-backed completion custody

Terminal rule evaluation (`core/validation_rules.rs::evaluate_terminal_rules`) derives `task_type` and `git_ref` from verified trajectory/branch task identities and canonical `refs/heads/<branch>` names without creating or mutating `.decapod/data` SQLite state. Plural store categories (`features`, `bugs`, `tests`) normalize to the built-in rule vocabulary (`feature`, `bugfix`, `test`) so pre-execution and publication scope/custody/evidence rules cannot be bypassed by category naming differences, while unmapped administrative categories fall back to `None` so `task_types`-scoped constitutional rules remain enforced. Undeclared workunits never block ordinary first-validation bootstrap, whereas any durably declared workunit remains fail-closed (`TERMINAL_WORKUNIT_UNPROVEN` or `UNVERIFIED_COMPLETION`) until its checkpointed execution binding and required proof-plan gates pass.

## PR 1666: accepted v0.143.2 source integration

Header parsing is bounded and accepts only unambiguous numeric quota hints from verified HTTP200 transport responses. A quota hint authenticates no evidence and authorizes no fallback identity. Exact repository/candidate/target and provenance checks remain necessary.

## PR1666 current-master integration

Preserve exact source/base checks, public-reader denial, final remote binding verification and accepted path/custody gates. GitHub PR metadata must identify the actual master tip before a fresh event is emitted. A successful process or historical test does not establish current independent readiness.

## PR1663 integrated storage behavior

Missing components, corrupt references, incompatible readers/writers and unauthorized custody must remain rejected. Storage migration1200 and format-v2 behavior stay confined to source and hosted disposable fixtures during this update. No direct machine datastore edit or candidate migration is authorized.

## PR 1662: accepted v0.143.2 source integration

Do not delete lock sidecars, signal foreign processes, steal claims, replay ambiguous writes or infer cloud privilege from local actor labels. Backup destinations and audit output remain bounded and confined. Default denial and accepted-policy revision checks survive integration with current master.

## PR1662 current-master preparation

Worktree policy edits cannot self-grant capabilities. db.recover remains owner-only and cloud authority fails closed when it cannot be authenticated. Retain non-destructive lock diagnosis, current-process-only release and confined backups. Coordination receipts are not independent acceptance proof.

## Bounded independent collection (#1573)

Only successful bounded quota headers affect polling. Denied requests remain
denials: there is no automatic alternate identity, authenticated fallback or
provider substitution. Hints cannot extend the deadline, erase cancellation,
authorize an endpoint or manufacture an execution receipt. Preserved real
complete collection on1658 does not silently complete this separate task.

## Required native dependencies cannot silently pass (#1568)

The required maintenance test helper now fails when native SQLite is missing;
it cannot return without running a purported integrity/backup/recovery check.
Governed fixture children remove inherited provider identity and diagnostic skip
flags and disable optional container claim startup. They retain real session,
task, workspace and custody admission. The parent job/provider context is never
mutated. Cloud principal environment spoofing remains a denial case, not a grant;
merged Propodus authority groundwork does not prove Decapod hosted consumption.

## SDK 0.16.1 authority and budget boundary (#1667)

The latest non-yanked registry artifact is pinned by checksum in Cargo and Bazel.
The SDK keeps transport/resource bounds distinct from application authorization.
No uncertain mutation is replayed by this upgrade, no cloud denial selects local
SQLite, and no connector feature grants repository policy or subscription access.
The existing Quinn protocol floor and storage features remain required. Native
fixtures and offline Supabase checks do not establish hosted deployment assurance.
Historical oversized-row tests construct corruption inside disposable SQLite
instead of evading the smaller SDK parameter-cell budget; the kernel's original
oversized-reader rejection remains asserted.

## Diagnostic variables never confer acceptance (#1659)

An unrestricted caller may select diagnostics but cannot use those flags to
obtain a full readiness or verified-completion observation through supported
current interfaces. The shared runtime denies before execution; publication and
completion also guard cached/historical admission. Existing successful evidence
is preserved as history, not rewritten into failure. New applicability still
requires current source, policy, task and custody checks.

Broker context treats diagnostic flags as caller-scoped values, removing prior
worker values before application. The originating local validated-completion
command rejects overrides before transport. These are kernel operation-boundary
guarantees, not OS filesystem isolation or a remote forge merge restriction.
Forged workunit declarations without overrides remain separately tracked by1660;
this change does not claim to discharge that independent proof obligation.

## Candidate repinning is not accepted policy (#1658)

The accepted target's local proof requirements remain authoritative even when a
candidate supplies a valid new digest for different commands, execution inputs
or coverage. `repository_profile::preserve_accepted` rejects those replacements
before selection; required failures cannot disappear through candidate review.
Accepted external limitations cannot become unsupported local-equivalence claims.

The regression boundary uses real Git targets and native subprocess checks,
including a replacement command that only prints claimed success. These tests
exercise kernel admission; a model refusal or a predetermined provider error
would not establish the guarantee. Accepted-target observation and the configured
executor remain trust roots. This repair does not claim OS isolation from an
unrestricted agent or add authenticated cloud authority.

### Hosted source-check substitution control

Buildkite9472 on candidate059dec1b actually returned local `ready=true` and
`proof_ready=true` after a candidate corrupted source, replaced its inventoried
check script with a success-printing script and repinned the source digest.
Command-only preservation was insufficient. The runtime now also preserves the
accepted local source-definition digest before selection. The failed observation
remains historical; fresh hosted execution must establish the repair. This is a
local-readiness finding, not evidence of bypassing an independent remote merge
service or of OS isolation from an unrestricted producer.

## Workunit producer and consumer boundary (#1660)

Missing, altered, contradictory, unbound or stale canonical observations cannot
be applied through supported workunit acceptance/completion/publication consumers.
The verifier rechecks candidate/base/material/policy/tool/input identities, live
task custody and durable workunit projection. Fresh complete terminal observation
is distinct from a previously declared success, avoiding a first-proof cycle.
History is retained after invalidation. Local hashes and a shared-UID datastore
are not an authenticated independent security domain; unrestricted direct SQL or
filesystem mutation requires separate isolation/enforcement. No signature, cloud
identity, arbitrary filesystem prevention or remote merge authority is asserted.

VERIFIED append and idempotent replay fence the canonical task checkpoint and
active lease on the same connection as workunit commit. A separately initialized
workspace workunit database cannot claim atomic acceptance against the parent
canonical task/proof store: WORKUNIT_PROOF_ATOMIC_DOMAIN_UNSUPPORTED blocks that
path. Its compatible migration/convergence remains unresolved. Tests exercise
the actual producer, owned task mutation and append boundary; hosted results
remain pending. Canonical task obligations cannot be replaced by a weaker
workunit declaration.

## Preserved uncertain legacy authority (#1657)

A placeholder decision uses no content timestamps and follows no symlink. Any
nonzero content, nonregular object, sidecar or unreadable/uncertain metadata is
not proof of emptiness. Journaled canonical authority cannot implicitly import
or delete that source. Real-journal regressions preserve logical digest/custody
and original input bytes across repeated reconciliation. This is no OS-level
filesystem isolation or permission to replay an unknown broker mutation; general
nonempty recovery remains unresolved and hosted proof is pending.

## Explicit legacy retirement trust boundary (#1657)

`legacy_todo_recovery::run` rejects cloud authority instead of falling back to
local recovery. Apply uses the existing strict local session verifier and the
canonical main-repository RBAC reader's distinct default-deny permission.
Candidate-worktree policy cannot grant physical retirement. Bounded no-follow
file reads bind SHA-256, device/inode, owner/group and mode; unknown links,
nonregular objects, unsafe ancestry and live SQLite sidecars block recovery.
Only the receipt-bound source/archive pair may temporarily have two hard links;
the shared state-file safety policy is unchanged.

A kernel-produced preparation is durable before archival; archive content and
identity are rechecked before unlink, then canonical bytes are checked again.
Corrupt or substituted receipts cannot supply missing task proof. Hashes detect
binding mismatch but do not authenticate local history. An unrestricted same-UID
agent or host administrator can still edit local files/session/policy; this
protocol provides no independent OS isolation or remote publication authority.
Stale prepared inputs require operator investigation; V1 has no automatic abort,
policy supersession or generalized historical-row reconciliation path.

## Nix publication ownership after squash releases (#1626)

Release subjects request packaging only within the existing accepted protected
workflow. Squash/linear support does not turn a PR title, candidate report or
local result into publication authority. The owner keeps empty token permissions,
anonymous exact-SHA fetch, native GitHub-hosted runner and protected-master push
requirements. Imported/self-hosted runners, PR/manual events, non-release
subjects and unsupported parent shapes cannot make that owner publish.

Cachix credentials remain scoped to the separate publisher after successful
locked packaging and smoke/reproducibility checks. Fixture URL rewrites apply
only to test child processes and contact a local Git repository. They neither
exercise real secrets nor prove actual native cache publication.

## Canonical local Git proof (#1069)

Canonical local proof shares the OS user trust domain with source, policy, tools and SQLite; it is not cryptographic authentication or OS isolation. The local authority is accepted on canonical Git rather than supplied by a candidate report, and cannot act as cloud/forge identity. Target reads disable ambient Git redirects, transport and replacement objects. Material/policy/target changes invalidate applicability without rewriting historical success. Supported forge publication rejects local-only authority; unrestricted shell/Git remains outside that enforcement boundary. No keys, credentials or machine-local logs belong in tracked state.

## Merge reconciliation fail-closed proof boundary (#1780)

`core::governance_document::reconcile_merge` enforces three invariants to prevent
Git merge reconciliation from laundering invalid or partial proof:
1. **Immutable parent authority**: Inputs come exclusively from `HEAD`,
   `MERGE_HEAD`, and `merge-base --all` blobs with graft and replace-object
   protections; corrupted or tampered working-tree `.decapod/governance.json`
   files outside an active Git merge (`MERGE_HEAD` absent) are rejected with
   `GOVERNANCE_MERGE_NOT_IN_PROGRESS`.
2. **Non-promotion of proof**: Merging a branch with failed or missing checks
   retains every failed check and missing proof hook, forces `proof_status` and
   `verdicts` recomputation via `TrajectoryArtifact::with_recomputed_hash()`, and
   strips stale `validation` receipts whenever trajectory or validation state
   changes. Contradictory statuses for the same named check fail closed with
   `GOVERNANCE_MERGE_PROOF_CONFLICT`.
3. **Authority disambiguation**: Unaccepted distinct workstreams (`change.id`
   mismatch where `MERGE_HEAD` is not an ancestor of the accepted `target_base`
   ref), conflicting `target_base` or `base_commit` values, or conflicting
   non-additive edits fail closed with `GOVERNANCE_MERGE_AUTHORITY_CONFLICT`.
   Full merge-commit checkpoint verification (`verify-checkpoints`) remains
   independently enforced after committing the merge.

## Per-platform execution selection boundary (#1736)

`repo.pr-readiness.v1` per-platform execution entries (`RepositoryExecution::platforms` / `PlatformExecution`) preserve fail-closed `#[serde(deny_unknown_fields)]` schema strictness, reject ambient `PATH` resolution, relative paths, mixed top-level/per-platform declarations, and uncanonical or unsupported platform keys, and bind every declared platform alternative into `command_contract_digest` and `preserve_accepted` (`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`). Candidate additions, removals, or edits to any platform table—including non-host platforms—cannot self-approve and remain rejected until independently accepted on the target (tracked separately from policy migration in #1738). At host selection and execution time (`RepositoryExecution::resolve_for_platform`, `repository_host::select_commands_for_platform`, `native_proof::validate_invocation`), only the current host platform's reviewed executable and `toolchain_files` are selected, checked for canonical host binary format, and byte-measured before and after execution; missing or unsupported host platforms fail closed without fallback.

## Compact storage v2 downgrade boundary (#1739)

Compatibility-aware evaluators below `0.144.2` (`0.141.0`, `0.143.2`, `0.144.1`) fail closed with `STORAGE_FORMAT_INCOMPATIBLE` against both newly initialized and restamped v2 datastores, and `core::events` read/write paths enforce `check_read_compatibility` / `check_storage_format_compatibility` before accessing event rows. Already-released `0.141.0..0.144.1` binaries shipped prior to PR #1663 without `meta.storage.*` compatibility checks and share `TODO_SCHEMA_VERSION = 18` and `events` SQLite DDL with `0.144.2..0.148.13`; metadata checks alone cannot force those historical binaries to reject a v2 store without breaking `>= 0.144.2` compatibility. Deterministic offline tests in `tests/datastore_relational_compaction.rs` and `tests/datastore_contract.rs` verify version floors, restamping, and lossless `_ce_v1` reconstruction without network or external binary execution.

## Fail-closed broker orphan acknowledgment (#1783)

`prepare_repair_with_probe` and `assess_writer_liveness` (`src/decapod/core/broker.rs`) enforce fail-closed orphan repair boundaries against reused PIDs, cross-host shared storage, and historical provenance-free audit entries. Confirmed active writers (matching boot/host/PID-namespace, live PID, and matching `writer_start_id`) are unconditionally refused in both preview and `--apply`, even when `--acknowledge-unknown-writer` is supplied.

Legacy provenance-free `pending` events, partial provenance lacking `writer_start_id` or `writer_boot_id`, cross-host boot mismatches, mismatched Linux PID namespaces, and uninspectable processes (`ProcessProbe::Unverifiable`) resolve to `WriterLiveness::Unknown` with `human_review_required = true`. Silent `broker repair --apply` is refused for all `Unknown` cases; applying requires explicit human review via `--acknowledge-unknown-writer`, records `writer_liveness`, `human_review_required`, and `acknowledge_unknown_writer` on the `"abandoned"` audit entry, preserves `original_outcome = "unknown"`, and remains gated by `control.mutate` policy and session checks.

## Actor custody enforcement and unknown-claim recovery (#1737)

`core::authoring::is_missing_or_unknown_actor` and
`core::authoring::authoring_actor_required_error` close the `"unknown"` / empty
actor bypass across task claiming, lease lifecycle (`renew`, `yield`), explicit
owner assignment, mentor reflection custody (`core::mentor::reflection::custody`),
and workspace provisioning (`workspace::ensure_workspace` for both CLI and RPC).
Failing closed at entry prevents `"unknown"` locks and unowned `agent/unknown/*`
coordination todos or worktrees from being created.

Recovery of legacy `assigned_to == "unknown"` claims is narrowly bounded:
`claim_task_with_lease` and `claim_branch_scoped_open_tasks` allow an explicit
real `DECAPOD_AGENT_ID` to adopt an `"unknown"` task only when
`core::authoring::has_real_actor_checkout_custody` and
`has_other_real_primary_owner` are both false. Tasks bound in `authoring_bindings`
to a real actor, holding another real primary owner, or actively claimed by a
real actor fail closed (`status: "conflict"` / `WORKSPACE_BRANCH_TODO_CLAIM_CONFLICT`).

## Acceptance presentation boundary (#1757)
The read-only flight-recorder acceptance exporter neither executes agent work
nor modifies kernel verdicts. It rejects sibling-file escapes, symlinks, altered
digests and unsafe public text. Raw tool arguments, reasoning, local paths,
credentials and environment stay private. Only reviewed objective text and
allowlisted facts are emitted. Host source labels are assertions, not producer
authentication; uncaptured unrestricted shell/forge activity remains unknown.
Wrong subjects, incomplete checks, missing custody, controller-only repair,
interventions and created-but-not-admitted PRs cannot become autonomous success.

## Fail-closed host attribution
Missing provider-requested tool identities and kernel calls outside an actual in-flight bash interval prevent an autonomous-admission classification. Express these rejection conditions directly, without lint exceptions or weakening missing-input behavior. Source CI must pass both the mandatory core/library regression owner and lint before publication.

## Candidate repinning and governed policy transitions (#1658, #1738)

The accepted target's local proof requirements remain authoritative even when a
candidate supplies a valid new digest for different commands, execution inputs,
coverage, or source inventory. `repository_profile::verify_accepted_transition`
and the built-in `decapod.policy.accepted_repository_contract` kernel rule
(`RuleAuthority::KernelInvariant`, `EvaluationPhase::PreExecution`,
`RuleSeverity::Critical`, `autonomous_remediation_permitted = false`) reject
unapproved replacements before execution selection; required failures cannot
disappear through candidate review or `governance.json` plan prose.

Legitimate `repo.pr-readiness.v1` transitions reuse Decapod's existing
`"policy.control"`-gated approval ledger (`src/decapod/plugins/policy.rs`,
`PolicyCommand::Approve` / `PolicyCommand::Revoke`). `PolicyCommand::Approve`
unconditionally enforces `core` trust for `"policy.control"` and rejects
`"unknown"` or empty `--actor` labels and malformed transition IDs. Approvals
are stored in the canonical main `.decapod/data` datastore (`decapod.db`, never
a worktree-local copy), carry bounded lifetimes (`expires_at` required), and
bind to the 64-hex-character `repo_policy_transition_id` covering exact
`(accepted_base_sha, candidate_head_sha, accepted_sources_digest,
candidate_sources_digest, accepted_commands_digest, candidate_commands_digest)`
under scope `"repo.pr-readiness.v1"`. Missing, unauthorized, wrong-scope,
expired, revoked, or stale approvals fail closed with
`ACCEPTED_REPOSITORY_CONTRACT_CHANGED`, and downgrading `decapod.pr-readiness.v1`
remains unconditionally blocked with `REQUIRED_PROFILE_DOWNGRADE`. The
consumer regression commits both profiles in separate Git revisions, records an
approval for that exact transition, and checks that the runtime refuses it
before selecting commands. Local cooperative actor labels and host-local SQLite
retain their shared-UID trust
boundary and do not claim cryptographic identity or OS sandboxing.

The regression boundary uses real Git targets and native subprocess checks,
including a replacement command that only prints claimed success. These tests
exercise kernel admission; a model refusal or a predetermined provider error
would not establish the guarantee. Accepted-target observation and the configured
executor remain trust roots. This repair does not claim OS isolation from an
unrestricted agent or add authenticated cloud authority.

### Hosted source-check substitution control

Buildkite9472 on candidate059dec1b actually returned local `ready=true` and
`proof_ready=true` after a candidate corrupted source, replaced its inventoried
check script with a success-printing script and repinned the source digest.
Command-only preservation was insufficient. The runtime now also preserves the
accepted local source-definition digest before selection. The failed observation
remains historical; fresh hosted execution must establish the repair. This is a
local-readiness finding, not evidence of bypassing an independent remote merge
service or of OS isolation from an unrestricted producer.

## Diagnostic trust and redaction (#1071)

Workspace diagnosis treats proposed config and binding overrides as candidate
material. Only the existing policy resolver supplies accepted authority,
with its local assurance level visible. Raw parser and transport errors,
credential values, and task intent text stay out of the diagnostic. Current
forge checks alone cannot establish Decapod admission.

## Source development release boundary

Preserving canonical source entrypoint metadata is conditional on the existing repository/origin/ancestry/template proof, not a configuration flag or caller-computed digest alone. The release verifier additionally compares the current entrypoint to the immutable classified head. This prevents a valid-looking replacement version from gaining authority during refresh. Existing exact current manifest, Dockerfile, source/configuration inventory and staged-edit checks remain enforced. The regression uses the existing published-root source fixture and includes header replacement, payload tampering and foreign-origin refusals; successful CI execution remains distinct from test registration.

The full Supabase-enabled library execution caught a test-fixture identity omission: the release-version helper must retain the canonical Cargo package repository field when it reuses the existing published-root source fixture. The correction changes fixture construction only; origin, ungrafted ancestry, committed template and immutable entrypoint checks remain strict. Prior failed candidate eee794ba and actual job136182283538 remain preserved.

Release projection integrity and retained observer-run completeness are separate proof surfaces. A successful version-only comparison cannot turn missing run boundaries or incomplete tool observation into an autonomous-admission claim; the existing acceptance observer retains that judgment. Preserving source-entrypoint versions grants no observation coverage.

## Decoder differential falsifiers and authority limits

Actual official0.153.2 rejected an unpaired surrogate that the prior Python decoder accepted; the prior independent decoder also accepted excessive nesting and overflowing integer tokens. The correction narrows that independent consumer to the existing native boundary. Positive twins preserve valid surrogate pairs, escaped control characters, distinct Unicode keys, finite large numbers and boundary-sized nesting. Malformed vectors must fail before project/session startup, with the specific JSON refusal rather than an unrelated initialization error. Diagnostic conformance reports identify actual consumers and inputs but do not authenticate an independent host, current proof, human consent, replay protection or cross-tenant authority. Those original campaign obligations remain open.

The encoding profile and independently retained agent-run coverage remain separate. Matching decoder reasons proves only the declared raw-input interpretation scope; it cannot substitute for required run boundaries, tool observations or the actual agent closing event. Missing observer coverage remains unknown even after every encoding vector passes.

The independent JSON encoding vectors remain a bounded parser compatibility check alongside accepted recorded-instruction readiness in `src/decapod/plan_governance.rs`. Agreement on Unicode, finite numbers, duplicate keys and nesting cannot converge unresolved instructions, satisfy spec review, or confer current publication authority.

## Release-only inventory qualification and publication privacy controls (#1838)

- **Fail-closed release-only boundary (`src/decapod/core/repository_profile.rs`)**: `qualifies_release_only_inventory_transition` never waives command/execution/coverage review (`commands_preserved` and exact `reviewed_commands_digest` equality remain required) and never permits candidate-authored edits to `Cargo.toml`, `Cargo.lock`, workflows, or other inventoried sources. Both line-level exact-diff checks and `toml::Value` AST roundtrips verify that only `[package].version` (`new > old` strict SemVer `X.Y.Z`) and the root `[[package]]` entry in `Cargo.lock` (with no `source` or `checksum`) changed between the bounded first-parent ancestor and `accepted_base_sha`. Any deviation, downgrade, dependency change, workflow edit, missing ancestor, or symlink/size-cap violation fails closed to `ACCEPTED_REPOSITORY_CONTRACT_CHANGED`.
- **Redacted public-boundary privacy lint (`src/decapod/core/validation_runtime/publication_privacy.rs`, `src/decapod/core/effective_grant.rs`)**: Local deny rules in private `.decapod/data/publication-deny.txt` (enforced via `fs_permissions::open_private_file`) and repo-configured `[governance].publication_deny_identifiers` (protected against candidate removal in `effective_grant.rs`) are matched literally against commit messages and non-deletion diffs. Findings fail closed with `PUBLICATION_PRIVATE_IDENTIFIER` referencing only the rule index/line and never echoing the matched secret/identifier back into logs.
- **Headless hypervisor boundary (`src/decapod/plugins/container.rs`)**: `start_podman_machine_quietly` scrubs `PODMAN_LOG_LEVEL` and `CONTAINERS_LOG_LEVEL` and rejects `--log-level=debug`/`--log-level=trace`/`--gui` so automatic workspace/validation startup cannot spawn a `vfkit` GUI window or steal desktop focus on macOS (#1417).

## Wave 7 Shard A Security & Custody Boundaries (#1047, #1134, #1388, #1573, #1626, #1890)

- **Fail-closed pre-build and shard aggregation gates (`assets/tools/source_ci/pre-build-validate.sh`, `assets/tools/source_ci/shard-status.sh`)**: Missing/tampered entrypoint or living-spec SHA-256 hashes terminate CI prior to build execution; shard aggregators (`Tests (core)`, `Tests (cli)`) reject skipped, cancelled, or failed prerequisites/shards.
- **Governed automation custody (`assets/tools/docs_autosync_governed.sh`)**: Scheduled docs sync never force-pushes, never skips exact-material checkpoints, verifies `govern artifacts verify-checkpoints --publication-material` before pushing, and fails closed with a drift report on concurrent branch movement or verification failure.
- **In-process ETag validation (`src/decapod/core/snapshot_provider/mod.rs`)**: ETag headers are strictly validated as printable ASCII (length 2..=256, single header, no CR/LF) to prevent header injection; the validator map is strictly in-process (max 64 entries, never persisted), always performs live TLS revalidation, and fails closed on unexpected HTTP 304 without prior ETag.
- **Native-only Cachix tag publication (`.github/workflows/nix-flake.yml`)**: `publication-owner` restricts SemVer `refs/tags/v*` publication to `github-hosted` native Actions runners with non-Buildkite identity and valid 1-parent or 2-parent `chore: release v*` commits, keeping `secrets.CACHIX_AUTH_TOKEN` isolated to the single `publish` job.

## Guarantee-to-Enforcement Binding & Cloud Proof Receipt Validation (#1964)

- **Fail-closed Cloud proof receipt validation (`src/decapod/core/cloud_proof.rs`)**: `submit_capture`, `submit_verify`, `submit_admit_or_publish`, `submit_retire_historical`, and `reconcile_operation_after_unknown` share `execute_proof_transition` and validate `version`, `profile`, lifecycle `state`/`status`, `operation_id`, `attestation_id`, `fingerprint`, `candidate` (`commit`/`tree`), `accepted_target.commit`, `policy_digest`, `task_revision`, `fencing_generation`, and `verified_evidence_digest`, refusing privilege-escalation flags (`forge_merge_authority = true`, or `acceptance_supported`/`publication_eligible = true` on `capture`/`verify`/`admit`). An authenticated HTTP `200` receipt for the wrong subject or stale digest fails closed with `CLOUD_PROOF_RECEIPT_INVALID`.
- **Cross-layer claim matrix & drift gate (`docs/security-assurance-matrix.md`, `tests/security_guarantee_assurance_1964.rs`)**: Maps published security/correctness claims across `README.md`, `SECURITY.md`, `docs/trust-and-egress-inventory.md`, `docs/publication-admission-acceptance.md`, `docs/book/src/reference/cloud-projects.md`, and `bindings/typescript/README.md` to exact enforcement components, evidence paths, and residual assumptions (`proven_for_exact_subject`, `trusted_assertion`, `conditional`, `not_proven`, `unsupported`), prohibiting unqualified absolute claims (`unspoofable`, `100% guaranteed`, unqualified `zero telemetry`).
- **Mutation coverage extension (`tests/security_guarantee_assurance_1964.rs`)**: Preserves `#1795` intact and adds isolated single-fault mutation detection across uncovered boundaries `RULE-APPROVAL-006` (`governed_transition::admit`), `RULE-CLOUD-PROOF-008` (`cloud_proof::assert_candidate_acceptance_envelope`), and `RULE-HTTP-READONLY-009` (`http_transport::read_only_only`).

<!-- decapod:codebase-contract:start -->

## Codebase Contract Snapshot

- Security/policy/auth surfaces (38): `.config/build/cargo_inputs_policy.py`, `.github/workflows/security-audit.yml`, `src/decapod/core/agent_eval.rs`, `src/decapod/core/auth.rs`, `src/decapod/core/authoring.rs`, `src/decapod/core/capsule_policy.rs`, `src/decapod/core/path_policy.rs`, `src/decapod/core/repository_policy_approval.rs`, `src/decapod/core/snapshot_provider/read_authorization.rs`, `src/decapod/core/trust_egress.rs`, `src/decapod/core/validation_runtime/snapshot/source_policy.rs`, `src/decapod/core/validation_runtime/snapshot/source_policy/real_compiler_tests.rs`, `src/decapod/plugins/eval.rs`, `src/decapod/plugins/policy.rs`, `tests/agent_eval.rs`, `tests/build_layout_policy.rs`, (+22 more)
<!-- decapod:codebase-contract:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `307ac4647f236b100d139c0c51ab31205678f6303929749ad6230df18802a088`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (41 files), `bindings/` (9 files), `docs/` (3 files), `packages/` (4 files), `src/` (286 files), `tests/` (297 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->
- Repository signal fingerprint: `f5b26099b3c76eff512a47775e0a5500ba3ed7c5bac4e4f69fd30a1358fd6205`

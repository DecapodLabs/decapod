# Observed Interoperability Conformance Receipts (`v0.163.0`)

Cross-references: `#870`, `#871`, `#872`, `#873`, `#875`, `#1733`, `#1735`.

## Evidence Boundary

These measurements were captured against the installed released `decapod` binary (`0.163.0`) in disposable local Git repositories under `/private/tmp/decapod-w7-exp-interop`. Raw stdout transcripts were hashed immediately upon capture. They establish **local black-box protocol, framing, redaction, gate-parity, and multi-consumer conformance** (`ProducerTrustClass::IndependentLocalObserver`). They do **not** claim hosted cross-organization CI signing, vendor pre-bundling, or live multi-host matrix completion where a host was unauthenticated or absent.

## Pinned Executables & Environment (2026-11-22 UTC)

| Component | Version | Binary SHA-256 |
| --- | --- | --- |
| `decapod` | `0.163.0` | `9ef9ac7242f7dfb609768dbbe823f304d2b2dfffbd497c8b453f85ffd2ce1762` |
| `node` | `v24.18.0` | `ee6fb0e015284d83a91e8ec5213f43a157f8a392b58555301682892ba928c04a` |
| `python3` | `3.9.6` | stdlib-only reference consumer (`tests/fixtures/agent_interop_reference_consumer.py`) |

## 1. Real Stdio MCP Session (`#872`)

- **Transport:** `decapod mcp serve --framing content-length` (plus `--framing jsonl` adversarial suite).
- **Observed session (`Content-Length` framing):** 14 frames sent, 12 JSON-RPC responses emitted in `10.15 ms`, process exited `0` cleanly on `shutdown` with zero lingering background processes.
- **Raw transcript digest:** `sha256:e4a63611b259f68131d9ebd4cff81381de9e85db0c11c336b61ed895e6ef05ac`.
- **MCP schema digest (`decapod mcp schema`):** `sha256:` captured deterministically; exposes 6 typed tools and 4 bounded redacted resources (`decapod://manifest`, `decapod://constitution/core/DECAPOD`, `decapod://rules/taxonomy`, `decapod://harness/matrix`).
- **Observed positive & fail-closed checks:**
  - Unsupported `protocolVersion: "1999-01-01"` -> rejected (`-32602`).
  - Supported `protocolVersion: "2025-03-26"` -> initialized cleanly.
  - `resources/read` on `decapod://constitution/core/DECAPOD` -> returned bounded markdown excerpt.
  - `resources/read` on `decapod://../.env` and `decapod://hidden/rubric` -> denied (`-32602`, `resource_denied`).
  - `tools/call` on `decapod_interop_manifest` and `decapod_interop_negotiate` -> succeeded (`isError: false`).
  - `tools/call` on `decapod_governed_mutate` in default read-only mode -> denied (`isError: true`, `code: "read_only_session_denied"`).
  - `notifications/cancelled` for request `11` -> request `11` returned `-32800` (`request cancelled by client`).
  - Adversarial JSONL stream (`--max-frame-bytes 512`): malformed JSON -> `-32700`, duplicate JSON key -> `-32700`, oversized 2 KiB frame -> `-32600`.
- **Gate parity in isolated worktree (`--allow-mutations`, verified in `tests/agent_interop_conformance.rs`):**
  - Default `decapod mcp serve` -> `read_only_session_denied`.
  - `decapod mcp serve --allow-mutations` without valid session -> `session_required`.
  - `decapod mcp serve --allow-mutations` with valid session on protected `master` -> `worktree_isolation_required`.
  - `decapod mcp serve --allow-mutations` with valid session inside claimed `.decapod/workspaces/*` worktree -> `admitted_for_kernel_dispatch` (`isError: false`).

## 2. Multi-Process A2A Agent Card & Handoff Validation (`#873`)

- **Agent Card (`decapod a2a card`):** exit `0`, digest `sha256:920278c1ac84ddc1cd4923fcc33d736d318c2a3949392b369effa81ff4f062bf`.
  - Advertises 4 executable governance skills only (`governance.orient`, `governance.handoff.validate`, `governance.overlap.classify`, `governance.rules.evaluate`).
  - Reports core A2A operations (`agent_card.get`, `task.handoff.validate`, `task.artifact.inspect`) separately from Decapod governance extensions (`custody_lease_generation`, `interlock_references`, `proof_lineage_commit_binding`, `local_authority_isolation`).
  - `daemonless_local_compatible: true`.
- **Multi-process handoff matrix (`decapod a2a handoff --json ...`):**

| Case | Exit Code | `admissible_for_continuation` | `imported_authority_granted` | Reason Codes |
| --- | ---: | --- | --- | --- |
| `valid_handoff` | `0` | `true` | `false` | `[]` |
| `tampered_digest` | `1` | `false` | `false` | `["handoff_digest_tampered"]` |
| `stale_head` | `1` | `false` | `false` | `["stale_head_commit", "stale_proof_commit"]` |
| `moved_base` | `1` | `false` | `false` | `["moved_accepted_base"]` |
| `cross_repo_replay` | `1` | `false` | `false` | `["cross_repository_handoff_denied"]` |
| `stale_lease` | `1` | `false` | `false` | `["stale_custody_lease_generation"]` |
| `claims_local_authority` | `1` | `false` | `false` | `["imported_authority_claim_denied"]` |
| `expired_handoff` | `1` | `false` | `false` | `["handoff_expired"]` |
| `canceled_state` | `1` | `false` | `false` | `["non_continuable_task_state"]` |

## 3. Independent Reference Consumer Vector Parity (`#870`, `#871`, `#1733`)

- **Python stdlib consumer (`tests/fixtures/agent_interop_reference_consumer.py`) over `tests/fixtures/agent_interop_vectors.v1.json`:**
  - Exit `0`, `41.5 ms`, 26/26 vectors evaluated, output digest `sha256:3673e6b92ec6cd49f6b732edfb51b917d9579e0eb713cd8650862ac98bf9b195`.
  - `all_imported_authority_false: true` across every single vector.
- **Rust kernel + independent raw-JSON Rust consumer + Python consumer:** 3-way verdict & reason-class agreement enforced by `tests/agent_interop_conformance.rs::independent_consumers_agree_on_all_conformance_vectors`.
- **Two-way dispatcher/authorization drift (`#871`):** `check_dispatcher_authorization_drift` verifies 100% alignment across `interop::RPC_OPS` (including `observe.telemetry`), manifest capabilities, and session guards.

## 4. Live Pi Extension Lifecycle Arm (`#875`, `#1277`)

- **Execution:** `node` `v24.18.0` running `@decapodlabs/pi-decapod` wired directly to the real `/Users/arx/.cargo/bin/decapod` (`0.163.0`) binary inside a disposable Decapod-governed worktree (zero mocks).
- **Observed results (`80.28 ms` total process time; `8 ms` lifecycle sequence):**
  - `onPromptReceived` invoked real `decapod eval --stdin --format json` -> `promptEvalAllowed: true`, `humanApprovalAuthenticated: false`.
  - Direct write bypassing hook (`invokedThroughDecapodHook: false`) -> refused (`bypassAllowed: false`, `bypassBoundary: "outside_native_enforcement"`).
  - Unconverged plan (`planConverged: false`) -> refused (`unconvergedAllowed: false`).
  - `switchModelOrHarness` -> preserved original human prompt verbatim (`swappedPreservedOriginalPrompt: true`).
  - Converged hook invocation -> admitted at `cooperative_hook` boundary with `provenanceDigest: sha256:346c5c1bb262c82577e5a9ac404824b97d719c8fb8e5566853ec7bb4aad329ff` and explicit note that kernel validation remains required for admission.

# Governance Audit (Decapod Kernel Lens)

Source note: the referenced post body was not included in the prompt payload; this audit uses the provided capability buckets as the authoritative source material.

## 1) Agent-Infra Capability Buckets

### Seamless identities across platforms
- Implies: agents need portable identity and trust continuity across tools and services.
- Decapod kernel version: session-bound identity (`agent_id + ephemeral password`) plus auditable invocation/proof receipts.
- Kernel vs external steward: split; identity attestations and policy boundary in-kernel, provider-specific federation outside kernel (steward).
- Minimal primitive: `identity.attest` artifact linking session token hash, actor, scope, and proof obligations to a deterministic receipt chain.

### File systems / databases for sessions & shared data
- Implies: persistent memory/state for autonomous execution and collaboration.
- Decapod kernel version: strict store purity with explicit user/repo separation and append-only/auditable ledgers for promotion-relevant state.
- Kernel vs external steward: in-kernel.
- Minimal primitive: canonical store manifest classifying each file/table as `canonical` or `derived` with a validate gate that blocks promotion on contamination.

### Collaboration with people
- Implies: human-in-the-loop delegation, handoff, and review loops.
- Decapod kernel version: TODO claim/ownership/handoff/presence with auditable event logs and policy-gated high-risk operations.
- Kernel vs external steward: in-kernel for coordination primitives; UI workflows outside kernel.
- Minimal primitive: `handoff.receipt` linking task id, from/to actors, summary, and policy approval evidence.

### Safe ways of spending/managing money
- Implies: autonomous financial actions need bounded controls, approvals, and traceability.
- Decapod kernel version: governance primitive for spend authority, not payments integration.
- Kernel vs external steward: split; authority policy in-kernel, payment rails entirely outside kernel.
- Minimal primitive: typed `spend.capability` envelope (budget, scope, expiry, approver) enforced as a precondition gate on spend-labeled operations.

### Computers to execute code / tasks (sandboxes, runners)
- Implies: reliable execution substrate for agent actions.
- Decapod kernel version: containerized, isolated workspace execution with deterministic safety defaults and runtime preflight.
- Kernel vs external steward: in-kernel for execution policy and artifacts; external for fleet orchestration.
- Minimal primitive: `runner.proof` artifact containing runtime profile, workspace ref, command, exit status, and evidence hashes.

### Oversight, responsibility, and privacy asymmetry
- Implies: operators need asymmetric visibility and accountability over agent actions.
- Decapod kernel version: provenance manifests, broker audit trails, actor/session binding, and policy checkpoints.
- Kernel vs external steward: in-kernel for accountability primitives; external for dashboards/reporting.
- Minimal primitive: immutable `accountability.record` per promotion-relevant command with actor, scope, policy decision, and evidence pointers.

### Agents drifting / not knowing when they’ve gone astray
- Implies: autonomous systems must detect and recover from drift/failure.
- Decapod kernel version: bounded validate termination, typed failure markers, and deterministic verification/gate surfaces.
- Kernel vs external steward: in-kernel.
- Minimal primitive: `drift.interlock` requiring typed reason code + remediation artifact before retries on promotion paths.

### API-first tooling (CLIs/APIs are agents’ native tongue)
- Implies: all core capabilities should be API-native and composable.
- Decapod kernel version: CLI + RPC envelope contracts, schema surfaces, and golden vectors.
- Kernel vs external steward: in-kernel.
- Minimal primitive: versioned control-plane envelope schema with immutable goldens and semver-gated compatibility checks.

## 2) Reality Check: Do We Actually Have This?

| Claim | Where in repo | Proof gate/test | Status |
|---|---|---|---|
| Validate terminates boundedly with typed lock timeout | `constitution/interfaces/CLAIMS.md` (`claim.validate.bounded_termination`), `src/lib.rs` (`run_validation_bounded`, `VALIDATE_TIMEOUT_OR_LOCK`) | `tests/validate_termination.rs` | VERIFIED |
| RPC envelope compatibility is pinned | `tests/golden/rpc/v1/agent_init.request.json`, `tests/golden/rpc/v1/agent_init.response.json` | `tests/rpc_golden_vectors.rs` | VERIFIED |
| STATE_COMMIT v1 vectors are immutable and bump-gated | `src/core/validate.rs` (`validate_state_commit_gate`), `tests/golden/state_commit/v1/*` | `decapod validate` STATE_COMMIT gate; `tests/state_commit_phase_gate.rs` | VERIFIED |
| Session authN boundary requires ephemeral password | `src/lib.rs` (`ensure_session_valid`, password hash checks), `constitution/specs/SECURITY.md` | `tests/entrypoint_correctness.rs` (`test_agent_session_requires_password`) | VERIFIED |
| Store purity (blank-slate/no-auto-seeding) is enforced | `constitution/interfaces/STORE_MODEL.md`, `constitution/interfaces/CLAIMS.md`, `src/core/validate.rs` (`validate_user_store_blank_slate`) | `decapod validate --store user` (no dedicated standalone test) | PARTIAL |
| Collaboration primitives (claim/handoff/ownership/presence) are implemented | `src/core/todo.rs`, `constitution/plugins/TODO.md` | `tests/plugins/todo.rs`, `tests/cli_contracts.rs` | VERIFIED |
| Container runner isolation and safety defaults are enforced | `src/plugins/container.rs`, `constitution/plugins/CONTAINER.md` | `src/plugins/container.rs` unit tests, `tests/cli_contracts.rs` | VERIFIED |
| Promotion requires provenance manifests | `src/core/workspace.rs` (`publish_workspace` checks), `src/lib.rs` (`release.check`) | runtime gate in `decapod workspace publish`; no direct dedicated test | PARTIAL |
| Oversight/privacy asymmetry as explicit accountability primitive | `constitution/specs/SECURITY.md`, `docs/VERIFICATION.md`, broker audit code | documentation + mixed tests, no single explicit accountability gate | PARTIAL |
| Money/spend governance primitive exists | no canonical interface/claim for spend authority | missing | MISSING |
| Cross-platform identity attestation chain exists | session auth exists, but no portable attestation artifact/chain | missing | MISSING |

## 3) Worthy of Including in Decapod (Max 3)

### A) Identity Attestation Chain (kernel primitive)
This directly strengthens Decapod’s thesis because promotion trust is actor-bound. Decapod already has session auth, but not a durable, transportable attestation artifact that can cross tool boundaries without importing provider-specific identity stacks. A small attestation primitive makes “who did what under what scope/policy” independently verifiable in repo-native artifacts.

Smallest kernel-shaped primitive:
- Interface: `constitution/interfaces/IDENTITY_ATTESTATION.md`
- Artifact: `artifacts/attestations/session_attestation.jsonl` (append-only)
- Envelope fields: `attestation_id`, `agent_id`, `session_token_hash`, `scope`, `declared_proofs`, `issued_at`, `expires_at`, `evidence_refs`
- Gate: validate attestation integrity + presence for promotion-relevant ops

Will NOT do:
- No OAuth provider adapters, social login, or external identity broker integrations in-kernel.

### B) Spend Authority Capability (governance-only)
The money bucket is valid only as policy and accountability in-kernel. Decapod should model permissioned spend intent and approval evidence, not execute payment rails. This keeps surface area minimal while giving operators deterministic boundaries for high-risk actions.

Smallest kernel-shaped primitive:
- Interface: `constitution/interfaces/SPEND_AUTHZ.md`
- Artifact: `artifacts/policy/spend_capabilities.json`
- Command surface: schema/envelope only (`policy.spend.authorize`, `policy.spend.verify`)
- Gate: promotion-blocking if spend-labeled actions lack valid capability artifact

Will NOT do:
- No direct payment processor clients, card vaulting, invoicing, or treasury workflows.

### C) Drift Interlock with Mandatory Remediation Artifact
Decapod already has bounded validate termination and typed failures; the missing piece is a deterministic interlock contract that prevents retry storms and “just rerun until green” behavior. A remediation artifact requirement converts failure handling into auditable governance behavior.

Smallest kernel-shaped primitive:
- Interface: `constitution/interfaces/DRIFT_INTERLOCK.md`
- Artifact: `artifacts/diagnostics/drift_remediation/<id>.json`
- Command contract: retry of promotion-relevant ops requires `remediation_id` when prior failure is typed drift/lock
- Gate: reject retries without remediation artifact and reason code alignment

Will NOT do:
- No autonomous “self-healing planner” product layer in-kernel; only typed interlocks and evidence checks.

## 4) 1-Shot-able Dependency TODO Tasks

### DCP-401
- Goal: Introduce identity attestation interface and claim registry entries.
- Preconditions: none.
- Files to change/add:
  - `constitution/interfaces/IDENTITY_ATTESTATION.md` (new)
  - `constitution/interfaces/CLAIMS.md`
  - `constitution/core/INTERFACES.md`
- Acceptance criteria:
  - `decapod validate` passes.
  - `cargo test --all-features --test canonical_evidence_gate -- --test-threads=1` passes.
- Proof/Gate impact: new explicit claim definitions for attestation become tracked and auditable.
- Risk level: LOW (docs + claim registry alignment).
- Estimated diff size: S.

### DCP-402
- Goal: Emit deterministic session attestation artifact on `session acquire`.
- Preconditions: DCP-401 merged.
- Files to change/add:
  - `src/lib.rs` (session acquire path)
  - `src/core/schemas.rs` (if schema helper is needed)
  - `docs/VERIFICATION.md`
  - `tests/session_attestation.rs` (new)
- Acceptance criteria:
  - `decapod session acquire` writes `artifacts/attestations/session_attestation.jsonl` with deterministic schema fields.
  - `cargo test --all-features --test session_attestation -- --test-threads=1` passes.
  - `decapod validate` passes.
- Proof/Gate impact: attestation artifact existence + shape can be enforced.
- Risk level: MED (touches session lifecycle path).
- Estimated diff size: M.

### DCP-403
- Goal: Add validate gate: promotion-relevant ops require valid session attestation.
- Preconditions: DCP-402 merged.
- Files to change/add:
  - `src/core/validate.rs`
  - `src/core/workspace.rs` (publish precondition alignment)
  - `constitution/interfaces/CLAIMS.md` (claim enforcement status update)
  - `tests/attestation_gate.rs` (new)
- Acceptance criteria:
  - `decapod workspace publish` fails with typed error if attestation missing/invalid.
  - `cargo test --all-features --test attestation_gate -- --test-threads=1` passes.
  - `decapod validate` passes.
- Proof/Gate impact: `claim.identity.attestation_required_for_promotion` becomes enforced.
- Risk level: MED (promotion path gating).
- Estimated diff size: M.

### DCP-404
- Goal: Introduce spend authorization interface and claims as governance primitive only.
- Preconditions: none.
- Files to change/add:
  - `constitution/interfaces/SPEND_AUTHZ.md` (new)
  - `constitution/interfaces/CLAIMS.md`
  - `constitution/core/INTERFACES.md`
- Acceptance criteria:
  - `decapod validate` passes.
  - `cargo test --all-features --test canonical_evidence_gate -- --test-threads=1` passes.
- Proof/Gate impact: spend authority semantics and claim IDs become canonicalized.
- Risk level: LOW (spec-only).
- Estimated diff size: S.

### DCP-405
- Goal: Add typed spend capability artifact parser + schema contract.
- Preconditions: DCP-404 merged.
- Files to change/add:
  - `src/lib.rs` (`schema.get`/policy command hooks)
  - `src/core/schemas.rs`
  - `tests/spend_capability_schema.rs` (new)
  - `artifacts/policy/spend_capabilities.example.json` (new)
- Acceptance criteria:
  - `decapod data schema --subsystem policy` includes spend capability schema.
  - `cargo test --all-features --test spend_capability_schema -- --test-threads=1` passes.
  - `decapod validate` passes.
- Proof/Gate impact: spend authority moves from intention to machine-validated artifact shape.
- Risk level: MED (new schema surface).
- Estimated diff size: M.

### DCP-406
- Goal: Enforce spend capability on spend-labeled operations with typed failures.
- Preconditions: DCP-405 merged.
- Files to change/add:
  - `src/core/policy.rs`
  - `src/lib.rs` (operation dispatch checks)
  - `tests/spend_policy_gate.rs` (new)
  - `constitution/interfaces/CLAIMS.md` (enforcement status updates)
- Acceptance criteria:
  - spend-labeled operation without capability returns typed policy denial.
  - with valid capability artifact, operation proceeds.
  - `cargo test --all-features --test spend_policy_gate -- --test-threads=1` passes.
  - `decapod validate` passes.
- Proof/Gate impact: `claim.spend.capability_required` becomes enforced.
- Risk level: HIGH (policy gating of operational flow).
- Estimated diff size: M.

### DCP-407
- Goal: Define drift interlock interface + remediation artifact contract.
- Preconditions: none.
- Files to change/add:
  - `constitution/interfaces/DRIFT_INTERLOCK.md` (new)
  - `constitution/interfaces/CLAIMS.md`
  - `constitution/core/INTERFACES.md`
- Acceptance criteria:
  - `decapod validate` passes.
  - `cargo test --all-features --test canonical_evidence_gate -- --test-threads=1` passes.
- Proof/Gate impact: drift remediation contract is canonical and claim-tracked.
- Risk level: LOW (interface-level).
- Estimated diff size: S.

### DCP-408
- Goal: Enforce retry interlock for typed drift/lock failures via remediation artifacts.
- Preconditions: DCP-407 merged.
- Files to change/add:
  - `src/lib.rs` (retry path / command precondition)
  - `src/core/validate.rs` (reason-code mapping helper exposure)
  - `tests/drift_interlock.rs` (new)
  - `docs/VERIFICATION.md` (new repro commands)
- Acceptance criteria:
  - after `VALIDATE_TIMEOUT_OR_LOCK`, promotion-relevant retry without remediation artifact fails deterministically.
  - with valid remediation artifact, retry is allowed.
  - `cargo test --all-features --test drift_interlock -- --test-threads=1` passes.
  - `decapod validate` passes.
- Proof/Gate impact: `claim.drift.remediation_artifact_required` becomes enforced.
- Risk level: MED (control-plane retry behavior).
- Estimated diff size: M.

## 5) Guardrails

1. Any new capability that can influence promotion MUST have a claim, schema artifact, and enforcing gate before it is marked REAL.  
2. Decapod kernel scope ends at governance primitives; provider integrations (identity/payment/orchestration adapters) must remain external steward concerns.  
3. No user-scoped or transient state may influence promotion unless it is materialized into repo-native, hash-verifiable artifacts.  
4. Typed failure modes are mandatory for all interlocks; warnings must never silently degrade promotion gates.  
5. Compatibility promises (CLI/RPC schemas and golden vectors) must not be expanded faster than deterministic enforcement coverage.  

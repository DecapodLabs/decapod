## 0) Decision Boundary

Decapod is an enforcement kernel, not a developer utility.

If a proposed change increases capability without increasing provable correctness inside explicit boundaries, it is a kernel regression.

## 1) Review Scope and Audit Trail

This file is canonical only when it is evidence-backed.

Required fields for every revision (must be filled to qualify as a Canonical Evidence Artifact):

* Reviewed commit / tag: _______________________________
* Reviewer(s): ________________________________________
* Review date: ________________________________________
* Coverage manifest path: ______________________________
* Not reviewed (explicit list, may be “none”): __________

Citation policy:

* Any statement about current runtime behavior must include file path + line range (or symbol + file).
* Any statement about intent/constitution/spec must cite the doc source.
* Any mismatch must be recorded in the Drift Ledger (Section 4).

Two-stage canon:

* Canonical Framework: structure + hypotheses (allowed “REPORTED/UNVERIFIED”).
* Canonical Evidence Artifact: header fields filled, citations present, and at least one drift row closed with a failing→passing test/proof.

## 2) Kernel Thesis and Thin Waist

Decapod turns agent work into governed change by forcing all meaningful state transitions through a narrow corridor (the “thin waist”) where actions are:

* attributable (who/what caused it),
* serialized (in what order),
* replayable (state can be rebuilt),
* proof-gated (“done” is validated),
* coordinated (multi-agent doesn’t corrupt outcomes).

If Decapod cannot deterministically reconstruct its own state from its event logs, the thin waist is broken.

## 3) Architecture Deep Preview (Current vs Target, explicitly split)

### 3.1 System Boundaries (Current)

Decapod mediates between:

* Agents/front-ends (models, CLIs/TUIs, wrappers)
  and
* The repo (git, workspaces, CI expectations, release discipline).

Decapod is not “the agent.” It is the runtime where agent output becomes enforceable.

### 3.2 Control-Plane Contract (Target, must be enforced)

A deterministic corridor for every session:

* validate environmental/methodology invariants
* ingest docs/spec/memory context
* acquire session + bind identity
* perform governed ops (claims/workspaces/state mutations)
* run proofs and publish outcomes

This contract is real only to the extent it is enforced by gates/mandates and audited artifacts.

### 3.3 State Model and Event-Sourced Determinism (Current + Target)

Current posture (intended):

* SQLite as materialized state
* JSONL events as append-only history enabling rebuild/replay

Target guarantees:

* every state mutation has a corresponding serialized event
* rebuild/replay reproduces materialized state deterministically
* divergence between DB and log is a correctness failure (detectable, testable, recoverable)

### 3.4 Proof Surfaces (“Done”) (Current)

* validation gates establish invariant compliance
* proofs define required checks for a claim/category
* completion is proof success, not narrative confidence

### 3.5 Multi-Agent Coordination (Current)

* isolated workspaces (e.g., worktrees)
* claim modes (exclusive/shared)
* presence/heartbeat + eviction rules
* reconciliation/handoff to prevent semantic collisions

### 3.6 Hardened Enforcement (REPORTED; must be VERIFIED with citations)

Reported (Gemini v3):

1. Non-bypassable mandates: constitutional mandates are resolved/evaluated during RPC calls; violations produce mandate_violation errors that block progression.
2. Session & identity binding: identity bound to per-agent ephemeral password (DECAPOD_SESSION_PASSWORD) and unique tokens; spoofing closed.
3. Constitutional awareness: high-authority mutating ops are gated by “Awareness” records, requiring validated environment + constitution ingest before state mutation.
4. Deterministic relevance injection: context.resolve performs relevance mapping and injects governing constitutional fragments into the agent’s context capsule based on operation + touched paths.

Status: REPORTED until citations land. If any item is inaccurate, it becomes drift.

### 3.7 Broker, Trace, Knowledge Lifecycle (REPORTED; must be VERIFIED with citations)

Reported (Gemini v2):

* Broker maturity: scoped read-cache, per-database locking, correlation_id/causation_id metadata for intent chains.
* Trace: auto-redacts sensitive fields before persistence.
* Risk zones: seeded with default required trust levels.
* Lineage enforcement: validation hard gate requiring intent-tagged events to map to verifiable memory nodes.
* Knowledge lifecycle: merge/supersede policies, TTL policies (ephemeral/decay), temporal search with as_of cutoffs.

Status: REPORTED until citations land. If any item is inaccurate, it becomes drift.

## 4) Drift Ledger (Immediate Priority)

This is the canonical truth table: “docs claim X; runtime does Y.”

Rules:

* Every row must include: doc claim → runtime behavior → severity → enforcing mechanism → failing test → source citations.
* A drift item is CLOSED only when:

  1. a failing test/proof demonstrates the mismatch,
  2. a fix makes it pass,
  3. docs are updated/classified accordingly.

Severity:

* CRITICAL: breaks determinism/auditability or allows silent bypass of kernel contract
* HIGH: security/correctness issue with plausible exploitation or misgovernance
* MEDIUM: undermines trust/clarity with visible symptoms
* LOW: ergonomics/documentation mismatch

### Drift Ledger Table (Canonical)

| ID    | Doc Claim (link)                                                     | Runtime Behavior (observed)                                                  | Severity | Enforcing Gate / Middleware                                | Failing Test / Proof (required)                         | Owner | Status                                                                                                                                                                                                                                                        |
| ----- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------- | -------- | ---------------------------------------------------------- | ------------------------------------------------------- | ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D-001 | Event-sourced rebuild is complete and deterministic                  | Missing rebuild handlers for emitted events causes non-reconstructible state | CRITICAL | Rebuild completeness gate + replay divergence detector     | test: replay fails before fix, passes after             |       | VERIFIED CLOSED (fixed in `src/core/todo.rs:3706` and `src/plugins/federation.rs:1473`)                                                                                                                                       |
| D-002 | Log/DB remain consistent under crash                                 | Crash between DB write and JSONL append can diverge                          | CRITICAL | Atomic log-and-commit semantics + recovery scan            | crash-injection test (kill between steps)               |       | VERIFIED CLOSED (Two-phase 'pending'->'success' log in `src/core/broker.rs:104`, recovery scan in `src/core/broker.rs:155`, test in `tests/crash_consistency.rs`)                                                               |
| D-003 | DEPRECATED ops are discouraged/blocked                               | Docs mark DEPRECATED; runtime dispatch allows silently                       | HIGH     | Runtime deprecation middleware                             | integration test calling deprecated op must warn/fail   |       | OPEN                                                                                                                                                                                                                          |
| D-004 | Validation/mandates are authoritative                                | Some gates warn / can be bypassed → contract not enforceable                 | HIGH     | Non-bypassable mandates OR audited override artifact       | test: bypass attempt fails or emits override event      |       | VERIFIED CLOSED (RPC dispatcher blocks on mandate violation in `src/lib.rs:2322`, evaluation in `src/core/validate.rs:1942`)                                                                                                  |
| D-005 | Agent identity is binding and attributable                           | Identity spoofing vector (e.g., env var identity)                            | HIGH     | Identity binding via receipt/session tokens/password       | test: spoof attempt rejected                            |       | VERIFIED CLOSED (Session bound to `agent_id` + hashed `DECAPOD_SESSION_PASSWORD` in `src/lib.rs:1370`)                                                                                                                        |
| D-006 | Secrets protected at rest                                            | Sensitive fields persisted plainly                                           | HIGH     | Redaction + at-rest handling that preserves audit metadata | test: tokens/passwords redacted; audit intact           |       | VERIFIED CLOSED (Auto-redaction in `src/core/trace.rs:21`)                                                                                                                                                                    |
| D-007 | High-authority mutations require constitutional awareness            | Mutations allowed without verified mandate compliance / constitution ingest  | HIGH     | “Awareness” middleware + mandate compliance check          | test: mutate without awareness fails                    |       | VERIFIED CLOSED (RPC enforcement in `src/lib.rs:1580`, record tracking in `src/lib.rs:1471`)                                                                                                                                  |
| D-008 | Context includes governing constraints relevant to current operation | Agent context lacks deterministically injected constitutional fragments      | MEDIUM   | context.resolve relevance mapping injection                | test: touched paths produce expected injected fragments |       | VERIFIED CLOSED (Deterministic relevance mapping in `src/core/docs.rs:31`, injection in `src/lib.rs:2485`)                                                                                                                    |

Process note:

* “REPORTED FIXED” is not CLOSED. It is a claim awaiting verification.
* Any REPORTED FIXED item that fails verification becomes OPEN with evidence.

## 5) Kernel Coherence Metric (KCR)

Kernel Coherence Ratio (KCR) =
(# enforcement claims backed by failing gates/tests)
/
(total enforcement claims asserted in docs/registry)

Interpretation:

* KCR ≥ 1.0: healthy (enforcement claims are mechanically real)
* KCR < 1.0: methodology debt is accruing (drift has begun)
* KCR trending down: constitution outpaces runtime (existential risk)

Operational rule:

* Any PR that adds an ENFORCED claim must add/extend a gate/test in the same PR (or it must be classified as PROCEDURAL/SPEC).

## 6) Kernel Hardening Order (Sequenced Bets)

### Bet 1: Crash Consistency + Replay Integrity

Goal: divergence between DB and log is impossible or automatically detected and recoverable.

Acceptance:

* crash-injection tests pass
* replay verification guarantees deterministic reconstruction
* D-002 CLOSED with evidence

### Bet 2: Session + Identity Binding (without opaque state)

Goal: identity is unspoofable and attributable while preserving auditability.

Acceptance:

* spoofing tests fail reliably
* receipts/events link identity to intent chains
* D-005 CLOSED with evidence

### Bet 3: Non-Bypassable Mandates (or Overrides Audited)

Goal: “validate/mandates decide done” is enforceable, not aspirational.

Acceptance:

* bypass attempts fail or produce auditable override artifacts
* D-004 CLOSED with evidence
* D-007 CLOSED with evidence

## 7) Opportunities for Enhancement (Kernel-Aligned)

1. Drift-to-Test Pipeline (Constitution Linter)
   Extract enforcement claims from docs/registry and require a gate/test mapping.

2. validate.rs → Registry of Predicates
   Turn monolithic validation into named gates with dependencies, evidence, and scheduling metadata.

3. Proof Plans Beyond Single Gates
   Structured proof plans: dependencies, retries, failure attribution, reproducible transcripts.

4. Replay Health Report
   Deterministic report: divergence, missing handlers, deprecated ops usage, overrides, KCR trend.

5. Runtime Deprecation Middleware
   Make DEPRECATED a runtime concept (warn/fail + telemetry/health impact).

## 8) Efficiency Improvements (Measurable)

1. Incremental validation (O(changes))
2. Parallel validation (safe read-only subsets)
3. Centralized filesystem traversal index
4. Broker hot-read optimization (scoped caching, without correctness loss)
5. Startup/binary slimming (only if it doesn’t harm determinism/auditability)

## 9) Missing Pieces (Blocking the Thesis)

1. Verified drift closure evidence
   This doc cannot claim “Hardened Enforcement” until at least one drift row is VERIFIED CLOSED with citations and failing→passing proof.

2. Crash consistency protocol + tests
   Even if mandates/identity are hardened, a divergent state model breaks the kernel.

3. Deprecation enforcement at runtime
   Docs-only deprecation is drift waiting to happen.

4. Cross-process discipline
   Either enforce single-writer hard, or implement IPC semantics explicitly.

5. Governance observability artifacts
   Humans must be able to see the corridor: what happened, why it passed, what failed.

## 10) Build Now (Killer Demos)

1. Governance Flight Recorder (killer app)
   Render event logs into a human-auditable timeline: intent → awareness → mandate checks → claim → workspace → edits → proofs → publish.

2. Mandate Violation Visualizer
   A UI that shows which mandate failed, with linked evidence and remediation steps.

3. Validation Trace Explorer
   Which gates ran, why, duration, evidence, cache hits.

4. Drift Ledger Generator
   Auto-generate ledger skeleton; fail CI if enforcement claims lack gate/test mapping.

5. Multi-Agent Handoff Simulator
   Claims, eviction, reconciliation, publish — produce reproducible governance transcripts.

## 11) Execution Plan (7 Days, with Day 0 discipline)

Day 0 (mandatory): Demonstrate failure, then fix (scientific method)

* Write at least one failing test/proof for a CRITICAL drift item (prefer D-002 if still open; otherwise D-001).
* Only then implement the fix and make it pass.
* Close one row as VERIFIED CLOSED with citations + failing→passing proof link.

Day 1: Verify REPORTED claims (prevent drift-by-report)

* For every “REPORTED” section (3.6/3.7) and every “REPORTED FIXED” drift row:

  * add citations,
  * add an explicit test/proof link,
  * downgrade any inaccurate report into an OPEN drift row with evidence.

Day 2: Crash consistency MVP

* Commit-marker / atomic protocol
* crash-injection harness
* move D-002 to VERIFIED CLOSED

Day 3: Gate registry skeleton + trace

* name/structure gates
* timings + evidence emission
* scheduling hooks

Day 4: Incremental validate prototype

* dependency metadata for a subset of gates
* correctness checks

Day 5: Replay health report + KCR computation

* divergence scan
* deprecated op scan
* override scan
* KCR trend output

Day 6: Flight recorder demo

* timeline over real logs
* exportable transcript artifact

Day 7: Multi-agent simulator + one chaos test

* eviction/handoff scenario
* race/crash scenario
* governance transcript artifacts

## 12) Guardrail

Primary (updated for hardened enforcement posture):
The kernel shall treat any state mutation without verifiable mandate compliance and attributable intent as a fatal governance failure.

Compatibility (still true):
Never increase agent capability without simultaneously increasing the system’s ability to prove the work was done correctly, inside the boundaries, with deterministic evidence.

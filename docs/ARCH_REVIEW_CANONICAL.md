## 0) Decision Boundary

Decapod is an enforcement kernel, not a utility shell. If capability grows without equally enforceable proof/boundary controls, that is kernel regression.

## 1) Review Scope and Audit Trail

- Reviewed commit / tag: `15c2483a0f0170605b06842760e0d53e84c3a2c1`
- Reviewer(s): `codex`
- Review date: `2026-02-19` (UTC)
- Coverage manifest path: `docs/ARCH_REVIEW_COVERAGE_MANIFEST.md`
- Not reviewed (explicit): all files not listed in `docs/ARCH_REVIEW_COVERAGE_MANIFEST.md`

Citation policy used here:
- Runtime behavior claims cite code/tests as `path:line-line`.
- Intent/spec claims cite canonical docs.
- Mismatches are ledgered below.

## 2) Kernel Thesis and Thin Waist

Observed
- RPC responses are structured with receipt/context/allowed-next/blockers (`src/core/rpc.rs:43-97`, `src/core/rpc.rs:119-153`).
- RPC ops are mandate-evaluated before execution and return `mandate_violation` with blockers when blocked (`src/lib.rs:2374-2400`).
- Stateful mutation paths are brokered with audited events (`src/core/broker.rs:48-52`, `src/core/broker.rs:99-147`, `src/core/broker.rs:191-201`).
- Completion gates are centralized in `validate` and replay/consistency checks are explicit (`src/core/validate.rs:301-355`, `src/core/validate.rs:1183-1305`).

Inferred
- The thin waist is implemented as a combination of RPC mandate evaluation + brokered mutations + validate gates; correctness depends on those three surfaces remaining aligned.

## 3) Hardened Enforcement Status (VERIFIED)

### 3.6 Control-plane hardening

1. VERIFIED: Non-bypassable mandate evaluation on RPC path.
- Evidence: mandate resolution/evaluation and explicit `mandate_violation` response (`src/lib.rs:2374-2400`).
- Evidence in tests: mandate violation expected and asserted (`tests/todo_enforcement.rs:165-193`).

2. VERIFIED: Session identity binds `DECAPOD_AGENT_ID` to session token + password hash validation.
- Evidence: per-agent session records include token/password hash (`src/lib.rs:1035-1041`, `src/lib.rs:1137-1149`, `src/lib.rs:1151-1171`).
- Evidence: password validation in command auth path (`src/lib.rs:1351-1397`).
- Evidence in tests: missing/wrong password fails; correct password passes (`tests/entrypoint_correctness.rs:130-178`).

3. VERIFIED: High-authority mutating RPC ops require constitutional awareness checkpoints.
- Evidence: awareness gate for `workspace.publish`, `store.upsert`, scaffold mutators (`src/lib.rs:1542-1550`).
- Evidence: required prior checkpoints (`validate`, `docs ingest`, `context.resolve`, active-session freshness) (`src/lib.rs:1552-1599`).

4. VERIFIED: `context.resolve` performs deterministic relevance mapping and bounded fragment injection.
- Evidence: mapping by op/path/tag bindings and deterministic ordering/dedup/truncation (`src/lib.rs:2579-2632`).
- Evidence in tests: deterministic repeated result assertion (`tests/agent_rpc_suite.rs:153-172`).

### 3.7 Broker/trace/knowledge lifecycle

1. VERIFIED: Broker has per-database locking, scoped read cache, and intent/correlation metadata.
- Evidence: per-db lock map and serialization (`src/core/broker.rs:127-133`, `src/core/broker.rs:262-276`).
- Evidence: cache scoped by `db::scope::key` (`src/core/broker.rs:204-259`, `src/core/broker.rs:283-286`).
- Evidence: correlation/causation/idempotency/session envelope (`src/core/broker.rs:72-89`, `src/core/broker.rs:159-184`).

2. VERIFIED: Trace persistence redacts token/secret/password/api_key/authorization keys.
- Evidence: recursive redaction and redacted append path (`src/core/trace.rs:17-39`, `src/core/trace.rs:55-66`).
- Evidence in tests: secret present in request, absent in exported trace, replaced with `[REDACTED]` (`tests/agent_rpc_suite.rs:223-250`).

3. VERIFIED: Risk zones are seeded with default trust requirements and enforced in claim/handoff policy checks.
- Evidence: default zone seeding with required trust/approval (`src/core/todo.rs:625-670`).
- Evidence: policy lookup + trust comparison (`src/core/todo.rs:1678-1704`).
- Evidence in tests: shared claim denied before trust grant and accepted after (`tests/plugins/todo.rs:297-329`).

4. VERIFIED: Lineage hard gate validates intent-tagged TODO events map to federation commitment/decision lineage nodes.
- Evidence: gate logic and failure conditions (`src/core/validate.rs:1183-1305`).

5. VERIFIED: Knowledge lifecycle primitives exist (merge/supersede policies, TTL, `as_of` temporal filtering).
- Evidence: conflict policy parse, ttl/status validation, `as_of` filtering, TTL decay/prune (`src/plugins/knowledge.rs:80-123`, `src/plugins/knowledge.rs:299-321`, `src/plugins/knowledge.rs:412-465`).

## 4) Drift Ledger (CI-checkable)

| ID | Doc Claim (with citation) | Runtime Behavior (with citation) | Severity | Enforcing Gate / Middleware | Failing Test / Proof (required) | Owner | Status |
|---|---|---|---|---|---|---|---|
| D-001 | Deterministic rebuild parity for federation state (`src/plugins/federation.rs:1465-1529`, `src/plugins/federation.rs:1977-1998`) | Rebuild parity gate exists and passes in tests; drift in derived artifacts is detected (`tests/plugins/federation.rs:524-574`, `tests/plugins/federation.rs:636-684`) | CRITICAL | `federation.rebuild_determinism` + derived freshness gates | `tests/plugins/federation.rs:636-684` (drift/fail) -> `tests/plugins/federation.rs:524-574` (pass) | | VERIFIED CLOSED |
| D-002 | Log/DB consistency should survive interruptions (`src/core/validate.rs:9-12`) | Validation currently downgrades federation drift signals to warnings due two-phase DB+JSONL behavior (`src/core/validate.rs:1607-1611`); writes still occur in separate DB and JSONL actions (`src/plugins/federation.rs:609-635`) | CRITICAL | federation consistency + replay gates | Add crash-injection proof for DB-write-before-JSONL-append window | | VERIFIED OPEN |
| D-003 | Deprecated surfaces should be discouraged with replacements (`constitution/core/DEPRECATION.md:17-27`, `constitution/core/PLUGINS.md:48-49`) | Deprecation metadata is published (`src/lib.rs:1910-1943`), but no dedicated runtime middleware emits per-invocation deprecation warnings/blocks | HIGH | metadata-only today | Add integration proof: deprecated surface invocation emits structured warning/denial | | VERIFIED OPEN |
| D-004 | Validation/mandates are authoritative (`core/DECAPOD.md#Validation (must pass before claiming done)`) | Mandates hard-block RPC (`src/lib.rs:2374-2400`), but some validate checks are explicitly skippable via env and some federation gates are warnings (`src/lib.rs:936-938`, `src/core/validate.rs:1607-1611`, `src/core/validate.rs:1650-1655`) | HIGH | mandate eval + validate gates | Add proof asserting bypass attempts produce audited override artifacts or are denied | | VERIFIED OPEN |
| D-005 | Agent identity should be attributable and bound (`src/lib.rs:1035-1041`, `src/lib.rs:1351-1397`) | Password+token checks are enforced for non-`unknown` agent auth path (`src/lib.rs:1380-1395`), but `DECAPOD_AGENT_ID` remains env-selected identity root (`src/lib.rs:1069-1075`) | HIGH | session acquire + ensure_session_valid | Extend spoof-resistance tests around `unknown`/agent-id transitions | | VERIFIED OPEN |
| D-006 | Secrets protected in persisted traces (`src/core/trace.rs:17-39`) | Redaction is implemented and test-verified (`src/core/trace.rs:55-66`, `tests/agent_rpc_suite.rs:223-250`) | HIGH | trace redact-on-write | Add explicit fail->pass redaction regression proof (current artifact is pass-only) | | VERIFIED OPEN |
| D-007 | High-authority mutations require constitutional awareness (`src/lib.rs:1542-1599`) | Gate is implemented for selected high-authority mutators (`src/lib.rs:1542-1550`) with explicit checkpoint failures (`src/lib.rs:1562-1599`) | HIGH | awareness middleware | Add negative tests per mutator (`workspace.publish`, `store.upsert`, `scaffold.*`) | | VERIFIED OPEN |
| D-008 | Context includes operation-relevant governing fragments (`src/lib.rs:2589-2632`) | Deterministic op/path/tag mapping, sorted+deduped bounded fragments; deterministic test exists (`src/lib.rs:2629-2632`, `tests/agent_rpc_suite.rs:153-172`) | MEDIUM | `context.resolve` relevance mapping | Add explicit fail->pass relevance-mismatch regression proof (current artifact is pass-only) | | VERIFIED OPEN |

## 5) Kernel Coherence Ratio (KCR)

Definition
- KCR = enforced claims with explicit gate/proof mapping / total enforced claims in claim registry.

Measured source
- Registry: `constitution/interfaces/CLAIMS.md:30-69`.

Current measurement
- `enforced_claims=13`, `enforced_with_gate=13`, `kcr=1.0` (tracked in `docs/metrics/KCR_TREND.jsonl`).

Trend artifact
- Machine-readable trend line lives in `docs/metrics/KCR_TREND.jsonl`.
- CI gate verifies trend entry matches current computed KCR and rejects malformed regressions.

## 6) End-to-End Workflow (intent -> change -> proof -> promotion)

Observed path
1. Intent/identity bootstrap:
- Acquire session (`src/lib.rs:1407-1443`), then `agent.init` / `context.resolve` (`src/lib.rs:2403-2491`, `src/lib.rs:2579-2653`).
2. Governed change in scoped workspace:
- Worktree enforcement and todo-scoped workspace requirement (`src/core/workspace.rs:170-215`, `tests/todo_enforcement.rs:284-338`).
3. Mutation through control-plane surfaces:
- RPC mandate + awareness checks (`src/lib.rs:2362-2368`, `src/lib.rs:2374-2400`).
- Brokered writes with audit events (`src/core/broker.rs:99-147`, `src/core/broker.rs:191-201`).
4. Proof and verification:
- `validate` core gates and deterministic replay checks (`src/core/validate.rs:301-355`, `src/core/validate.rs:1183-1305`).
- Replay verification workflow (`src/plugins/verify.rs:17-27`, `src/plugins/verify.rs:182-212`, `tests/verify_mvp.rs:81-122`).
5. Promotion semantics:
- completion/promotion posture is explicitly proof-gated in outputs (`src/lib.rs:2483-2487`) and verify artifacts (`tests/verify_mvp.rs:74-122`).

## 7) CI Rules Added for This Artifact

- Fail CI if any `enforced` claim in `constitution/interfaces/CLAIMS.md` lacks an explicit gate/proof mapping.
- Fail CI if any Drift Ledger row marked `VERIFIED CLOSED` lacks:
  - code/doc citations (`path:line-line`) in claim/runtime columns,
  - explicit failing->passing proof reference in the proof column.
- Fail CI if KCR trend artifact is malformed or does not match current registry-derived KCR.

## 8) Guardrail

No change may reduce auditability, determinism, or proof-boundary enforcement, even if it improves throughput.

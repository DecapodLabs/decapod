# Operator Verification Guide

This guide defines how to verify Decapod behavior using reproducible commands, tests, and artifacts.

## Supported Kernel Invariants

| Invariant | Enforcement Surface | Verify |
|---|---|---|
| Validate must terminate under lock contention with a typed failure marker | `tests/validate_termination.rs` | `cargo test --all-features --test validate_termination -- --test-threads=1` |
| RPC envelope compatibility is pinned to golden vectors | `tests/rpc_golden_vectors.rs`, `tests/golden/rpc/v1/*` | `cargo test --all-features --test rpc_golden_vectors -- --test-threads=1` |
| Enforced claims must map to gates and KCR trend counts | `tests/canonical_evidence_gate.rs` | `cargo test --all-features --test canonical_evidence_gate -- --test-threads=1` |
| JIT context capsules are deterministic and policy-bound | `tests/context_capsule_cli.rs`, `tests/context_capsule_rpc.rs`, `tests/context_capsule_schema.rs`, `tests/validate_optional_artifact_gates.rs` | `cargo test --test context_capsule_cli --test context_capsule_rpc --test context_capsule_schema --test validate_optional_artifact_gates` |
| Promotion requires provenance manifests | `src/core/workspace.rs`, `decapod workspace publish` gate | `decapod workspace publish` (fails if manifests missing) |

## Proof Surfaces

- RPC contract anchors: `tests/golden/rpc/v1/agent_init.request.json`, `tests/golden/rpc/v1/agent_init.response.json`
- Validate contention semantics: `tests/validate_termination.rs`
- Canonical evidence mapping: `tests/canonical_evidence_gate.rs`
- JIT capsule policy contract: `.decapod/generated/policy/context_capsule_policy.json` (override path: `.decapod/policy/context_capsule_policy.json`)
- Promotion provenance gate: `.decapod/generated/artifacts/provenance/artifact_manifest.json`, `.decapod/generated/artifacts/provenance/proof_manifest.json`
- Policy lineage anchor: each provenance manifest includes `policy_lineage.{policy_hash,policy_revision,risk_tier,capsule_path,capsule_hash}`
- `decapod release check` deterministically normalizes `policy_lineage` (shared across all provenance manifests) before enforcing release gates.

## Validate Diagnostics (Safe Mode)

Enable diagnostics only when needed:

```bash
DECAPOD_DIAGNOSTICS=1 decapod validate
```

If `VALIDATE_TIMEOUT_OR_LOCK` occurs, Decapod emits a sanitized diagnostic artifact:

- `.decapod/generated/artifacts/diagnostics/validate/<run_id>.json`

Expected fields:

- `reason_code`
- `elapsed_ms`
- `timeout_secs`
- `lock_age_ms`
- `stale_lock_recovery_triggered`
- `artifact_hash`

Diagnostics MUST NOT include host/user/environment identifiers or absolute paths.

## Repro Commands

```bash
cargo test --all-features --test validate_termination -- --test-threads=1
cargo test --all-features --test rpc_golden_vectors -- --test-threads=1
cargo test --all-features --test canonical_evidence_gate -- --test-threads=1
decapod release check
decapod release lineage-sync
```

## 10-Minute JIT Capsule Verification

```bash
decapod init --force
decapod session acquire
decapod govern capsule query --topic "policy bound context" --scope interfaces --risk-tier low --task-id R_demo --write
decapod govern capsule query --topic "denied scope" --scope plugins --risk-tier low
decapod validate
```

Expected:
- success for interfaces/low query and artifact write at `.decapod/generated/context/R_demo.json`
- fail-closed denial on plugins/low query (`CAPSULE_SCOPE_DENIED`)
- policy contract present at `.decapod/generated/policy/context_capsule_policy.json`

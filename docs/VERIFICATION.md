# Operator Verification Guide

This guide defines how to verify Decapod behavior using reproducible commands, tests, and artifacts.

## Supported Kernel Invariants

| Invariant | Enforcement Surface | Verify |
|---|---|---|
| Validate must terminate under lock contention with a typed failure marker | `tests/validate_termination.rs` | `cargo test --all-features --test validate_termination -- --test-threads=1` |
| RPC envelope compatibility is pinned to golden vectors | `tests/rpc_golden_vectors.rs`, `tests/golden/rpc/v1/*` | `cargo test --all-features --test rpc_golden_vectors -- --test-threads=1` |
| Enforced claims must map to gates and KCR trend counts | `tests/canonical_evidence_gate.rs` | `cargo test --all-features --test canonical_evidence_gate -- --test-threads=1` |
| Promotion requires provenance manifests | `src/core/workspace.rs`, `decapod workspace publish` gate | `decapod workspace publish` (fails if manifests missing) |

## Proof Surfaces

- RPC contract anchors: `tests/golden/rpc/v1/agent_init.request.json`, `tests/golden/rpc/v1/agent_init.response.json`
- Validate contention semantics: `tests/validate_termination.rs`
- Canonical evidence mapping: `tests/canonical_evidence_gate.rs`
- Promotion provenance gate: `artifacts/provenance/artifact_manifest.json`, `artifacts/provenance/proof_manifest.json`

## Validate Diagnostics (Safe Mode)

Enable diagnostics only when needed:

```bash
DECAPOD_DIAGNOSTICS=1 decapod validate
```

If `VALIDATE_TIMEOUT_OR_LOCK` occurs, Decapod emits a sanitized diagnostic artifact:

- `artifacts/diagnostics/validate/<run_id>.json`

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
```

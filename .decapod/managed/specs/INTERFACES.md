# Interfaces

## Contract Principles
- Prefer explicit schemas over implicit behavior.
- Every mutating interface defines idempotency semantics.
- Every failure path maps to a typed, documented error code.

## Generated Contract Depth
Generated interface specs should include:
- API/CLI contracts with request/response schemas.
- Read/write ownership for each storage path.
- Idempotency and retry behavior for mutations.
- Typed failure classes and recovery instructions.

## API / RPC Contracts
| Interface | Method | Request Schema | Response Schema | Errors | Idempotency |
|---|---|---|---|---|---|
| `TODO` | `TODO` | `TODO` | `TODO` | `TODO` | `TODO` |

## Storage Boundary Contracts
| Trajectory writer | Trajectory recorder | Governance trajectory file | Writes one canonical JSON object with a recomputed hash through atomic replacement |
| Migration report | Local command path | Migration ledger and datastore | Returns version transition and applied migration evidence after verification succeeds |

| Interface | Producer | Consumer | Contract |
|---|---|---|---|
| `DecapodError::storage_failure_kind` | Current storage adapter | Retry, validation, and governance policy | Backend-neutral failure class; only bounded retryable contention/storage-I/O classes may be retried |
| `migration::plan_pending_migrations` | Decapod migration catalog and applied ledger | Migration executor | Deterministic version/sequence filtering with no datastore access |
| `DbBroker::execute_write_sync` | Broker caller | Storage write path | Returns affected rows; stable IDs are supplied by the caller and no ambient generated-ID lookup is allowed |

## Event Consumers
| Consumer | Event | Ordering Requirement | Retry Policy | DLQ Policy |
|---|---|---|---|---|
| `TODO` | `TODO` | `TODO` | `TODO` | `TODO` |

## Outbound Dependencies
| Dependency | Purpose | SLA | Timeout | Circuit-Breaker |
|---|---|---|---|---|
| `TODO` | `TODO` | `TODO` | `TODO` | `TODO` |

## Inbound Contracts
- API / RPC entrypoints:
- CLI surfaces:
- Event/webhook consumers:
- Repository-detected surfaces: cargo, rust

## Data Ownership
- Source-of-truth tables/collections:
- Cross-boundary read models:
- Consistency expectations:

## Epistemic Integrity Contracts
| Interface | Input | Output | Failure semantics |
|---|---|---|---|
| `OVERRIDE.md` resolution | Markdown or another documentation style inside each generated subsection's four-backtick source block | Extracted body without wrapper plus derived authority evidence | Unclosed wrappers, duplicate exact IDs, or non-empty unknown Decapod-namespaced IDs reject the complete overlay |
| `context.resolve` / capsule query | Repository root and context request | Applied directive ID, source, source/body hashes, bytes, precedence | No partial authority result on structural ambiguity |
| `core::events::{query,latest,exists,actors}` | Semantic stream and bounded filter/limit | Canonical SQLite event observations | Unknown stream or malformed canonical payload is typed failure |
| startup reconciliation | Unproven legacy JSONL or post-consolidation legacy SQLite stores | Idempotently imported canonical rows plus durable retirement receipts | Fresh malformed input or same-ID/different-event conflict stops migration; previously proven inputs are never reinterpreted as live authority |

## Error Taxonomy Example (service_or_library)
```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("upstream timeout")]
    UpstreamTimeout,
    #[error("conflict: {0}")]
    Conflict(String),
}
```

## Failure Semantics
| Failure Class | Retry/Backoff | Client Contract | Observability |
|---|---|---|---|
| Validation | No retry | 4xx typed error | warn log + metric |
| Dependency timeout | Exponential backoff | 503 with retryable code | error log + alert |
| Conflict | Conditional retry | 409 with conflict detail | info log + metric |
| Storage contention | Bounded retry with explicit budget | Actionable validation/governance outcome | warning with retry classification |

## Timeout Budget
| Hop | Budget (ms) | Notes |
|---|---|---|
| Client -> Edge/API | 500 | Includes auth + routing |
| API -> Domain | 300 | Includes validation |
| Domain -> Store/Dependency | 200 | Includes retry overhead |

## Interface Versioning

## Current PR Contract Details
### Trajectory Cookie
- Input: a valid run identifier and trajectory fields.
- Output: exactly one schema-valid JSON object at the canonical cookie path.
- Update semantics: same-run initialization is rejected when the existing
  object is valid; a different or malformed legacy cookie is replaced.
- Historical semantics: Git commits preserve prior cookies; the file is not an
  append-only JSONL stream.

### Migration Notice
- Trigger: every local command performs the version/ledger check; a previously
  recorded release differing from the installed release, or newly applied
  migrations, requires notice.
- Output: a warning naming previous/current release, applied IDs, and the
  applied ledger and catalog inspection paths.
- Failure: migration or verification errors remain typed and block command
  continuation; a notice is never used as proof that migration succeeded.

- Version strategy (`v1`, date-based, semver):
- Backward-compatibility guarantees:
- Deprecation window and removal policy:

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `b29bb2bc5fd7f123a4f0e48ca940326bf5ad322ce38191edc5868db014d8abd1`
- Significant implementation surfaces: `.github/` (10 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (101 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

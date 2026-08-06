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
- Version strategy (`v1`, date-based, semver):
- Backward-compatibility guarantees:
- Deprecation window and removal policy:

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `90c2209cbca9ecf63ccdca4f7fccb73aa35d8f7dc6f9145a6f9106f14a611ffe`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (101 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

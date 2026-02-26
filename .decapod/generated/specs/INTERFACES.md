# Interfaces

## Contract Principles
- Each interface must have explicit schema, idempotency, and typed error behavior.
- Mutating operations return deterministic receipts when state changes.
- Backward-compatible versioning is mandatory for stable surfaces.

## API / RPC Contracts
| Interface | Method | Request Schema | Response Schema | Errors | Idempotency |
|---|---|---|---|---|---|
| `rpc.agent.init` | `rpc --op agent.init` | op + session context | capabilities + session envelope | `SESSION_REQUIRED` | idempotent for same active session |
| `rpc.context.resolve` | `rpc --op context.resolve` | query/context scope | deterministic context capsule | validation errors | idempotent for same inputs |
| `rpc.context.scope` | `rpc --op context.scope` | scoped query params | ranked scoped context list | query validation errors | idempotent for same inputs |

## Event Consumers
| Consumer | Event | Ordering Requirement | Retry Policy | DLQ Policy |
|---|---|---|---|---|
| Todo ledger writer | `todo.add|claim|done` | per-task order | bounded retry on lock contention | append DLQ record on persistent failure |
| Knowledge promotions | `knowledge.promote` | global append order | retry with backoff | write rejected payload to diagnostics |

## Outbound Dependencies
| Dependency | Purpose | SLA | Timeout | Circuit-Breaker |
|---|---|---|---|---|
| Git worktree | workspace isolation | local op availability | 60s | fail-fast + typed error |
| SQLite | canonical task/session state | local DB availability | 30s validate bound | lock timeout + retry policy |
| Filesystem | artifact emission | local writeability | op-specific | fail-fast |

## Inbound Contracts
- CLI surfaces: `init`, `session`, `workspace`, `todo`, `validate`, `rpc`, `govern`, `data`.
- RPC surfaces: deterministic JSON envelope with typed failure codes.
- Event consumers: todo/workunit/provenance ledgers.

## Data Ownership
- Source-of-truth: repo store for repo-scoped tasks/specs/proof artifacts.
- User store: cross-repo memory and aptitude where explicitly requested.
- Consistency model: single-writer command boundaries with durable append logs.

## Error Taxonomy Example (Rust)
```rust
#[derive(Debug, thiserror::Error)]
pub enum InterlockError {
    #[error("workspace required")]
    WorkspaceRequired,
    #[error("verification required")]
    VerificationRequired,
    #[error("store boundary violation")]
    StoreBoundaryViolation,
    #[error("validation timeout or lock")]
    ValidateTimeoutOrLock,
}
```

## Failure Semantics
| Failure Class | Retry/Backoff | Client Contract | Observability |
|---|---|---|---|
| Input validation | no retry | typed non-zero response | warning log |
| Lock contention | bounded retry + jitter | `VALIDATE_TIMEOUT_OR_LOCK` on exhaustion | error log + metric |
| Policy interlock | no retry until condition fixed | explicit next action | policy event log |

## Timeout Budget
| Hop | Budget (ms) | Notes |
|---|---|---|
| CLI parse + preflight checks | 500 | local checks |
| Core operation + store transaction | 5000 | per-command budget |
| Validation full gate sweep | 30000 | hard bounded runtime |

## Interface Versioning
- Stable CLI verbs are additive-first.
- JSON response shape changes require migration notes and docs update.
- Removal/deprecation requires at least one release-cycle warning window.

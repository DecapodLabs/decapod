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
| `DbBroker::with_transaction` | Decapod domain mutation | Dactyl-backed local transaction facade | Commits state and its canonical event together; rolls both back on failure; no physical connection handle crosses the application boundary |
| `core::backend::BackendSelection` / `BackendRoute` | `.decapod/config.toml` backend plus Git origin identity | Local datastore or authenticated Dactyl session | `local` binds `.decapod/data/decapod.db`; `cloud` binds the GitHub owner/repository and accepts only an opaque remote URI supplied by the session boundary; provider names and URI construction are outside ordinary Decapod persistence |
| `core::backend::StorageContext` | `BackendSelection`, opaque route, and optional session bearer | Dactyl bridge or future physical driver | Version 1 distinguishes local and remote targets; local has no cloud scope or credential, remote requires an authenticated bearer, credentials are never serialized, and unsupported future versions fail closed before I/O; Propodus owns effective membership/repository authorization |
| `core::dactyl_db::Connection` | Decapod relational caller, SQL text, typed parameters | Dactyl v0.8.2 local connection | Provides the only application-facing connection facade; it seeds a new empty read-write filesystem target for Dactyl v0.8.2's pre-open validation, while Dactyl owns physical execution, access mode, normalized rows/results, typed errors, schema inspection, and host-runtime availability. No direct SQL-driver dependency or physical handle is exposed |
| `core::dactyl::DactylBridge` | Backend route, access mode, and optional opaque bearer | Dactyl v0.8.2 operation/context contract | Provides explicit reads, writes, atomic batches, portable schema inspection, access-mode enforcement, and Decapod-normalized errors; local existing files open directly and cloud construction fails closed without a bearer |

Read callers must not invoke schema DDL or migration repair through a read-only connection. Decapod initialization owns that write-side preparation, and the facade exposes the connection access mode so compatibility helpers can avoid prohibited writes before issuing a read.
| Decapod agent session | Backend selection and machine-local session directory | Canonical Dactyl store or authenticated cloud command path | Machine-local session records use a backend discriminator (`local_` or `cloud_`) and default to a four-hour lifetime, bounded to 30 minutes minimum and six hours maximum; cloud bearer credentials remain separate opaque machine-local material |
| `todo::update_status` | Decapod lifecycle caller | Dactyl-backed local transaction facade | Requires expected status and revision; returns `ok`, `not_found`, or `conflict` and emits an event only after the conditional update wins |
| `todo` lease/ownership mutations | Decapod coordination caller | Dactyl-backed local transaction facade | Claim, yield, handoff, owner, heartbeat, cleanup, and expertise state plus their events share the broker transaction boundary; cloud atomic-envelope proof remains downstream |
| `events::append_on_conn` | Canonical Decapod event writer | Dactyl-backed local transaction facade | Replaying the same event identity is idempotent; divergent content fails with `EVENT_ID_CONFLICT`; Dactyl executes the write while Decapod owns sequence and event policy |
| `workspace::ensure_isolated_workspace_for_projection_mutation` | Git toplevel of the invoking checkout | `refresh_specs_from_codebase`, validate self-heal, `specs.refresh` | Writes `.decapod/managed/specs/*` only when the Git worktree root is a non-protected path under `.decapod/workspaces/*`; protected `main`/`master` fails closed with `workspace_required` and does not touch root files (GitHub #1255) |
| `workspace::get_main_repo_root` / `host_repo_from_canonical_workspace` | Cwd inside `.decapod/workspaces/*` | Todo store, session, `find_governance_root` | Local-clone workspaces resolve the host checkout so `todo done` sees the claim that created them (GitHub #1259) |
| `verify::resolve_artifact_path_for_todo` | `--artifact` path plus optional todo id | `todo done --validated` | Resolves first against cwd, then the newest workspace directory matching the todo id |
| `workspace::resolve_publish_remote` | Workspace remotes, then parent filesystem remotes | `workspace publish` | Inherits a GitHub remote from the local-clone parent as `upstream`; never treats a path `origin` as publication |
| `validate::governance_paths_updated_vs_base` | `base...HEAD` plus working tree | `GOVERNANCE_PR_UPDATES` | Working-tree and just-written `validation.json` count; the gate does not require `DECAPOD_VALIDATE_SKIP_GIT_GATES` |

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
| `core::events::{query,latest,exists,actors}` | Semantic stream and bounded filter/limit | Dactyl-backed canonical event observations | Unknown stream, malformed canonical payload, unsupported physical operation, or unavailable host runtime is a typed failure; no catalog query or second store is used |
| startup reconciliation | Unproven legacy JSONL or a legacy local database source | Dactyl-backed canonical rows plus Decapod retirement receipts | Dactyl owns physical reads/writes; Decapod owns schema compatibility, row translation, backup, and idempotency. Fresh malformed input or same-ID/different-event conflict stops migration |

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

### Bootstrap Diagnostics
- `capabilities`, `system capabilities`, `constitution get`, and `system
  version` are bootstrap-safe and do not require a project session, datastore,
  or migration reconciliation before returning their output.
- `docs list`, `docs show`, and `docs ingest` are dispatched before the
  stateful command path so embedded agent guidance remains available when
  project state needs repair. `docs ingest` may update the existing
  project-local constitutional-awareness marker after successfully emitting
  the embedded documents.
- Stateful commands retain the migration, session, worktree, and validation
  gates; this exception is limited to discovery and diagnostics needed to
  recover those gates.

- Version strategy (`v1`, date-based, semver):
- Backward-compatibility guarantees:
- Deprecation window and removal policy:

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `11221e49b022a1186ebd1aa5c0fc44c5ecbb7a07f0897960c6c8b0370dd4d21c`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (104 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

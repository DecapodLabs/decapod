# Architecture

## Direction
cli

## What This Project Is
decapod is a service_or_library project built using Rust, rust.
cli

Architectural principles:
- **Simplicity**: Keep components focused and reusable.
- **Modularity**: Clearly defined interface boundaries and dependency separation.
- **Reliability**: Graceful failure handling and thorough verification.

## Current Facts
- Runtime/languages: Rust, rust
- Detected surfaces/framework hints: cargo, rust
- Product type: service_or_library

## Architecture Map
This project's architecture consists of the following key layers/directories:
- `src/`: Main source directory containing primary logic.
- `tests/`: Integration and unit test suite.

## Data Flows
- Inbound request/command parses and validates at the entrypoint.
- Core runtime handles business logic and initiates queries or state changes.
- Storage adapter reads or writes data to the underlying persistence layers.

## Strongest Existing Primitives
- `core::error::StorageFailureKind` is the application-owned classification seam for contention, storage I/O, constraint, query, value, capability, and unknown failures. The current SQLite mapping is a compatibility adapter; retry and governance callers do not select behavior by backend name.
- `core::migration` separates version-gated pending-plan selection and applied-ledger recording from migration execution. Backup, restore, legacy import, and schema policy remain Decapod responsibilities while the future dactyl boundary supplies execution.
- `DbBroker::execute_write_sync` returns affected rows. Decapod callers own stable IDs rather than reading ambient connection-generated row IDs.
- `DbBroker::with_transaction` is the Decapod-owned atomic mutation seam. Material TODO lifecycle, ownership, lease, agent-session, and federation state changes append their canonical event within the same transaction; `events::append_on_conn` composes with caller-owned transactions and uses idempotent event identity plus unique stream sequencing.
- `core::backend::BackendSelection` is the provider-neutral route seam: it reads `repo.backend`, uses the Git `origin` remote for cloud repository scope, binds local to `.decapod/data/decapod.db`, and passes a session-supplied cloud URI through as opaque data for Dactyl. Ordinary Decapod persistence does not construct a Propodus, Vercel, or Neon path.
- `core::dactyl::DactylBridge` is the physical-driver seam for backend-neutral conformance against `dactyl-db` v0.6.2: it exposes Dactyl's explicit result, atomic-batch, access-mode, and typed-error contract without leaking a backend handle. It forwards the Decapod-owned versioned context envelope unchanged to remote Dactyl connections, can reopen an explicitly named Dactyl snapshot, and refuses to treat the canonical SQLite path as that snapshot, preventing a second local authority; cloud route construction remains bearer-gated and opaque.
- `core::backend::StorageContext` is the versioned Decapod-owned handoff between logical selection and physical execution. Local contexts contain only the canonical repository path; remote contexts contain the opaque route, logical repository scope, and an in-memory bearer that is excluded from serialization. Organization membership and repository authorization remain Propodus concerns.
- Session custody is machine-local for both backend choices: the Decapod agent-session record is stored under the machine config directory, uses `local_`/`cloud_` token prefixes to detect backend changes, and defaults to a four-hour TTL bounded between 30 minutes and six hours. Cloud access and refresh tokens are stored separately under the machine data directory and refreshed before the remaining lifetime falls below 30 minutes.

## Local-Clone Publication and Store Binding (#1259)
- A container local-clone under `.decapod/workspaces/*` has its own `.git`
  whose `origin` is the parent filesystem path. `get_main_repo_root` walks
  out of that path so todo/session store operations hit the host
  `.decapod/data`, not an empty clone copy.
- `prepare_workspace_clone` fetches `origin/<base>` on the parent, checks
  the feature branch out at that OID, and copies the parent's network remotes
  onto the clone as `upstream`.
- `resolve_publish_remote` first uses a network remote already on the
  workspace; if none exist it walks local remotes and the canonical host
  checkout, adds `upstream`, and pushes there.

## Isolated Workspace Ownership for Spec Projections (#1255)
- Mutation of `.decapod/managed/specs/*` is a custody operation, not a
  convenience rewrite of the host checkout. The control-plane identity is
  the Git worktree root, never `DECAPOD_WORKSPACE` or the current branch
  name alone.
- `workspace::ensure_isolated_workspace_for_projection_mutation` is the
  choke point in front of `refresh_specs_from_codebase`. Protected
  `main`/`master` roots fail closed with `workspace_required`. Isolated
  clones and git worktrees under `.decapod/workspaces/` are the only
  permitted writers.
- `workspace status` must not pass `refresh_specs=true`. Status reports
  drift; the claimed workspace repairs it. Publication then requires the
  managed spec files to appear in the same `base...HEAD` diff as the
  code that caused the projection change.

## Publication Bundle Currency Architecture (#1232)
- `core::validate::validate_publication_bundle_currency` proves presence and
  version-stability at HEAD for the publication bundle. It replaced the
  per-commit `diff-tree` participation model (`PER_COMMIT_PUBLICATION_BUNDLE`)
  that forced artificial churn on long-lived consumer releases.
- Sibling gates remain authoritative for fingerprint integrity, material
  living-spec mutation, and specs-manifest freshness. The currency gate adds
  the explicit rule: when base `AGENTS.md` release pin differs from
  `RELEASE_VERSION`, release-bound paths must appear in `base...HEAD`.
- `workspace::ensure_required_governance_artifacts_in_pr` and the CI
  `governance-artifacts` job require present+valid artifacts, not mandatory
  PR-diff membership. `governance_artifacts::run_inventory` treats
  `all_in_pr_diff` as informational only.

## Topology
```mermaid
flowchart LR
  H[Host Application] --> L[Library API]
  L --> D[Domain Core]
  D --> AD[Adapter Layer]
  AD --> DB[(Store)]
  AD --> N[Network]
```

## Store Boundaries
```mermaid
flowchart LR
  I[Inbound Requests] --> C[Core Logic]
  C --> W[(Write Store)]
  C --> R[(Read Store)]
```

## Happy Path Sequence
```mermaid
sequenceDiagram
  participant C as Client
  participant G as API
  participant D as Domain
  participant DB as Datastore
  C->>G: Request
  G->>D: Validate + execute
  D->>DB: Commit transaction
  DB-->>D: Commit ok
  D-->>G: Domain result
  G-->>C: Response + trace_id
```

## Error Path
```mermaid
sequenceDiagram
  participant Client
  participant Service
  participant Store
  Client->>Service: Request
  Service->>Store: Database Query
  Store--xService: Error/Timeout
  Service-->>Client: Typed Error / Recovery Instructions
```

## Execution Path
- Ingress parse + validation:
- Policy/interlock checks:
- Core execution + persistence:
- Verification and artifact emission:

## Concurrency and Runtime Model
- Execution model:
- Isolation boundaries:
- Backpressure strategy:
- Shared state synchronization:

## Deployment Topology
- Runtime units:
- Region/zone model:
- Rollout strategy (blue/green/canary):
- Rollback trigger and blast-radius scope:

## Data and Contracts
- Inbound contracts (CLI/API/events):
- Outbound dependencies (datastores/queues/external APIs):
- Data ownership boundaries:
- Schema evolution + migration policy: Decapod owns migration identity, ordering, version gates, applied-ledger persistence, backup/restore, and legacy-store import. Storage execution is a replaceable boundary.

## Current PR Control-Plane Sequence
1. Bootstrap discovery and diagnostics (`capabilities`, constitution lookup,
   embedded docs, and version inspection) dispatch before stateful setup so a
   damaged or missing datastore cannot hide the recovery instructions.
2. For stateful commands, the CLI resolves the repository governance root and
   creates the local data directory when required.
3. Before stateful command dispatch, migration reconciliation runs against the
   version-counter and applied-migration ledger.
4. Successful version transitions return a report; the agent-facing command
   emits a warning/instruction naming the applied migrations and inspection
   paths.
5. Trajectory initialization validates the requested run only when the existing
   cookie is a valid same-run artifact, then atomically replaces the single
   tracked JSON file.
6. Validation and publication consume the resulting artifact; no runtime
   reader treats appended JSON values as history.

## Artifact Ownership Matrix
| Artifact | Authority | Update Mechanism | Failure Policy | Historical Record |
|---|---|---|---|---|
| Managed spec authored sections | Project/user contract | Agent-authored PR edit | Material change required when affected | Git history |
| Specs attestation/manifest | Decapod projection | Governed refresh | Validation blocks stale/malformed projection | Git history |
| Managed migration ledger/catalog | Migration history | Startup migration check | Backup/restore and visible failure | Git history |
| Governance trajectory file | Current run evidence | Atomic replace/update | Invalid legacy cookie is replaced by explicit init | Git history |

## Governance Authority and Evidence Boundaries
- Each exact registered directive H3 in `.decapod/OVERRIDE.md` owns a fenced human-authored documentation body. The scaffold uses a four-backtick Markdown source block so headings and nested triple-backtick examples do not render as outer document structure. `core::assets` extracts the wrapper-free body, preserves legacy body bytes during upgrade, and fails the whole overlay on unclosed wrappers, duplicate exact IDs, or non-empty unknown Decapod-namespaced IDs.
- Context resolution and context capsules carry derived authority evidence: directive ID, source path, source hash, body hash, byte count, and precedence.
- `core::events` is the semantic read/write boundary for append-only runtime evidence. Callers do not bind to per-stream tables, a future consolidated table, or legacy JSONL.
- Startup migration reconciles unproven legacy JSONL into canonical `.decapod/data/decapod.db` idempotently before governed consumers run. A successful single-datastore migration is durable proof that its JSONL inputs are retired. If an older binary recreates legacy SQLite stores, startup copies their newer rows forward and removes them before consumers run. Fresh conflicts fail visibly.

## ADR Register
| ADR | Title | Status | Rationale | Date |
|---|---|---|---|---|
| ADR-001 | Initial topology choice | Proposed | Define first stable architecture | YYYY-MM-DD |

## Delivery Plan (first 3 slices)
- Slice 1 (ship first):
- Slice 2:
- Slice 3:

## Risks and Mitigations
| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Contract drift across components | Medium | High | Spec + schema checks in CI |
| Runtime saturation under peak load | Medium | High | Capacity model + load tests |

<!-- decapod:capability-overlay:persistent-state:start -->

## Persistent State Architecture Overlay

### State Ownership
- Each entity type MUST have a designated state owner
- State ownership boundaries MUST be explicitly documented
- Cross-boundary state access MUST go through defined interfaces

### Transaction Boundaries
- All multi-entity mutations MUST occur within explicit transactions
- Transaction boundaries MUST be documented in ARCHITECTURE.md
- Compensating transactions for distributed operations

### Storage Abstraction
- Storage ownership, consistency behavior, and access boundaries MUST be explicit
- Portability or swappable implementations are project decisions, not universal requirements
- Migration and rollback treatment MUST match the selected storage technology
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `8323113c658e1bc9c9215b9397ca3f106ea1f459b971ea0edc12cd460eb4da06`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (102 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

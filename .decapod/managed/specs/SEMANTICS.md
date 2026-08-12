# Semantics

## State Machines
```mermaid
stateDiagram-v2
  [*] --> Draft
  Draft --> InProgress
  InProgress --> Verified
  InProgress --> Blocked
  Blocked --> InProgress
  Verified --> [*]
```

## Invariants
| Invariant | Type | Validation |
|---|---|---|
| No promoted change without proof | System | validation gate |
| Canonical source-of-truth per entity | Data | interface/spec review |
| Mutation events are replayable | Data | deterministic replay |
| Reported authority equals loaded authority | Governance | resolved authority hashes and capsule custody |
| Reported evidence equals canonical observations | Governance | semantic event-query boundary and migration tests |
| Durable task or agent state and its required event commit together | Data | broker transaction rollback regression |
| Read-only projections do not acquire mutation transactions | Runtime | operation-path review and read-path tests |

The local proof covers the Decapod-owned portion of the shared-state contract:
state-plus-event mutations are atomic before they cross the future Dactyl
boundary. Dactyl must provide the corresponding atomic operation primitive,
while Propodus must host compatible ordinary database constraints; neither
backend decides whether a Decapod transition is valid.

The Dactyl bridge exercises this boundary with caller-owned identifiers, an
atomic batch that rolls back on constraint failure, read-only enforcement, and
typed error normalization. It is intentionally an isolated physical-driver
probe: the canonical local SQLite route fails closed until Dactyl can prove
file-backed compatibility, and the cloud route requires an opaque bearer
without deriving organization or repository semantics in Decapod.

## Event Sourcing Schema
| Field | Type | Description |
|---|---|---|
| event_id | string | globally unique event id |
| aggregate_id | string | entity/workflow id |
| event_type | string | semantic transition |
| payload | object | transition data |
| recorded_at | timestamp | append time |

## Replay Semantics
- Replay order: canonical sequence ascending for replay and timeline construction
- Conflict resolution: identical event IDs with equal semantic events are idempotent across fresh legacy and split-envelope storage shapes; different fresh events fail with `LEGACY_EVENT_CONFLICT`. Inputs covered by a successful single-datastore migration are retired evidence and are not re-read.
- Snapshot cadence:
- Determinism proof strategy: delete preserved JSONL after import and compare validation, health, heartbeat, and flight-recorder results

## Error Code Semantics
- Namespace:
- Stable compatibility window:
- Mapping to retry/degrade behavior:

## Domain Rules
- Business rule 1:
- Business rule 2:
- Business rule 3:

## Idempotency Contracts

| Operation | Idempotency Key | Duplicate Behavior |
|---|---|---|
| create/update mutation | request_id | return original result |
| async enqueue | event_id | ignore duplicate enqueue |

## Current Governance Artifact Semantics
### Trajectory Cookie
- Cardinality: one file, one canonical object.
- Replacement: a new run replaces the previous cookie through an atomic
  write; same-run initialization remains a duplicate error for a valid object.
- Recovery: an explicit new initialization may replace a malformed or appended
  legacy cookie, restoring the single-object invariant.
- History: the repository commit graph is the history mechanism.

### Migration Notice
- Detection is idempotent and occurs before each local command dispatch.
- A release transition is reported once when the version counter advances;
  applied migration IDs are also reported when work is performed.
- The command must stop on migration/verification failure. A notice does not
  authorize the agent to skip migration-specific instructions.

## Language Note

- Primary language inferred: Rust

<!-- decapod:capability-overlay:background-processing:start -->

## Background Processing Semantics Overlay

### Retry Semantics
- Retry and backoff behavior MUST be selected and documented for each work class
- Poison-work handling MUST be selected and documented for each work class
- Retry MUST preserve the declared side-effect and idempotency semantics

### Idempotency
- Each job MUST declare whether it is idempotent, transactional, compensating, or otherwise duplicate-safe
- Deduplication or compensation mechanisms are project decisions and require proof
- Duplicate execution MUST follow the job's declared duplicate-handling semantics

### Poison Message Handling
- Messages failing after max retries go to dead letter queue
- DLQ MUST be monitored and alerted
- Manual replay capability for DLQ messages
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

## Persistent State Semantics Overlay

### Transaction Semantics
- All multi-entity operations MUST be atomic
- Read-after-write consistency within transaction boundaries
- Eventual consistency windows MUST be documented

### Migration Semantics
- Schema migrations MUST be backward-compatible
- Migration rollback procedures MUST be documented
- Data integrity checks post-migration

### Recovery Semantics
- Point-in-time recovery capability
- Recovery objectives MUST be selected for the project and recorded as proof obligations
- Recovery test cadence MUST be selected for the project and recorded as a proof obligation
<!-- decapod:capability-overlay:persistent-state:end -->

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `e3322ee47bc3060006029a5ddae3f36e5a88979405b6a2f40f033af6a26c583b`
- Significant implementation surfaces: `.github/` (9 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (103 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

# Semantics## State Machines
```mermaid
stateDiagram-v2
  [*] --> Draft
  Draft --> InProgress
  InProgress --> Verified
  InProgress --> Blocked
  Blocked --> InProgress
  Verified --> [*]
```## Invariants
| Invariant | Type | Validation |
|---|---|---|
| No promoted change without proof | System | validation gate |
| Canonical source-of-truth per entity | Data | interface/spec review |
| Mutation events are replayable | Data | deterministic replay |## Event Sourcing Schema
| Field | Type | Description |
|---|---|---|
| event_id | string | globally unique event id |
| aggregate_id | string | entity/workflow id |
| event_type | string | semantic transition |
| payload | object | transition data |
| recorded_at | timestamp | append time |## Replay Semantics
- Replay order:
- Conflict resolution:
- Snapshot cadence:
- Determinism proof strategy:## Error Code Semantics
- Namespace:
- Stable compatibility window:
- Mapping to retry/degrade behavior:## Domain Rules
- Business rule 1:
- Business rule 2:
- Business rule 3:## Idempotency Contracts
| Operation | Idempotency Key | Duplicate Behavior |
|---|---|---|
| create/update mutation | request_id | return original result |
| async enqueue | event_id | ignore duplicate enqueue |## Language Note
- Primary language inferred: Rust

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `8bd5bdb5eb8da3aaced6c1772378bc7364bfeb2fa926f35dbb33d9d4f6c47c03`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (100 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

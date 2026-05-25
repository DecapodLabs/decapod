# Architecture

## Direction
cli

## Current Facts
- Runtime/languages: Rust
- Detected surfaces/framework hints: cargo
- Product type: cli

## Topology
```mermaid
flowchart LR
  U[User] --> C[CLI Entrypoint]
  C --> R[Command Router]
  R --> E[Core Engine]
  E --> S[(Local Store)]
  E --> X[External APIs / Filesystem]
```

## Store Boundaries
```mermaid
flowchart LR
  I[Inbound Requests] --> C[Core Runtime]
  C --> W[(Write Store)]
  C --> R[(Read Store)]
  C --> E[External Dependency]
  E --> DLQ[(DLQ / Retry Queue)]
```

## Happy Path Sequence
```mermaid
sequenceDiagram
  participant U as User
  participant C as CLI
  participant E as Core Engine
  participant S as Store
  U->>C: Run command
  C->>E: Parse + validate
  E->>S: Persist mutation
  S-->>E: Ack
  E-->>C: Result
  C-->>U: Structured output
```

## Error Path
```mermaid
sequenceDiagram
  participant Client
  participant API
  participant Upstream
  Client->>API: Request
  API->>Upstream: Call with timeout budget
  Upstream--xAPI: Timeout / failure
  API-->>Client: Typed error + retry guidance + trace_id
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
- Schema evolution + migration policy:

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

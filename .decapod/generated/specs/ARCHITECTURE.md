# Architecture

## Direction

Composable repository architecture with explicit boundaries and proof-backed delivery invariants. Decapod is a daemonless control plane for AI coding agents - invoked on-demand, exits immediately after, state persists locally.

## Current Facts

- Runtime/languages: rust
- Detected surfaces/framework hints: cargo
- Product type: service_or_library

## Topology

```mermaid
Human Intent
  │
  ▼
AI Agent (Claude/Codex/Gemini/Cursor)
  │
  │ decapod rpc --op agent.init
  ▼
Decapod Runtime (On-Demand)
  ├── Session Manager
  ├── Workspace Isolation
  ├── Todo Ledger
  ├── Validate Gates
  ├── Knowledge Store
  └── Context Capsule
  │
  ▼
Repository + Services
  ├── Git worktree (.decapod/workspaces/*)
  ├── SQLite ledger (.decapod/data/*.db)
  └── Constitution embedded in binary
```

### System Topology with Store Boundaries

```mermaid
graph TD
    subgraph "Agent Process"
        A[AI Agent]
    end

    subgraph "Decapod Runtime (On-Demand)"
        R[Runtime]
        S[Session Manager]
        W[Workspace Isolation]
        V[Validate Gate]
    end

    subgraph "Repo Store (<repo>/.decapod/)"
        DB[(SQLite)]
        J[JSONL Logs]
        WT[Git Worktree]
    end

    subgraph "User Store (~/.decapod/)"
        UDB[(SQLite)]
        UJ[JSONL Logs]
    end

    A -->|"rpc --op agent.init"| R
    R --> S
    R --> W
    W -->|"validate"| V
    V -->|"receipt + hash"| A

    R -->|read/write| DB
    R -->|append| J
    W -->|branch| WT

    R -.->|"--store user"| UDB

    style W fill:#f9f,stroke:#333
    style V fill:#9f9,stroke:#333
    style DB fill:#ff9,stroke:#333
    style UDB fill:#ff9,stroke:#333
```

### Happy Path Sequence

```mermaid
sequenceDiagram
    participant Agent as AI Agent
    participant Decapod as Decapod Runtime
    participant Git as Git Worktree
    participant Store as SQLite/JSONL

    Note over Agent,Decapod: 1. Initialize session
    Agent->>Decapod: decapod rpc --op agent.init
    Decapod->>Store: Create session receipt
    Decapod-->>Agent: session_id, allowed_next_ops

    Note over Agent,Decapod: 2. Ensure workspace
    Agent->>Decapod: decapod workspace ensure
    Decapod->>Git: git worktree add
    Git-->>Decapod: worktree_path
    Decapod-->>Agent: branch_name, worktree_path

    Note over Agent,Decapod: 3. Add task
    Agent->>Decapod: decapod todo add "implement X"
    Decapod->>Store: Append to todos.jsonl
    Decapod-->>Agent: task_id, status=Draft

    Note over Agent,Decapod: 4. Claim task
    Agent->>Decapod: decapod todo claim --id <task_id>
    Decapod->>Store: Update status=Claimed
    Decapod-->>Agent: status=Claimed

    Note over Agent,Decapod: 5. Do work
    Agent->>Git: Edit files, run tests
    Git-->>Agent: Success

    Note over Agent,Decapod: 6. Validate
    Agent->>Decapod: decapod validate
    Decapod->>Store: Check gates
    alt All gates pass
        Decapod-->>Agent: receipt with hash, exit 0
    else Gates fail
        Decapod-->>Agent: typed error, exit non-zero
    end

    Note over Agent,Decapod: 7. Complete task
    Agent->>Decapod: decapod todo done --id <task_id>
    Decapod->>Store: Update status=Verified
    Decapod-->>Agent: status=Verified
```

### Preflight Before Operation

```mermaid
sequenceDiagram
    participant Agent
    participant Decapod

    Note over Agent,Decapod: BEFORE any operation
    Agent->>Decapod: decapod preflight --op todo.add
    Decapod->>Workspace: Check workspace status
    Workspace-->>Decapod: git_is_protected, can_work
    Decapod->>Capsules: Determine required context
    Decapod-->>Agent: risk_flags[], likely_failures[], required_capsules[]

    Note over Agent,Decapod: Agent decides to proceed or fix issues
    Agent->>Decapod: decapod workspace ensure
    Agent->>Decapod: decapod todo add "task"
```

### Impact Before Validate

```mermaid
sequenceDiagram
    participant Agent
    participant Decapod

    Note over Agent,Decapod: AFTER changes, BEFORE validate
    Agent->>Decapod: decapod impact --changed-files "src/a.rs,src/b.rs"
    Decapod->>Workspace: Check workspace status
    Decapod->>Validate: Predict gate outcomes
    Decapod-->>Agent: will_fail_validate, predicted_failures[], recommendation

    alt will_fail_validate = true
        Agent->>Decapod: Fix issues
    else will_fail_validate = false
        Agent->>Decapod: decapod validate
    end
```

## Execution Path

```
Input/Event --> Contract Parse --> Session Init --> Workspace Isolation
      |              |                  |                  |
      +--------------+------------------+------------------+
                        Trace + Metrics + Artifacts
```

### Detailed Data Flow

```mermaid
flowchart LR
    subgraph Input["Input"]
        C[CLI Command]
        R[RPC Call]
        E[Event/File]
    end

    subgraph Parse["Contract Parse"]
        P[Parse & Validate]
        A[Auth Check]
    end

    subgraph Runtime["Runtime"]
        S[Session Manager]
        W[Workspace Manager]
        T[Todo Manager]
        V[Validate Manager]
        K[Knowledge Manager]
    end

    subgraph State["State Layer"]
        DB[(SQLite)]
        J[(JSONL)]
        G[(Git)]
    end

    subgraph Output["Output"]
        O1[Receipt Hash]
        O2[JSON Response]
        O3[Exit Code]
    end

    C --> P
    R --> P
    E --> P
    P --> A
    A --> S
    A --> W
    A --> T
    A --> V
    A --> K
    
    S --> DB
    W --> G
    T --> J
    T --> DB
    V --> DB
    
    S --> O1
    W --> O2
    T --> O1
    V --> O1
    V --> O2
    V --> O3
    
    style P fill:#9ff,stroke:#333
    style Runtime fill:#ff9,stroke:#333
    style V fill:#f99,stroke:#333
```

1. **Agent Initialization**: `decapod rpc --op agent.init` creates session
2. **Workspace Check**: `decapod workspace ensure` creates isolated worktree
3. **Task Tracking**: `decapod todo add/claim/done` with event sourcing
4. **Validation**: `decapod validate` produces proof receipt

### Component Interaction Map

```mermaid
flowchart TB
    S[Session<br/>Manager] -->|creates| SR[Session<br/>Receipt]
    W[Workspace<br/>Isolation] -->|creates| WT[Worktree]
    W -->|blocks| PB[Protected<br/>Branches]
    T[Todo<br/>Ledger] -->|appends| J[todos.jsonl]
    T -->|creates| TR[Task<br/>Receipt]
    V[Validate<br/>Gate] -->|reads| J
    V -->|reads| DB[(SQLite)]
    V -->|emits| VR[Validation<br/>Receipt]
    V -->|emits| EH[Exit Code]
    K[Knowledge<br/>Store] -->|appends| KP[knowledge.jsonl]
    C[Context<br/>Capsule] -->|queries| CE[Constitution<br/>Embed]
    
    SR -->|stored| DB
    TR -->|stored| J
    VR -->|stored| DB
    
    style V fill:#f99,stroke:#333
    style W fill:#f9f,stroke:#333
```

## Data and Contracts

### Inbound Contracts (CLI/API/events)

- CLI: `decapod <command>` with subcommands
- RPC: `decapod rpc --op <operation> --params <json>`
- Events: File-based ledger appends

### Outbound Dependencies (datastores/queues/external APIs)

- SQLite: `.decapod/data/*.db` for persistent state
- JSONL: `.decapod/data/*.jsonl` for event logs
- Git: Worktree management via `git worktree`

### Data Ownership Boundaries

- User store: `~/.decapod/` - personal blank-slate tasks
- Repo store: `<repo>/.decapod/` - project dogfood backlog
- Strict separation: no cross-contamination

## Service Contracts

| Component | Responsibility | CLI Surface | Schema |
|-----------|---------------|-------------|--------|
| Session Manager | Track agent session lifecycle | `decapod session` | Session receipt |
| Workspace Isolation | Enforce branch protection + worktree | `decapod workspace` | Branch state |
| Todo Ledger | Event-sourced task tracking | `decapod todo` | todos.jsonl |
| Validate Gate | Deterministic completion proof | `decapod validate` | Gate results |
| Knowledge Store | Repository knowledge + provenance | `decapod data knowledge` | SQLite |
| Context Capsule | Scoped constitution query | `decapod govern capsule` | Deterministic hash |

## Delivery Plan

1. **Slice 1**: Core session + workspace + todo + validate
2. **Slice 2**: Knowledge store + context capsules
3. **Slice 3**: Policy enforcement + eval gates

## Risks and Mitigations

- **Risk**: Store contamination between user/repo
  - **Mitigation**: Explicit --store flag, validation gates

- **Risk**: Validation hangs under DB contention
  - **Mitigation**: Bounded timeout with typed error (`VALIDATE_TIMEOUT_OR_LOCK`)

- **Risk**: Agent works on main/master
  - **Mitigation**: Workspace enforcement blocks this

- **Risk**: Scope creep beyond daemonless on-demand model
  - **Mitigation**: Constitution documents non-goals explicitly

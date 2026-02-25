# Semantics

## State Machines

### WorkUnit Status Machine

```mermaid
stateDiagram-v2
    [*] --> Draft: todo add
    Draft --> Claimed: todo claim
    Claimed --> Verified: todo done + validate pass
    Verified --> [*]
    Claimed --> Draft: todo release
    Draft --> Draft: Idempotent
    Claimed --> Claimed: Idempotent
    Verified --> Verified: Idempotent
```

**States**:
| State | Meaning | CLI Transition | Enforced By |
|-------|---------|----------------|-------------|
| `Draft` | Task created, not yet started | `todo add` | Binary |
| `Claimed` | Agent has claimed this task | `todo claim` | Binary |
| `Verified` | All proof gates passed + `todo done` | `todo done` (gated by validate) | Binary |
| `Archived` | Task completed and archived | `todo archive` | Binary |

**Actual Transitions** (from code analysis):
- `Draft` → `Claimed` via `decapod todo claim`
- `Claimed` → `Verified` via `decapod todo done` (requires passing `decapod validate`)
- `Claimed` → `Draft` via `decapod todo release`

**Note**: `Executing` is an agent-local internal state, NOT persisted in the ledger. Agents track "work in progress" locally; the ledger only knows `Claimed`.

**Invariant**: Cannot transition to `Verified` without:
1. Passing `decapod validate` first
2. All gates in `proof_plan` have passing `proof_results`

### Store Boundary Enforcement

```mermaid
graph TD
    subgraph "Write Operation"
        A[Agent Command] --> R{Store Flag?}
    end

    R -->|"--store repo"| Repo[Repo Store]
    R -->|"--store user"| User[User Store]
    R -->|none| D{Default}

    D -->|"in repo context"| Repo
    D -->|"in user context"| User

    Repo -.->|"cross-store write attempt"| Violate[STORE_BOUNDARY_VIOLATION]
    User -.->|"cross-store write attempt"| Violate

    style Violate fill:#f99,stroke:#333
    style Repo fill:#ff9,stroke:#333
    style User fill:#9ff,stroke:#333
```

**Invariant**: Explicit `--store` flag required. Cross-store writes produce `STORE_BOUNDARY_VIOLATION`.

## Invariants (Machine-Checkable)

```mermaid
flowchart TB
    subgraph Invariant["Invariant Enforcement"]
        I1[Protected Branch<br/>Check]
        I2[Session Active<br/>Check]
        I3[Proof Gates<br/>Check]
        I4[Store Boundary<br/>Check]
        I5[Timeout<br/>Check]
    end

    subgraph Verify["Verification Commands"]
        V1[decapod workspace status]
        V2[decapod session status]
        V3[decapod validate]
        V4[decapod validate]
        V5[decapod validate]
    end

    subgraph Fail["Failure Codes"]
        F1[WORKSPACE_REQUIRED]
        F2[SESSION_REQUIRED]
        F3[VERIFICATION_REQUIRED]
        F4[STORE_BOUNDARY_VIOLATION]
        F5[VALIDATE_TIMEOUT_OR_LOCK]
    end

    I1 -->|"branch matches"| V1 --> F1
    I2 -->|"no session"| V2 --> F2
    I3 -->|"missing proof"| V3 --> F3
    I4 -->|"cross-store"| V4 --> F4
    I5 -->|"timeout"| V5 --> F5

    style I1 fill:#f99,stroke:#333
    style I3 fill:#f99,stroke:#333
    style I4 fill:#f99,stroke:#333
```

| Invariant | Verification Command | Failure Code |
|-----------|---------------------|--------------|
| No mutation on protected branch | `decapod workspace status` → `git_is_protected` | `WORKSPACE_REQUIRED` |
| Verified requires passing proof gates | `decapod validate` | `VERIFICATION_REQUIRED` |
| Store boundary not crossed | `decapod validate` | `STORE_BOUNDARY_VIOLATION` |
| Validate terminates boundedly | `decapod validate` (30s timeout) | `VALIDATE_TIMEOUT_OR_LOCK` |
| Session required for operations | `decapod session status` | `SESSION_REQUIRED` |

## Event Log Semantics

### Event Flow

```mermaid
sequenceDiagram
    participant Agent
    participant CLI as Decapod CLI
    participant Parse as Command Parse
    participant Validate as Validation
    participant Store as JSONL/SQLite

    Agent->>CLI: decapod todo add "task"
    CLI->>Parse: validate command
    Parse->>Validate: check session + workspace
    Validate->>Store: BEGIN transaction
    Store-->>Validate: lock acquired
    Validate->>Store: APPEND event to todos.jsonl
    Validate->>Store: WRITE to SQLite
    Validate->>Store: COMMIT transaction
    Store-->>Validate: success
    Validate->>Store: COMPUTE receipt hash
    Validate-->>CLI: receipt with hash
    CLI-->>Agent: task_id, status=Draft, hash
```

### Append-Only Rule

All state changes are logged as events in JSONL files:

- `.decapod/data/todos.jsonl` - Task lifecycle events
- `.decapod/data/knowledge.promotions.jsonl` - Knowledge promotions
- `.decapod/data/decisions.jsonl` - Agent decisions

### Receipt Hashing

Every mutation produces a SHA256 receipt:

```rust
// From workunit.rs
pub fn canonical_hash_hex(&self) -> String {
    let bytes = self.canonical_json_bytes()?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x}", hasher.finalize())
}
```

**Invariant**: Receipt hash is computed from canonicalized JSON (sorted keys, deduplicated arrays).

### Deterministic Replay

Given the same event log sequence, replay must produce identical state hashes.

## Proof Surface Semantics

### Validation Gate Contract

```
decapod validate --store repo --format json
```

**Output Schema**:
```json
{
  "gate": "validate",
  "timestamp": "ISO8601",
  "results": [
    { "name": "workspace_isolation", "status": "pass|fail", "detail": "..." }
  ],
  "receipt": {
    "hash": "sha256:...",
    "touched_paths": ["path1", "path2"]
  }
}
```

**Bounded Execution**: Must terminate within 30 seconds or return `VALIDATE_TIMEOUT_OR_LOCK`.

### Capabilities Contract

```mermaid
flowchart LR
    subgraph Command["CLI Command"]
        C[decapod capabilities<br/>--format json]
    end

    subgraph Output["JSON Output"]
        V[version]
        CAP[capabilities]
        SUB[subsystems]
        INT[interlock_codes]
        WS[workspace]
    end

    C --> Output
    
    V --> V1[semver]
    CAP --> CAP1[name]
    CAP --> CAP2[description]
    CAP --> CAP3[stability]
    SUB --> SUB1[name]
    SUB --> SUB2[status]
    SUB --> SUB3[ops]
    INT --> INT1[workspace_required]
    INT --> INT2[verification_required]
    INT --> INT3[store_boundary_violation]
    WS --> WS1[protected_patterns]
    WS --> WS2[docker_available]
    WS --> WS3[enforcement_available]

    style C fill:#9ff,stroke:#333
    style Output fill:#ff9,stroke:#333
```

```
decapod capabilities --format json
```

**Output Schema**:
```json
{
  "version": "semver",
  "capabilities": [
    { "name": "...", "description": "...", "stability": "stable|beta" }
  ],
  "subsystems": [
    { "name": "...", "status": "active", "ops": ["op1", "op2"] }
  ],
  "interlock_codes": ["WORKSPACE_REQUIRED", "VERIFICATION_REQUIRED", ...]
}
```

**Invariant**: Adding a new capability/subsystem/op requires updating this output.

### Schema Contract

```
decapod data schema --format json --deterministic
```

**Output**: JSON schema for all entities with volatile fields (timestamps) removed when `--deterministic` is passed.

**Invariant**: Schema changes are breaking changes and require version bump.

## Error Code Semantics

| Code | Trigger Condition | Remediation |
|------|------------------|--------------|
| `WORKSPACE_REQUIRED` | Operating on protected branch | Run `decapod workspace ensure` |
| `VERIFICATION_REQUIRED` | Claiming done without proof | Run `decapod validate` |
| `STORE_BOUNDARY_VIOLATION` | Cross-store write detected | Use explicit `--store` flag |
| `VALIDATE_TIMEOUT_OR_LOCK` | DB contention > timeout | Retry with backoff |
| `SESSION_REQUIRED` | No active session | Run `decapod session acquire` |

## Agent Loop Semantics

### Complete Agent Workflow

```mermaid
stateDiagram-v2
    [*] --> NoSession: Agent starts
    
    NoSession --> Session: rpc --op agent.init
    Session --> ProtectedBranch: workspace status
    ProtectedBranch --> WorkspaceOk: git_is_protected=false
    
    WorkspaceOk --> AddTask: todo add
    AddTask --> Draft: status=Draft
    
    Draft --> ClaimTask: todo claim
    ClaimTask --> Claimed: status=Claimed
    
    Claimed --> Work: Agent edits files
    Work --> Tests: Run tests
    
    Tests --> Validate: decapod validate
    Validate --> Valid: exit 0
    Valid --> Done: todo done
    
    Validate --> Invalid: exit non-zero
    Invalid --> Work: Fix issues
    
    Done --> [*]
    
    ProtectedBranch --> WorkspaceOk: workspace ensure
```

### Step-by-Step Contract

```
1. decapod rpc --op agent.init
   → Returns: session_id, allowed_next_ops, context_capsule

2. decapod workspace ensure
   → Returns: branch_name, worktree_path
   → Invariant: Cannot be on main/master

3. decapod todo add --text "..."
   → Returns: task_id, events[]
   → Creates: Draft status

4. (repeat)
   decapid <task_idod todo claim -->
   → Transitions: Draft → Claimed

5. decapod todo done --id <task_id>
   → Transitions: Claimed → Verified
   → Requires: proof_plan gates passed

6. decapod validate
   → Returns: pass/fail with receipt
   → Invariant: Must pass before claiming done
```

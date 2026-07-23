# Interfaces

## Contract Principles
- Prefer explicit schemas over implicit behavior.
- Every mutating interface defines idempotency semantics.
- Every failure path maps to a typed, documented error code with auto-remediation guidance.
- Daemonless: all interfaces are synchronous CLI or JSON-RPC over stdin/stdout.
- Capability-gated: external actions (git, docker, cargo, etc.) require declared capability.
- CLI-only access to `.decapod/` (jail rule enforced in entrypoint docs).

## Generated Contract Depth
Generated interface specs include:
- CLI commands with request/response shapes (see `decapod <cmd> --help` and `decapod capabilities`)
- JSON-RPC operations with typed params/results (see `src/core/rpc.rs`)
- SQLite schemas for all 6 consolidated bins (see `src/core/schemas.rs`)
- JSONL event log formats (one per subsystem)
- Context Capsule schema with policy binding
- WorkUnit / Plan / Obligation manifest schemas
- Agent trajectory artifact schema with computed proof status and run-level verdicts
- Capabilities report schema for agent discovery

## CLI / RPC Contracts

### Declared Capability Configuration

`.decapod/config.toml` may declare `repo.declared_capabilities` as a sorted, deduplicated list. The legacy `repo.capabilities` spelling remains readable for compatibility. Capability declarations shape context and generated specifications but do not grant external-action permissions by themselves; runtime authorization remains governed by the action capability and session/policy checks.

### Core Commands (Agent-Facing)

| Command | Purpose | Key Flags | Output |
|---------|---------|-----------|--------|
| `decapod activate` | Activate control plane + run migrations | — | JSON status |
| `decapod init` | Bootstrap repo with config, specs, entrypoints | `--dir`, `--force`, `--specs`, `--ci`, `--all`, `--proof` | Scaffolded files |
| `decapod validate` | Run all blocking validation gates | `--store user\|repo`, `--format`, `--refresh-specs`, `--verbose` | ValidationReport (JSON/text) |
| `decapod capabilities` | Agent discovery: version, caps, subsystems, config | `--format json\|text` | CapabilitiesReport |
| `decapod session acquire` | Get session token (required for agent ops) | — | Session token |
| `decapod session status` | Show current session | — | SessionInfo |
| `decapod session release` | Release session token | — | void |
| `decapod workspace ensure` | Create/enter isolated worktree | `--branch`, `--container` | WorkspaceStatus |
| `decapod workspace status` | Check workspace state | — | WorkspaceStatus |
| `decapod workspace publish` | Commit, push, create PR | `--title`, `--description` | PublishResult |
| `decapod workspace prune` | Remove stale worktrees | `--force` | PrunedWorkspace[] |
| `decapod todo add` | Create task | `--title`, `--priority`, `--tags`, `--ref`, `--scope` | Task |
| `decapod todo claim` | Claim task exclusively | `--id`, `--agent`, `--mode` | ClaimResult |
| `decapod todo done` | Complete task with optional validation | `--id`, `--validated`, `--artifact` | DoneResult |
| `decapod todo list` | List tasks | `--status`, `--scope`, `--tags` | Task[] |
| `decapod govern plan init` | Create governed plan | `--title`, `--intent`, `--todo-id`, `--proof-hook` | Plan |
| `decapod govern plan check-execute` | Verify plan ready for execution | `--todo-id` | Plan / Error |
| `decapod govern workunit init` | Create workunit manifest | `--task-id`, `--intent-ref` | WorkUnitManifest |
| `decapod govern workunit transition` | Transition workunit status | `--task-id`, `--to` | WorkUnitManifest |
| `decapod govern trajectory init` | Create inspectable run trajectory | `--run-id`, `--original-intent`, `--derived-intent`, `--boundary`, `--scope` | TrajectoryArtifact |
| `decapod govern trajectory record` | Record run actions, checks, assumptions, and claims | `--run-id`, `--inspected-file`, `--modified-file`, `--command`, `--check` | TrajectoryArtifact |
| `decapod govern trajectory get` | Inspect the complete run trajectory artifact | `--run-id` | TrajectoryArtifact |
| `decapod govern trajectory status` | Show computed trajectory proof and verdicts | `--run-id` | TrajectoryStatus |
| `decapod govern capsule query` | Deterministic context capsule | `--topic`, `--scope`, `--limit`, `--risk-tier`, `--write` | DeterministicContextCapsule |
| `decapod obligation add` | Add obligation node | `--intent`, `--risk`, `--depends-on`, `--proofs` | ObligationNode |
| `decapod obligation verify` | Derive obligation status | `--id` | ObligationValidationResult |
| `decapod data knowledge add` | Add knowledge entry | `--id`, `--title`, `--text`, `--provenance` | KnowledgeEntry |
| `decapod data knowledge search` | Search knowledge | `--query` | KnowledgeEntry[] |
| `decapod qa verify` | Proof replay + drift check | — | VerificationReport |
| `decapod qa verify completion <ID>` | Generate, verify, export, or import completion evidence | `--write`, `--path`, `--export`, `--import` | CompletionEvidenceVerification |
| `decapod infer orientation` | Pre-inference context packet | `--intent`, `--task-id`, `--format` | OrientationPacket |
| `decapod infer validate` | Post-inference verification | `--result`, `--intent`, `--format` | ValidationResult |
| `decapod rpc --op <op>` | JSON-RPC interface | `--params`, `--stdin` | RpcResponse |

### JSON-RPC Operations (Agent-Native)

All RPC calls use standard envelope:
```json
{
  "op": "operation.name",
  "params": {},
  "id": "ulid",
  "session": "optional-token"
}
```

Response envelope (`RpcResponse`):
```json
{
  "id": "ulid",
  "success": true,
  "receipt": { "op": "...", "timestamp": "...", "inputs_hash": "...", "outputs_hash": "...", "touched_paths": [], "governing_anchors": [] },
  "mandates": [],
  "context_capsule": { "fragments": [], "spec": "...", "architecture": "...", "security": "...", "standards": {} },
  "result": {},
  "allowed_next_ops": [{ "op": "...", "reason": "...", "required_params": [] }],
  "blocked_by": [],
  "interlock": null,
  "advisory": null,
  "attestation": null,
  "error": null
}
```

#### RPC Operation Catalog

| Operation | Params | Result | Purpose |
|-----------|--------|--------|---------|
| `agent.init` | `{}` | `AgentInitResult` | Environment context + tool summary |
| `workspace.status` | `{}` | `WorkspaceStatusResult` | Git branch, protection, container, can_work |
| `workspace.ensure` | `{ branch?: string }` | `WorkspaceEnsureResult` | Create/enter isolated worktree |
| `workspace.publish` | `{ title?, description? }` | `WorkspacePublishResult` | Commit, push, create PR |
| `context.resolve` | `{ op?, touched_paths?, intent_tags?, query?, limit? }` | `ContextResolveResult` | Constitution fragments + project specs |
| `context.capsule.query` | `{ topic, scope, task_id?, workunit_id?, limit?, risk_tier?, write? }` | `DeterministicContextCapsule` | Deterministic context with policy binding |
| `constitution.get` | `{ section, subsection? }` | `ConstitutionGetResult` | Structured constitution section |
| `constitution.links.query` | `{ section }` | `ConstitutionLinksQueryResult` | Bidirectional links |
| `constitution.links.navigate` | `{ start_section, intent }` | `ConstitutionLinksNavigateResult` | Graph navigation |
| `constitution.migrate` | `{ target_version }` | `ConstitutionMigrateResult` | Schema migration |
| `schema.get` | `{ entity? }` | `SchemaGetResult` | JSON Schema for entity |
| `store.upsert` | `{ entity?, payload?, provenance? }` | `StoreUpsertResult` | Deterministic storage |
| `store.query` | `{ entity?, query? }` | `StoreQueryResult` | Canonical retrieval |
| `validate.run` | `{ gate?, refresh_specs? }` | `ValidateRunResult` | Run validation gates |
| `scaffold.next_question` | `{ project_name? }` | `ScaffoldNextQuestionResult` | Interview engine |
| `scaffold.apply_answer` | `{ question_id, value }` | `ScaffoldApplyAnswerResult` | Record answer |
| `scaffold.generate_artifacts` | `{}` | `ScaffoldGenerateArtifactsResult` | Emit specs/ADRs |
| `standards.resolve` | `{}` | `StandardsResolveResult` | Applicable standards |
| `mentor.obligations` | `{ op?, params?, touched_paths?, diff_summary?, project_profile_id?, session_id?, high_risk? }` | `MentorObligationsResult` | Compute obligations for op |
| `assurance.evaluate` | `{ op?, params?, touched_paths?, diff_summary?, session_id?, phase?, time_budget_s? }` | `AssuranceEvaluateResult` | Interlock + advisory + attestation |
| `specs.refresh` | `{}` | `SpecsRefreshResult` | Regenerate specs manifest |
| `agent.registry.query` | `{ active_only? }` | `AgentRegistryQueryResult` | Active agent sessions |

### Subsystem CLIs (Human-Facing)

| Command Group | Subcommands |
|---------------|-------------|
| `decapod constitution` | `get`, `links query`, `links navigate`, `migrate`, `list`, `search` |
| `decapod docs` | `show`, `search`, `ingest`, `validate` |
| `decapod data` | `archive`, `knowledge`, `context`, `schema`, `repo`, `broker`, `aptitude`, `federation`, `primitives`, `map` |
| `decapod auto` | `cron`, `reflex`, `workflow`, `container` |
| `decapod qa` | `verify`, `check`, `gatling`, `eval`, `demo` |
| `decapod decide` | Architecture decision prompting |
| `decapod trace` | `export`, `flight-recorder` |
| `decapod system` | `version`, `doctor`, `capabilities` |
| `decapod context` | `infer`, `lcm`, `internalize`, `preflight`, `impact` |
| `decapod release` | `check`, `inventory`, `lineage-sync` |
| `decapod setup` | `hook` |
| `decapod obligation` | `add`, `list`, `get`, `verify`, `complete`, `validate-graph` |
| `decapod todo` | Full task management (see above) |

## Event Consumers

| Consumer | Event Source | Ordering | Retry | DLQ |
|----------|--------------|----------|-------|-----|
| `todo` rebuild | `todo.events.jsonl` | FIFO (timestamp) | Deterministic rebuild | N/A (rebuild is idempotent) |
| `broker` verify | `broker.events.jsonl` | FIFO | Crash divergence detection | N/A |
| `health` cache | `proof.events.jsonl` | FIFO | SLA-based staleness | N/A |
| `federation` sync | `federation.events.jsonl` | FIFO | Merkle sync | N/A |
| `flight-recorder` | All `.jsonl` | Timestamp merge | Read-only | N/A |

## Outbound Dependencies

| Dependency | Purpose | Capability | Timeout | Circuit Breaker |
|------------|---------|------------|---------|-----------------|
| `git` | Worktree, commit, push, status | `VcsRead`, `VcsWrite` | 30s | Retry 3x |
| `docker` / `podman` | Container build/run | `ContainerExec` | 300s | Fallback to local binary |
| `cargo` | Test, build, clippy, fmt | `ProofExec` | 600s | N/A |
| `gh` (GitHub CLI) | PR creation | `VcsWrite` | 30s | Optional (skip if missing) |
| `curl` | Crates.io version check | `ProofExec` | 5s | Cache 24h |

## Inbound Contracts

### CLI Surfaces
- **Entrypoint**: `decapod` binary (clap 4, derived from `src/cli.rs`)
- **Global flags**: `--format json|text` (on most commands), `--verbose` (validate)
- **Shell completions**: Generated via clap (bash, zsh, fish, powershell)

### JSON-RPC Surface
- **Transport**: stdin/stdout (newline-delimited JSON)
- **Session**: `DECAPOD_SESSION_PASSWORD` env var + `session.acquire` RPC
- **Capabilities**: `decapod capabilities` / `rpc --op agent.init`

### Repository-Detected Surfaces
- `cargo` (Rust project)
- `git` (version control)
- `docker` / `podman` (container runtime)

## Data Ownership

### Store Roots
| Store Kind | Path | Semantics |
|------------|------|-----------|
| User | `~/.decapod/data/` | Blank slate, no auto-seeding, agent-local |
| Repo | `<repo>/.decapod/data/` | Dogfood backlog, event-sourced, deterministic rebuild |

### SQLite Databases (4 Consolidated Bins + 2 Additional)

| Bin | File | Purpose | Key Tables |
|-----|------|---------|------------|
| Governance | `governance.db` | Policies, health, feedback, archive, obligations | `approvals`, `claims`, `proof_events`, `health_cache`, `feedback`, `archives`, `obligations`, `obligation_edges` |
| Memory | `memory.db` | Federation knowledge graph, decisions | `nodes`, `sources`, `edges`, `federation_events` |
| Knowledge | `knowledge.db` | Project knowledge base | `knowledge` |
| Decide | `decisions.db` | Decision sessions + trees | `sessions`, `decisions` |
| Automation | `automation.db` | Cron + Reflex | `cron_jobs`, `reflexes` |
| Todo | `todo.db` | Task tracking + event sourcing | `tasks`, `task_events`, `categories`, `agent_*`, `risk_zones`, `task_verification` |
| LCM | `lcm.db` | Lossless Context Management | `originals_index`, `summaries` |
| Aptitude | `aptitude.db` | Agent preferences/patterns | `preferences`, `patterns`, `observations`, `consolidations`, `agent_prompts` |

### Event Logs (JSONL)
| Log | Schema | Retention |
|-----|--------|-----------|
| `todo.events.jsonl` | `TodoEvent` | Permanent (audit) |
| `broker.events.jsonl` | `BrokerEvent` | Permanent |
| `federation.events.jsonl` | `FederationEvent` | Permanent |
| `proof.events.jsonl` | `ProofEvent` | Permanent |
| `watcher.events.jsonl` | `WatcherEvent` | Permanent |
| `map.events.jsonl` | `MapEvent` | Permanent |
| `lcm.events.jsonl` | `LcmEvent` | Permanent |
| `memory.events.jsonl` | `MemoryEvent` | Permanent |
| `assurance_attestations.jsonl` | `Attestation` | Permanent |

### Read/Write Ownership
| Path | Writer | Readers |
|------|--------|---------|
| `.decapod/data/*.db` | Decapod CLI (single-writer per store) | Decapod CLI, validation (read-only) |
| `.decapod/data/*.jsonl` | Decapod CLI (append-only) | Decapod CLI, flight-recorder, rebuild |
| `.decapod/generated/context/*.json` | `capsule.query --write` | Current-run context only; ignored by Git |
| `.decapod/generated/artifacts/provenance/completion_evidence/*.json` | `qa verify completion <ID> --write` | Completion verifier, promotion review |
| `.decapod/generated/artifacts/provenance/completion_evidence/imports/*.json` | `qa verify completion <ID> --import` | Structural inspection and receiver-local decision |
| `.decapod/generated/policy/context_capsule_policy.json` | `init` / scaffold | Current-run capsule query resolution; ignored by Git |
| `.decapod/generated/specs/*.md` | `init --force` / `rpc specs.refresh` | Validation, agents |
| `.decapod/generated/artifacts/provenance/*.json` | `workspace publish` / `validate` | Current-run diagnostics/evidence; ignored by Git |
| `.decapod/governance/trajectory.json` | `govern trajectory` / `validate` | Tracked current-run custody and proof review; historical states are recovered from Git |
| `.decapod/governance/validation.json` | `validate` | Tracked current successful validation receipt; overwritten per commit and historically recoverable from Git |

## Error Taxonomy

```rust
#[derive(Debug, thiserror::Error)]
pub enum DecapodError {
    #[error("validation failed: {0}")]
    ValidationError(String),  // Auto-remediable with agent_action hint
    #[error("not found: {0}")]
    NotFound(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    IoError(std::io::Error),
    #[error("SQLite error: {0}")]
    RusqliteError(rusqlite::Error),
    #[error("not implemented: {0}")]
    NotImplemented(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}
```

**Auto-remediable errors** include `agent_action` and `user_note` in message:
```
AUTOREMEDIABLE_VALIDATION_ERROR code=WORKSPACE_TODO_CLAIM_CONFLICT severity=transient auto_remediable=true audience=agent agent_action="inspect `decapod todo list`..." user_note="..."
```

## Failure Semantics

| Failure Class | Retry/Interlock Code | Retry/Backoff | Client Contract | Observability |
|---------------|------------------------|---------------|-----------------|---------------|
| Validation (input) | `VALIDATION_ERROR` | No retry | 4xx equivalent + `agent_action` | warn log + metric |
| Workspace required | `INTERLOCK: workspace_required` | Conditional (after `workspace.ensure`) | Blocked + `unblock_ops` | info log + attestation |
| Protected branch | `INTERLOCK: protected_branch` | No retry | Blocked + `resolve_hint` | warn log |
| Store boundary violation | `INTERLOCK: store_boundary_violation` | No retry | Blocked + allowed control ops | error log + attestation |
| Verification required | `INTERLOCK: verification_required` | Conditional (after proofs) | Blocked + `qa.check`, `validate` | info log |
| Decision required | `INTERLOCK: decision_required` | Conditional (after human input) | Blocked + `scaffold.next_question` | info log |
| Dependency timeout | `VcsWrite`/`ContainerExec` timeout | Exponential (3x) | 503 equivalent + `retry_after` | error log + alert |
| SQLite lock contention | `SQLiteOpen` retry | 5 retries, exp backoff + jitter | Transparent | debug log |
| Capability missing | `PermissionDenied` | No retry | 403 equivalent + required capability | error log |

## Timeout Budget

| Hop | Budget | Notes |
|-----|--------|-------|
| CLI parse → dispatch | 50ms | Clap derivation |
| RPC request → response | 500ms | Includes validation, context resolve |
| Context capsule query | 200ms | Embedded doc search + policy resolve |
| Workspace ensure (git) | 30s | Worktree create + optional container build |
| Workspace ensure (container) | 300s | Docker build + run |
| Validation (full) | 30s | Bounded by `INV-BOUNDED-VALIDATE` |
| Proof execution (cargo test) | 600s | Configurable via `proofs.toml` |
| JSON-RPC roundtrip | 1s | stdin/stdout framing |

## Interface Versioning
- **Version strategy**: Semantic versioning for binary (`Cargo.toml` version), schema_version in config.toml (1.0.0), POLICY_SCHEMA_VERSION for capsule policy
- **Backward-compatibility**:
  - CLI: New flags additive; breaking changes require major version
  - RPC: Operation names stable; new ops additive; params/results extensible
  - Schemas: Additive migrations only (TODO_SCHEMA_VERSION tracks)
  - Config: `schema_version` gate in validate
- **Deprecation window**: 2 minor versions for CLI flags; RPC ops never removed
- **Removal policy**: Never remove RPC ops; mark CLI subcommands deprecated in help

## Key Data Structures (Schema References)

### WorkspaceStatus
```json
{
  "can_work": true,
  "git": { "current_branch": "agent/feat_abc", "is_protected": false, "in_worktree": true, "worktree_path": "...", "is_main_repo": false, "has_local_mods": false },
  "container": { "in_container": false, "docker_available": true },
  "blockers": [],
  "required_actions": []
}
```

### DeterministicContextCapsule
```json
{
  "schema_version": "1.1.0",
  "topic": "auth",
  "scope": "core",
  "task_id": "feat_01...",
  "workunit_id": "feat_01...",
  "sources": [{ "path": "core/AUTH.md", "section": "OAuth" }],
  "snippets": [{ "source_path": "core/AUTH.md", "text": "..." }],
  "policy": { "risk_tier": "medium", "policy_hash": "...", "policy_version": "...", "policy_path": "...", "repo_revision": "abc123" },
  "capsule_hash": "sha256..."
}
```

### WorkUnitManifest
```json
{
  "task_id": "feat_01...",
  "intent_ref": "Add OAuth2",
  "spec_refs": [".decapod/generated/specs/INTERFACES.md"],
  "state_refs": [".decapod/generated/context/feat_01....json"],
  "proof_plan": ["cargo test", "decapod validate"],
  "proof_results": [{ "gate": "cargo test", "status": "pass", "artifact_ref": "..." }],
  "validation_epoch": { "epoch_id": "...", "timestamp": "..." },
  "status": "Verified"
}
```

### GovernedPlan
```json
{
  "schema_version": "1.0.0",
  "title": "Add OAuth2",
  "intent": "Implement OAuth2 provider integration",
  "state": "Approved",
  "todo_ids": ["feat_01..."],
  "proof_hooks": ["cargo test", "decapod validate"],
  "unknowns": [],
  "human_questions": [],
  "stop_conditions": [],
  "unresolved_contradictions": [],
  "deferred_questions": [],
  "constraints": { "forbidden_paths": [".decapod/data/"], "file_touch_budget": 50 },
  "phases": [{
    "id": "plan",
    "name": "Plan",
    "description": "Ordered execution phase",
    "entry_gates": [],
    "exit_gates": [],
    "entered": false,
    "completed": false
  }],
  "updated_at": "1720000000Z"
}
```

Plans with phases expose ordered execution through `phase-add`, `enter-phase`, and
`exit-phase`; entry and exit gates must be satisfied before the corresponding
transition, and `DONE` requires every declared phase to be complete.

### ObligationNode
```json
{
  "id": "01...",
  "intent_ref": "Add OAuth2",
  "risk_tier": "medium",
  "required_proofs": ["cargo test", "decapod validate"],
  "state_commit_root": "sha256...",
  "status": "Open",
  "created_at": "...",
  "updated_at": "...",
  "metadata": {}
}
```

### CapabilitiesReport
```json
{
  "version": "0.63.4",
  "capabilities": [{ "name": "daemonless", "description": "...", "stability": "stable", "cost": "none" }, ...],
  "subsystems": [{ "name": "constitution", "status": "active", "ops": ["get", "links.query", ...] }, ...],
  "workspace": { "enforcement_available": true, "docker_available": true, "protected_patterns": ["main", "master", ...] },
  "interview": { "available": true, "artifact_types": ["spec", "architecture", ...], "standards_resolution": true },
  "interlock_codes": ["workspace_required", "verification_required", ...],
  "config": { ... },
  "is_latest": true,
  "latest_version": "0.63.4"
}
```

## Capability-Gated External Actions

Every external command execution goes through `external_action::execute(store_root, capability, op_name, cmd, args, cwd)`:

| Capability | Commands | Audit Log |
|------------|----------|-----------|
| `VcsRead` | `git status`, `git branch`, `git rev-parse` | broker.events.jsonl |
| `VcsWrite` | `git add`, `git commit`, `git push`, `git worktree` | broker.events.jsonl |
| `ContainerExec` | `docker build`, `docker run`, `podman` | broker.events.jsonl |
| `ProofExec` | `cargo test`, `cargo clippy`, `cargo fmt`, custom proofs | proof.events.jsonl |
| `ShellExec` | Arbitrary (restricted) | broker.events.jsonl |

Agents declare needed capabilities via `assurance.evaluate` params; interlocks block undeclared mutations.

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `53c7ec4ec10b518ece79a4f9c05b66a3c3cdfd000531bd5406fc6e67d1df0e3e`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (94 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

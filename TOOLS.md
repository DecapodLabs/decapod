# TOOLS.md — Command Reference and Workflows

## Build & Test

```bash
cargo build --locked                    # Build with locked dependencies
cargo test --locked                     # Run all tests
cargo clippy --all-targets --all-features --locked  # Lint
decapod validate                        # Authoritative completion gate (MUST pass)
```

Environment variables:
```bash
export DECAPOD_LOG=debug                # Enable debug logging
export DECAPOD_AGENT_ID=<id>            # Agent identity (set by session acquire)
export DECAPOD_SESSION_PASSWORD=<pass>  # Session credential (set by session acquire)
```

## Decapod CLI Reference

### Initialization & Session

| Command | Purpose |
|---------|---------|
| `decapod validate` | Run all proof gates. MUST pass before claiming done. |
| `decapod docs ingest` | Load constitution docs into context. |
| `decapod docs show <path>` | Display a constitution document (e.g., `core/DECAPOD.md`). |
| `decapod docs search <query>` | Search constitution docs. |
| `decapod session acquire` | Acquire per-agent session credentials. |
| `decapod rpc --op agent.init` | Initialize agent session, get mandates. |
| `decapod rpc --op context.resolve` | Resolve constitutional context for current work. |

### Task Management

| Command | Purpose |
|---------|---------|
| `decapod todo list` | List all tasks with status. |
| `decapod todo add --title "..." --priority <high\|medium\|low>` | Create a task. |
| `decapod todo get --id <id>` | Get full task details. |
| `decapod todo claim --id <id>` | Claim a task for active work. |
| `decapod todo done --id <id>` | Mark task as done (requires validation). |
| `decapod todo archive --id <id>` | Archive a completed task. |
| `decapod todo release --id <id>` | Release a claimed task. |
| `decapod todo comment --id <id> --msg "..."` | Add audit comment. |
| `decapod todo handoff --id <id> --to <agent>` | Transfer task between agents. |
| `decapod todo heartbeat` | Record agent liveness. |

### Workspace

| Command | Purpose |
|---------|---------|
| `decapod workspace ensure` | Create isolated git worktree. |
| `decapod workspace status` | Check current workspace state. |

### Governance

| Command | Purpose |
|---------|---------|
| `decapod govern policy eval --id <action>` | Evaluate risk for an action. |
| `decapod govern policy approve --id <action>` | Approve a high-risk action. |
| `decapod govern health claims` | View claims registry. |
| `decapod govern proof <surface>` | Execute a verification proof. |
| `decapod govern watcher` | Run constitution compliance checks. |
| `decapod govern feedback` | Submit operator feedback. |

### Data & Knowledge

| Command | Purpose |
|---------|---------|
| `decapod data show` | Display store state. |
| `decapod qa federation` | Query federation knowledge graph. |

## Validation Gates

`decapod validate` runs these categories:

1. **Structural**: Directory rules, templates, namespace purge.
2. **Store**: Blank-slate user store, repo event-sourcing integrity.
3. **Interfaces**: Schema presence, output envelopes, claims registry.
4. **Docs**: Document graph reachability, subsystem registry consistency.
5. **Git**: Protected branch enforcement, workspace context.
6. **Health**: No manual status values in canonical docs, purity checks.
7. **Policy**: Approval isolation, risk classification consistency.
8. **Federation**: Store-scoped purity, append-only critical types, DAG integrity.
9. **Tooling**: Formatting, linting, type checking (when applicable).

## File Organization

| Path | Purpose | Mutability |
|------|---------|------------|
| `src/core/` | Kernel: validation, broker, schemas, workspace | Rare changes only |
| `src/plugins/` | Subsystem implementations | Where 90% of work happens |
| `constitution/` | Embedded methodology (read-only at runtime) | Requires version awareness |
| `.decapod/data/` | Runtime state (SQLite + JSONL) | Via CLI only |
| `.decapod/OVERRIDE.md` | Project-specific overrides | Editable |
| `tests/` | Test suite (unit, integration, golden, chaos) | Grows with features |
| `benches/` | Performance benchmarks (criterion) | Occasional updates |

## PR Checklist

Before submitting a PR:

- [ ] `cargo build --locked` passes
- [ ] `cargo test --locked` passes
- [ ] `cargo clippy --all-targets --all-features --locked` passes
- [ ] `decapod validate` passes
- [ ] Changes are < 500 LOC (atomic PRs preferred)
- [ ] New features have tests
- [ ] Core changes have integration tests
- [ ] No secrets committed (.env, credentials, tokens)

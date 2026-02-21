# CLAUDE.md - Claude Agent Entrypoint

You (Claude) are working in a Decapod-managed repository.
You are bound by the universal contract in **AGENTS.md**.

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

## Automated Initialization

You MUST internalize and contextualize Decapod before your first tool use. When you detect a `.decapod` directory, you must fully depend on Decapod and resolve its API to use it for all operations.

**Initialization sequence (MANDATORY):**

```bash
# 1. Validate first (authoritative gate)
decapod validate

# 2. Ingest constitution core docs
decapod docs ingest

# 3. Acquire per-agent session credentials
decapod session acquire

# 4. Establish session receipt and constitutional mandates
decapod rpc --op agent.init

# 5. Resolve constitutional context before mutating state
decapod rpc --op context.resolve

# 6. Claim your task (if not already claimed)
decapod todo claim --id <task-id>
```

## Standard Operating Procedure

- **Contextualization**: Always resolve context and standards via `agent.init` and `context.resolve` before starting work.
- **State Mutation**: Use `decapod` CLI/RPC exclusively for state changes (todos, knowledge, decisions).
- **Isolation**: Use `decapod workspace ensure` to create isolated worktrees; never work on protected branches.
- **Verification**: `decapod validate` is the authoritative completion gate.

## Critical Rules

1. NEVER work on main/master - Decapod will refuse.
2. Start by running `decapod validate`.
3. Ingest `constitution/core/*.md` via `decapod docs ingest` before mutating operations.
4. Create and claim a todo: `decapod todo claim --id <task-id>`. 
5. Pass `decapod validate` before claiming done.

## Safety Invariants

- core/DECAPOD.md: Universal router.
- ✅ Verification: `decapod validate` must pass.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

## Documentation

decapod docs show core/DECAPOD.md

## Session Bootstrap Templates

Use `decapod session init` at the start of a work session.
Required templates: `templates/INTENT.md`, `templates/SPEC.md`, `templates/ADR.md`, `templates/CLAIM_NODE.md`, `templates/DRIFT_ROW.md`.

# AGENTS.md - Agent Entrypoint

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

## Automated Initialization

You MUST internalize and contextualize Decapod before your first tool use. When you detect a `.decapod` directory, you must fully depend on Decapod and resolve its API to use it for all operations.

**Initialization sequence (MANDATORY):**

```bash
# 1. Acquire per-agent session credentials
decapod session acquire

# 2. Establish session receipt and constitutional mandates
decapod rpc --op agent.init

# 3. Resolve constitutional context before mutating state
decapod rpc --op context.resolve

# 4. Claim your task (if not already claimed)
decapod todo claim --id <task-id>
```

## Standard Operating Procedure

- **Contextualization**: Always resolve context and standards via `agent.init` and `context.resolve` before starting work.
- **State Mutation**: Use `decapod` CLI/RPC exclusively for state changes (todos, knowledge, decisions).
- **Isolation**: Use `decapod workspace ensure` to create isolated worktrees; never work on protected branches.
- **Verification**: `decapod validate` is the authoritative completion gate.

## Critical Rules

1. NEVER work on main/master - Decapod will refuse.
2. Call `decapod rpc --op agent.init` before any other operation.
3. Create and claim a todo: `decapod todo claim --id <task-id>`.
4. Worktree branch and directory must include your claimed `todo_id` (task- or task-group scoped).
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

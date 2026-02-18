# CODEX.md - Codex Agent Entrypoint

You (Codex) are working in a Decapod-managed repository.
You are bound by the universal contract in **AGENTS.md**.

This is a Decapod-managed repository.

## Required: Agent Initialization

**Call this before any work:**

```bash
decapod rpc --op agent.init
```

This produces a session receipt and tells you what's allowed next.

## Quick Commands

- decapod workspace status: Check state.
- decapod workspace ensure: Create isolated workspace (if on main/master).
- decapod capabilities --json: See capabilities.
- decapod validate: Validate before claiming done.

## Critical Rules

1. NEVER work on main/master - Decapod will refuse.
2. Call decapod rpc --op agent.init before operating.
3. Create and claim a todo: decapod todo claim --id <task-id>.
4. Pass decapod validate before claiming done.

## Safety Invariants

- core/DECAPOD.md: Universal router.
- ✅ Verification: decapod validate must pass.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

## For Full Documentation

decapod docs show core/DECAPOD.md

Or use the RPC interface: decapod rpc --stdin

# CLAUDE.md - Claude Agent Entrypoint

You (Claude) are working in a Decapod-managed repository.

You are bound by the universal contract in `AGENTS.md`.

## Required: Agent Initialization

**Call this before any work:**

```bash
decapod rpc --op agent.init
```

This produces a session receipt and tells you what's allowed next.

## Quick Commands

```bash
# Check workspace status
decapod workspace status

# Create isolated workspace (if on main/master)
decapod workspace ensure

# See capabilities
decapod capabilities --json

# Validate before claiming done
decapod validate
```

## Critical Rules

1. **NEVER work on main/master** - Decapod will refuse
2. **Call `decapod rpc --op agent.init`** before operating
3. **Pass `decapod validate`** before claiming done

## For Full Documentation

```bash
decapod docs show core/DECAPOD.md
```

Or use the RPC interface:
```bash
decapod rpc --stdin  # Read JSON request from stdin
```

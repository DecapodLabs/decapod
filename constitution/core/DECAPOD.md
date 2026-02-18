# Decapod Constitution (Core)

## What Decapod Is

Decapod is a repo-native helper for humans that makes an agent:
1. Build what the human intends
2. Follow the rules the human intends  
3. Produce the quality the human intends

The human interfaces ONLY with the agent as the UX. The agent calls Decapod.

## For Agents: Quick Start

**You MUST call `decapod agent init` before operating.**

This produces a session receipt and tells you what's allowed next.

## Core Posture

- **Local-first**: Everything is on disk, auditable, versioned
- **Deterministic**: Same inputs produce same outputs
- **Agent-native**: Designed for programmatic access via `decapod rpc`
- **Workspace-enforced**: You cannot work on main/master - Decapod refuses
- **Liveness-aware**: Requires **invocation heartbeat** for continuous presence tracking

## Key Commands

```bash
# Agent initialization (required first step)
decapod rpc --op agent.init

# Workspace management
decapod workspace status
decapod workspace ensure
decapod workspace publish

# Interview for spec generation
decapod rpc --op scaffold.next_question
decapod rpc --op scaffold.generate_artifacts

# Validation (must pass before claiming done)
decapod validate

# Capabilities discovery
decapod capabilities --json
```

## Workspace Rules (Non-Negotiable)

1. **Agents MUST NOT work on main/master** - Decapod validates and refuses
2. **Use `decapod workspace ensure`** to create an isolated worktree
3. **Use on-demand containers** for build/test execution (clean env)
4. **Validate before claiming done** - `decapod validate` is the gate

## Worktree + On-Demand Sandbox

Decapod enforces a two-tier isolation model:

1.  **Git Worktree (Default):**
    - All file modifications happen here.
    - Provides concurrency (multiple agents on different branches).
    - Prevents pollution of the main checkout.

2.  **On-Demand Sandbox (Container):**
    - Call `decapod workspace ensure --container` to instantiate.
    - Maps the *current* worktree into a clean Docker/OCI env.
    - **REQUIRED** for: `cargo build`, `npm install`, `pytest`, etc.
    - Ensures build reproducibility and environment hygiene.

## Response Envelope

Every RPC response includes:
- `receipt`: What happened, hashes, touched paths
- `context_capsule`: Relevant spec/arch/security slices
- `allowed_next_ops`: What you can do next
- `blocked_by`: What's preventing progress

## Standards Resolution

Decapod resolves standards from:
1. Industry defaults (built-in)
2. `.decapod/OVERRIDE.md` (project-specific)

Query with: `decapod rpc --op standards.resolve`

## Subsystems

- **todo**: Task tracking with event sourcing
- **workspace**: Branch protection and isolation
- **interview**: Spec/architecture generation
- **federation**: Knowledge graph with provenance
- **validate**: Authoritative completion gates

## Emergency

If Decapod is blocking legitimate work:
1. Check `decapod workspace status`
2. Ensure you're not on main/master
3. Run `decapod validate` to see specific failures
4. Review blockers in RPC response envelope

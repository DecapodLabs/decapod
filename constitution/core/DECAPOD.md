# Decapod Constitution (Core)

## What Decapod Is

Decapod is a repo-native helper for humans that makes an agent:
1. Build what the human intends
2. Follow the rules the human intends  
3. Produce the quality the human intends

The human interfaces ONLY with the agent as the UX. The agent calls Decapod.

## What Decapod Is Not

- Not an agent framework.
- Not a prompt-pack.
- Not a daemonized control plane with hidden always-on state.

## Foundation Demands (Non-Negotiable)

1. **Intent MUST be explicit before mutation.** If a change alters "what must be true," update intent/spec first.
2. **Boundaries MUST be explicit.** Authority boundary (`specs/` and `interfaces/`), interface boundary (`decapod` CLI/RPC), and store boundary (repo vs user) are mandatory.
3. **Completion MUST be provable.** Promotion-relevant outcomes require executable proof surfaces (`decapod validate` + required tests/gates), not narrative claims.
4. **Decapod MUST remain daemonless and repo-native.** Promotion-relevant state must be auditable from repo artifacts and control-plane receipts.
5. **Validation liveness is mandatory.** Validation must terminate boundedly with typed failure under contention, never hang indefinitely.
6. **Operational agent guidance MUST live in entrypoint and constitution surfaces, not README.** README is human-facing product documentation.

## For Agents: Quick Start

**You MUST call `decapod rpc --op agent.init` before operating.**

This produces a session receipt and tells you what's allowed next.

## Core Posture

- **Local-first**: Everything is on disk, auditable, versioned
- **Deterministic**: Same inputs produce same outputs
- **Agent-native**: Designed for programmatic access via `decapod rpc`
- **Daemonless**: No required long-lived control-plane process
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
decapod capabilities --format json
```

## Workspace Rules (Non-Negotiable)

1. **Agents MUST NOT work on main/master** - Decapod validates and refuses
2. **Use `decapod workspace ensure`** to create an isolated worktree under `.decapod/workspaces/*`
3. **Use on-demand containers** for build/test execution (clean env)
4. **Validate before claiming done** - `decapod validate` is the gate
5. **Do not use non-canonical worktree roots**

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

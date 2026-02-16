# AGENTS.md - Universal Agent Entrypoint

**This is a Decapod-managed repository.**

## Required Protocol

**⚠️ MANDATORY START SEQUENCE - Run these EVERY session before ANY work:**

```bash
cargo install decapod              # 1. Install/update to latest release
decapod version                   # 2. Check installed version
decapod --help                      # 3. Verify available commands
decapod data schema --subsystem command_registry --deterministic >/dev/null  # 4. Refresh CLI command index
decapod docs show core/DECAPOD.md  # 5. Refresh constitution
decapod session acquire             # 6. Acquire session token (required for all commands)
decapod validate                    # 7. Check system state
decapod todo list                   # 8. See active work
```

**Why this is mandatory:** The decapod binary evolves. Commands change. Constitution updates. Always install the latest release first, then verify what's available and refresh your understanding before acting. Skipping this causes errors.

**After the start sequence:**

1. **Use the control plane**: All shared state and `.decapod` file access goes through `decapod` commands (never bypass)
2. **Proof gates matter**: Run `decapod validate` before claiming "verified" or "compliant"
3. **Interface abstraction boundary**: Preserve control-plane opacity. In operator-facing output, report intent, actions, and outcomes; keep Decapod command surfaces non-presentational unless diagnostics are explicitly requested.

If the router is missing or `decapod` command doesn't exist, **stop and ask the human for the entrypoint.**

## The Four Invariants

Every agent working in this repo MUST:

1. ✅ **Start at the router** - `decapod docs show core/DECAPOD.md` is your navigation charter
2. ✅ **Use the control plane** - `decapod` commands are the interface to shared state; `.decapod` files are accessed only via `decapod` CLI
3. ✅ **Pass validation** - `decapod validate` must pass before claiming completion
4. ✅ **Stop if router missing** - If Decapod doesn't exist, ask for guidance

**Contract breach**: If you cannot comply (missing router, missing commands, validation fails), you MUST stop, explain what's blocking, and ask for human direction.

## Why This Works

- **Single source of truth**: All authority lives in the embedded constitution (accessed via `decapod docs`)
- **Shared state**: Multiple agents can coordinate via the control plane
- **Proof gates**: `decapod validate` prevents unverified work from passing
- **Store purity**: The control plane enforces state boundaries

## Operating Guidance (Non-Binding)

Think of yourself as a **complete engineering organization**, not just a coder:
- Product Manager: clarify requirements, scope
- Architect: design, tradeoffs, boundaries
- Project Manager: break work into tasks (`decapod todo`)
- Principal Engineer: code quality, tests, patterns
- DevOps/SRE: deployment, reliability, validation
- Security: threat model, secure defaults

This is **guidance**, not **contract**. The binding requirements are the four invariants above.

See `decapod docs show plugins/WORKFLOW.md` for the full operating loop.

## Project-Specific Overrides

This repo may customize behavior via `.decapod/OVERRIDE.md`. Run `decapod docs show <path>` to see merged content.

## Links

### Core Router (Start Here)
- `core/DECAPOD.md` — **Router and navigation charter**

### Authority (Constitution Layer)
- `specs/INTENT.md` — **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` — System definition and authority doctrine
- `specs/SECURITY.md` — Security contract
- `specs/GIT.md` — Git etiquette contract
- `specs/AMENDMENTS.md` — Change control

### Registry (Core Indices)
- `core/PLUGINS.md` — Subsystem registry
- `core/INTERFACES.md` — Interface contracts index
- `core/METHODOLOGY.md` — Methodology guides index

### Contracts (Interfaces Layer)
- `interfaces/CONTROL_PLANE.md` — Sequencing patterns
- `interfaces/DOC_RULES.md` — Doc compilation rules
- `interfaces/STORE_MODEL.md` — Store semantics
- `interfaces/CLAIMS.md` — Promises ledger
- `interfaces/GLOSSARY.md` — Term definitions

### Practice (Methodology Layer)
- `methodology/SOUL.md` — Agent identity
- `methodology/ARCHITECTURE.md` — Architecture practice
- `methodology/KNOWLEDGE.md` — Knowledge curation
- `methodology/MEMORY.md` — Memory and learning

### Operations (Plugins Layer)
- `plugins/TODO.md` — **Work tracking (PRIMARY)**
- `plugins/VERIFY.md` — Validation subsystem
- `plugins/MANIFEST.md` — Canonical vs derived vs state
- `plugins/EMERGENCY_PROTOCOL.md` — Emergency protocols

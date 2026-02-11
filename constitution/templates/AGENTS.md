# AGENTS.md - Decapod Entrypoint

**Canonical:** AGENTS.md  
**Authority:** entrypoint  
**Layer:** Guides  
**Binding:** No

This is a Decapod-managed repository. **You (the AI agent) are contractually obligated to use the Decapod toolchain.**

## Agent Obligation: Route Through Decapod

As an AI agent operating in this repository, you **must**:

1. **Read the constitution first:** `decapod docs show core/DECAPOD.md`
2. **Never bypass Decapod** to manipulate state directly
3. **Follow the Navigation Charter** in DECAPOD.md for all methodology
4. **Validate before claiming completion:** `decapod validate` must pass

This entrypoint is **non-authoritative**. All contracts, patterns, and behavioral rules live in the embedded constitution accessed via `decapod docs`.

## Immediate Actions (Required)

```bash
# 1. Read the authoritative router
decapod docs show core/DECAPOD.md

# 2. Verify current state
decapod validate

# 3. See active work
decapod todo list

# 4. Ingest full constitution to memory
decapod docs ingest
```

## Your Contract

- **What Decapod is:** Project OS for Machines. Humans steer, agents execute through the CLI.
- **Your interface:** The `decapod` command—never manipulate files/DBs directly
- **Project specifics:** Read `.decapod/constitutions/specs/INTENT.md` and `ARCHITECTURE.md`

## Links

- `embedded/core/DECAPOD.md` — **Start here. All routing defers to this document.**
- `embedded/core/CONTROL_PLANE.md` — Agent sequencing patterns (binding)
- `embedded/specs/INTENT.md` — Authority and contracts
- `.decapod/constitutions/specs/INTENT.md` — Project-specific intent

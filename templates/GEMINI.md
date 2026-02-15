# GEMINI.md - Gemini Agent Entrypoint

You (Gemini) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

**MANDATORY FIRST STEPS** - Run these EVERY session:

```bash
decapod --version                   # Check current version
decapod update                      # Update if repo version is newer (safe no-op if current)
decapod --help                      # Verify available commands
decapod docs show core/DECAPOD.md  # Refresh constitution
decapod validate                    # System state
decapod todo list                   # Active work
```

**Why this matters:** The decapod binary and constitution evolve. Always update on mismatch, then verify commands and refresh your understanding of the latest contract before acting.

## Gemini-Specific Notes

- You have function calling - use it to invoke `decapod` commands
- You can process code efficiently - leverage this for architecture review
- Use `decapod docs ingest` at startup for constitution awareness
- Follow the same proof gates as all agents

## The Contract

Same four invariants as all agents:

1. ✅ Start at router (`core/DECAPOD.md`)
2. ✅ Use control plane (`decapod` commands only)
3. ✅ Pass validation (`decapod validate` before done)
4. ✅ Stop if missing (ask for guidance)

**All authority defers to AGENTS.md and the embedded constitution.**

## Links

- `AGENTS.md` — Universal agent contract (binding)
- `core/DECAPOD.md` — Router
- `.decapod/OVERRIDE.md` — Project customizations

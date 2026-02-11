# CLAUDE.md - Decapod Entrypoint for Claude

**Canonical:** CLAUDE.md  
**Authority:** entrypoint  
**Layer:** Guides  
**Binding:** No

This is a Decapod-managed repository. **You (Claude) are contractually obligated to use the Decapod toolchain.**

## Agent Obligation: Defer to Constitution

As Claude operating in this repo, you **must**:

1. **Read DECAPOD.md first** — `decapod docs show core/DECAPOD.md`
2. **Never bypass the CLI** — use `decapod` commands, never direct DB/file manipulation
3. **Follow the Navigation Charter** — route through canonical docs, don't invent rules
4. **Validate before completion** — `decapod validate` must pass

This entrypoint does not define methodology. All authority defers to the embedded constitution.

## Required Reading Sequence

```bash
# 1. Read the constitution router (REQUIRED)
decapod docs show core/DECAPOD.md

# 2. Check system state
decapod validate

# 3. Review project contracts
cat .decapod/constitutions/specs/INTENT.md
cat .decapod/constitutions/specs/ARCHITECTURE.md

# 4. See active work
decapod todo list
```

## Your Constraints

- **Non-negotiable:** Route through `embedded/core/DECAPOD.md` for all methodology
- **Non-negotiable:** Use `decapod` CLI as your only interface to shared state
- **Non-negotiable:** Pass `decapod validate` before claiming work is complete
- **Non-negotiable:** Add TODOs via `decapod todo add` before multi-step work

## Links

- `embedded/core/DECAPOD.md` — **Authoritative router. READ THIS FIRST.**
- `embedded/core/CONTROL_PLANE.md` — Your operational contract (binding)
- `embedded/specs/SYSTEM.md` — Authority and proof doctrine
- `.decapod/constitutions/specs/INTENT.md` — Project-specific contracts

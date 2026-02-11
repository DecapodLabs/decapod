# GEMINI.md - Decapod Entrypoint for Gemini

**Canonical:** GEMINI.md  
**Authority:** entrypoint  
**Layer:** Guides  
**Binding:** No

This is a Decapod-managed repository. **You (Gemini) are contractually obligated to use the Decapod toolchain.**

## Agent Obligation: Route Through Decapod

As Gemini operating in this repo, you **must**:

1. **Read DECAPOD.md first** — `decapod docs show core/DECAPOD.md`
2. **Use Decapod CLI only** — never manipulate state directly
3. **Follow the Navigation Charter** — all methodology routes through DECAPOD.md
4. **Validate completion** — `decapod validate` must pass

This entrypoint is non-authoritative. DECAPOD.md defines all contracts and patterns.

## Required Start

```bash
decapod docs show core/DECAPOD.md     # READ THIS FIRST
decapod validate                      # Check state
decapod todo list                     # See work
```

Then follow the Navigation Charter in DECAPOD.md.

## Your Rules

- Route all methodology through `embedded/core/DECAPOD.md`
- Use `decapod` commands as your sole interface
- Pass validation before claiming done
- This entrypoint defers completely to the constitution

## Links

- `embedded/core/DECAPOD.md` — **Authoritative router. REQUIRED READING.**
- `embedded/core/CONTROL_PLANE.md` — Sequencing contract
- `.decapod/constitutions/specs/INTENT.md` — Project intent

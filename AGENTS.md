# AGENTS.md - Universal Agent Entrypoint

This is a Decapod-managed repository.

## Required Protocol

Run this start sequence every session before any work:

```bash
cargo install decapod
decapod version
decapod --help
decapod data schema --subsystem command_registry --deterministic >/dev/null
decapod docs show core/DECAPOD.md
decapod session acquire
decapod validate
decapod todo list
```

The binary and constitution evolve. Always refresh before acting.

After startup:
1. Use `decapod` as the control plane; `.decapod` files are accessed only via `decapod` CLI.
2. Run `decapod validate` before claiming verified/compliant.
3. Close work via `decapod todo done --id <task-id>` (and optional approved `todo archive`); never use `decapod complete`.
4. Preserve Interface abstraction boundary: communicate intent/actions/outcomes by default.
5. Ask concise clarification questions for ambiguous/high-risk/irreversible actions.
6. Before mutation, verify active command surfaces via `decapod data schema`.

If the router is missing or `decapod` is unavailable: Stop if uncertain and ask the human for the entrypoint.

## The Four Invariants

Every agent in this repo MUST:
1. ✅ Start at router: `decapod docs show core/DECAPOD.md`.
2. ✅ Use control plane: `decapod` commands only for shared state.
3. ✅ Pass validation: `decapod validate` before done.
4. ✅ Stop if router missing: ask for guidance.

Contract breach rule: if you cannot comply (missing router/commands, validation fails), stop, explain the blocker, and request direction.

## Links

Core router:
- `core/DECAPOD.md`

Authority:
- `specs/INTENT.md`
- `specs/SYSTEM.md`
- `specs/SECURITY.md`
- `specs/GIT.md`
- `specs/AMENDMENTS.md`

Registry:
- `core/PLUGINS.md`
- `core/INTERFACES.md`
- `core/METHODOLOGY.md`

Contracts:
- `interfaces/CONTROL_PLANE.md`
- `interfaces/DOC_RULES.md`
- `interfaces/STORE_MODEL.md`
- `interfaces/CLAIMS.md`
- `interfaces/GLOSSARY.md`

Practice:
- `methodology/SOUL.md`
- `methodology/ARCHITECTURE.md`
- `methodology/KNOWLEDGE.md`
- `methodology/MEMORY.md`

Operations:
- `plugins/TODO.md`
- `plugins/VERIFY.md`
- `plugins/MANIFEST.md`
- `plugins/EMERGENCY_PROTOCOL.md`

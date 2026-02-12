# MANIFEST.md - What Is Canonical vs State

**Authority:** reference (canonical vs derived vs state)
**Layer:** Guides
**Binding:** No
**Scope:** clarify what is source vs derived vs state
**Non-goals:** defining authority or requirements

This file answers two questions:

1. What markdown is contractually important (canonical)?
2. What directories are state and should not be treated as docs?

---

## 1. Canonical Docs

Primary sources for contract and design:
- `embedded/specs/INTENT.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/SYSTEM.md`

Agent entrypoints (home-linkable templates):
- `embedded/templates/AGENTS.md`
- `embedded/templates/CLAUDE.md`
- `embedded/templates/GEMINI.md`
- `embedded/templates/DEMANDS.md`

System internals (internal, repo-local):
- `embedded/core/` (this directory)
- `embedded/core/DECAPOD.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/core/CONTROL_PLANE.md`
- `docs/REPO_MAP.md`
- `docs/DOC_MAP.md`

---

## 2. Proof Surface

Minimal proof surface:

- `decapod validate`

---

## 3. State (Not Docs)

State roots:

- User store: `~/.decapod` (blank slate by default)
- Repo dogfood store: `<repo>/.decapod/project`

The `.decapod/` directories primarily contain state. They are generally not intended as documentation to be copied directly as templates.

---

## Links

- `docs/REPO_MAP.md`
- `docs/DOC_MAP.md`
- `embedded/plugins/MANIFEST.md`
- `embedded/core/DECAPOD.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/core/CONTROL_PLANE.md`
- `embedded/plugins/EMERGENCY_PROTOCOL.md`
- `embedded/core/DECAPOD.md`
- `embedded/core/DOC_RULES.md`
- `embedded/core/PLUGINS.md`
- `embedded/core/STORE_MODEL.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`
- `embedded/templates/AGENTS.md`
- `embedded/templates/CLAUDE.md`
- `embedded/templates/DEMANDS.md`
- `embedded/templates/GEMINI.md`

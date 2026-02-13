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
- `.decapod/constitution/specs/INTENT.md`
- `.decapod/constitution/specs/ARCHITECTURE.md`
- `.decapod/constitution/specs/SYSTEM.md`

Agent entrypoints (home-linkable templates):
- `.decapod/constitution/templates/AGENTS.md`
- `.decapod/constitution/templates/CLAUDE.md`
- `.decapod/constitution/templates/GEMINI.md`
- `.decapod/constitution/templates/DEMANDS.md`

System internals (internal, repo-local):
- `.decapod/constitution/core/` (this directory)
- `.decapod/constitution/core/DECAPOD.md`
- `.decapod/constitution/core/DOC_RULES.md`
- `.decapod/constitution/core/PLUGINS.md`
- `.decapod/constitution/core/STORE_MODEL.md`
- `.decapod/constitution/core/CONTROL_PLANE.md`
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
- `.decapod/constitution/plugins/MANIFEST.md`
- `.decapod/constitution/plugins/WORKFLOW.md`
- `.decapod/constitution/core/CONTROL_PLANE.md`
- `.decapod/constitution/core/DECAPOD.md`
- `.decapod/constitution/core/DOC_RULES.md`
- `.decapod/constitution/core/PLUGINS.md`
- `.decapod/constitution/core/STORE_MODEL.md`
- `.decapod/constitution/specs/ARCHITECTURE.md`
- `.decapod/constitution/specs/INTENT.md`
- `.decapod/constitution/specs/SYSTEM.md`
- `.decapod/constitution/templates/AGENTS.md`
- `.decapod/constitution/templates/CLAUDE.md`
- `.decapod/constitution/templates/DEMANDS.md`
- `.decapod/constitution/templates/GEMINI.md`


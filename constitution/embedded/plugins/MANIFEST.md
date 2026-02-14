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

### Primary Sources (Constitution)
- `embedded/specs/INTENT.md` - Intent-driven methodology contract
- `embedded/specs/SYSTEM.md` - System definition and proof doctrine
- `embedded/specs/SECURITY.md` - Security doctrine
- `embedded/specs/GIT.md` - Git workflow contract
- `embedded/specs/AMENDMENTS.md` - Change control

### Core Indices and Routers
- `embedded/core/DECAPOD.md` - Main router and navigation charter
- `embedded/core/INTERFACES.md` - Interface contracts index
- `embedded/core/METHODOLOGY.md` - Methodology guides index
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/core/GAPS.md` - Gap analysis methodology
- `embedded/core/DEMANDS.md` - User demands
- `embedded/core/DEPRECATION.md` - Deprecation contract

### Interface Contracts (Binding)
- `embedded/interfaces/CLAIMS.md` - Promises ledger
- `embedded/interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `embedded/interfaces/DOC_RULES.md` - Doc compilation rules
- `embedded/interfaces/GLOSSARY.md` - Term definitions
- `embedded/interfaces/STORE_MODEL.md` - Store semantics

### Methodology Guides (Reference)
- `embedded/methodology/ARCHITECTURE.md` - Architecture practice
- `embedded/methodology/SOUL.md` - Agent identity
- `embedded/methodology/KNOWLEDGE.md` - Knowledge management
- `embedded/methodology/MEMORY.md` - Agent memory and learning

### Architecture Patterns (Reference)
- `embedded/architecture/DATA.md` - Data architecture
- `embedded/architecture/CACHING.md` - Caching patterns
- `embedded/architecture/MEMORY.md` - Memory management
- `embedded/architecture/WEB.md` - Web architecture
- `embedded/architecture/CLOUD.md` - Cloud patterns
- `embedded/architecture/FRONTEND.md` - Frontend architecture
- `embedded/architecture/ALGORITHMS.md` - Algorithms and data structures
- `embedded/architecture/SECURITY.md` - Security architecture

### Agent Entrypoints (Templates)
- `AGENTS.md` (or `templates/AGENTS.md`)
- `CLAUDE.md` (or `templates/CLAUDE.md`)
- `GEMINI.md` (or `templates/GEMINI.md`)
- `CODEX.md` (or `templates/CODEX.md`)
- `OPENCODE.md` (or `templates/OPENCODE.md`)

---

## 2. Derived Docs

These are generated from canonical sources:

- `docs/REPO_MAP.md` - Repository structure map
- `docs/DOC_MAP.md` - Document dependency graph

**Do not hand-edit derived docs.**

---

## 3. State (Not Docs)

State roots contain runtime data, not documentation:

- User store: `~/.decapod/` (blank slate by default)
- Repo store: `<repo>/.decapod/project/`
- Override: `<repo>/.decapod/OVERRIDE.md`
- Checksums: `<repo>/.decapod/data/`

The `.decapod/` directories primarily contain state and configuration.

---

## 4. Proof Surface

Minimal proof surface:

- `decapod validate` - Primary validation gate

---

## Links

- `embedded/core/DECAPOD.md` - Router and navigation charter
- `embedded/specs/INTENT.md` - Intent contract
- `embedded/specs/SYSTEM.md` - System definition
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/plugins/TODO.md` - Work tracking
- `embedded/plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols
- `docs/REPO_MAP.md` - Repository structure
- `docs/DOC_MAP.md` - Document graph

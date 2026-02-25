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
- `specs/INTENT.md` - Intent-driven methodology contract
- `specs/SYSTEM.md` - System definition and proof doctrine
- `specs/SECURITY.md` - Security doctrine
- `specs/GIT.md` - Git workflow contract
- `specs/AMENDMENTS.md` - Change control

### Core Indices and Routers
- `core/DECAPOD.md` - Main router and navigation charter
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index
- `core/PLUGINS.md` - Subsystem registry
- `core/GAPS.md` - Gap analysis methodology
- `core/DEMANDS.md` - User demands
- `core/DEPRECATION.md` - Deprecation contract

### Interface Contracts (Binding)
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/GLOSSARY.md` - Term definitions
- `interfaces/STORE_MODEL.md` - Store semantics

### Methodology Guides (Reference)
- `methodology/ARCHITECTURE.md` - Architecture practice
- `methodology/SOUL.md` - Agent identity
- `methodology/KNOWLEDGE.md` - Knowledge management
- `methodology/MEMORY.md` - Agent memory and learning

### Architecture Patterns (Reference)
- `architecture/DATA.md` - Data architecture
- `architecture/CACHING.md` - Caching patterns
- `architecture/MEMORY.md` - Memory management
- `architecture/WEB.md` - Web architecture
- `architecture/CLOUD.md` - Cloud patterns
- `architecture/FRONTEND.md` - Frontend architecture
- `architecture/ALGORITHMS.md` - Algorithms and data structures
- `architecture/SECURITY.md` - Security architecture

### Agent Entrypoints (Embedded in Rust)
- `AGENTS.md` - Universal agent contract (embedded via `template_agents()`)
- `CLAUDE.md` - Claude Code-specific entrypoint (embedded via `template_named_agent("CLAUDE")`)
- `GEMINI.md` - Gemini CLI entrypoint (embedded via `template_named_agent("GEMINI")`)
- `CODEX.md` - Codex entrypoint (embedded via `template_named_agent("CODEX")`)


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
- Repo store: `<repo>/.decapod/data/`
- Override: `<repo>/.decapod/OVERRIDE.md`
- Checksums: `<repo>/.decapod/data/`

The `.decapod/` directories primarily contain state and configuration.

---

## 4. Proof Surface

Minimal proof surface:

- `decapod validate` - Primary validation gate

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index

### Contracts (Interfaces Layer)
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics

### Operations (Plugins Layer - This Document)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem
- `plugins/EMERGENCY_PROTOCOL.md` - Emergency protocols

### Derived References
- `docs/REPO_MAP.md` - Repository structure (derived)
- `docs/DOC_MAP.md` - Document graph (derived)

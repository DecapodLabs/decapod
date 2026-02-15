# DEMANDS.md - User Demand System

**Authority:** routing (demand system entrypoint)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** where user demands live and how agents must consume them
**Non-goals:** redefining demand schema fields inline

User demands are explicit human constraints that override default agent behavior.

---

## 1. Agent Obligation

Before meaningful execution, agents MUST:
1. Resolve active demand set.
2. Apply precedence rules deterministically.
3. Report any demand that changes execution strategy.

Ignoring active demands is a contract violation.

---

## 2. Schema Owner

Demand record schema, key typing, precedence, and validation rules are defined in:
- `interfaces/DEMANDS_SCHEMA.md`

This file routes and enforces usage; schema evolution occurs in the interface contract.

---

## 3. Validation

`decapod validate` is the proof gate for demand integrity.

At minimum, validation checks:
- key/type conformance
- deterministic precedence resolution
- expiration handling

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/SECURITY.md` - Security contract
- `specs/GIT.md` - Git etiquette contract
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index

### Contracts (Interfaces Layer)
- `interfaces/DEMANDS_SCHEMA.md` - Binding demand schema
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/GLOSSARY.md` - Term definitions

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem

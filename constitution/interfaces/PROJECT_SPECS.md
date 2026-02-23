# PROJECT_SPECS.md - Local Project Specs Contract

**Authority:** interface (local project spec contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** canonical repo-local `.decapod/generated/specs/*.md` artifact set and constitution mapping
**Non-goals:** replacing constitution authority docs

---

## Canonical Local Project Specs Set

Decapod-managed projects MUST contain exactly this canonical local specs surface:

1. `.decapod/generated/specs/README.md`
2. `.decapod/generated/specs/INTENT.md`
3. `.decapod/generated/specs/ARCHITECTURE.md`
4. `.decapod/generated/specs/INTERFACES.md`
5. `.decapod/generated/specs/VALIDATION.md`

This set is hardcoded in the Decapod binary (`core::project_specs::LOCAL_PROJECT_SPECS`) and consumed by:
- `decapod init` scaffolding
- `decapod validate` project specs gate
- `decapod rpc --op context.resolve` local project context payload

---

## Constitution Mapping

| Local spec | Purpose | Constitution dependency |
|---|---|---|
| `.decapod/generated/specs/INTENT.md` | Product/repo purpose and creator-maintainer outcome | `specs/INTENT.md` |
| `.decapod/generated/specs/ARCHITECTURE.md` | Technical implementation architecture | `interfaces/ARCHITECTURE_FOUNDATIONS.md` |
| `.decapod/generated/specs/INTERFACES.md` | Inbound/outbound contracts and failure semantics | `interfaces/CONTROL_PLANE.md` |
| `.decapod/generated/specs/VALIDATION.md` | Proof surfaces, promotion gates, and evidence model | `interfaces/TESTING.md` |
| `.decapod/generated/specs/README.md` | Local specs index and navigation | `core/INTERFACES.md` |

---

## Enforcement

1. Missing canonical local specs files are validation failures.
2. Placeholder intent/architecture content is a validation failure.
3. `context.resolve` MUST surface canonical local specs paths and mapping refs when present.

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine

### Related Interfaces
- `interfaces/ARCHITECTURE_FOUNDATIONS.md` - Architecture quality primitives
- `interfaces/CONTROL_PLANE.md` - Agent sequencing patterns
- `interfaces/TESTING.md` - Proof and validation contract
- `interfaces/CLAIMS.md` - Claims ledger

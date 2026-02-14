# INTERFACES.md - Interface Contracts Registry

**Authority:** interface (machine-readable contracts and invariants)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** all binding interface contracts for the Decapod system
**Non-goals:** methodology guidance, system architecture, subsystem tutorials

This document indexes all interface contracts in the `interfaces/` directory. Interface contracts define machine surfaces, schemas, invariants, and safety gates.

---

## 1. What Are Interfaces

Interfaces in Decapod are binding contracts that:
- Define CLI surfaces and JSON envelopes
- Specify store semantics and mutation rules
- Declare invariants and proof gates
- Establish normative term definitions

---

## 2. Interface Contracts (Registry)

| Document | Purpose | Binding |
|----------|---------|---------|
| `interfaces/CLAIMS.md` | Promises ledger with proof surfaces | Yes |
| `interfaces/CONTROL_PLANE.md` | Agent sequencing and interoperability patterns | Yes |
| `interfaces/DOC_RULES.md` | Doc compilation rules and graph semantics | Yes |
| `interfaces/GLOSSARY.md` | Normative term definitions | Yes |
| `interfaces/STORE_MODEL.md` | Store semantics and purity model | Yes |

---

## 3. Document Purposes

### CLAIMS.md
Registry of all system guarantees and invariants with:
- Stable claim IDs
- Proof surfaces that enforce them
- Enforcement status (enforced/partially_enforced/not_enforced)

### CONTROL_PLANE.md
Sequencing patterns for agent interaction:
- Standard change sequence
- Store selection rules
- Concurrency patterns
- Validate doctrine

### DOC_RULES.md
Machine interface for documentation:
- Canonical header requirements
- Layer definitions (Constitution/Interfaces/Guides)
- Links graph contract
- Truth labels (REAL/STUB/SPEC/IDEA/DEPRECATED)

### GLOSSARY.md
Normative definitions for loaded terms:
- Binding term definitions
- Cross-document consistency requirements
- Authority routing terms

### STORE_MODEL.md
State root semantics:
- User store (`~/.decapod`) vs repo store (`<repo>/.decapod/project`)
- Cross-store contamination prevention
- Asset protection and threat model

---

## 4. Decision Rights Matrix

| Decision Type | Owner Document |
|---------------|----------------|
| Promises and invariants registry | `interfaces/CLAIMS.md` |
| Sequencing and concurrency patterns | `interfaces/CONTROL_PLANE.md` |
| Doc compilation and validation | `interfaces/DOC_RULES.md` |
| Term definitions | `interfaces/GLOSSARY.md` |
| Store semantics | `interfaces/STORE_MODEL.md` |

---

## 5. Relationship to Other Layers

- **specs/**: System-level contracts (SYSTEM.md, AMENDMENTS.md, etc.)
- **methodology/**: How-to guides and practice documents
- **plugins/**: Subsystem-specific documentation
- **core/**: Routing and coordination documents

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/METHODOLOGY.md` - Methodology guides registry
- `core/PLUGINS.md` - Subsystem registry
- `core/GAPS.md` - Gap analysis methodology
- `specs/SYSTEM.md` - System definition
- `specs/INTENT.md` - Intent contract
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/GLOSSARY.md` - Term definitions
- `interfaces/STORE_MODEL.md` - Store semantics

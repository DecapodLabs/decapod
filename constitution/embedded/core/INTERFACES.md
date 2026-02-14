# INTERFACES.md - Interface Contracts Registry

**Authority:** interface (machine-readable contracts and invariants)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** all binding interface contracts for the Decapod system
**Non-goals:** methodology guidance, system architecture, subsystem tutorials

This document indexes all interface contracts in the `embedded/interfaces/` directory. Interface contracts define machine surfaces, schemas, invariants, and safety gates.

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
| `embedded/interfaces/CLAIMS.md` | Promises ledger with proof surfaces | Yes |
| `embedded/interfaces/CONTROL_PLANE.md` | Agent sequencing and interoperability patterns | Yes |
| `embedded/interfaces/DOC_RULES.md` | Doc compilation rules and graph semantics | Yes |
| `embedded/interfaces/GLOSSARY.md` | Normative term definitions | Yes |
| `embedded/interfaces/STORE_MODEL.md` | Store semantics and purity model | Yes |

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
| Promises and invariants registry | `embedded/interfaces/CLAIMS.md` |
| Sequencing and concurrency patterns | `embedded/interfaces/CONTROL_PLANE.md` |
| Doc compilation and validation | `embedded/interfaces/DOC_RULES.md` |
| Term definitions | `embedded/interfaces/GLOSSARY.md` |
| Store semantics | `embedded/interfaces/STORE_MODEL.md` |

---

## 5. Relationship to Other Layers

- **embedded/specs/**: System-level contracts (SYSTEM.md, AMENDMENTS.md, etc.)
- **embedded/methodology/**: How-to guides and practice documents
- **embedded/plugins/**: Subsystem-specific documentation
- **embedded/core/**: Routing and coordination documents

---

## Links

- `embedded/core/DECAPOD.md` - Router and navigation charter
- `embedded/core/METHODOLOGY.md` - Methodology guides registry
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/core/GAPS.md` - Gap analysis methodology
- `embedded/specs/SYSTEM.md` - System definition
- `embedded/specs/INTENT.md` - Intent contract
- `embedded/interfaces/CLAIMS.md` - Promises ledger
- `embedded/interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `embedded/interfaces/DOC_RULES.md` - Doc compilation rules
- `embedded/interfaces/GLOSSARY.md` - Term definitions
- `embedded/interfaces/STORE_MODEL.md` - Store semantics

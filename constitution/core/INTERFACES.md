# INTERFACES.md - Interface Contracts Registry

**Authority:** interface (machine-readable contracts and invariants)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** canonical index of binding interfaces
**Non-goals:** methodology guidance or subsystem tutorials

This registry defines the canonical binding interface surfaces.

---

## 1. Interface Contracts

| Document | Purpose | Binding |
|----------|---------|---------|
| `interfaces/CLAIMS.md` | Promises ledger with proof surfaces | Yes |
| `interfaces/CONTROL_PLANE.md` | Agent sequencing and interoperability | Yes |
| `interfaces/DOC_RULES.md` | Doc compilation and graph semantics | Yes |
| `interfaces/GLOSSARY.md` | Normative term definitions | Yes |
| `interfaces/STORE_MODEL.md` | Store semantics and purity model | Yes |
| `interfaces/TESTING.md` | Verification and proof claim contract | Yes |
| `interfaces/ARCHITECTURE_FOUNDATIONS.md` | Architecture quality primitives and governed artifact contract | Yes |
| `interfaces/KNOWLEDGE_SCHEMA.md` | Knowledge schema + invariants | Yes |
| `interfaces/MEMORY_SCHEMA.md` | Memory schema + retrieval-event contract | Yes |
| `interfaces/DEMANDS_SCHEMA.md` | User-demand schema + precedence rules | Yes |
| `interfaces/RISK_POLICY_GATE.md` | Deterministic PR risk-policy gate semantics | Yes |
| `interfaces/AGENT_CONTEXT_PACK.md` | Agent context-pack layout and mutation contract | Yes |

---

## 2. Decision Rights (Routing)

- Proof claims and testing obligations: `interfaces/TESTING.md`
- Architecture delivery primitives and artifact contract: `interfaces/ARCHITECTURE_FOUNDATIONS.md`
- Knowledge structure and validation: `interfaces/KNOWLEDGE_SCHEMA.md`
- Memory structure and retrieval-event semantics: `interfaces/MEMORY_SCHEMA.md`
- User demand typing and precedence: `interfaces/DEMANDS_SCHEMA.md`
- Deterministic PR risk policy and evidence discipline: `interfaces/RISK_POLICY_GATE.md`
- Agent memory/context pack semantics: `interfaces/AGENT_CONTEXT_PACK.md`

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
- `core/METHODOLOGY.md` - Methodology guides index
- `core/DEPRECATION.md` - Deprecation contract

### Contracts (Interfaces Layer - This Registry)
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/GLOSSARY.md` - Term definitions
- `interfaces/TESTING.md` - Testing contract
- `interfaces/ARCHITECTURE_FOUNDATIONS.md` - Architecture quality primitives
- `interfaces/RISK_POLICY_GATE.md` - Deterministic PR risk-policy gate
- `interfaces/AGENT_CONTEXT_PACK.md` - Agent context-pack contract

### Operations (Plugins Layer)
- `plugins/TODO.md` - Work tracking
- `plugins/VERIFY.md` - Validation subsystem

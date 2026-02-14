# METHODOLOGY.md - Methodology Guides Registry

**Authority:** guidance (how-to guides and practice documents)
**Layer:** Guides
**Binding:** No
**Scope:** all methodology and practice guidance for the Decapod system
**Non-goals:** binding contracts, interface schemas, system requirements

This document indexes all methodology guides in the `methodology/` directory. Methodology guides teach *how to think and work* in the Decapod system.

---

## 1. What Is Methodology

Methodology in Decapod is guidance that:
- Teaches workflows and practices
- Defines agent personas and behaviors
- Provides cognitive frameworks
- Offers learning and adaptation patterns

**⚠️ CRITICAL**: If any methodology conflicts with `specs/INTENT.md`, INTENT WINS. Methodology must never contradict binding contracts.

---

## 2. Methodology Guides (Registry)

| Document | Purpose | Role |
|----------|---------|------|
| `methodology/INTENT.md` | Intent-first workflow methodology | Root methodology + binding contract |
| `methodology/ARCHITECTURE.md` | Architectural practice and decision-making | Architect persona guide |
| `methodology/SOUL.md` | Agent identity and behavioral constraints | Agent persona guide |
| `methodology/KNOWLEDGE.md` | Knowledge management practices | Knowledge curator guide |
| `methodology/MEMORY.md` | Agent learning and memory practices | Learning guide |

---

## 3. Document Purposes

### INTENT.md
The root methodology for intent-driven engineering:
- Unidirectional flow: Intent → Spec → Code → Build → Proof → Promotion
- Drift detection and recovery protocols
- Proof obligation requirements
- **Note**: Contains binding methodology contract

### ARCHITECTURE.md
How to practice architecture in Decapod:
- Required outputs of architecture work
- Architecture update protocol
- Decision records (ADRs)
- System maps and documentation
- Test requirements

**Role**: When doing design work, agents reference this to channel a master architect.

### SOUL.md
Agent identity and prime directives:
- Core identity statement
- Unalterable directives
- Voice and behavioral constraints
- Error handling protocols

**Role**: Defines how agents present themselves and interact.

### KNOWLEDGE.md
Knowledge base management:
- Knowledge categories and tagging
- Entry model and structure
- Lifecycle management
- Integration with subsystems

**Role**: How to capture and maintain project context.

### MEMORY.md
Agent learning and memory:
- Memory types and purposes
- Entry model and metadata
- Lifecycle (create → use → consolidate → prune)
- ROI and retrieval tracking

**Role**: How agents learn from experience.

---

## 4. Pragmatism Principle

**Methodology is pragmatic, not dogmatic.**

- Don't refactor for refactoring's sake
- Keep documents useful even if not perfectly pure
- Extract content only when it creates actual confusion
- A "mostly methodology" file with minor cross-cutting content is acceptable

The goal is **clarity and utility**, not taxonomic perfection.

---

## 5. Relationship to Other Layers

- **specs/**: System-level contracts that methodology must not contradict
- **interfaces/**: Binding machine contracts
- **plugins/**: Subsystem-specific documentation
- **core/**: Routing and coordination documents

---

## 6. Future Audits

The following methodology files may contain content that should be extracted:

- **INTENT.md**: Interface schemas, routing logic → dedicated files
- **ARCHITECTURE.md**: Test requirements → `specs/TESTING.md`
- **SOUL.md**: Emergency protocols → `core/EMERGENCY_PROTOCOL.md`

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts registry
- `core/GAPS.md` - Gap analysis methodology
- `specs/INTENT.md` - Root methodology (binding)
- `specs/SYSTEM.md` - System definition
- `methodology/ARCHITECTURE.md` - Architecture practice
- `methodology/SOUL.md` - Agent identity
- `methodology/KNOWLEDGE.md` - Knowledge management
- `methodology/MEMORY.md` - Agent memory

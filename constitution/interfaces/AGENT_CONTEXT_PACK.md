# AGENT_CONTEXT_PACK.md - Agent Memory and Context Pack Contract

**Authority:** interface (binding contract for agent context-pack layout and mutation boundaries)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** canonical context-pack layout, deterministic load order, mutation authority, and distillation rules
**Non-goals:** persona-writing tips or runner-specific prompt formatting

This interface defines the Decapod-native context pack for persistent agent memory behavior.

---

## 1. Canonical Layout

`(Truth: SPEC)` Context-pack files MUST live under `.decapod/` directory surfaces and not as extra root entrypoints (claim: `claim.context_pack.canonical_layout`).

Required layout:
- `.decapod/context/soul.md`
- `.decapod/context/identity.md`
- `.decapod/context/user.md`
- `.decapod/context/tools.md`
- `.decapod/context/memory.md` (distilled projection)
- `.decapod/memory/daily/`
- `.decapod/memory/decisions/`
- `.decapod/memory/incidents/`
- `.decapod/memory/people/`

---

## 2. Deterministic Load Order

`(Truth: SPEC)` Runners loading the context pack MUST use deterministic order (claim: `claim.context_pack.deterministic_load_order`).

Required order:
1. `soul.md`
2. `identity.md`
3. `user.md`
4. `tools.md`
5. `memory.md`
6. Append-first logs (`daily/`, `decisions/`, `incidents/`, `people/`) by deterministic filename order

---

## 2.1 Deterministic Context Capsule Query

`(Truth: SPEC)` Context retrieval for active execution MUST support deterministic capsule queries (claim: `claim.context.capsule.deterministic`).

Required query inputs:
- `topic` (required)
- `scope` (`core` | `interfaces` | `plugins`, required)
- `task_id` or `workunit_id` (optional, for execution scoping)

Required capsule output shape:
- `topic`
- `scope`
- `sources` (ordered list of canonical source refs)
- `snippets` (ordered extracted slices or summaries)
- `capsule_hash` (hash of canonical serialized capsule bytes)

Determinism rule:
- Same `(topic, scope, task_id/workunit_id, embedded-doc set)` input MUST produce byte-identical capsule JSON and identical `capsule_hash`.

Boundaries:
- Capsule sources MUST resolve from canonical embedded constitution surfaces.
- Capsule queries MUST NOT infer hidden runtime state outside repo-scoped artifacts and embedded docs.

---

## 3. Mutation Authority

`(Truth: SPEC)` High-authority files require human-owned updates or explicit approval workflow (claim: `claim.context_pack.mutation_authority_rules`).

High-authority files:
- `soul.md`
- `identity.md`
- `user.md`
- `tools.md`

Agent-write policy:
- Agents MAY append to `.decapod/memory/*` log files.
- Agents MUST NOT silently overwrite high-authority files.

Store semantics and CLI-only access rules are governed by `interfaces/STORE_MODEL.md`.

---

## 4. Memory Distillation Contract

`(Truth: SPEC)` `memory.md` is a distilled projection from append-first logs and requires a deterministic distill proof surface (claim: `claim.memory.distill_proof_required`).

Required behavior:
- Source inputs are append-first logs plus referenced proofs/decisions.
- Distillation process must be reproducible for same inputs.
- Free-form manual rewrites without explicit approval are non-compliant.

---

## 5. Append-Only Log Contract

`(Truth: SPEC)` `.decapod/memory/daily`, `decisions`, `incidents`, and `people` are append-first operational memory surfaces (claim: `claim.memory.append_only_logs`).

Allowed operations:
- Add new entries.
- Add superseding entries.

Disallowed operation:
- Silent in-place history erasure.

---

## 6. Security Scoping

`(Truth: SPEC)` Sensitive memory contexts must be scope-gated and not automatically loaded into broad/shared contexts (claim: `claim.context_pack.security_scoped_loading`).

Minimum policy:
- Direct operator sessions may load full pack.
- Shared/group contexts must load a scoped subset unless explicitly approved.

---

## 7. Correction Loop Contract

`(Truth: SPEC)` Corrections must become durable artifacts through control-plane flow: correction -> artifact update -> validate -> proof event (claim: `claim.context_pack.correction_loop_governed`).

This forbids "mental note" behavior that is not persisted.

---

## 8. Truth Labels and Upgrade Path

- `claim.context_pack.canonical_layout`: `SPEC` -> `REAL` when validate enforces full shape and root-entrypoint constraints.
- `claim.context_pack.deterministic_load_order`: `SPEC` -> `REAL` when load-order checks are executable.
- `claim.context_pack.mutation_authority_rules`: `SPEC` -> `REAL` when unauthorized overwrites are blocked.
- `claim.memory.append_only_logs`: `SPEC` -> `REAL` when append-only policy is validated.
- `claim.memory.distill_proof_required`: `SPEC` -> `REAL` when distill pipeline has named, enforced proof surface.
- `claim.context_pack.security_scoped_loading`: `SPEC` -> `REAL` when runtime loader enforces scope policies.
- `claim.context_pack.correction_loop_governed`: `SPEC` -> `REAL` when correction-to-proof audit linkage is enforced.

---

## 9. Planned Proof Surfaces

Planned (not yet enforced):
- `decapod validate` gate: context-pack interface and section structure presence.
- Deterministic distill command/proof surface for `memory.md`.
- Policy checks for unauthorized high-authority file mutation.

---

## Links

### Core Router
- `core/DECAPOD.md` - Router and navigation charter

### Registry (Core Indices)
- `core/INTERFACES.md` - Interface contracts index

### Contracts (Interfaces Layer)
- `interfaces/CLAIMS.md` - Claims registry
- `interfaces/DOC_RULES.md` - Doc compiler and truth-label rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/MEMORY_SCHEMA.md` - Memory schema contract
- `interfaces/KNOWLEDGE_SCHEMA.md` - Knowledge schema contract
- `interfaces/RISK_POLICY_GATE.md` - Deterministic PR risk policy contract

### Practice (Methodology Layer)
- `methodology/MEMORY.md` - Memory practice
- `methodology/KNOWLEDGE.md` - Knowledge practice

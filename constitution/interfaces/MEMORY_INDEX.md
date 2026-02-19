# MEMORY_INDEX.md - Optional Local Memory Index Contract

**Authority:** interface (optional local indexing contract)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** optional local-first vector/graph indexing semantics for memory retrieval acceleration
**Non-goals:** default hosted services, always-on daemon promises, or benchmark superiority claims

This document specifies an optional index layer for memory retrieval. It is not enabled by default.

---

## 1. Truth Labels and Status

- Retrieval/event invariants in `interfaces/MEMORY_SCHEMA.md` remain the canonical source.
- Local vector-graph index support in this document is `SPEC` unless explicitly promoted with proof.
- Experimental ranking extensions are `IDEA` unless explicitly promoted with proof.

---

## 2. Optional Capability Surface (`SPEC`)

When enabled explicitly by operator choice, an implementation may maintain a local index with:
- lexical postings
- vector embeddings
- graph edges (`relates_to`, `supersedes`, `depends_on`)

Required boundaries:
1. Index data is store-scoped (`user` or `repo`) and cannot cross-seed stores.
2. Ingestion is from control-plane events and persisted memory/knowledge entries only.
3. Agents do not write index files directly; all mutations are through Decapod CLI surfaces.

---

## 3. Ingestion Contract (`SPEC`)

Input classes:
- retrieval feedback events
- memory entry mutations
- knowledge lifecycle events

Derived artifacts:
- deterministic index snapshots keyed by `(store, as_of, index_version)`
- rebuildable from source events and entries

---

## 4. Safety Constraints (`SPEC`)

1. No implicit network calls for embeddings in default mode.
2. No secret-bearing raw blob persistence in index artifacts.
3. Pointerization/redaction constraints from `specs/SECURITY.md` apply unchanged.

---

## 5. Proof Upgrade Path

To promote any section here to `REAL`:
1. Register/upgrade claim(s) in `interfaces/CLAIMS.md`.
2. Add deterministic replay and schema checks in `decapod validate`.
3. Add reproducible benchmark harness and publish methodology.

External benchmark claims remain aspirational until reproduced in-repo.

---

## Links

- `core/INTERFACES.md` - Interface contracts registry
- `interfaces/MEMORY_SCHEMA.md` - Binding memory schema
- `interfaces/KNOWLEDGE_SCHEMA.md` - Binding knowledge schema
- `interfaces/STORE_MODEL.md` - Store semantics and purity
- `specs/SECURITY.md` - Security and redaction policy

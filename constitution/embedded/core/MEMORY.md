# MEMORY.md - Agent Persistent Memory & Learning

**Authority:** guidance (data model and hygiene suggestions for memory)
**Layer:** Interfaces
**Binding:** No
**Scope:** memory entry model, hygiene rules, and integration points
**Non-goals:** changing intent/specs or acting as proof

This document outlines what “memory” can mean inside Decapod: what agents might store, what they might not, how memory can stay useful, and how it links to TODO, KNOWLEDGE, and PROOF without becoming a second specification.

Memory serves as a control surface, not just a historical record.

---

## 1. Purpose of Persistent Memory

Agent memory aims to make the system more efficient, insightful, and correct over time:

-   **Learning & Adaptation:** Capturing insights from experiences (successes, challenges, and near-misses).
-   **Context & Continuity:** Maintaining continuity across sessions, reducing the need to re-derive information.
-   **Decision Rationale:** Recording the context and reasons for significant decisions for review and learning.
-   **Efficiency:** Helping avoid repeated research, analysis, or recurrent issues.
-   **Audit Trail:** Providing a traceable record of agent actions and state changes where relevant.

---

## 2. Key Principle: Memory Focuses on Pointers, Not Essays

Decapod distinguishes between different types of information for clarity:

-   **Specs** represent agreements.
-   **Proof** provides evidence.
-   **Knowledge** offers context.
-   **Memory** can be seen as the agent’s compressed operational residue: what it might retain to enhance future performance.

Memory can store:
-   Pointers (knowledge IDs, TODO IDs, proof links, file paths).
-   Short summaries (e.g., 1–5 lines).
-   Heuristics or "when X, do Y" patterns.

If content is long-lived and broadly useful as reference material, it may be more appropriate for KNOWLEDGE.

---

## 3. What Memory Can Contain (Suggested Types)

Memory entries can be structured objects. Here are some standard types:

### 3.1 Task Residue
Reflects learnings from task completion.
-   Links to TODO/task IDs.
-   Insights on what worked, what presented challenges, or surprises.
-   Smallest reproducible examples or fix patterns.

### 3.2 Decision Residue
Captures the context and implications of decisions.
-   Intent/spec references.
-   Alternatives considered and reasons for rejection.
-   Risks acknowledged and planned mitigations.

### 3.3 Operational Heuristics
Rules of thumb that can improve execution.
-   e.g., “If validation fails with X, consider checking Y first.”
-   Should be concise, potentially verifiable, and linked to evidence where possible.

### 3.4 Environment Fingerprints
Identifiers that help detect relevant changes:
-   Git commit SHA, schema version, migration version.
-   High-level inventory hashes (optional).
-   Markers indicating "this state was observed at time T."

### 3.5 External Reference Pointers
-   URLs, documents, papers used.
-   Brief explanations: “why it was relevant.”
-   Avoid pasting large blocks of external content.

---

## 4. What Memory Should Generally Avoid

### 4.1 Replicating Knowledge Base Content
If it’s evergreen reference material, it might be better suited for KNOWLEDGE.

### 4.2 Substituting for Specifications
If it defines “what the system is,” it likely belongs in `.decapod/constitution/specs/*` and should be traceable.

### 4.3 Storing Unduly Private/Sensitive Data
Memory should minimize sensitive content. If sensitive details are needed for operation, store references to secure locations rather than raw content.

### 4.4 Journalistic Narratives
Avoid extensive journal entries or subjective accounts unless they directly contribute to improving execution quality in a verifiable way.

---

## 5. Memory Model (Fields + Metadata)

Each memory entry can benefit from machine-readable metadata. Minimum fields often include:

**Core fields:**
-   `id` (UUID/ULID)
-   `type` (`task_residue` | `decision_residue` | `heuristic` | `fingerprint` | `external_pointer`)
-   `title`
-   `summary` (1–5 lines)
-   `created_ts`, `updated_ts` (UTC ISO8601)
-   `source` (agent id / human)
-   `tags` (structured)
-   `links` (array)
-   `confidence` (`high` | `medium` | `low`)
-   `ttl_policy` (`ephemeral` | `decay` | `persistent`)

**Additional recommended fields:**
-   `rel_todos` (array)
-   `rel_knowledge` (array)
-   `rel_specs` (array)
-   `rel_proof` (array)
-   `expires_ts` (explicit expiration for decay entries)

If a memory entry is difficult to tag or link, its utility might be limited.

---

## 6. Lifecycle: Create → Use → Consolidate → Prune

### 6.1 Creation Triggers
Memory can be created when:
-   A TODO is completed (task residue).
-   A decision is made that affects approach or architecture (decision residue).
-   A failure occurs, leading to discovery of a reliable fix (heuristic).
-   Migrations run or schema changes occur (fingerprint).

### 6.2 Retrieval Principles
-   Prioritize relevance.
-   Prefer linked artifacts over pure recollection.
-   Retrieval can return **pointers** first: TODO/KNOWLEDGE/PROOF.
-   If confidence is low, acknowledge it and suggest a minimal verification step.

### 6.3 Consolidation
Memory can be periodically reviewed for consolidation:
-   Multiple related residues might be collapsed into a knowledge entry.
-   Outdated heuristics could be pruned.
-   Low-confidence entries might decay unless reinforced by evidence.

### 6.4 Pruning
Pruning memory is a hygiene practice.
-   Ephemeral entries can expire automatically.
-   Decay entries may expire unless refreshed by usage.
-   Persistent entries might require explicit deprecation (stale/superseded).

---

## 7. Integration with Decapod Subsystems

-   **SYSTEM.md:** Can outline the significance of storing certain information.
-   **TODO:** Memory can link to tasks; tasks can link back to memory residues.
-   **KNOWLEDGE:** Memory can inform and contribute to durable knowledge entries.
-   **PROOF:** Memory can reference proofs; proof failures might generate new memory residues.
-   **REFLEX:** Reflex triggers could consult memory heuristics before acting.
-   **PERCEIVE:** Observations could create memory fingerprints or residues.

Memory often acts as a connective tissue within the system.

---

## 8. Store Separation (User vs Repo, Dogfood Mode)

Decapod supports multiple stores:
-   **User store (default):** `~/.decapod` (starts empty for new users).
-   **Repo store (dogfood):** `<repo>/.decapod` (used for Decapod development).

Memory should respect store boundaries:
-   Repo memory should not automatically leak into user memory.
-   Users typically start with a clean slate; repo dogfooding often involves explicit `--store repo` usage.

---

## 9. Security & Privacy (Considerations)

-   **Minimize:** Store the smallest useful residue.
-   **Redact:** Avoid storing secrets, tokens, private keys, or personal identifiers directly.
-   **Pointerize:** Consider storing references to secure locations rather than raw sensitive content.
-   **Auditability:** Memory entries can be traceable and reviewable.
-   **Access control:** Memory access might be constrained to the selected store and authorized operators.

If content would be problematic in a public forum, it likely requires careful handling in memory.

---

## 10. Memory ROI (Measuring Utility)

Memory that doesn’t influence outcomes can become clutter.

### 10.1 The Concept
When an agent *uses* memory (retrieval that meaningfully affects its next action), recording a **retrieval event** can be valuable. If that retrieval helps complete work faster or avoids a known failure mode, it indicates return on investment (ROI).

Memory ROI focuses on how often memory *saves* from repeated effort, rather than just how often it's read.

### 10.2 Retrieval Event Model (Minimum)
Each retrieval event could record:

**Core details:**
-   `event_id`
-   `ts`
-   `store` (`user` | `repo`)
-   `actor` (agent id)
-   `query` (string or structured)
-   `returned_ids` (memory IDs)
-   `used_ids` (the subset that influenced the action)
-   `context` (e.g., TODO id, command, component)
-   `outcome` (`helped` | `neutral` | `hurt` | `unknown`)

**Recommended additions:**
-   `time_saved_sec` (estimate)
-   `prevented_rework` (boolean)
-   `prevented_failure` (boolean)
-   `notes` (brief description of impact)

### 10.3 How ROI Is Measured (Practical Signals)
ROI can be measured using simple signals:

-   **Helped:** retrieval directly improved efficiency, avoided a known pitfall, or pointed to a correct path.
-   **Hurt:** retrieval led to an inefficient or incorrect approach (suggests a memory hygiene issue).
-   **Neutral:** retrieval did not significantly alter the action.

Optional metrics could include:
-   `time_saved_sec` estimate.
-   Count of "prevented_failure=true" instances.
-   Ratio of "helped" to total retrievals.

### 10.4 Promotion and Pruning Rules
-   If a memory entry consistently "helped," consider promoting it:
    -   Consolidate into KNOWLEDGE if broadly useful.
    -   Mark it `persistent` if it's a high-impact heuristic.
-   If a memory entry ever "hurt," it warrants investigation:
    -   Mark it `stale` immediately.
    -   Add a TODO to address/replace it.
    -   Link the retrieval event as evidence.

### 10.5 Validation Support (Ensuring ROI relevance)
`decapod validate` could help flag:
-   Many retrievals with low "helped" outcomes (suggests memory noise).
-   "Hurt" outcomes without follow-up TODOs.
-   Persistent memories with low confidence and no proof/knowledge links.

---

## 11. Validation Support (Maintaining Memory Quality)

`decapod validate` can help ensure:

-   Memory storage exists and its schema is valid for the selected store.
-   Entries have expected metadata (type, tags, timestamps, confidence, ttl_policy).
-   Entries using normative/contract language (“must/shall/contract”) might be flagged if they don't link to intent/spec, to prevent memory from accidentally becoming a specification.
-   Stale/superseded markers are consistent with linked TODO status (e.g., closed TODO but memory still says “pending investigation”).
-   Optional: warnings on persistent entries with low confidence and no proof links.

---

## See Also

- `.decapod/constitution/core/SOUL.md`: Agent identity and core principles.
- `.decapod/constitution/core/KNOWLEDGE.md`: Durable project context and research base.
- `.decapod/constitution/specs/SYSTEM.md`: Decapod system definition.
- `proof.md`: Evidence and verification surfaces.

## Links

- `.decapod/constitution/core/KNOWLEDGE.md`
- `.decapod/constitution/core/MEMORY.md`
- `.decapod/constitution/core/SOUL.md`
- `.decapod/constitution/specs/SYSTEM.md`


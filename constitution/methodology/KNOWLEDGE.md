# KNOWLEDGE.md - Project Knowledge Base

**Authority:** guidance (knowledge routing, schema expectations, and hygiene suggestions)
**Layer:** Interfaces
**Binding:** No
**Scope:** knowledge entry model, access contract, and hygiene rules
**Non-goals:** redefining intent/specs or substituting for proof

This document outlines how project knowledge is captured, retrieved, and maintained within the Decapod Intent-Driven Engineering System. Knowledge is an execution asset: searchable context that can help prevent rework, preserve decisions, and leverage past investigations.

---

## 1. Purpose of the Knowledge Base

The `decapod data knowledge` subsystem aims to:

-   **Centralize non-contractual context:** Store information that doesn’t primarily belong in `specs/INTENT.md` (contract), `methodology/ARCHITECTURE.md` (compiled design), or `proof.md` (verification), but still holds value.
-   **Preserve rationale:** Capture reasons behind decisions, not just outcomes.
-   **Accelerate onboarding:** Help humans and agents understand historical context quickly.
-   **Reduce rework:** Minimize re-learning and re-investigation.
-   **Support decisions:** Make prior tradeoffs and research accessible for new decisions.
-   **Create a searchable map:** Ensure knowledge is discoverable and accessible.

---

## 2. Key Principles: Knowledge Management

Knowledge entries are most effective when they are:
-   **Actionable:** They should ideally influence future actions.
-   **Searchable:** Ease of discovery is crucial for utility.
-   **Traceable:** Where relevant, entries can link back to intent, code, proof, or TODOs.
-   **Honest:** Outdated entries can be misleading. Consider marking stale entries.

If the utility of an entry isn't clear (e.g., "when would someone use this?"), it might not be suitable for the knowledge base.

---

## 2.5 Boundaries: Intent vs Spec vs Proof vs Knowledge

Decapod benefits from information residing in its appropriate place.

-   **Intent (`specs/INTENT.md`) is a key contract.**
    It defines what is being built, its purpose, and success criteria.

-   **Specification (`specs/INTENT.md`, `methodology/ARCHITECTURE.md`, `specs/SYSTEM.md`) details the design.**
    This covers how intent will be satisfied: interfaces, invariants, architectures, data models, workflows. Specs can evolve but should remain coherent and traceable to intent.

-   **Proof (`proof.md`, tests, checks) provides evidence.**
    It demonstrates that the system behaves as intended. Proof should be executable when possible, and falsifiable otherwise.

-   **Knowledge (`decapod data knowledge`) offers context.**
    This includes rationale, research, decisions, runbooks, historical context, and other supporting information. Knowledge can explain and guide but generally does not alter core contracts.

### Guidelines (for effective knowledge base usage)

1.  **Knowledge should generally not redefine intent or spec.**
    If an entry suggests the system works differently than a spec, it might indicate drift. Address drift by updating intent/spec or implementation, rather than just documenting discrepancies in knowledge.

2.  **Knowledge should not typically substitute for proof.**
    Assertions like “We believe this is correct” might belong in a TODO or a proof plan. Knowledge can describe a proof approach, but the proof itself should be elsewhere.

3.  **If a knowledge entry suggests a next step, it's helpful to link to a relevant TODO or spec.**
    Context gains value when connected to actionable items.

4.  **Stale knowledge can be misleading.**
    If an entry is suspected to be outdated, consider marking it stale or superseded.

---

## 3. Knowledge Categories (Standard Tags)

Categories are expressed as tags. Consider using these defaults:

-   `cat=decision` — Architectural decisions, tradeoffs, constraints
-   `cat=pattern` — Reusable solutions and conventions
-   `cat=debt` — Known limitations, compromises, sharp edges
-   `cat=research` — Spikes, investigations, comparisons, benchmark notes
-   `cat=glossary` — Terms, acronyms, domain concepts
-   `cat=runbook` — Operational steps, local dev, deploy, debugging
-   `cat=external` — Curated external references with brief “why it matters”
-   `cat=example` — Code snippets, usage examples, minimal repros

Additional categories can be added, but aim for discoverability.

---

## 4. Storage, Structure, and Where It Lives (Dogfood + User)

Decapod supports multiple stores ("roots"), similar to TODO:

-   **User store (default):** `~/.decapod`
    Starts empty for end users.

-   **Repo store (dogfood mode):** `<repo>/.decapod`
    Used for developing Decapod and holding repo-scoped knowledge for the codebase.

The `decapod data knowledge` subsystem stores entries in a structured, queryable format. The primary storage is typically a database (SQLite), with optional merge-friendly exports.

Recommended repo layout (repo store):
-   `<repo>/.decapod/project/knowledge.db`
-   `<repo>/.decapod/project/knowledge.events.jsonl` (optional, for merge-friendly collaboration)
-   `<repo>/.decapod/project/knowledge/exports/` (optional derived artifacts)

A key guideline: the repo’s knowledge base should not auto-seed the user store.

---

## 5. Entry Model (Machine-Readable Structure)

Each knowledge entry can benefit from:

**Key elements:**
-   `id` (UUID/ULID)
-   `title` (short, specific)
-   `summary` (one paragraph max)
-   `content` (markdown)
-   `tags` (structured, e.g., `cat=decision`, `area=todo`, `milestone=M1`)
-   `created_ts`, `updated_ts` (UTC ISO8601)
-   `author` (human/codex/agent id)
-   `status` (`active` | `stale` | `superseded`)

**Additional elements (recommended):**
-   `links` (files, PRs, issues, URLs)
-   `rel_todos` (array of TODO IDs)
-   `rel_specs` (e.g., `specs/INTENT.md#...`)
-   `rel_components` (modules/subsystems)
-   `confidence` (`high` | `medium` | `low`)
-   `expires_ts` (optional "this will be stale after X")

If the entry concerns a decision, consider including:
-   `decision`
-   `alternatives`
-   `tradeoffs`
-   `proof/validation` (what verified it, or what still needs proving)

---

## 6. Access Contract (CLI)

Humans and agents can interact via CLI. Core commands might include:

-   `decapod data knowledge add`
-   `decapod data knowledge get <id>`
-   `decapod data knowledge search <query>`
-   `decapod data knowledge list`
-   `decapod data knowledge link --todo <id> --knowledge <id>` (or equivalent)
-   `decapod data knowledge mark-stale <id>`
-   `decapod data knowledge supersede <old> <new>`

Commands are designed to support `--json` (or `--format json|text`) and return a stable machine envelope.

Search capabilities generally include:
-   full-text query (title/summary/content)
-   tag filters
-   time filters
-   references (by TODO/spec/component)

Effective search contributes significantly to knowledge utility.

---

## 7. Integration with Decapod Subsystems

-   **`SYSTEM.md`:** Can describe the role of knowledge.
-   **TODO subsystem:** TODOs might link to knowledge for context; knowledge could link back to TODOs for provenance.
-   **MEMORY:** Memory could store pointers, referencing knowledge IDs and brief summaries.
-   **REFLEX (proposed):** Reflexes might consult knowledge before acting (e.g., "consult runbook before mutation").
-   **PERCEIVE (proposed):** Observations could generate new knowledge or mark old knowledge stale.

---

## 8. Contribution & Maintenance

-   **Agents are encouraged to contribute:** If a non-trivial learning occurs, documenting it as knowledge can be valuable.
-   **Humans can curate:** Refinement of language, corrections, and marking stale entries are valuable contributions.
-   **Staleness awareness:** If an entry is outdated, marking it `stale` or `superseded` helps maintain accuracy.
-   **Versioning:** Knowledge schema can be versioned and migratable. If stored as files/exports, they are derived artifacts.
-   **Deletion policy:** Prefer `superseded` over deletion to preserve historical context.

---

## 9. Validation Support

`decapod validate` can help ensure:

-   Knowledge storage exists and its schema is valid for the selected store.
-   Entries have expected metadata (tags, status, timestamps).
-   Entries linked to closed TODOs are not left in a contradictory state.
-   **Consistency checks:** It can be beneficial to flag when "contract language" appears in knowledge without a spec link, suggesting it should reference `specs/INTENT.md` or a relevant spec section.
-   Optional: warnings on `stale` entries older than a certain duration.

If documentation indicates a CLI knowledge subsystem exists, the CLI should generally be present. If not, documentation should clarify its "planned" status.

---

## 8.5 Review & Fix Discipline

Engineering quality comes from discipline applied consistently. These practices ensure fixes are complete and regressions are prevented.

### Fix the pattern, not just the instance
When fixing a bug, search the entire codebase for all instances of the same pattern. A fix in one location that ignores the same bug elsewhere is half a fix. Use `grep` or equivalent to find siblings before declaring a fix complete.

### Propagate architectural fixes to satellite types
When a core type changes its model (e.g., a store changes its locking model, a broker changes its event format), every type holding a reference from the old model must be updated. Grep for the old type across `src/` and update all call sites.

### Mechanical verification before committing
Run these checks on changed files before declaring work complete:
- Grep for `.unwrap()` / `.expect()` in production code (tests are fine)
- Grep for import style violations (`super::` vs `crate::`)
- If you fixed a pattern bug, grep for other instances of the same pattern
- Run `cargo clippy --all` and resolve all warnings

### Feature flag testing
When the project uses feature flags, verify across configurations:
```bash
cargo check                        # default features
cargo check --no-default-features  # minimal
cargo check --all-features         # everything
```

---

## See Also

- `methodology/MEMORY.md`: How agents store and retrieve persistent information.
- `methodology/SOUL.md`: Agent identity and core principles.
- `specs/SYSTEM.md`: Decapod system definition.

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/METHODOLOGY.md` - Methodology guides index
- `methodology/MEMORY.md` - Agent memory and learning
- `methodology/SOUL.md` - Agent identity
- `methodology/ARCHITECTURE.md` - Architecture practice
- `specs/INTENT.md` - Intent contract
- `specs/SYSTEM.md` - System definition
- `plugins/TODO.md` - Work tracking

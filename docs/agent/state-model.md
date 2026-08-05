# State Model

Decapod manages a finite set of stateful entities. Understanding their lifecycles is critical for successful agentic operation.

These entities are the durable substrate for governed work. Each Decapod
invocation is ephemeral and no daemon owns the task. Repository state turns a
temporary agent conversation into work that can be resumed, audited, validated,
and handed off across processes, models, harnesses, and later invocations.

## 1. Tasks (Todos)
The primary unit of work.
- **States:** `open` -> `claimed` -> `done` | `archived`.
- **Ownership:** A task in the `claimed` state is locked to a specific `agent_id`.
- **Identity:** ULID-based (e.g., `code_01H2...`).

## 2. Workspaces
Isolated execution environments.
- **Types:** Git Worktree | Docker Container.
- **Relationship:** Each active workspace is mapped to exactly one `task_id` and one `agent_id`.
- **Artifacts:** Changes made in a workspace are transient until `workspace publish` is called.
- **Cleanup:** Stale/unused workspaces (associated with done/archived tasks, deleted branches, or matching no active claim) can be cleaned up using the `workspace prune` command.

## 3. Sessions
Authentication and identity verification tokens.

- **Dual-Token Architecture:**
  - **Local Agent Sessions:** Ephemeral, short-lived tokens generated on-the-fly via `session acquire`. Stored machine-locally under `~/.config/decapod/sessions/<project-hash>/<agent-id>.json` when available, with secure workspace-local fallback under `.decapod/managed/sessions/<agent-id>.json` when machine-local storage is unusable. Gates local coordination, TODO subsystem access, and database locking in the workspace (verified using the process-local or environment-provided `DECAPOD_SESSION_PASSWORD`).
  - **Cloud Session Token:** Long-lived global OAuth identity token stored as JSON (`{"token": "..."}`) under `~/.local/share/decapod/session_token.json`. Used to authenticate the user's client with the Propodus cloud backend when cloud storage modes are enabled.
- **Lifecycle:** Local sessions are acquired via `session acquire` and released via `session release`.
- **Restriction:** Most repository mutation commands (e.g., `todo add`, `workspace ensure`) require an active local session.

## 4. Constitution
The static/override rules of the repository.
- **Authority:** Immutable (Global) | Mutable (Local `OVERRIDE.md`).
- **Access:** Read-only via `rpc` or `docs`.
- **Authoring:** Each exact current generated directive subsection owns a four-backtick source block. Humans replace the visible instruction inside it with Markdown or any documentation style they prefer. The content does not render as outer `OVERRIDE.md` structure. Decapod derives structure, hashes, byte counts, source, and precedence.
- **Resolution:** Decapod extracts the wrapper-free body. Duplicate exact registered directives, unclosed wrappers, or non-empty unknown IDs in Decapod namespaces invalidate the complete repository overlay. Empty retired generated sections are ignored for upgrade compatibility. Nested headings and triple-backtick examples remain body content.

## 5. Event Evidence
Append-only operational evidence.
- **Authority:** Canonical tables in `.decapod/data/decapod.db` only (`events` streams + projection tables), accessed through `core::events`.
- **No live JSONL:** Runtime writers never append to `*.jsonl`. Historical files under `.decapod/data/` are one-shot migration inputs: imported into `events`, then moved to `.decapod/data/.retired-jsonl/`. Validate fails if known live legacy JSONL reappears.
- **Migration:** `events.retire_legacy_jsonl.v001` and related migrations import residual logs, unwrap double-wrapped federation payloads, and migrate assurance attestations into `events` (stream=`assurance`). Recreated legacy SQLite stores are copied forward and removed.
- **Federation payload shape:** Native federation writers and imports store only the inner domain `payload` object in `events.payload`. Older double-wrapped rows are unwrapped automatically. Operators must not hand-edit the SQLite store; verify recovery with `decapod validate --projections` and a green `federation.rebuild_determinism` gate.

## 6. Knowledge (Memory)
The persistent, shared understanding of the project.
- **Class:** Advisory (Aptitude) | Procedural (Federated Knowledge).
- **Persistence:** Surmounts individual sessions and agents.

## 7. Governance and Proof Artifacts

- **Intent and Plans:** The human supplies intent; the agent records and updates its governed interpretation through Decapod. Plans are execution state, not proof by themselves.
- **Claims:** Falsifiable repository-owned statements linked to a baseline, failure mode, measurement, and proof gate. They change as research evidence changes.
- **Trajectories:** Agent-recorded custody evidence for intent, boundaries, inspected and modified files, assumptions, tool calls, checks, and proof references across a run.
- **Living Specifications:** Agent-authored interpretations under `.decapod/managed/specs/`. Decapod requires and validates them but does not independently author their semantic claims.
- **Validation Receipts and Evidence:** Decapod records validation outcomes and binds required evidence to identified repository state. A failed result leaves the task incomplete.
- **Projections:** Generated views derived from supported authoritative inputs. Refresh may update them, but a projection does not become a second source of truth.
- **Publication State:** A governed transition that remains blocked while required validation, evidence, or approval is unsatisfied.

External systems such as GitHub Issues, Jira, Linear, or Beads may remain the
organizational system of record. Decapod todos and claims govern the accepted
work at the execution layer.

# State Model

Decapod manages a finite set of stateful entities. Understanding their lifecycles is critical for successful agentic operation.

These entities are the durable substrate for governed work. They turn a temporary agent conversation into inspectable project state that can be resumed, audited, verified, and handed off.

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
- **Authority:** Canonical tables in `.decapod/data/decapod.db`, accessed through `core::events`.
- **Migration:** Unproven legacy JSONL is imported idempotently during startup. A successful single-datastore migration durably retires its inputs, and later runtime validation, health, heartbeat, and flight-recorder reads do not consult them. Recreated legacy SQLite stores are copied forward and removed.

## 6. Knowledge (Memory)
The persistent, shared understanding of the project.
- **Class:** Advisory (Aptitude) | Procedural (Federated Knowledge).
- **Persistence:** Surmounts individual sessions and agents.

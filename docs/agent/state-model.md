# State Model

Decapod manages a finite set of stateful entities. Understanding their lifecycles is critical for successful agentic operation.

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
  - **Local Agent Sessions:** Ephemeral, short-lived tokens generated on-the-fly via `session acquire`. Stored machine-locally under `~/.config/decapod/sessions/<project-hash>/<agent-id>.json`. Gates local coordination, TODO subsystem access, and database locking in the workspace (verified using the process-local or environment-provided `DECAPOD_SESSION_PASSWORD`).
  - **Cloud Session Token:** Long-lived global OAuth identity token stored as JSON (`{"token": "..."}`) under `~/.local/share/decapod/session_token.json`. Used to authenticate the user's client with the Propodus cloud backend when cloud storage modes are enabled.
- **Lifecycle:** Local sessions are acquired via `session acquire` and released via `session release`.
- **Restriction:** Most repository mutation commands (e.g., `todo add`, `workspace ensure`) require an active local session.

## 4. Constitution
The static/override rules of the repository.
- **Authority:** Immutable (Global) | Mutable (Local `OVERRIDE.md`).
- **Access:** Read-only via `rpc` or `docs`.

## 5. Knowledge (Memory)
The persistent, shared understanding of the project.
- **Class:** Advisory (Aptitude) | Procedural (Federated Knowledge).
- **Persistence:** Surmounts individual sessions and agents.

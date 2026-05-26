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

## 3. Sessions
Short-lived authentication tokens.
- **Credential:** `DECAPOD_SESSION_PASSWORD`.
- **Lifecycle:** Acquired via `session acquire`, released via `session release`.
- **Restriction:** Most mutation commands (e.g., `todo add`, `workspace ensure`) require an active session.

## 4. Constitution
The static/override rules of the repository.
- **Authority:** Immutable (Global) | Mutable (Local `OVERRIDE.md`).
- **Access:** Read-only via `rpc` or `docs`.

## 5. Knowledge (Memory)
The persistent, shared understanding of the project.
- **Class:** Advisory (Aptitude) | Procedural (Federated Knowledge).
- **Persistence:** Surmounts individual sessions and agents.

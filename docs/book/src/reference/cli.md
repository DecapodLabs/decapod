# CLI Reference

Decapod provides a unified CLI that supports both human-friendly text output and machine-readable JSON.

## Command Aliases

Decapod provides short aliases for common subcommands:
- `v` -> `validate`
- `i` -> `init`
- `t` -> `todo`
- `w` -> `workspace`
- `g` -> `govern`
- `s` -> `session`
- `d` -> `docs`

---

## Core Operations

### `validate` (alias: `v`)
Perform methodology compliance checks.
- `--store <repo|user>`: The task store to validate.
- `--format <text|json>`: Output formatting.
- `--verbose`: Enable detailed per-gate timing.

### `init` (alias: `i`)
Bootstrap or manage the Decapod lifecycle.
- `with`: Apply explicit options (non-interactive).
- `clean`: Remove all Decapod state from the directory.

### `capabilities`
Discover the features supported by the current Decapod binary.

---

## Workspace Management (alias: `w`)

### `workspace ensure`
Create or enter an isolated task worktree.
- `--branch <name>`: Provide a custom branch name.
- `--container`: Wrap the workspace in a Docker container.

### `workspace status`
Display active workspaces, their owners, and their current state.

### `workspace publish`
Prepare and bundle changes from an isolated workspace for promotion (PR/merge).

---

## Task Tracking (alias: `t`)

### `todo list`
List tasks from the backlog.
- `--status <open|claimed|done|archived>`: Filter by state.
- `--category <name>`: Filter by task category.

### `todo claim`
Lock a task for active implementation.
- `--id <task-id>`: The specific ULID of the task.
- `--mode <exclusive|shared>`: Set the locking mode.

### `todo done`
Complete a work unit and generate proof artifacts.
- `--id <task-id>`: The task to close.
- `--validated`: Capture a cryptographic proof baseline of the changes.

---

## Governance & Subsystems (alias: `g`)

### `govern policy`
Classification and approval for high-risk actions.

### `govern health`
Claims, proofs, and system-wide integrity status.

### `govern capsule query`
Perform a deterministic query over the embedded constitution.
- `--topic <name>`: The subject of inquiry.
- `--scope <scope>`: The context boundary (e.g., "interfaces").

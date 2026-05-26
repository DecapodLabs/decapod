# Error Recovery

Decapod uses deterministic error messages and exit codes. Treat these as **Operational Instructions**, not just failure reports.

## Standard Exit Codes

| Code | Label | Meaning | Recovery Path |
|---|---|---|---|
| `1` | `Validation` | Methodology gate failed. | Read the error, fix the code/state, and re-run `validate`. |
| `2` | `Config` | `config.toml` error. | Verify key names and types in `config.toml`. |
| `3` | `Auth` | Missing session. | Run `decapod session acquire`. |
| `4` | `NotFound` | Entity missing. | Verify the ID with `todo list` or `workspace status`. |
| `5` | `Conflict` | Resource locked. | Select a different task; the resource is owned by another agent. |

## Common Error Patterns

### `Conflict("TODO already claimed")`
- **Reason:** Another agent instance is working on this task.
- **Protocol:** **STOP**. List other tasks with `decapod todo list` and select an unclaimed one.

### `ValidationError("AGENTS.md missing")`
- **Reason:** The repository has been corrupted or not initialized.
- **Protocol:** Run `decapod init` to restore the mandatory agent entrypoints.

### `RiskGate("Action requires approval")`
- **Reason:** You are attempting a high-risk operation (e.g., handoff or policy change).
- **Protocol:** Notify the human operator. They must approve the action via `decapod govern policy approve`.

### `WorkspaceError("Dirty tree")`
- **Reason:** Uncommitted changes exist in a directory where a worktree is being created.
- **Protocol:** Commit or stash your changes before running `workspace ensure`.

## General Strategy
1.  **Parse the Error:** Decapod errors are strongly typed. Look for the `kind` and `message`.
2.  **Consult the Contract:** Cross-reference the command in `command-contracts.md`.
3.  **No Guessing:** Do not attempt "brute-force" argument variations. If a recovery path is not obvious, stop and request human assistance.

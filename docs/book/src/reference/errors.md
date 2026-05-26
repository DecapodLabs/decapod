# Error Codes

Decapod uses descriptive error messages and idiomatic exit codes.

## Exit Codes

| Code | Meaning | Description |
|---|---|---|
| 0 | Success | The operation completed successfully. |
| 1 | Validation Failure | `decapod validate` found issues in the repo state. |
| 2 | Configuration Error | Problem with `config.toml` or environment variables. |
| 3 | Permission Denied | Missing `DECAPOD_SESSION_PASSWORD` or insufficient rights. |
| 4 | Not Found | A requested task, workspace, or artifact was not found. |
| 5 | Conflict | Another agent has already claimed the task or workspace. |
| 127 | Command Not Found | A required external tool (git, docker) is missing. |

## Common Error Messages

### `ValidationError("AGENTS.md missing")`
The repository lacks the mandatory `AGENTS.md` entrypoint. Run `decapod init` to restore it.

### `Conflict("TODO already claimed")`
The task you are trying to claim is already owned by another agent. Use `decapod todo list` to see current owners.

### `NotFound("Session token expired")`
Your session has expired. Run `decapod session acquire` to get a new one.

### `RiskGate("Action requires approval")`
The action you are attempting is high-risk and requires human approval via `decapod govern policy approve`.

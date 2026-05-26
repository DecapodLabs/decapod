# Configuration Schema

Agents must consult Decapod configuration to understand repo-local operational policy. This is the "human-to-agent" configuration substrate.

## Critical Policy Keys

### `repo.container_workspaces`
- **Type:** `bool`
- **Policy:** If `true`, you MUST enable Docker isolation.
- **Command:** Add `--container` to your `decapod workspace ensure` calls.

### `repo.done_criteria`
- **Type:** `string`
- **Policy:** Defines the subjective and objective requirements for completion.
- **Constraint:** Do not call `todo done` until your implementation satisfies this string.

### `repo.external_tracker`
- **Type:** `bool`
- **Policy:** If `true`, Decapod expects tasks to be linked to external systems.
- **Action:** Ensure you provide the `--ref` flag when creating or updating tasks.

### `repo.primary_languages`
- **Type:** `list<string>`
- **Orientation:** Use this to select the correct compilers, linters, and test runners in your workspace.

### `init.entrypoints`
- **Type:** `list<string>`
- **Policy:** Defines which root-level files (like `AGENTS.md`) Decapod will automatically maintain and validate.

## Machine Discovery
For a structured representation of the project's configuration and current orientation, use:
```bash
decapod capabilities --format json
```
Or use the Orientation RPC:
```bash
decapod rpc --op infer.orientation
```

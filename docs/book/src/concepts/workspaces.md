# Workspaces

Decapod workspaces are isolated execution environments that ensure agentic work is performed safely and reproducibly.

## The Isolation Hierarchy

Decapod provides three levels of isolation:

1.  **Branch Isolation:** Every task is performed on a dedicated git branch, preventing accidental mutations to `main`.
2.  **Filesystem Isolation:** Decapod uses **Git Worktrees** to create unique directory structures for every task. This allows multiple agents to work on the same repository concurrently without filesystem collisions.
3.  **Process Isolation:** By enabling `container_workspaces`, Decapod wraps each worktree in a Docker container. This ensures that an agent's processes, environment variables, and local dependencies (like `node_modules`) are completely isolated from other agents.

## Workspace Lifecycle

- **Acquisition:** Triggered by `decapod workspace ensure`. Decapod calculates the necessary isolation level based on project config.
- **Entry:** The agent `cd`s into the workspace. All subsequent tool calls (compilers, linters, tests) must be executed within this boundary.
- **Promotion:** Once work is verified, `decapod workspace publish` bundles the changes for merging back into the root repository.
- **Eviction:** Completed or abandoned workspaces are automatically cleaned up to save disk space and maintain repository hygiene.

## Why it Matters

Without workspace isolation, multi-agent systems are prone to "environment drift" and race conditions. Decapod's workspace model makes concurrent agent work as safe as concurrent human work on separate machines.

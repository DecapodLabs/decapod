# Multi-Agent Workflow

Decapod is designed from the ground up to support concurrent multi-agent operations, providing the coordination and isolation necessary to prevent collisions.

## The Coordination Model

Decapod uses a **Lock-then-Isolate** model for multi-agent work:

1.  **Global Lock:** Agents use `decapod todo claim` to acquire an exclusive lock on a task. This prevents two agents from working on the same logical unit of work.
2.  **Filesystem Isolation:** Each agent is assigned a unique git worktree. Even if multiple agents are working on the same repository, they never see each other's uncommitted files.
3.  **Container Isolation:** For maximum safety, `container_workspaces = true` ensures that each agent has its own process space and system dependencies.

## Shared Context

While execution is isolated, context is shared. Agents use `decapod data memory` (Aptitude) to share learned preferences and project-specific knowledge across sessions. This allows Agent B to benefit from a code convention learned by Agent A five minutes earlier.

## Best Practices

- **Frequent Heartbeats:** Agents should run `decapod todo heartbeat` to signal they are still active.
- **Explicit Handoffs:** Use `decapod todo handoff` to transfer a task (and its current uncommitted state) between agents.
- **Centralized Validation:** Always run `decapod validate` before publishing to ensure your changes haven't introduced regressions against the latest state of the root repository.

# Workspace Isolation

Workspace isolation is Decapod's primary defense against the "dirty tree" problem and multi-agent environment corruption.

## Git Worktrees: The First Line of Defense

By default, `decapod workspace ensure` creates a **Git Worktree**. Unlike a standard clone, a worktree allows you to have multiple branches checked out simultaneously in different directories while sharing a single `.git` database.

- **Speed:** Creating a worktree is nearly instantaneous.
- **Integrity:** Each workspace is a clean slate. There are no residual build artifacts from other branches.

## Container Isolation: The Gold Standard

When working with multiple agents or complex dependency chains, filesystem isolation is often not enough. Decapod can wrap each worktree in a Docker container.

### Benefits of Containerization:
- **Dependency Sandboxing:** One agent can run `npm install` for Node 18 while another uses Node 20 in a separate workspace.
- **Process Protection:** A rogue agent process (e.g., an infinite loop or a memory leak) cannot crash the host machine or affect other agents.
- **Restricted Access:** You can define network and volume policies to limit what an agent can see outside its workspace.

## Managing the Workspace Pool

Use `decapod workspace status` to view the health and ownership of all active workspaces. Decapod handles the complex plumbing of mapping these directories to specific agents and tasks, so you don't have to.

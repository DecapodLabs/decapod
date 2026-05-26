# Quickstart

Get Decapod operational in your repository in under five minutes.

## 1. Installation

Install the Decapod binary using Cargo:

```bash
cargo install decapod
```

## 2. Initialization

Initialize your repository. This creates the `.decapod/` directory and scaffolds the initial agent entrypoints (`AGENTS.md`, etc.).

```bash
decapod init
```

## 3. Orientation

Verify that your repository meets basic governance requirements. Decapod will check for the presence of mandatory files and invariants.

```bash
decapod validate
```

## 4. The Agent Handshake

Before performing governed work, an agent must acquire a session. This establishes the agent's identity and permissions for the current work period.

```bash
decapod session acquire
```

## 5. Claiming a Task

Identify a task from the backlog and claim it. This prevents other agents from attempting the same work simultaneously.

```bash
# Add a task if one doesn't exist
decapod todo add "Refactor the parser logic" --priority high

# List and claim
decapod todo list
decapod todo claim --id <task-id>
```

## 6. Entering the Workspace

Create an isolated git worktree for the task. Decapod ensures you are working in a clean environment, safely away from the main branch.

```bash
decapod workspace ensure
```

**Note:** If `container_workspaces = true` is set in your config, add the `--container` flag to wrap the workspace in Docker.

## 7. Delivery and Proof

Once implementation is complete within the isolated workspace, run validation and mark the task as done. This generates the final proof artifacts.

```bash
decapod validate
decapod todo done --id <task-id>
```

# Quickstart

Get Decapod operational in your repository in under five minutes.

## 1. Installation

Install the Decapod binary using Cargo:

```bash
cargo install decapod
```

For an existing Decapod repository, installation is followed by an autonomous,
idempotent upgrade on the next normal command. `decapod init`, `decapod validate`,
and other governed commands reconcile unproven legacy event JSONL into the
canonical SQLite event store before runtime consumers read it. A prior successful
single-datastore migration retires its JSONL inputs, while any legacy SQLite
stores recreated by an older binary are copied forward and removed. Legacy files
are never treated as a second runtime authority. Existing
override body bytes are preserved while sections are rendered into fenced
documentation source areas with visible authoring instructions; unsafe or
ambiguous authority fails closed.

## 2. Initialization

Initialize your repository. This is the only human-facing setup command: use
`--backend cloud` when the repository should use the Propodus todo service.
Initialization creates the `.decapod/` directory and scaffolds the initial
agent entrypoints (`AGENTS.md`, etc.). `.decapod/` is the repo-native substrate
where governed agent work records intent, context, custody, boundaries,
validation evidence, and completion state. Agents will routinely run this
during validation stages with the `--proof` flag for non-interactive
agent-driven autonomous upgrades (see [Configuration](configuration.md) and
[Constitution](concepts/constitution.md)).

```bash
decapod init
# For Propodus-backed todos:
decapod init --backend cloud
```

## 3. Orientation

Verify that your repository meets basic governance requirements. Decapod will check for the presence of mandatory files and invariants, then report whether the current repo state satisfies the governed execution contract. Agents will automatically call `decapod validate` as needed (see [Proof & Validation](concepts/proof.md)).

```bash
decapod validate
```

## 4. The Agent Handshake

Before performing governed work, an agent must acquire a session. This establishes the agent's identity and permissions for the current work period so later task, workspace, and proof records are attributable (see [CLI Reference](reference/cli.md#decapod-session)). For cloud repositories, session acquisition first establishes local custody and then reuses or refreshes the machine-local Propodus session. Human users should never call this.

```bash
decapod session acquire
```

## 5. Claiming a Task

Identify a task from the backlog and claim it. The todo turns a user request into explicit project state and prevents other agents from attempting the same work simultaneously (see [Single-Agent Workflows](workflows/single-agent.md) and [Multi-Agent Workflows](workflows/multi-agent.md)). Human users should never call this.

```bash
# Add a task if one doesn't exist
decapod todo add "Refactor the parser logic" --priority high

# List and claim
decapod todo list
decapod todo claim --id <task-id>
```

## 6. Entering the Workspace

Create an isolated git worktree for the task. Decapod turns workspace custody into inspectable repo state and ensures you are working in a clean environment, safely away from the main branch (see [Workspace Sandboxing](concepts/workspaces.md)). Human users should never call this.

```bash
decapod workspace ensure
```

**Note:** If `container_workspaces = true` is set in your config (see [Config Specification](reference/config-toml.md)), add the `--container` flag to wrap the workspace in Docker (see [Workspace Isolation Workflow](workflows/workspace-isolation.md)).

## 7. Delivery and Proof

Once implementation is complete within the isolated workspace, run validation and mark the task as done. This turns completion into a verified state transition and generates the final proof artifacts (see [Artifact Reference](reference/artifacts.md)). Human users should never call this.

```bash
decapod validate
decapod todo done --id <task-id>
```

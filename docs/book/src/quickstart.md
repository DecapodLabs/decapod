# Quickstart

Get Decapod operational in your repository in under five minutes.

## 1. Installation

Install the Decapod binary using Cargo:

```bash
cargo install decapod
```

For an existing Decapod repository, installation is followed by an autonomous,
idempotent upgrade on the next normal command. `decapod init`, `decapod validate`,
and other governed commands one-shot-import any residual legacy event JSONL into
the canonical SQLite event store (`decapod.db`) and move those files under
`.decapod/data/.retired-jsonl/`. Runtime writers never append to JSONL. Legacy
SQLite stores recreated by an older binary are copied forward and removed. Existing
override body bytes are preserved while sections are rendered into fenced
documentation source areas with visible authoring instructions; unsafe or
ambiguous authority fails closed.

## 2. Initialization

Initialize your repository. This is the only human-facing setup command: use
`--backend cloud` when the repository should use the managed cloud task service.
Initialization creates the `.decapod/` directory and scaffolds the initial
agent entrypoints (`AGENTS.md`, etc.). `.decapod/` is the repo-native substrate
where governed agent work records intent, context, custody, boundaries,
validation evidence, and completion state. Agents will routinely run this
during validation stages with the `--proof` flag for non-interactive
agent-driven autonomous upgrades (see [Configuration](configuration.md) and
[Constitution](concepts/constitution.md)).

```bash
decapod init
# For managed cloud tasks:
decapod init --backend cloud
```

Cloud initialization may return `onboarding_pending` with a one-time browser
URL. Continue with your coding agent: it shares the link, resumes the saved
connection with `decapod cloud login`, and continues your task after sign-in.
The human runs init once per repository; later Decapod operations belong to the
agent. Setup is ready only when the service confirms account
completion and an authenticated repository read succeeds. The `--proof` flag
prepares cloud configuration only; it does not authenticate or establish
readiness. See [Cloud sign-in and task access](reference/propodus.md) for
private custody, account requirements, and explicit interrupted-flow recovery.

## 3. Agent Documentation

Before using Decapod, the agent loads the command contracts and evaluates the
incoming prompt as required by the generated entrypoints. Task orientation is a
later step: it runs inside the task's claimed, isolated worktree, not from the
main checkout.

```bash
decapod docs ingest
printf '%s' '<incoming prompt>' | decapod eval --stdin --format json
```

Continue only when the prompt gate succeeds and returns `status: allow`.

## 4. The Agent Handshake

Before performing governed work, an agent must acquire a session. This establishes the agent's identity and permissions for the current work period so later task, workspace, and proof records are attributable (see [CLI Reference](reference/cli.md#decapod-session)). For cloud repositories, session acquisition first establishes local custody and then reuses or refreshes the private managed session through Dactyl. Human users should never call this.

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
decapod workspace ensure --branch agent/<agent-id>/<task-id>-<topic>
cd .decapod/workspaces/<your-worktree>
```

Use the actual `worktree_path` returned by `workspace ensure` for the directory
change; the path above is a placeholder.

**Note:** If `container_workspaces = true` is set in your config (see [Config Specification](reference/config-toml.md)), add the `--container` flag to wrap the workspace in Docker (see [Workspace Isolation Workflow](workflows/workspace-isolation.md)).

## 7. Task Orientation

From the reported worktree, orient the agent before implementation. Creating the
workspace satisfies the isolation prerequisite; it does not replace orientation
or authorize work outside the claimed task.

```bash
decapod infer orientation --task-id <task-id>
decapod constitution get core/DECAPOD
decapod rpc --op context.resolve
```

## 8. Delivery and Proof

After the agent has performed the work in the isolated workspace, it runs validation. A failed gate leaves the task incomplete. When the result includes supported remediation, the agent corrects the violated artifact or state, re-runs validation, and continues. Only passing required gates and evidence permit the governed completion and publication transitions (see [Artifact Reference](reference/artifacts.md)). Human users should never call these commands.

```bash
decapod validate
decapod todo done --id <task-id>
```

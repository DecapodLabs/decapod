# Quickstart

Get Decapod operational in your repository in under five minutes.

> **Who runs what:** A **human operator** installs Decapod and runs `decapod init` once per repository. After initialization, **your coding agent** (Codex, Claude Code, Cursor, Antigravity, or another harness) invokes the remaining Decapod commands automatically as part of its workflow.

## 1. Installation (Human)

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

## 2. Repository Initialization (Human, once per repo)

Initialize your repository. This is the only human-facing setup command: use
`--backend local` (**Decapod Local**, default, free and offline) to store task custody locally in `.decapod/data/decapod.db`, or `--backend cloud` (**Decapod Cloud**) when teams on separate machines should share live task claims through the managed cloud task service. Both modes keep specs, plans, trajectories, worktrees, and Git proof repo-native.

Initialization creates `.decapod/` and scaffolds the agent entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`) so whichever coding agent you open knows how to invoke the kernel. Agents later run `decapod init --proof` during validation for non-interactive upgrades (see [Configuration](configuration.md) and [Constitution](concepts/constitution.md)).

```bash
decapod init
# Or for shared managed cloud tasks:
decapod init --backend cloud
```

Terminal setup follows **Project → Preferences → Review**. Describe the project,
keep **Recommended** defaults, or use **Customize** to adjust project details,
workspace isolation, connections and advanced settings. Local storage needs no
account. Existing authored settings remain the defaults.

**Apply setup** creates the selected directory, Git repository and governance
files. **Edit choices** returns to preferences; **Cancel** leaves a newly selected
project uncreated. A dry run writes no project state. Changed configuration is
backed up privately under `.decapod/data/config-backups/`.

Cloud sign-in starts after applying the reviewed setup. Pricing, seat assignment
and repository authorization remain service-owned; setup does not collect a
credential or maintain an access roster in TOML.

Cloud initialization may return `onboarding_pending` with a one-time browser
URL. Continue with your coding agent: it shares the link, resumes the saved
connection with `decapod cloud login`, and continues your task after sign-in.
The human runs init once per repository; later Decapod operations belong to the
agent. Setup is ready only when the service confirms account completion and an
authenticated repository read succeeds. The `--proof` flag prepares cloud
configuration only; it does not authenticate or establish readiness. See
[Decapod Cloud sign-in and task access](reference/propodus.md) for private
custody, account requirements, and explicit interrupted-flow recovery.

Once `decapod init` completes, **open your preferred coding agent** in the project and describe the engineering goal. You do not need to run the remaining kernel commands by hand.

## 3. Prompt Gate & Contract Ingestion (Agent-invoked)

Before reading repository content or mutating state, the coding agent evaluates the incoming prompt and ingests current command contracts from the binary:

```bash
printf '%s' '<incoming prompt>' | decapod eval --stdin --format json
decapod docs ingest
```

The agent continues only when `decapod eval` exits `0` and returns `"status": "allow"`.

## 4. Agent Session Custody (Agent-invoked)

Before performing governed mutations, the agent acquires a session so task claims, workspace leases, and proof records are attributable:

```bash
decapod session acquire
```

Export the printed `DECAPOD_AGENT_ID` and `DECAPOD_SESSION_PASSWORD` for session-scoped commands. For cloud repositories, session acquisition first establishes local custody and then reuses or refreshes the private managed session through Dactyl.

## 5. Claiming a Task (Agent-invoked)

The agent creates or claims a `todo` to establish scoped ownership and prevent other agents or teammates from silently duplicating the same unit of work (see [Single-Agent Workflow](workflows/single-agent.md) and [Multi-Agent Workflow](workflows/multi-agent.md)):

```bash
# Add a task if one doesn't exist
decapod todo add "Refactor the parser logic" --priority high

# Inspect open tasks and fleet overlap, then claim with an explicit lease
decapod todo list
decapod todo fleet
decapod todo claim --id <task-id> --lease-seconds 3600
```

## 6. Entering an Isolated Workspace (Agent-invoked)

The agent creates an isolated Git worktree for the claimed task, keeping the root checkout (`master`/`main`) untouched (see [Workspaces](concepts/workspaces.md)):

```bash
decapod workspace ensure --branch agent/<agent-id>/<task-id>-<topic>
cd <reported-worktree-path>
```

Use the exact `worktree_path` returned by `decapod workspace ensure`. If `container_workspaces = true` is configured (see [Config Specification](reference/config-toml.md)), add `--container` to wrap the workspace in a Docker container (see [Workspace Isolation Workflow](workflows/workspace-isolation.md)).

## 7. Decision-Time Orientation & Reflection (Agent-invoked)

Inside the isolated worktree, the agent gathers task-bound constraints, context, and optional bounded reflection before writing code:

```bash
decapod infer orientation --task-id <task-id>
decapod rpc --op context.resolve
# Optional: consult embedded global engineering doctrine inside the governed loop
decapod constitution get core/DECAPOD
```

At a consequential decision boundary or forbidden-path risk, the agent can pass `--reflection '<request-json>'` to `decapod infer orientation --task-id <task-id>` or call `decapod rpc --op mentor.obligations` for a source-bound, three-round challenge check.

## 8. Checkpoints, Validation, and Proof (Agent-invoked)

As the agent implements the change in the isolated worktree, it updates living specs (`.decapod/managed/specs/*`), records trajectory checks, checkpoints staged material before committing, and runs validation on the exact candidate commit:

```bash
decapod govern trajectory record --run-id <run-id> --modified-file <path> --check "unit=passed"
git add <authored-paths>
decapod govern artifacts checkpoint --id <checkpoint-id> --summary "What changed and why"
git add .decapod/governance.json
git commit -m "Describe the governed change"

decapod validate
```

If `decapod validate` fails, the agent inspects the reported blockers, fixes the root cause on the work branch, checkpoints and commits the repair, and re-runs `decapod validate` against the revised commit. Only after required gates pass does it publish and complete the task:

```bash
decapod workspace publish
decapod todo done --id <task-id> --validated
```

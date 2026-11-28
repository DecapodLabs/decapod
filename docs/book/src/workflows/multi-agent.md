# Multi-Agent Workflow

Decapod supports concurrent work across **multiple engineers and multiple coding agents** on the same repository—whether two teammates use **Codex** and **Claude Code** in parallel, or several agent workers tackle distinct slices of one codebase.

## The Coordination Model: Lease, Isolate, Inspect

Decapod coordinates parallel work through explicit custody and isolated filesystems:

1. **Scoped Task Claims & Leases:** Agents acquire exclusive or shared custody with `decapod todo claim --id <id> --lease-seconds 3600` (default exclusive; `--mode shared` joins as secondary owner). Leases expire automatically if abandoned unless renewed via `decapod todo renew --id <id>`.
2. **Overlap Inspection (`todo fleet`):** Before modifying overlapping files or paths, agents run `decapod todo fleet` to inspect active leases, expired claims, and path/scope overlaps across the repository (`tests/unit/core/fleet_coord_tests.rs`).
3. **Worktree & Container Isolation:** Each claimed task runs in its own Git worktree via `decapod workspace ensure --branch agent/<agent-id>/<task-id>-<topic>`, optionally wrapped in Docker via `--container` when `container_workspaces = true` (see [Workspace Isolation](workspace-isolation.md)).
4. **Explicit Handoffs & Yields:** When passing a task to another engineer's agent or a specialist reviewer, use `decapod todo handoff --id <id> --to <receiver> --summary "..."` (which advances the lease generation and re-issues the lease) or `decapod todo yield --id <id>` (which releases exclusive lease capacity without archiving the task).

```bash
decapod todo fleet --format json
decapod todo claim --id <task-id> --lease-seconds 3600
decapod workspace ensure --branch agent/<agent-id>/<task-id>-<topic>
```

## Shared Context Without Shared Dirty Trees

While each agent edits files in an isolated worktree, durable project context stays shared:

- **Tasks, scope & governance:** Task state lives in `.decapod/data/decapod.db` (`backend = "local"`) or the authorized **Decapod Cloud** service (`backend = "cloud"`; see [Cloud project routing](../reference/cloud-projects.md) and [Organization Admin](https://decapodlabs.ai/admin) for repository enrollment, seat assignments, and teammate access policies), while plans, claims, trajectories, and living specs live in `.decapod/governance.json` and `.decapod/managed/specs/*`.
- **Aptitude & federated knowledge:** Agents can record and retrieve project conventions and procedural observations via `decapod data aptitude` and `decapod data federation`.
- **Heartbeats & presence:** Agents can record liveness via `decapod todo heartbeat` and renew active leases via `decapod todo renew`.

## Concrete journey: two engineers using different agents in parallel

Suppose Engineer A (using **Claude Code**) works on authentication (`src/auth/`) and Engineer B (using **Codex**) works on CLI formatting (`src/cli/`):

1. Both agents run `decapod session acquire` with distinct `DECAPOD_AGENT_ID` values, check `decapod todo fleet`, and claim separate tasks with disjoint scopes.
2. Each agent calls `decapod workspace ensure` to obtain its own worktree and branch. Uncommitted edits, build caches, and test runs never collide on disk.
3. If Engineer B's task later needs a file Engineer A already claimed, `decapod todo fleet` surfaces the path/scope overlap. Rather than silently overwriting work, the engineers coordinate via `decapod todo handoff`, sequential ordering, or separate commits.
4. When Engineer A merges to `master` first, Engineer B's earlier validation becomes stale relative to the moved target. Engineer B reconciles `master` into their worktree, resolves any Git conflicts, checkpoints the merge, and runs `decapod validate` on the exact merged commit before PR readiness.

## What shared governance does NOT guarantee (Negative Cases)

- **No conflict-free edits to identical files:** If two parallel branches edit the same lines of the same file, Decapod does not magically auto-merge them. Standard Git conflict resolution, human code review, CI, and fresh validation still apply.
- **Conflicting claims:** An active exclusive lease owned by Agent A blocks Agent B from claiming the same task until Agent A runs `todo handoff`, `todo yield`, `todo release`, or the lease expires.
- **Expired custody:** Letting a `--lease-seconds` window lapse without `decapod todo renew` exposes the claim as expired in `decapod todo fleet` and prevents stale custody from blocking progress forever.
- **Stale proof across concurrent merges:** A passing `decapod validate` run before an upstream `master` move or rebase does not authorize publication; revalidation against the exact current candidate and accepted target is mandatory.
- **Cloud access boundaries:** In `backend = "cloud"` mode, parallel engineers on separate machines share live task custody only when the hosted service authorizes each principal for that repository (`repository_denied` or `action_required` halts without falling back to local SQLite).

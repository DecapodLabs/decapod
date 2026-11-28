# Introduction

Decapod occupies a specific layer in software delivery:

```text
Models produce intelligence.
Agents perform work.
Repositories preserve state.
Decapod governs the transition from intent to proof.
```

Decapod is an **agent-agnostic, daemonless metacognitive governance kernel application**. Coding agents invoke its CLI and structured RPC interface to orient decisions against human intent, coordinate task custody, isolate workspaces, and validate exact candidate commits. Each call exits; `.decapod/` and Git preserve governance state and proof across agents and sessions.

## Bring Your Own Coding Agents (BYOA)

One engineering project rarely uses a single harness forever. A single engineer may switch between **Cursor** and **Claude Code** on the same repository, while teammates run **Codex**, **Antigravity**, or custom CLI harnesses in parallel.

Decapod treats agent tools as interchangeable consumers of a common repository kernel:

- **No proprietary harness or central chat UI:** Keep the coding agent and editor you already use.
- **No manual command choreography:** Humans run `decapod init` once per repository; coding agents invoke Decapod through its CLI and structured RPC interface (`decapod rpc`).
- **Shared durable substrate:** `.decapod/` preserves intent, plans, task claims, trajectories, living specifications, and validation receipts in the repository so any later agent can inspect what happened and carry unresolved obligations forward.

See [BYOA & Interoperability](concepts/interoperability.md) for the exact discovery and negotiation contract (`interop.manifest`, `interop.negotiate`, `interop.project`).

## Human → Agent → Kernel → Repository

| Participant | Responsibility |
| --- | --- |
| **Human** | Expresses engineering intent, supplies judgment at decision gates, and approves meaningful outcomes. |
| **Coding Agent** | Interprets the prompt and codebase, performs edits and tests, authors living specs, and invokes Decapod at decision, workspace, and validation boundaries. |
| **Decapod Kernel** | Exposes deterministic CLI and structured RPC operations for custody, orientation, bounded reflection, policy checks, validation, and publication gates. |
| **Repository** | Preserves `.decapod/` governance artifacts, Git history, worktrees, and commit-bound evidence across sessions, models, and harnesses. |

Like an operating system kernel, Decapod manages resources (task claims, lease generations, isolated Git worktrees, optional containers) and enforces invariants at transition boundaries without taking over user-space work.

## The metacognitive decision-time loop

Natural language alone does not make execution reliable. Transcripts live in transient tool sessions and disappear when an engineer switches agents or hits a failure. Decapod moves that control loop into explicit, decision-time kernel calls:

1. **Before inference and implementation:** The agent evaluates the prompt (`decapod eval --stdin --format json`), ingests contracts (`decapod docs ingest`), claims scoped custody (`decapod todo claim`), enters an isolated worktree (`decapod workspace ensure`), and retrieves constraints, plans, and optional bounded reflection challenges (`decapod infer orientation --task-id <id>`).
2. **During execution:** The agent updates living specifications (`.decapod/managed/specs/*`), records trajectory observations and checks (`decapod govern trajectory record`), and queries repository policy or the embedded Constitution when needed.
3. **At verification and remediation:** Validation (`decapod validate`) evaluates the exact candidate commit and accepted target. A failure returns actionable, typed diagnostics rather than silent completion; the agent repairs the root cause on the work branch, records a fresh checkpoint, and rechecks the exact revised commit.

This is **decision-time invocation and iterative correction** grounded in repository state—not model-level self-awareness or guaranteed automatic arbitration.

## Concurrency and explicit limits

Shared governance helps concurrent engineers and agents coordinate overlapping scope through exclusive/shared claims, expiring leases (`--lease-seconds`), `decapod todo fleet` overlap inspection, explicit handoffs (`decapod todo handoff`), and isolated Git worktrees or containers ([Workspace Isolation](workflows/workspace-isolation.md)).

Just as important is what Decapod **does not** replace:

- Shared governance **does not promise conflict-free concurrent edits to identical files** across separate branches.
- It **does not replace** Git conflict resolution, peer code review, CI, human decision gates, or independent exact-commit validation.

See [Multi-Agent Workflows](workflows/multi-agent.md) for concrete parallel-engineer coordination and conflict boundaries.

## Local and Cloud parity

Both storage modes remain **repo-native**:

- **Local (`repo.backend = "local"`):** Keeps the task and event datastore locally in `.decapod/data/decapod.db` alongside Git-tracked `.decapod/` governance files. Free, offline, and requires no account or network connectivity.
- **Cloud (`repo.backend = "cloud"`):** Routes supported task operations (`todo list/add/get/show/claim/release/done`) through Dactyl to the hosted **Decapod Cloud** service so engineers and agents on different machines share live task claims and custody, subject to verified repository and organization access (see [Cloud project routing](reference/cloud-projects.md)). Organization owners and admins manage repository enrollment, seat assignments, teammate access policies, and organization administration in [Organization Admin](https://decapodlabs.ai/admin).

Cloud mode does **not** replace Git, execute agents in a separate remote runtime, or waive worktree isolation and local/CI validation requirements. See [Cloud sign-in and task access](reference/propodus.md) for authorization and failure boundaries.

## Where the Constitution fits

Decapod includes an embedded, agent-queryable **Constitution** covering engineering golden paths, architecture, research, and software-business context, open to community PRs. It supplies defaults and reference context **inside** the governed loop (`decapod constitution get/search`, `decapod govern capsule query`). It is **not** a per-project instruction blob—project rules live in `.decapod/config.toml` and `.decapod/OVERRIDE.md`—and reading a Constitution entry is never proof of correctness. See [Constitution](concepts/constitution.md).

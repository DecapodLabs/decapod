# Mental Model

Decapod is an **agent-agnostic, repo-native governance application**. Its metacognitive kernel serves coding agents through on-demand CLI and structured RPC calls.

Humans express engineering intent and judgment through the coding agent they already use (Codex, Claude Code, Cursor, Antigravity, or another harness). **The coding agent invokes Decapod** through its CLI or Decapod-specific structured RPC interface (`decapod rpc`) whenever work crosses a custody, decision, boundary, or proof transition.

Each invocation ends without leaving a resident daemon. The repository—specifically `.decapod/` and Git history—preserves governance state. One engineering task can span processes, models, sessions, and harnesses without losing its recorded constraints or unresolved obligations.

## The Kernel Analogy

```text
Human (goal & approval)
  │
  ▼
Coding Agent (user space: reads code, plans, edits, runs tests, calls Decapod)
  │  CLI (`decapod ...`) / Structured RPC (`decapod rpc --op ...`)
  ▼
Decapod Kernel (ephemeral control plane: custody, orientation, policy, validation)
  │
  ▼
Repository (`.decapod/` + Git: plans, specs, claims, worktrees, checkpoints, proof)
```

- **Resources:** Decapod manages task claims and lease generations (`todo`), isolated Git worktrees and optional containers (`workspace`), and normalized governance records (`.decapod/governance.json`).
- **Interface:** Agents call Decapod through CLI commands or its Decapod-specific structured RPC interface (`decapod rpc --op <op> --params <json>` or `--stdin`) to acquire state, test proposed scopes, and validate candidates.
- **Isolation:** Each claimed task works on its own worktree branch, keeping the root checkout (`master`/`main`) clean and preventing uncommitted file collisions between parallel agents.
- **Continuity:** When a second agent resumes the work—or a human inspects a PR—plans, living specifications, trajectory checks, and validation receipts live in repository state rather than a vanished chat transcript.

## The metacognitive decision-time loop

Agents do not call Decapod for every keystroke, nor does Decapod inspect model weights or provide magical self-awareness. Instead, agents invoke Decapod at **decision and state-transition boundaries** to ground inference before acting and verify results before claiming completion:

1. **Prompt Gate & Contract Ingestion:** Before reading repository files or mutating state, the agent evaluates the prompt (`decapod eval --stdin --format json`), ingests current command contracts (`decapod docs ingest`), and acquires a session (`decapod session acquire`).
2. **Custody & Isolation Pressure:** *"I need exclusive ownership and a clean branch before touching files."* The agent claims or adds a task (`decapod todo claim --id <id> --lease-seconds 3600`), checks fleet overlaps (`decapod todo fleet`), and enters an isolated worktree (`decapod workspace ensure --branch ...`).
3. **Orientation & Reflection Pressure:** *"Before implementing, what constraints, specs, and risks govern this change?"* The agent calls `decapod infer orientation --task-id <id>` (and optionally `--reflection '<json>'` or `decapod rpc --op mentor.obligations` at consequential decisions) to surface plan constraints, forbidden paths, living specs, and escalation admissibility across a bounded three-round reflection loop. Reference knowledge from the embedded [Constitution](concepts/constitution.md) (`decapod constitution get` / `decapod govern capsule query`) informs decisions inside this step.
4. **Execution & Trajectory Custody:** As the agent works, it updates `.decapod/managed/specs/*`, records inspected/modified files and checks (`decapod govern trajectory record`), reviews changed contracts (`decapod govern plan review-spec`), and binds staged material with `decapod govern artifacts checkpoint` before committing.
5. **Validation & Remediation Pressure:** *"Does the exact candidate commit pass all required gates against the accepted target?"* The agent runs `decapod validate`. If validation fails, the kernel returns typed error codes and recovery guidance rather than marking work done. The agent repairs the root cause on the work branch, checkpoints and commits the revision, and re-runs `decapod validate` on the exact new commit.
6. **Publication Pressure:** Only when required gates and proof pass for the exact candidate does the work move to `decapod workspace publish` and `decapod todo done --id <id> --validated`.

```text
prompt gate → claim & isolate → orient & reflect → implement & record trajectory
      → checkpoint & commit → validate exact commit
      → [if failed: repair → checkpoint & commit → revalidate exact commit]
      → publish & mark done
```

## Epistemic Custody

A central invariant in Decapod is **Epistemic Custody**: preserving the falsifiable chain between the human's original intent, the context and constraints retrieved before inference, the assumptions and unresolved obligations carried during execution, and the exact-commit proof required for completion (see [Artifact Reference](reference/artifacts.md) and [Governance Artifact Inventory](reference/governance-artifacts.md)).

## What Decapod does not guarantee

Clear boundaries keep governance trustworthy:

- **No always-on daemon or shell interception:** Decapod cannot force an arbitrary harness to call it, nor can it intercept direct `git` or filesystem edits made outside Decapod's CLI/RPC boundaries.
- **No automatic merge arbitration:** Parallel worktrees isolate uncommitted work, but if two agents edit the same lines in separate branches, standard Git merge conflict resolution, code review, and re-validation still apply.
- **No self-certifying testimony or stale proof reuse:** Reading the Constitution, writing a living spec, or declaring a check `"passed"` in trajectory input is never validation proof. Editing code or rebasing invalidates prior proof; `decapod validate` must pass on the exact revised commit.
- **No unverified Cloud authority:** In `backend = "cloud"` mode, `DECAPOD_AGENT_ID` is a cooperative coordination label, not cryptographic authorization; the hosted service independently authorizes repository access on every request.

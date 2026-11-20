# Single-Agent & Handoff Workflow

Even when a single engineer drives the work, Decapod prevents context loss, silent scope drift, and unverified completion claims—and makes it effortless to switch coding agents mid-task (see [Agent-First Architecture](../concepts/agent-first.md) and [BYOA & Interoperability](../concepts/interoperability.md)).

## The Decision-Time Loop

The human describes the goal to their preferred coding agent. The agent invokes Decapod at decision and state boundaries:

1. **Prompt Gate & Session Custody:**
   ```bash
   printf '%s' '<incoming prompt>' | decapod eval --stdin --format json
   decapod docs ingest
   decapod session acquire
   ```
2. **Task Claim & Workspace Isolation:**
   ```bash
   decapod todo claim --id <task-id> --lease-seconds 3600
   decapod workspace ensure --branch agent/<agent-id>/<task-id>-<topic>
   cd <reported-worktree-path>
   ```
3. **Orientation & Bounded Reflection:** Before editing source files, the agent gathers constraints, plan state, and optional reflection checks:
   ```bash
   decapod infer orientation --task-id <task-id>
   decapod rpc --op context.resolve
   # Optional at risky or forbidden-path boundaries:
   decapod infer orientation --task-id <task-id> --reflection '<request-json>'
   ```
   When engineering tradeoffs or doctrine apply, the agent queries the embedded [Constitution](../concepts/constitution.md) (`decapod constitution get core/DECAPOD`).
4. **Implementation, Trajectory & Checkpoint:** Inside the worktree, the agent updates code and living specs (`.decapod/managed/specs/*`), records trajectory evidence, reviews changed contracts, and checkpoints staged material before committing:
   ```bash
   decapod govern trajectory record --run-id <run-id> --modified-file <path> --check "unit=passed"
   decapod govern plan review-spec --path .decapod/managed/specs/ARCHITECTURE.md --disposition updated --reason "..."
   git add <authored-paths>
   decapod govern artifacts checkpoint --id <checkpoint-id> --summary "What changed and was verified"
   git add .decapod/governance.json
   git commit -m "Describe the governed change"
   ```
5. **Exact-Commit Validation & Failure Recovery:**
   ```bash
   decapod validate
   ```
6. **Publication & Completion:** Only after required gates pass for the exact candidate commit:
   ```bash
   decapod workspace publish
   decapod todo done --id <task-id> --validated
   ```

## Concrete journey: switching between two agents on one repo

An engineer starts a bugfix in **Cursor** or **Codex**, then switches to **Claude Code** or **Antigravity** to finish and validate it:

1. **Agent A** records the plan, claims `<task-id>`, works in the isolated worktree, and either runs `decapod todo handoff --id <task-id> --to <agent-b> --summary "Parser fix implemented; unit tests still failing"` or `decapod todo yield --id <task-id>`. Unresolved obligations and checkpoints remain in `.decapod/governance.json` and `.decapod/managed/specs/*`.
2. **Agent B** runs `decapod session acquire`, claims or resumes `<task-id>`, and inspects `decapod todo get --id <task-id>`, `decapod govern artifacts status`, and `decapod infer orientation --task-id <task-id>`.
3. Agent B inherits the exact scope, prior edits, and remaining proof gaps without asking the human to re-explain the whole project—and still must pass `decapod validate` on its own final commit.

## Concrete journey: recovering from failed verification

When `decapod validate` fails on a candidate commit:

1. **Do not claim completion or bypass gates:** Validation exits nonzero and prints typed error codes and recovery guidance (see [Proof & Validation](../concepts/proof.md) and [Error Handling](../reference/errors.md)).
2. **Fix the root cause on the same work branch:** The agent inspects the failure, repairs the code, spec, or governance artifact inside the claimed worktree, refreshes specs if needed (`decapod rpc --op specs.refresh`), and records a new trajectory entry and checkpoint before committing the fix.
3. **Recheck the exact revised commit:** Prior proof from before the edit is stale. The agent re-runs `decapod validate` against the new `HEAD` commit and accepted target, publishing only after the revised commit passes.

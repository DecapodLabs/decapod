# TODO.md - TODO Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

**Quick Reference:**
| Command | Purpose |
|---------|---------|
| `decapod todo add "title" --priority high` | Create task |
| `decapod todo list` | List all tasks |
| `decapod todo done <id>` | Mark complete |
| `decapod todo archive <id>` | Archive (REQUIRED) |

**Related:** `core/PLUGINS.md` (subsystem registry) | `AGENTS.md` (entrypoint)

---

## CLI Surface

```bash
decapod todo add "<title>" [--priority high|medium|low] [--tags <tags>] [--owner <owner>]
decapod todo list [--status open|done|archived] [--scope <scope>] [--tags <tags>]
decapod todo get --id <id>
decapod todo done --id <id>
decapod todo archive --id <id>
decapod todo claim --id <id> [--agent <agent-id>]
decapod todo register-agent --agent <agent-id> --category <name> [--category <name>]
decapod todo ownerships [--category <name>] [--agent <agent-id>]
decapod todo heartbeat [--agent <agent-id>]
decapod todo presence [--agent <agent-id>]
decapod todo schema  # JSON schema for programmatic use
```

## Task Lifecycle & Agent Obligations

All tasks track three timestamps:
- **created_at**: When the task was created
- **completed_at**: When the task was marked done (via `decapod todo done`)
- **closed_at**: When the task was archived (via `decapod todo archive`)

### Agent Requirement: Close Completed Tickets

**As an AI agent, you MUST close out tickets you complete.**

When you finish work on a task:
1. Mark it done: `decapod todo done <task-id>`
2. Archive it: `decapod todo archive <task-id>`

This ensures proper audit trails and lifecycle tracking. Tasks left in "done" state without being archived create ambiguity about whether the work is truly complete and ready for archival.

### Workflow

```bash
# 1. Create a task (from AGENTS.md §)
decapod todo add "Implement feature X" --priority high

# 2. Do the work...
# ... implementation ...

# 3. Mark as done (sets completed_at)
decapod todo done R_XXXXXXXX

# 4. Archive (sets closed_at) - REQUIRED
decapod todo archive R_XXXXXXXX
```

**Rule**: If you mark a task done, you must also archive it unless explicitly instructed otherwise.

---

## Multi-Agent Coordination

The TODO subsystem coordinates multiple agents using category ownership plus heartbeats.

### Ownership model

- Agents claim category ownership via `decapod todo register-agent`.
- Category ownership is durable and queryable via `decapod todo ownerships`.
- New tasks auto-assign to the active owner of their inferred category.

### Presence model

- Agents publish liveness via `decapod todo heartbeat`.
- Presence state is visible via `decapod todo presence`.
- Ownership checks treat missing/stale presence as inactive.

### Timeout eviction (30 minutes)

- If category owner heartbeat is stale for more than 30 minutes, another agent can claim work in that category.
- On successful claim, ownership transfers to the claiming agent.
- This prevents abandoned ownership from blocking progress.

---

## Pre-TODO Audit Requirement

**Binding: Yes**

Before creating or modifying any TODO (via `decapod todo add`, `decapod todo done`, `decapod todo archive`, or any TODO mutation), agents MUST:

1. Run `decapod validate` to audit system state
2. Review validation results for any failures
3. Address critical issues before proceeding with TODO operations
4. Document any intentional exceptions in the TODO description

**Rationale:** TODO operations mutate shared state. System audits ensure integrity before mutations occur, preventing corrupted state from being propagated through the task lifecycle.

---

## State Transition Validation

Every lifecycle enum must have an explicit transition table. Invalid transitions must be rejected with an error, not silently ignored.

### Valid Transitions

```
pending  → active     (start work)
pending  → archived   (skip/cancel)
active   → done       (complete work)
active   → pending    (revert/reassign)
done     → archived   (close out)
```

All other transitions are invalid and must produce an error.

### Transition Discipline

1. **Explicit transition tables**: Every state enum must define `can_transition_to()` with an exhaustive match.
2. **Reject invalid transitions**: Return an error with the current state, target state, and valid alternatives — never silently ignore.
3. **Transition history**: Every state change must be recorded in the event log with a `reason` field. The reason should explain *why* the transition happened, not just *what* changed.
4. **Bounded history**: Cap transition history at a reasonable limit (e.g., 200 entries per task) to prevent unbounded growth.

---

**See also:** `core/PLUGINS.md` for subsystem registry and truth labels.

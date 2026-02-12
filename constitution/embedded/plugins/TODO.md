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

**Related:** `embedded/core/PLUGINS.md` (subsystem registry) | `AGENTS.md` (entrypoint)

---

## CLI Surface

```bash
decapod todo add "<title>" [--priority high|medium|low] [--tags <tags>] [--owner <owner>]
decapod todo list [--status open|done|archived] [--scope <scope>] [--tags <tags>]
decapod todo get --id <id>
decapod todo done --id <id>
decapod todo archive --id <id>
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

**See also:** `embedded/core/PLUGINS.md` for subsystem registry and truth labels.


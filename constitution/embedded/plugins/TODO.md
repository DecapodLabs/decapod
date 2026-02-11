# TODO.md - TODO Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

This document defines the todo subsystem.

## CLI Surface
- `decapod todo ...`

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
# 1. Create a task
decapod todo add "Implement feature X" --priority high

# 2. Do the work...
# ... implementation ...

# 3. Mark as done (sets completed_at)
decapod todo done <task-id>

# 4. Archive (sets closed_at) - REQUIRED
decapod todo archive <task-id>
```

**Rule**: If you mark a task done, you must also archive it unless explicitly instructed otherwise.


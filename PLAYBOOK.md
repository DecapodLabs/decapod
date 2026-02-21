# PLAYBOOK.md — Decision Frameworks and Failure Modes

## When Stuck: Triage Flow

```
1. Is the task clear?
   NO  → Re-read the task description. Check `decapod todo get --id <id>`.
         Still unclear? Ask the user. Do not guess.

2. Does `decapod validate` pass?
   NO  → Fix validation failures first. They are the authoritative gate.
         Read the failure messages — they tell you exactly what's wrong.

3. Do tests pass?
   NO  → Fix failing tests. Cite the test name and error.
         Do not disable tests to make progress.

4. Is the change on the right branch?
   NO  → `decapod workspace ensure`. Never work on main/master.

5. Is the scope creeping?
   YES → Stop. Finish the current scope. File new tasks for extras.

6. Is the approach getting hacky?
   YES → Stop. Revisit the plan. Consider a simpler approach.
```

## Decision Frameworks

### Should I Create a New File?

```
Can I accomplish the goal by editing an existing file?
  YES → Edit the existing file.
  NO  → Is the new file required for the task?
    YES → Create it. Follow existing naming conventions.
    NO  → Do not create it.
```

### Should I Add a Dependency?

```
Does an existing dependency already cover this?
  YES → Use the existing dependency.
  NO  → Is the dependency well-maintained and small?
    YES → Add it to Cargo.toml. Run `cargo update`. Commit Cargo.lock.
    NO  → Can I implement the needed functionality in < 50 lines?
      YES → Implement it inline.
      NO  → Add the dependency, but document why in the commit message.
```

### Should I Refactor Surrounding Code?

```
Was the refactoring explicitly requested?
  YES → Do it.
  NO  → Is the surrounding code blocking the current task?
    YES → Refactor the minimum needed to unblock.
    NO  → Do not refactor. File a separate task if it's important.
```

### Core vs Plugin?

```
Does the change affect state integrity, validation, or the broker?
  YES → Core change. Requires extra tests. Keep minimal.
  NO  → Plugin change. This is where 90% of work happens.
```

## Common Failure Modes

### "I'll just quickly fix this too"
**Problem**: Scope creep. Unrelated changes mixed into a task.
**Fix**: One task, one scope. File new tasks for discovered issues.

### "The tests are too strict"
**Problem**: Tests encode invariants. Weakening them is a regression.
**Fix**: If a test is wrong, explain why and fix the test. If the test is right, fix your code.

### "I need to restructure everything first"
**Problem**: Premature abstraction. Over-engineering before understanding.
**Fix**: Make it work, make it right, make it fast — in that order. Ship the smallest correct change.

### "decapod validate is failing on something unrelated"
**Problem**: Existing drift in the repo.
**Fix**: If truly unrelated, note it and file a task. Do not ignore it. Do not disable the gate.

### "I can't test this change"
**Problem**: Missing test infrastructure.
**Fix**: Add the test. Even a smoke test is better than no test. Mark untestable claims as `partially_enforced`.

### "The session expired"
**Problem**: Decapod sessions have TTLs.
**Fix**: Run `decapod session acquire` again. Re-export the environment variables.

## Evidence Standards

When claiming a task is done, provide:

1. **What changed**: File paths and line ranges.
2. **Why it changed**: Link to task/issue/spec.
3. **Proof**: Which tests pass. Which gates are green. Exact command + output.
4. **Gaps**: What is NOT covered. What remains aspirational.

Example:
```
Changed: src/core/validate.rs:45-62
Why: Fixes #123 — namespace purge gate was not checking plugins/
Proof: `cargo test --locked test_namespace_purge` passes (was failing)
       `decapod validate` passes (was failing on namespace gate)
Gaps: Does not cover dynamically loaded plugins (filed as task R_xxx)
```

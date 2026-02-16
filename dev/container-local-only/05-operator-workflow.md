# Operator Workflow: Local-Only Container Branch Cycle

## Command
```bash
decapod auto container run \
  --agent <agent-id> \
  --branch <local-branch> \
  --local-only \
  --cmd "<work command>"
```

## Behavior
- Creates isolated workspace clone from local repo only.
- Checks out requested branch from local refs.
- Executes command in container.
- Commits workspace changes when dirty.
- Syncs branch back to host repo as local branch.
- Never fetches/pushes remotes.

## Constraints
- `--local-only` cannot be combined with `--push` or `--pr`.

## Verification
- `git branch --list <local-branch>` on host should show returned branch.

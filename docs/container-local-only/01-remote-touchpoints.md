# Remote Touchpoints in Current Container Flow

## Host-side remote touchpoints (pre-patch behavior)
- `prepare_workspace_clone()` executed `git fetch origin <base>`.
- `prepare_workspace_clone()` resolved `git remote get-url origin`.
- Clone source was remote URL (`git clone ... <origin_url>`).

## In-container remote touchpoints (pre-patch behavior)
- `ssh-keyscan github.com` for known hosts.
- `git fetch origin <base>` and `git rebase origin/<base>`.
- Optional `git push -u origin <branch>`.
- Optional `gh pr create ...`.

## Why these failed in this environment
- GitHub SSH auth was not available in container runtime path (`Permission denied (publickey)`).
- Workflow design coupled normal execution to remote availability.

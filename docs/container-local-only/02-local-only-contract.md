# Local-Only Container Contract

## Objective
Run container workflow with zero remote Git network operations.

## Rules
- Clone source MUST be local host repository path.
- `--local-only` MUST reject `--push` and `--pr`.
- In-container script MUST avoid remote fetch/rebase/push/PR and `ssh-keyscan`.
- Branch work must be checked out from local refs only.
- On successful run, resulting branch MUST be synced back into host repo as a local branch.

## Non-goals
- No remote synchronization.
- No PR automation in local-only mode.

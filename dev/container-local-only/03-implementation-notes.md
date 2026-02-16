# Local-Only Implementation Notes

## Code changes
- Added CLI flag: `--local-only` to `decapod auto container run`.
- Added validation gate in `run_container()` rejecting `--push/--pr` when `--local-only` is set.
- Added local clone backend in `prepare_workspace_clone(..., local_only)` using:
  - `git clone --local --no-hardlinks <repo> <workspace>`
  - local branch checkout from `refs/heads/<base>` or `refs/remotes/origin/<base>` fallback.
- Added host sync step:
  - `sync_workspace_branch_to_host_repo()`
  - uses local fetch from workspace path to update `refs/heads/<branch>` in host repo.
- Added container env marker: `DECAPOD_LOCAL_ONLY=1`.
- Updated in-container script builder to skip remote fetch/rebase/push/PR and skip `ssh-keyscan` when local-only.

## Files touched
- `src/plugins/container.rs`
- `tests/cli_contracts.rs`
- `constitution/plugins/CONTAINER.md`

# Test Evidence

## Automated tests run
1. `cargo test -q --test cli_contracts container_help_schema_and_docs_stay_in_sync`
- Result: pass

2. `cargo test -q docker_spec_contains_safety_flags_and_sdlc_steps`
- Result: pass

3. `cargo test -q docker_spec_local_only_avoids_remote_git_operations`
- Result: pass
- Verifies generated container script in local-only mode omits:
  - `git fetch origin`
  - `git rebase origin/...`
  - `git push origin`
  - `gh pr create`
  - `ssh-keyscan github.com`

## Runtime smoke status
- A full docker runtime smoke remains environment-sensitive here (long-running/hangs observed under this harness).
- Evidence of local workspace and branch behavior exists in host `.decapod/workspaces/` and local branch artifacts.

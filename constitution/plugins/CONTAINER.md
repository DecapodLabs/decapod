# CONTAINER.md - CONTAINER Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

Container subsystem runs agent actions in ephemeral Docker/Podman containers with isolated git clone workspaces.

## CLI Surface
- `decapod auto container run --agent <id> --cmd "<command>"`
- Optional branch/task controls: `--branch`, `--task-id`, `--pr-base`
- Optional SDLC automation: `--push`, `--pr`, `--pr-title`, `--pr-body`
- Optional runtime profile: `--image-profile debian-slim|alpine`
- Optional hard overrides: `--image`, `--memory`, `--cpus`, `--timeout-seconds`, `--repo`
- Optional lifecycle/env controls: `--keep-worktree`, `--inherit-env`
- `decapod data schema --subsystem container`

## Contracts
- One container per invocation (`--rm`), then teardown.
- Before each run, Decapod fetches `origin/<base>` (default `origin/master`) and creates an isolated clone workspace under `.decapod/workspaces/`.
- Container mounts that worktree at `/workspace`; user can remain on local `master`.
- Container always mounts repo control-plane state at `/workspace/.decapod` so in-container build/test can run Decapod commands against shared state.
- Decapod manages a generated Dockerfile at `.decapod/generated/Dockerfile` for `--image-profile alpine`.
- In-container script syncs from base (`fetch` + `rebase`), executes command, optionally commit/push/PR.
- Local environment is inherited by default (`--inherit-env`), including SSH agent passthrough when present.
- Safety defaults: cap-drop all, no-new-privileges, pids limit, tmpfs `/tmp`.
- Runtime selection auto-detects `docker` first, then `podman`.
- Host UID/GID is mapped when available so file ownership remains correct.
- Generated image expansion policy:
- Start from minimal Alpine.
- Add only stack packages inferred from repo markers (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`).
- Accept operator overrides via `DECAPOD_CONTAINER_APK_PACKAGES`.

## Operator Runbook
1. Run isolated task worktree from master:
   `decapod auto container run --agent clawdious --task-id R_01ABC --cmd "cargo test -q"`
2. Complete SDLC in one run (commit/push/PR):
   `decapod auto container run --agent clawdious --task-id R_01ABC --push --pr --pr-title "Fix R_01ABC" --cmd "cargo test -q"`.
3. Use lightweight profile when needed:
   `decapod auto container run --agent clawdious --image-profile alpine --cmd "cargo check -q"`.
4. Keep worktree for postmortem debugging:
   `decapod auto container run --agent clawdious --task-id R_01ABC --keep-worktree --cmd "..."`
5. Inspect generated image template:
   `.decapod/generated/Dockerfile`

Expected loop:
- Agent claims TODO.
- Claim autorun starts isolated container branch from `origin/master`.
- Command exits with JSON envelope, then worktree is removed unless `--keep-worktree` is set.
- Optional push + PR closes the ephemeral loop.

## Permission Note
- Shared `.git/worktrees` backends can fail in containerized runs with daemon/user namespace permission errors (for example, `FETCH_HEAD` lock/write failures).
- Clone workspace isolation avoids these shared git metadata writes and is the default strategy.

## Claim Autorun
- `todo claim` (exclusive mode) can automatically launch container execution for claimed task.
- Guard rails:
- Disabled inside container recursion (`DECAPOD_CONTAINER=1`).
- Toggle with `DECAPOD_CLAIM_AUTORUN` (`true` default).
- Configure defaults with `DECAPOD_CLAIM_CMD`, `DECAPOD_CLAIM_PUSH`, `DECAPOD_CLAIM_PR`.

## Proof Surfaces
- Command output envelope includes runtime, container name, branch/base, exit code, elapsed seconds.
- `todo claim` output includes nested `container` result when autorun is attempted.
- Schema: `decapod data schema --subsystem container`

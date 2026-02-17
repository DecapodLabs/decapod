# CONTAINER.md - CONTAINER Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

Container subsystem runs agent actions in ephemeral Docker/Podman containers with isolated git clone workspaces.

## CLI Surface
- `decapod auto container run --agent <id> --cmd "<command>"`
- Optional branch/task controls: `--branch`, `--task-id`, `--pr-base`
- Compatibility flags (disabled in local-workspace mode): `--push`, `--pr`, `--pr-title`, `--pr-body`
- Optional runtime profile: `--image-profile debian-slim|alpine`
- Optional hard overrides: `--image`, `--memory`, `--cpus`, `--timeout-seconds`, `--repo`
- Optional lifecycle/env controls: `--keep-worktree`, `--inherit-env`
- Local-workspace execution is mandatory; `--local-only` remains accepted for compatibility.
- `decapod data schema --subsystem container`

## Contracts
- One container per invocation (`--rm`), then teardown.
- Container workspace is always cloned from local repo state in the control-plane workspace area.
- Container runtime performs zero remote Git network operations (no fetch/pull/push/PR in-container).
- Container mounts only the isolated workspace plus shared host `.decapod` state volume.
- Repo root is not mounted directly; this avoids agents contending on the same live branch/worktree mount.
- Overlay workspace is branched from base (`master` by default), so container edits happen in isolation.
- On success, the workspace branch is folded back into host repo refs via local fetch from workspace clone.
- Decapod manages a generated Dockerfile template for `--image-profile alpine`.
- In-container script checks out branch from local refs, executes command, and optionally commits.
- Local environment is inherited by default (`--inherit-env`) for non-Git-network runtime context.
- Safety defaults: cap-drop all, no-new-privileges, pids limit, tmpfs `/tmp`.
- Runtime selection auto-detects `docker` first, then `podman`.
- Runtime access is preflight-validated (`docker|podman info`) before workspace/image steps; permission or daemon failures return actionable diagnostics.
- Host UID/GID mapping is on by default (`DECAPOD_CONTAINER_MAP_HOST_USER=true`) so file ownership stays writable on host.
- Generated image expansion policy:
- Start from minimal Alpine.
- Add only stack packages inferred from repo markers (`Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`).
- Accept operator overrides via `DECAPOD_CONTAINER_APK_PACKAGES`.

## Operator Runbook
1. Run isolated task worktree from master:
   `decapod auto container run --agent clawdious --task-id R_01ABC --cmd "cargo test -q"`
2. Run command and fold branch back to host repo refs:
   `decapod auto container run --agent clawdious --task-id R_01ABC --cmd "cargo test -q"`.
3. Use lightweight profile when needed:
   `decapod auto container run --agent clawdious --image-profile alpine --cmd "cargo check -q"`.
4. Keep worktree for postmortem debugging:
   `decapod auto container run --agent clawdious --task-id R_01ABC --keep-worktree --cmd "..."`
5. Local-workspace mode is default and mandatory (flag is compatibility only):
   `decapod auto container run --agent clawdious --task-id R_01ABC --local-only --cmd "cargo test -q"`
6. Inspect generated image template from the control-plane generated output.

Expected loop:
- Agent claims TODO.
- Claim autorun starts isolated container branch from local `master` (or local fallback ref).
- Shared `.decapod` state remains mounted for coordination and proofs.
- Command exits with JSON envelope, then worktree is removed unless `--keep-worktree` is set.
- Host-side Git operations (push/PR) happen after branch foldback, outside container run.

## Permission Note
- Shared `.git/worktrees` backends can fail in containerized runs with daemon/user namespace permission errors (for example, `FETCH_HEAD` lock/write failures).
- Clone workspace isolation avoids these shared git metadata writes and is the default strategy.

## Claim Autorun
- `todo claim` (exclusive mode) can automatically launch container execution for claimed task.
- Guard rails:
- Disabled inside container recursion (`DECAPOD_CONTAINER=1`).
- Toggle with `DECAPOD_CLAIM_AUTORUN` (`true` default).
- Configure defaults with `DECAPOD_CLAIM_CMD`; claim push/PR toggles are compatibility-only and disabled by local-workspace contract.

## Proof Surfaces
- Command output envelope includes runtime, container name, branch/base, exit code, elapsed seconds.
- `todo claim` output includes nested `container` result when autorun is attempted.
- Schema: `decapod data schema --subsystem container`

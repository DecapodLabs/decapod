# CONTAINER.md - CONTAINER Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

Container subsystem runs agent actions in ephemeral Docker/Podman containers with repository mount isolation.

## CLI Surface
- `decapod auto container run --agent <id> --cmd "<command>"`
- Optional: `--branch <name> --push`
- Optional runtime profile: `--image-profile debian-slim|alpine`
- Optional hard overrides: `--image`, `--memory`, `--cpus`, `--timeout-seconds`, `--repo`
- `decapod data schema --subsystem container`

## Contracts
- One container per invocation (`--rm`), then teardown.
- Repo is mounted at `/workspace`; no background daemon lifecycle is required.
- Branch isolation is explicit with `--branch`; push is gated by `--push`.
- Safety defaults: cap-drop all, no-new-privileges, pids limit, tmpfs `/tmp`.
- Runtime selection auto-detects `docker` first, then `podman`.
- Host UID/GID is mapped when available so file ownership remains correct.
- `SSH_AUTH_SOCK` is passed through only when present, enabling optional authenticated `git push`.

## Operator Runbook
1. Create branch-scoped execution:
   `decapod auto container run --agent clawdious --branch ahr/feature --cmd "cargo test -q"`
2. Allow push from isolated run:
   `decapod auto container run --agent clawdious --branch ahr/feature --push --cmd "cargo test -q && git add -A && git commit -m 'feat: update'"`.
3. Use lightweight profile when needed:
   `decapod auto container run --agent clawdious --image-profile alpine --cmd "cargo check -q"`.
4. Enforce tighter limits for risky workloads:
   `decapod auto container run --agent clawdious --memory 1g --cpus 1.0 --timeout-seconds 600 --cmd "..."`

Expected loop:
- Human trigger selects command and branch.
- Agent runs containerized command.
- Command exits, emits JSON envelope, container is removed.
- Follow-up decisions/knowledge updates stay in normal Decapod control-plane flow.

## Proof Surfaces
- Command output envelope includes runtime, container name, exit code, elapsed seconds.
- Schema: `decapod data schema --subsystem container`

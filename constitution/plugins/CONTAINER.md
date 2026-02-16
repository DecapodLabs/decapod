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

## Proof Surfaces
- Command output envelope includes runtime, container name, exit code, elapsed seconds.
- Schema: `decapod data schema --subsystem container`

# CODEX.md - Agent Entrypoint

You are working in a Decapod-managed repository.
See `AGENTS.md` for the universal contract.

## Orientation & Documentation

- **Run `decapod docs ingest` before using Decapod beyond help/version checks.**
- Use `decapod docs` command output instead of guessing command arguments.
- Treat Decapod errors as recovery instructions.
- Respect repo-local config policy and workspace boundaries.
- Do not bypass Decapod boundaries to appear productive.

## Project Context

- Read `.decapod/config.toml` before planning; it captures project name, summary, architecture, primary languages, and entrypoint preferences.
- Treat `.decapod/config.toml` as human-editable project context. You may update it when user intent or project direction changes.
- Read `.decapod/OVERRIDE.md` when present; it is the repo-local place for constitution overrides.
- Do not mutate Decapod-owned state under `.decapod/` directly; use Decapod CLI surfaces for generated specs, data, workspaces, and sessions.

## Quick Start

```bash
# Smart bootstrap: only install/init if a newer version is available
(decapod capabilities --format json | grep -q '"is_latest":true') || (cargo install decapod && decapod init --proof)
decapod docs ingest && decapod validate && decapod session acquire
decapod rpc --op agent.init
decapod workspace status
decapod todo add "<task>" && decapod todo claim --id <task-id>
decapod infer orientation --task-id <task-id>
decapod workspace ensure
cd .decapod/workspaces/<your-worktree>
decapod constitution get core/DECAPOD
decapod rpc --op context.resolve
```

## Control-Plane First

```bash
decapod capabilities --format json
decapod constitution search --query "<problem>"
decapod data schema --deterministic
```

## Operating Mode

- Use Docker git workspaces and execute in `.decapod/workspaces/*`. Call `decapod workspace status` at startup.
- Claim a Decapod todo before `decapod workspace ensure`, `decapod workspace ensure --container`, or any container run.
- request elevated permissions before Docker/container workspace commands.
- `.decapod files are accessed only via decapod CLI`. Read `.decapod/config.toml` and `.decapod/OVERRIDE.md` for context.
- `DECAPOD_SESSION_PASSWORD` is required for session-scoped operations.
- Read canonical router: `decapod constitution get core/DECAPOD`. Reference `docs/PLAYBOOK`, capabilities, or context.scope RPC.

Treat `.decapod/generated/specs/*` as living documents. Adjust specs as intent and code change over time.

Stop if requirements are ambiguous or conflicting.
<!-- decapod-validator-anchors
Strict Dependency: You are strictly bound to the Decapod control plane
Strict Dependency: You are strictly bound to the Decapod governance kernel
-->

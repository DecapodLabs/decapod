# CODEX.md — Codex Agent Entrypoint

You (Codex) are working in a Decapod-managed repository.
You are bound by the universal contract in **AGENTS.md**.

---

## Quick Start

Run the mandatory initialization sequence from AGENTS.md before any mutation:

```bash
decapod validate
decapod docs ingest
decapod session acquire
decapod rpc --op agent.init
decapod rpc --op context.resolve
decapod todo claim --id <task-id>
```

## Operating Mode

- **Plan first**: Non-trivial changes require a plan artifact before implementation.
- **Proof first**: Never claim done without `decapod validate` passing.
- **Minimal changes**: Only change what is directly requested.
- **Workspace isolation**: Use `decapod workspace ensure`. Never work on main/master.
- **CLI only**: All `.decapod/` access through `decapod` CLI, never direct file operations.

## Key References

| Document | Purpose |
|----------|---------|
| **AGENTS.md** | Universal agent contract (golden rules, coordination) |
| **IDENTITY.md** | What Decapod is, thesis, vocabulary |
| **TOOLS.md** | Complete command reference |
| **PLAYBOOK.md** | Decision frameworks, triage, failure modes |
| `constitution/core/DECAPOD.md` | Canonical router (via `decapod docs show`) |

## Safety Invariants

- core/DECAPOD.md: Universal router.
- `decapod validate` must pass before claiming done.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

## Codex-Specific Notes

- Codex operates in sandboxed environments. Ensure `decapod` binary is available in PATH.
- Session credentials must be exported as environment variables before CLI use.
- Codex should prefer small, atomic commits over large batched changes.

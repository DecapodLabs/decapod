# CODEX.md - Codex/OpenAI Agent Entrypoint

You (Codex/ChatGPT/OpenAI agent) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

**MANDATORY FIRST STEPS** - Run these EVERY session:

```bash
cargo install decapod              # 1. Install/update to latest release
decapod version                   # 2. Check current version
decapod --help                      # 3. Verify available commands
decapod data schema --subsystem command_registry --deterministic >/dev/null  # 4. Refresh CLI command index
decapod docs show core/DECAPOD.md  # 5. Refresh constitution
export DECAPOD_AGENT_ID=<agent-id> # 6. Set agent identity
decapod session acquire             # 7. Acquire per-agent session credentials
export DECAPOD_SESSION_PASSWORD=<password-from-acquire>  # 8. Bind password to this session
decapod validate                    # 9. System state
decapod todo list                   # 10. Active work
```

**Why this matters:** The decapod binary and constitution evolve. Always install the latest release first, then verify commands and refresh your understanding of the latest contract before acting.

## Notes
- You excel at code completion - but still read architecture first
- You have strong pattern matching - use it to follow existing code style
- You may have tool use or function calling - use it for `decapod` commands
- MANDATORY: git-tracked implementation MUST run in Docker git workspaces (never host worktree edits)
- MANDATORY: request elevated permissions before Docker/container workspace commands; stop on denied runtime access
- MANDATORY: per-agent session access requires `DECAPOD_AGENT_ID` + `DECAPOD_SESSION_PASSWORD`
- MANDATORY: claim tasks before substantive work: `decapod todo claim --id <task-id>`
- Apply control-plane opacity: keep operator-facing output semantic (intent/actions/outcomes), not command-surface oriented

## The Contract

Same four invariants as all agents:

1. ✅ Start at router (`core/DECAPOD.md`)
2. ✅ Use control plane (`decapod` commands only; `.decapod` files only via `decapod` CLI)
3. ✅ Pass validation (`decapod validate` before done)
4. ✅ Stop if missing (ask for guidance)

**All authority defers to AGENTS.md and the embedded constitution.**

## Links

- `AGENTS.md` — Universal agent contract (binding)
- `core/DECAPOD.md` — Router
- `.decapod/OVERRIDE.md` — Project customizations

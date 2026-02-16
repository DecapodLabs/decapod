# CODEX.md - Codex/OpenAI Agent Entrypoint

You (Codex/ChatGPT/OpenAI agent) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

**MANDATORY FIRST STEPS** - Run these EVERY session:

```bash
cargo install decapod              # 1. Install/update to latest release
decapod --version                   # 2. Check current version
decapod --help                      # 3. Verify available commands
decapod docs show core/DECAPOD.md  # 4. Refresh constitution
decapod session acquire             # 5. Acquire session token (required for all commands)
decapod validate                    # 6. System state
decapod todo list                   # 7. Active work
```

**Why this matters:** The decapod binary and constitution evolve. Always install the latest release first, then verify commands and refresh your understanding of the latest contract before acting.

## Codex-Specific Notes

- You excel at code completion - but still read architecture first
- You have strong pattern matching - use it to follow existing code style
- You may have tool use or function calling - use it for `decapod` commands
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

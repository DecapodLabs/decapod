# CLAUDE.md - Claude Agent Entrypoint

You (Claude) are working in a Decapod-managed repository.

## ⚠️ MANDATORY: Docker + Git Workspace Path

**This is CRITICAL for any work involving git-controlled files:**

- Work MUST execute within the Docker container workspace environment
- Git operations MUST use the isolated workspace path (not the host repo)
- Branch creation, commits, and pushes MUST occur from within the container
- Never modify git-controlled files directly on the host - always use the container workflow

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

**MANDATORY FIRST STEPS** - Run these EVERY session:

```bash
cargo install decapod              # 1. Install/update to latest release
decapod version                   # 2. Check current version
decapod --help                      # 3. Verify available commands
decapod data schema --subsystem command_registry --deterministic >/dev/null  # 4. Refresh CLI command index
decapod docs show core/DECAPOD.md  # 5. Refresh constitution
decapod session acquire             # 6. Acquire session token (required for all commands)
decapod validate                    # 7. System state
decapod todo list                   # 8. Active work
```

**Why this matters:** The decapod binary and constitution evolve. Always install the latest release first, then verify commands and refresh your understanding of the latest contract before acting.

## Claude-Specific Notes

- You have strong tool use - use `decapod` commands via Bash tool
- You can read multiple files in parallel - use this for exploration
- Your context window is large - but still use `decapod docs` for constitution access
- Apply control-plane opacity: keep operator-facing output semantic (intent/actions/outcomes), not command-surface oriented
- Do NOT add yourself as co-author on commits (user preference)

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

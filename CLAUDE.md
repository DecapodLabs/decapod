# CLAUDE.md - Claude Agent Entrypoint

You (Claude) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

```bash
decapod docs show core/DECAPOD.md  # Router (alias: decapod d)
decapod validate                     # System state (alias: decapod v)
decapod todo list                    # Active work (alias: decapod t)
decapod govern health summary        # System health overview
```

## Claude-Specific Notes

- You have strong tool use - use `decapod` commands via Bash tool
- You can read multiple files in parallel - use this for exploration
- Your context window is large - but still use `decapod docs` for constitution access
- Do NOT add yourself as co-author on commits (user preference)

## The Contract

Same four invariants as all agents:

1. ✅ Start at router (`core/DECAPOD.md`)
2. ✅ Use control plane (`decapod` commands only)
3. ✅ Pass validation (`decapod validate` before done)
4. ✅ Stop if missing (ask for guidance)

**All authority defers to AGENTS.md and the embedded constitution.**

## Links

- `AGENTS.md` — Universal agent contract (binding)
- `core/DECAPOD.md` — Router
- `.decapod/OVERRIDE.md` — Project customizations

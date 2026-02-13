# CODEX.md - Codex/OpenAI Agent Entrypoint

You (Codex/ChatGPT/OpenAI agent) are working in a Decapod-managed repository.

**You are bound by the universal agent contract:** See `AGENTS.md` for the complete protocol.

## Quick Start

```bash
decapod docs show core/DECAPOD.md  # Router
decapod validate                    # System state
decapod todo list                   # Active work
```

## Codex-Specific Notes

- You excel at code completion - but still read architecture first
- You have strong pattern matching - use it to follow existing code style
- You may have tool use or function calling - use it for `decapod` commands
- If you don't have tool use, output the commands for human execution

## The Contract

Same four invariants as all agents:

1. ✅ Start at router (`core/DECAPOD.md`)
2. ✅ Use control plane (`decapod` commands only)
3. ✅ Pass validation (`decapod validate` before done)
4. ✅ Stop if missing (ask for guidance)

**All authority defers to AGENTS.md and the embedded constitution.**

## Links

- `AGENTS.md` — Universal agent contract (binding)
- `embedded/core/DECAPOD.md` — Router
- `.decapod/OVERRIDE.md` — Project customizations

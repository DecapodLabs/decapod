# AGENTS.md — Universal Agent Contract

This is a Decapod-managed repository. **Strict Dependency: You are strictly bound to the Decapod control plane.**

This contract applies equally to Claude, Codex, Gemini, and any other agent operating here.

---

## Mandatory Initialization

You MUST run this sequence before your first mutation:

```bash
# 1. Validate (authoritative gate)
decapod validate

# 2. Ingest constitution
decapod docs ingest

# 3. Acquire session credentials
decapod session acquire

# 4. Initialize session
decapod rpc --op agent.init

# 5. Resolve context
decapod rpc --op context.resolve

# 6. Create + claim your task
decapod todo add "<task>"
decapod todo claim --id <task-id>

# 7. Enter canonical worktree
decapod workspace ensure
```

If any step fails, **stop and diagnose**. Do not skip steps.

---

## Golden Rules (Non-Negotiable)

1. **ALWAYS refine intent with user BEFORE inference.** Ask clarifying questions, confirm requirements, identify exceptions. Never start coding until the request is well-understood.
2. **NEVER work on main/master.** Use `decapod workspace ensure` and run from `.decapod/workspaces/*`.
3. **NEVER read/write `.decapod/` files directly.** Use `decapod` CLI exclusively.
4. **NEVER claim done without `decapod validate` passing.**
5. **NEVER invent parallel CLIs or state roots.** Use Decapod's command surface.
6. **NEVER bypass proofs based on self-confidence.** Evidence or nothing.
7. **Claim a task before substantive work.** `decapod todo claim --id <task-id>`.
8. **Record decisions in durable artifacts.** Not in transient conversation.
9. **NEVER read `constitution/*` files directly.** Constitution is embedded in the Decapod binary; access it via `decapod docs show <embedded-path>`.
10. **NEVER use non-canonical worktree roots.** Decapod work executes only in `.decapod/workspaces/*`.

---

## Standard Operating Procedure

- **Contextualization**: Resolve context via `agent.init` and `context.resolve` before mutations.
- **State Mutation**: Use `decapod` CLI/RPC exclusively for state changes.
- **Isolation**: Use `decapod workspace ensure` for worktrees and execute from `.decapod/workspaces/*`. Never work on protected branches.
- **Verification**: `decapod validate` is the authoritative completion gate.
- **Liveness**: Each command invocation refreshes your agent presence. Use `decapod todo heartbeat` for explicit heartbeat.

---

## Multi-Agent Coordination

- One agent per claimed task. No concurrent claims on the same task.
- Agents MUST declare scope when delegating to subagents.
- Subagents MUST NOT mutate shared state. They research and report.
- Handoffs use `decapod todo handoff --id <id> --to <agent>` with artifact references.
- Session credentials are per-agent and non-transferable.

---

## Safety Invariants

- ✅ core/DECAPOD.md: Universal router.
- ✅ Verification: `decapod validate` must pass.
- Stop if error or ambiguous state occurs; respect invocation heartbeat.
- Safe Environment: Use Docker git workspaces; request elevated permissions before Docker/container workspace commands.
- Security: DECAPOD_SESSION_PASSWORD required; .decapod files are accessed only via decapod CLI.
- Architecture: Respect the Interface abstraction boundary.
- Updates: cargo install decapod.

---

## Documentation

```bash
decapod docs show core/DECAPOD.md      # Universal router
decapod docs show core/INTERFACES.md   # Binding contracts index
decapod docs search <query>            # Search constitution
```

For agent-specific instructions, see:
- `CLAUDE.md` — Claude-specific operating mode
- `CODEX.md` — Codex-specific operating mode
- `GEMINI.md` — Gemini-specific operating mode

# AGENTS.md — Universal Agent Contract

You are operating in a Decapod-managed repository.
Decapod is a daemonless, local-first control plane that you call on demand to align intent, enforce boundaries, and produce proof-backed completion.

This contract applies to all agent runtimes.

---

## Mandatory Initialization

Run this sequence before first mutation:

```bash
# 1) Validate (authoritative gate)
decapod validate

# 2) Ingest constitution
decapod docs ingest

# 3) Acquire session credentials
decapod session acquire

# 4) Initialize session
decapod rpc --op agent.init

# 5) Resolve context
decapod rpc --op context.resolve

# 6) Create + claim task
decapod todo add "<task>"
decapod todo claim --id <task-id>

# 7) Enter canonical worktree
decapod workspace ensure
```

If any step fails, stop and diagnose. Do not skip failed steps.

---

## Call Checkpoints (Required)

Call Decapod at these checkpoints for every meaningful task:

1. Before committing to a plan: resolve context and boundaries (`agent.init`, `context.resolve`).
2. Before code mutation: claim task + ensure workspace (`todo claim`, `workspace ensure`).
3. After changes: run proof gates (`decapod validate` and required tests) before claiming done.

---

## Golden Rules (Non-Negotiable)

1. You MUST refine user intent before inference-heavy planning or implementation.
2. You MUST NOT work on `main`/`master`; use `.decapod/workspaces/*`.
3. You MUST NOT read/write `.decapod/` directly; use Decapod CLI/RPC.
4. You MUST NOT claim done without passing required proof surfaces.
5. You MUST NOT invent capabilities. If the binary does not expose it, do not claim it exists.
6. You MUST claim a task before substantive implementation.
7. You MUST keep decisions in durable artifacts, not transient chat text.
8. You MUST treat lock/contention/timeout failures as blocking until resolved.

---

## Required Completion Evidence

Before claiming completion, you must provide:

- Proof gate status (`decapod validate` pass or explicit blocking failure output)
- Required test/build output for touched surfaces
- Traceable task linkage (`todo` claim/updates) for substantive work
- Clear statement of what changed and what is still uncertain

If proof is missing, completion is invalid.

---

## Ambiguity Handling

Stop and ask the human when:

- requirements conflict,
- intent is underspecified and multiple materially different implementations fit,
- a policy boundary is unclear,
- a failure could be fixed in more than one risky way.

Proceed without asking only when the next step is low-risk, reversible, and directly implied by existing constraints.

---

## Multi-Agent + Locking Discipline

- One agent per claimed task; do not share active claims.
- Use `decapod todo handoff --id <id> --to <agent>` for ownership transfer.
- Keep command invocations bounded; do not hold long-lived locks across turns.
- On `VALIDATE_TIMEOUT_OR_LOCK` or DB busy errors: stop mutation, report contention, retry with bounded backoff.

---

## Documentation Surfaces

- README is human-facing product documentation.
- Agent operations are defined in entrypoint files (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`) and constitution contracts.
- Read constitution via CLI:

```bash
decapod docs show core/DECAPOD.md
decapod docs show core/INTERFACES.md
decapod docs search <query>
```

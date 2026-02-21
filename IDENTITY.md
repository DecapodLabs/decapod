# IDENTITY.md — What Decapod Is

## Thesis

Decapod is a **daemonless, repo-native control plane** for AI agent governance.

AI agents can write code fast. Shipping safely is hard. Decapod provides:
- **Advisory**: Guidance toward the next high-value move.
- **Interlock**: Hard stops for unsafe flows.
- **Attestation**: Structured evidence of completion.

## What Decapod IS

- A CLI tool (`cargo install decapod`) that operates on repo-scoped state.
- A constitution embedded at compile time, overridable per-project.
- An event-sourced store (SQLite + append-only JSONL ledgers) for deterministic replay.
- A validation harness that gates promotion with proof, not persuasion.
- A multi-agent coordination layer with session isolation and workspace enforcement.
- Agent-native: designed for programmatic access (CLI/RPC), not human GUIs.

## What Decapod IS NOT

- Not a daemon or background service. Every capability is invoked via CLI/library calls.
- Not an agent itself. It is infrastructure that agents use.
- Not an LLM wrapper or orchestrator. It does not call models.
- Not a "memory manager" that silently becomes source of truth.
- Not a cloud service. All state is local, repo-scoped, and auditable.

## Invariants (Non-Negotiable)

1. **Daemonless**: Nothing requires a background service.
2. **Repo-native canonicality**: Any state that influences promotion MUST be stored in the repo-scoped Decapod store, reproducible, content-addressed, and verifiable.
3. **Proof over persuasion**: Any "enforced" claim MUST map to a deterministic gate. If it can't be gated yet, it MUST be classified `partially_enforced` with a tracked path to enforcement.
4. **Determinism**: Reducers MUST be deterministic over append-only ledgers. No stochastic scoring. If you rank, use deterministic functions with explicit inputs, stable ordering, and fixed weights committed in code.
5. **Minimal surface area**: Protect the kernel. Prefer one canonical mechanism with clean invariants over multiple overlapping layers.
6. **No silent drift**: If the repo says something is true, the gates MUST prove it. If the gates can't prove it, the repo MUST admit it.

## Vocabulary

| Term | Definition |
|------|------------|
| **Constitution** | Embedded methodology docs shipped with Decapod. Read-only baseline. |
| **Override** | Project-specific `.decapod/OVERRIDE.md` that extends/modifies constitution behavior. |
| **Gate** | A deterministic validator that blocks progress until criteria are met. |
| **Claim** | A registered guarantee in `constitution/interfaces/CLAIMS.md` with proof surface. |
| **Obligation** | A deterministic pointer to context an agent MUST consider before acting. |
| **Store** | Repo-scoped (`.decapod/data/`) or user-scoped (`~/.decapod/`) state root. |
| **Promotion** | Moving work from branch to main via validated merge. |
| **Trace** | Append-only interaction record in `.decapod/data/traces.jsonl`. |
| **Snapshot** | Deterministic summary derived from trace data (co-player inference). |
| **Worktree** | Git worktree for workspace isolation. Agents MUST NOT work on main/master. |
| **Broker** | Serialization lock ensuring single-threaded state mutations. |
| **Event sourcing** | All state changes logged to `.events.jsonl` for deterministic replay. |
| **Golden vectors** | Frozen test outputs that prove determinism. Version bump required to change. |

## Architecture Layers

```
┌─────────────────────────────────────┐
│  Agent (Claude, Codex, Gemini, ...) │  ← Reads CLAUDE.md / AGENTS.md
├─────────────────────────────────────┤
│  Decapod CLI / RPC                  │  ← decapod <command>
├─────────────────────────────────────┤
│  Core Kernel                        │  ← src/core/ (minimal, stable)
│  ├── validate.rs (proof gates)      │
│  ├── broker.rs (serialization lock) │
│  ├── mentor.rs (obligations engine) │
│  ├── trace.rs (interaction ledger)  │
│  └── workspace.rs (isolation)       │
├─────────────────────────────────────┤
│  Plugins                            │  ← src/plugins/ (extensible)
│  ├── health.rs, policy.rs, verify.rs│
│  ├── federation.rs, knowledge.rs    │
│  ├── teammate.rs, decide.rs         │
│  └── ...20+ subsystems              │
├─────────────────────────────────────┤
│  Constitution (embedded)            │  ← constitution/ (read-only)
│  ├── core/ (routers, interfaces)    │
│  ├── interfaces/ (binding contracts)│
│  ├── methodology/ (guidance)        │
│  ├── plugins/ (subsystem specs)     │
│  └── specs/ (technical standards)   │
├─────────────────────────────────────┤
│  Store (.decapod/data/)             │  ← SQLite + JSONL (event-sourced)
└─────────────────────────────────────┘
```

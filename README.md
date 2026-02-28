<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  A daemonless control plane for AI coding agents.
</p>

<p align="center">
  Decapod is a repo-native substrate that agents call during a run to turn human intent into explicit, checkable work artifacts
  (intent → context → spec → proof) before the next inference step. No background service. No new workflow. Local-first state you can inspect.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="https://ko-fi.com/decapodlabs"><img alt="Ko-fi" src="https://img.shields.io/badge/Support-Ko--fi-ff5f5f?logo=ko-fi&logoColor=white"></a>
</p>

---

## What Decapod does (plain language)

Decapod does **not** replace your agent. You keep using Claude Code, Codex, Gemini CLI, Cursor, etc.

Decapod adds one missing layer: when an agent needs to make a control-plane decision (what the user *meant*, what's in/out of scope, what "done" means, what must be proven), it calls Decapod as a local CLI/RPC. Decapod writes the resulting intent/spec/proof artifacts into `.decapod/`, so the run becomes auditable and repeatable instead of "trust me bro."

Decapod ships with an embedded [constitution](constitution/core/DECAPOD.md): a set of governance docs that agents receive as just-in-time context. The constitution defines boundaries, interfaces, and methodology so agents don't have to guess the rules — they query them on demand.

Decapod is **daemonless**: it starts when called and exits immediately after the call.

### Scenarios where Decapod helps immediately

1. **"This request is underspecified."**
   The agent stops guessing. Decapod forces intent to become explicit (constraints, boundaries, acceptance criteria), then the agent continues with tighter context.

2. **"Multiple agents are editing the same repo."**
   Decapod coordinates shared state and prevents collisions so parallel runs don't silently overwrite or drift.

3. **"It compiled, but is it actually done?"**
   Decapod enforces completion gates: tests, validations, and proof artifacts must pass before a run can claim VERIFIED.

Related research: [Evaluating AGENTS.md](https://arxiv.org/pdf/2602.11988) (ETH SRI, 2026) on context-file quality and agent cost/performance.

<p align="center">
  ☕ Like Decapod? <a href="https://ko-fi.com/decapodlabs"><strong>Buy us a coffee on Ko-fi</strong></a> 💙
</p>

## Getting started

```bash
cargo install decapod
decapod init
```

### What changes after init?

Decapod creates a `.decapod/` directory (local-first state) and a small set of agent entrypoint files so agents know the contract. Your existing code and workflow are untouched.

### What files get created?

```text
.decapod/
  config.toml                 # project configuration
  data/                       # durable state (governance, memory, traces)
  generated/
    specs/                    # intent, architecture, validation specs
    artifacts/                # proof artifacts, internalizations, provenance
    sessions/                 # per-session provenance logs
AGENTS.md                     # agent-facing contract overview
CLAUDE.md / CODEX.md / GEMINI.md  # tool-specific entrypoints
```

### How to tell it's working

1. Run your agent normally and ask for a real change (not just "explain X").
2. Check `.decapod/generated/` for new artifacts (specs, proofs, session logs).
3. Run `decapod validate` and see typed pass/fail gates instead of narrative claims.
4. Ask the agent "what did Decapod change about your plan?" — it should reference explicit intent/spec/proof steps.

Agent integration: see `AGENTS.md` and tool-specific entrypoints (`CLAUDE.md`, `CODEX.md`, `GEMINI.md`) for the operational contract.

Learn more about the embedded [constitution](constitution/core/DECAPOD.md). Override defaults with plain English in `.decapod/OVERRIDE.md`.

---

## Why Decapod

AI coding agents are strong at generating code. Most failures happen before and after generation: unclear intent, fuzzy boundaries, and weak completion checks.

Decapod is the missing layer in that loop. Agents call it mid-run to lock intent, enforce boundaries, and prove completion with explicit gates. It shapes inference without doing inference.

State is local and durable in `.decapod/`: shared context, decisions, and traces persist across sessions and remain retrievable over time.

## Assurance model

Decapod centers execution around three outcomes:

- `Advisory`: clear next actions that tighten intent and reduce wasted loops.
- `Interlock`: hard policy boundaries that block unsafe or out-of-contract flow.
- `Attestation`: durable, structured proof that completion criteria actually passed.

## Operating model

```text
Human Intent
    |
    v
AI Agent(s)  <---->  Decapod Runtime  <---->  Repository + Policy
                         |    |    |
                         |    |    +-- Interlock (enforced boundaries)
                         |    +------- Advisory (guided execution)
                         +------------ Attestation (verifiable outcomes)
```

## Features

Core properties:

- Daemonless execution: no background agent manager, no hidden runtime.
- Two-command adoption: `cargo install decapod` and `decapod init`.
- Agent-agnostic contract: one CLI/RPC surface across Claude, Codex, Gemini, Cursor, and others.
- Parallel-safe collaboration: multiple agents can operate in one repo without state collisions.
- VERIFIED is enforced: completion requires passing proof-plan results, not narrative claims.
- Local-first auditability: `.decapod/` keeps durable traces, decisions, and proof artifacts.
- Internalized context artifacts: turn long documents into mountable, verifiable context adapters so agents stop paying the long-context tax repeatedly (`decapod internalize create`).

Deep surface area (interfaces, capsules, eval kernel, promotions, etc.):

- `decapod docs show core/INTERFACES.md`
- `.decapod/OVERRIDE.md` template after init

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
decapod validate
```

## Documentation

- Development guide: [CONTRIBUTING.md](CONTRIBUTING.md)
- Security policy: [SECURITY.md](SECURITY.md)
- Release history: [CHANGELOG.md](CHANGELOG.md)

## Support

- [File an issue](https://github.com/DecapodLabs/decapod/issues)
- [Support on Ko-fi](https://ko-fi.com/decapodlabs)

## License

MIT. See [LICENSE](LICENSE).

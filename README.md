<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  The governance kernel for AI coding agents.
</p>

<p align="center">
  Called on demand inside agent loops to turn intent into context, then context into explicit specifications before inference.<br />
  No daemon. No workflow tax. Just artifacts you can read, hash, and trust.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="https://ko-fi.com/decapodlabs"><img alt="Ko-fi" src="https://img.shields.io/badge/Support-Ko--fi-ff5f5f?logo=ko-fi&logoColor=white"></a>
</p>

---

## The idea

Your agent writes code. Nobody checks its homework.

Decapod is the layer that makes agents show their work. Before an agent mutates your repo, Decapod forces it to answer three questions: *What did the human actually ask for?* *What boundaries apply?* *How will we prove it's done?* The answers become artifacts — not comments, not logs, artifacts with hashes and validation gates — stored in `.decapod/` where anyone on the team can see exactly what happened and why.

It ships with an embedded [constitution](constitution/core/DECAPOD.md): governance docs agents receive as just-in-time context so they query the rules on demand instead of guessing them.

Decapod doesn't replace your agent. It doesn't run in the background. It starts when called, writes its receipts, and exits. Two commands to adopt. Zero config to maintain.

### When it clicks

**"The spec was vibes."** Your agent asks Decapod what the user actually meant. Decapod forces intent to crystallize — constraints, boundaries, acceptance criteria — before a single line is generated. The agent stops hallucinating requirements.

**"Three agents, one repo, total chaos."** Decapod coordinates shared state across parallel runs. No silent overwrites. No drift. Each agent gets an isolated workspace with a provenance trail.

**"It passes CI but is it *done*?"** Decapod gates completion on proof artifacts, not narrative claims. `VERIFIED` means every gate in the proof plan actually passed — not "the agent said it looks good."

Related: [Evaluating AGENTS.md](https://arxiv.org/pdf/2602.11988) (ETH SRI, 2026) on context-file quality and agent cost/performance.

<p align="center">
  <a href="https://ko-fi.com/decapodlabs"><strong>Buy us a coffee</strong></a> ☕
</p>

---

## Get running

```bash
cargo install decapod
decapod init
```

That's it. Keep using Claude Code, Codex, Gemini CLI, Cursor — whatever you already use. Decapod gets called by your agent automatically when control-plane decisions are needed. Your workflow doesn't change; the agent just gets smarter about when to stop and think.

### What lands in your repo

```text
.decapod/
  config.toml                 # project configuration
  data/                       # durable state (governance, memory, traces)
  generated/
    specs/                    # intent, architecture, validation specs
    artifacts/                # proof artifacts, internalizations, provenance
    sessions/                 # per-session provenance logs
AGENTS.md                     # universal agent contract
CLAUDE.md / CODEX.md / GEMINI.md  # tool-specific entrypoints
```

Everything material is a file you can `cat`. No databases to query, no dashboards to check. The state *is* the directory.

### How to know it's working

1. Ask your agent to make a real change. Watch `.decapod/generated/` populate with new specs and proof artifacts.
2. Ask your agent to validate the work. It will report typed pass/fail gates, not "looks good to me."
3. Ask the agent *"what did Decapod change about your plan?"* — it should cite spec and proof steps, not vibes.

Agent integration: `AGENTS.md` and tool-specific entrypoints (`CLAUDE.md`, `CODEX.md`, `GEMINI.md`) define the full operational contract your agent follows.

Override any constitution default with plain English in `.decapod/OVERRIDE.md`. Learn more about the embedded [constitution](constitution/core/DECAPOD.md).

---

## Why this exists

AI coding agents are extraordinarily good at generating code. They are extraordinarily bad at knowing when to stop, what not to touch, and whether the thing they built is the thing you asked for.

The failure mode isn't "bad code." It's unaccountable code: no intent recorded, no boundaries enforced, no proof that completion criteria were met. You get a PR that compiles. You have no idea if it's right.

Decapod closes that gap. Agents call it mid-run to lock intent, enforce boundaries, and prove completion. It shapes what goes into inference without doing inference itself.

State is local and durable in `.decapod/`. Context, decisions, and traces persist across sessions and stay retrievable over time. Nothing hides. Nothing phones home.

## How it works

Every Decapod operation returns one of three things:

| Signal | What it does | Think of it as |
|--------|-------------|----------------|
| **Advisory** | Tightens intent, reduces wasted loops | Guardrails |
| **Interlock** | Hard policy boundary — blocks unsafe flow | Circuit breaker |
| **Attestation** | Structured proof that criteria actually passed | Receipt |

```text
Human Intent
    |
    v
AI Agent(s)  <---->  Decapod  <---->  Repository + Policy
                       |  |  |
                       |  |  +-- Interlock (enforced boundaries)
                       |  +----- Advisory (guided execution)
                       +-------- Attestation (verifiable outcomes)
```

## What you get

- **Daemonless.** No background process. The binary starts, does its job, exits.
- **Two-command install.** Install and init. Done.
- **Agent-agnostic.** Works with Claude, Codex, Gemini, Cursor, and anything else that can shell out.
- **Parallel-safe.** Multiple agents, one repo, no collisions.
- **Proof-gated completion.** `VERIFIED` requires passing proof-plan results, not narrative.
- **Fully auditable.** Every decision, trace, and proof artifact lives in `.decapod/` as plain files.
- **Context internalization.** Turn long documents into mountable, verifiable context adapters with explicit source hashes, determinism labels, session-scoped attach leases, and explicit detach so agents stop re-ingesting the same 50-page spec every session.

The deep surface area — interfaces, capsules, eval kernel, knowledge promotions, obligation graphs — lives in the embedded constitution. Ask your agent to explore it.

---

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
```

## Docs

- [CONTRIBUTING.md](CONTRIBUTING.md) — development guide
- [SECURITY.md](SECURITY.md) — security policy
- [CHANGELOG.md](CHANGELOG.md) — release history

## Support

- [Issues](https://github.com/DecapodLabs/decapod/issues)
- [Ko-fi](https://ko-fi.com/decapodlabs)

## License

MIT. See [LICENSE](LICENSE).

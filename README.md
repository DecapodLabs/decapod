<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  A daemonless control plane for AI coding agents.
</p>

<p align="center">
  Called on demand inside agent loops. No background process, no new workflow, local-first state you can verify.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="https://ko-fi.com/decapodlabs"><img alt="Ko-fi" src="https://img.shields.io/badge/Support-Ko--fi-ff5f5f?logo=ko-fi&logoColor=white"></a>
</p>

---

## Why Decapod 🧠

AI coding agents are strong at generating code. Most failures happen before and after generation: unclear intent, fuzzy boundaries, and weak completion checks.

Decapod is the missing layer in that loop. Agents call it mid-run to lock intent, enforce boundaries, and prove completion with explicit gates. It shapes inference without doing inference.

Decapod is **daemonless**. There is no long-lived service. The binary starts when an agent calls it and exits immediately after the call.

"Just use Decapod" is literal:
- `cargo install decapod`
- `decapod init`

Then continue with Claude Code, OpenAI Codex, Gemini CLI, Cursor, or any tool that can invoke a CLI command. Decapod is agent-agnostic and safe for concurrent multi-agent execution.

State is local and durable in `.decapod/`: shared context, decisions, and traces persist across sessions and remain retrievable over time.

Related: [Evaluating AGENTS.md](https://arxiv.org/pdf/2602.11988) (ETH SRI, 2026) on context-file quality and agent cost/performance.

<p align="center">
  ☕ Like Decapod? <a href="https://ko-fi.com/decapodlabs"><strong>Buy us a coffee on Ko-fi</strong></a> 💙
</p>

## Assurance Model ✅

Decapod centers execution around three outcomes:

- `Advisory`: clear next actions that tighten intent and reduce wasted loops.
- `Interlock`: hard policy boundaries that block unsafe or out-of-contract flow.
- `Attestation`: durable, structured proof that completion criteria actually passed.

## Operating Model ⚙️

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

## Features ✨

- ✅ Daemonless runtime: invoked on demand, exits immediately after each call.
- ✅ Agent-agnostic CLI/RPC surface for Claude, Codex, Gemini, Cursor, and more.
- ✅ Multi-agent-safe task/workspace coordination in the same repo.
- ✅ Shared transient skills memory: human-taught rules persist once and resolve across agents/sessions.
- ✅ Work Unit manifests (`intent/spec/state/proof`) with governed status transitions.
- ✅ VERIFIED completion gating backed by explicit proof-plan pass results.
- ✅ Variance-aware eval kernel: repeat-run aggregation, strict judge contracts, and promotion gates.
- ✅ Deterministic context capsule query (`core|interfaces|plugins` scoped).
- ✅ Optional persisted context capsules in `.decapod/generated/context/*.json`.
- ✅ Knowledge promotion firewall ledger in `.decapod/data/knowledge.promotions.jsonl`.
- ✅ Procedural knowledge writes require event-backed promotion provenance.
- ✅ Repo-native durable state, traces, and proof gates under `.decapod/` (local-first, auditable).

And dozens more. For the full high-level and data-level surface area, see `decapod docs show core/INTERFACES.md` and the override template at `.decapod/OVERRIDE.md`.

## Getting Started 🚀

```
cargo install decapod
decapod init
```

Then keep using your agents normally. Decapod is called from inside those agent runs when control-plane decisions are needed.

Agent integration: If you use Claude Code / Codex / Gemini / Cursor / similar tools, see `AGENTS.md` and the tool-specific entrypoint files (`CLAUDE.md`, `CODEX.md`, `GEMINI.md`) for the exact operational contract.

Learn more about the embedded constitution via the CLI:

```bash
decapod docs show core/DECAPOD.md
```

Override constitution defaults with plain English in `.decapod/OVERRIDE.md`.

## Contributing 🤝

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
decapod validate
```

## Documentation 📚

- Development guide: [CONTRIBUTING.md](CONTRIBUTING.md)
- Security policy: [SECURITY.md](SECURITY.md)
- Release history: [CHANGELOG.md](CHANGELOG.md)

## Support 💖

- 🐛 [File an issue](https://github.com/DecapodLabs/decapod/issues)
- ☕ [Support on Ko-fi](https://ko-fi.com/decapodlabs)

## License 📄

MIT. See [LICENSE](LICENSE).

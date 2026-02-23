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

## Getting Started 🚀

```
cargo install decapod
decapod init
```

Then keep using your agents normally. Decapod is called from inside those agent runs when control-plane decisions are needed.

Agent integration: If you use Claude Code / Codex / Gemini / Cursor / similar tools, see `AGENTS.md` and the tool-specific entrypoint files (`CLAUDE.md`, `CODEX.md`, `GEMINI.md`) for the exact operational contract.

Learn more about the embedded (constitution)[constitution/core/DECAPOD.md].

Override constitution defaults with plain English in `.decapod/OVERRIDE.md` after you initilaize Decapod in your project directory.

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

- ✅ Daemonless execution: no background agent manager, no hidden runtime.
- ✅ Two-command adoption: `cargo install decapod` and `decapod init`.
- ✅ Agent-agnostic contract: one CLI/RPC surface across Claude, Codex, Gemini, Cursor, and others.
- ✅ Parallel-safe collaboration: multiple agents can operate in one repo without state collisions.
- ✅ Work Unit governance: `intent/spec/state/proof` is explicit, durable, and machine-checkable.
- ✅ VERIFIED is enforced: completion requires passing proof-plan results, not narrative claims.
- ✅ Promotion discipline: validate-time and publish-time gates block silent regressions.
- ✅ Deterministic context capsules: scoped (`core|interfaces|plugins`), hashable, and reproducible.
- ✅ Variance-aware eval kernel: repeat-run plans, strict judge contracts, statistical regression gates.
- ✅ Knowledge promotion firewall: procedural truth requires event-backed provenance in `.decapod/data/knowledge.promotions.jsonl`.
- ✅ Shared transient aptitude memory: capture human-taught preferences once, reuse across agents and sessions.
- ✅ Plain-English policy control in `.decapod/OVERRIDE.md`.
- ✅ Local-first auditability: `.decapod/` keeps durable traces, decisions, and proof artifacts.

And dozens more. For the full high-level and data-level surface area, see `decapod docs show core/INTERFACES.md` and the override template at `.decapod/OVERRIDE.md`.

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

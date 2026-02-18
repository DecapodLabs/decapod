<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  The governance runtime for AI coding agents.
</p>

<p align="center">
  Local-first, repo-native, and built for verifiable delivery.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="https://ko-fi.com/decapodlabs"><img alt="Ko-fi" src="https://img.shields.io/badge/Support-Ko--fi-ff5f5f?logo=ko-fi&logoColor=white"></a>
</p>

---

## Why Decapod 🧠

AI coding agents can write code fast. Shipping it safely is the hard part.

Decapod gives agents a consistent operational contract: guided execution, enforceable boundaries, and auditable completion signals. It replaces "looks done" with explicit outcomes.

Decapod is **invoked by agents; it never runs in the background**. It is a single executable binary that provides deterministic primitives:
- Retrieve **canon (constitution .md fragments)** as context.
- Provide authoritative schemas for **structured state** (todos, knowledge, decisions).
- Run deterministic **validation/proof gates** to decide when work is truly done.
  Example gate: *forbid direct pushes to protected branches* — fails if the agent has unpushed commits on main.

AGENTS.md stays tiny (entrypoint). OVERRIDE.md handles local exceptions. Everything else is pulled just-in-time.

Traces: `.decapod/data/traces.jsonl`. Bindings: `context.bindings`. Architecture-agnostic (not coupled to a specific OS or CPU).

Recent independent research confirms this design direction: [Evaluating AGENTS.md](https://arxiv.org/pdf/2602.11988) (Gloaguen et al., ETH SRI, 2026; [AgentBench repo](https://github.com/eth-sri/agentbench)) found that LLM-generated context files tend to reduce agent performance while increasing cost by over 20 %; human-written minimal requirements can help slightly. Decapod was built independently and without knowledge of ETH SRI's AgentBench research or this paper.

<p align="center">
  ☕ Like Decapod? <a href="https://ko-fi.com/decapodlabs"><strong>Buy us a coffee on Ko-fi</strong></a> 💙
</p>

## Assurance Model ✅

Decapod is built around three execution outcomes:

- `Advisory`: guidance toward the next high-value move.
- `Interlock`: hard stops for unsafe or out-of-policy flow.
- `Attestation`: structured evidence that completion criteria were met.

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

- Agent-native CLI and RPC surface for deterministic operation.
- Guided project understanding through structured prompting.
- Standards-aware execution aligned with project policy.
- Workspace safety for isolated implementation flow.
- Validation and completion gates with explicit pass/fail outcomes.
- Multi-agent-ready orchestration surface for tooling integrations.

## Getting Started 🚀

```
cargo install decapod
decapod init
```

Then use your agents as normal. Decapod works on your behalf from inside the agent.
Override defaults in `.decapod/OVERRIDE.md`.

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

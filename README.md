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
</p>

---

## Why Decapod

AI coding agents can write code fast. Shipping it safely is the hard part.

Decapod gives agents a consistent operational contract: guided execution, enforceable boundaries, and auditable completion signals. It replaces "looks done" with explicit outcomes.

Decapod is **invoked by agents; it never runs in the background**. It is a single executable binary that provides deterministic primitives:
- Retrieve **canon (constitution .md fragments)** as context.
- Provide authoritative schemas for **structured state** (todos, knowledge, decisions).
- Run deterministic **validation/proof gates** to decide when work is truly done.

Traces are stored locally in `.decapod/data/traces.jsonl`. Bindings are introspectable via `context.bindings`.

Decapod is architecture-agnostic software. It is not a Linux kernel binding and is not coupled to a specific OS or CPU architecture.

## Assurance Model

Decapod is built around three execution outcomes:

- `Advisory`: guidance toward the next high-value move.
- `Interlock`: hard stops for unsafe or out-of-policy flow.
- `Attestation`: structured evidence that completion criteria were met.

## Operating Model

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

- Agent-native CLI and RPC surface for deterministic operation.
- Guided project understanding through structured prompting.
- Standards-aware execution aligned with project policy.
- Workspace safety for isolated implementation flow.
- Validation and completion gates with explicit pass/fail outcomes.
- Multi-agent-ready orchestration surface for tooling integrations.

## Getting Started

Install Decapod with Cargo, initialize it in your repository, and let your agent operate through the Decapod contract instead of direct ad-hoc repo mutation.

For command details and full usage, use `decapod --help`.

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
decapod validate
```

## Documentation

- Development guide: `CONTRIBUTING.md`
- Security policy: `SECURITY.md`
- Release history: `CHANGELOG.md`

## Support

- [File an issue](https://github.com/DecapodLabs/decapod/issues)
- [Support on Ko-fi](https://ko-fi.com/decapodlabs)

## License

MIT. See [LICENSE](LICENSE).

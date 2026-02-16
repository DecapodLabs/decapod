<p align="center"><code>cargo install decapod</code></p>
<p align="center"><code>decapod init</code></p>

<p align="center">
  <img src="assets/decapod-ultra.svg" width="600" alt="Decapod">
</p>

<p align="center">
  <strong>Decapod</strong> is a governance runtime for AI coding agents. Local-first, repo-native, built in Rust.
  <br>
  <sub>Named for the ten-legged crustaceans: hardened exoskeleton, distributed nervous system, no central brain. They coordinate anyway.</sub>
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

---

<table>
  <tr>
    <td align="center" width="50%">
      <img src="assets/screenshot-task-1.png" alt="Create agent-agnostic tasks" width="100%">
      <br>
      <sub><b>Make Tasks</b> — agents create tasks to do later.</sub>
    </td>
    <td align="center" width="50%">
      <img src="assets/screenshot-task-2.png" alt="Discover tasks with other agents" width="100%">
      <br>
      <sub><b>Share the Work</b> — other agents discover tasks and complete them.</sub>
    </td>
  </tr>
</table>

Demo: **[Watch on GitHub](https://github.com/DecapodLabs/decapod/raw/master/assets/decapod-demo.mp4)** (12 MB)

---

## What Is Decapod

Like Docker is a runtime for containers, Decapod is a runtime for agents. You set it up once, then agents operate inside a governed environment with persistent state, enforced methodology, proof gates, and coordination primitives.

Agents can write code. But they can't reliably **ship** because they forget what they built yesterday, treat best practices as vibes, say "done" without evidence, and trip over each other in parallel. Decapod fixes that with over 50 validation gates that enforce the agent<->Decapod contract.

Decapod is **not** a prompt pack, an agent framework, a hosted SaaS platform, a review bot, or a human workflow tool. Humans may read artifacts; agents are the operators. It's infrastructure: the environment where agent work becomes enforceable.

## Quickstart

```bash
cargo install decapod
cd your-project
decapod init
```

That's it. Agents now operate inside the governed environment. Proofs decide 'done'.

## How It Works

Decapod is a tool for agents, not for humans. After `decapod init` is present in a repo, agents operate inside the governed environment. Two things set it apart from orchestration frameworks: an **embedded constitution** and **coordination primitives**.

### Embedded Constitution

Every Decapod binary ships with a compiled-in methodology: binding contracts, authority chains, proof doctrine, and architectural guidance — 40+ documents covering specs, interfaces, architecture, and plugins. Agents don't receive tips; they receive contracts.

`decapod init` generates thin entrypoints (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, `CODEX.md`) that point agents into the constitution. Every agent, regardless of provider, enters through the same contract and follows the same authority ladder:

**Intent > Architecture > Implementation > Proof**

Projects customize behavior through `.decapod/OVERRIDE.md` — extend or adjust any contract without forking the constitution.

### Coordination Primitives

Agents operating in the same repo share durable infrastructure:

- **Architecture decision prompting** — When starting a new project, `decapod decide` walks agents through curated engineering questions (runtime, framework, orchestration, database, etc.) with only the best options. Decisions are stored in SQLite and cross-linked into the knowledge graph as a durable Architecture Decision Record.
- **Proof ledger + knowledge graph** — Decisions, conventions, and proof events survive sessions and model switches. Stored in `.decapod/data/` as SQLite + append-only event logs, forming a repo-native knowledge graph of intent → change → proof.
- **Proof gates** — `decapod validate` is authoritative. If it fails, the change is not complete, regardless of summary confidence.
- **Shared backlog** — A brokered task system with audit trails. Agents claim work, record transitions, and archive completions. No duplicate effort, no lost context.
- **Policy boundaries** — Trust tiers, risk zones, and approval gates. Governance that scales with autonomy.
- **Event sourcing** — Every state mutation goes through a broker that records who did what, when, and why. State can be deterministically rebuilt from events.

## Architecture

```
your-project/
├── AGENTS.md              Agent universal contract (generated)
├── CLAUDE.md              Claude entrypoint (generated)
├── GEMINI.md              Gemini entrypoint (generated)
├── CODEX.md               Codex entrypoint (generated)
└── .decapod/
    ├── data/              State: SQLite DBs + event logs
    ├── generated/         Derived artifacts (auto-managed)
    ├── OVERRIDE.md        Project-specific constitution overrides
    └── README.md          Control plane documentation
```

Agents interact through the CLI control surface. Direct mutation of `.decapod/data/` violates the control-plane contract.

## Security

See [`SECURITY.md`](SECURITY.md). Agents must handle credentials securely — never log, never commit, always rotate. Violations are constitutional breaches.

## Contributing

Decapod is built with Decapod. To develop:

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
decapod validate
```

## Support

- [File an issue](https://github.com/DecapodLabs/decapod/issues)
- [Support on Ko-fi](https://ko-fi.com/decapodlabs)

## License

MIT. See [LICENSE](LICENSE) for details.

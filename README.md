<p align="center"><code>cargo install decapod</code></p>

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

Agents can write code. But they can't reliably **ship** because they forget what they built yesterday, treat best practices as vibes, say "done" without evidence, and trip over each other in parallel. Decapod fixes that.

Decapod is **not** a prompt pack, an agent framework, a hosted SaaS platform, or a review bot. It's infrastructure: the environment where agent work becomes enforceable.

## Quickstart

```bash
cargo install decapod
cd your-project
decapod init
```

That's it. Agents now operate inside the governed environment. You observe outcomes, review summaries, and merge when proofs pass.

## How It Works

**Persistent state** — Agents persist work to `.decapod/`: todos, conventions, decisions, proof events. Durable state that survives sessions and model switches.

**Enforced methodology** — An embedded constitution defines binding contracts for how agents operate. Generated entrypoints (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`, `CODEX.md`, `OPENCODE.md`) require agents to read the constitution, use the control surface, and follow Intent > Architecture > Implementation > Proof. Projects customize via `.decapod/OVERRIDE.md`.

**Proof gates** — Agents must satisfy validation gates before claiming completion. If `decapod validate` fails, the work isn't done. Evidence required, not assertions.

**Coordination** — A shared backlog with audit trail, shared conventions and preferences, a proof ledger, and policy boundaries. Multiple agents work in parallel without collisions.

## CLI Reference

```
decapod init        Bootstrap a project                    (alias: i)
decapod docs        Access methodology documentation       (alias: d)
decapod todo        Track tasks and work items              (alias: t)
decapod validate    Validate methodology compliance         (alias: v)
decapod govern      Governance: policy, health, proofs      (alias: g)
decapod data        Data: archives, knowledge, context
decapod auto        Automation: scheduled, event-driven     (alias: a)
decapod qa          Quality assurance: verification         (alias: q)
```

## Architecture

```
your-project/
├── AGENTS.md              Agent universal contract (generated)
├── CLAUDE.md              Claude entrypoint (generated)
├── GEMINI.md              Gemini entrypoint (generated)
├── CODEX.md               Codex entrypoint (generated)
├── OPENCODE.md            OpenCode entrypoint (generated)
└── .decapod/
    ├── data/              State: SQLite DBs + event logs
    ├── generated/         Derived artifacts (auto-managed)
    ├── OVERRIDE.md        Project-specific constitution overrides
    └── README.md          Control plane documentation
```

Agents interact through the CLI control surface. You don't touch `.decapod/data/` directly.

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

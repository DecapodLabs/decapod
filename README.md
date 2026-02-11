<div align="center">
  <img src="assets/decapod-ultra.svg" width="320" alt="Decapod Logo">

  <h3>A constitutional control plane for AI agent swarms.</h3>

  <p>
    <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://crates.io/crates/decapod"><img alt="Crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  </p>
</div>

---

## The Problem

Coding agents forget context, corrupt state, and drift from intent. Decapod fixes that.

---

## Get Started

```bash
cargo install decapod
cd your-repo && decapod init
```

Point your agent at `CLAUDE.md`, `GEMINI.md`, or `AGENTS.md` in the project root. The agent reads the constitution, learns the interface, and operates within the discipline.

---

## What You Get

```
.decapod/
├── data/                 # Persistent state (SQLite + event logs)
└── constitution/         # Behavioral law the agent internalizes
    └── specs/
        ├── INTENT.md     # What we're building
        └── ARCHITECTURE.md
```

- **One interface** — CLI + schema + store-aware state
- **One authority ladder** — Intent → Spec → Code → Proof → Promotion
- **One proof gate** — `validate` prevents "sounds right" from becoming "is right"

The constitution is embedded in the binary. Your project can override any doc — drop files in `.decapod/constitution/` and your rules take precedence.

---

## What's Real Today

| Subsystem | Status | Purpose |
|-----------|--------|---------|
| `todo` | **REAL** | Coordinated task queue with event log |
| `validate` | **REAL** | Proof gate — enforces store purity |
| `cron` | **REAL** | Scheduled jobs |
| `reflex` | **REAL** | Event → action triggers |
| `docs` | **REAL** | Embedded constitution, project overrides |

**Designed, not yet enforced:** Brokered writes, serialized concurrency, always-on audit. These graduate to REAL when the repo can fail fast on violations.

---

## Why "Decapod"?

Ten-legged crustaceans. Small shell, built for pressure. The kernel stays minimal. Everything else lives in plugins.

---

## Get Involved

**Build the periphery.** Adapters, connectors, proof harnesses. Plugins are first-class.

**Improve the constitution.** Find a pattern that stops agent drift? PR it.

---

<div align="center">
  <p>
    <strong>AI agents will ship code whether we're ready or not.</strong><br>
    Give them discipline.
  </p>

  <a href="https://ko-fi.com/decapodlabs"><img height="36" alt="Support on Ko-fi" src="https://storage.ko-fi.com/cdn/kofi5.png?v=3" /></a>
</div>

<div align="center">
  <img src="assets/decapod-ultra.svg" width="320" alt="Decapod Logo">

  <h3>You gave AI commit access.<br>You forgot to give it discipline.</h3>

  <p>
    <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://crates.io/crates/decapod"><img alt="Crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  </p>
</div>

---

You wouldn't run microservices without coordination. You wouldn't run a database without ACID. But right now, you're running AI agents with write access to production code and *nothing* keeping them honest.

No shared state. No memory. No proof of work. Just vibes.

**Decapod is the missing OS layer.** A local control plane that gives agents persistent state, shared coordination, and a constitution they actually follow.

---

## 30 Seconds to Sanity

```bash
cargo install decapod
cd your-repo && decapod init
```

Point your agent at `CLAUDE.md` or `AGENTS.md`. It reads the constitution, learns the interface, operates within the discipline. You supervise. The system enforces.

---

## What You Get

```
.decapod/
├── data/           # State that survives sessions
└── constitution/   # Law the agent internalizes
```

**One interface.** CLI + schema + store-aware state.
**One authority chain.** Intent → Spec → Code → Proof → Promotion.
**One proof gate.** `validate` — where "sounds right" meets "is right."

---

## Why "Decapod"?

Small shell. Ten legs. Built for pressure.

Kernel stays minimal. Plugins do the rest.

---

## Get Involved

**Ship a plugin.** Adapters, connectors, proof harnesses — first-class citizens.

**Harden the constitution.** Found a pattern that stops drift? PR it. You're writing law.

---

<div align="center">
  <strong>Agents will ship code whether you're ready or not.</strong><br>
  <sub>Make them earn it.</sub>
  <br><br>
  <a href="https://ko-fi.com/decapodlabs"><img height="36" alt="Support on Ko-fi" src="https://storage.ko-fi.com/cdn/kofi5.png?v=3" /></a>
</div>

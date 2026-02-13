<div align="center">
  <img src="assets/decapod-ultra.svg" width="800" alt="Decapod Logo">

  <h2>Unlock Agentic Engineering</h2>

  <p>
    The missing control plane that makes agent autonomy intelligent.
  </p>

  <p>
    <strong>Decapod turns “AI wrote code” into “I can merge this.”</strong>
  </p>

  <p>
    <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://crates.io/crates/decapod"><img alt="Crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  </p>

  <p>
    <strong>Built in Rust 🦀 · Local-first · Repo-native · Works with any agent</strong>
  </p>
</div>

---

<div align="center">
  <h3>Agents capture TODOs. Track progress. Hand off context. Prove their work. Ship.</h3>
</div>

---

## Quick Start

```bash
cargo install decapod
cd your-project
decapod init
```

That's it. `decapod init` creates (and backs up existing) `CLAUDE.md`, `AGENTS.md`, and `GEMINI.md` with methodology your agents follow.

**What agents get:**
- Persistent memory across sessions
- Standard interface (no guessing)
- Proof requirements before claiming "done"

**What you get:**
- Embedded constitution that all agents follow
- Confidence to merge agent work
- Simple overrides via OVERRIDE.md

---

## Security

Decapod is designed with security at the foundation. See [`SECURITY.md`](SECURITY.md) for:
- Credential architecture and lifecycle management
- Agent identity and session security
- Supply chain integrity
- Incident response philosophy

**TL;DR:** Agents must handle credentials securely—never log, never commit, always rotate. Violations are constitutional breaches.

---

## The Problem

AI lowers the barrier to *writing* code—but shipping code is still hard.

Every agent session starts from scratch. Context evaporates. You can't trust it to:
- Remember what it built yesterday
- Follow your standards without drift
- Prove the code works before claiming "done"
- Ship without you checking every line

**Shipping is a system.** Decapod is that system for agentic development.

---

## What It Does

You wouldn't run microservices without coordination. You wouldn't run a database without ACID.
But we're handing agents write access to production repos and hoping "good prompting" substitutes for discipline.

**Decapod turns agent output into an engineering pipeline:**

- **Shared state that survives sessions** — work doesn't reset on handoff
- **One agent-first interface** (CLI + schemas) — agents don't poke internals
- **One authority chain** — Intent → Spec → Code → Proof → Merge
- **Proof gates** — "sounds right" can't ship without evidence
- **Full traceability** — what changed, who changed it, why

---

## Env Layout

```text
.decapod/
├── data/        # state that survives sessions   (DON'T TOUCH)
├── generated/   # generated system files         (DON'T TOUCH)
└── OVERRIDE.md  # user managed override config   (BELONGS TO YOU)
```

**Local-first by design:**
- No daemon
- No hosted service
- No "agent memory SaaS"
- Just a repo-native file-based control plane

---

## Who This Is For

✅ You're building real products with AI agents<br>
✅ You want CI/CD discipline, not "vibes-based" shipping<br>
✅ You need multiple agents working without chaos<br>
✅ You merge to production (not just prototyping)

If you want a one-off script, Decapod is overkill.
If you want agents to ship production code, Decapod is the missing layer.

---

## Get Involved

- **Ship a subsystem** — adapters, proof harnesses, connectors
- **Harden the constitution** — if you found a rule that stops drift, PR it upstream
- **Break it** — open issues with repros (they become proof gates)
- **Share overrides** — Found a useful project pattern? Add it to OVERRIDE.md examples

---

<div align="center">
  <strong>Agents will ship code whether you're ready or not.</strong><br>
  <sub>Make them earn the merge.</sub>
  <br><br>
  <a href="https://github.com/DecapodLabs/decapod">⭐ Star on GitHub</a> •
  <a href="https://crates.io/crates/decapod">📦 Crates.io</a> •
  <a href="https://ko-fi.com/decapodlabs">☕ Support</a>
</div>

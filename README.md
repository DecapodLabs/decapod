<div align="center">
  <svg width="240" height="120" viewBox="0 0 240 120" xmlns="http://www.w3.org/2000/svg">
    <g fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <!-- Central carapace segment -->
      <path d="M100 30 Q 120 20, 140 30" stroke-width="4" />
      <!-- Hind legs - set 1 -->
      <path d="M105 45 L 80 60 L 50 85" />
      <path d="M135 45 L 160 60 L 190 85" />
      <!-- Hind legs - set 2 -->
      <path d="M110 60 L 90 80 L 65 105" />
      <path d="M130 60 L 150 80 L 175 105" />
      <!-- Hind legs - set 3 -->
      <path d="M115 75 L 105 95 L 90 115" />
      <path d="M125 75 L 135 95 L 150 115" />
    </g>
  </svg>
</div>

# Decapod

[![CI](https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg)](https://github.com/DecapodLabs/decapod/actions)
[![Crates.io](https://img.shields.io/crates/v/decapod.svg)](https://crates.io/crates/decapod)

**Decapod is a Project OS for Machines.** A Rust-powered, agent-executed communal workspace where AI swarms coordinate without babysitting. Humans steer from the outside; agents execute inside a deterministic, shared environment built for machine consumption—durable state, verifiable proofs, and repeatable handoffs.

### Built for Agents, Not Humans
Decapod ignores “user experience” in favor of “agent efficiency.” Every interface is a CLI-as-API contract. There are no dashboards or chat bubbles—only machine-readable state, verifiable proofs, and deterministic handoffs. It’s the communal nervous system that lets parallel loops like OpenClaw and coding agents (Claude, OpenCode, Codex, Gemini) stay aligned by sharing the same project-local memory concurrently, across executions, and across models—so your swarm doesn’t splinter into parallel realities.

### The Ecosystem
The Decapod core is a minimal kernel designed for state integrity and orchestration. All functional power lives in the periphery: plugins—connectors, adapters, caches, and workflow modules that bridge the kernel to external systems. This separation keeps the core stable while the ecosystem evolves fast, and you can override each component as needed. We actively want contributors shipping first-class periphery plugins—this is where Decapod meets the world.

### Contributing (Core + Periphery)
Want maximum impact fast? Build the periphery: connectors, adapters, caches, proof/eval harnesses, and workflow modules that make agents actually useful in real environments. Core PRs are welcome too—but periphery plugins are first-class citizens here, not “extras.”

## Get started

```bash
# 1) Install Decapod (once)
cargo install decapod

# 2) Initialize in your project repo
cd your-project-repo
decapod init
```

Running `decapod init` will:
- Create the `.decapod/` directory structure.
- Scaffold root agent entrypoints (`AGENTS.md`, `GEMINI.md`, `CLAUDE.md`).

**Safe initialization:** If any root entrypoints already exist, `decapod init` will safely back them up to `<file>.md.bak` before writing new ones.

## Hand the wheel to an agent

Once you’ve run `init`, tell your AI to read `AGENTS.md`, `GEMINI.md`, or `CLAUDE.md` in the project root. From there, the agent learns how to work the system:

- It reads its methodology (the constitution) directly from the binary: `decapod docs show core/DECAPOD.md`
- It records progress and facts via the `decapod` CLI.
- It keeps `decapod validate` passing after every change.

## On-disk layout

```text
your-project/
├── AGENTS.md               <-- Rules of engagement
├── CLAUDE.md
├── GEMINI.md
└── .decapod/               <-- Decapod control plane state
    ├── README.md           (Internal guide)
    ├── data/               (Persistent state - SQLite DBs & event logs)
    └── constitutions/      (Methodology overrides & living project intelligence)
        ├── specs/
        │   ├── INTENT.md
        │   └── ARCHITECTURE.md
        └── core/
            └── ...
```

## What it is / What it isn’t

- **YES:** A communal workspace for AI agents.
- **YES:** A source-controlled source of truth.
- **YES:** Enforceable methodology.
- **NO:** A hosted service or “chat with your docs” app.
- **NO:** A framework that forces you to rewrite your code.
- **NO:** MCP or a proprietary plugin system.

## The Living Constitution

Decapod’s methodology is open source and embedded in the engine. When you contribute a better workflow pattern to our `constitution/` directory, you’re helping upgrade the “firmware” for every AI agent using Decapod.

If you’ve found a way to stop an agent from hallucinating context or drifting from intent, [open a PR](https://github.com/DecapodLabs/decapod/compare).

---

If Decapod helps your swarm stay comfy, [sponsor the work](https://github.com/sponsors/DecapodLabs) or drop a star. Keep the shell clean. 🦀

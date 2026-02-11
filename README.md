<div align="center">
  <img src="assets/decapod-ultra.svg" width="320" alt="Decapod Logo">
  <p>
    🦀 A Rust-built, repo-native control-plane kernel for AI swarms — safe shared state, enforced truth, loop-agnostic orchestration.
  </p>

  <p>
    <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://crates.io/crates/decapod"><img alt="Crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
    <a href="https://ko-fi.com/decapodlabs">
      <img alt="Ko-fi" height="28" src="https://storage.ko-fi.com/cdn/kofi2.png?v=3">
    </a>
  </p>
</div>

---

### Why “Decapod”?
A decapod is a ten-legged crustacean (crabs and lobsters). Tough shell, relentless grip, built to survive pressure. That’s the vibe: a small kernel that keeps your swarm grounded while it crawls the real world. 🦀🦞

## Get started

~~~bash
# 1) Install Decapod (once)
cargo install decapod

# 2) Initialize in your project repo
cd your-project-repo
decapod init
~~~

Running `decapod init` will:
- Create the `.decapod/` directory structure.
- Scaffold root agent entrypoints (`AGENTS.md`, `GEMINI.md`, `CLAUDE.md`).

**Safe initialization:** If any root entrypoints already exist, `decapod init` will safely back them up to `<file>.md.bak` before writing new ones. 

After initialization, if you have backups, open your agent of choice and tell it to: 
> "Blend the `*.md.bak` files into my `.decapod/constitutions/` overrides."

## Project OS for Machines

Decapod turns “a bunch of agents” into an actual system. Not chat logs. Not vibes. A shared, deterministic workspace where agents can work in parallel without inventing parallel realities. You steer. The swarm executes. The kernel keeps everyone honest.

### Built for Agents, Not Humans
Decapod optimizes for **agent efficiency** over “UX.” Every interface is a CLI-as-API contract. No dashboards. No chat bubbles. Just machine-readable state, proof surfaces, and deterministic handoffs. Run a coding agent in parallel with OpenClaw and other loopers: while you direct one, the rest can read/write the same Decapod workspace for coordination, todos, caching, and clean handoffs.

### The Ecosystem
The core stays small on purpose: a minimal kernel for state integrity and orchestration. The blast radius stays tight. The ecosystem stays wild. All functional power lives in the periphery—plugins (connectors, adapters, caches, workflow modules) that let Decapod touch the real world without bloating the kernel. We want contributors shipping periphery plugins as first-class citizens. This is where Decapod becomes inevitable.

### Contributing (Core + Periphery)
Want maximum impact fast? Build the periphery. New connectors, adapters, caches, proof/eval harnesses, and workflow modules that make agents useful in real environments. Core PRs are welcome too—but periphery plugins are not “extras.” They’re the expansion pack.

## Hand the wheel to an agent

Once you’ve run `init`, tell your AI to read `AGENTS.md`, `GEMINI.md`, or `CLAUDE.md` in the project root. From there, the agent learns how to work the system:

- It reads its methodology (the constitution) directly from the binary: `decapod docs show core/DECAPOD.md`
- It records progress and facts via the `decapod` CLI.
- It keeps `decapod validate` passing after every change.

## On-disk layout

~~~text
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
~~~

## What it is / What it isn’t

- **YES:** A communal workspace for AI agents.
- **YES:** A source-controlled source of truth.
- **YES:** Enforceable methodology.
- **NO:** A hosted service or “chat with your docs” app.
- **NO:** A framework that forces you to rewrite your code.
- **NO:** MCP or a proprietary plugin system.

## The Living Constitution

Decapod’s methodology is open source and embedded in the engine. When you contribute a better workflow pattern to our `constitution/` directory, you’re helping upgrade the “firmware” for every AI agent using Decapod.

If you’ve found a way to stop an agent from hallucinating context or drifting from intent, open a PR.

---

If Decapod helps your swarm stay comfy, sponsor the work, drop a star, or fuel the kernel on Ko-fi 🦀  
<a href="https://ko-fi.com/decapodlabs"><img height="36" alt="Support DecapodLabs on Ko-fi" src="https://storage.ko-fi.com/cdn/kofi5.png?v=3" /></a>


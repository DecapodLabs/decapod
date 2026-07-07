<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  Repo-native governance for AI coding agents.
</p>

<p align="center">
  Decapod is a daemonless, local-first kernel that agents call when coding work needs intent, context, boundaries, coordination, or proof.
  You keep working in Cursor, Claude Code, Codex, Antigravity, or any other agent tool; Decapod gives those agents a shared control plane inside the repo.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

Canonical Contract: `assets/constitution.json` section `core/DECAPOD`

---

## Quick Start

```bash
cargo install decapod
decapod init
```

`decapod init` creates `.decapod/`, the repo-native substrate your agent uses to turn intent, rules, context, custody, validation, and completion into inspectable project state.

Your **conversational** workflow does not change. You keep working through your agent; Decapod gives the agent the missing control plane. Intent is captured, scope is bounded, context is shaped, protected areas are respected, work is isolated, and completion is proven against the project's rules and the Decapod constitution.

Agent conversations are temporary. Repo state is durable. Decapod preserves the parts of agent work that should not live only in a chat transcript, so a later agent, reviewer, CI run, or human maintainer can recover what was requested, what was understood, what boundaries applied, what changed, what validation ran, and what remains unresolved.

---

## How it works

AI coding agents often lose the plot: they forget intent, pull too much context, skip dependencies, and touch protected files. Decapod gives them a repo-native governance layer that makes intent explicit, boundaries enforceable, context deliberate, and completion provable.

### The Loop

```mermaid
flowchart TD
    UserIn["User"] -->|"intent"| AgentPre["Agent (Pre)"]
    AgentPre -->|"governed request"| Model["Model"]
    Model -->|"response"| AgentPost["Agent (Post)"]
    AgentPost -->|"verified result"| UserOut["User"]

    AgentPre -.->|"ping for context"| UserIn

    AgentPre -. "optional governance path" .-> DecapodPre["Decapod (Pre)"]
    DecapodPre -. "intent, context, gates" .-> AgentPre

    AgentPost -. "optional proof path" .-> DecapodPost["Decapod (Post)"]
    DecapodPost -. "boundaries, checks, proof" .-> AgentPost
    DecapodPost -. "needs more context" .-> AgentPre

    style UserIn fill:#ff6b9d,stroke:#c44569,color:#fff
    style UserOut fill:#ff6b9d,stroke:#c44569,color:#fff
    style AgentPre fill:#a855f7,stroke:#7c3aed,color:#fff
    style AgentPost fill:#a855f7,stroke:#7c3aed,color:#fff
    style Model fill:#06b6d4,stroke:#0891b2,color:#fff
    style DecapodPre fill:#fbbf24,stroke:#f59e0b,color:#000
    style DecapodPost fill:#fbbf24,stroke:#f59e0b,color:#000
```

**Agent ↔ User pings** — The 1st agent (governance) and 2nd agent (proof) can ping the user for additional context when intent is unclear or verification needs human input.

Decapod is called by the agent at governance boundaries. Before inference, the agent may branch into Decapod to shape intent, context, and gates. After inference, the agent may branch into Decapod when the work needs boundary checks, verification, proof, or another governed pass.

Each Decapod call may recurse until the work is shaped, bounded, and provable. Decapod is not the agent and not the model; it is the governance kernel the agent calls whenever work needs control.

Decapod is called before:

- **Acting** — clarify intent and generate specs
- **Inference** — resolve focused context capsules
- **Touching Code** — enforce boundaries and protected paths
- **Completing** — produce verification and proof

---

## Capabilities

1. **Clarifies intent** — Converts vague requests into explicit, versioned specifications.
2. **Bounds context** — Resolves only the minimal relevant code/docs for the task.
3. **Coordinates concurrent agents** — Lets Cursor, Claude Code, Codex, Gemini CLI, and other tools work against the same repo at the same time without duplicating work, trampling workspaces, or losing state.
4. **Enforces boundaries** — Safeguards protected branches and sensitive modules.
5. **Governs adaptation** — Manages feedback-driven instruction changes through explicit review.
6. **Requires proof** — Gates completion on deterministic verification artifacts.

---

## The substrate

Decapod preserves what agent workbenches lose: governed project state that survives a session, a tool switch, a crash, a retry, or a handoff.

`.decapod/` is the repo-native substrate for governed agent execution. It records the durable state Decapod needs to keep work bounded, attributable, resumable, and provable without depending on any one model provider, agent workbench, or conversation transcript.

```text
.decapod/
  generated/
    specs/         # Human-visible intent and architecture specs
    context/       # Deterministic context capsules
    artifacts/     # Verification output and proof provenance
  data/            # Durable repo-native state (DBs, events, todos)
  config.toml      # Project shape and agent-facing configuration
  OVERRIDE.md      # Local rules that override embedded defaults
```

The substrate turns the important parts of agent work into durable repo state:

- **Intent** becomes specs and todos instead of an implicit prompt.
- **Context** becomes scoped capsules instead of whatever fit in chat history.
- **Custody** becomes claimed tasks and isolated workspaces instead of informal ownership.
- **Boundaries** become project rules, overrides, and validation gates instead of reminders.
- **Validation** becomes proof artifacts and receipts instead of a final assertion.
- **Completion** becomes a verified state transition instead of "looks done".

Every governed run leaves operational evidence. The generated files are the human-visible proof surface: inspect them locally, review them in PRs, and use them to re-establish state across different agents like Claude, Codex, Gemini, Cursor, and Kilo.

Decapod does not make agents smarter by giving them longer conversations. Decapod makes agent work shippable by turning intent, context, boundaries, custody, validation, and completion into governed repo state.

---

## The constitution

Decapod ships with an embedded engineering constitution: over 100 declarative documents covering architecture, security, performance, and testing.

Everything an engineering org usually keeps in tribal memory or review culture becomes executable guidance. Your agent does not guess; it reads the constitution, cites claim IDs, follows gates, and produces proof.

---

## Guarantees

- **Daemonless** — Runs on demand like `git` or `grep`.
- **Repo-native** — All state lives in your repository.
- **Provider-agnostic** — Works across agent workbenches.
- **Proof-gated** — Completion requires passed verification gates.
- **Boundary-aware** — Enforces protected paths and branch isolation.

Decapod is not an agent framework, prompt pack, model router, or generic orchestrator. It is the repo-native governance layer agents call when work needs bounded execution, coordination, continuity, and proof.

---

## Documentation

Decapod provides comprehensive documentation for both human operators and AI agents.

- **[Human Documentation (mdBook)](https://decapodlabs.github.io/decapod/)**: Conceptual overview, workflows, adoption guide, and reference.
- **[Agent Orientation Corpus](docs/agent/api-index.md)**: API-awareness layer for agents, including command contracts and payload examples.
- **[Universal Agent Contract (AGENTS.md)](AGENTS.md)**: The machine-readable entrypoint for all agents operating in this repo.

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build && cargo test
```

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [SECURITY.md](SECURITY.md)
- [Issues](https://github.com/DecapodLabs/decapod/issues)

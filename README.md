<p align="center">🦀</p>

<p align="center">
  <code>cargo binstall decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  Repo-native governance kernel for AI coding agents.
</p>

<p align="center">
  You keep working in Cursor, Claude Code, Codex, Antigravity, or any other harness; Decapod gives the agents operating there a shared governance kernel inside the repository.
</p>

<p align="center">
  Decapod is a local-first, daemonless, repo-native governance kernel that agents call at governance boundaries — before acting, before inference, before touching code, before completing — to shape intent, bound context, enforce boundaries, and produce proof.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

Canonical Contract: [assets/constitution.json (core/DECAPOD)](assets/constitution.json#L5)

Paper: [Accountable Agentic Execution (Raber, 2026)](https://decapodlabs.github.io/accountable-agentic-execution/)

---

## Quick Start

```bash
cargo binstall decapod
decapod init
```

`decapod init` creates `.decapod/`, the repo-native substrate your agent uses to turn intent, rules, context, custody, validation, and completion into inspectable project state.

Agent conversations are temporary. Repo state is durable. Decapod preserves the parts of agent work that should not live only in a chat transcript, so a later agent, reviewer, CI run, or human maintainer can recover what was requested, what was understood, what boundaries applied, what changed, what validation ran, and what remains unresolved.

---

## How it works

AI coding agents often lose the plot: they forget intent, pull too much context, skip dependencies, and touch protected files. Decapod gives them a repo-native governance layer that makes intent explicit, boundaries enforceable, context deliberate, and completion provable.

### The Loop

```mermaid
flowchart LR
    subgraph HumanGroup["Human"]
        UserIn["User"]
        UserOut["User"]
    end

    subgraph HarnessGroup["Agent Harness"]
        HarnessNode["Harness"]
    end

    subgraph IntelligenceGroup["Intelligence"]
        ModelNode["Model<br/>(LLM)"]
    end

    subgraph GovernanceGroup["Governance Kernel"]
        DecapodNode["Decapod"]
    end

    subgraph AgentGroup["Agent"]
        AgentNode["Agent"]
    end

    UserIn ==>|intent| HarnessNode
    HarnessNode ==>|governed request| AgentNode

    %% Optional pre-inference governance
    AgentNode -.->|"may call Decapod<br/>(pre-inference)"| DecapodNode
    DecapodNode -.->|"intent, context,<br/>gates"| AgentNode

    AgentNode ==>|inference| ModelNode
    ModelNode ==>|response| AgentNode

    %% Optional post-inference verification and proof
    AgentNode -.->|"may call Decapod<br/>(post-inference)"| DecapodNode
    DecapodNode -.->|"boundaries,<br/>checks, proof"| AgentNode

    AgentNode ==>|verified result| UserOut

    AgentNode -.->|"clarification ping"| UserIn

    style UserIn fill:#ff6b9d,stroke:#c44569,color:#fff
    style UserOut fill:#ff6b9d,stroke:#c44569,color:#fff
    style HarnessNode fill:#3b82f6,stroke:#2563eb,color:#fff
    style AgentNode fill:#a855f7,stroke:#7c3aed,color:#fff
    style ModelNode fill:#06b6d4,stroke:#0891b2,color:#fff
    style DecapodNode fill:#fbbf24,stroke:#f59e0b,color:#000

    style HumanGroup fill:#f3f4f6,stroke:#d1d5db,color:#000
    style HarnessGroup fill:#eff6ff,stroke:#bfdbfe,color:#000
    style IntelligenceGroup fill:#ecfdf5,stroke:#a7f3d0,color:#000
    style GovernanceGroup fill:#fef9c3,stroke:#fde047,color:#000
    style AgentGroup fill:#faf5ff,stroke:#d8b4fe,color:#000
```

**Harness ↔ User pings** — The harness can ping the user for additional context when intent is unclear or verification needs human input.

Decapod is called by the agent at governance boundaries. Before inference, the agent *may* branch into Decapod to shape intent, context, and gates. After inference, the agent *may* branch into Decapod when the work needs boundary checks, verification, proof, or another governed pass. Each call may recurse until the work is shaped, bounded, and provable. Decapod is not the agent and not the model; it is the governance kernel the agent calls whenever work needs control.

Decapod is called before:

- **Acting** — clarify intent and generate specs
- **Inference** — resolve focused context capsules
- **Touching Code** — enforce boundaries and protected paths
- **Completing** — produce verification and proof

---

## Capabilities

1. **Clarifies intent** — Converts vague requests into explicit, versioned specifications.
2. **Bounds context** — Resolves only the minimal relevant code and docs for the task.
3. **Coordinates concurrent agents** — Lets the harness work against the same repo simultaneously without duplicating work, trampling workspaces, or losing state.
4. **Enforces boundaries** — Safeguards protected branches and sensitive modules.
5. **Governs adaptation** — Manages feedback-driven instruction changes through explicit review.
6. **Requires proof** — Gates completion on deterministic verification artifacts.

---

## The substrate

Decapod preserves what agent workbenches lose: governed project state that survives a session, a tool switch, a crash, a retry, or a handoff.

`.decapod/` is the repo-native substrate for governed agent execution. It records the durable state Decapod needs to keep work bounded, attributable, resumable, and provable — without depending on any one model provider, agent workbench, or conversation transcript.

```
.decapod/
  managed/
    specs/         # Living specs (INTENT, ARCHITECTURE, INTERFACES, OPERATIONS, README, SECURITY, SEMANTICS, VALIDATION) — tracked
    sessions/      # Agent session custody and correlation — tracked
  generated/
    awareness/     # Deterministic context capsules — generated at runtime
    artifacts/     # Verification output and proof provenance — generated at runtime
  data/            # Durable repo-native state — untracked (optionally select `backend=cloud` in `.decapod/config.toml` or during `decapod init`)
  governance/      # Living evidence (trajectory, proof rubrics, validation receipts, research claims) — tracked
  workspaces/      # Isolated git worktrees (container workspaces require explicit opt-in) — created on demand
  config.toml      # Project shape and agent-facing configuration — tracked
  OVERRIDE.md      # Plain-Markdown local authority; Decapod derives resolution proof — tracked
```

The substrate turns the important parts of agent work into durable repo state:

- **Intent** becomes specs and todos instead of an implicit prompt.
- **Context** becomes scoped capsules instead of whatever fit in chat history.
- **Custody** becomes claimed tasks and isolated workspaces instead of informal ownership.
- **Boundaries** become project rules, overrides, and validation gates instead of reminders.
- **Validation** becomes proof artifacts and receipts instead of a final assertion.
- **Completion** becomes a verified state transition instead of "looks done".

Every governed run leaves operational evidence. The generated files are the human-visible proof surface: inspect them locally, review them in PRs, and use them to re-establish state across different agents like Cursor, Codex, Gemini, and Kilo.

After `cargo install decapod`, the next normal Decapod invocation autonomously
reconciles unproven event JSONL into canonical SQLite before runtime reads. A
durable single-datastore migration receipt retires already-imported JSONL and
repairs any legacy SQLite stores recreated by an older binary without rereading
the archives as live authority. Existing
override bodies are rendered into a fenced source area under each exact current
directive subsection. Humans replace the visible instruction inside that block
with Markdown or any documentation style they prefer; Decapod extracts the body
without rendering it as document structure. Existing body bytes are preserved,
and ambiguous binding authority fails closed with derived source/body hashes and byte counts.

Decapod does not make agents smarter by giving them longer conversations. It makes agent work shippable by turning intent, context, boundaries, custody, validation, and completion into governed repo state.

---

## The constitution

Decapod ships with an embedded engineering constitution: 100+ embedded constitution documents covering architecture, security, performance, and testing.

Agents consult the constitution, cite claim IDs, follow gates, and produce proof — reducing guesswork but not eliminating the need for judgment.

---

## Guarantees

- **Daemonless** — Runs on demand like `git` or `grep`.
- **Local-first** — Ordinary governance runs locally without requiring a persistent hosted service.
- **Repo-native** — Governed state remains durable and inspectable with the repository.
- **Provider-agnostic** — Works with any model provider, agent harness, or toolchain (behavior may vary per integration).
- **Completion requires passed proof-plan gates** — `VERIFIED` status requires passed proof-plan gates (INV-PROOF-GATED).
- **Enforces protected paths and branch isolation (configured)** — Protected paths and branch isolation enforced per `.decapod/config.toml`.

Decapod is not an agent framework, prompt pack, model router, or generic orchestrator. It is the repo-native governance kernel agents call when work needs bounded execution, coordination, continuity, and proof.

---

## Documentation

Decapod provides comprehensive documentation for both human operators and AI agents.

- **[Human Documentation (mdBook)](https://decapodlabs.github.io/decapod/)**: Conceptual overview, workflows, adoption guide, and reference.
- **[Agent API Index](docs/agent/api-index.md)** — Contracts and interfaces for agents integrating with Decapod.
- **[Universal Agent Contract (AGENTS.md)](AGENTS.md)**: The machine-readable entrypoint for all agents operating in this repo.

---

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build && cargo test
```

- [CONTRIBUTING.md](CONTRIBUTING.md)
- [SECURITY.md](SECURITY.md)
- [Issues](https://github.com/DecapodLabs/decapod/issues)

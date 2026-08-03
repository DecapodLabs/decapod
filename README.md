<p align="center">🦀</p>

<p align="center">
  <code>cargo binstall decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  Repo-native governance for AI coding agents.<br />
  <em>Intent · Custody · Trajectory · Proof — <strong>fleet coherence</strong></em>
</p>

<p align="center">
  Work in Cursor, Claude Code, Codex, Antigravity, Grok, or any harness you prefer.
  Decapod is the shared governance layer inside the repository —
  so agent work stays bounded, attributable, and recoverable.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="assets/constitution.json#L5"><img alt="Constitution" src="https://img.shields.io/badge/Constitution-core%2FDECAPOD-0A66C2"></a>
  <a href="https://decapodlabs.github.io/accountable-agentic-execution/"><img alt="Research" src="https://img.shields.io/badge/Research-Accountable%20Agentic%20Execution-blueviolet"></a>
</p>

---

## Quick Start

```bash
cargo binstall decapod
decapod init
```

Nix users can build and run Decapod straight from the repository flake — the
dependency closure derives from the committed `Cargo.lock`, so no hashes need
computing:

```bash
nix run github:DecapodLabs/decapod -- init
nix build github:DecapodLabs/decapod    # binary at ./result/bin/decapod
nix develop github:DecapodLabs/decapod  # contributor shell
```

One command installs the kernel. One command prepares the repository.
You keep speaking naturally. Agents call Decapod at the points that matter —
before acting, before inference, and before claiming done.

What was asked, what was understood, what changed, and what was proven
is written into the project itself.

---

## Fleet coherence

Independently launched agents need one project authority: distinct claims,
a selected governance record, and a shared proof boundary for completion.

| | |
| --- | --- |
| **Intent** | Outcome, constraints, and completion standard — carried in plans, todos, and specs. |
| **Custody** | Exclusive claims and isolated workspaces so agents do not silently collide. |
| **Trajectory** | Selected events so a later process can reorient without replaying a vendor session. |
| **Proof** | Validation bound to governed state. Completion is a verified transition. |

The hard case is concurrent work that is *similar but distinct*:
shared modules, different outcomes. Worktrees stop file trampling.
Claims, context, trajectory, and publication gates keep the fleet coherent.

You speak to whichever agent is available. The agent calls Decapod.
Humans keep natural language. Agents hold a stable machine contract.

---

## How it works

Decapod sits between the harness and the model as a governance kernel —
not another agent, and not a model router.

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
        ModelNode["Model"]
    end

    subgraph GovernanceGroup["Governance"]
        DecapodNode["Decapod"]
    end

    subgraph AgentGroup["Agent"]
        AgentNode["Agent"]
    end

    UserIn ==>|intent| HarnessNode
    HarnessNode ==>|request| AgentNode
    AgentNode -.->|pre-inference| DecapodNode
    DecapodNode -.->|context, gates| AgentNode
    AgentNode ==>|inference| ModelNode
    ModelNode ==>|response| AgentNode
    AgentNode -.->|post-inference| DecapodNode
    DecapodNode -.->|proof| AgentNode
    AgentNode ==>|verified result| UserOut
    AgentNode -.->|clarify| UserIn

    style UserIn fill:#9f1239,stroke:#4c0519,color:#ffffff
    style UserOut fill:#9f1239,stroke:#4c0519,color:#ffffff
    style HarnessNode fill:#1e40af,stroke:#172554,color:#ffffff
    style AgentNode fill:#6b21a8,stroke:#3b0764,color:#ffffff
    style ModelNode fill:#155e75,stroke:#083344,color:#ffffff
    style DecapodNode fill:#111827,stroke:#f59e0b,color:#fbbf24,stroke-width:3px
    style HumanGroup fill:#fff1f2,stroke:#9f1239,color:#4c0519,stroke-width:2px
    style HarnessGroup fill:#dbeafe,stroke:#1e40af,color:#172554,stroke-width:2px
    style IntelligenceGroup fill:#cffafe,stroke:#155e75,color:#083344,stroke-width:2px
    style GovernanceGroup fill:#fef3c7,stroke:#b45309,color:#78350f,stroke-width:2px
    style AgentGroup fill:#f3e8ff,stroke:#6b21a8,color:#3b0764,stroke-width:2px
    linkStyle default stroke:#334155,stroke-width:1.5px
```

The model stays fixed. The corridor around it changes:
resolve project context before implementation, gate completion on proof.
A later agent can re-enter the same work without an ad hoc briefing.

---

## Capabilities

1. **Intent** — Vague requests become explicit, versioned specifications.
2. **Context** — Only the relevant code and docs, with provenance.
3. **Coordination** — Exclusive claims and isolated workspaces for concurrent agents.
4. **Trajectory** — Durable events for handoff across sessions and harnesses.
5. **Boundaries** — Protected branches and sensitive modules stay protected.
6. **Adaptation** — Instruction changes go through explicit review.
7. **Proof** — Completion requires deterministic verification.

---

## The substrate

`.decapod/` holds the durable state of governed agent work:

```
.decapod/
  managed/specs/     # Living project specifications
  managed/sessions/  # Session custody
  generated/         # Context capsules and proof artifacts
  data/              # Local project store
  governance/        # Trajectory, claims, and validation
  workspaces/        # Isolated worktrees (containers when configured)
  config.toml        # Project shape
  OVERRIDE.md        # Local authority, in plain Markdown
```

- **Intent** becomes specs and todos.
- **Context** becomes scoped capsules.
- **Custody** becomes claimed tasks and workspaces.
- **Trajectory** becomes event-backed records.
- **Boundaries** become rules, overrides, and gates.
- **Validation** becomes proof artifacts.
- **Completion** becomes a verified state transition.

`OVERRIDE.md` is yours to edit. Write instructions the way you write documentation.
Decapod binds each directive to its source and body with fail-closed resolution.

Decapod does not make agents smarter with longer chats.
It makes agent work shippable by turning governance into repository state.

---

## The constitution

Decapod ships with an embedded engineering constitution —
architecture, security, performance, and testing as queryable doctrine.

Agents consult it, cite claims, follow gates, and produce proof.
Judgment remains human. Authority is explicit:
baseline constitution, project override, task-scoped projection.

---

## Guarantees

- **Daemonless** — Invoked like `git` or `grep`.
- **Local-first** — Governance runs on your machine by default.
- **Repo-native** — State stays with the repository.
- **Provider-agnostic** — Any model, any harness. The project is the coordination surface.
- **Proof-gated completion** — `VERIFIED` requires passed proof-plan gates.
- **Branch isolation** — Protected paths and branches stay protected.

Decapod is a governance kernel — not an agent framework, prompt pack, or model router.

---

## Documentation

- [Human docs](https://decapodlabs.github.io/decapod/)
- [Agent API](docs/agent/api-index.md)
- [Agent contract](AGENTS.md)
- [Paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf)
- [Research](https://decapodlabs.github.io/accountable-agentic-execution/)
- [Fleet coherence protocol](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/docs/fleet-coherence-protocol.md)

---

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build && cargo test
```

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

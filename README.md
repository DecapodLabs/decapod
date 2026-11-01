
<p align="center">🦀</p>

<h1 align="center">Decapod</h1>

<p align="center"><strong>Give your coding agents a target. Make them prove they reached it.</strong></p>

<p align="center">
  Works with Cursor, Claude Code, Codex, Amp, Pi, Antigravity, Grok, and other coding agents.<br />
  Keep your harness. Govern the work.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="assets/constitution.json#L5"><img alt="Constitution: core/DECAPOD" src="https://img.shields.io/badge/Constitution-core%2FDECAPOD-0A66C2"></a>
  <a href="https://decapodlabs.github.io/accountable-agentic-execution/"><img alt="Research" src="https://img.shields.io/badge/Research-Accountable%20Agentic%20Execution-blueviolet"></a>
</p>

Coding agents are fast. They also misunderstand requirements, lose context, repeat failed approaches, and sometimes declare work complete before the important checks have passed. Running more agents makes those problems harder to track.

Decapod gives agentic development a persistent engineering record and a way to check progress against the original intent. You describe what you want to build. Your agent plans and implements it. Decapod keeps the requirements, decisions, task ownership, and validation evidence attached to the repository, so the work can continue across agents and sessions without starting over.

**Decapod validates that the work converges on what you asked for, and that its evidence is persistence for future reference.**

## Get started

Run this inside your project:

```bash
cargo binstall decapod && decapod init
```

Then open your preferred coding agent and describe the change you want. Decapod supplies the project guidance and workflow the agent uses to plan, implement, and validate the work.

## What changes with Decapod

| Without a durable workflow | With Decapod |
| --- | --- |
| Requirements drift as the conversation grows | Project specs and plans preserve intended behavior and scope |
| A new agent must reconstruct what happened | Recorded decisions and work history support handoffs |
| Parallel agents can step on the same work | Task claims, overlap checks, and isolated workspaces make ownership explicit |
| Engineering rules depend on what fits in a prompt | A shared Constitution and project overrides provide guidance |
| “Done” can mean the agent stopped working | Configured checks and recorded evidence determine what has actually been verified |

Decapod does not replace the agent's judgment or guarantee correctness. It makes the work easier to inspect, continue, and challenge.

## How it works

**1. Establish intent.** Project specs describe the system's purpose, architecture, and expected behavior. The agent creates a scoped plan before implementation.

**2. Execute with context.** The agent retrieves relevant guidance with source references, follows project rules, and records decisions as it works. Multiple agents can claim separate tasks and use isolated workspaces.

**3. Check the result.** Decapod runs the configured validation workflow and associates results with the current change. Missing required evidence leaves the work unverified, rather than silently treating it as complete.

**4. Continue or hand off.** Another agent can pick up the same pull request using the recorded plan, decisions, work history, and validation state—not a reconstructed chat transcript.

The agent still writes code, runs project tools, and prepares the pull request. You still review and merge it.

## Built for the tools you already use

Decapod is a **Rust CLI**, not a resident service or inference router. Each invocation runs, returns its result, and exits. No daemon is required.

Its state lives with the project:

```text
.decapod/
├── governance.json       # Plans, work history, claims, validation results
├── managed/specs/        # Purpose, architecture, expected behavior
├── workspaces/           # Isolated task checkouts
├── config.toml           # Project and validation settings
└── OVERRIDE.md           # Project-specific engineering guidance
```

The validation pipeline runs locally by default, with optional remote Buildkite preflight. Decapod works alongside your existing tracker, CI, and code review process.

**Boundary:** Decapod governs the workflow agents use through it. It cannot guarantee the behavior of commands executed outside that workflow, and validation is only as comprehensive as the checks you configure.

[Governed execution architecture](docs/architecture/governed-execution.md) · [Configuration reference](docs/book/src/reference/config-toml.md)

## The Constitution

Decapod includes engineering guidance for architecture, security, testing, and other development decisions. Agents consult it during planning and implementation; `.decapod/OVERRIDE.md` adds project-specific instructions. When a decision requires human judgment, the conflict remains visible instead of being quietly resolved by the model.

## Documentation and research

[Documentation](https://decapodlabs.github.io/decapod/) · [Agent API](docs/agent/api-index.md) · [Agent contract](AGENTS.md) · [Research paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf) · [Research site](https://decapodlabs.github.io/accountable-agentic-execution/)

## Contributing

[Contributing guide](CONTRIBUTING.md) · [Security](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

<p align="center">🦀</p>

<p align="center">
  <strong>Decapod</strong><br />
  AI coding work you can verify.
</p>

<p align="center">
  Work in Cursor, Claude Code, Codex, Antigravity, Grok, or your preferred coding agent.
  Decapod governs the work while you keep using the same tools.
  Decapod can use <a href="https://docs.typesafe.ai/introduction">Jev</a> for System 1 decisions.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="assets/constitution.json#L5"><img alt="Constitution: core/DECAPOD" src="https://img.shields.io/badge/Constitution-core%2FDECAPOD-0A66C2"></a>
  <a href="https://decapodlabs.github.io/accountable-agentic-execution/"><img alt="Research" src="https://img.shields.io/badge/Research-Accountable%20Agentic%20Execution-blueviolet"></a>
</p>


Decapod keeps an agent's work tied to the request and the repository's required
checks. The agent records its understanding, makes the change, runs validation,
and repairs failures it can resolve. Missing proof and decisions that need a
human remain visible blockers.

What was asked, what was understood, what changed, and what was proven stays
with the project. Another agent or reviewer can pick up those records after
the original conversation ends.

## Quick start

Run this in your project, then open your coding agent and give it a task.

```bash
cargo binstall decapod && decapod init
```

## Capabilities

1. **Review what the agent understood.** Agent-authored specifications make the request, scope, and expected behavior available for review.
2. **Agents get relevant context.** Scoped context points to code and documentation with their sources.
3. **Concurrent tasks stay separate.** Exclusive task claims establish ownership; isolated workspaces separate changes.
4. **Work continues across sessions.** Recorded intent, decisions, changes, and evidence support handoff to another agent.
5. **Project rules govern agent work.** Explicit boundaries and project overrides govern the work and expose conflicts that need a decision.
6. **Review the proof.** Decapod requires deterministic verification for completion; Jev can add advisory input.

## How it works

Decapod is a repo-native governance kernel. The agent interprets the request,
writes the code and specifications, and gathers evidence. Decapod manages
coordination, checks invariants, and blocks governed publication when required
conditions remain unsatisfied.

Decapod runs on demand, without a daemon. Agents call it before inference,
while coordinating work, at validation boundaries, and before publication.
Durable state lives in the repository:

- `.decapod/governance.json` records the plan, trajectory, claims, and validation
- `.decapod/managed/specs/` holds the agent's reviewable understanding of the project
- `.decapod/workspaces/` separates task execution from the main checkout
- `.decapod/config.toml` describes the project and its configuration
- `.decapod/OVERRIDE.md` holds project-specific instructions

Decapod runs locally by default. You keep your coding agent, review process,
and project tracker. Its checks cover the configured requirements; they cannot
establish correctness beyond that scope or control commands an agent runs
outside Decapod.

[Governed execution architecture](docs/architecture/governed-execution.md) ·
[Configuration reference](docs/book/src/reference/config-toml.md)

## The Constitution

Decapod includes engineering guidance for architecture, security,
performance, research, and testing. Agents use it to shape their approach before
implementation. Project overrides make local requirements explicit.

## Documentation

- [Human docs](https://decapodlabs.github.io/decapod/)
- [Agent API](docs/agent/api-index.md)
- [Agent contract](AGENTS.md)
- [Paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf)
- [Research](https://decapodlabs.github.io/accountable-agentic-execution/)

## Contributing

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

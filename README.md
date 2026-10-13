<p align="center">🦀</p>

<p align="center">
  <strong>Decapod</strong><br />
  AI coding work you can verify.
</p>

<p align="center">
  Work in Cursor, Claude Code, Codex, Antigravity, Grok, or your preferred coding agent.
  Decapod governs the work while you keep using the same tools.
  Decapod can use <a href="https://docs.typesafe.ai/introduction">Jev</a> for System 1 input.
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <a href="assets/constitution.json#L5"><img alt="Constitution: core/DECAPOD" src="https://img.shields.io/badge/Constitution-core%2FDECAPOD-0A66C2"></a>
  <a href="https://decapodlabs.github.io/accountable-agentic-execution/"><img alt="Research" src="https://img.shields.io/badge/Research-Accountable%20Agentic%20Execution-blueviolet"></a>
</p>


Tell your agent what you want to build. Decapod helps it turn that request into
a plan and keeps the work tied to your requirements as the code changes.
The agent handles the engineering, checks its work, and brings unresolved
decisions back to you.

Run agents in parallel on separate tasks, or switch agents and tools during the
same pull request. Plans and decisions stay in the repository, so the next agent
can continue from recorded work without relying on the previous chat.

## Quick start

Run this in your project, then open your coding agent and tell it what you want to build.

```bash
cargo binstall decapod && decapod init
```

## Capabilities

1. **Catch misunderstandings early.** Project specs record the intended behavior. A plan gives each change a clear scope before the agent starts implementing it.
2. **Give agents useful context.** Agents retrieve relevant project guidance with source references, including the repository's own requirements.
3. **Run agents in parallel.** Task claims make ownership explicit. Overlap checks and isolated workspaces help keep concurrent changes separate.
4. **Continue the same PR in another tool.** The next agent can use the recorded plan, decisions and work history to pick up where work stopped.
5. **Apply your project’s rules.** Agents consult engineering guidance as they plan and build. Conflicts that need your decision stay visible.
6. **Know what has been checked.** Built-in validation ties results to the current change and the project's required checks. Missing required proof keeps the work unverified; Jev can add advisory input.

## How it works

Decapod is a Rust CLI that agents use throughout development. It reads the
project's configuration and recorded work, checks the relevant requirements, and
returns context or a next step. The agent writes the code and operates the
project's tools, including configuring builds and CI when needed.

Each call runs and exits. There is no background service to keep alive. The
repository keeps the working record:

- `.decapod/governance.json` holds the plan, work history, claims and validation results
- `.decapod/managed/specs/` describes the project's purpose, architecture and behavior
- `.decapod/workspaces/` gives tasks separate checkouts
- `.decapod/config.toml` holds project and validation settings
- `.decapod/OVERRIDE.md` records project-specific engineering instructions

The validation pipeline runs locally by default and can use remote Buildkite
preflight when configured. You keep your coding tools, tracker and review
process. Validation covers the configured requirements; Decapod cannot govern commands an agent runs
outside its workflow.

[Governed execution architecture](docs/architecture/governed-execution.md) ·
[Configuration reference](docs/book/src/reference/config-toml.md)

## The Constitution

Decapod includes engineering guidance that agents consult before and during
implementation, covering areas such as architecture, security and testing.
Project overrides add your local requirements.

## Documentation

- [Human docs](https://decapodlabs.github.io/decapod/)
- [Agent API](docs/agent/api-index.md)
- [Agent contract](AGENTS.md)
- [Paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf)
- [Research](https://decapodlabs.github.io/accountable-agentic-execution/)

## Contributing

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

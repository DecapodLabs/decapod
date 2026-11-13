<p align="center">🦀</p>

<h1 align="center">Decapod</h1>

<p align="center">
  <strong>Metacognition for AI agents.</strong><br />
  A repo-native governance kernel for bounded, convergent, proof-backed work.
</p>

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

Decapod gives coding agents a persistent way to check their work against the project's intent. Requirements, decisions, task ownership, and validation evidence stay with the repository across agents and sessions.

An agent plans and implements a change. Decapod keeps track of what that change must satisfy and the evidence required to call it complete. Failed checks and unmet requirements remain visible for the next iteration or handoff.

That is the metacognitive loop: establish intent, do the work, check the result, and resolve what remains. The project retains the basis for judging its progress as the work changes hands.

## Get started

Run this inside your project:

```bash
cargo binstall decapod && decapod init
```

Open your preferred coding agent and describe the change you want. Decapod supplies the project guidance and workflow the agent uses to plan, implement, and validate it.

You run `decapod init` once per repository. Your agent handles Decapod after
that. For cloud projects, it shares the browser sign-in link when needed,
resumes the connection, and renews the privately stored session across later
invocations. You keep chatting with your agent as usual.

Your agent carries the request through Decapod: it records intent and scope,
claims the task, works in the governed workspace, runs Decapod validation,
repairs failed checks, and returns an inspectable proof artifact. The artifact
identifies the source and required checks that actually ran. You describe the
engineering outcome; the agent operates these steps through Decapod.

## What Decapod governs

| Concern | What stays with the project |
| --- | --- |
| Intent | Specs and scoped plans describing purpose, expected behavior, and boundaries |
| Decisions | Engineering decisions, their context, and relevant source references |
| Ownership | Task claims, overlap checks, and isolated workspaces for parallel work |
| Rules | A shared Constitution and project-specific overrides |
| Completion | Configured checks, validation results, and the evidence required for acceptance |
| Continuity | Plans, work history, and unresolved validation state for the next agent |

**Bounded** means the work has explicit scope, constraints, and ownership.

**Convergent** means progress is checked against recorded intent, with unmet requirements carried forward until resolved.

**Proof-backed** means completion is supported by recorded evidence from the project's validation requirements.

## How it works

### 1. Establish intent

Project specs describe the system's purpose, architecture, and expected behavior. The agent creates a scoped plan before implementation, making the intended change and its boundaries explicit.

### 2. Execute with context

The agent retrieves relevant guidance with source references, follows the Constitution and project overrides, and records decisions as it works.

Parallel agents can claim separate tasks, check for overlap, and use isolated workspaces. Ownership stays explicit as work moves between contributors.

### 3. Validate the change

Decapod runs the configured validation workflow and associates the results with the current change. Failed checks and missing required evidence keep completion unresolved.

The agent uses those results to address outstanding requirements and validate again. What has been verified remains available for inspection.

### 4. Continue or hand off

Another agent can continue the same work using the recorded plan, decisions, task ownership, and validation state. Unfinished obligations survive the session that created them.

Your existing project tools, tracker, CI, and pull request process remain part of the workflow.

## Architecture

Decapod is a **Rust CLI**. Each invocation runs, returns its result, and exits. It is local-first and daemonless.

State lives with the project:

```text
.decapod/
├── governance.json       # Plans, work history, claims, validation results
├── managed/specs/        # Purpose, architecture, expected behavior
├── workspaces/           # Isolated task checkouts
├── config.toml           # Project and validation settings
└── OVERRIDE.md           # Project-specific engineering guidance
```

Validation runs locally by default, with optional remote Buildkite preflight. The configured checks and recorded evidence define what Decapod can recognize as verified completion.

[Governed execution architecture](docs/architecture/governed-execution.md) · [Configuration reference](docs/book/src/reference/config-toml.md)

## The Constitution

Decapod includes engineering guidance for architecture, security, testing, and other development decisions. Agents consult it during planning and implementation.

`.decapod/OVERRIDE.md` adds project-specific instructions. The repository carries the shared rules and local requirements together, giving each agent the same starting point.

Conflicts that require a decision remain visible so they can be resolved explicitly and preserved for subsequent work.

## Documentation and research

[Documentation](https://decapodlabs.github.io/decapod/) · [Agent API](docs/agent/api-index.md) · [Agent contract](AGENTS.md)

[Research paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf) · [Research site](https://decapodlabs.github.io/accountable-agentic-execution/)

## Contributing

Bug reports, reproducible failure cases, documentation fixes, and pull requests are welcome.

See the contributing guide for the development workflow and the security policy for reporting vulnerabilities.

[Contributing guide](CONTRIBUTING.md) · [Security](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

## License

[MIT](LICENSE)

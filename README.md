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
  <a href="assets/constitution.json#L5"><img alt="Constitution" src="https://img.shields.io/badge/Constitution-core%2FDECAPOD-0A66C2"></a>
  <a href="https://decapodlabs.github.io/accountable-agentic-execution/"><img alt="Research" src="https://img.shields.io/badge/Research-Accountable%20Agentic%20Execution-blueviolet"></a>
</p>


Decapod keeps an agent's work tied to the request and the repository's required
checks. The agent records its understanding, makes the change, and repairs
failures it can resolve. Missing proof and decisions that need a human remain
visible blockers.

What was asked, what was understood, what changed, and what was proven stays
with the project. Another agent or reviewer can pick up those records after
the original conversation ends.

```text
intent → work → validate → repair when needed → revalidate → ready for review
```

## Quick start

In your Git repository:

```bash
cargo binstall decapod
decapod init
```

The install command uses cargo-binstall. With standard Cargo, use
`cargo install decapod` instead. Initialization prepares the project and writes
the agent instructions.

Have your coding agent follow the generated `AGENTS.md`, then give it a small
task, for example:

> Fix the bug described in this issue and add a regression test. Show me the
> change and its validation result.

Your agent handles task claims and workspace setup, then runs this from its
isolated task workspace:

```bash
decapod validate
```

Review the diff, the agent's recorded interpretation, and the validation
result. Required project tools and any configured CI access must be available;
validation reports missing prerequisites as blockers.

[Setup and workflow details](docs/book/src/quickstart.md)

## A real validation result

This historical run checked Decapod commit
[`873ab797`](https://github.com/DecapodLabs/decapod/commit/873ab7979e67d6b80d1e5d9b622fd110035acede)
against `master` at `f2073bd4` on October 12, 2026. `decapod validate` exited 0.
Selected fields from its JSON output:

```json
{
  "publication_ready": true,
  "readiness": {
    "blockers": [],
    "ready": true,
    "scope": "source_pr_readiness"
  },
  "scope": "publication_readiness",
  "status": "pass"
}
```

The evidence came from Buildkite runs for
[CI #1024](https://buildkite.com/decapodlabs/decapod/builds/1024),
[constitution checks #1025](https://buildkite.com/decapodlabs/decapod/builds/1025),
[validation #1026](https://buildkite.com/decapodlabs/decapod/builds/1026),
[docs #1027](https://buildkite.com/decapodlabs/decapod/builds/1027), and
[docs sync #1028](https://buildkite.com/decapodlabs/decapod/builds/1028).

That result covers the configured source-PR checks for that candidate and target.
Release-only packaging was outside the profile; validation did not publish or
deploy anything. This is an example of the report, not a current-branch status
or a timing benchmark.

A failed or incomplete required check keeps readiness blocked. The agent follows the
report's recovery instructions and validates again after repairs. Relevant
changes to the candidate, target, configuration, or toolchain require fresh
validation. Passing checks still leaves the change for human review.

## Capabilities

1. **Review what the agent understood.** Agent-authored specifications make the request, scope, and expected behavior available for review.
2. **Give agents relevant context.** Scoped context points to code and documentation with their sources.
3. **Keep concurrent tasks separate.** Exclusive task claims establish ownership; isolated workspaces separate changes.
4. **Pick up work across sessions.** Recorded intent, decisions, changes, and evidence support handoff to another agent.
5. **Apply project rules.** Explicit boundaries and project overrides govern the work and expose conflicts that need a decision.
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
- `.decapod/OVERRIDE.md` holds project-specific instructions you can edit

Decapod runs locally by default. You keep your coding agent, review process,
and project tracker. Its checks cover the configured requirements; they cannot
establish correctness beyond that scope or control commands an agent runs
outside Decapod.

[Governed execution architecture](docs/architecture/governed-execution.md) ·
[Configuration reference](docs/book/src/reference/config-toml.md)

## The Constitution

Decapod includes queryable engineering guidance for architecture, security,
performance, research, and testing. Agents use it to shape their approach before
implementation. Project overrides make local requirements explicit.

The canonical router is `core/DECAPOD` in
[`assets/constitution.json`](assets/constitution.json):

```bash
decapod constitution get core/DECAPOD
```

## Documentation

- [Human docs](https://decapodlabs.github.io/decapod/)
- [Agent API](docs/agent/api-index.md)
- [Agent contract](AGENTS.md)
- [Paper](https://github.com/DecapodLabs/accountable-agentic-execution/blob/main/paper/Accountable_Agentic_Execution.pdf)
- [Research](https://decapodlabs.github.io/accountable-agentic-execution/)

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build && cargo test
```

[CONTRIBUTING](CONTRIBUTING.md) · [SECURITY](SECURITY.md) · [Issues](https://github.com/DecapodLabs/decapod/issues)

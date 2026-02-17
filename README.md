<p align="center">🦀</p>

<p align="center">
  <code>cargo install decapod && decapod init</code>
</p>

<p align="center">
  <strong>Decapod</strong><br />
  Governance Runtime for AI Coding Agents
</p>

<p align="center">
  <a href="https://github.com/DecapodLabs/decapod/actions"><img alt="CI" src="https://github.com/DecapodLabs/decapod/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://crates.io/crates/decapod"><img alt="crates.io" src="https://img.shields.io/crates/v/decapod.svg"></a>
  <a href="https://github.com/DecapodLabs/decapod/blob/master/LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
</p>

---

> Local-first. Repo-native. Built to turn agent output from "probably fine" into verifiable delivery.

## What Is Decapod

Decapod is an execution contract for coding agents.
It standardizes how agents operate in a repository, enforces workflow boundaries, and requires evidence before completion is accepted.

Not a hosted control plane. Not locked to one model vendor. It runs in your repo, on your infrastructure.

## What It Replaces

- Narrative completion claims with explicit pass/fail gates.
- Ad-hoc agent behavior with consistent operational contracts.
- Fragile one-agent flows with repeatable multi-agent coordination patterns.
- "Trust the summary" with auditable delivery signals.

## Assurance Model

Decapod is built around three outcomes for agent execution:

- `Advisory`: route the agent toward the highest-leverage next action.
- `Interlock`: block unsafe, out-of-policy, or out-of-sequence execution.
- `Attestation`: produce structured evidence that completion criteria were actually met.

This is how intent survives contact with automation: guide, constrain, verify.

## Why Teams Use It

- Agents follow one contract instead of inventing workflow per session.
- Quality gates are part of execution, not an afterthought.
- Repo policy stays enforceable as autonomy increases.
- Teams can scale parallel agent work without losing control.

## Capability Highlights

- Guided project understanding via structured prompts.
- Standards-aware execution aligned to project expectations.
- Workspace safety for isolated implementation flow.
- Validation/completion gates with explicit outcomes.
- Machine-readable RPC interface for orchestration and tooling integration.

## Quick Start

```bash
# Install
cargo install decapod

# Initialize in your repository
decapod init

# Inspect available commands
decapod --help

# Inspect machine-readable capability surface
decapod capabilities --json
```

## Typical Agent Flow

```bash
# Confirm workspace status
decapod workspace status

# Ensure isolated workspace when needed
decapod workspace ensure

# Validate before completion
decapod validate
```

## Contributing

```bash
git clone https://github.com/DecapodLabs/decapod
cd decapod
cargo build
cargo test
decapod validate
```

See also:

- Development guide: `CONTRIBUTING.md`
- Security policy: `SECURITY.md`
- Release history: `CHANGELOG.md`

## Support

- [File an issue](https://github.com/DecapodLabs/decapod/issues)
- [Support on Ko-fi](https://ko-fi.com/decapodlabs)

## License

MIT. See [LICENSE](LICENSE).

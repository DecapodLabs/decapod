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

> Local-first. Repo-native. Built to make autonomous coding work auditable and enforceable.

## What Is Decapod

Decapod gives coding agents a consistent way to operate inside a repository.
It provides a stable CLI/RPC interface, workflow guardrails, and validation gates so work is verifiable before completion.

Decapod is not a hosted service and not tied to a single model provider. It runs locally in your repo.

## Assurance Model

Decapod is built around three outcomes for agent execution:

- `Advisory`: practical guidance toward the next correct move.
- `Interlock`: explicit constraints that block unsafe or non-compliant flow.
- `Attestation`: structured evidence that work actually satisfies completion criteria.

You can think of it as a runtime contract for moving from intent to implementation with fewer silent failure modes.

## Why Teams Use It

- Standardizes how agents interact with a codebase.
- Keeps execution local and auditable.
- Reduces "done" claims without evidence.
- Supports parallel work with safer branch/workspace patterns.

## Capability Highlights

- Guided project understanding through interview-style prompting.
- Standards-aware execution that respects project conventions and policy.
- Workspace safety checks for parallel agent operation.
- Validation and completion gates that produce auditable outcomes.
- Machine-readable RPC surface for multi-agent orchestration.

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

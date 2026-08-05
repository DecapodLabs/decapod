# Agent Orientation Corpus

This directory contains the machine-facing orientation layer for Decapod, a
repo-native governance kernel for work performed by AI coding agents. Decapod
governs accepted work; the agent interprets and performs it.

The architecture boundary is:

```text
Models produce intelligence.
Agents perform work.
Repositories preserve state.
Decapod governs the transition from intent to proof.
```

Reliability is designed, not hoped for. Keep that boundary explicit when
choosing commands, recording state, interpreting validation, or claiming
publication.

- [api-index.md](api-index.md): The primary entrypoint.
- [command-contracts.md](command-contracts.md): Operational contracts for CLI commands.
- [payload-examples.md](payload-examples.md): Valid call shapes.
- [error-recovery.md](error-recovery.md): Handling failures.
- [state-model.md](state-model.md): Conceptual state entities.
- [config-schema.md](config-schema.md): Configuration policy keys.
- [contribution-conventions.md](contribution-conventions.md): Project tooling standard, source layout, and per-PR governance artifact rules.
- [llms.txt](llms.txt): Full documentation index.

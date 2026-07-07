# Artifacts

Decapod generates and manages various artifacts to maintain the chain of custody.

These artifacts are the inspectable surface of `.decapod/` as the repo-native substrate for governed agent work. They preserve the state that should survive beyond a chat transcript: intent, architecture assumptions, selected context, validation output, and proof provenance.

## `.decapod/generated/`

All generated artifacts live under this directory.

### `specs/`
Living documentation of the project's intent and design.
- `INTENT.md`: What the project is trying to achieve.
- `ARCHITECTURE.md`: High-level design and diagrams.
- `INTERFACES.md`: Defined APIs and boundaries.

### `artifacts/`
Evidence and provenance records.
- `provenance/`: Manifests and checklists for promotion.
- `custody/`: Detailed evidence logs and contradiction records.
- `diagnostics/`: Optional logs for troubleshooting.

### `context/`
Deterministic context capsules used by agents to orient themselves.

## `AGENTS.md`, `CLAUDE.md`, etc.
Root-level entrypoints for AI agents. These files point agents to the Decapod kernel and provide their starting instructions.

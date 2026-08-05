# Artifacts

Decapod governs authored state and produces generated evidence and projections. Those classes are not interchangeable.

These artifacts are the inspectable surface of `.decapod/` as the repo-native substrate for governed agent work. They preserve the state that should survive beyond a chat transcript: intent, architecture assumptions, selected context, validation output, and proof provenance.

## Authored and managed state

The files under `.decapod/managed/` include agent-authored project contracts and Decapod-managed state. They are not all generated.

### `specs/`
Living documentation of the project's intent and design. The acting agent authors and maintains this prose; Decapod requires and validates it.
- `INTENT.md`: What the project is trying to achieve.
- `ARCHITECTURE.md`: High-level design and diagrams.
- `INTERFACES.md`: Defined APIs and boundaries.

## Generated evidence and projections

Decapod generates supported context capsules, attestations, manifests, receipts, and proof artifacts from governed inputs. A generated projection is a consumable view, not an independent authority that can replace its source.

### Evidence and provenance records
- `provenance/`: Manifests and checklists for promotion.
- `custody/`: Detailed evidence logs and contradiction records.
- `diagnostics/`: Optional logs for troubleshooting.

### Context capsules
Deterministic, generated projections used by agents to orient themselves. Their provenance identifies the authority from which they were resolved.

## `AGENTS.md`, `CLAUDE.md`, etc.
Root-level entrypoints for AI agents. These files point agents to the Decapod kernel and provide their starting instructions.

See [Governance Artifact Inventory](governance-artifacts.md) for state ownership and lifecycle details.

# Decapod Project Specifications

Decapod is a repo-native governance kernel for AI coding work. These documents
describe the product: how human intent becomes reviewable agent understanding,
how work stays within project boundaries, and what evidence is required before
governed completion and publication.

The agent authors these specifications. Decapod checks their required presence,
material updates, bindings, and generated projections. Their prose remains an
interpretation that people and agents must review against the implementation.

## Contract map

- [INTENT.md](INTENT.md) defines the user outcome, responsibilities, scope, and
  acceptance expectations
- [ARCHITECTURE.md](ARCHITECTURE.md) describes the runtime, subsystems, state
  ownership, and integration boundaries
- [INTERFACES.md](INTERFACES.md) defines the CLI, structured RPC, configuration,
  and artifact contracts
- [SEMANTICS.md](SEMANTICS.md) defines authority, state transitions, evidence,
  replay, and retry behavior
- [VALIDATION.md](VALIDATION.md) defines proof obligations and the distinction
  between checked work, publication readiness, and external delivery
- [OPERATIONS.md](OPERATIONS.md) describes the agent-operated lifecycle,
  diagnostics, recovery, and maintenance
- [SECURITY.md](SECURITY.md) defines trust boundaries, credentials, authorization,
  data handling, and limits of enforcement

## Ownership

The canonical directory is `.decapod/managed/specs/`. Authored Markdown outside
explicit generated blocks belongs to the project contract. Agents edit that
prose in a claimed, isolated workspace as understanding or behavior changes.

Decapod owns generated attestations, declared-capability blocks, capability
overlays, and `.manifest.json`. Supported refresh operations update those
projections. A refresh does not write the project's meaning or substitute for
a material authored spec change. It must preserve authored content and stop
before writing if that preservation cannot be established.

`.decapod/config.toml` and `.decapod/OVERRIDE.md` supply project configuration
and local authority. `.decapod/governance.json` records the active change's
plan, claims, trajectory, checkpoints, and validation. Runtime coordination
lives in the canonical datastore. Git retains accepted specification and
governance history.

## Per-Change Maintenance Loop

Update the owning document when a change affects the product outcome, public
behavior, state, trust boundary, proof obligation, or recovery path. Keep
cross-document terms consistent. Record current behavior and explicit limits;
use the changelog and Git history for release narratives and past measurements.

Before publication, the agent reviews the authored changes with the source,
refreshes supported projections through `decapod rpc --op specs.refresh` or
`decapod validate --refresh-specs`, and satisfies the applicable validation
contract. Fingerprint-only changes do not meet the living-spec requirement.

Command syntax and operational preconditions come from the installed binary's
agent documentation. The source architecture is described in
[Governed Execution](../../../docs/architecture/governed-execution.md); the
[Agent API](../../../docs/agent/api-index.md) defines how agents use it.

<!-- decapod:codebase-attestation:start -->

## Codebase Attestation

- Repository signal fingerprint: `bbd7f745eff75ef6e5b01b06b1ccc5edae2a43c5d9e65ead511980fd969d4c6a`
- Significant implementation surfaces: `.config/` (1 files), `.github/` (11 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `assets/` (40 files), `docs/` (1 files), `src/` (210 files), `tests/` (194 files)
- Refreshed from the current codebase by `decapod rpc --op specs.refresh`
<!-- decapod:codebase-attestation:end -->

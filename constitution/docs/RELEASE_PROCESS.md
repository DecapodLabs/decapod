# Release Process

## Release Checklist (Enforced)

Run:

```bash
decapod release check
decapod release inventory
```

Release readiness requires:

- `CHANGELOG.md` with `## [Unreleased]` section.
- `constitution/docs/MIGRATIONS.md` present and current.
- `Cargo.lock` present for locked builds.
- RPC golden vectors present (`tests/golden/rpc/v1`).
- Provenance manifests present in `artifacts/provenance/`.
- Intent-convergence checklist present and valid (`artifacts/provenance/intent_convergence_checklist.json`).
- Every provenance manifest carries `policy_lineage` with a valid capsule reference and hash.
- `decapod release check` auto-stamps/normalizes `policy_lineage` across all three manifests before validation.
- If schema/interface surfaces changed in the working tree, `CHANGELOG.md` `## [Unreleased]` MUST include a schema/interface note.

Risk-tier override for stamping:

- `DECAPOD_RELEASE_RISK_TIER=low|medium|high|critical` (default: `medium`)

`decapod release inventory` writes deterministic CI inventory output to:

- `artifacts/inventory/repo_inventory.json`

## Versioning Rules

- Schema changes require a version bump.
- Breaking CLI/RPC changes require a major bump.
- Golden vector breaking updates require major bump.

## Changelog Discipline

Every release PR MUST include:

- intent summary
- invariants affected
- proof gates added/updated

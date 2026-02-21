# Release Process

## Release Checklist (Enforced)

Run:

```bash
decapod release check
```

Release readiness requires:

- `CHANGELOG.md` with `## [Unreleased]` section.
- `MIGRATIONS.md` present and current.
- `Cargo.lock` present for locked builds.
- RPC golden vectors present (`tests/golden/rpc/v1`).
- Provenance manifests present in `artifacts/provenance/`.

## Versioning Rules

- Schema changes require a version bump.
- Breaking CLI/RPC changes require a major bump.
- Golden vector breaking updates require major bump.

## Changelog Discipline

Every release PR MUST include:

- intent summary
- invariants affected
- proof gates added/updated

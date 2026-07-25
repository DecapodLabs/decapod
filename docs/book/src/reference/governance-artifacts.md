# Governance Artifact Inventory

Every publication-ready Decapod change carries four repository-native
artifacts:

- `.decapod/governance/plan.json`: refined intent and phase state.
- `.decapod/governance/claims.json`: the research claims ledger.
- `.decapod/governance/trajectory.json`: run scope and proof evidence.
- `.decapod/governance/validation.json`: the successful validation receipt.

The research claims ledger is distinct from Health Engine claims in
`.decapod/data/health.db`. Health Engine claims record operational health and
proof events; `claims.json` records falsifiable, repository-owned research
claims and is part of the PR proof surface.

## Inventory and repair

Run the agent-facing inventory before staging or publishing:

```bash
decapod govern artifacts inventory --base-branch master
```

For an initialized repository that predates the claims-ledger template, use:

```bash
decapod govern artifacts inventory --repair
```

Repair creates `.decapod/governance/claims.json` only when it is absent. It
never overwrites project-specific claim content. The inventory reports each
artifact's presence, schema validity, staged state, and inclusion in the
actual PR diff. Publication remains strict when any required artifact is
missing, invalid, or absent from that diff.

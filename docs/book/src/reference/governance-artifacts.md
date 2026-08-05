# Governance Artifact Inventory

Publication-ready Decapod changes carry four repository-native governance
artifacts:

| Artifact | Updated by | Classification | Purpose |
| --- | --- | --- | --- |
| `.decapod/governance/plan.json` | Agent through `decapod govern plan`; approved through the governed surface | Governed execution state | Records refined intent, scope, phases, decisions, and proof hooks as the plan changes. |
| `.decapod/governance/claims.json` | Agent or researcher through the sanctioned claims surface | Authoritative falsifiable-claims ledger | Connects a claim to its baseline, observable condition, failure mode, measurement, and proof gate. It is not proof by itself. |
| `.decapod/governance/trajectory.json` | Agent through `decapod govern trajectory` | Evidentiary custody record | Records run intent, boundaries, inspected and modified files, commands, assumptions, checks, and evidence over time. |
| `.decapod/governance/validation.json` | `decapod validate` | Generated validation receipt | Records the validation result for identified repository state and supports the publication gate. |

These artifacts work with todos, assignments, living specifications, evidence,
receipts, projections, custody, and publication state. See the
[governed execution model](../../../architecture/governed-execution.md) for the
full ownership and lifecycle map.

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

An external tracker such as GitHub Issues, Jira, Linear, or Beads may remain the
organizational system of record. Decapod's todo and claim state governs the
accepted work at the repository execution layer.

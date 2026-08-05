# Governed Execution Model

Decapod occupies the transition layer between agent capability and trusted delivery:

```text
Models produce intelligence.
Agents perform work.
Repositories preserve state.
Decapod governs the transition from intent to proof.
```

Reliability is designed, not hoped for. Decapod is a repo-native governance
kernel for work performed by AI coding agents. It does not perform the work,
replace the agent harness, or author the repository's meaning. It governs
accepted work until the applicable boundaries, validation, evidence, and
publication requirements converge.

## Responsibility boundaries

| Participant | Responsibility |
| --- | --- |
| Human | Expresses intent, provides judgment, and approves meaningful outcomes. |
| Agent | Interprets the request and repository, decomposes and performs the work, authors living specifications, updates governed state through Decapod, follows validation feedback, and gathers evidence. |
| Decapod | Makes accepted intent and boundaries explicit, maintains governance state, validates invariants, refreshes supported projections, exposes contradictions, and blocks publication while required conditions are unsatisfied. |
| Repository | Preserves governed state, custody, history, specifications, evidence, and receipts across processes, models, harnesses, and Decapod invocations. |

## Daemonless lifecycle

Each Decapod CLI or RPC invocation is ephemeral. One agent task may span many invocations. The repository is the durable execution surface, so a later process can resume or review the work without relying on the original model context or harness session.

```text
intent → interpretation → bounded execution → validation
       → remediation when required → revalidation
       → publication → proof-backed completion
```

Validation is an iterative control boundary. Failure means at least one required condition remains unsatisfied. When the result exposes a supported remediation, the agent applies it and revalidates. Decision gates, contradictions, unavailable proof, and unsupported remediation remain visible blockers rather than implied success.

Publication is a governed state transition. An agent's statement that the work is complete does not establish that the transition occurred.

## Durable governance artifacts

| Primitive | Author or updater | Classification | Contribution to convergence |
| --- | --- | --- | --- |
| Intent | Human expresses it; the agent records its accepted interpretation | Authoritative input and governed interpretation | Anchors the requested outcome, constraints, and completion standard as understanding changes. |
| Plan | Agent updates it through Decapod; meaningful outcomes may require human approval | Governed execution state | Makes scope, phases, decisions, and proof hooks explicit as the work develops. |
| Todo or assignment | Agent and Decapod update claim and lifecycle state | Authoritative coordination state for accepted execution | Establishes task scope and exclusive custody without replacing an external organizational tracker. |
| Claim | Agent or researcher records a falsifiable claim through the governed surface | Authoritative claim ledger; not proof by itself | Connects a stated condition to a failure mode, measurement, and proof gate as evidence changes. |
| Trajectory | Agent records run scope, inspected and modified files, commands, assumptions, and checks through Decapod | Evidentiary custody record | Preserves how the work moved from intent to evidence across runs. |
| Living specification | Acting agent authors and maintains it | Authoritative record of the agent's explicit repository interpretation | Makes understanding reviewable. Incorrect or stale content exposes incomplete governed work before publication. |
| Validation result | Decapod evaluates repository state and records the result | Evidentiary receipt | Identifies satisfied and violated invariants and, where supported, the next remediation step. |
| Evidence | Agent gathers it; Decapod binds required references to governed state | Evidentiary | Supports or falsifies completion claims against identified repository state. |
| Receipt | Decapod emits it for supported operations and transitions | Generated evidence | Correlates an invocation, its inputs or state, and its outcome over time. |
| Projection | Decapod refreshes it from supported authoritative inputs | Generated, non-authoritative view | Makes selected state consumable without becoming a second source of truth. |
| Custody | Decapod updates sessions, claims, workspaces, and recorded history | Authoritative coordination state plus evidence | Identifies who accepted the work and where it may proceed without collision. |
| Publication state | Decapod updates it through governed completion and workspace publication paths | Authoritative transition state | Prevents incomplete or contradictory work from being represented as published completion. |

GitHub Issues, Jira, Linear, Beads, or another service may remain the organizational system of record. Decapod governs the accepted task at the repository execution layer.

## Living specifications

Living specifications are authored interpretations, not semantic content generated independently by Decapod. Refresh operations update supported fingerprints, attestations, overlays, manifests, or projections; they do not replace the agent's responsibility to maintain authored prose.

If a living specification is wrong, Decapod has made the misunderstanding visible before publication. That is a successful governance result. Reviewers can correct visible repository state; they cannot review an assumption that existed only inside transient model context.

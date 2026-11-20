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

Publication is a governed state transition. An agent's statement that the work is complete does not establish that the transition occurred. The judgment rules, canonical order, freshness, and recovery behavior are specified in [Governed transitions](governed-transitions.md).

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

Workspace promotion uses a normal fast-forward push. Decapod does not force-push
managed branches. A non-fast-forward rejection remains a publication blocker;
the agent must inspect the remote divergence, reconcile it in the workspace,
rerun validation, and retry. Decapod can return that instruction, but it cannot
control Git commands issued directly by an agent or its harness.

GitHub Issues, Jira, Linear, Beads, or another service may remain the organizational system of record. Decapod governs the accepted task at the repository execution layer.

## Accepted repository-local proof

Accepted local proof requirements constrain candidate configuration through
`repository_profile::preserve_accepted`. A candidate may add required checks;
it cannot replace accepted invocations or execution/coverage declarations by
calculating another review digest. An unsupported replacement remains blocked
until policy is independently accepted on the target. This uses the accepted Git
state as the existing policy authority and introduces no separate policy store.
Repository-local readiness makes only its declared local assurance claim; strict
source-PR readiness retains its accepted hosted execution requirements.

## Living specifications

Living specifications are authored interpretations, not semantic content generated independently by Decapod. Refresh operations update supported fingerprints, attestations, overlays, manifests, or projections; they do not replace the agent's responsibility to maintain authored prose.

If a living specification is wrong, Decapod has made the misunderstanding visible before publication. That is a successful governance result. Reviewers can correct visible repository state; they cannot review an assumption that existed only inside transient model context.

## Bounded broker request windows

The daemonless Unix group broker shares one absolute request deadline between
queueing and execution. `DECAPOD_GROUP_BROKER_REQUEST_BUDGET_MS` selects a workload
budget from 1 through 300000 milliseconds; the default remains 120000. Invalid
client values and native peer deadlines beyond the hard ceiling are rejected
before durable mutation admission. The foreground election stops accepting new
work when its originating request window expires, capped at 300 seconds.
In-flight commands retain their request deadline and existing command limits.

Deadline exhaustion or a lost acknowledgement preserves an unproven outcome.
The existing receipt and same-ID retrieval rules continue to prevent re-execution
of an unknown request. A workload budget grants time; custody, evidence and
readiness requirements remain independently enforced.

The twenty-client contention fixture explicitly selects 300000 milliseconds to
match its existing 300-second outer bound. Both storage modes still require all
twenty mutations to commit exactly once. The steady-state validation probe keeps
its separate 10-second per-call and 20-second aggregate limits.

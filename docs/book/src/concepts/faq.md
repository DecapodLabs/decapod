# Common Questions

## Why does Decapod not write my specifications?

The agent interprets the request and repository, so the agent authors the semantic claims. Decapod requires, validates, and may refresh supported projections of those claims.

## Why are living specifications required?

They move the agent's interpretation out of transient model context and into a durable, reviewable repository artifact.

## Why can specifications be wrong?

They record an agent interpretation, not an independent truth invented by Decapod. Validation makes a misunderstanding visible before publication so it can be corrected.

## Why does validation block publication?

Publication is a governed state transition. It remains blocked while required invariants or evidence are unsatisfied.

## Why are governance artifacts committed?

Plans, claims, trajectories, validation receipts, and related evidence must remain available to reviewers, CI, and later agents after the original process ends.

## Why does durable state live in the repository?

The repository already provides shared custody and history. Keeping governed state with the work allows execution to continue across processes, models, harnesses, and Decapod invocations.

## Why is Decapod daemonless?

Agents invoke it when governance is needed. No background process is required because the repository, not process memory, is the durable execution surface.

## Why can one task span many invocations?

Each CLI or RPC call is ephemeral. The task continues through durable repository state until validation, publication, and proof requirements are satisfied.

## Why should an agent continue after a recoverable validation failure?

Failure means a required condition is still unsatisfied. The agent should follow the supported remediation, update the affected artifact, revalidate, and continue toward publication.

## How is Decapod different from an agent?

The agent interprets and performs the work. Decapod governs accepted work and validates the conditions around it.

## How is it different from an orchestrator?

An orchestrator schedules or coordinates execution. Decapod defines and enforces repository-native governance boundaries; it does not replace the harness that runs the agent.

## How is it different from a task tracker?

Trackers organize work. Decapod governs accepted work at the execution layer. GitHub Issues, Jira, Linear, or another tracker may remain the organizational system of record.

## What does convergence mean?

The agent preserves accepted intent, stays within explicit boundaries, maintains durable state, responds to validation, remediates supported failures, and produces evidence before completion.

## What does proof-backed completion establish?

It establishes that required checks ran against identified repository state, produced the required evidence, and permitted the governed completion or publication transition.

# Security Threat Model

## Threats We Explicitly Model

- Drift and unverifiable completion.
- Malicious or compromised agent edits.
- Dependency tampering/supply-chain substitution.
- Provenance forgery.
- Shadow state and bypass of the control plane.

## What Decapod Prevents

- Direct promote/publish flow without provenance manifests.
- Protected-branch implementation flow.
- Unclaimed-task worktree execution.
- Silent schema drift without validation pressure.

## What Decapod Does Not Prevent

- A fully privileged local user bypassing process policy.
- A compromised host kernel or filesystem.
- Social-process failures (approvals done without review).

## Security Posture

- Local-first and auditable.
- Deterministic envelope and reducer discipline.
- Proof-first promotion and explicit invariants.

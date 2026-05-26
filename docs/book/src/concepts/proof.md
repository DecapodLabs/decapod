# Proof

Decapod replaces the unreliable "agent says it's done" claim with deterministic, **Proof-Backed Completion**.

## Verification Gates

A "Gate" is a discrete check that must pass for a task to be considered valid. Common gates include:
- **Compliance Gates:** Checked by `decapod validate`.
- **Quality Gates:** Unit tests, linting, and type-checking.
- **Security Gates:** Secret scanning and dependency audits.
- **Human Gates:** Explicit approval for high-risk changes.

## The Evidence Ledger

When an agent calls `decapod todo done --validated`, Decapod captures a snapshot of the repository state and the results of all required gates. This evidence is recorded in the `.decapod/generated/artifacts/` ledger.

This creates **Epistemic Custody**: a verifiable chain of proof that shows *how* the agent verified the work and *what* the state of the world was at the moment of completion.

## Determinism

Decapod strives for deterministic proof. A proof is valid only if it can be re-run or re-verified by another agent or a human at a later date. This ensures that the repository's integrity is not dependent on a single agent's transient state.

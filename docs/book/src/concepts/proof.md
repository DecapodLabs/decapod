# Proof

Decapod distinguishes an agent's completion claim from **proof-backed completion**. A claim reports what the agent believes. Proof-backed completion establishes that the required gates ran against identified repository state, produced evidence, and permitted the governed publication transition.

## Verification Gates

A "Gate" is a discrete check that must pass for a task to be considered valid. Common gates include:
- **Compliance Gates:** Checked by `decapod validate` (see [CLI Reference](../reference/cli.md#core-operations)).
- **Quality Gates:** Unit tests, linting, and type-checking.
- **Security Gates:** Secret scanning and dependency audits.
- **Human Gates:** Explicit approval for high-risk changes.

## The Evidence Ledger

When an agent completes validated work, Decapod binds validation receipts and evidence references to governed repository state (see [Artifacts Reference](../reference/artifacts.md)). Agent testimony alone does not satisfy that boundary.


This creates **epistemic custody**: a reviewable chain from accepted intent and assumptions to the checks that ran and the repository state they measured.

## Failure and recovery

A failed gate means the proof requirement is unsatisfied; it does not mean the task is complete. If the result provides a supported recovery path, the agent should:

1. inspect the validation result;
2. identify the violated invariant;
3. perform the sanctioned remediation;
4. update the relevant artifact;
5. re-run validation; and
6. continue toward publication.

Decapod does not assume every failure is recoverable. Decision gates, contradictions, unsupported remediation, and unavailable proof remain blockers for human review.

## Determinism

Decapod strives for deterministic proof. A proof is valid only if it can be re-run or re-verified by another agent or a human at a later date. This ensures that the repository's integrity is not dependent on a single agent's transient state.

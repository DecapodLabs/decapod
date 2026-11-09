# Intent

In Decapod, **Intent** is the primary driver of the development lifecycle. It is the human-originated "Why" that must be preserved and formalized before implementation begins.

## The Spec-First Mandate

Agents often dive into implementation before fully understanding the human operator's goal. Decapod prevents this by mandating a spec-first approach:

1.  **Capture:** Vague requests are converted into explicit tasks via `decapod todo add` (see [CLI Reference](../reference/cli.md#task-tracking)).
2.  **Formalize:** Agents are required to update or verify the `specs/INTENT.md` document scaffolded in the repository (see [Artifacts Reference](../reference/artifacts.md)).
3.  **Validate:** The final implementation is checked against this intent. If the outcome deviates from the recorded intent, validation fails (see [Proof & Validation](proof.md)).


## Intent Pressure

"Intent Pressure" occurs when an agent encounters ambiguity. Decapod's philosophy is that **uncertainty must be preserved, not compressed**. If an agent is 70% sure of a requirement, it must not guess the remaining 30%. Instead, it should record the ambiguity in the task's metadata or `INTENT.md` and request human clarification.

## Versioned Specifications

Because Decapod is repo-native, your specifications are versioned alongside your code. This creates a permanent, auditable link between a feature's requirements and its implementation, which is invaluable for long-term maintenance and multi-agent handover.

## Continuous Intent Reconciliation

The human request is retained in the governed plan; agent-authored project specifications
express the current understanding of the product. Changes to source code cannot silently
authorize a change to that intent. The coding harness generates code and proposed spec
updates; Decapod reports observable mismatches and retains existing governance/proof gates.

Read the advisory report at any point:

```bash
decapod rpc --op intent.reconcile --params '{}'
```

This read-only reconciliation operation uses the existing repo fingerprint,
spec manifest, authored Markdown, and governed plan. It reports outdated manifest
snapshots, missing canonical specs, unresolved human questions, and stale bound
spec reviews. Hash agreement establishes input freshness, not semantic correctness.

For stable cross-document traceability, the agent may place an exact standalone
marker before a requirement in the authored portion of
`.decapod/managed/specs/INTENT.md`:

```markdown
<!-- decapod:requirement REQ-OFFLINE-EDIT -->
Offline editing remains available without connectivity.
```

Use that same marker in the authored sections of the relevant architecture,
interface, semantics, security, operations, or validation document. Only
`INTENT.md` defines an ID; another document merely references it.
IDs must be `REQ-` followed by uppercase letters, digits, hyphens or
underscores, at most 64 bytes total. Never put markers in generated blocks or
code fences. Missing, duplicate, invalid, orphaned, and unvalidated references
remain visible. A reference is an agent-authored traceability assertion, never
evidence that code satisfies the requirement or that a test passed.

## Immutable change impact

For a bounded, exact Git comparison, provide the **full immutable base commit ID**.
The current `HEAD` commit is the other end of the comparison. This view excludes
uncommitted working-tree changes; it never silently resolves a moving branch name.

```bash
decapod rpc --op intent.reconcile --params '{"base_commit":"<full-commit-oid>"}'
```

The additive `change_impact` section (report version 1.1.0 when requested)
lists changed paths and status, including renames, authored material-spec changes
versus generated-only projection updates, and REQ identifiers explicitly present
in changed authored specs. It separately exposes changed implementation paths
that have **no verified code-to-requirement mapping**. A marker appearing in a
spec cannot establish that any changed code implements it.

The inspection requires a real commit object that is an ancestor of `HEAD`.
Unsupported or oversized paths and incomplete observations are reported with
`complete: false`, without inferring unaffected files. Invalid or symbolic
base identifiers fail explicitly. `review_needed` and `evidence_needed` are
advisory; neither field satisfies spec review, validation or publication proof.
Empty RPC parameters retain the existing report shape and version.

This first reconciliation contract is advisory: it does not write specs or code,
approve changes, execute tests, or certify semantic alignment. Agents resolve
the findings in their isolated workspace and use the configured validation
and publication process for acceptance. A reported `not_proven` is
not a test failure and cannot be treated as `proven`.

# Constitution

The **Constitution** is the foundational set of rules and engineering standards that govern all behavior within a Decapod-managed repository.

## Authority Layers

Decapod's authority model is hierarchical:

1.  **Global Constitution:** Over 100 normative documents embedded in the Decapod binary. These cover universal engineering standards (e.g., "Always write tests", "Minimize breaking changes").
2.  **Project Overrides (`.decapod/OVERRIDE.md`):** Repo-local rules that extend or supersede the global constitution. This is where you define team-specific conventions.
3.  **Task Policy:** Temporary rules or constraints defined for a specific work unit.

## Selective Context

Agents do not ingest the entire constitution. Instead, Decapod provides a **Context Capsule** interface. Agents perform targeted queries to retrieve only the directives relevant to their current task.

```bash
decapod rpc --op constitution.get --params '{"section":"core/DECAPOD"}'
```

This high-signal, low-noise approach ensures that agents remain oriented without being overwhelmed by irrelevant documentation.

## Enforcement

The constitution is not merely "guidance"—it is enforced. `decapod validate` checks the repository state against the normative claims made in the constitution. If a change violates a constitutional rule, it cannot be promoted to `main`.

# Mental Model

Decapod is a governance kernel, not a prompt framework, model router, swarm manager, or wrapper. Humans usually express intent through an agent. The agent invokes Decapod through its CLI or structured RPC interface.

Each invocation is ephemeral. Decapod deliberately runs without a daemon. The repository is the durable execution surface: `.decapod/` preserves the intent, selected context, workspace custody, boundaries, validation evidence, and publication state that should not live only in a chat transcript. One agent task may span many invocations, processes, models, or harnesses.

## The Kernel Analogy

In an operating system, the kernel manages hardware resources and provides a stable API for user-space applications. Decapod performs a similar role for the repository:

- **Resources:** Decapod manages git worktrees, containers, and the state of work units (Todos).
- **API:** Agents call Decapod via a structured CLI or Decapod-specific RPC interface to request resources or validate their state.
- **Isolation:** Decapod ensures that processes (agents) don't interfere with each other or the system's "main memory" (the root repository branch).
- **State:** Decapod records the governed trail of the work in the repository so another agent, reviewer, or CI run can resume or audit without depending on the original transcript.

## The governed execution loop

Decapod is not called for every mechanical step (see the [Single-Agent Workflow](workflows/single-agent.md)). Agents invoke it at decision and state-transition boundaries:

1.  **Intent Pressure:** "I know what to do, but I need to formalize the spec." (see [Explicit Intent](concepts/intent.md), `decapod todo add`, `decapod infer orientation`)
2.  **Boundary Pressure:** "I'm about to touch a sensitive file or move to a new area." (see [Workspace Sandboxing](concepts/workspaces.md), `decapod workspace ensure`, `decapod govern gatekeeper`)
3.  **Coordination Pressure:** "I need to ensure no one else is working on this." (see [Multi-Agent Workflows](workflows/multi-agent.md), `decapod todo claim`, `decapod workspace status`)
4.  **Validation Pressure:** "I need to test the work against repository invariants and respond to the result." (see [Proof & Validation](concepts/proof.md), `decapod validate`)
5.  **Publication Pressure:** "The required gates and evidence pass, so the work may move to a published state." (see `decapod workspace publish`, `decapod todo done`)

The lifecycle is:

```text
intent → interpretation → bounded execution → validation
       → remediation when required → revalidation
       → publication → proof-backed completion
```

A validation failure is not completion. When remediation is supported, the agent inspects the result, identifies the violated invariant, performs the sanctioned remediation, updates the relevant artifact, re-runs validation, and continues toward publication. Some failures require human judgment or cannot be recovered automatically; Decapod keeps those blockers visible.

## Epistemic Custody

A central concept in Decapod is **Epistemic Custody**. This is the preserved, auditable chain between the initial human intent, the context provided to the model, the assumptions made during implementation, and the final proof of completion. Decapod keeps that chain in governed repo state, making agent work fully falsifiable and transparent even after the original session has ended (see [Artifact Reference](reference/artifacts.md)).

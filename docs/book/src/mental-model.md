# Mental Model

To use Decapod effectively, it helps to understand it not as a prompt framework, model router, swarm manager, or wrapper, but as a **kernel** for repository governance. Decapod is built with an agent-first design, users will rarely interface directly with the decapod cli. Most users only install decapod and run the initializer in each project directory.

The practical output of that kernel is durable project state. `.decapod/` preserves the parts of agent work that should not live only in a chat transcript: intent, selected context, workspace custody, boundaries, validation evidence, and completion status.

## The Kernel Analogy

In an operating system, the kernel manages hardware resources and provides a stable API for user-space applications. Decapod performs a similar role for the repository:

- **Resources:** Decapod manages git worktrees, containers, and the state of work units (Todos).
- **API:** Agents call Decapod via a structured CLI or JSON-RPC interface to request resources or validate their state.
- **Isolation:** Decapod ensures that processes (agents) don't interfere with each other or the system's "main memory" (the root repository branch).
- **State:** Decapod records the governed trail of the work in the repository so another agent, reviewer, or CI run can resume or audit without depending on the original transcript.

## Pressure Points: The Agentic Loop

Decapod is not intended to be called for every mechanical step (see the [Single-Agent Workflow](workflows/single-agent.md)). Instead, agents are taught to call Decapod at specific **Pressure Points** where governance is required:

1.  **Intent Pressure:** "I know what to do, but I need to formalize the spec." (see [Explicit Intent](concepts/intent.md), `decapod todo add`, `decapod infer orientation`)
2.  **Boundary Pressure:** "I'm about to touch a sensitive file or move to a new area." (see [Workspace Sandboxing](concepts/workspaces.md), `decapod workspace ensure`, `decapod govern gatekeeper`)
3.  **Coordination Pressure:** "I need to ensure no one else is working on this." (see [Multi-Agent Workflows](workflows/multi-agent.md), `decapod todo claim`, `decapod workspace status`)
4.  **Proof Pressure:** "I have finished the implementation and need to generate verification artifacts." (see [Proof & Validation](concepts/proof.md), `decapod validate`, `decapod todo done`)

## The Thin Waist

Decapod serves as the "thin waist" of agentic software engineering. On the "top" are diverse agents (Claude, Gemini, Codex, etc.) and human developers. On the "bottom" are the actual tools and codebases. Decapod provides the shared, neutral protocol that allows all these entities to converge on a single, verified outcome, without increasing complexity or cognitive load of the human user. All interactions the human wants to execute through decapod, will happen by the agent when the agent determines such an action is necessary.

## Epistemic Custody

A central concept in Decapod is **Epistemic Custody**. This is the preserved, auditable chain between the initial human intent, the context provided to the model, the assumptions made during implementation, and the final proof of completion. Decapod keeps that chain in governed repo state, making agent work fully falsifiable and transparent even after the original session has ended (see [Artifact Reference](reference/artifacts.md)).

# Agent-First Architecture

Decapod exposes a machine-facing governance contract for agents. The human expresses intent and applies judgment; the agent calls the kernel while performing the work.

## Closed-loop execution, proof-backed acceptance

Decapod's agent-facing API turns a rejected boundary or missing obligation into usable feedback for the next iteration. The agent preserves the accepted intent from the original request and subsequent instructions, updates its work, and designs the evidence needed to substantiate the result. This is **closed-loop metacognitive agent governance**.

Validation has a separate role. Required gates execute the agent-developed proof plan under Decapod's governance expectations, bind results to the candidate state, and determine whether the required evidence exists. A well-formed plan or an agent's confidence cannot substitute for executed proof.

[Closed-loop model](convergence.md) · [Error and recovery contract](../reference/errors.md) · [Proof](proof.md)

## The Agentic Lifecycle

Decapod structures agent work into a predictable, machine-readable lifecycle:

1.  **Ingestion & Orientation:** The agent reads `docs/agent/` and queries the `constitution` (see [Repository Constitution](constitution.md)) to understand the repo's rules and available tools.
2.  **Task Claiming:** The agent claims a `todo` to establish exclusive custody and prevent collisions (see [Single-Agent Workflow](../workflows/single-agent.md)).
3.  **Context Resolution:** The agent uses `rpc --op context.resolve` or `infer orientation` to gather the precise context needed for the specific task.
4.  **Implementation:** The agent works in an isolated `workspace` (see [Workspace Sandboxing](workspaces.md)).
5.  **Validation and Recovery:** The agent runs `decapod validate`, follows supported remediation, updates the affected artifact, and revalidates until the work passes or reaches a blocker (see [Proof & Validation](proof.md)).
6.  **Publication:** Passing gates and evidence permit a governed publication transition. Marking a task `done` records completion against that proof surface (see [Artifacts Reference](../reference/artifacts.md)).

## Key Agent-First Concepts

### 1. Deterministic Context
AI models are sensitive to context pollution. Decapod's **Context Capsules** ensure that every agent sees exactly what it needs, and nothing more. This reduces hallucinations and token waste.

### 2. Living Specifications
Living specifications (`.decapod/managed/specs/*`) are the acting agent's explicit interpretation of the repository. The agent authors and maintains them; Decapod requires and validates them. Decapod may refresh supported attestations or projections, but it does not invent the specifications' semantic claims (see [Explicit Intent](intent.md)).

An incorrect specification exposes the agent's misunderstanding before publication. That is a successful governance outcome: a visible misunderstanding can be reviewed and corrected, while one hidden in transient model context cannot. A stale specification generally means the governed work remains incomplete.

### 3. Aptitude & Memory
Shared memory allows agents to learn from each other. If one agent discovers an obscure bug in a library, it can record that observation in Aptitude, which subsequent agents will automatically retrieve during context resolution.

### 4. Protocol-Native (MCP)
Decapod reserves an adapter boundary for the **Model Context Protocol (MCP)** so future integrations can expose the repository as a structured resource graph (see [Model Context Protocol (MCP)](mcp.md)). The current binary provides a Decapod-specific RPC interface; it does not itself implement MCP.


## Design Patterns for Agents

- **Pressure Points:** Call Decapod at decision boundaries (e.g., before choosing a library).
- **Epistemic Custody:** Preserve the "Why" behind a change in the `INTENT.md` spec.
- **Follow validation:** Use `decapod validate` early, remediate supported failures, and re-run it before publication.

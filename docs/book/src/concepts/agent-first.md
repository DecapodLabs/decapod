# Agent-First Architecture

Decapod is not just a CLI; it is an **Agent-First Governance Kernel**. This means every feature is designed with the AI model's cognitive lifecycle in mind, rather than just the human operator's convenience.

## The Agentic Lifecycle

Decapod structures agent work into a predictable, machine-readable lifecycle:

1.  **Ingestion & Orientation:** The agent reads `docs/agent/` and queries the `constitution` (see [Repository Constitution](constitution.md)) to understand the repo's rules and available tools.
2.  **Task Claiming:** The agent claims a `todo` to establish exclusive custody and prevent collisions (see [Single-Agent Workflow](../workflows/single-agent.md)).
3.  **Context Resolution:** The agent uses `rpc --op context.resolve` or `infer orientation` to gather the precise context needed for the specific task.
4.  **Implementation:** The agent works in an isolated `workspace` (see [Workspace Sandboxing](workspaces.md)).
5.  **Validation:** The agent runs `decapod validate` to verify its work against local gates *before* human review (see [Proof & Validation](proof.md)).
6.  **Proof Generation:** Marking a task `done` generates cryptographic and evidence-based artifacts that prove the work was governed (see [Artifacts Reference](../reference/artifacts.md)).

## Key Agent-First Concepts

### 1. Deterministic Context
AI models are sensitive to context pollution. Decapod's **Context Capsules** ensure that every agent sees exactly what it needs, and nothing more. This reduces hallucinations and token waste.

### 2. Living Specifications
Agents should not just "write code"; they should maintain intent. Decapod promotes the use of "Living Specs" (`.decapod/managed/specs/*`) which are synchronized with both the code and the agent's internal state (see [Explicit Intent](intent.md)).

### 3. Aptitude & Memory
Shared memory allows agents to learn from each other. If one agent discovers an obscure bug in a library, it can record that observation in Aptitude, which subsequent agents will automatically retrieve during context resolution.

### 4. Protocol-Native (MCP)
Decapod reserves an adapter boundary for the **Model Context Protocol (MCP)** so future integrations can expose the repository as a structured resource graph (see [Model Context Protocol (MCP)](mcp.md)). The current binary provides a Decapod-specific RPC interface; it does not itself implement MCP.


## Design Patterns for Agents

- **Pressure Points:** Call Decapod at decision boundaries (e.g., before choosing a library).
- **Epistemic Custody:** Preserve the "Why" behind a change in the `INTENT.md` spec.
- **Fail Fast:** Use `decapod validate` early and often to catch alignment drift.

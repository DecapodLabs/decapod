# Agent-First Architecture

Decapod is not just a CLI; it is an **Agent-First Governance Kernel**. This means every feature is designed with the AI model's cognitive lifecycle in mind, rather than just the human operator's convenience.

## The Agentic Lifecycle

Decapod structures agent work into a predictable, machine-readable lifecycle:

1.  **Ingestion & Orientation:** The agent reads `docs/agent/` and queries the `constitution` to understand the repo's rules and available tools.
2.  **Task Claiming:** The agent claims a `todo` to establish exclusive custody and prevent collisions.
3.  **Context Resolution:** The agent uses `rpc --op context.resolve` or `infer orientation` to gather the precise context needed for the specific task.
4.  **Implementation:** The agent works in an isolated `workspace` (worktree or container).
5.  **Validation:** The agent runs `decapod validate` to verify its work against local gates *before* human review.
6.  **Proof Generation:** Marking a task `done` generates cryptographic and evidence-based artifacts that prove the work was governed.

## Key Agent-First Concepts

### 1. Deterministic Context
AI models are sensitive to context pollution. Decapod's **Context Capsules** ensure that every agent sees exactly what it needs, and nothing more. This reduces hallucinations and token waste.

### 2. Living Specifications
Agents should not just "write code"; they should maintain intent. Decapod promotes the use of "Living Specs" (`.decapod/generated/specs/*`) which are synchronized with both the code and the agent's internal state.

### 3. Aptitude & Memory
Shared memory allows agents to learn from each other. If one agent discovers a obscure bug in a library, it can record that observation in Aptitude, which subsequent agents will automatically retrieve during context resolution.

### 4. Protocol-Native (MCP)
By supporting the **Model Context Protocol (MCP)**, Decapod allows agents to treat the repository as a structured resource graph rather than a raw filesystem.

## Design Patterns for Agents

- **Pressure Points:** Call Decapod at decision boundaries (e.g., before choosing a library).
- **Epistemic Custody:** Preserve the "Why" behind a change in the `INTENT.md` spec.
- **Fail Fast:** Use `decapod validate` early and often to catch alignment drift.
- **Skill Discovery:** Instead of hardcoding steps, use `skill resolve` to find the project's established best practices.

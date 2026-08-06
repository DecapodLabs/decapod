# Decapod Documentation

**Decapod** is a daemonless, repo-native governance kernel for AI coding agents. It turns human intent into bounded, durable, and proof-backed agent work.

This book explains the product, its execution model, and the repository contracts used by humans, agents, and reviewers.

---

## 🚀 Getting Started

*   **[Introduction](introduction.md)**: What is Decapod? Learn about the governance gap and the core pillars of repo-native agent control.
*   **[Quickstart](quickstart.md)**: Install, initialize, and run your first agent handshake and validation in under five minutes.
*   **[Mental Model](mental-model.md)**: Understand how agents, tasks, sessions, and workspaces interact.
*   **[Configuration](configuration.md)**: Structure your repository config, enable cloud backends, and configure containerization.

---

## 🔄 Governed Workflows

Learn how agents move through the lifecycle of planning, execution, validation, and completion.

*   **[Single-Agent Workflows](workflows/single-agent.md)**: The lifecycle of an agent claim, ensure, validate, and finish loop.
*   **[Multi-Agent Workflows](workflows/multi-agent.md)**: Handling concurrent agents, task claiming, and locking database state.
*   **[Workspace Isolation](workflows/workspace-isolation.md)**: Setting up isolated Git worktrees and Docker containers to run tasks securely.
*   **[External Trackers](workflows/external-trackers.md)**: Integrating Decapod with Jira, Linear, or GitHub Issues.

---

## 💡 Core Concepts

Deep dive into the architecture and mechanisms that make Decapod unique.

*   **[Agent-First Architecture](concepts/agent-first.md)**: Why Decapod is designed to be called directly at agent "inference pressure points".
*   **[Explicit Intent](concepts/intent.md)**: Converting ambiguous prompts into concrete, versioned specifications.
*   **[Workspace Sandboxing](concepts/workspaces.md)**: How isolated execution layers keep your primary branches clean and safe.
*   **[Proof & Validation](concepts/proof.md)**: Verifying correctness programmatically through policy evaluation instead of agent self-reporting.
*   **[Common Questions](concepts/faq.md)**: Short architectural answers about agents, specifications, daemonless execution, trackers, and proof-backed completion.
*   **[Repository Constitution](concepts/constitution.md)**: Setting the global guidelines that steer agent behavior.
*   **[Config Overrides](concepts/overrides.md)**: Project-specific adjustments to constitutional guidelines.
*   **[Model Context Protocol (MCP)](concepts/mcp.md)**: Navigating Decapod tools through structured agent protocols.

---

## 📖 Reference Manual

Hard specifications, command lists, configuration schemas, and error codes.

*   **[Config Specification (config.toml)](reference/config-toml.md)**: Key-value reference for repo-level policy control.
*   **[CLI Reference](reference/cli.md)**: Detailed breakdown of commands and options (init, validate, session, todo, decide).
*   **[Error Reference](reference/errors.md)**: Decapod exit codes, validator failures, and self-healing instructions.
*   **[Artifact Reference](reference/artifacts.md)**: Layout and schema of generated intent, handshake, and validation specs.

---

## 🛠️ Developer Resources

To contribute or integrate Decapod into your platform:
*   Read the **[Contributing Guidelines](contributing.md)**.
*   Visit the main GitHub Repository: [DecapodLabs/decapod](https://github.com/DecapodLabs/decapod)

# Model Context Protocol (MCP)

Decapod is designed to be a "Protocol First" governance layer. While it provides a rich CLI, its core value is in standardizing the context and control signals exchanged between repositories and AI models. This aligns perfectly with the **Model Context Protocol (MCP)**.

## Decapod as an MCP Provider

Decapod integrates with MCP-compatible environments (like Claude Desktop or specialized IDE extensions) by acting as an MCP Server. This allows agents to access Decapod's governance kernel directly through standard protocol messages.

### Exposed MCP Resources

- **Constitution:** Browsable nodes of the technology and methodology graph.
- **Tasks:** The current todo list and ownership state.
- **Specs:** The living specifications (`INTENT.md`, `ARCHITECTURE.md`, etc.) for the project.
- **Context Capsules:** Deterministic snapshots of repo state used for inference.

### Exposed MCP Tools

Decapod exposes its core CLI capabilities as MCP Tools, allowing agents to:
- `todo_add` / `todo_claim`
- `workspace_ensure`
- `constitution_search`
- `validate`

## Benefits of MCP Integration

1.  **Lower Friction:** Agents don't need to "learn" CLI flags if they can call structured tools via MCP.
2.  **Rich Context:** MCP allows for richer metadata (like URIs and structured resources) to be passed alongside documentation.
3.  **Discovery:** MCP-native agents can automatically discover Decapod's capabilities as soon as they connect to the repository.

## Handshaking and Identity

Decapod uses its `handshake` command to generate a deterministic identity artifact. In an MCP context, this ensures that the agent provider and the local governance kernel have a shared, cryptographically verifiable understanding of which agent is operating on which task.

## Future Direction

We are actively expanding Decapod's MCP surface to support native resource templates and more granular tool definitions, making Decapod the standard "Governance MCP" for any repository it manages.

# Model Context Protocol (MCP)

Decapod supports integration with MCP-compatible agent environments to provide structured resources and tools.

## Resources

When connected via MCP, Decapod exposes:
- `decapod://constitution/*`: Direct access to constitution nodes.
- `decapod://todo/list`: The current actionable backlog.
- `decapod://specs/intent`: The project's intent specification.

## Tools

Standard Decapod operations are available as MCP tools. If your environment supports MCP, you SHOULD use these tools instead of raw CLI calls to benefit from structured parsing and better error handling.

Key tools include:
- `orientation_get`: Retrieve the orientation packet for a task.
- `validate_repo`: Run the full project validation gate.
- `aptitude_query`: Search the shared memory for project-specific knowledge.

## Handshake

In MCP sessions, ensure you have called `decapod handshake` at least once to establish your agent identity within the repository's governance log. This identity persists across tool calls and resource retrievals.

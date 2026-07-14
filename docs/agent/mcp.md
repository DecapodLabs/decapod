# Model Context Protocol (MCP)

Decapod currently provides a Decapod-specific structured RPC interface over process stdin/stdout. It does not currently implement an MCP server, MCP lifecycle, MCP resource discovery, or MCP tool discovery.

MCP adapters remain a planned integration surface. An adapter may map MCP resources and tools to Decapod operations, but that mapping is not part of the current Decapod binary contract.

## Current local handshake

`decapod handshake` records local declarations, scope, proof declarations, document hashes, and a deterministic artifact hash. The hash makes the recorded data tamper-evident; it does not authenticate a model provider, harness, binary, human principal, or organization.

The local session credential establishes repository-local custody and correlation for the current session. It must not be described as provider authentication.

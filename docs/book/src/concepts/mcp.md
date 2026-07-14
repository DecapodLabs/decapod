# Model Context Protocol (MCP)

Decapod's current agent interface is a Decapod-specific structured RPC envelope over process stdin/stdout. It is not JSON-RPC 2.0 and the binary does not currently implement an MCP server, MCP lifecycle, or native MCP resource and tool discovery.

MCP support is an adapter boundary tracked separately from the current runtime. Future adapters may expose Decapod resources and operations through MCP, but those bindings should not be read as capabilities of the current binary.

## Current local handshake

`decapod handshake` records local declarations, scope, proof declarations, document hashes, and a deterministic artifact hash. That hash provides tamper-evident integrity for the recorded handshake data; it does not authenticate a model provider, harness, binary, human principal, or organization.

The repository-local session credential establishes local custody and correlation for the current session. It is not external provider authentication. See [Identity and provenance](https://github.com/DecapodLabs/decapod/issues/876) and [the interoperability profile](https://github.com/DecapodLabs/decapod/issues/870) for the boundaries future adapters must preserve.

# Model Context Protocol (MCP)

Decapod currently provides a CLI and a Decapod-specific structured RPC interface over process stdin/stdout (plus an optional loopback HTTP server via `decapod rpc --http-server`). It does not currently implement an MCP server, MCP lifecycle, MCP resource discovery, or MCP tool discovery.

MCP adapters remain a planned integration surface. An adapter may map MCP resources and tools to Decapod operations, but that mapping is not part of the current Decapod binary contract.

## Current local handshake

`decapod session handshake --scope <scope>` records local declarations, scope, proof declarations, document hashes, and a deterministic artifact hash. That hash provides tamper-evident integrity for the recorded handshake data; it does not authenticate a model provider, harness, binary, human principal, or organization.

The handshake's `identity_assertions` preserve identity claims separately from verification. An assertion carries its claim kind, subject type, value, evidence class, scope, lifecycle, authority/verifier slots, method, and result. Environment-provided agent and provider values are `self-declared` and `unverified` unless a configured trust root establishes stronger evidence.

The repository-local session credential establishes local custody and correlation for the current session. It is not external provider authentication. See [BYOA & Interoperability](interoperability.md) for the live `decapod.interop` contract, and see [the interoperability profile](https://github.com/DecapodLabs/decapod/issues/870), [capability negotiation](https://github.com/DecapodLabs/decapod/issues/871), [the native MCP adapter](https://github.com/DecapodLabs/decapod/issues/872), [the A2A adapter](https://github.com/DecapodLabs/decapod/issues/873), [the optional HTTP transport](https://github.com/DecapodLabs/decapod/issues/874), and [identity and provenance](https://github.com/DecapodLabs/decapod/issues/876) for the boundaries future adapters must preserve.

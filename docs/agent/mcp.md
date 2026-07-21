# Model Context Protocol (MCP)

Decapod currently provides a Decapod-specific structured RPC interface over process stdin/stdout. It does not currently implement an MCP server, MCP lifecycle, MCP resource discovery, or MCP tool discovery.

MCP adapters remain a planned integration surface. An adapter may map MCP resources and tools to Decapod operations, but that mapping is not part of the current Decapod binary contract.

## Current local handshake

`decapod handshake` records local declarations, scope, proof declarations, document hashes, and a deterministic artifact hash. The hash makes the recorded data tamper-evident; it does not authenticate a model provider, harness, binary, human principal, or organization.

Handshake records also expose identity-adjacent values as `identity_assertions`. Each assertion keeps the claim kind, subject type, asserted value, evidence class, scope, lifecycle fields, authority/verifier slots, verification method, and claim-specific result together. Environment-provided agent and provider values are recorded as `self-declared` with an `unverified` result; they are not promoted to authenticated identity without a configured trust root.

The local session credential establishes repository-local custody and correlation for the current session. It must not be described as provider authentication.

## Related adapter work

The future-facing boundaries are tracked separately: [the interoperability profile](https://github.com/DecapodLabs/decapod/issues/870), [capability negotiation](https://github.com/DecapodLabs/decapod/issues/871), [the native MCP adapter](https://github.com/DecapodLabs/decapod/issues/872), [the A2A adapter](https://github.com/DecapodLabs/decapod/issues/873), [the optional HTTP transport](https://github.com/DecapodLabs/decapod/issues/874), and [identity and provenance](https://github.com/DecapodLabs/decapod/issues/876).

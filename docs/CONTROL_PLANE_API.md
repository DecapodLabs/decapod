# Decapod Control Plane API

## Scope

This document defines the stable API contract for agents and wrappers integrating with Decapod.

## Stable Surfaces

CLI contract:

- `decapod validate`
- `decapod rpc --stdin`
- `decapod handshake --scope <scope> --proof <cmd>...`
- `decapod session init`
- `decapod release check`

RPC envelope (v1):

Request fields:

- `id` (request_id)
- `op`
- `params`
- `session` (optional)

Response fields:

- `id`
- `success`
- `receipt`
- `result`
- `allowed_next_ops`
- `blocked_by`
- `error`

See golden vectors:

- `tests/golden/rpc/v1/agent_init.request.json`
- `tests/golden/rpc/v1/agent_init.response.json`

## Interface Stability Policy

SemVer policy:

- Patch: bug fixes, no schema-breaking envelope changes.
- Minor: backward-compatible additive fields/ops.
- Major: breaking CLI flags, breaking RPC envelope/schema, breaking compatibility guarantees.

Compatibility guarantees:

- Existing envelope fields MUST NOT be removed in minor/patch versions.
- New fields MUST be additive and optional for older clients.
- Golden vectors are required contract anchors.

## Agent Handshake Protocol

A compliant agent handshake MUST:

1. Declare it read `CLAUDE.md` and contract docs.
2. Report Decapod repo version.
3. Declare intended scope.
4. Declare proof commands it will run.
5. Emit a hashed handshake record in `.decapod/records/handshakes/`.

Command:

```bash
decapod handshake --scope "<scope>" --proof "decapod validate"
```

# DB_BROKER.md - SQLite Front Door (Local-First)

**Authority:** guidance (design scope; not implemented yet)
**Layer:** Interfaces
**Binding:** No
**Scope:** intended broker interface and invariants for multi-agent SQLite safety
**Non-goals:** distributed system semantics or networked IPC (until proven)

This doc scopes the DB broker subsystem that sits in front of SQLite for multi-agent correctness.

## Goal

Turn “agents poking SQLite” into “agents sending requests” so we can get determinism, auditability, and eventually policy.

The broker is a *thin, local-first* request layer. It solves two problems first:

1. Serialized writes (multi-writer safety).
2. Read de-dupe and in-flight coalescing (multi-agent efficiency + consistency).

## Non-Goals (Now)

- Distributed system semantics.
- Networked “universal” broker.
- Pluggable everything.
- Cross-process IPC (until the in-process design proves out).

If we need cross-process later, we add a Unix socket front-end after the in-process broker is stable.

## Architecture (Phase 1: In-Process)

- One broker instance in the Rust process.
- One request queue.
- One worker loop (single authority).
- Explicit request types; no arbitrary SQL passthrough as the public API.

## Request Protocol (Shape)

All broker requests are explicit and typed.

### Read

- Key for de-dupe/coalescing:
  - `(db_id, query_fingerprint, params_hash)`
- Behavior:
  - If identical read is already in-flight, join and return the same in-flight result.
  - If the same read finished “recently”, serve from a tiny TTL cache.
  - Reads must be bounded: timeout, max rows/bytes, and cancellation where possible.

### Write

- Always serialized per DB (or per logical namespace later).
- Optional idempotency keys:
  - repeated requests with the same key should not double-apply.
- Behavior:
  - Apply mutation.
  - Emit audit event.
  - Invalidate affected cache keys.

## Audit Trail (Always-On)

The broker emits an append-only audit trail for every request:

- `ts`, `request_id`, `actor` (agent), `store_root`, `db_id`
- `request_type`, `key` (for reads), `idempotency_key` (for writes, if present)
- `status`, `latency_ms`
- `affected_keys` / invalidations

This is a proof surface: “show me every mutation and who did it.”

## Incremental Rollout Plan

1. Add broker module with in-process queue and explicit request types for existing subsystems.
2. Refactor subsystems to call broker instead of opening SQLite directly.
3. Add validate gate: “no code outside broker opens SQLite”.
4. Only if needed: add a daemon/IPC front door so multiple agent processes share one broker.

## Golden Invariant (Enforced Later)

No code outside the broker opens SQLite.

## Links

- `embedded/core/CONTROL_PLANE.md`
- `embedded/core/PLUGINS.md`
- `embedded/plugins/VERIFY.md`
- `embedded/specs/ARCHITECTURE.md`
- `embedded/specs/INTENT.md`
- `embedded/specs/SYSTEM.md`

When we reach step (3) above, `decapod validate --store repo` should fail if any `rusqlite::Connection::open` (or equivalent open path) is used outside the broker module.

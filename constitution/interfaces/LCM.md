# LCM — Lossless Context Management

## Purpose

LCM provides the **memory layer** for Decapod agents. It prevents agents from inventing ad-hoc chunking loops while preserving the append-only, deterministic, auditable store model.

Two subsystems:

1. **`decapod lcm`** — Immutable originals ledger + deterministic summary DAG.
2. **`decapod map`** — Structured parallel processing with scope-reduction enforcement.

## LCM Store Model

### Originals (append-only, never mutated)

Stored in `lcm.events.jsonl` — an append-only JSONL ledger.

Each entry contains:
- `event_id` — ULID, globally unique
- `ts` — ISO 8601 timestamp
- `actor` — agent identifier
- `content_hash` — SHA256 of raw content bytes (deterministic)
- `kind` — one of: `event`, `message`, `artifact`, `tool_result`
- `content` — verbatim original text
- `metadata` — session_id, source, etc.

### Derived Index

`lcm.db` is a SQLite database that indexes the ledger:
- `originals_index` — content_hash, event_id, ts, actor, kind, byte_size, session_id
- `summaries` — summary_hash, ts, scope, original_hashes, summary_text, token_estimate
- `meta` — key-value configuration

**The index is always rebuildable from the ledger.** If `lcm.db` is deleted, it can be reconstructed by replaying `lcm.events.jsonl`.

### Summaries

Summaries are deterministic:
- Same originals in timestamp order produce the same summary hash.
- Summary hash = SHA256(original_hashes joined by comma | summary_text).
- Summaries reference originals by content hash, forming a DAG.

## Map Operators

### `map llm` — Stateless Parallel Processing

Applies a prompt template + JSON schema to each item in a JSON array. The operator defines the **contract** (input format, output schema, audit trail) — actual LLM inference is pluggable.

### `map agentic` — Subagent Delegation

Delegates items to subagents with mandatory scope-reduction:
- The `--retain` flag declares what the caller keeps responsibility for.
- If `--retain` is empty, the command rejects with: "Delegation without retention violates scope-reduction invariant."
- Each delegation is logged to `map.events.jsonl`.

## Determinism Guarantees

1. **Content addressing**: SHA256 of raw bytes — same content always produces same hash.
2. **Append-only ledger**: Events are never mutated or deleted.
3. **Deterministic summaries**: Same originals produce the same summary hash across runs.
4. **Rebuildable index**: `lcm.db` can always be reconstructed from `lcm.events.jsonl`.
5. **Audit trail**: All map operations logged with input/output hashes.

## Validation Gate

`decapod validate` includes the **LCM Immutability Gate** which verifies:
- Every entry's `content_hash` matches SHA256(content).
- No duplicate `event_id` values.
- Monotonic timestamps (each entry >= previous).

## Progressive Disclosure

- **Level 0**: `decapod lcm schema` — discover capabilities.
- **Level 1**: `decapod lcm ingest` / `decapod lcm list` — store and browse originals.
- **Level 2**: `decapod lcm summarize` / `decapod lcm summary` — produce and inspect summaries.
- **Level 3**: `decapod map llm` / `decapod map agentic` — structured parallel processing.

# DB Broker Write Queue + Cache Specification

## Problem

SQLite lock contention occurs when multiple agents try to write simultaneously. The current broker opens connections per-operation with per-DB locks, but this doesn't prevent:
- Database is locked errors
- Write serialization failures
- Retry loops

## Solution

Enhance the broker with:
1. **Write Queue**: Serialized write pipeline that queues mutations and processes them sequentially
2. **Read Cache**: In-memory cache that serves reads without hitting SQLite

## Architecture

```
Agent CLI Call
       │
       ▼
┌──────────────────┐
│  DbBroker       │
│  ┌────────────┐ │
│  │ Write Queue │ │  ← Serialized mutation pipeline
│  └────────────┘ │
│  ┌────────────┐ │
│  │ Read Cache │ │  ← In-memory cache with TTL
│  └────────────┘ │
└──────────────────┘
       │
       ▼
┌──────────────────┐
│ SQLite DB        │
└──────────────────┘
```

## Implementation

### 1. Write Queue

- Mutex-protected queue of pending writes
- Each write has: db_path, sql, params, result_sender
- Background thread processes queue sequentially
- Returns result via channel

```rust
struct WriteRequest {
    db_path: PathBuf,
    sql: String,
    params: Vec<Box<dyn rusqlite::ToSql>>,
    result_tx: oneshot::Sender<Result<(), Error>>,
}
```

### 2. Read Cache

- HashMap keyed by (db_path, query_hash, params_hash)
- Cache entries have TTL (configurable, default 5s)
- Cache invalidation on writes to same DB
- Check cache before hitting SQLite

```rust
struct CacheEntry {
    value: serde_json::Value,
    expires_at: Instant,
}
```

### 3. Broker API Changes

```rust
impl DbBroker {
    // Queue a write operation (async, returns result via channel)
    pub fn queue_write(&self, db_path, sql, params) -> impl Future<Output = Result<()>>
    
    // Read from cache or DB
    pub fn readCached<F, R>(&self, db_path, query, f: F) -> Result<R>
    where F: FnOnce(&Connection) -> Result<R>
}
```

## Files to Modify

- `src/core/broker.rs`: Add write queue and cache
- `src/core/db.rs`: Maybe add helper functions
- Add tests for queue and cache behavior

## Backward Compatibility

- Keep existing `with_conn` for reads that need fresh data
- New `queue_write` is opt-in
- Cache can be disabled via env var

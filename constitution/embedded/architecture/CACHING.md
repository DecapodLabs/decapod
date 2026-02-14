# CACHING.md - Caching Architecture

**Authority:** guidance (caching strategies, invalidation, and performance patterns)
**Layer:** Guides
**Binding:** No
**Scope:** caching patterns, cache levels, and invalidation strategies
**Non-goals:** specific cache implementations, cache-as-database patterns

---

## 1. Caching Principles

### 1.1 Cache Purpose
Cache is a **performance optimization**, not a:
- Source of truth
- Consistency mechanism
- Data storage layer
- Reliability guarantee

### 1.2 The Two Hard Problems
"There are only two hard things in Computer Science: cache invalidation and naming things."

**Design for invalidation first:**
- How will this cache entry be invalidated?
- What events trigger invalidation?
- How do we handle invalidation failures?
- What's the blast radius of stale data?

### 1.3 Cache Trade-offs

| Aspect | Cache Hit | Cache Miss |
|--------|-----------|------------|
| Latency | Low | High (fetch + store) |
| Throughput | High | Variable |
| Consistency | Stale | Fresh |
| Complexity | High | Low |

---

## 2. Cache Levels

### 2.1 L1: In-Memory (Application)
- **Scope:** Single process
- **Speed:** Fastest (microseconds)
- **Size:** Limited by heap/available memory
- **Eviction:** LRU, LFU, TTL
- **Use for:** Hot data, computed values, parsed configs

**Implementation:**
- ConcurrentHashMap (Java)
- sync.Map (Go)
- Dictionary (Python)
- std::unordered_map (C++)

### 2.2 L2: Distributed (Redis/Memcached)
- **Scope:** Multiple processes/servers
- **Speed:** Fast (milliseconds)
- **Size:** GB range
- **Eviction:** Configurable (LRU, random, TTL)
- **Use for:** Session data, rate limiting, aggregated data

**Redis vs Memcached:**
- Redis: Data structures, persistence, pub/sub
- Memcached: Simple, multi-threaded, memory efficient

### 2.3 L3: CDN (CloudFront/Cloudflare)
- **Scope:** Global edge locations
- **Speed:** Fastest for end users
- **Size:** Large (TB range)
- **Eviction:** TTL-based
- **Use for:** Static assets, API responses, full pages

### 2.4 L4: Browser Cache
- **Scope:** Single user
- **Speed:** Instant (no network)
- **Control:** Limited (HTTP headers)
- **Use for:** Static assets, API responses with Cache-Control

---

## 3. Caching Patterns

### 3.1 Cache-Aside (Lazy Loading)
```
1. Check cache
2. If miss: fetch from DB, store in cache, return
3. If hit: return cached value
```

**Pros:** Simple, cache only what's needed
**Cons:** Cache stampede on expiry

### 3.2 Write-Through
```
1. Write to cache
2. Write to DB (synchronously)
3. Return success
```

**Pros:** Consistency, no stale reads
**Cons:** Write latency, cache churn for write-heavy workloads

### 3.3 Write-Behind (Write-Back)
```
1. Write to cache
2. Return success immediately
3. Async write to DB
```

**Pros:** Low write latency, high write throughput
**Cons:** Data loss risk, eventual consistency complexity

### 3.4 Refresh-Ahead
```
1. Background process refreshes cache before expiry
2. Users always get cache hits
```

**Pros:** No cache misses for users
**Cons:** Complex, wastes resources if data not accessed

---

## 4. Cache Invalidation Strategies

### 4.1 TTL (Time To Live)
- Set expiration time on cache entry
- Simple, automatic cleanup
- Stale data possible until TTL expires

**Best for:** Slowly changing data, temporary data

### 4.2 Explicit Invalidation
- Application invalidates cache on write
- Immediate consistency
- Requires cache write on every DB write

**Best for:** Critical data, small working set

### 4.3 Event-Driven Invalidation
- Database publishes change events
- Cache subscribes and invalidates
- Decoupled, scalable

**Best for:** Distributed systems, microservices

### 4.4 Version-Based Invalidation
- Cache key includes version
- New version = new key
- Old entries expire naturally

**Best for:** Immutable data, deployments

---

## 5. Cache Stampede Prevention

### 5.1 The Problem
When cache expires, multiple requests hit DB simultaneously.

### 5.2 Solutions

**Per-Item Jitter:**
- Add random offset to TTL
- Stagger expiry across cache entries

**Mutex/Lock:**
- First request locks and rebuilds
- Others wait or serve stale

**External Recomputation:**
- Background process updates cache
- Application never experiences miss

**Probabilistic Early Expiration:**
- Expire with probability before TTL
- Reduces thundering herd

---

## 6. Cache Warming

### 6.1 When to Warm
- Application startup
- Cache failure/restart
- Deployment (new version)
- Daily/scheduled (predictable access patterns)

### 6.2 What to Warm
- Most frequently accessed data
- Computationally expensive results
- Critical path data (can't afford miss)

### 6.3 How to Warm
- Read-through on startup
- Background job populates cache
- Lazy loading with pre-warming for hot data

---

## 7. Monitoring & Alerting

### 7.1 Key Metrics
- **Hit rate:** Target > 90% for hot data
- **Miss rate:** Track by endpoint/query
- **Eviction rate:** Should be steady, not spiking
- **Latency:** P50, P95, P99 for cache operations
- **Memory usage:** Prevent OOM

### 7.2 Alerting Thresholds
- Hit rate drops below threshold
- Memory usage > 80%
- Connection errors
- Eviction rate spikes

### 7.3 Cache Efficiency
- Cache hit rate alone isn't enough
- Measure end-to-end latency improvement
- Consider cost per cached item

---

## 8. Anti-Patterns

- **Cache as database:** Don't rely on cache persistence
- **No TTL:** Cache grows forever, memory leak
- **No invalidation:** Stale data served indefinitely
- **Over-caching:** Cache everything, complex invalidation
- **Cache bypass:** Not using cache for hot data
- **Large objects:** Cache small, frequently accessed items
- **No monitoring:** Blind to cache performance
- **Single cache server:** SPOF for performance

---

## Links

- `embedded/methodology/ARCHITECTURE.md` - CTO-level architecture doctrine
- `embedded/architecture/DATA.md` - Data architecture
- `embedded/architecture/MEMORY.md` - Memory management
- `embedded/architecture/CONCURRENCY.md` - Concurrent cache access

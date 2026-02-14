# WEB.md - Web Architecture

**Authority:** guidance (web protocols, API design, and stateless service patterns)
**Layer:** Guides
**Binding:** No
**Scope:** HTTP protocols, API patterns, and web service architecture
**Non-goals:** specific frameworks, frontend implementation details

---

## 1. Web Architecture Principles

### 1.1 Statelessness
**HTTP is stateless.** Server treats each request independently.
- Scalability: Any server can handle any request
- Reliability: No server affinity required
- Simplicity: No session state to manage

**State Management:**
- Client-side: Tokens, cookies, localStorage
- Server-side: Database, cache (not server memory)
- URL-based: Resource identifiers

### 1.2 Resource-Oriented Design
Everything is a resource with:
- **URI:** Unique identifier (/users/123)
- **Methods:** Actions (GET, POST, PUT, DELETE)
- **Representation:** Format (JSON, XML, HTML)
- **Statelessness:** Self-contained requests

### 1.3 HTTP/2 and HTTP/3
**HTTP/2 (baseline):**
- Multiplexing: Multiple requests per connection
- Header compression: HPACK
- Server push: Proactive resource sending
- Binary protocol: More efficient parsing

**HTTP/3 (next-gen):**
- QUIC transport: UDP-based, faster handshake
- Built-in TLS: Security by default
- Connection migration: Survive network changes
- Reduced latency: 0-RTT for repeat connections

---

## 2. API Design Patterns

### 2.1 REST (Representational State Transfer)
**Constraints:**
- Client-server separation
- Stateless interactions
- Cacheable responses
- Uniform interface (resources, methods)
- Layered system

**Best Practices:**
- Nouns for resources (/orders), not verbs (/createOrder)
- Plural for collections (/users), singular for singletons
- Use HTTP status codes correctly
- Version in URL (/v1/users) or header
- Pagination for collections

### 2.2 GraphQL
**When to use:**
- Complex data requirements
- Mobile apps (reduce over-fetching)
- Rapidly evolving frontends
- Aggregating multiple services

**When to avoid:**
- Simple CRUD operations
- File uploads/downloads
- High-performance requirements
- Caching-heavy workloads

### 2.3 gRPC
**When to use:**
- Internal service communication
- High-performance requirements
- Strong typing needed
- Streaming operations

**When to avoid:**
- Public APIs (browser support limited)
- Simple request/response
- Debugging needs (binary protocol)

### 2.4 WebSocket
**When to use:**
- Real-time bidirectional communication
- Live updates (chat, notifications)
- Low-latency requirements
- Persistent connections

**When to avoid:**
- Stateless/scalable requirements
- Simple request/response
- HTTP caching benefits needed

---

## 3. API Design Best Practices

### 3.1 URL Design
```
Good:
GET /users?page=2&limit=10
POST /orders
PUT /users/123
DELETE /orders/456

Bad:
GET /getUsers
POST /createOrder
GET /users/123/update
```

### 3.2 Status Codes
- **200 OK:** Success
- **201 Created:** Resource created
- **204 No Content:** Success, no body
- **400 Bad Request:** Client error (validation)
- **401 Unauthorized:** Authentication required
- **403 Forbidden:** No permission
- **404 Not Found:** Resource doesn't exist
- **409 Conflict:** Business logic conflict
- **422 Unprocessable:** Semantic errors
- **429 Too Many Requests:** Rate limited
- **500 Internal Error:** Server error
- **503 Service Unavailable:** Temporary issue

### 3.3 Request/Response Format
**Consistency:**
- Use JSON by default
- CamelCase for keys
- ISO 8601 for dates
- Consistent error format

**Error Response:**
```json
{
  "error": {
    "code": "INVALID_PARAMETER",
    "message": "Email is required",
    "field": "email",
    "requestId": "uuid"
  }
}
```

### 3.4 Pagination
**Offset-based:**
- `?page=2&limit=10`
- Simple, works with SQL
- Inconsistent on data changes

**Cursor-based:**
- `?cursor=abc123&limit=10`
- Consistent on data changes
- Requires ordered unique field

**Response:**
```json
{
  "data": [...],
  "pagination": {
    "nextCursor": "xyz789",
    "hasMore": true,
    "total": 1000
  }
}
```

---

## 4. Security

### 4.1 Authentication
**JWT (JSON Web Tokens):**
- Stateless, self-contained
- Signed, optionally encrypted
- Short-lived access tokens
- Refresh token rotation

**OAuth 2.0:**
- Authorization framework
- Grant types: code, implicit, client credentials
- PKCE for mobile/SPA
- Scope-based permissions

**API Keys:**
- Simple, for server-to-server
- Limited scope and rate
- Rotate regularly

### 4.2 HTTPS Everywhere
- TLS 1.2+ required
- Certificate pinning for mobile
- HSTS headers
- Redirect HTTP to HTTPS

### 4.3 Input Validation
- Validate at API boundary
- Schema validation (JSON Schema)
- Sanitize inputs (XSS prevention)
- Size limits (prevent DoS)

### 4.4 Rate Limiting
- Per-user, per-IP, per-endpoint
- Burst vs sustained limits
- Return 429 with Retry-After
- Different limits per tier

---

## 5. Performance

### 5.1 Caching
**Cache-Control headers:**
- `max-age=3600`: Cache for 1 hour
- `no-cache`: Revalidate every time
- `no-store`: Never cache
- `private`: Browser only, not CDN
- `public`: CDN can cache

**ETags:**
- Content-based versioning
- 304 Not Modified responses
- Bandwidth savings

### 5.2 Compression
- Gzip: Universal support
- Brotli: Better compression, modern browsers
- Compress responses > 1KB
- Skip compression for images (already compressed)

### 5.3 Connection Management
- Keep-alive for HTTP/1.1
- Connection pooling
- HTTP/2 multiplexing
- Circuit breakers for resilience

---

## 6. Resilience Patterns

### 6.1 Circuit Breaker
- Open: Fail fast, don't call failing service
- Closed: Normal operation
- Half-open: Test if service recovered

### 6.2 Retry with Backoff
- Exponential backoff: 1s, 2s, 4s, 8s...
- Jitter: Randomize to avoid thundering herd
- Max retries: 3-5 attempts
- Idempotency keys for safety

### 6.3 Timeout Strategy
- Connection timeout: 5-10s
- Request timeout: 30-60s
- Client timeout > server timeout
- Graceful degradation on timeout

### 6.4 Bulkhead Pattern
- Isolate resources per client/endpoint
- Prevent cascade failures
- Separate thread pools
- Resource quotas

---

## 7. Anti-Patterns

- **Session state in server memory:** Breaks scalability
- **Chatty APIs:** Multiple calls for one use case
- **GET for mutations:** Violates HTTP semantics
- **200 for errors:** Use proper status codes
- **No versioning:** Breaking changes hurt clients
- **Exposing internal IDs:** Leak implementation details
- **No rate limiting:** Abuse and DoS vulnerability
- **Synchronous dependency chains:** Cascading latency
- **No timeouts:** Hung requests consume resources

---

## Links

- `embedded/methodology/ARCHITECTURE.md` - CTO-level architecture doctrine
- `embedded/architecture/SECURITY.md` - Security architecture
- `embedded/architecture/CACHING.md` - HTTP caching
- `embedded/architecture/FRONTEND.md` - Frontend architecture
- `embedded/architecture/CLOUD.md` - Cloud deployment

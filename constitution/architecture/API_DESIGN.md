# API_DESIGN.md - API Design Standards

**Authority:** guidance (API design conventions)
**Layer:** Architecture
**Binding:** No
**Scope:** REST, GraphQL, and RPC API design standards

---

## 1. API Design Principles

### REST APIs
- **Resource-oriented:** URLs represent resources, not actions
- **CRUD mapping:**
  - `GET /resources` - List
  - `GET /resources/:id` - Get one
  - `POST /resources` - Create
  - `PUT /resources/:id` - Update
  - `DELETE /resources/:id` - Delete
- **Stateless:** Every request contains all context
- **Consistent naming:** snake_case for URLs, camelCase for JSON

### GraphQL
- **Schema-first:** Define schema before implementation
- **Nested relationships:** Leverage graph structure
- **Pagination:** Cursor-based for large datasets
- **N+1 prevention:** Use DataLoader patterns

### RPC
- **Protocol buffers:** Use for type safety
- **Versioning:** Major version in path
- **Streaming:** Use for real-time data

---

## 2. Request/Response Standards

### Request Headers
```
Content-Type: application/json
Accept: application/json
Authorization: Bearer <token>
```

### Response Envelope
```json
{
  "data": { },
  "meta": {
    "request_id": "uuid",
    "timestamp": "ISO8601"
  },
  "errors": []
}
```

### Error Response
```json
{
  "error": {
    "code": "RESOURCE_NOT_FOUND",
    "message": "Human readable message",
    "details": {}
  }
}
```

---

## 3. Versioning Strategy

- **URL versioning:** `/v1/`, `/v2/`
- **Deprecation policy:** 2 versions maintained, 6 month sunset
- **Breaking changes:** Only in major version bumps

---

## 4. Authentication & Authorization

- **Authentication:** Bearer tokens (JWT preferred)
- **Authorization:** Scope-based permissions
- **Rate limiting:** Standardized headers (`X-RateLimit-*`)

---

## 5. Documentation

All APIs must have:
- OpenAPI/Swagger spec
- Example requests/responses
- Error code reference
- Authentication requirements

---

## 6. Agent Guidelines

When agents design APIs:
1. Follow existing patterns in the codebase
2. Document all endpoints
3. Include OpenAPI specs in PR
4. Add integration tests for critical paths

# DATA.md - Data Architecture

**Authority:** guidance (data storage, modeling, and governance patterns)
**Layer:** Guides
**Binding:** No
**Scope:** data architecture principles, storage selection, and data governance
**Non-goals:** specific database implementations, one-size-fits-all solutions

---

## 1. Data Architecture Principles

### 1.1 Data Longevity
**Data outlives code by orders of magnitude.** Design for data that will survive:
- Multiple code rewrites
- Technology stack changes
- Team turnover
- Business pivots

### 1.2 Schema as Contract
Schema is the interface between data producers and consumers:
- Schema changes are migrations, not patches
- Backward compatibility is required unless explicitly coordinated
- Schema versioning enables gradual evolution
- Documentation is part of the schema

### 1.3 Data Ownership
Every data entity has a single owner:
- Owner defines schema and access patterns
- Owner manages lifecycle (retention, archival)
- Owner handles migrations
- Other services access through defined interfaces

---

## 2. Storage Selection Framework

### 2.1 Decision Matrix

| Use Case | Primary Choice | When to Consider Alternatives |
|----------|---------------|------------------------------|
| Transactional (ACID) | PostgreSQL | Scale > 10TB or extreme write throughput |
| Document (flexible schema) | MongoDB | Need complex transactions |
| Key-Value (caching/session) | Redis | Need persistence guarantees |
| Time-series (metrics/logs) | TimescaleDB/InfluxDB | Small scale (< 1M points/day) |
| Graph (relationships) | Neo4j | Relationships fit in relational model |
| Search (full-text) | Elasticsearch | Simple search fits in Postgres |
| Blob (files/images) | S3 | Need filesystem semantics |
| Queue (async work) | Kafka/RabbitMQ | Simple queues fit in Redis |

### 2.2 Multi-Model Considerations

**When one database isn't enough:**
- Primary database for transactions
- Elasticsearch for search
- Redis for caching
- S3 for blobs
- Kafka for events

**Consistency challenges:**
- Eventual consistency between stores
- Saga pattern for distributed transactions
- Outbox pattern for reliable publishing

---

## 3. Data Modeling Patterns

### 3.1 Relational Modeling
- **Normalization:** 3NF for OLTP, denormalized for OLAP
- **Indexes:** Query-driven, measure impact on writes
- **Partitioning:** Time-based or hash-based for scale
- **Foreign Keys:** Use for data integrity, not navigation

### 3.2 Document Modeling
- **Embedding:** One-to-few relationships, access together
- **Referencing:** One-to-many, many-to-many, independent lifecycle
- **Array containment:** Tags, categories, permissions
- **Schema validation:** Enforce structure at database level

### 3.3 Event Sourcing
- **When to use:** Audit requirements, temporal queries, undo/redo
- **When to avoid:** Simple CRUD, reporting-heavy workloads
- **Snapshots:** Required for performance at scale
- **CQRS:** Separate read models for query optimization

---

## 4. Data Governance

### 4.1 Data Classification
- **Public:** No restrictions
- **Internal:** Company use only
- **Confidential:** Restricted access, encryption required
- **Restricted:** Compliance requirements (PII, PHI, PCI)

### 4.2 Data Retention
- Define retention policies by data type
- Automated archival to cold storage
- Right to deletion (GDPR/CCPA compliance)
- Backup retention separate from data retention

### 4.3 Data Quality
- Schema validation at ingestion
- Data lineage tracking
- Anomaly detection for critical datasets
- Regular data quality audits

---

## 5. Migration Strategies

### 5.1 Types of Migrations
- **Schema migrations:** Add/remove columns, indexes
- **Data migrations:** Transform existing data
- **System migrations:** Move between databases

### 5.2 Zero-Downtime Migrations
1. Dual-write to old and new schema
2. Backfill historical data
3. Verify consistency
4. Switch reads to new schema
5. Stop writes to old schema
6. Remove old schema

### 5.3 Rollback Planning
- Every migration must have rollback procedure
- Test rollback in staging
- Keep backward compatibility during transition
- Monitor for data corruption post-migration

---

## 6. Integration Patterns

### 6.1 Database per Service
- Each service owns its data
- No shared database between services
- Services communicate via APIs or events
- Enables independent scaling and deployment

### 6.2 Shared Database (Anti-Pattern)
- **Problems:** Coupling, schema conflicts, scaling limits
- **When acceptable:** Monolith transitioning to microservices
- **Migration path:** Strangler fig pattern

### 6.3 API Composition
- Aggregate data from multiple services
- BFF (Backend for Frontend) pattern
- GraphQL for flexible querying
- Circuit breakers for resilience

---

## 7. Performance & Scaling

### 7.1 Read Scaling
- Read replicas for query offload
- Materialized views for complex queries
- Caching layers (see CACHING.md)
- CQRS for read optimization

### 7.2 Write Scaling
- Sharding by tenant or time
- Async processing for heavy writes
- Batch operations
- Queue-based ingestion

### 7.3 Connection Management
- Connection pooling mandatory
- Circuit breakers for DB failures
- Retry with exponential backoff
- Timeout configuration per query type

---

## 8. Security & Compliance

### 8.1 Encryption
- **At rest:** Database-level encryption
- **In transit:** TLS for all connections
- **In use:** Application-level for sensitive fields
- **Key management:** KMS or Vault, never in code

### 8.2 Access Control
- Principle of least privilege
- Database roles per service
- Audit logging for sensitive access
- Regular access reviews

### 8.3 Compliance
- GDPR: Right to erasure, data portability
- CCPA: Consumer data rights
- HIPAA: Healthcare data protection
- PCI-DSS: Payment card data

---

## 9. Anti-Patterns

- **SELECT ***: Specify columns explicitly
- **N+1 queries:** Use joins or batching
- **No indexes:** Every query needs index strategy
- **No connection limits:** Resource exhaustion risk
- **Storing files in database:** Use blob storage
- **No backups:** Assume data loss will happen
- **Hard deletes:** Soft delete for audit trail
- **No data validation:** Validate at every boundary

---

## Links

- `methodology/ARCHITECTURE.md` - binding architecture doctrine
- `architecture/CACHING.md` - Caching patterns
- `architecture/SECURITY.md` - Security architecture
- `architecture/OBSERVABILITY.md` - Data observability

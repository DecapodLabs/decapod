# ARCHITECTURE.md - Chief Technology Officer's Architecture Doctrine

**Authority:** binding (CTO-level architectural principles and technology strategy)
**Layer:** Guides
**Binding:** No ⚠️ BUT STILL REQUIRED READING
**Scope:** high-level technology strategy, cross-cutting concerns, and architectural governance
**Non-goals:** implementation details (see embedded/architecture/*), project-specific designs, subsystem registries

⚠️ **THIS IS THE CTO'S ARCHITECTURAL VISION. REQUIRED READING FOR ALL TECHNICAL LEADERS.** ⚠️

This document establishes the technology strategy, architectural principles, and cross-cutting concerns that guide all technical decisions. It defines *what kind of architecture we build*, not *how to build specific systems*.

Think of this as the CTO's mandate: the principles that every VP of Engineering, Principal Engineer, and Architect must uphold.

---

## 1. Technology Strategy: First Principles

### 1.1 The Fundamental Constraint

**Architecture must serve the business, not exist for its own sake.**

Every architectural decision is evaluated against:
- **Velocity:** Does it help us ship faster?
- **Reliability:** Does it reduce failure modes?
- **Scalability:** Does it handle 10x growth without redesign?
- **Maintainability:** Can new engineers understand it in 30 minutes?
- **Cost:** Is the total cost of ownership justified?

**If a decision doesn't improve at least two of these, it's architectural vanity.**

### 1.2 Architectural North Star

**"Simple things should be simple; complex things should be possible."**

- Default to boring technology
- Introduce complexity only when it solves a real problem
- Prefer explicit over implicit
- Favor composition over inheritance
- Optimize for readability and debuggability

---

## 2. Cross-Cutting Technology Domains

The CTO's architecture spans these critical domains. Each domain has dedicated principles and references detailed guidance in `embedded/architecture/*`.

### 2.1 Data Architecture (`embedded/architecture/DATA.md`)

**Principle:** Data is the only thing that persists. Code is temporary; data is forever.

**CTO Mandate:**
- Schema changes are migrations, not patches
- Data ownership must be clear (single source of truth)
- Query patterns drive storage decisions, not vice versa
- Backup and recovery are first-class concerns
- Data residency and compliance are non-negotiable

**Key Decisions:**
- SQL vs NoSQL vs Graph vs Time-series
- Event sourcing vs state storage
- Data mesh vs data lake vs data warehouse
- GDPR/CCPA compliance architecture

**Reference:** See `embedded/architecture/DATA.md` for data modeling, storage, and governance patterns.

---

### 2.2 Caching Architecture (`embedded/architecture/CACHING.md`)

**Principle:** Cache is a performance tool, not a consistency mechanism.

**CTO Mandate:**
- Cache invalidation is harder than caching; design for it first
- Never cache what you can't afford to lose
- Monitor cache hit rates; low hit rate = cache smell
- Distributed caches need distributed invalidation strategies
- Cache warming is production readiness, not optimization

**Key Decisions:**
- L1 (in-memory) vs L2 (Redis/Memcached) vs L3 (CDN)
- Cache-aside vs write-through vs write-behind
- TTL policies and cache coherence
- Cache stampede prevention

**Reference:** See `embedded/architecture/CACHING.md` for cache strategies and patterns.

---

### 2.3 Memory Architecture (`embedded/architecture/MEMORY.md`)

**Principle:** Memory is fast, finite, and fragile. Use it deliberately.

**CTO Mandate:**
- Understand memory hierarchy (L1/L2/L3/DRAM/disk)
- Memory leaks are architecture failures, not bugs
- Large heap = GC pressure = latency spikes
- Off-heap for large data; on-heap for hot paths
- Memory mapping for large files; streams for unbounded data

**Key Decisions:**
- Stack vs heap allocation strategies
- Object pooling vs allocation
- Memory-mapped files vs traditional I/O
- Off-heap storage (unsafe/direct buffers)

**Reference:** See `embedded/architecture/MEMORY.md` for memory management and optimization.

---

### 2.4 Web Architecture (`embedded/architecture/WEB.md`)

**Principle:** The web is stateless by default; state must be explicit.

**CTO Mandate:**
- REST is a convention, not a religion; use the right pattern for the job
- HTTP/2 and HTTP/3 are the baseline; HTTP/1.1 is legacy
- Stateless services scale horizontally; stateful services scale vertically
- Authentication is not authorization; separate concerns
- API versioning is a commitment; version carefully or not at all

**Key Decisions:**
- REST vs GraphQL vs gRPC vs WebSocket
- Stateful vs stateless session management
- JWT vs session cookies vs OAuth2 tokens
- API gateway vs direct service communication

**Reference:** See `embedded/architecture/WEB.md` for web patterns and API design.

---

### 2.5 Cloud Architecture (`embedded/architecture/CLOUD.md`)

**Principle:** Cloud is a capability multiplier, not a complexity generator.

**CTO Mandate:**
- Design for failure: every component will fail eventually
- Multi-region is for availability, not performance
- Serverless is for event-driven, not everything-driven
- Infrastructure as Code is non-negotiable
- Cost visibility is architecture observability

**Key Decisions:**
- IaaS vs PaaS vs SaaS vs FaaS
- Kubernetes vs container orchestration alternatives
- Multi-cloud vs single-cloud + DR
- Spot instances vs reserved capacity

**Reference:** See `embedded/architecture/CLOUD.md` for cloud patterns and infrastructure.

---

### 2.6 Frontend Architecture (`embedded/architecture/FRONTEND.md`)

**Principle:** Frontend is the user experience; performance is the experience.

**CTO Mandate:**
- Core Web Vitals are engineering requirements, not marketing metrics
- Bundle size budgets are architecture constraints
- Hydration is expensive; consider server components
- State management complexity grows exponentially; keep it minimal
- Accessibility is not a feature; it's a requirement

**Key Decisions:**
- SPA vs MPA vs islands architecture
- CSR vs SSR vs SSG vs ISR
- Component framework selection
- State management (Redux vs Zustand vs Context)

**Reference:** See `embedded/architecture/FRONTEND.md` for frontend patterns and performance.

---

### 2.7 Algorithms & Data Structures (`embedded/architecture/ALGORITHMS.md`)

**Principle:** The right algorithm makes impossible possible; the wrong one makes simple complex.

**CTO Mandate:**
- Big-O is the start, not the end; constants matter in practice
- Algorithm choice is an architecture decision, not an implementation detail
- NP-hard problems need approximation, not brute force
- Lock-free data structures for hot paths; locks for correctness
- Benchmark real workloads, not theoretical ones

**Key Decisions:**
- Time complexity vs space complexity tradeoffs
- Probabilistic data structures (Bloom filters, HyperLogLog)
- Concurrency algorithms (lock-free, wait-free)
- Streaming vs batch processing

**Reference:** See `embedded/architecture/ALGORITHMS.md` for algorithm selection and optimization.

---

### 2.8 Security Architecture (`embedded/architecture/SECURITY.md`)

**Principle:** Security is not a feature; it's a property of the system.

**CTO Mandate:**
- Defense in depth: no single point of security failure
- Principle of least privilege at every layer
- Secrets don't belong in code, configs, or logs
- Encryption in transit is baseline; encryption at rest is required
- Security audits are architecture reviews, not checkbox exercises

**Key Decisions:**
- Zero Trust architecture vs perimeter security
- mTLS vs API keys vs OAuth2
- Secret management (Vault, KMS, etc.)
- Encryption strategies (at-rest, in-transit, in-use)

**Reference:** See `embedded/architecture/SECURITY.md` and `embedded/specs/SECURITY.md` for security patterns.

---

### 2.9 Observability Architecture (`embedded/architecture/OBSERVABILITY.md`)

**Principle:** You can't operate what you can't observe.

**CTO Mandate:**
- Metrics for dashboards, logs for debugging, traces for understanding
- Alert on symptoms, not causes (but log causes for investigation)
- Structured logging is required; string parsing is prohibited
- Sampling is acceptable for high-volume data
- Cost of observability < cost of not observing

**Key Decisions:**
- Metrics (Prometheus vs CloudWatch vs Datadog)
- Logging (ELK vs Loki vs CloudWatch)
- Tracing (OpenTelemetry vs Jaeger)
- Profiling (continuous vs on-demand)

**Reference:** See `embedded/architecture/OBSERVABILITY.md` for observability patterns.

---

### 2.10 Concurrency & Parallelism (`embedded/architecture/CONCURRENCY.md`)

**Principle:** Concurrency is hard; parallelism is harder. Choose the right model.

**CTO Mandate:**
- Shared memory is fast but dangerous; message passing is slower but safer
- Actor model for distributed state; CSP for coordination
- Async/await is not free; understand the runtime cost
- Thread pools have limits; backpressure is required
- Deadlocks are architecture failures; race conditions are correctness failures

**Key Decisions:**
- Threads vs processes vs coroutines
- Locks vs lock-free vs STM
- Actor model (Akka, Orleans) vs CSP (Go channels)
- Async I/O models (epoll, io_uring)

**Reference:** See `embedded/architecture/CONCURRENCY.md` for concurrency patterns.

---

## 3. Architecture Governance

### 3.1 Architecture Review Board (ARB)

**When to escalate to CTO/ARB level:**
- Cross-domain architectural changes
- Technology stack changes
- Breaking API changes
- Security architecture changes
- Cost > $X/month infrastructure decisions

**ARB Process:**
1. Document decision context and constraints
2. Present options with tradeoffs
3. Define proof of concept success criteria
4. Make decision and record ADR
5. Set review date for irreversible decisions

### 3.2 Technology Radar

**Four zones for technology adoption:**
- **Adopt:** Proven, use by default
- **Trial:** Promising, use with caution
- **Assess:** Interesting, evaluate in POCs
- **Hold:** Problematic, avoid for new work

**CTO maintains the radar; architects advocate for movements.**

### 3.3 Architecture Decision Records (ADRs)

Every significant architectural decision needs an ADR:
- Context (why we needed to decide)
- Decision (what we decided)
- Consequences (trade-offs and implications)
- Status (proposed, accepted, deprecated, superseded)

**See `methodology/DECISIONS.md` for ADR format and process.**

---

## 4. Quality Standards

### 4.1 The Definition of Done (Architecture)

An architecture change is complete when:
- ✅ ADR accepted and published
- ✅ Security review completed
- ✅ Cost analysis reviewed
- ✅ Operational runbook updated
- ✅ Monitoring and alerting configured
- ✅ Rollback plan documented
- ✅ Team trained on new patterns
- ✅ Deprecation plan (if replacing something)

### 4.2 Architecture Fitness Functions

Automated tests that verify architectural constraints:
- No circular dependencies
- API compatibility checks
- Security vulnerability scans
- Performance regression tests
- Cost anomaly detection
- Compliance policy checks

**These run in CI; violations block merge.**

---

## 5. The Architecture Practice

### 5.1 Role of the Architect

The architect is:
- **Technical leader:** Sets direction and mentors
- **Decision maker:** Owns architectural decisions
- **Communicator:** Explains trade-offs to all stakeholders
- **Governor:** Enforces standards and reviews
- **Learner:** Stays current with technology trends

**Not:** The smartest coder, the only decision maker, or removed from implementation.

### 5.2 Architecture Update Protocol

When changing architecture:
1. **Impact Assessment:** What breaks? What's the blast radius?
2. **Stakeholder Review:** Who needs to approve this?
3. **Proof of Concept:** Does this actually work?
4. **Migration Plan:** How do we get from here to there?
5. **Decision Record:** Document the irreversible choices
6. **Communication:** Tell everyone who needs to know
7. **Implementation:** Execute the plan
8. **Validation:** Verify the architecture works as designed

**SKIPPING STEPS = ARCHITECTURAL DEBT.**

### 5.3 Drift Detection & Recovery

Architecture drifts when:
- Implementation contradicts design
- Shortcuts bypass standards
- "Temporary" solutions become permanent
- Documentation becomes fiction

**Recovery Protocol:**
1. Acknowledge the drift (don't hide it)
2. Assess impact (can we live with it?)
3. Decide: fix architecture, fix implementation, or document exception
4. Record decision in ADR
5. Prevent future drift with fitness functions

---

## 6. Technology Domains Reference

| Domain | File | Owner Concern |
|--------|------|---------------|
| Data | `embedded/architecture/DATA.md` | Persistence, schema, governance |
| Caching | `embedded/architecture/CACHING.md` | Performance, invalidation |
| Memory | `embedded/architecture/MEMORY.md` | Resource optimization |
| Web | `embedded/architecture/WEB.md` | APIs, protocols, statelessness |
| Cloud | `embedded/architecture/CLOUD.md` | Infrastructure, scaling, cost |
| Frontend | `embedded/architecture/FRONTEND.md` | UX, performance, accessibility |
| Algorithms | `embedded/architecture/ALGORITHMS.md` | Efficiency, complexity |
| Security | `embedded/architecture/SECURITY.md` | Threats, defense, compliance |
| Observability | `embedded/architecture/OBSERVABILITY.md` | Monitoring, debugging, tracing |
| Concurrency | `embedded/architecture/CONCURRENCY.md` | Parallelism, coordination |

**Each domain doc contains:**
- Domain-specific principles
- Common patterns and anti-patterns
- Technology selection criteria
- Decision frameworks
- Integration with other domains

---

## 7. CTO's Architectural Principles (The Immutable)

1. **Simplicity over cleverness.** If you can't explain it to a junior engineer, it's too complex.

2. **Explicit over implicit.** Magic creates debugging nightmares.

3. **Boring over novel.** Proven technology beats cutting-edge unless cutting-edge solves a real problem.

4. **Correctness over performance.** Make it right, then make it fast.

5. **Observability over assumptions.** Measure everything; trust nothing.

6. **Failure is inevitable.** Design for graceful degradation, not perfection.

7. **Security is not optional.** Every layer must defend itself.

8. **Data outlives code.** Respect the data; it will outlast your code by years.

9. **Teams build systems.** Optimize for team productivity, not individual heroics.

10. **Architecture is a social contract.** It's only real when everyone understands and follows it.

---

## Links

- `embedded/specs/INTENT.md` - System intent and binding contracts
- `embedded/specs/SYSTEM.md` - System definition
- `embedded/specs/SECURITY.md` - Security doctrine
- `embedded/architecture/DATA.md` - Data architecture
- `embedded/architecture/CACHING.md` - Caching patterns
- `embedded/architecture/MEMORY.md` - Memory management
- `embedded/architecture/WEB.md` - Web architecture
- `embedded/architecture/CLOUD.md` - Cloud patterns
- `embedded/architecture/FRONTEND.md` - Frontend architecture
- `embedded/architecture/ALGORITHMS.md` - Algorithms & data structures
- `embedded/architecture/SECURITY.md` - Security architecture
- `embedded/architecture/OBSERVABILITY.md` - Observability
- `embedded/architecture/CONCURRENCY.md` - Concurrency patterns
- `embedded/methodology/SOUL.md` - Agent identity
- `embedded/methodology/TESTS.md` - Testing methodology (extracted)
- `embedded/core/GAPS.md` - Gap analysis methodology

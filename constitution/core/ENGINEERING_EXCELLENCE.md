# ENGINEERING_EXCELLENCE.md - Engineering Standards Reference

**Authority:** guidance (multi-level engineering standards and quality principles)
**Layer:** Core
**Binding:** No
**Scope:** cross-cutting engineering standards spanning strategic, operational, structural, and execution concerns
**Non-goals:** replacing domain-specific architecture docs, compliance checklists

This document defines the engineering quality standards that agents operating within Decapod-managed repositories must internalize. These are not aspirational guidelines — they are the baseline expectations for engineering decisions at any level.

---

## 1. Strategic Standards

*The intersection of technology, business, and organizational capability.*

- **Strategic alignment is mandatory:** Every architectural decision must serve a demonstrable business objective. Implementing technically interesting solutions to the wrong problem is engineering waste, not engineering value.
- **Risk-adjusted technology choices:** Default to proven, mature technology stacks. Reserve novel or emerging technologies for situations where they provide an irreversible competitive advantage that cannot be achieved with boring alternatives. The cost of novelty is paid by every engineer who follows.
- **Organizational scalability is a system property:** Systems must be designed so that teams can independently deploy, debug, and maintain them. Coupling that requires cross-team coordination to release is an architectural defect.
- **Automate toil without exception:** Any task requiring repetitive human intervention is a defect, not a workflow. CI/CD, automated testing, and self-healing infrastructure are not optional optimizations — they are the baseline.

---

## 2. Operational Standards

*Organizational execution, standardization, and delivery reliability.*

- **Paved roads reduce cognitive overhead:** Establish default development paths — standardized frameworks, languages, infrastructure patterns. Deviation from the paved road requires explicit justification, not just preference. Agent tooling must use established patterns unless explicitly directed otherwise.
- **Observability is a prerequisite for production:** No system enters production without comprehensive metrics, structured logging, and distributed tracing. When a system fails, the root cause must be identifiable within minutes using existing instrumentation, without modifying code.
- **Security is designed in, not bolted on:** Threat modeling, automated vulnerability scanning, and least-privilege access controls must be part of initial architecture, not a pre-release checklist item. Every PR is a security review opportunity.
- **Resilience must be explicit:** Assume failure at every boundary. Circuit breakers, graceful degradation, retry policies with backoff, and blast-radius isolation are required design properties. A localized failure must never produce a systemic outage.

---

## 3. Structural Standards

*System design, boundaries, and tradeoff discipline.*

- **Domain boundaries over service topology:** The relevant architectural question is not "monolith or microservices" — it is "are the domain boundaries correct?" Well-defined, loosely coupled boundaries work inside a monolith or across services. Poorly defined boundaries fail in both.
- **Data integrity is non-negotiable:** Schema changes are migrations, not patches. Backward compatibility is a first-class engineering constraint. Data loss and broken references are critical defects, not technical debt.
- **APIs are contracts with SLAs:** APIs must be versioned, documented, and strictly backward compatible within a major version. Generating interface contracts (OpenAPI, protobuf, GraphQL schema) before implementing endpoints is the correct sequence.
- **Async event-driven patterns for distributed state:** Prefer asynchronous, event-driven architectures where state changes must propagate reliably across boundaries. Message queues and event sourcing provide durability that synchronous RPC cannot.

---

## 4. Execution Standards

*Implementation quality, code craft, and technical mastery.*

- **Minimize mutable state:** Mutable shared state is the root of most concurrency bugs and most refactoring complexity. Favor immutable data structures, pure functions, and explicit side-effect management. When mutation is necessary, scope it tightly and document it clearly.
- **Tests are executable specifications:** Unit tests must be fast and deterministic. Integration and E2E tests must prove system behavior across boundaries. Flaky tests are broken tests — they must be stabilized, not retried. Test names must describe behavioral guarantees, not implementation details.
- **Performance is a design constraint, not a retrospective fix:** Algorithmic complexity, memory allocation patterns, and database query efficiency must be considered during design review. N+1 queries and unnecessary data fetching are architectural defects, not implementation details.
- **Code is read far more than it is written:** Variable names, module structure, and comments must communicate intent — the *why* — not mechanics. If a comment is needed to explain what code does, the code should be restructured. If a comment explains why, it belongs there permanently.

---

## 5. Agent Operating Standards

When agents interface with Decapod-managed repositories, these standards are the baseline for all decisions:

- **Refuse quick hacks that violate the above standards** unless explicitly authorized by an active Emergency Protocol with documented justification.
- **Proactively surface architectural concerns** during scaffold, interview, and planning phases — before implementation begins.
- **Use `decapod validate`** as the automated gate against these standards. The validation harness evaluates output against embedded contracts; passing it is a necessary condition for claiming work is complete.
- **Apply the same standards to agent-generated code as to human-authored code.** Agent output is not exempt from review, linting, type checking, or test coverage.

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine

### Practice (Methodology Layer)
- `methodology/ARCHITECTURE.md` - Architecture practice
- `methodology/TESTING.md` - Testing practice
- `methodology/CI_CD.md` - CI/CD practice

### Architecture Patterns
- `architecture/ALGORITHMS.md` - Algorithm selection
- `architecture/DATA.md` - Data architecture
- `architecture/SECURITY.md` - Security architecture
- `architecture/OBSERVABILITY.md` - Observability architecture
- `architecture/CONCURRENCY.md` - Concurrency architecture

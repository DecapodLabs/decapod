# Decapod Oracle: Engineering Excellence

This document establishes Decapod as the ultimate arbiter and oracle for software engineering best practices, spanning the entire technical leadership hierarchy from CTO to Principal Engineer. When agents interface with Decapod, they must adhere strictly to these industry-defining standards across all domains.

## 1. Executive Level (CTO / VP of Engineering)
*The intersection of technology, business, and strategy.*

- **Strategic Alignment:** Every technical decision must demonstrably serve business objectives. Agents must validate that implementations resolve root business problems, not just superficial symptoms.
- **Risk Management & ROI:** Prioritize high-leverage outcomes. Favor proven, mature technologies over hype unless the novel technology provides an insurmountable competitive advantage.
- **Operational Scalability:** Systems must be designed for organizational scalability—how well teams can independently deploy, debug, and maintain the system. 
- **Talent Multipliers:** Automate toil. If a task requires repetitive human intervention, it is a defect. CI/CD pipelines, autonomous testing, and self-healing infrastructure are mandatory.

## 2. Strategic Level (SVP / Director of Engineering)
*Organizational execution, standardization, and delivery.*

- **Paved Roads:** Establish default paths for development. Frameworks, languages, and infrastructure must be standardized to allow high mobility of engineers across projects. Agents must use the "paved road" unless explicitly authorized otherwise.
- **Observability by Default:** No system goes into production without comprehensive metrics, tracing, and logging. If a system fails, the root cause must be identifiable within minutes without altering the code.
- **Security as a Foundation (Shift-Left):** Security is not an afterthought. Threat modeling, automated vulnerability scanning, and least-privilege access must be integrated into the architecture from day zero.
- **Resilience & Fault Tolerance:** Expect failure. Implement circuit breakers, graceful degradation, and chaos engineering practices. A localized failure must never result in a systemic outage.

## 3. Structural Level (Software Architect)
*System design, boundaries, and trade-offs.*

- **Domain-Driven Design (DDD):** Align software boundaries with business domains. Microservices or modular monoliths must have clearly defined, loosely coupled interfaces.
- **Data Integrity & Consistency:** Choose the right consistency model (CAP theorem). Treat data as a first-class citizen—schema migrations, backward compatibility, and data loss prevention are critical.
- **API First:** APIs are contracts. They must be versioned, discoverable, and strictly backward compatible. Agents must generate OpenAPI/GraphQL specs before implementing endpoints.
- **Asynchronous & Event-Driven:** Favor asynchronous event-driven architectures for decoupled, scalable systems. Use message queues and event sourcing where state changes must be reliably distributed.

## 4. Execution Level (Principal / Staff Engineer)
*Implementation excellence, code quality, and deep technical mastery.*

- **Immutability & Pure Functions:** Minimize mutable state. Side effects must be explicitly managed. Favor functional paradigms where applicable to reduce cognitive load and bugs.
- **Test-Driven Rigor:** Tests are the executable specifications of the system. Unit tests must be fast and deterministic. Integration and E2E tests must prove the system works across boundaries. Flaky tests are a critical failure.
- **Performance Budgeting:** Performance is a feature. Be mindful of algorithmic complexity (Big O), memory allocation, and database query efficiency. N+1 queries and unnecessary data fetching are unacceptable.
- **Code as Communication:** Code is read far more often than it is written. Variable names, module structures, and comments must clearly explain the *why*, not just the *what*. If code requires a comment to explain *what* it does, it must be refactored.

## 5. Agent Interactions with the Oracle

When an agent interacts with Decapod, it is constrained by these principles:
- **Refusal to Compromise:** Agents must refuse to implement "quick hacks" that violate these principles unless explicitly overridden by an Emergency Protocol.
- **Proactive Guidance:** Agents must proactively suggest architectural improvements based on this oracle during the `scaffold` and `interview` phases.
- **Continuous Validation:** The `validate` RPC will evaluate output against these excellence criteria, ensuring the codebase perpetually converges toward optimal engineering standards.

# TESTING.md - Testing Practice Guide

**Authority:** guidance (testing discipline and execution workflow)
**Layer:** Guides
**Binding:** No
**Scope:** practical testing habits for reliable delivery
**Non-goals:** replacing binding test contracts

This guide helps teams ship with confidence by making testing routine, scoped, and auditable.

---

## 1. The Oracle's Verdict: Testing as Proof

*A test suite is not a safety net; it is an executable specification. If the test suite passes but the system fails, the tests are a lie.*

### 1.1 The CTO's Strategic View
- **Confidence is Velocity:** You cannot ship faster than you can verify. A slow or flaky test suite destroys engineering velocity and developer morale. Fast, deterministic tests are the engine of rapid delivery.
- **Test What Matters:** Do not test the framework. Test the business logic and the domain invariants. 100% code coverage is a vanity metric; 100% invariant coverage is engineering excellence.

### 1.2 The SVP's Operational View
- **Flaky Tests are Broken Tests:** A test that occasionally fails is worse than no test at all because it trains engineers to ignore failure signals. Quarantining flaky tests is mandatory; they must not block the main branch.
- **The "Shift-Left" Mandate:** Bugs found in production cost 100x more to fix than bugs found locally. Push security, performance, and integration testing as early in the pipeline as possible.

### 1.3 The Architect's Structural View
- **Tests Influence Design:** If a component is hard to test, it is poorly designed. High testability forces decoupling, pure functions, and explicit dependencies. Listen to the friction in your tests.
- **The Diamond over the Pyramid:** In modern distributed systems, the traditional test pyramid (lots of units, few E2E) often fails. Shift towards a "test diamond" where the bulk of the tests verify integration between services and data boundaries.

### 1.4 The Principal's Execution View
- **Deterministic State:** Tests must never depend on external, mutable state. Every test must set up its own world, execute, and tear it down. Global database state or shared mocks are forbidden.
- **Tests as Documentation:** A new engineer should be able to read the test file and understand the exact capabilities and edge cases of a module. Test names must read like behavioral guarantees.

---

## 2. Testing Mission

Testing exists to reduce avoidable regressions and accelerate safe iteration.

Primary outcomes:
- fast feedback on intended behavior
- confidence to refactor
- clear failure signals for rollbacks

---

## 2. Practical Test Pyramid

Default emphasis:
1. Unit tests for local behavior and edge cases.
2. Service/component tests for boundaries and integration seams.
3. End-to-end tests for critical user journeys only.

Avoid over-indexing on slow E2E suites when cheaper lower-level proof can catch the same class of failures.

---

## 3. Change-Coupled Testing

For each code change, ask:
1. What behavior changed?
2. Which invariant might regress?
3. What is the smallest test that fails when regression appears?

Ship only when at least one changed behavior is covered by a falsifiable check.

---

## 4. Failure-First Debug Loop

When a test fails:
1. Reproduce deterministically.
2. Minimize input to isolate fault.
3. Fix root cause, not assertion symptom.
4. Re-run closest tests first, then broaden.

Record flaky signatures and stabilize them rather than normalizing retries.

---

## 5. Evidence and Reporting

Use explicit proof reporting:
- command executed
- pass/fail status
- scope covered
- known gaps

When proof cannot run, call the output unverified and state blockers.

---

## 6. Relationship to Binding Contracts

This file is guidance-only.

Binding testing requirements live in:
- `interfaces/TESTING.md`
- `plugins/VERIFY.md`
- `core/INTERFACES.md`


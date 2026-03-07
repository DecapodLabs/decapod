# TESTING.md - Testing Practice Guide

**Authority:** guidance (testing discipline and execution workflow)
**Layer:** Guides
**Binding:** No
**Scope:** practical testing habits for reliable delivery
**Non-goals:** replacing binding test contracts

This guide helps teams ship with confidence by making testing routine, scoped, and auditable.

---

## 1. Testing Mission

Testing exists to reduce avoidable regressions and accelerate safe iteration.

Primary outcomes:
- fast feedback on intended behavior
- confidence to refactor
- clear failure signals for rollbacks

A test suite is not a safety net — it is an executable specification of what the system must do. The following principles define how to build one that is worth trusting:

- **Test velocity is delivery velocity:** You cannot ship faster than you can verify. A slow or flaky test suite directly limits how often code can be merged and deployed. Fast, deterministic tests are the engine of rapid delivery — not optional infrastructure.
- **Test invariants, not coverage:** 100% line coverage is a vanity metric. 100% invariant coverage — proving that every documented behavioral guarantee holds — is engineering excellence. Focus test effort on behavior that, if broken, would cause a failure in production.
- **Flaky tests are broken tests:** A test that occasionally fails is worse than no test. It trains engineers to dismiss failure signals. Flaky tests must be quarantined and stabilized on the same timeline as production bugs. They do not belong on the main branch.
- **Shift left on all failure modes:** A bug found in production costs two orders of magnitude more to fix than a bug found locally. Security, performance, and integration failures should be caught as early in the pipeline as possible — ideally before the PR is merged.
- **Hard-to-test code is poorly designed code:** If a component requires extensive mocking infrastructure to unit test, it has too many implicit dependencies. Testing friction is a design signal. Listen to it and decouple before adding the mocking scaffolding.
- **Integration coverage over unit volume:** In distributed and concurrent systems, the majority of real failures occur at boundaries — between services, between async components, between schema and code. The test suite should reflect where failures actually happen, not where they are easiest to write.
- **Tests must own their state:** No test may depend on external mutable state or the execution order of other tests. Every test sets up the state it needs, executes, and tears down cleanly. Shared database state and global mocks are defects in the test design.
- **Test names are behavioral specifications:** A new engineer reading a test file should understand what the component guarantees and what edge cases are explicitly handled. Test names that describe behavior (`returns_empty_list_when_store_is_uninitialized`) are documentation. Test names that describe implementation (`test_init_path_2`) are noise.

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

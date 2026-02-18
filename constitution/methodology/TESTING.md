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


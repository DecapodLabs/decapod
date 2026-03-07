# CI_CD.md - CI/CD Practice Guide

**Authority:** guidance (delivery automation and release hygiene)
**Layer:** Guides
**Binding:** No
**Scope:** practical CI/CD patterns for production-grade software delivery
**Non-goals:** replacing release contracts or environment-specific runbooks

This guide helps teams move from ad hoc shipping to repeatable, low-risk delivery.

---

## 1. The Oracle's Verdict: CI/CD as the Assembly Line

*If it is painful to deploy, you will deploy less often. If you deploy less often, every deployment becomes a crisis.*

### 1.1 The CTO's Strategic View
- **Deployment Frequency is a Business Metric:** The ability to push code to production 10 times a day is not a technical flex; it is a competitive advantage. It allows the business to test hypotheses faster than the competition.
- **The End of "Release Nights":** Releases must be boring, non-events that happen during normal business hours. If a release requires a war room, the CI/CD pipeline is fundamentally inadequate.

### 1.2 The SVP's Operational View
- **Continuous Integration is a Practice, Not a Tool:** CI is not "running Jenkins." CI is the practice of merging code to the main branch at least once a day. Long-lived feature branches are the enemy of integration.
- **Fail Closed, Rollback Fast:** If a deployment metric degrades, the pipeline must automatically halt the rollout and revert. Mean Time to Recovery (MTTR) is far more important than Mean Time Between Failures (MTBF).

### 1.3 The Architect's Structural View
- **Immutable Artifacts:** Build once, deploy anywhere. The exact same container image or binary tested in staging must be the one deployed to production. Environment-specific builds are a catastrophic anti-pattern.
- **Separation of Deployment and Release:** Deploying code to a server and releasing a feature to users are two different concepts. Use feature flags to decouple them.

### 1.4 The Principal's Execution View
- **Pipeline as Code:** The CI/CD configuration must live in the repository next to the application code. It must be versioned, reviewed, and testable.
- **The "Broken Build" Rule:** If the main branch build is broken, all feature work stops. Fixing the build is the highest priority for the entire engineering team.

---

## 2. CI/CD Mission

CI/CD should make high-quality delivery the default path:
- every change is validated the same way
- release risk is visible before merge
- deployment outcomes are observable and reversible

---

## 2. CI Baseline (Per PR)

Minimum PR pipeline stages:
1. Build and static checks.
2. Test suites matched to changed surface.
3. Policy/security checks required by project standards.
4. Artifact/release metadata generation when applicable.

Pipelines should fail closed for required gates.

---

## 3. CD Baseline (Post-Merge)

Production-oriented deployment flow:
1. Promote immutable build artifacts.
2. Deploy with rollback-ready versioning.
3. Verify runtime health and critical paths.
4. Stop rollout when health or policy checks fail.

Prefer progressive rollout strategies for user-facing or stateful systems.

---

## 4. Branch and Release Hygiene

- Keep feature branches small and merge frequently.
- Use consistent commit semantics to support automated versioning/changelog tools.
- Never bypass required verification gates without explicit incident-level justification.

If automated release PRs are used, ensure trigger coverage for both push-driven and manual recovery paths.

---

## 5. Secrets and Environment Safety

- Store secrets in managed secret stores only.
- Scope credentials to minimum privileges.
- Separate build-time and runtime credentials.
- Rotate and audit access paths regularly.

Do not bake secrets into artifacts, logs, or test fixtures.

---

## 6. Relationship to Binding Contracts

This file is guidance-only.

Binding release and verification interfaces live in:
- `interfaces/CONTROL_PLANE.md`
- `interfaces/TESTING.md`
- `plugins/VERIFY.md`
- `specs/GIT.md`


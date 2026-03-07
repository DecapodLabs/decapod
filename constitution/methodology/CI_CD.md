# CI_CD.md - CI/CD Practice Guide

**Authority:** guidance (delivery automation and release hygiene)
**Layer:** Guides
**Binding:** No
**Scope:** practical CI/CD patterns for production-grade software delivery
**Non-goals:** replacing release contracts or environment-specific runbooks

This guide helps teams move from ad hoc shipping to repeatable, low-risk delivery.

---

## 1. CI/CD Mission

CI/CD should make high-quality delivery the default path:
- every change is validated the same way
- release risk is visible before merge
- deployment outcomes are observable and reversible

The pipeline is not infrastructure — it is engineering discipline made executable. The following principles define what that means in practice:

- **Deployment frequency is a competitive metric:** The ability to ship to production ten times a day is not a technical indulgence — it is the mechanism by which an organization tests hypotheses faster than competitors who deploy monthly. Infrequent deployment is infrequent feedback.
- **Releases must be boring non-events:** A release that requires a war room, a release manager, or an after-hours window is a release that will cause an incident. If shipping is painful, teams will ship less. If teams ship less, every deployment becomes higher-stakes. The pipeline's job is to make this cycle impossible.
- **CI is a practice, not a tool:** Continuous Integration means merging to the main branch at least once per day. Long-lived feature branches are the opposite of integration — they are divergence accumulation. The discipline of small, frequent merges is the practice; the tool enforces it.
- **Fail closed, recover fast:** When deployment metrics degrade, the pipeline must halt the rollout and revert automatically. Mean Time to Recovery is more operationally important than Mean Time Between Failures. Optimize for fast recovery, not for preventing every failure.
- **Build once, deploy everywhere:** The same artifact that passes staging must be the artifact deployed to production. Environment-specific builds destroy the value of staging. Immutable, hash-verified artifacts are the only trustworthy promotion mechanism.
- **Deployment and release are independent operations:** Deploying code to a server is a technical operation. Releasing a feature to users is a product operation. Feature flags decouple them, enabling dark launches, gradual rollouts, and instant kill switches without a full redeployment.
- **The pipeline is code:** CI/CD configuration must live in the repository, versioned alongside application code, subject to the same review process. Pipelines that exist only in a CI provider's UI are unversioned infrastructure.
- **A broken main branch stops all feature work:** When the main branch build fails, it is the highest-priority incident for the entire engineering team. Not because it is urgent in isolation, but because it blocks all downstream work. Fix it before anything else.

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

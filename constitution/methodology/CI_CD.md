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


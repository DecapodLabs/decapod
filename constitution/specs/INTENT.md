# INTENT.md - Intent-Driven Engineering Contract (General)

**Authority:** binding (general methodology contract; not project-specific)
**Layer:** Constitution
**Binding:** Yes ⚠️
**Scope:** intent-first flow, choice protocol, proof doctrine, drift recovery
**Non-goals:** project-specific requirements, control-plane interfaces, subsystem registries, or document routing

⚠️ **THIS IS A BINDING CONSTITUTIONAL CONTRACT. AGENTS MUST COMPLY.** ⚠️

This file is a general-purpose contract for how an agent should behave when operating in an intent-driven codebase.

It is intentionally not project-specific. Project-specific truth belongs in the repo's own manifest/requirements and is enforced by its proof surface.

---

## 1. Intent Is the API

**⚠️ FUNDAMENTAL LAW: Intent is a versioned contract that states what must be true. Everything downstream is derived.**

```
Intent → Architecture → Implementation → Proof → Promotion
```

If reality disagrees with intent, do NOT hand-wave. Either:

1. Update intent explicitly (and then recompile downstream artifacts).
2. Enter explicit drift recovery mode (time-boxed), then reestablish one-way flow.

**FAILURE TO FOLLOW THIS FLOW = UNVERIFIED, UNSAFE WORK.**

---

## 2. Authority and Conflict Resolution

When artifacts conflict, authority resolves it. The **mandatory ladder** in an intent-driven repo:

```
1. BINDING INTENT CONTRACT (this spec describes how to treat it) ← HIGHEST AUTHORITY
2. Architecture (compiled from intent)
3. Proof surface (tests, validate commands, proof notes)
4. Agent entrypoints (AGENTS/CLAUDE/etc)
5. Human workflow docs
6. Philosophy/context (must be explicitly marked non-binding if present)
```

**AGENTS: If the repo defines its own authority ladder, follow it, but require it to be explicit and stable.**

---

## 3. What "Working With Intent" Means (Agent Protocol) ⚠️ REQUIRED ⚠️

When asked to do work that changes behavior, state, or interfaces:

1. **Name the intent** in one sentence (what must be true when you are done).
2. **Identify the smallest proof surface** that can falsify success.
3. **If a change would alter the contract**, propose the contract change BEFORE touching code.
4. **Produce traceability**: connect the change to a promise/invariant/requirement in writing.

For non-trivial changes, use the **explicit change protocol**:

1. Intent delta (if needed).
2. Architecture delta.
3. Implementation delta.
4. Proof delta.
5. Validation run and report.

---

## 4. Choice Protocol (No Silent Defaults)

If a choice materially impacts build/run/ops/security/data semantics, it MUST be explicit.

**Material choices include:**

- language and runtime
- data store and schema strategy
- concurrency and process model
- secrets handling
- interface contracts (CLI/HTTP/event formats)
- portability and platform assumptions

If you inherit a default, you MUST say that you are inheriting it, and from where.

**SILENT DEFAULTS = VIOLATION OF THIS CONTRACT.**

---

## 5. Proof Is the Price of Promotion

Promotion means any claim that work is "ready", "verified", "compliant", or safe to merge/deploy.

**RULES:**

- If there is a proof surface, **RUN IT**.
- If you cannot run it, say "unverified" and state exactly what blocks verification.
- If proofs are missing, **your job is to create the smallest proof step** that collapses the uncertainty.

**UNVERIFIED PROMOTION = VIOLATION OF THIS CONTRACT.**

---

## 6. Traceability (Stable IDs)

Intent-driven work requires stable identifiers so artifacts can link without drift.

**Minimum expectations:**

- promise IDs are stable (P1, P2, ...) and never renumbered
- architecture references those IDs
- proofs reference those IDs (directly or via a mapping table)

If a repo uses a different stable ID scheme, **keep it stable and linkable.**

---

## 7. Drift: Detection and Recovery

Drift is any mismatch between:

- intent vs code
- architecture vs code
- proofs vs reality
- docs claiming capabilities that do not exist

**Recovery is allowed, but it MUST be explicit:**

1. Label recovery mode.
2. Update contracts to match reality (or roll reality back to match contracts).
3. Re-run proofs.
4. Exit recovery mode.

**UNDETECTED DRIFT = SYSTEM INVALID.**

---

## 8. Layer Boundaries (Methodology vs Interface vs Router)

This contract defines methodology only.

Interface semantics for agent<->CLI sequencing live in `interfaces/CONTROL_PLANE.md`.
Routing/navigation semantics live in `core/DECAPOD.md`.

If this file starts specifying command envelopes, store wiring, subsystem indexing, or routing policy, that content belongs elsewhere.

---

## 9. Changelog

- v0.0.2: Clarified layer boundaries by extracting control-plane interface and routing content out of this methodology contract.
- v0.0.1: A general agent-facing methodology contract (not project-specific), restoring the original intent-driven engineering emphasis: authority, one-way flow, choice protocol, proof gating, and drift recovery.

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `methodology/ARCHITECTURE.md` - Architecture practice
- `specs/SYSTEM.md` - System definition
- `methodology/SOUL.md` - Agent identity
- `interfaces/CONTROL_PLANE.md` - Sequencing patterns
- `interfaces/CLAIMS.md` - Promises ledger

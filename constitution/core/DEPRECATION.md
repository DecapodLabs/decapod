# DEPRECATION.md - Deprecation and Migration Contract

**Authority:** interface (how binding meaning is retired safely)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** marking deprecated material, required replacement pointers, and sunset rules
**Non-goals:** adding new requirements; this doc governs retirement/migration only

This contract prevents duplicate authority during transitions by making deprecation explicit, time-bounded, and migration-first.

---

## 1. Core Rule

Deprecated material is not binding.

If a binding document contains deprecated text, that text MUST be explicitly marked as deprecated and MUST include a replacement pointer and a sunset date. After the sunset date, it MUST be removed.

---

## 2. How To Deprecate (Required Fields)

To deprecate a doc, section, rule, or interface:

- Mark it `DEPRECATED` clearly at the point of use.
- Provide:
  - Replacement: link to the replacement canonical doc/section.
  - Sunset: a concrete date (YYYY-MM-DD).
  - Migration: short steps, or a pointer to a migration guide.
- Record an amendment: `specs/AMENDMENTS.md`.
- Update `interfaces/CLAIMS.md` if a claim is being retired or replaced.

---

## 3. Allowed Transitional State (No Duplicate Authority)

During a transition, both old and new text may exist only if:

- The old text is explicitly `DEPRECATED` and therefore non-binding.
- The new text is binding and canonical.
- The replacement pointer is unambiguous.

"Temporary" duplicated authority without a deprecation marker is forbidden.

---

## 4. Sunset Policy

- Sunset dates MUST be concrete (not "soon").
- Sunset dates SHOULD be short (days/weeks), not indefinite.
- After sunset:
  - Remove deprecated text from binding docs.
  - Remove deprecated interfaces from registries.
  - Remove or update claims in `interfaces/CLAIMS.md`.

---

## 5. Deprecation Registry (Optional, Recommended)

For large transitions, maintain a small registry table here:

| Deprecated Item | Replacement | Sunset | Notes |
|---|---|---|---|
| (none) |  |  |  |

---

## Links

### Core Router
- `core/DECAPOD.md` - **Router and navigation charter (START HERE)**

### Authority (Constitution Layer)
- `specs/INTENT.md` - **Methodology contract (READ FIRST)**
- `specs/SYSTEM.md` - System definition and authority doctrine
- `specs/AMENDMENTS.md` - Change control

### Registry (Core Indices)
- `core/PLUGINS.md` - Subsystem registry
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index
- `core/GAPS.md` - Gap analysis methodology

### Contracts (Interfaces Layer)
- `interfaces/DOC_RULES.md` - Doc compilation rules
- `interfaces/CLAIMS.md` - Promises ledger
- `interfaces/GLOSSARY.md` - Term definitions

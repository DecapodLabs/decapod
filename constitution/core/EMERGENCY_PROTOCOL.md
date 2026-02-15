# EMERGENCY_PROTOCOL.md - Core Stop-The-Line Contract

**Authority:** process (operational emergency handling)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** mandatory behavior when authority, store, or verification context is unclear
**Non-goals:** normal workflow guidance

When confusion creates risk, mutation stops immediately.

---

## 1. Stop Conditions

You MUST stop before mutating state if any are true:
1. You cannot identify the authoritative document for a decision.
2. You cannot identify which store a command will mutate.
3. You are unable to define the proof surface for the requested change.
4. Two binding documents appear to conflict.

---

## 2. Required Recovery Sequence

1. Halt all write operations.
2. Re-anchor router context via `core/DECAPOD.md`.
3. Re-check store semantics via `interfaces/STORE_MODEL.md`.
4. Run `decapod validate`.
5. Record a blocking TODO with the conflicting sources and intended mutation.

---

## 3. Escalation Record Requirements

A blocking record must include:
- conflicting files/sections
- store context (`user` or `repo`)
- command that was blocked
- unresolved decision needing human input

---

## 4. Exit Criteria

Resume work only when:
- authority conflict is resolved, and
- proof surface is defined, and
- validation is passing or an explicit blocker is documented.

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/DOC_RULES.md` - Decision rights
- `specs/INTENT.md` - Intent contract
- `specs/AMENDMENTS.md` - Change control

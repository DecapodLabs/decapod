# TODO_USER.md - User Checklist (Docs Only)

**Authority:** guidance (docs only; does not seed any store)
**Layer:** Guides
**Binding:** No
**Scope:** onboarding checklist
**Non-goals:** requirements or any kind of state seeding

This is a human onboarding checklist. It is not a database and does not seed any store.

When you install Decapod:
- Your user store is `~/.decapod`.
- It starts empty until you add tasks.

Suggested first tasks for a new project:

- Add/confirm `.decapod/constitution/specs/INTENT.md` exists and is accurate.
- Add/confirm `.decapod/constitution/specs/ARCHITECTURE.md` compiles from intent.
- Define your proof surface (`proof surface (decapod validate, tests, proof.md)` scripts, tests) and promotion gate.
- Add 1 TODO per intent promise that requires implementation.
- Add 1 TODO per proof obligation that is missing or deferred.

Optional workflow conventions (example):
- Tag tasks with `promise:P#`, `capability:<name>`, `proof:automated|manual`, and `intent:vX.Y.Z`.
- Set `--dir` to the project root so tasks are scoped correctly.

## Links

- `.decapod/constitution/specs/ARCHITECTURE.md`
- `.decapod/constitution/specs/INTENT.md`
- `.decapod/constitution/specs/SYSTEM.md`

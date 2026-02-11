# What changed (one sentence)
<!-- Be precise. Example: "Add GitHub Issues connector plugin that syncs TODO.md to issues with idempotent upserts." -->

## Why (what problem / what user outcome)
<!-- No ideology. Describe impact. -->

## Scope
- [ ] Core
- [ ] Plugin
- [ ] Docs
- [ ] CI / tooling

## How to test (required)
<!-- Paste exact commands + expected output. -->
Commands run:
- 
Expected result:
- 

## Risk / blast radius (required)
<!-- What could break, where, and how badly. -->
- Affected components:
- Backward compat:
- Failure modes:

## Evidence (required)
<!-- At least one: logs, screenshots, or trace output. Prefer logs. -->
- Logs / output:

---

# Plugin checklist (required if plugin touched)
- [ ] Plugin has a clear name and category (connector / adapter / cache / proof-eval / workflow).
- [ ] README/docs updated with purpose + configuration + example usage.
- [ ] Versioning / compat notes included (Decapod version, API surface used).
- [ ] Idempotency defined (what happens on re-run).
- [ ] Error handling defined (retries, backoff, hard-fail vs soft-fail).
- [ ] Permissions / secrets model stated (what it reads, what it writes, where secrets live).
- [ ] Proof surface or validation harness included (even minimal).
- [ ] Includes a minimal “smoke test” path (CI or documented local commands).

# Core checklist (required if core touched)
- [ ] Public interfaces documented (or explicitly unchanged).
- [ ] Added/updated tests covering the change.
- [ ] Failure behavior is deterministic (no silent partial success).
- [ ] Logging added/updated at the right level (info/warn/error) with actionable context.

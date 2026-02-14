# EMERGENCY_PROTOCOL.md - Stop-The-Line Procedure

**Authority:** guide (operational kill switch)
**Layer:** Guides
**Binding:** No
**Scope:** what to do when authority/store/truth-label semantics are unclear mid-task
**Non-goals:** adding requirements; this is an operational safety procedure only

If you are confused about authority, stores, or truth labels, treat that as a stop condition.

This guide exists to reduce "agent keeps going and damages shared state" failure mode.

---

## 1. Stop-The-Line Checklist

1. **Stop.** Do not mutate stores or rewrite binding docs while confused.
2. **Identify store context:**
   - Read `embedded/interfaces/STORE_MODEL.md`.
   - If you cannot say "I am operating on user store vs repo store" in one sentence, stop.
3. **Run proof surface:**
   - Run `decapod validate` for relevant store(s).
4. **Re-anchor authority:**
   - Start at `embedded/core/DECAPOD.md`.
   - For decision rights, consult `embedded/interfaces/DOC_RULES.md` (Decision Rights Matrix).
   - For subsystem existence and truth labels, consult `embedded/core/PLUGINS.md`.
5. **If contradiction exists:**
   - Treat it as invalid state.
   - Do not interpret; route to amendment: `embedded/specs/AMENDMENTS.md`.
6. **Escalate with a durable record:**
   - Create a TODO item with tag `BLOCKED:AUTHORITY` describing:
     - conflicting docs/sections
     - store you would have mutated
     - what proof surface was run

---

## 2. What Counts As "Confused"

Any of:

- You cannot identify the authoritative owner doc for a decision.
- You cannot identify which store will be mutated by the next command.
- You are about to claim something is verified without a proof surface.
- Two canonical binding docs appear to disagree.

---

## 3. Emergency Recovery Commands

When stopped, use these commands to re-establish context:

```bash
# Re-establish constitutional context
decapod docs ingest
decapod validate

# Check for drift
decapod proof run

# Record the stop condition
decapod todo create "BLOCKED:AUTHORITY - [specific confusion]"
```

---

## 4. Prevention

To avoid emergency stops:

1. **Always read before acting:** Follow the ladder in DECAPOD.md
2. **Validate assumptions:** Run `decapod validate` before making changes
3. **Check authority:** Use `embedded/core/DECAPOD.md` as your router for all decisions
4. **Document uncertainty:** When uncertain, ask questions before proceeding

---

## Links

- `embedded/core/DECAPOD.md` - Router and navigation charter
- `embedded/core/PLUGINS.md` - Subsystem registry
- `embedded/interfaces/DOC_RULES.md` - Doc compilation rules
- `embedded/interfaces/STORE_MODEL.md` - Store semantics
- `embedded/interfaces/CLAIMS.md` - Promises ledger
- `embedded/specs/AMENDMENTS.md` - Change control
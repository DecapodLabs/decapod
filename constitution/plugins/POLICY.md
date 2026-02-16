# POLICY.md - POLICY Subsystem (Embedded)

**Authority:** subsystem (REAL)
**Layer:** Operational
**Binding:** No

This document defines the policy subsystem.

## CLI Surface
- `decapod govern policy ...`

## Human-In-The-Loop (HITL) Overrides

Policy enforcement can read project overrides from `.decapod/OVERRIDE.md` under `### plugins/POLICY.md`.

Supported override directives:
- `HITL: I don't want human in the loop`
- `HITL_DISABLE scope=<scope>`
- `HITL_DISABLE min_risk=<level> max_risk=<level>`
- `HITL_DISABLE scope=<scope> min_risk=<level> max_risk=<level>`
- `HITL_ENABLE ...` (narrow re-enable after broad disable)

Matching behavior:
- Most-specific rule wins.
- If specificity ties, the latest rule wins.
- Scope values are exact string matches.
- Risk levels are `low|medium|high|critical`.

# FRONTEND_BACKEND_E2E.md - Frontend/Backend E2E Governance

**Authority:** spec (engineering execution contract)
**Layer:** Specs
**Binding:** Yes

## Scope

Govern agent-built frontend/backend flows where timing, DOM state, third-party services, and asynchronous behavior are variable.

## Modeling Rules

1. Each E2E task MUST be represented in an EVAL_PLAN task set.
2. Each execution attempt MUST be recorded as EVAL_RUN.
3. Completion claims for non-deterministic flows MUST be judged and aggregated before promotion.

## Required Artifacts

Promotion-relevant E2E evaluation requires:
1. `EVAL_PLAN` - reproducible settings + seeds + environment capture.
2. `EVAL_RUN` - per-attempt metadata + status + timing + optional cost.
3. `TRACE_BUNDLE` - event timeline and optional attachment pointers.
4. `EVAL_VERDICT` - strict judge JSON verdict.
5. `EVAL_AGGREGATE` - CI, deltas, and regression flags.
6. `FAILURE_BUCKETS` - actionable grouped failure reasons.

## Trace Discipline

1. Trace bundles MUST include event timeline sufficient for replay/debug.
2. Attachments (screenshots/video/har) are optional and referenced by content address.
3. External observability sinks are optional adapters; canonical truth is repo store artifacts.

## Selector/Timeout Discipline

1. Selector/DOM fragility MUST be treated as measurable failure mode, not ignored noise.
2. Timeout outcomes MUST be explicit failures with reason codes.
3. Failure buckets MUST include selector drift and timeout classes when observed.

## Promotion Rules

1. No promotion if minimum run count is not met.
2. No promotion if judge timeout failures are present.
3. No promotion if regression gate fails by statistical rule.
4. No promotion from stochastic failure buckets unless consensus policy is explicitly defined.


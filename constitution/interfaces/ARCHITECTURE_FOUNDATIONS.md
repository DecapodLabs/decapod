# ARCHITECTURE_FOUNDATIONS.md - Industry-Grade Engineering Foundations

**Authority:** interface (binding architecture delivery primitives)  
**Layer:** Interfaces  
**Binding:** Yes  
**Scope:** baseline architecture quality gates that turn user intent into production-grade software artifacts  
**Non-goals:** framework-specific style guides or language-specific implementation details

## Purpose

Decapod MUST force architecture clarity before execution and promotion.  
The control plane does this with explicit artifacts, typed gates, and proof surfaces.

## Mandatory Primitives

1. **Intent primitive**: governed PLAN (`<repo>/.decapod/governance/plan.json`) defines intent, scope, unknowns, and proof hooks.
2. **Architecture primitive**: architecture artifact (`<repo>/.decapod/governance/architecture.md`) captures system design and operational readiness.
3. **Proof primitive**: executable checks (`decapod validate`, tests, linters) verify outcomes.

## Architecture Artifact Contract

Before execution/promotion-relevant operations, the architecture artifact MUST exist and include all sections:

- `## Intent Alignment`
- `## System Design`
- `## Invariants`
- `## Tradeoffs`
- `## Verification Strategy`
- `## Rollout & Operations`

If missing or incomplete, Decapod MUST return typed marker `NEEDS_HUMAN_INPUT` with remediation questions.

## Golden Path Expectations

For production-grade delivery, agents MUST:

1. Preserve deterministic behavior and typed failure semantics.
2. Maintain explicit state boundaries and avoid hidden side effects.
3. Document compatibility/migration impact before promotion.
4. Define verification strategy before execution.
5. Keep rollback/remediation path explicit.

## Proof Surfaces

- `decapod govern plan check-execute` MUST fail with typed marker when architecture artifact is missing/incomplete.
- `decapod validate` Plan-Governed Execution Gate MUST enforce architecture artifact readiness for governed plans.
- `tests/plan_governed_execution.rs` MUST cover artifact generation and missing-artifact failure.

## Claim Mapping

- `claim.architecture.artifact_required_for_governed_execution`
- `claim.architecture.intent_to_design_traceability`

# ARCHITECTURE_FOUNDATIONS.md - Industry-Grade Engineering Foundations

**Authority:** interface (binding architecture directives)  
**Layer:** Interfaces  
**Binding:** Yes  
**Scope:** architecture fundamentals that keep intent alignment and production-grade engineering explicit in the constitution  
**Non-goals:** runtime architecture files under `.decapod/*`, framework-specific style guides, language-specific implementation detail

## Purpose

Decapod MUST keep architecture guidance in constitution documents and enforce quality through deterministic gates.
Architecture directives are policy, not mutable runtime state.

## Mandatory Primitives

1. **Intent primitive**: governed PLAN defines intent, scope, unknowns, and proof hooks.
2. **Architecture directive primitive**: constitution interfaces define required architecture thinking before promotion.
3. **Proof primitive**: executable checks (`decapod validate`, tests, linters) verify outcomes.

## Golden Path Expectations

For production-grade delivery, agents MUST:

1. Preserve deterministic behavior and typed failure semantics.
2. Maintain explicit boundaries (state, interfaces, ownership) and avoid hidden side effects.
3. Document compatibility and migration impact before promotion.
4. Define verification strategy tied to concrete proof hooks.
5. Keep rollback/remediation path explicit.
6. Make tradeoffs explicit (what was chosen, what was rejected, why).

## Required Architecture Reasoning Surfaces

Architecture reasoning MUST be present in governed artifacts and reviewable evidence, including:

- intent alignment (problem, user outcome, non-goals)
- system design (interfaces, boundaries, data ownership)
- invariants and failure modes
- tradeoffs and risk posture
- verification strategy
- rollout and operations

## Proof Surfaces

- `decapod validate` Plan-Governed Execution Gate enforces plan state, intent resolution, unknown resolution, and verification readiness.
- CI proof surfaces (`cargo fmt`, `cargo clippy`, `cargo test`, `decapod validate`) remain mandatory before promotion.

## Claim Mapping

- `claim.architecture.artifact_required_for_governed_execution`
- `claim.architecture.intent_to_design_traceability`

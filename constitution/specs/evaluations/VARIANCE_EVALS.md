# VARIANCE_EVALS.md - Variance-Aware Evaluation Contract

**Authority:** spec (evaluation methodology contract)
**Layer:** Specs
**Binding:** Yes

## Purpose

Define how Decapod treats non-deterministic frontend/backend evaluation so promotion decisions remain reproducible and falsifiable.

## Core Rules

1. Evaluations that involve browser flows, async services, or LLM judgment MUST use repeated runs.
2. Promotion-relevant comparisons MUST include confidence intervals (CI), not single-run point estimates.
3. Deterministic asserts are allowed only for deterministic units (schema checks, hashing, canonical serialization).
4. Non-deterministic integration/e2e outcomes MUST be represented as distributions over repeated runs.

## Repeat-Run Policy

1. Minimum default runs per variant: `N >= 5`.
2. Variant means baseline vs candidate under identical settings except intended treatment variable.
3. Runs MUST be labeled by plan lineage and variant id.

## Bootstrap CI Contract

1. Decapod aggregate computes bootstrap CI over `delta_success_rate = candidate - baseline`.
2. Aggregate artifact MUST store: baseline_n, candidate_n, iterations, ci_low, ci_high, observed_delta.
3. CI computation inputs MUST be hash-addressable via referenced run/verdict artifacts.

## Regression Policy

1. Silent regression is forbidden.
2. Default regression failure condition: CI upper bound is below zero beyond configured tolerance.
3. Gate decisions MUST emit explicit reasons for each failing condition.

## Reproducibility Contract

1. EVAL_PLAN MUST capture model/agent settings, judge settings, tool versions, environment fingerprint, and seed.
2. Cross-plan comparisons MUST fail if plan hashes differ, unless explicitly acknowledged.
3. Any critical setting change MUST produce a different plan hash.


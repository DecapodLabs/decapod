# Obligations Engine Implementation Plan

## Goal
Add a "mentor/babysitter" that pushes agents back to prior decisions, specs, knowledge graph, and todos through deterministic obligations returned in RPC envelope.

## Implementation Steps

### 1. Core Module: src/core/mentor.rs
- Define `MentorEngine` struct
- Implement `compute_obligations()` method
- Deterministic candidate retrieval from:
  - ADRs (docs/decisions/)
  - Spec docs (docs/spec.md, docs/architecture.md, etc.)
  - Knowledge graph nodes
  - Active todos/commitments

### 2. RPC Integration
- Add `mentor.obligations` op to rpc.rs
- Return obligations in standard envelope:
  - obligations.must (<= 5 items, default 2-3)
  - obligations.recommended (<= 5 items)
- Each item: { kind, ref, title, why_short, evidence }

### 3. Candidate Retrieval & Scoring
- Keyword/tag matching
- Recency weighting
- Path-based relevance
- Contradiction detection

### 4. High-Risk Operation Detection
- Detect git operations, deps changes, auth, network, security paths
- Increase strictness for these ops

### 5. Integration Points
- Update capabilities manifest
- Update README
- Add tests for determinism, capping, contradictions

## Key Constraints
- Deterministic: same repo state + input = same obligations
- Immutable sources only (never modify existing docs/KG)
- Compact views (max 5 items per list)
- Optional LLM only for ranking/phrasing, never for adding obligations

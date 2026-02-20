# Decapod Governance Kernel Architecture Review

**Date:** 2026-02-19  
**Version:** v1.0  
**Status:** Complete

## Executive Summary

This architecture review analyzes Decapod's governance kernel following the implementation of STATE_COMMIT v1, a cryptographic state commitment protocol. The review examines the system's core abstractions, proof surfaces, enforcement mechanisms, and identifies strategic wedges for market positioning.

## 1. Core Abstractions

### 1.1 Intent-Driven Control Plane

Decapod operates on the principle that **intent is the primitive; spec is yielded by intent**.

```
INTENT → SPEC → PROOF → IMPLEMENTATION
```

This flow is now machine-enforceable:
- **INTENT**: Decapod cryptographically commits to exact state agent claims
- **SPEC**: CBOR schema, canonicalization rules, entry semantics (STATE_COMMIT v1)
- **PROOF**: Golden vectors, CI enforcement, cross-platform verification
- **IMPLEMENTATION**: prove/verify CLI commands, promotion gates

### 1.2 Dual-Store Architecture

| Store | Purpose | Persistence |
|-------|---------|-------------|
| **User Store** | Agent state, session data, preferences | ~/.decapod/ |
| **Repo Store** | Event-sourced governance state, proofs, traces | .decapod/data/ |

The repo store is the source of truth for governance decisions.

### 1.3 Proof Surfaces

Decapod implements multiple proof surfaces:

| Surface | Purpose | Status |
|---------|---------|--------|
| **VALIDATE** | Methodology compliance, toolchain checks | ✅ Mature |
| **STATE_COMMIT** | Cryptographic state commitment | ✅ v1 Shipped |
| **TRACE** | Execution provenance, audit trail | ✅ Mature |
| **VERIFY** | Baseline drift detection | ✅ Mature |

## 2. STATE_COMMIT v1: Deep Analysis

### 2.1 Protocol Design

**Core Innovation**: Offline-verifiable commitment to repository state using only git objects + constitution.

**Inputs:**
- `base_sha`: Merge-base commit
- `head_sha`: Target commit
- `constitution_ref`: Governance snapshot
- `ignore_policy_hash`: Resolved ignore patterns

**Canonical Encoding:**
- CBOR with integer keys (1-6)
- Definite-length maps/arrays
- Keys sorted numerically
- No floats, shortest int encoding

**Hash Computation:**
```
leaf_hash = SHA256(CBOR([path, kind, mode_exec, content_hash]))
state_commit_root = Merkle_root(sorted(leaf_hashes))
scope_record_hash = SHA256(scope_record_bytes)
```

### 2.2 Verification Contract

A STATE_COMMIT proof is valid iff:

1. ✅ scope_record.cbor exists
2. ✅ scope_record_hash matches SHA256(bytes)
3. ✅ head_sha in record matches current HEAD
4. ✅ Root recomputed from git objects matches recorded root
5. ✅ All verification offline (no network)
6. ✅ All content from git objects (not filesystem)

### 2.3 Immutability Guarantees

**Golden Vectors (v1):**
```
scope_record_hash: 41d7e3729b6f4512887fb3cb6f10140942b600041e0d88308b0177e06ebb4b93
state_commit_root: 28591ac86e52ffac76d5fc3aceeceda5d8592708a8d7fcb75371567fdc481492
```

Any byte-level change requires SPEC_VERSION bump to v2.

## 3. Enforcement Mechanisms

### 3.1 Promotion Gates

STATE_COMMIT is now enforced at promotion time:

```rust
// proof_plan must include "state_commit"
if proof_plan.contains("state_commit") {
    // Recompute root from git objects
    let recomputed = state_commit::prove(base, head)?;
    // Verify against expected
    assert_eq!(recomputed.root, expected_root);
}
```

**Failure Modes:**
- Root mismatch → "Files changed since scope recorded"
- HEAD mismatch → "Current HEAD not in scope_record"
- Missing proof → "Run decapod state-commit prove"

### 3.2 CI Enforcement

**Cross-platform verification:**
- ubuntu-latest: Golden vectors reproducibility
- macos-latest: Determinism verification
- Golden vectors committed as `tests/golden/state_commit/v1/`

**Policy Knobs:**
- `DECAPOD_STATE_COMMIT_CI_JOB`: Configurable CI job name
- `DECAPOD_STATE_COMMIT_REQUIRED`: Enforce on promotion

## 4. Strategic Wedges

### 4.1 Market Positioning

**Current State:**
Decapod is the only AI agent governance system with:
1. Cryptographic state commitments (not just assertions)
2. Offline-verifiable proofs
3. Cross-platform deterministic verification

**Differentiation:**

| Competitor | Approach | Decapod Advantage |
|------------|----------|-------------------|
| General AI tools | Assertions/vibes | Provable commitments |
| CI/CD platforms | Build artifacts | Semantic state proofs |
| Agent frameworks | Logging/tracing | Cryptographic verification |

### 4.2 Flagship Feature

> "Decapod doesn't just gate actions; it cryptographically commits to the exact state an agent claims to have produced—then verifies it."

This is the core wedge. It turns "trust me" into "show me the proof."

### 4.3 Next Wedge Opportunities

Based on the kernel analysis, the next strategic features are:

1. **Gatekeeper** (HIGH): Path allowlists, secret scanning, dangerous pattern detection
   - Builds on STATE_COMMIT's deterministic scope
   - Adds safety constraints to the proof

2. **Secret Redaction** (HIGH): Automatic PII/credential scrubbing from traces
   - Enables enterprise adoption
   - Required for regulated environments

3. **Doctor** (MEDIUM): Read-only health checks
   - Pre-flight validation
   - Reduces failed promotions

4. **Keystore** (LOW): Secure credential management
   - macOS Keychain / Linux Secret Service
   - Optional but important for security-conscious orgs

## 5. Technical Debt & Risks

### 5.1 Current Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Git plumbing dependency | MEDIUM | Well-tested, standard git commands |
| CBOR canonicalization | LOW | Fixed schema, integer keys |
| Constitution drift | MEDIUM | Versioned refs, explicit policy hash |
| Performance on large diffs | LOW | O(changed bytes), acceptable for v1 |

### 5.2 Architecture Strengths

1. **Hermetic**: Same code path for prove/verify
2. **Deterministic**: Reproducible across platforms
3. **Offline**: No network dependencies
4. **Binding**: Proof tied to specific commit + constitution
5. **Immutable**: v1 protocol frozen, version bumps required for changes

## 6. Recommendations

### 6.1 Immediate (Next 2 Weeks)

1. ✅ **Ship STATE_COMMIT v1** - DONE
   - Golden vectors committed
   - CI enforcement active
   - Documentation complete

2. **Implement Gatekeeper** (1-shot task)
   - Path allowlist/blocklist
   - Secret scanning
   - Dangerous pattern detection

3. **Implement Secret Redaction** (1-shot task)
   - API key detection
   - Bearer token masking
   - Enterprise-ready traces

### 6.2 Short-term (Next Month)

4. **Doctor Preflight Checks**
   - Git status validation
   - Required files check
   - Config validation

5. **Validation Gate Enhancement**
   - Auto-detect PRs touching verifier code
   - Require golden vectors CI
   - Block merges on mismatch

### 6.3 Long-term (Next Quarter)

6. **STATE_COMMIT v2 Research**
   - Incremental diff computation
   - Performance optimization
   - LFS materialization (optional)

7. **Keystore Integration**
   - macOS Keychain support
   - Linux Secret Service
   - Optional feature flag

## 7. Conclusion

STATE_COMMIT v1 establishes Decapod as the only AI agent governance platform with cryptographic state commitments. The kernel is solid, the protocol is frozen, and the enforcement mechanisms are in place.

**The wedge is real**: "Proofs before power" is now a technical reality, not just a slogan.

**Next move**: Ship Gatekeeper and Secret Redaction to complete the safety story, then focus on enterprise adoption through the compliance narrative.

---

**Reviewers:** Minimax (AI), Agent Unknown  
**Approved:** 2026-02-19  
**Next Review:** After Gatekeeper implementation

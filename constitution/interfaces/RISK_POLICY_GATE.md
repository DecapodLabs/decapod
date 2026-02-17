# RISK_POLICY_GATE.md - Deterministic PR Risk Policy Contract

**Authority:** interface (binding contract for risk-aware PR gating and review freshness)
**Layer:** Interfaces
**Binding:** Yes
**Scope:** machine-readable risk contract semantics, gate ordering, SHA freshness, and evidence requirements
**Non-goals:** CI provider-specific implementation details or workflow YAML tutorials

This interface defines the canonical control-plane semantics for deterministic PR gating.

---

## 1. Contract Source (Single Machine Contract)

`(Truth: SPEC)` Risk and merge policy MUST be declared in one machine-readable contract file (claim: `claim.risk_policy.single_contract_source`).

Minimum contract sections:
- `version`
- `riskTierRules` (path globs -> risk tier)
- `mergePolicy` (risk tier -> required checks)
- `docsDriftRules` (required doc updates for control-plane changes)
- `evidenceRequirements` (risk tier/path class -> evidence manifest requirements)

Template reference: section `## 10. Contract Example (JSON)`.

---

## 2. Preflight Ordering (Before CI Fanout)

`(Truth: SPEC)` The risk-policy gate MUST execute before expensive CI fanout jobs (claim: `claim.risk_policy.preflight_before_fanout`).

Preflight sequence:
1. Resolve changed files.
2. Resolve risk tier(s) from the machine contract.
3. Compute required checks.
4. Enforce docs-drift rules.
5. Enforce current-head review freshness gates.

Only after preflight success may build/test/security fanout begin.

---

## 3. Current-Head SHA Discipline

`(Truth: SPEC)` Review-agent evidence is valid only for the current PR head SHA (claim: `claim.review.sha_freshness_required`).

Required behavior:
- Wait for review-agent check run associated with current `head_sha`.
- Ignore stale comments/check results tied to older SHAs.
- Fail if current-head review status is missing, failed, or timed out.
- Require rerun on every synchronize/push event.

---

## 4. Canonical Rerun Writer

`(Truth: SPEC)` Exactly one workflow/service is the canonical rerun-comment writer (claim: `claim.review.single_rerun_writer`).

Required dedupe contract:
- Use stable marker token.
- Include `sha:<head_sha>` in rerun request payload.
- Do not emit duplicate rerun comments for same marker + SHA.

---

## 5. Optional Remediation Loop

`(Truth: SPEC)` A remediation agent may patch in-branch only when findings are actionable; it MUST re-enter the same policy loop (claim: `claim.review.remediation_loop_reenters_policy`).

Required guardrails:
- Patch and push to same PR branch.
- Do not bypass policy gates.
- Treat stale findings as non-authoritative.

---

## 6. Browser Evidence Manifest (UI/Critical Flows)

`(Truth: SPEC)` UI and critical user-flow changes require machine-verifiable evidence manifests, not prose screenshots (claim: `claim.evidence.manifest_required_for_ui`).

Evidence contract requirements:
- Manifest records flow IDs, entrypoint, actor/account assertions, timestamps, artifact paths or hashes.
- Verification step fails on missing required flows, stale artifacts, or assertion mismatch.

---

## 7. Harness-Gap Loop

`(Truth: SPEC)` Production regressions MUST route to harness-gap tracking: incident -> harness case -> tracked follow-up (claim: `claim.harness.incident_to_case_loop`).

This keeps regressions from remaining one-off fixes without test/evidence growth.

---

## 8. Truth Labels and Upgrade Path

- `claim.risk_policy.single_contract_source`: `SPEC` -> upgrade to `REAL` when a named enforcement surface blocks drift.
- `claim.risk_policy.preflight_before_fanout`: `SPEC` -> `REAL` when gate ordering is validated automatically.
- `claim.review.sha_freshness_required`: `SPEC` -> `REAL` when current-head SHA matching is enforced by CI/control plane.
- `claim.review.single_rerun_writer`: `SPEC` -> `REAL` when duplicate-writer/race checks exist.
- `claim.review.remediation_loop_reenters_policy`: `SPEC` -> `REAL` when remediation runs are policy-gated and auditable.
- `claim.evidence.manifest_required_for_ui`: `SPEC` -> `REAL` when manifest verifier is mandatory for tiered changes.
- `claim.harness.incident_to_case_loop`: `SPEC` -> `REAL` when incident-to-case linkage is machine-audited.

---

## 9. Planned Proof Surfaces

Planned (not yet enforced):
- `decapod validate` gate: interface structure + contract presence checks.
- `risk-policy-gate` CI job.
- `harness:ui:verify-browser-evidence` CI job.
- review-agent current-head check run verifier.

---

## 10. Contract Example (JSON)

```json
{
  "version": "1",
  "riskTierRules": {
    "high": [
      "app/api/legal-chat/**",
      "lib/tools/**",
      "db/schema.ts"
    ],
    "medium": [
      "app/ui/**",
      "apps/web/**"
    ],
    "low": [
      "**"
    ]
  },
  "mergePolicy": {
    "high": {
      "requiredChecks": [
        "risk-policy-gate",
        "code-review-agent",
        "harness-smoke",
        "browser-evidence-verify",
        "ci-pipeline"
      ]
    },
    "medium": {
      "requiredChecks": [
        "risk-policy-gate",
        "code-review-agent",
        "ci-pipeline"
      ]
    },
    "low": {
      "requiredChecks": [
        "risk-policy-gate",
        "ci-pipeline"
      ]
    }
  },
  "docsDriftRules": {
    "controlPlaneTouchedRequires": [
      "constitution/interfaces/RISK_POLICY_GATE.md",
      "constitution/interfaces/CLAIMS.md"
    ]
  },
  "evidenceRequirements": {
    "uiOrCriticalFlowChanged": {
      "requireManifest": true,
      "requiredChecks": [
        "browser-evidence-capture",
        "browser-evidence-verify"
      ]
    }
  }
}
```

---

## Links

### Core Router
- `core/DECAPOD.md` - Router and navigation charter

### Registry (Core Indices)
- `core/INTERFACES.md` - Interface contracts index

### Contracts (Interfaces Layer)
- `interfaces/CLAIMS.md` - Claims registry
- `interfaces/CONTROL_PLANE.md` - Control-plane sequencing patterns
- `interfaces/DOC_RULES.md` - Doc compiler and truth-label rules
- `interfaces/STORE_MODEL.md` - Store semantics
- `interfaces/AGENT_CONTEXT_PACK.md` - Agent context pack contract

### Machine Contracts
- `interfaces/RISK_POLICY_GATE.md` - Inline JSON contract example (§10)

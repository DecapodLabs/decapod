# Decapod Docs

This is the operator and integrator landing page for embedded Decapod docs.

## Start Here

- `README.md`: product positioning and quickstart.
- `docs/ARCHITECTURE_OVERVIEW.md`: canonical runtime model.
- `docs/CONTROL_PLANE_API.md`: stable CLI/RPC control-plane contract.
- `docs/GOVERNANCE_AUDIT.md`: governance-first capability audit + dependency-ordered kernel TODOs.
- `docs/VERIFICATION.md`: operator verification commands and proof surfaces.
- `docs/SECURITY_THREAT_MODEL.md`: security posture and limits.
- `docs/RELEASE_PROCESS.md`: release readiness and versioning discipline.
- `docs/MIGRATIONS.md`: forward-only schema evolution policy.

## Enforcement Surfaces

- `decapod validate`
- `decapod release check`
- `decapod handshake`
- `decapod workspace publish` (requires provenance manifests)

## Foundation Anchors

- `core/DECAPOD.md` (foundation demands: intent, boundaries, proof, daemonless/repo-native posture)
- `specs/SYSTEM.md` (binding doctrine and promotion semantics)
- `interfaces/CONTROL_PLANE.md` (integration and liveness contract)
- `interfaces/CLAIMS.md` (claim registry + proof surface mapping)

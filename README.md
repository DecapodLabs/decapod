# Decapod

Decapod is a daemonless, repo-native control plane for governed agent work.

It is the kernel between an agent runtime and a git repository. It makes intent explicit, boundaries explicit, and completion provable.

## 30-Second Model

Decapod gives every agent the same three primitives:

- Intent: what work is being attempted and under which constraints.
- State: deterministic, append-only, schema-bound repository state.
- Proof: executable gates that decide whether work is complete.

Differentiator:

- Daemonless: no always-on coordinator process.
- Repo-native: promotion-relevant state lives with the repo.
- Proof-gated: no "looks done" completion.

## What Decapod Is Not

- Not an agent framework.
- Not a prompt pack.
- Not a replacement for your model runtime (Claude Code, Codex, OpenCode, CrewAI, etc.).

Decapod is a stable governance shim those systems call.

## Quickstart (5 Commands)

```bash
decapod init
decapod validate
decapod docs ingest
decapod session acquire
decapod handshake --scope "bootstrap" --proof "decapod validate"
```

Result: a repo-native handshake artifact is written to `.decapod/records/handshakes/` and can be audited offline.

## Compatibility

Decapod is agent-agnostic. Integrations are done through:

- CLI: stable command groups and JSON envelopes.
- RPC: typed request/response envelope.
- Tiny SDK shims: `sdk/python` and `sdk/typescript`.

See:

- `docs/CONTROL_PLANE_API.md`
- `examples/claude_code_workflow.md`
- `examples/python_validate_demo.py`
- `examples/ts_validate_demo.js`

## Trust Model

Assumptions:

- Git history and local filesystem are available.
- Agents invoke Decapod instead of mutating `.decapod` directly.
- Operators review proofs before promotion.

Decapod enforces:

- Protected-branch/worktree boundaries.
- Session-scoped agent identity.
- Deterministic reducers and schema-bound envelopes.
- Publish-time provenance manifest requirement.

Decapod refuses:

- Promotion without required provenance manifests.
- Protected branch implementation flow.
- Unclaimed-task worktree provisioning.

## Proof Surfaces

Primary proof surfaces:

- Claims registry: `constitution/interfaces/CLAIMS.md`
- Validation gates: `decapod validate`
- STATE_COMMIT: `decapod state-commit prove|verify`
- Drift/verification: `decapod qa verify`
- KCR trend artifact: `docs/metrics/KCR_TREND.jsonl`

## Core Docs

- Docs landing: `docs/README.md`
- Architecture overview: `docs/ARCHITECTURE_OVERVIEW.md`
- Control plane API + stability: `docs/CONTROL_PLANE_API.md`
- Security threat model: `docs/SECURITY_THREAT_MODEL.md`
- Release process: `docs/RELEASE_PROCESS.md`
- Migrations policy: `MIGRATIONS.md`
- Maintainers: `MAINTAINERS.md`

## Contributing

Read `CONTRIBUTING.md` before opening a PR.

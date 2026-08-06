# Contribution Conventions

Project-wide standards for where code and tooling live, and how changes are packaged for review. Agents proposing or implementing changes MUST respect these conventions in addition to the per-PR rules in `CONTRIBUTING.md`.

## 1. Project Tooling Standard

Project tooling is Rust. Non-Rust one-offs (Python, Bash) are not the contribution path; raise them in an issue first so they can be folded into a Rust tool under `assets/` rather than committed as stray scripts.

### Source layout

| Path | Purpose |
|---|---|
| `src/decapod/` | Runtime library code. |
| `src/main.rs` | Runtime binary entrypoint. |
| `assets/build/` | Build-time Rust (build scripts, codegen). Declared via `build =` in `Cargo.toml`. |
| `assets/benches/` | Benchmark Rust. |
| `assets/tools/` | Maintenance and release-ops Rust. Each tool is its own `.rs` file exposed as a `[[bin]]` target in `Cargo.toml` and invoked via `cargo run --bin <name>`. |

### Determinism requirement

Tools under `assets/tools/` MUST be deterministic: fixed inputs produce fixed outputs. This keeps a tool callable from a `.github/` workflow today and from a future `decapod validate` repository-specific gate extension without rework. Avoid time-based seeds, network fetches, or ambient environment reads inside the tool surface.

## 2. Before Implementation

When asked to add a maintenance or release-ops tool:

1. Place the implementation at `assets/tools/<name>.rs`.
2. Register a `[[bin]]` target in `Cargo.toml` (`name = "<name>", path = "assets/tools/<name>.rs"`).
3. Keep inputs/outputs deterministic. Read from explicit file paths or CLI args; never from ambient state.
4. Wire execution through a `.github/workflows/*.yml` job that calls `cargo run --bin <name> ...` and records the result as visible CI output.
5. Do not invent a new `decapod validate` gate for repository-specific checks. The binary does not currently expose a custom-gate seam. If a check should be part of `validate`, raise an issue first to scope a binary-side capability.

## 3. Governance Artifacts Per PR

Every PR MUST change all four governance files, enforced by the `governance-artifacts` CI job:

- `.decapod/governance/claims.json` — the falsifiable research claims ledger. Refresh for the change being made: add or update the claim this PR advances, with its baseline, observable Decapod condition, failure mode, measurement, and proof gate. Do not hand-edit; use the sanctioned CLI surface.
- `.decapod/governance/plan.json` — the governed plan for the change. Initialize with `decapod govern plan init`, approve with `decapod govern plan approve`, patch with `decapod govern plan update`.
- `.decapod/governance/trajectory.json` — the per-run custody ledger. Initialize with `decapod govern trajectory init` and record evidence with `decapod govern trajectory record`.
- `.decapod/governance/validation.json` — the validation receipt. Refreshed by `decapod validate`.

If a PR legitimately advances no falsifiable claim about the governance kernel (e.g. a pure docs/conventions change), surface that in the PR body and confirm with the reviewer that no `claims.json` entry is required before merge. Do not invent a fake claim to satisfy the gate.

## 3b. Material Living-Spec Rewrites Per PR

Every non-release PR MUST include a **material** change under `.decapod/managed/specs/*.md` — authored prose that reflects the change under review. Enforced by:

- `decapod validate` (Living Specs Material Mutation Gate on feature branches)
- `decapod workspace publish` / PR publication (`FINGERPRINT_ONLY_SPECS`)
- the `material-specs` CI job

Material means the document body differs from the PR base after stripping auto-generated blocks:

- `<!-- decapod:codebase-attestation:* -->` (repo-signal fingerprints)
- `<!-- decapod:declared-capabilities:* -->`
- `<!-- decapod:capability-overlay:* -->`

`decapod rpc --op specs.refresh` and `decapod validate --refresh-specs` only re-attest fingerprints and overlays. That is required hygiene, not a living-spec rewrite. Not every file needs an edit; at least one of INTENT, ARCHITECTURE, INTERFACES, VALIDATION, SEMANTICS, OPERATIONS, SECURITY, or README must carry a material prose change.

Living specs are evidence material for proof completion: VERIFIED workunits must bind at least one `.decapod/managed/specs/*` path in `spec_refs`, validation epochs hash authored material bodies (`living_spec_material:*`), and completion evidence fails when those digests or bindings are missing.

The acting agent authors and maintains the semantic content of living specs.
Decapod requires and validates that content, and `specs.refresh` updates supported
fingerprints, attestations, overlays, and manifests. Refresh is not authorship.
If a spec is wrong, validation has exposed the agent's misunderstanding in a
reviewable artifact before publication. Correct the prose and revalidate; a
stale spec normally means the governed work remains incomplete.

## 4. Entrypoint and Dockerfile Pin Discipline

`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`, and `.decapod/managed/Dockerfile.decapod` carry a Decapod release pin and fingerprint that MUST agree with the installed binary. `decapod validate` self-heals these when they drift; do not hand-edit the release pins or fingerprints. If `validate` reports `entrypoint_release_mismatch`, rerun it and let the binary refresh the pinned headers; the file bodies (project-specific prose, governed sections) are preserved.

## 5. First-Commit Publication Readiness

Run validation before opening the pull request and commit every generated
projection it refreshes in that first commit. The PR diff must carry changed
entrypoint fingerprints, the managed Dockerfile release pin, managed-spec
fingerprints or authored spec updates, and all four governance artifacts when
the change affects them. A pull request is a completion signal, not a place to
discover local-only drift. If a generated artifact is stale, regenerate it
through Decapod, stage it, and rerun validation before publication.

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

`.decapod/governance.json` is the sole tracked governance authority. It contains
one compact baseline, current-PR claims and checkpoints, explicit unresolved
obligations, and logical plan, trajectory, validation, and optional Jev sections.
Runtime workunit manifests remain ignored under `.decapod/data/workunits/`.

Update logical sections through their existing CLI surfaces: `govern plan`,
`govern trajectory`, and `validate`. Use `govern artifacts claim` for a real
current-PR claim with a falsifier; a supported claim requires proof references.
Do not invent a claim when the change advances none. `inventory --claims-note`
records a checkpoint instead of appending history to a claim policy.

### Population order

Populate stable PR identity first, then intent/scope in the plan, actual current
claims and planned checks, work/evidence in the trajectory, validation, and the
staged-material checkpoint for each commit. Persist meaningful boundaries while
working, not only at final completion. Combine related updates at each boundary
and skip unchanged re-statements; return to the affected step when intent, scope,
or evidence changes without resetting the PR identity.

### Current-PR sequence

1. Fast-forward the protected base, claim a todo, and run `workspace ensure`.
2. Preserve any legacy evidence in Git, then use `govern artifacts migrate` for
   lossless adoption and `govern artifacts begin-pr --id <change> --base-branch master`
   for the explicit boundary. Accepted completed claims remain in Git history;
   unresolved obligations carry forward and require explicit proof-backed resolution.
3. Refine the plan, record trajectory and proof, review material living specs,
   refresh supported projections, and run `decapod validate` without skipping gates.
4. Stage authored code/spec changes, record `govern artifacts checkpoint --id <unique-id>
   --summary "..." --proof-ref <evidence>`, then stage governance.json and commit.
   Repeat for every authored commit, including merges; later proof cannot repair a
   missing historical checkpoint.
5. Run `workspace publish`. It checks staged proof before auto-commit, then verifies
   every checkpoint against the immutable pushed commit graph and exact PR target.

Inventory is read-only unless a repair/compact/note option is explicitly used.
Repair initializes absent empty claims without overwriting existing evidence;
compact preserves valid legacy semantics. Neither replaces a missing plan,
trajectory, or validation receipt. See [governance-artifacts.md](../book/src/reference/governance-artifacts.md).

### Research claims versus Health Engine claims

Current-PR research claims in governance.json are distinct from operational
Health Engine claims and proof events in `.decapod/data/decapod.db`. Use
`govern health claim` and `govern health proof` for Health Engine state. Never
hand-edit either authority or copy accepted historical claim catalogs into each PR.

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

The authored portion of these files is a narrow, explicit exception to the
machine-state mutation boundary: agents may read and edit semantic prose
directly in an isolated workspace. They must preserve generated attestation,
declared-capability, capability-overlay, and manifest data, and refresh those
machine-owned projections through the Decapod CLI/RPC. This does not authorize
direct edits to `.decapod/data`, governance, sessions, workspaces, or any other
control-plane state.


## 4. Entrypoint and Dockerfile Pin Discipline

`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, and `GEMINI.md` are **Decapod-owned
templates**. Agents and humans MUST NOT hand-edit them in project PRs. The only
allowed content is the exact template and release fingerprint for the installed
Decapod binary (`decapod-release` + `decapod-fingerprint` headers plus the
compiled body).

### Always verify; bump only on mismatch

| Situation | Behavior |
| --- | --- |
| On-disk pin **matches** evaluating Decapod release/fingerprint | **Verify only** — do not rewrite entrypoints; no PR noise |
| On-disk pin **differs** (project or CI moved to a newer Decapod) | **Decapod rewrites** entrypoints (and the managed Dockerfile pin) to the evaluating template; **include those diffs in the PR** |
| Hand edit, mode-only touch, or invented fingerprint | **Hard fail** |

Subsequent project PRs on the **same** Decapod version should confirm pins and
leave entrypoints untouched. The next Decapod upgrade will fail verification
until Decapod bumps the fingerprints again.

`.decapod/managed/Dockerfile.decapod` likewise carries a Decapod release pin.
Do not hand-edit the image/version header; validate self-heals the pin when the
installed release changes. Project-specific package lines below the pin remain
the only intentional local Dockerfile edits.

### Pin lag after a Decapod release (flywheel)

Master entrypoint/Dockerfile/manifest pins record the Decapod version that
**generated** that master tip. Automated post-release heal PRs and the Release
Artifact Sync workflow are removed.

1. Master generated by Decapod `vN` (pins say `vN`).
2. User/agent PRs with installed `vN` merge.
3. Release `vN+1` cuts Cargo/CHANGELOG only.
4. Users install `vN+1`; master pins stay `vN` until a user/agent PR validates
   with `vN+1` and commits all release-bound surfaces.


## 4b. Governance JSON — explicit current-PR evidence

Every project PR must update `.decapod/governance.json`. Presence at the tip is
insufficient: each authored commit needs its own exact-material checkpoint,
with prior checkpoints preserved cumulatively until the PR is accepted.
The validation receipt is a logical section written by successful `decapod validate`.

## 4c. Living specs — material every PR; fingerprint only on Decapod version advance

- Every non-release PR must include a **unique material** living-spec rewrite that
  encompasses the code/ops/intent change under review (`FINGERPRINT_ONLY_SPECS`).
- Always re-verify attestation against the evaluating binary (`specs.refresh` /
  validate). Do not skip the check.
- Spec **fingerprint** values (and release-bound attestation) only **need to
  change** when this PR evaluates a **newer Decapod version** than the project
  base (previous merged pin). Same Decapod version as base/master → fingerprint
  may stay stable even while material prose updates.

## 4d. Early alignment check (agent + validate)

Agents calling Decapod must establish version alignment **early** (before heavy
implementation), typically as part of the first `decapod validate` / workspace
entry sequence:

1. Resolve evaluating Decapod release (binary identity).
2. Verify entrypoint + Dockerfile pins; rewrite only on mismatch.
3. Ensure governance JSON is updated for the active work.
4. Verify/refresh specs attestation (material prose as needed).
5. Continue remaining validation / implementation.

## 5. First-Commit Publication Readiness

Run validation before opening the pull request and commit every generated
projection it refreshes in that first commit. The PR diff must carry:

- **Always:** the normalized governance document with current-PR checkpoint coverage
- **When changed:** managed Dockerfile pin, managed-spec attestation, material
  living-spec prose
- **Entrypoints only when** Decapod rewrote them because the evaluating release
  fingerprint differed from on-disk pins

A pull request is a completion signal, not a place to discover local-only drift.
If a generated artifact is stale, regenerate it through Decapod, stage the
surfaces that legitimately changed, and rerun validation before publication.

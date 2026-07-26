# Intent

<!-- decapod:declared-capabilities:start -->

## Declared Capability Surfaces

- `agent-helper`
- `authentication`
- `authorization`
- `background-processing`
- `control-plane`
- `event-driven`
- `external-integrations`
- `governance-kernel`
- `persistent-state`
- `scheduled-jobs`

<!-- decapod:declared-capabilities:end -->
## Product Outcome
- Decapod is the daemonless, local-first governance kernel behind AI coding agents. Agents call it on demand to converge on human intent, shape context before inference, enforce boundaries, and deliver proof-backed completion across concurrent multi-agent work.

## What This Project Is
decapod is a service_or_library project built using Rust, rust.
Decapod is the daemonless, local-first governance kernel behind AI coding agents. Agents call it on demand to converge on human intent, shape context before inference, enforce boundaries, and deliver proof-backed completion across concurrent multi-agent work.

Key operating facts:
- **Primary languages**: Rust, rust
- **Detected surfaces**: cargo, rust

## Product View
```mermaid
flowchart LR
  U[Primary User] --> P[decapod]
  P --> O[User-visible Outcome]
  P --> G[Proof Gates]
  G --> E[Evidence Artifacts]
```

## Inferred Baseline
- Repository: decapod
- Product type: service_or_library
- Primary languages: Rust, rust
- Detected surfaces: cargo, rust

## Scope
| Area | In Scope | Proof Surface |
|---|---|---|
| Core workflow | Define a concrete user-visible workflow | Acceptance criteria + tests |
| Data contracts | Document canonical inputs/outputs | [INTERFACES.md](./INTERFACES.md) and schema checks |
| Delivery quality | Block promotion on broken proof surfaces | [VALIDATION.md](./VALIDATION.md) blocking gates |

## Non-Goals (Falsifiable)
| Non-goal | How to falsify |
|---|---|
| Feature creep beyond the primary outcome | Any PR adds capability not tied to outcome criteria |
| Shipping without evidence | Missing validation artifacts for promoted changes |
| Ambiguous ownership boundaries | Missing owner/system-of-record in interfaces |

## Constraints
- Technical: runtime, dependency, and topology boundaries are explicit.
- Operational: deployment, rollback, and incident ownership are defined.
- Security/compliance: sensitive data handling and authz are mandatory.

## Acceptance Criteria (must be objectively testable)
- [ ] Decapod validate passes, required tests pass, and promotion-relevant artifacts are present.
- [ ] Non-functional targets are met (latency, reliability, cost, etc.).
- [ ] Validation gates pass and artifacts are attached.
- [ ] `cargo test` passes for unit/integration coverage
- [ ] `cargo clippy -- -D warnings` passes with no denied lints
- [ ] `cargo fmt --check` passes on the repo

## Epistemic Custody Fields

### Active Assumptions
- Host has functional `git` (verified in workspace preflight)
- Host has functional `docker`/`podman` for container workspaces (optional but gated)
- Agent has write access to `.decapod/` directory
- DECAPOD_SESSION_PASSWORD set per-agent for session acquisition
- Project is a git repository (workspace ensure requires it)

### Confidence & Risk Level
- **Confidence**: High (local coordination, deterministic rebuild, proven workspace isolation)
- **Risk**: Low (local isolation limits blast radius; no background processes)

### Measured vs Inferred Facts
| Fact | Source (Provenance) | Type |
|------|---------------------|------|
| Git is installed | `Command::new("git")` check in workspace.rs | Measured |
| SQLite WAL mode works | `db_connect` with journal_mode=WAL fallback | Measured |
| Isolated branch naming | `workspace::ensure` execution with agent/todo scoping | Measured |
| Deterministic DB rebuild | `todo::rebuild_db_from_events` round-trip test | Measured |
| Capsule hash determinism | `context_capsule::canonical_json_bytes` + SHA256 | Measured |

### Unresolved Contradictions
- Cloud backend experimental opt-in exists but sync/migration not implemented
- Federation (multi-repo knowledge graph) vs single-repo governance boundaries need clarification

### Deferred Questions
- Full cloud synchronization protocol (beyond init registration)
- Cross-repository obligation graphs
- Long-term capsule storage tiering

### Stop Conditions
- SQLite lock contention exceeding retry budget (5 retries with exponential backoff)
- Conflicting todo claims by another active agent session
- Workspace on protected branch with local modifications
- Validation epoch drift detected without refresh

### Proof Required Before Completion
- Green `cargo test` suite (unit + integration)
- Complete spec verification manifest generation
- `decapod validate --refresh-specs` passes
- All promotion gates satisfied with attached evidence

## Tradeoffs Register
| Decision | Benefit | Cost | Review Trigger |
|---|---|---|---|
| Simplicity vs extensibility | Faster iteration | Potential rework | Feature set expands |
| Strict gates vs dev speed | Higher confidence | More upfront discipline | Lead time regressions |

## First Implementation Slice
- [ ] Define the smallest user-visible workflow to ship first.
- [ ] Define required data/contracts for that workflow.
- [ ] Define what is intentionally postponed until v2.

## Open Questions (with decision deadlines)
| Question | Owner | Deadline | Decision |
|---|---|---|---|
| Which interfaces are versioned at launch? | TBD | YYYY-MM-DD | |
| Which non-functional target is hardest to hit? | TBD | YYYY-MM-DD | |

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `4b90d3e50a5c0a0113a3cc5c2c7b7bd178b159861eb3307967669fc7f72694cd`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (97 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

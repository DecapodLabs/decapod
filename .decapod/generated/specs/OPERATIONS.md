# Operations## Capability Operations

Operational ownership follows the declared surfaces: session/authentication and policy/authorization protect mutations; SQLite/JSONL state and migrations require executable proof; workflow and scheduled jobs require bounded execution and failure visibility; Git/Cargo/container integrations remain explicit external actions; and CLI/JSON-RPC contracts remain machine-facing compatibility surfaces. Capability declarations must not be used as a substitute for command-level evidence.## Operational Readiness Checklist
- [ ] On-call ownership defined (local development tool — typically self-serve)
- [ ] SLOs defined for validation latency and workspace creation
- [ ] Runbooks linked for validation failures and workspace interlocks
- [ ] Rollback plan: `decapod workspace prune --force` + git reset
- [ ] Capacity guardrails: workspace disk quota, SQLite connection limits## Deployment Model

**Local-first CLI tool** — No server deployment. Decapod is distributed as:
- Cargo-installable binary (`cargo install decapod`)
- Prebuilt binaries via `cargo-dist` (GitHub Releases): linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64
- Runs entirely in user's repository context
- State stored in `.decapod/` within each repository

**Runtime Requirements:**
- Git 2.30+ (worktree support)
- Rust 1.96+ (for building from source)
- Docker/Podman (optional, for container workspaces)
- SQLite 3.35+ (bundled via rusqlite)## Service Level Objectives

| SLI | SLO Target | Measurement Window | Notes |
|-----|------------|-------------------|-------|
| Validate latency (P95) | < 30s | Per invocation | Bounded by `INV-BOUNDED-VALIDATE` |
| Workspace creation (P95) | < 30s | Per invocation | Excludes container build |
| Workspace creation + container (P95) | < 300s | Per invocation | Docker build time variable |
| Validation gate pass rate | 100% blocking gates | Per promotion | Zero tolerance for blocking gate failures |
| SQLite lock contention retry | < 5 retries | Per connection | Exponential backoff + jitter |
| Capabilities discovery | < 500ms | Per call | Includes crates.io version check |## Monitoring

### Key Signals (Self-Observability via Flight Recorder)
| Signal | Source | Query |
|--------|--------|-------|
| Validation duration | `decapod trace flight-recorder timeline` | `validation` events |
| Workspace interlocks | `decapod trace flight-recorder timeline` | `interlock.code` = `workspace_required` |
| Proof execution | `decapod qa verify` / `proof.events.jsonl` | `proof.run` events |
| Todo claim conflicts | `todo.events.jsonl` | `task.claim` with conflict |
| Session acquisition | `session.acquire` | broker events |

### Alerting (Human-Visible)
| Condition | Severity | Action |
|-----------|----------|--------|
| `decapod validate` fails blocking gates | Critical | Block promotion; inspect validation report |
| Workspace creation fails > 3x | Warning | Check git/docker availability; run `decapod system doctor` |
| SQLite `DatabaseLocked` > 5 retries | Warning | Check concurrent agents; run `decapod workspace prune` |
| Capsule policy lineage mismatch | Critical | Block publish; re-run `decapod govern capsule query --write` |
| Specs manifest out of sync | Warning | Run `decapod validate --refresh-specs` |## Health Checks

### Liveness
- `decapod system doctor` — Preflight checks (git, docker, config, store health)
- `decapod capabilities` — Binary functional + version check
- `decapod validate --store user` — Blank-slate store validation

### Readiness
- `decapod workspace status` — Can work in current directory?
- `decapod session status` — Valid session token?
- `decapod validate --store repo` — Repo store healthy?

### Dependency Health
- Git: `git rev-parse --is-inside-work-tree`
- Docker: `docker version` / `podman version`
- Crates.io: `curl https://crates.io/api/v1/crates/decapod` (cached 24h)
- Store: `storage_health_preflight` (fs type, writability, tmpdir)

### Synthetic Transaction
```bash
# Full governance loop smoke test
decapod init --force --dry-run --dir /tmp/smoke-test
cd /tmp/smoke-test && decapod activate && decapod todo add "Smoke test" && decapod todo claim --id <id> && decapod workspace ensure && decapod validate
```## Incident Response

### Detection
- Validation gate failures (CI or local `decapod validate`)
- Workspace interlock errors (agent receives `INTERLOCK` in RPC response)
- Flight recorder gaps (missing event sources)

### Triage
1. Run `decapod system doctor` — identifies environment issues
2. Run `decapod trace flight-recorder timeline --limit 50` — recent governance events
3. Check `decapod capabilities` — version, config, docker status

### Mitigation
| Incident | Mitigation |
|----------|------------|
| Protected branch with local mods | `git stash` → `decapod workspace ensure` |
| SQLite locked | `decapod workspace prune` → retry |
| Specs out of sync | `decapod validate --refresh-specs` |
| Capsule policy drift | `decapod govern capsule query --write` |
| Session expired | `decapod session acquire` |
| Container build fails | `decapod workspace ensure` (no `--container`) |

### Communication
- Local tool: human reads terminal output
- CI: GitHub Actions annotations on validation failure
- Multi-agent: `decapod data broker audit` shows all agent actions

### Post-Mortem
- `decapod trace flight-recorder transcript --output postmortem.md`
- Correlate attestations (`.decapod/generated/assurance_attestations.jsonl`)
- Review validation report (`.decapod/generated/artifacts/provenance/validation_report.json`)## Rollout Strategy

**Binary releases via cargo-dist:**
- Tagged releases: `v0.x.y` → GitHub Release with binaries
- `cargo install decapod` pulls from crates.io
- Auto-update check in `decapod capabilities` (crates.io API, cached 24h)

**Config/Schema Migrations:**
- `schema_version` in `.decapod/config.toml` (validated at startup)
- `TODO_SCHEMA_VERSION` + additive migrations in `todo::ensure_schema`
- `POLICY_SCHEMA_VERSION` for capsule policy
- `decapod constitution migrate` for embedded constitution graph

**Rollback:**
- Binary: `cargo install decapod@<prev-version>` or download prior release
- Workspace: `decapod workspace prune --force` + `git worktree remove`
- Config: Manual edit `.decapod/config.toml` (schema_version backwards compatible)## Capacity Planning

### Resource Limits
| Resource | Limit | Enforcement |
|----------|-------|-------------|
| SQLite connections | 1 writer + N readers (pool) | `db_pool` with busy_timeout |
| Workspace disk | Unbounded (user disk) | `decapod workspace prune` |
| Event log size | Unbounded | Manual archive via `decapod data archive` |
| Container memory | Host default | Docker `--memory` flag (not yet exposed) |
| Token budget (capsule) | Risk-tier max (4/6/12/20) | `CapsulePolicyBinding` enforcement |

### Scaling Triggers
- Multiple concurrent agents → container workspaces mandatory
- Large repos → `decapod validate` uses read-only DB connections
- Many todos → pagination in `todo list` (not yet implemented)## Logging

### Structured Logging (Internal)
- `tracing` + `tracing-subscriber` with JSON output
- Correlation IDs: `event_id` (ULID) per operation
- Session IDs: `session_id` from `session.acquire`
- Workspace IDs: git branch name (agent/todo_hash-timestamp)

### Audit Logs (Immutable)
- `broker.events.jsonl` — All mutations (The Thin Waist)
- `todo.events.jsonl` — Task lifecycle
- `proof.events.jsonl` — Proof execution
- `assurance_attestations.jsonl` — Interlock decisions
- `federation.events.jsonl` — Knowledge graph mutations

### Log Redaction
- `DECAPOD_SESSION_PASSWORD` never logged
- `.decapod/data/` paths logged but contents not
- Git tokens/credentials never in command args (use credential helper)## Secrets Management

| Secret | Source | Rotation | Consumer |
|--------|--------|----------|----------|
| `DECAPOD_SESSION_PASSWORD` | Per-agent env var | Per-session (acquire/release) | `session acquire`, broker auth |
| Git credentials | SSH agent / git credential helper | Standard git | `VcsWrite` actions |
| Container registry auth | `docker login` / config | Standard docker | `ContainerExec` build |
| Cargo registry token | `CARGO_REGISTRY_TOKEN` | Standard cargo | `ProofExec` publish |

**Policy**: No secrets in `.decapod/config.toml` (validated by `validate_project_config_toml` gate).## Security Testing

| Test Type | Cadence | Tooling |
|-----------|---------|---------|
| SAST | Every PR | `cargo clippy -- -D warnings`, `cargo deny` |
| Dependency Scan | Every PR + Weekly | `cargo audit`, `cargo deny check` |
| Container Scan | On image build | `docker scout` / `trivy` (optional) |
| Config Validation | Every `validate` run | Schema + forbidden keys gate |
| Fuzzing | Periodic | `cargo fuzz` (not yet configured) |## Compliance and Audit

### Regulatory Scope
- **Not in scope**: PCI, HIPAA, SOC2, FedRAMP (local-first dev tool)
- **Applicable**: Supply chain (SLSA Build L2 via cargo dist), audit trail completeness

### Audit Evidence Location
| Artifact | Path | Generated By |
|----------|------|--------------|
| Validation report | `.decapod/generated/artifacts/provenance/validation_report.json` | `validate` |
| Proof manifest | `.decapod/generated/artifacts/provenance/proof_manifest.json` | `proof run` |
| Artifact manifest | `.decapod/generated/artifacts/provenance/artifact_manifest.json` | `workspace publish` |
| Flight recorder | `decapod trace flight-recorder transcript` | `trace flight-recorder` |
| Broker audit | `decapod data broker audit` | `data broker audit` |
| Assurance attestations | `.decapod/generated/assurance_attestations.jsonl` | `assurance.evaluate` |

### Audit Trail Coverage
- Every mutation → `broker.events.jsonl` (verified by `data broker verify`)
- Every task claim/release → `todo.events.jsonl`
- Every proof run → `proof.events.jsonl`
- Every interlock → `assurance_attestations.jsonl`
- Every capsule write → `context_capsule` with policy lineage

### Exception Process
1. Document exception in `OVERRIDE.md` with justification
2. `decapod validate` will warn on override drift
3. Review at each promotion gate

<!-- decapod:capability-overlay:background-processing:start -->

## Background Processing Operations Overlay

### Queue Visibility
- Queue depth, processing rate, and latency MUST be monitored
- Dead letter queue MUST be visible and alerted
- Worker health and processing rate metrics required

### Shutdown Behavior
- Graceful shutdown: stop accepting new work, finish current job
- Drain behavior and timeout MUST be selected for the deployment
- Termination and requeue behavior MUST be selected and proven for the deployment

### Worker Health
- Worker liveness and readiness probes
- Queue depth alerts for backpressure detection
- Processing latency percentiles (p50, p95, p99)
<!-- decapod:capability-overlay:background-processing:end -->

<!-- decapod:capability-overlay:persistent-state:start -->

## Persistent State Operations Overlay

### Backup & Recovery
- Backup scope, schedule, retention, and restore evidence MUST be selected for the project
- Recovery point objectives MUST be explicit project decisions, not assumed values
- Recovery time objectives MUST be explicit project decisions, not assumed values
- Restore verification cadence MUST be recorded with the operational proof plan

### Migration Operations
- All schema changes via migration files
- Migration rollback procedures documented
- Zero-downtime migration strategy for production
- Migration health checks and rollback triggers
<!-- decapod:capability-overlay:persistent-state:end -->

## Pre-Promotion Security Checklist
- [ ] Threat model reviewed for changed surfaces (new CLI commands, RPC ops)
- [ ] Auth/authz tests pass (`session`, `workspace`, `capability` gates)
- [ ] Dependency vulnerability scan clean (`cargo audit`)
- [ ] No unresolved critical/high findings from `cargo audit`
- [ ] Config.toml validates (no forbidden secrets, schema current)
- [ ] Capsule policy `repo_revision` matches HEAD
- [ ] WorkUnit manifests `VERIFIED` with proof artifacts attached
- [ ] Provenance manifests present for `workspace publish`

## Strongest Operational Primitives
1. **Deterministic Rebuild**: `todo::rebuild_db_from_events` proves store integrity
2. **Flight Recorder**: Read-only timeline from all event logs
3. **Validation Gates**: Self-checking quality bar with auto-remediable errors
4. **Workspace Isolation**: Git worktrees + containers prevent environment corruption
5. **Capsule Lineage**: Policy hash + repo revision binding prevents context drift
6. **Attestation Trail**: Every interlock decision recorded with hash + touched paths

## Security Practices
- **Least Privilege**: Agents claim todo exclusively; containers run unprivileged; capabilities minimal
- **Input Validation**: All CLI args validated by clap; RPC params by serde + custom gates; config.toml schema enforced
- **Secure Storage**: No secrets in config.toml; session passwords in env only; event logs append-only
- **Defense in Depth**: Validation gates + capability gating + workspace isolation + session auth + audit trail

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `336fe4b382d8d0b6ca90712c17be972163d09aa42be30cba74058797e5e55e4d`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (90 files), `tests/` (4 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

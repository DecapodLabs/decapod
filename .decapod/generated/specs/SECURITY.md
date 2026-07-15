# Security

## Capability Security Boundaries

Authentication establishes the agent session; authorization and external-action capability checks independently constrain mutations such as Git, Cargo, and container execution. Declared repository capabilities are intent context only and never bypass session, policy, workspace, or proof gates. Secrets remain environment/session material and are not placed in `.decapod/config.toml`.

## Threat Model
```mermaid
flowchart LR
    A[Agent Process] --> B[decapod CLI]
    B --> C[Git Worktree]
    B --> D[Container Runtime]
    B --> E[SQLite Stores]
    B --> F[External Actions]
    F --> G[git]
    F --> H[docker/podman]
    F --> I[cargo]
    F --> J[gh CLI]
    B --> K[.decapod/ State]
    K --> L[User Store (~/.decapod)]
    K --> M[Repo Store (.decapod/data)]
    M --> N[broker.events.jsonl]
    M --> O[todo.events.jsonl]
    M --> P[proof.events.jsonl]
    M --> Q[federation.events.jsonl]
    R[Human] --> S[Main Repo]
    S -.-> C
```

### Trust Boundaries
1. **Agent ↔ Decapod**: JSON-RPC over stdin/stdout (same process trust)
2. **Decapod ↔ Git**: Capability-gated (`VcsRead`/`VcsWrite`), audit logged
3. **Decapod ↔ Container**: Capability-gated (`ContainerExec`), elevated perms required
4. **Decapod ↔ Cargo**: Capability-gated (`ProofExec`), runs in workspace
5. **Decapod ↔ Stores**: CLI-only access (jail rule), brokered mutations

## STRIDE Table

| Threat | Surface | Mitigation | Verification |
|--------|---------|------------|--------------|
| Spoofing | Agent session | `DECAPOD_SESSION_PASSWORD` per-agent, broker verification | `session status` shows agent_id |
| Tampering | Store mutation | Brokered writes only, event sourcing, deterministic rebuild | `data broker verify`, `todo rebuild` |
| Repudiation | Critical actions | Immutable JSONL event logs, attestation trail | `flight-recorder transcript` |
| Info Disclosure | Config/secrets | No secrets in config.toml, capability gating, .decapod/ jail | `validate` config gate |
| DoS | SQLite contention | WAL mode, 5s busy_timeout, 5 retries exponential backoff | `validate` storage preflight |
| EoP | Container escape | Elevated perms required, worktree isolation, no privileged containers | `workspace ensure --container` gate |
| EoP | Config injection | Schema validation, forbidden secret keys, auto-remediable errors | `validate` config gate |

## Authentication

### Agent Session
- **Identity Source**: `DECAPOD_SESSION_PASSWORD` environment variable (per-agent secret)
- **Token Lifetime**: Session acquired via `session acquire`, released via `session release` or process exit
- **Rotation/Revocation**: `session release` invalidates; re-acquire for new session
- **Verification**: Broker checks session token on mutating operations

### Human (CLI)
- **Identity Source**: Git author/committer + SSH keys for remote
- **Token Lifetime**: N/A (direct CLI invocation)
- **Elevation**: `sudo`/`doas` required for `workspace ensure --container`

## Authorization

### Role Model
| Role | Capabilities |
|------|--------------|
| Agent (session) | Todo CRUD, claim, workspace, context, validate, proof, capsule |
| Human (CLI) | All agent caps + init, clean, setup hooks, release, publish, prune |
| Container Runtime | Build/run workspace image (no host access) |

### Resource-Level Policy
| Resource | Read | Write | Admin |
|----------|------|-------|-------|
| User store (`~/.decapod/data`) | Agent | Agent (own) | Human |
| Repo store (`.decapod/data`) | Agent + Human | Agent (claimed) + Human | Human |
| Context capsules | Agent + Human | Agent (claimed task) | Human |
| WorkUnit manifests | Agent + Human | Agent (claimed task) | Human |
| Obligation graph | Agent + Human | Agent (claimed) | Human |
| Config.toml | Agent + Human | Human (init) | Human |

### Privilege Escalation Controls
- **Container**: Requires explicit elevated permissions request before `workspace ensure --container`
- **Store Mutation**: Only via Decapod CLI (jail rule: `.decapod/` files CLI-only)
- **External Actions**: Capability allowlist (`VcsRead`, `VcsWrite`, `ContainerExec`, `ProofExec`)
- **Config Secrets**: Forbidden keys rejected at validation (access_token, private_key, etc.)

## Data Classification

| Class | Examples | Storage | Access |
|-------|----------|---------|--------|
| Public | README, docs, specs, architecture | Repo (tracked) | Unrestricted |
| Internal | Todo titles, task metadata, knowledge entries | Repo store (`.decapod/data`) | Team agents + human |
| Sensitive | Session passwords, capability tokens | Env vars only (never persisted) | Per-agent only |
| Secrets | **Never stored** by Decapod | N/A | N/A |

## Sensitive Data Handling

### Encryption at Rest
- SQLite databases: **Not encrypted** (local filesystem, controlled access)
- Event logs: Plaintext JSONL (audit requirement)
- Config.toml: Plaintext (no secrets allowed by validation gate)

### Encryption in Transit
- Git: SSH/HTTPS (delegated to git CLI)
- Container: Local Docker socket (no network)
- RPC: stdin/stdout (local process boundary)

### Redaction in Logs
- `DECAPOD_SESSION_PASSWORD` never logged
- External action commands logged without env vars
- Attestation `input_hash`/`output_hash` are SHA256 (no plaintext)

### Retention + Deletion
| Data | Retention | Deletion |
|------|-----------|----------|
| Event logs | Permanent (audit) | `workspace prune` removes worktree data only |
| SQLite DBs | Permanent | Manual `rm -rf .decapod/data` |
| Context capsules | Until task verified | Auto-clean on `workunit transition Verified` |
| Session tokens | Session lifetime | `session release` |
| Worktrees | Active work | `workspace prune --force` |

## Supply Chain Security

### Recommended Scanners
```bash
# CI-integrated
cargo audit           # Advisory DB (crates.io)
cargo deny            # License, bans, sources
cargo vet             # Supply-chain auditing (manual)
```

### Dependency Update Cadence
- **Security advisories**: Immediate (cargo audit in CI on every PR)
- **Minor/patch**: Weekly via dependabot/renovate
- **Major**: Manual review (breaking changes)

### Signed Artifact / Provenance Strategy
- `cargo dist` generates signed checksums (GPG)
- `decapod release inventory` emits deterministic SBOM
- Provenance manifests: `artifact_manifest.json`, `proof_manifest.json`
- Lineage sync: `decapod release lineage-sync` normalizes policy hashes

## Secrets Management

| Secret | Source | Rotation | Consumer |
|--------|--------|----------|----------|
| `DECAPOD_SESSION_PASSWORD` | Agent environment | Per-session (acquire/release) | `session acquire`, broker auth |
| Git credentials | SSH agent / git credential helper | Standard git | `VcsWrite` actions |
| Container registry auth | `docker login` / config | Standard docker | `ContainerExec` build |
| Cargo registry token | `CARGO_REGISTRY_TOKEN` | Standard cargo | `ProofExec` publish |

**Forbidden in config.toml** (validated):
`access_token`, `refresh_token`, `device_code`, `auth_code`, `client_secret`, `session_cookie`, `account_password`, `private_key`, `supabase_key`, `supabase_url`

## Security Testing

| Test Type | Cadence | Tooling |
|-----------|---------|---------|
| SAST | Every PR | `cargo clippy -- -D warnings`, `cargo deny` |
| Dependency Scan | Every PR + Weekly | `cargo audit`, `cargo deny check` |
| Container Scan | On image build | `docker scout` / `trivy` (optional) |
| Config Validation | Every `validate` run | Schema + forbidden keys gate |
| Fuzzing | Periodic | `cargo fuzz` (not yet configured) |

## Compliance and Audit

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

## Pre-Promotion Security Checklist
- [ ] Threat model reviewed for changed surfaces (new CLI commands, RPC ops)
- [ ] Auth/authz tests pass (`session`, `workspace`, `capability` gates)
- [ ] Dependency vulnerability scan clean (`cargo audit`)
- [ ] No unresolved critical/high findings from `cargo audit`
- [ ] Config.toml validates (no forbidden secrets, schema current)
- [ ] Capsule policy `repo_revision` matches HEAD
- [ ] WorkUnit manifests `VERIFIED` with proof artifacts attached
- [ ] Provenance manifests present for `workspace publish`

## Strongest Security Primitives
1. **Capability-Gated External Actions**: Every `git`, `docker`, `cargo` call requires declared capability, logged with actor + operation
2. **CLI-Only Store Access**: `.decapod/` directory never accessed directly; all mutations via brokered CLI commands
3. **Event-Sourced Audit Trail**: Every mutation appended to JSONL with ULID, timestamp, actor, payload — deterministic rebuild proves integrity
4. **Session-Bound Agents**: Per-agent password prevents session hijacking; broker verifies on each mutating op
5. **Workspace Isolation**: Git worktrees + optional containers prevent agent/human environment interference
6. **Auto-Remediable Errors**: Validation failures include `agent_action` — agent can self-correct without human
7. **Deterministic Context Capsules**: SHA256 of canonicalized content + policy binding (risk tier, scopes, repo revision) prevents context injection

## Security Practices
- **Least Privilege**: Agents claim todo exclusively; containers run unprivileged; capabilities minimal
- **Input Validation**: All CLI args validated by clap; RPC params by serde + custom gates; config.toml schema enforced
- **Secure Storage**: No secrets in config.toml; session passwords in env only; event logs append-only
- **Defense in Depth**: Validation gates + capability gating + workspace isolation + session auth + audit trail

<!-- decapod:codebase-attestation:start -->
## Codebase Attestation

- Repository signal fingerprint: `3b6e857461916ca64233e814547602329c46dae403ffa58b14a379aac4dbf4a1`
- Significant implementation surfaces: `.github/` (8 files), `Cargo.lock/` (1 files), `Cargo.toml/` (1 files), `README.md/` (1 files), `docs/` (1 files), `src/` (86 files), `tests/` (3 files)
- Refreshed from the current codebase by `decapod specs.refresh`
<!-- decapod:codebase-attestation:end -->

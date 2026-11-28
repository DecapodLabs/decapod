# Decapod Trust, Credential Isolation, and Egress Inventory

**Status:** Normative Release Acceptance Contract (`DecapodLabs/decapod#1944`, `DecapodLabs/decapod#1933`)  
**Scope:** `DecapodLabs/decapod` CLI, core library (`src/decapod/core/*`), RPC, observability projections, Dactyl/Propodus cloud adapters, and release/CI adapters.

## 1. Permanent Trust Invariants

Every Decapod build, release artifact, and runtime surface MUST uphold all nine permanent trust invariants:

1. **Identity sovereignty** — Users retain sole ownership and control over their identities, keys, and local/cloud repository associations; no implicit cross-workspace or cross-account identity linking or migration occurs without explicit user action.
2. **No identity fingerprinting** — Never derive, hash, truncate, or correlate machine, user, third-party tool, or environment identifiers to bypass consent, egress, or isolation boundaries.
3. **Credential isolation** — Decapod must never read, hash, cache, store, log, or transmit third-party CLI/agent credentials (`~/<codex-home>/auth.json`, `~/.claude.json`, `~/.config/gh/hosts.yml`, etc.) or ambient secret environment tokens (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `STRIPE_SECRET_KEY`, `GITHUB_TOKEN`, `GH_TOKEN`, etc.) outside explicitly scoped, user-initiated operations; local `.decapod/` state and diagnostics remain strictly secret-free.
4. **Consent persistence** — Explicit opt-in/opt-out consent state and backend selection (`local` vs `cloud`, optional `repo.session_auto_update`, optional RPC/HTTP adapters) must persist durably across schema upgrades, migrations, config reloads, and binary restarts without silent re-enabling.
5. **Effective revocation** — Disabling or revoking any optional capability, cloud binding (`decapod cloud logout`, `decapod cloud revoke`, switching `backend = "local"`), or collection surface must take effect immediately, halt buffering/queued transmissions, purge pending outbound state, and remain stopped across subsequent invocations and restarts.
6. **Relay transparency** — Every outbound network path, relay, or hosted adapter (`Dactyl` / `Propodus` / optional RPC) must be explicitly declared in this inventory with exact initiating user action/config, endpoint class, HTTPS/no-redirect enforcement, payload category, principal, and retention owner; local CLI core defaults to zero non-loopback network egress.
7. **Distribution integrity** — Published binaries, container definitions, release metadata, and living-spec projections remain reproducible, pinned, and verifiable without covert post-install phone-home, update checks that leak host metadata, or unreviewed runtime downloads.
8. **Privacy contract integrity** — Public and in-repo privacy/documentation claims (`.decapod/config.toml`, `docs/`, `README.md`, `SECURITY.md`, `.decapod/managed/specs/`) must match observed binary runtime behavior under automated regression and negative-control testing; never claim unqualified "zero telemetry" where an opt-in hosted service exists, and never allow silent behavioral drift.
9. **Incident accountability** — Maintain deterministic audit trails, static + runtime egress/credential regression gates (`cargo test --test trust_egress_gate`, `trust_egress` unit tests), deliberate negative-control counter-tests, and documented incident response/remediation procedures so any violation fails CI immediately and has an auditable recovery path.

---

## 2. Complete Module, Local Store, and Outbound Egress Inventory

### 2.1 Local-First Default Surfaces (Zero Non-Loopback Network Egress)

| Surface / Module | Initiating Action / Config | Local Store / Retention Owner | Outbound Endpoint Class | Can Disable / Default |
|---|---|---|---|---|
| `decapod init` (`src/decapod/lib.rs`) | Default `backend = "local"` | `.decapod/config.toml`, `.decapod/data/decapod.db`, `.decapod/managed/*` | **None** | Default offline |
| `decapod docs ingest/get/show` (`src/decapod/core/docs_cli.rs`) | Embedded constitution & agent corpus | Read-only from compiled binary | **None** | Always offline |
| `decapod session acquire` (`src/decapod/lib.rs`) | Local session token issuance (`session_auto_update = false`) | Machine-local `XDG_CONFIG_HOME/decapod/sessions/` (or fallback `.decapod/managed/sessions/`) | **None** (zero `crates.io` check when `session_auto_update = false`) | Default `session_auto_update = false` |
| `decapod capabilities` (`src/decapod/core/rpc.rs`) | Local capability inspection (without `DECAPOD_CHECK_LATEST=1`) | Read-only compiled contract + `.decapod/config.toml` | **None** by default | Default offline (`DECAPOD_CHECK_LATEST=1` opt-in) |
| `decapod todo ...` (`src/decapod/core/todo.rs`, `dactyl_todo.rs`) | Local task & custody management (`backend = "local"`) | `.decapod/data/decapod.db` (SQLite via `dactyl-db`) | **None** | Default local |
| `decapod rpc` (stdio, local ops) (`src/decapod/core/rpc.rs`) | Local RPC ops (`agent.init`, `context.resolve`, `specs.refresh`, `observe.telemetry`, etc.) | `.decapod/data/decapod.db`, `.decapod/governance.json` | **None** | Default local |
| `decapod analyze --read-only` (`src/decapod/core/analyze.rs`) | Read-only static repository inspection | Zero disk mutation; excludes `.env*`, `*.pem`, `*.key`, `credentials*` | **None** | Always offline |
| `decapod observe telemetry` / `observability_projections/telemetry.rs` | `ExecutionTelemetryProjection` (`read_only = true`, `authoritative = false`) | Read-only projection from `.decapod/data/decapod.db` & `.decapod/governance.json`; redacted via `trace::redact_string` | **None** (strictly local-only; never auto-uploaded or exported) | Always local-only |
| `decapod validate --pre-build` (`src/decapod/core/pre_build_validate.rs`) | Read-only structural & manifest verification | Read-only `.decapod/*` | **None** | Always offline |
| `decapod govern ...` (`src/decapod/core/governance_document.rs`, `plan_governance.rs`) | Plan, trajectory, claims, checkpoints | `.decapod/governance.json`, `.decapod/data/decapod.db` | **None** | Always local |

### 2.2 Explicit Opt-In Outbound / Network Surfaces (Purpose-Limited)

Every non-local network call in Decapod MUST trace to an entry in this table and enforce HTTPS (loopback HTTP `127.0.0.1` / `[::1]` is permitted exclusively for explicit local IPC or `DECAPOD_TEST_MODE=1` test fixtures):

| Channel ID | Source Modules | Initiating User Action / Config | Endpoint / Origin Class | Transport & Redirect Policy | Payload Category & Principal | Storage / Retention Owner | Revocation / Disable Mechanism |
|---|---|---|---|---|---|---|---|
| `cloud_gateway` | `src/decapod/core/cloud_client.rs`, `cloud_admin.rs`, `cloud_dpop.rs`, `cloud_session.rs`, `cloud_workspace.rs`, `cloud_proof.rs`, `cloud_todo_operation.rs`, `container_signer.rs`, `propodus.rs`, `dactyl.rs` | `backend = "cloud"` in `.decapod/config.toml` OR explicit `decapod cloud ...` / `cloud.*` RPC | Default Dactyl origin (`https://...` via `dactyl_db::CloudConnector::default_origin()`), or `https://api.decapodlabs.ai` (`DECAPOD_CLOUD_FIRST_PARTY_ORIGIN=1`), or `https://*.supabase.co` when Supabase backend is configured, or `https://decapodlabs.ai/pricing` URL display; loopback only under `DECAPOD_TEST_MODE=1` | HTTPS enforced via `dactyl_db::CloudConnector`, `validate_egress_url`, `container_signer::extract_origin_relative_path`, & `validate_https_portal_url`; zero redirects; RFC 9449 DPoP proofs (`device_session.json`) or short-lived bearer for human admin ops | Repository-scoped tasks, governance proofs, org/member admin RPCs; repository `<owner>/<repo>` + Ed25519 device key `jkt` / user session | Machine-local `XDG_DATA_HOME/decapod/cloud-sessions/` + OS keystore (`cloud_dpop.rs`); zero credentials in `.decapod/` or Git | `decapod cloud logout`, `decapod cloud revoke`, or switching `backend = "local"` immediately stops cloud traffic and clears local sidecar custody |
| `http_rpc_loopback` | `src/decapod/core/http_transport.rs`, `observability_projections/server.rs` | Explicit `decapod rpc --transport http` or `decapod observe --serve` | Loopback `127.0.0.1` / `[::1]` by default (remote bind requires explicit `--allow-remote`) | Constant-time Bearer token auth, `Host` & `Origin` allowlist, revoked token ID list, read-only enforcement | Local RPC / read-only observability projections; local operator Bearer token | Process-volatile replay cache only (`DEFAULT_MAX_CACHE_ENTRIES = 256`) | Stop process or revoke token ID in `HttpTransportConfig.revoked_token_ids` |
| `crates_version_check` | `src/decapod/lib.rs`, `src/decapod/core/rpc.rs` | Explicit `[repo] session_auto_update = true` on `session acquire`, OR explicit `DECAPOD_CHECK_LATEST=1` on `decapod capabilities` | `https://crates.io/api/v1/crates/decapod` | HTTPS via `curl --connect-timeout 2` validated by `validate_egress_url` with non-identifying project User-Agent `decapod/<version> (https://github.com/DecapodLabs/decapod)` | Read-only package version metadata; no host/repo/user identifiers transmitted | Ephemeral stdout/stderr report | Set `session_auto_update = false` (default) and omit `DECAPOD_CHECK_LATEST` |
| `ci_snapshot_readback` | `src/decapod/core/snapshot_provider/public_buildkite.rs`, `snapshot_provider/mod.rs`, `github_artifacts.rs`, `read_authorization.rs`, `external_action.rs`, `workspace_publication.rs`, `validate.rs` | Explicit `decapod validate` (`check_gh_pr_exists` / GHA preflight / `proof.validation_buildkite_preflights = true`), `decapod qa verify commit`, or `decapod workspace publish` | `https://github.com/...`, `https://api.github.com/repos/...`, `https://buildkite.com/...`, `https://api.buildkite.com/v2/organizations/...`, `https://graphql.buildkite.com/v1`, `https://s3.amazonaws.com/buildkiteartifacts.com/...`, `https://buildkiteartifacts.com/...`, `https://buildkite-cloud.s3.amazonaws.com/...` | HTTPS only (`curl` invoked with `--proto =https --max-redirs 0` and isolated env; `gh` CLI only for explicit PR publication / `validate` PR existence checks, never during `validate --pre-build`) | Public CI status, build plans, and signed CI artifact bundles bound to candidate commit SHA | Local `.decapod/governance.json` & `.decapod/data/` validation evidence | Set `proof.validation_buildkite_preflights = false` / `validation_github_actions_preflights = false` or use `validate --pre-build` |
| `model_assets_download` | `src/decapod/core/model_assets.rs`, `laya_setup.rs` | Explicit user-initiated Laya model setup (`decapod context ...`) | `https://huggingface.co/...`, `https://api.github.com/repos/NandhaKishorM/laya/...` | HTTPS enforced (`validate_egress_url` + `--proto =https --proto-redir =https --noproxy *`), bounded byte limit | Model weights / tokenizer config; anonymous public download | Machine-local model cache | Omit explicit model download command |
| `derived_release_verify` | `src/decapod/core/derived_release_driver.rs` | Explicit `decapod release` derived-release verification | `https://crates.io/api/v1/crates/decapod`, `https://static.crates.io/crates/decapod/...` | HTTPS allowlist + `validate_egress_url` check | Release crate tarball digest verification | Local release proof bundle | Omit `decapod release` |

---

## 3. Consent, Revocation, Credential Isolation, and Anti-Fingerprinting Contracts

### 3.1 Consent Persistence & Effective Revocation
- `backend = "local"` and `session_auto_update = false` are the immutable defaults generated by `decapod init`.
- `.decapod/config.toml` persists backend and update opt-in choices across binary restarts, schema migrations, and `decapod init --proof` refreshes without silently resetting `local` to `cloud` or `session_auto_update = false` to `true`.
- Revoking consent:
  - Switching `backend = "cloud"` back to `backend = "local"` immediately stops all Dactyl/Propodus cloud client calls across all subsequent invocations and restarts.
  - Setting `session_auto_update = false` immediately stops `session acquire` update checks and `cargo install` actions across all subsequent invocations and restarts.
  - Running `decapod cloud logout` / `decapod cloud revoke` immediately purges machine-local `device_session.json` and `SessionStore` tokens.
  - `HttpTransportConfig.revoked_token_ids` immediately rejects revoked HTTP adapter tokens (`401 token_revoked`).

### 3.2 Credential Isolation & No Identity Fingerprinting
- Local workflows never open, stat-read, parse, hash, or log third-party credential files:
  - `~/<codex-home>/auth.json` (Codex CLI auth store)
  - `~/.claude.json`
  - `~/.config/gh/hosts.yml`
  - `~/.ssh/id_*` / `~/.netrc` / `~/.aws/credentials`
- Secret environment canaries (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `STRIPE_SECRET_KEY`, `GITHUB_TOKEN`, `GH_TOKEN`, `DECAPOD_CLOUD_TOKEN`, `PROPODUS_TOKEN`, `SUPABASE_SERVICE_ROLE_KEY`, etc.) are never persisted into `.decapod/`, emitted into `ExecutionTelemetryProjection`, or echoed in errors/traces (`trace::redact`, `trace::redact_string`, `cloud_admin::RedactedSecret`, `container_signer::filter_container_environment`).
- **No identity fingerprinting**: Decapod never hashes, truncates, or derives cross-service tracking fingerprints from third-party credentials, user emails, SSH keys, or machine IDs to circumvent consent or egress rules.

### 3.3 Privacy Contract Integrity
- Decapod Local (`backend = "local"`) performs **zero non-loopback outbound connections** during init, idle, `docs ingest`, `session acquire` (default), `capabilities` (default), `todo`, `govern`, `observe`, `analyze --read-only`, `validate --pre-build`, and local `rpc` commands.
- Decapod Cloud (`backend = "cloud"`) is an explicitly opted-in hosted coordination service; documentation never claims "zero network traffic" for Cloud, CI readback, or release download modes.

---

## 4. Incident Accountability & Verification Gates

- **Automated CI Gate (`tests/trust_egress_gate.rs` + `src/decapod/core/trust_egress.rs`)**:
  1. Executes the compiled `decapod` binary across local workflows under denied egress (network-intercepting `curl`/`gh` shims and proxy traps) and asserts zero non-loopback connection attempts.
  2. Plants synthetic third-party auth stores (`~/<codex-home>/auth.json`, `~/.claude.json`, `~/.config/gh/hosts.yml`) and secret env canaries (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `STRIPE_SECRET_KEY`, `GITHUB_TOKEN`), runs local workflows, and asserts neither raw values nor SHA-256 hashes appear in `.decapod/`, machine config/data dirs, stdout, or stderr.
  3. Verifies consent persistence across reloads and effective revocation across restarts.
  4. Runs deliberate negative-control counter-tests proving the trust guard fails on undeclared hosts, plain `http://` non-loopback endpoints, credential/fingerprint leaks, and simulated shim egress.
  5. Verifies static/inventory completeness between `src/` network callsites, `trust_egress::DECLARED_EGRESS_CHANNELS`, and this document.
- **Incident Response Procedure**:
  1. Any CI or audit failure blocks merge/release immediately.
  2. Contain and revoke affected sessions/keys (`decapod cloud revoke --all` / rotate affected tokens).
  3. Preserve forensic trace without exporting raw secrets, patch the violating callsite or inventory entry with explicit user consent & test coverage, re-run `cargo test --test trust_egress_gate` and `cargo audit`, and publish an advisory per `SECURITY.md`.

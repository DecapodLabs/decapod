# Quickstart

Follow this guide to initialize Decapod once in your repository and verify your first governed result from a real coding agent run.

> **Who runs what:** A **human operator** installs `decapod` and runs `decapod init` once per repository. After initialization, **your coding agent** (Codex, Claude Code, Cursor, Antigravity, or a custom harness) invokes the remaining commands automatically via the generated repository entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`).

---

<div aria-hidden="true"><span id="1-installation-human"></span></div>

## 1. Install the binary (Human)

Choose either prebuilt binary installation or building from source:

```bash
# Fastest when cargo-binstall is installed (requires Rust/Cargo + cargo-binstall):
cargo binstall decapod

# Or compile directly from crates.io with locked dependencies:
cargo install decapod --locked
```

**Local SQLite prerequisite:** Default local mode (`backend = "local"`) uses a host SQLite shared library via Dactyl. If `decapod` reports `LOCAL_SQLITE_RUNTIME_REQUIRED`, install your OS SQLite runtime or set `DACTYL_SQLITE_LIBRARY`; see [Local Dactyl SQLite prerequisite](reference/errors.md#local-dactyl-sqlite-prerequisite). Cloud-backed repositories do not require local SQLite.

---

<div aria-hidden="true"><span id="2-repository-initialization-human-once-per-repo"></span></div>

## 2. Initialize once per repository (Human)

Run `decapod init` from your repository root:

```bash
# Free local mode (SQLite in .decapod/data/decapod.db, offline, no signup):
decapod init

# Or Decapod Cloud (hosted team task coordination and repository-bound custody):
decapod init --backend cloud
```

Interactive terminal setup steps through **Project → Preferences → Review** to generate `.decapod/config.toml`, `.decapod/governance.json`, living spec templates under `.decapod/managed/specs/`, and agent entrypoints. Scripted setups use `decapod init with` or `--proof` (for config-only preparation without signing in or claiming readiness). Existing repo upgrades and legacy JSONL imports are documented in [Configuration: Legacy Upgrades](configuration.md#legacy-upgrades-and-config-backups).

For **Decapod Cloud** teams:
- Start a personal or team workspace at [Cloud Signup](https://decapodlabs.ai/signup) and review [Cloud project routing](reference/cloud-projects.md).
- Manage repository enrollment, seats, access policies, and organization administration in [Organization Admin](https://decapodlabs.ai/admin).
- Each teammate signs in per machine with `decapod cloud login`; credentials stay private, and missing authorization (`action_required` or `repository_denied`) fails closed without falling back to local SQLite (see [Decapod Cloud Boundary](reference/propodus.md)).

---

<div aria-hidden="true"><span id="3-prompt-gate--contract-ingestion-agent-invoked"></span><span id="4-agent-session-custody-agent-invoked"></span><span id="5-claiming-a-task-agent-invoked"></span><span id="6-entering-an-isolated-workspace-agent-invoked"></span><span id="7-decision-time-orientation--reflection-agent-invoked"></span></div>

## 3. Hand off to your coding agent

Once `decapod init` succeeds, open your coding agent in the repository and ask for the engineering outcome you want in ordinary language—for example:

```text
Fix the failing retry-backoff test.
```

(Or, for a UI/UX task: `Improve the terminal interface.`)

You do **not** need to append `"Read AGENTS.md, claim a task, write tests, update docs, commit, push, merge"` or operate Decapod commands manually. Your repository entrypoints tell the agent how to identify concrete improvements, perform the implementation, add or update meaningful tests, keep affected documentation and living specs aligned, repair failures, and deliver through the repository's configured endpoint—stopping at PR review by default or advancing autonomously only where explicitly configured in [Standing Project Delegation](configuration.md#standing-project-delegation).

At decision and commit boundaries, your agent invokes the kernel:

| Stage | Agent command | Purpose |
| --- | --- | --- |
| Prompt & contract gate | `printf '%s' '<prompt>' \| decapod eval --stdin --format json` & `decapod docs ingest` | Screen untrusted input (`"status": "allow"`) and load current binary contracts |
| Session & task claim | `decapod session acquire` & `decapod todo claim --id <task-id>` | Attribute custody and lease exclusive work ownership |
| Worktree isolation | `decapod workspace ensure --branch agent/<agent>/<task-id>-<topic>` | Keep `main`/`master` untouched (`--container` when `container_workspaces = true`) |
| Orientation | `decapod infer orientation --task-id <task-id>` & `decapod rpc --op context.resolve` | Load task constraints, living specs, and optional bounded reflection |
| Validation & proof | `decapod govern artifacts checkpoint ...` & `decapod validate` | Bind exact candidate `HEAD` to configured proof checks |

See [Single-Agent & Handoff Workflow](workflows/single-agent.md) and the [CLI Reference](reference/cli.md) for full flag contracts and mid-task agent handoffs.

---

<div aria-hidden="true"><span id="8-checkpoints-validation-and-proof-agent-invoked"></span></div>

## 4. First Governed Result Walkthrough

Here is what one currently supported, verifiable Decapod run produces in your repository—grounded in recorded runs on the [Proof page](https://decapodlabs.ai/proof.html), cross-host entrypoint observations in [`#880`](https://github.com/DecapodLabs/decapod/issues/880), and the end-to-end fixture in `tests/workunit_verified_proof_1660.rs` (target outcome-only defaults for full duty derivation, outcome-relevant test controls, and unattended push-through-merge are tracked in [`#1981`](https://github.com/DecapodLabs/decapod/issues/1981), [`#1982`](https://github.com/DecapodLabs/decapod/issues/1982), and [`#1983`](https://github.com/DecapodLabs/decapod/issues/1983) as target/in-progress):

1. **Isolated branch & active custody:** After claiming `<task-id>`, your agent works strictly inside `.decapod/workspaces/<worktree>` on branch `agent/<agent-id>/<task-id>-<topic>`, leaving the root checkout untouched.
2. **Authored intent & staged checkpoint:** Before committing edits, the agent updates code, tests, affected guidance, and `.decapod/managed/specs/` (`INTENT.md`, `ARCHITECTURE.md`, `INTERFACES.md`), records checks in `.decapod/governance.json` (`decapod govern trajectory record`), and runs:

```bash
git add <authored-paths>
decapod govern artifacts checkpoint --id <checkpoint-id> --summary "Implement fix and update specs"
git add .decapod/governance.json
git commit -m "Implement governed change"
```

3. **Validation binds to the exact `HEAD` commit:** The agent runs:

```bash
decapod validate
```

   - If a configured build, test, spec, or policy gate fails, `decapod validate` exits nonzero with typed blockers and remediation steps so the agent can repair the failure and revalidate before claiming completion.
   - **Stale-evidence invalidation:** If the agent fixes code and creates a new commit, any earlier green receipt for the previous commit immediately becomes `stale` and cannot admit `VERIFIED` completion or PR creation. The agent must run `decapod validate` again on the revised `HEAD`.
4. **Visible proof in the final result:** Once `decapod validate` passes for the exact current commit against the accepted target branch, the agent publishes or advances to the authorized endpoint (`decapod workspace publish` and `decapod todo done --id <task-id> --validated`) and reports the verifiable result:
   - Requested behavior and isolated branch/worktree (`agent/<agent-id>/<task-id>-<topic>`)
   - What changed across code, tests, affected documentation, and living specs
   - Which required checks actually ran and passed against exact candidate `HEAD`
   - Current accepted target branch, PR or merge state (`PR opened` vs `merged`), and any unresolved work or authority boundary

```bash
decapod workspace publish
decapod todo done --id <task-id> --validated
```

**Boundary of what proof establishes:** `decapod validate` proves that the configured repository gates, specs, custody checks, and tests actually executed and passed against the exact candidate `HEAD` commit (not fabricated `record-proof` claims). It does **not** guarantee semantic correctness beyond the configured checks and human review, nor physical confinement of unrestricted host tools. See [Proof & Verification Gates](concepts/proof.md).

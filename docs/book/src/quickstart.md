# Quickstart

Follow this guide to initialize Decapod once in your repository and verify your first governed result from a real coding agent run.

> **Who runs what:** Install Decapod where your coding agent runs. The repository administrator initializes the project once. Your agent reads the committed configuration and follows the repository entrypoints (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`).

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

# Or Decapod Cloud for a team:
decapod init --backend cloud
```

Interactive terminal setup steps through **Project → Preferences → Review** to generate `.decapod/config.toml`, `.decapod/governance.json`, living spec templates under `.decapod/managed/specs/`, and agent entrypoints. Scripted setups use `decapod init with` or `--proof` (for config-only preparation without signing in or claiming readiness). Existing repo upgrades and legacy JSONL imports are documented in [Configuration: Legacy Upgrades](configuration.md#legacy-upgrades-and-config-backups).

For **Decapod Cloud** teams:
- Start a personal or team workspace and subscription at [Cloud Signup](https://decapodlabs.ai/signup) (see [Cloud Pricing](https://decapodlabs.ai/pricing)).
- Manage repository enrollment, seats, access policies, and billing in [Organization Admin](https://decapodlabs.ai/admin).
- Contributors open the configured repository in their agent and supply the goal. The agent handles supported sign-in and repository routing. Cloud access failures stay explicit and preserve Local data. The [Cloud reference](reference/propodus.md) describes current support.

---

<div aria-hidden="true"><span id="3-prompt-gate--contract-ingestion-agent-invoked"></span><span id="4-agent-session-custody-agent-invoked"></span><span id="5-claiming-a-task-agent-invoked"></span><span id="6-entering-an-isolated-workspace-agent-invoked"></span><span id="7-decision-time-orientation--reflection-agent-invoked"></span></div>

## 3. Hand off to your coding agent

Once `decapod init` succeeds, open your coding agent in the repository and state your goal in plain language. You do not need to run kernel commands manually.

> Improve the terminal interface.

Give the agent the intended result and any constraints specific to the work. The agent investigates the interface, chooses an improvement it can verify, updates affected guidance, and addresses ordinary failures within the active task. The repository's standing delegation and current checks determine how far delivery can proceed. A review-required repository reaches its review boundary. A repository with standing authority for merge can proceed once the required checks and forge controls admit it.

See [Single-Agent & Handoff Workflow](workflows/single-agent.md) and the [CLI Reference](reference/cli.md) for the commands the agent invokes and the information it carries across a handoff.

---

<div aria-hidden="true"><span id="8-checkpoints-validation-and-proof-agent-invoked"></span></div>

## 4. First Governed Result Walkthrough

A first result should let you answer four questions from the repository and delivery record.

1. **What was requested?** Compare the original goal and its constraints with the behavior the agent changed.
2. **What changed?** Follow the candidate commit or PR to inspect the implementation and affected documentation.
3. **What was verified?** Read the relevant checks and their results. If a relevant test fails, the agent repairs the change and runs the required proof again against the new candidate and accepted target.
4. **Where did it land?** Check the observed endpoint. A published PR is available for review. A verified merge identifies the commit on the target branch. A release or deployment has its own artifact and readback.

This is the intended complete interaction. Host support and actual delivery vary with the repository's controls and the agent integration. The [recorded evidence](https://decapodlabs.ai/proof.html) describes observed runs. The [host qualification tracker](https://github.com/DecapodLabs/decapod/issues/880) tracks the zero-follow-up case through merge. The [proof reference](concepts/proof.md) explains which exact inputs a validation receipt covers.

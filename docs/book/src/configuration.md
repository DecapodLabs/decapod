# Configuration

Decapod is configured via `.decapod/config.toml` (see the [Config Specification Reference](reference/config-toml.md)). This file is human-editable and should be committed to your repository.

## The `[init]` Section

Controls how `decapod init` behaves.

- `specs`: (bool) Whether to generate spec scaffolding under `.decapod/managed/specs/`.
- `diagram_style`: ("ascii" or "mermaid") The style for generated architecture diagrams.
- `entrypoints`: (list) Which agent entrypoints to create (e.g., `["AGENTS.md", "CLAUDE.md"]`).

## The `[repo]` Section

Defines project-specific policy and metadata.

- `product_name`: The name of your project.
- `product_summary`: A short description of what the project does.
- `architecture_direction`: A high-level note on the architectural style (e.g., "modular monolith").
- `product_type`: (e.g., "library", "service", "application").
- `done_criteria`: Global "done" criteria that all tasks must satisfy.
- `primary_languages`: A list of languages used in the repo.
- `container_workspaces`: (bool) Whether to enforce container isolation for all work. **Recommended for multi-agent workflows.**

## Project Overrides

For deep behavioral changes, use `.decapod/OVERRIDE.md` (see [Config Overrides](concepts/overrides.md)). Find the generated directive section and replace the instruction inside its four-backtick source block with Markdown or any documentation style you prefer. Decapod extracts the block contents as authority while the outer file renders them as code. Duplicate or Decapod-namespaced unknown directive headings outside a body block fail closed instead of applying partial authority.

## Standing Project Delegation

Repository configuration determines where a coding agent stops after implementing a change—without requiring the human to repeat workflow commands in every prompt:

- **Human PR review (default):** `[governance].main_merge` defaults to `false` (omitted when false). After you describe an outcome, your agent claims a task, works in an isolated worktree, runs checks, updates affected documentation and living specs, checkpoints and commits, runs `decapod validate`, and opens or updates a pull request with `decapod workspace publish`, then waits for human review and merge.
- **Autonomous delivery through merge (explicit repository opt-in):** A human maintainer may set `main_merge = true` under `[governance]` in the accepted base repository's `.decapod/config.toml` (within the repository's branch-protection and CI rules). When accepted standing policy permits it, Decapod's merge-authorization gate (`evaluate_main_merge_authorization`) lets the agent advance through commit, push, PR checks, and merge without repeated approval prompts once exact-`HEAD` proof passes. A candidate workbranch cannot self-authorize `main_merge = true` (`POLICY_SELF_AUTHORIZATION_REJECTED`), and any base-branch movement or new commit invalidates prior readiness immediately.

**Currently demonstrated vs. target defaults:** Repository entrypoint activation (`AGENTS.md`, `CLAUDE.md`, `CODEX.md`, `GEMINI.md`), isolated task/worktree custody, exact-`HEAD` validation, stale-evidence invalidation, and recorded proof fixtures (`/proof.html`, `tests/workunit_verified_proof_1660.rs`, `#880`) are implemented today (cross-host unattended qualification remains in progress in `#880`). Target outcome-only defaults—automatically deriving ordinary UX/QA/engineering/docs duties without keyword triggers (`#1981`), enforcing outcome-relevant coverage and negative controls (`#1982`), and continuous autonomous continuation through the authorized endpoint (`#1983`)—are actively tracked work rather than universally enabled defaults.

## Legacy Upgrades and Config Backups

When upgrading an existing Decapod repository after installing a newer `decapod` binary:

- **Idempotent store migration:** `decapod init`, `decapod validate`, and other governed commands one-shot-import any residual legacy event JSONL into the canonical SQLite event store (`.decapod/data/decapod.db`) and move retired files under `.decapod/data/.retired-jsonl/`. Runtime writers never append to JSONL, and legacy SQLite stores recreated by an older binary are copied forward and removed.
- **Override preservation:** Existing `.decapod/OVERRIDE.md` body bytes are preserved while sections are rendered into fenced documentation source areas with visible authoring instructions; unsafe or ambiguous authority fails closed.
- **Non-destructive config refresh:** `decapod init --refresh` preserves existing authored settings as defaults, makes no state edits during `--dry-run`, and backs up changed configuration privately under `.decapod/data/config-backups/`. Agents run `decapod init --proof` during validation for non-interactive upgrade checks without authenticating Cloud credentials or asserting readiness.

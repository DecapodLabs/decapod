# DECIDE.md - Architecture Decision Prompting

**Authority:** interface (subsystem contract)
**Layer:** Plugins
**Binding:** Yes
**Scope:** curated engineering decision trees with SQLite-backed decision records and federation cross-links
**Non-goals:** replacing federation's decision nodes; decide is for structured upfront architecture questioning, not ad-hoc decision recording

---

## 1. Purpose

Decide gives agents **structured architecture prompting** — when a user describes a project ("make a calculator web app", "build a microservice"), the agent walks a curated decision tree to surface consequential engineering choices before writing code.

Each answered question is recorded as a durable decision record in SQLite, cross-linked into the federation memory graph. This produces an Architecture Decision Record (ADR) that persists across sessions and agents.

---

## 2. Store Model

Decision data lives under the selected Decapod store root:

- **Repo store:** `<repo>/.decapod/data/decisions.db`

No event log (decisions are point-in-time records, not event-sourced). Federation cross-links provide the audit trail.

**claim.decide.store_scoped**: Decision data exists only under the selected store root.

---

## 3. Decision Trees

Trees are embedded in the binary. Each tree targets a project archetype:

| Tree ID | Name | Questions | Keywords |
|---------|------|-----------|----------|
| `web-app` | Web Application | 6 | web, app, website, frontend, spa, dashboard |
| `microservice` | Microservice | 6 | microservice, service, api, backend, server |
| `cli-tool` | CLI Tool | 4 | cli, command, terminal, shell, tool |
| `library` | Library / Package | 4 | library, lib, crate, package, module, sdk |

### 3.1 Tree Structure

Each tree contains ordered questions. Each question has:

- **id** — machine-readable identifier (e.g., `runtime`, `framework`)
- **prompt** — human-readable question text
- **context** — brief explanation of why this decision matters
- **options** — curated list of choices, each with value, label, and rationale
- **depends_on / depends_value** — optional conditional: only shown if a prior answer matches

### 3.2 Conditional Questions

Questions may depend on prior answers. For example, in the `web-app` tree:
- `framework` (TypeScript frameworks) only appears if `runtime=typescript`
- `framework_wasm` (WASM frameworks) only appears if `runtime=wasm`

The `next` command resolves these conditionals automatically.

---

## 4. Schema

### 4.1 Sessions Table

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | TEXT PK | Yes | ULID (prefix: DS_) |
| `tree_id` | TEXT | Yes | Decision tree identifier |
| `title` | TEXT | Yes | Session title |
| `description` | TEXT | No | Optional description |
| `status` | TEXT | Yes | active, completed |
| `federation_node_id` | TEXT | No | Cross-link to federation.db |
| `created_at` | TEXT | Yes | Epoch seconds + 'Z' |
| `updated_at` | TEXT | Yes | Epoch seconds + 'Z' |
| `completed_at` | TEXT | No | When session was completed |
| `dir_path` | TEXT | Yes | Store root path |
| `scope` | TEXT | Yes | repo |
| `actor` | TEXT | Yes | Who created this session |

### 4.2 Decisions Table

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | TEXT PK | Yes | ULID (prefix: DD_) |
| `session_id` | TEXT FK | Yes | References sessions.id |
| `question_id` | TEXT | Yes | Question identifier within tree |
| `tree_id` | TEXT | Yes | Decision tree identifier |
| `question_text` | TEXT | Yes | Question prompt text |
| `chosen_value` | TEXT | Yes | Selected option value |
| `chosen_label` | TEXT | Yes | Selected option label |
| `rationale` | TEXT | No | Why this option was chosen |
| `user_note` | TEXT | No | Additional user notes |
| `federation_node_id` | TEXT | No | Cross-link to federation.db |
| `created_at` | TEXT | Yes | Epoch seconds + 'Z' |
| `actor` | TEXT | Yes | Who recorded this decision |

**claim.decide.no_duplicate_answers**: Each question can only be answered once per session.

---

## 5. Federation Integration

Every decision session and individual decision creates a corresponding federation node:

- **Session** creates a `decision` node with `priority: notable`
- **Each answer** creates a `decision` node with `priority: background`, linked to the session node via a `depends_on` edge

This connects the architecture decision record to the broader memory graph, making decisions discoverable through `decapod data federation list --type decision`.

**claim.decide.federation_cross_linked**: Active sessions have a corresponding federation node.

---

## 6. Agent Workflow

The expected agent flow when handling a project creation prompt:

```
1. Agent analyzes user prompt
2. decapod decide suggest --prompt "user's prompt"     # Get tree suggestion
3. decapod decide start --tree <id> --title "..."      # Create session
4. Loop:
   a. decapod decide next --session <id>               # Get next question
   b. Present options to user                          # Agent surfaces the question
   c. decapod decide record --session <id> ...         # Record answer
5. decapod decide complete --session <id>              # Finalize
```

Agents SHOULD use `suggest` to match the prompt to a tree. Agents MUST present each question's options and rationale to the user, not make choices autonomously.

---

## 7. CLI Contract

All commands under `decapod decide`.

| Command | Description |
|---------|-------------|
| `trees` | List all available decision trees |
| `suggest --prompt P` | Score trees against a user prompt |
| `start --tree T --title T` | Start a new decision session |
| `next --session ID` | Get the next unanswered question (resolves conditionals) |
| `record --session ID --question Q --value V` | Record a decision |
| `complete --session ID` | Mark session as completed |
| `list [--session ID] [--tree T]` | List recorded decisions |
| `get --id ID` | Get a specific decision |
| `session list [--status S]` | List sessions |
| `session get --id ID` | Get session with all its decisions |
| `init` | Initialize decisions.db (no-op if exists) |
| `schema` | Print JSON schema |

Output: all commands emit JSON for machine consumption.

---

## 8. Validation Gates

| Gate ID | Check | Claim |
|---------|-------|-------|
| `decide.store_scoped` | decisions.db exists only under store root | claim.decide.store_scoped |
| `decide.no_duplicates` | No duplicate question answers within a session | claim.decide.no_duplicate_answers |
| `decide.federation_linked` | Active sessions have federation node references | claim.decide.federation_cross_linked |

---

## 9. Override

Projects can customize the decide subsystem through `.decapod/OVERRIDE.md`:

```markdown
### plugins/DECIDE.md

## Custom Trees
Projects may define additional domain-specific decision trees by extending
the decide plugin. Use `decapod feedback propose` to request new trees.

## Mandatory Questions
If your project requires specific decisions to be made before any code is written,
document them here. Agents should check for active decision sessions before
beginning implementation work.

## Decision Policies
- All new projects MUST have a completed decision session before implementation
- Decisions may be superseded by starting a new session for the same tree
```

---

## 10. Security

- All access through DbBroker (serialized, audited)
- Federation cross-links provide provenance trails
- Actor field enables per-agent audit
- Duplicate detection prevents answer overwrites

---

## Links

- `core/PLUGINS.md` — Subsystem registry
- `plugins/FEDERATION.md` — Memory graph (cross-linked)
- `plugins/APTITUDE.md` — Preference system (complementary)
- `interfaces/STORE_MODEL.md` — Store semantics

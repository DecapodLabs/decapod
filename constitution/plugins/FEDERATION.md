# FEDERATION.md - Governed Agent Memory (Knowledge Graph)

**Authority:** interface (subsystem contract)
**Layer:** Plugins
**Binding:** Yes
**Scope:** typed memory objects with provenance, lifecycle, and knowledge graph edges
**Non-goals:** replacing knowledge subsystem; federation is for cross-session continuity, not code-level rationale

---

## 1. Purpose

Federation gives agents **governed memory** — typed, provenance-tracked, lifecycle-aware memory objects that survive across sessions. Memory objects are claims, not truth: each carries metadata that lets consumers assess reliability, freshness, and lineage.

Biological metaphor: in decapod crustaceans, the brain sets policy while regional ganglia run autonomous local loops. Federation nodes are the ganglia — typed objects with their own status and relationships — governed by Decapod's control plane.

---

## 2. Store Model

Federation data lives under the selected Decapod store root:

- **User store:** `~/.decapod/data/federation.db` + `federation.events.jsonl`
- **Repo store:** `<repo>/.decapod/data/federation.db` + `federation.events.jsonl`

No mixing. No cross-store references. Store boundaries are hard.

**claim.federation.store_scoped**: Federation data exists only under the selected store root.

---

## 3. Node Types

| Type | Semantics | Critical | Example |
|------|-----------|----------|---------|
| `decision` | Architectural or process choice | Yes | "Use event-driven architecture" |
| `commitment` | Promise with deadline or stakeholder | Yes | "Ship v2 by March" |
| `person` | Human or agent identity + role | No | "Sarah — CTO, primary stakeholder" |
| `preference` | Style, tooling, or workflow preference | No | "Prefers dark mode, tab width 4" |
| `lesson` | Post-mortem or operational insight | No | "Never deploy on Fridays" |
| `project` | Project scope and context | No | "Hale Pet Door migration" |
| `handoff` | Session boundary context transfer | No | "Left off at PR #142 review" |
| `observation` | Compressed session note | No | "Discussed auth refactor with team" |

Critical types (`decision`, `commitment`) have additional write-safety rules (see §6).

---

## 4. Schema

### 4.1 Nodes Table

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | TEXT PK | Yes | ULID |
| `node_type` | TEXT | Yes | One of: decision, commitment, person, preference, lesson, project, handoff, observation |
| `status` | TEXT | Yes | active, superseded, deprecated, disputed |
| `priority` | TEXT | Yes | critical, notable, background |
| `confidence` | TEXT | Yes | human_confirmed, agent_inferred, imported |
| `title` | TEXT | Yes | Short descriptive title |
| `body` | TEXT | Yes | Markdown content (the claim) |
| `scope` | TEXT | Yes | repo, user |
| `tags` | TEXT | No | Comma-separated |
| `created_at` | TEXT | Yes | ISO 8601 epoch seconds + 'Z' |
| `updated_at` | TEXT | Yes | ISO 8601 epoch seconds + 'Z' |
| `effective_from` | TEXT | No | When this claim became valid |
| `effective_to` | TEXT | No | When this claim expired (null = still active) |
| `dir_path` | TEXT | Yes | Store root path |
| `actor` | TEXT | Yes | Who created this node |

### 4.2 Sources Table

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | TEXT PK | Yes | ULID |
| `node_id` | TEXT FK | Yes | References nodes.id |
| `source` | TEXT | Yes | Scheme-prefixed pointer (file:, url:, cmd:, commit:, event:) |
| `created_at` | TEXT | Yes | ISO 8601 epoch seconds + 'Z' |

**claim.federation.provenance_required_for_critical**: Nodes with `priority=critical` OR `node_type in {decision, commitment}` MUST have at least one source with a valid scheme prefix.

### 4.3 Edges Table

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | TEXT PK | Yes | ULID |
| `source_id` | TEXT FK | Yes | References nodes.id (from) |
| `target_id` | TEXT FK | Yes | References nodes.id (to) |
| `edge_type` | TEXT | Yes | relates_to, depends_on, supersedes, invalidated_by |
| `created_at` | TEXT | Yes | ISO 8601 epoch seconds + 'Z' |
| `actor` | TEXT | Yes | Who created this edge |

---

## 5. Event Model

All mutations append to `federation.events.jsonl` (append-only, never truncated).

### 5.1 Event Envelope

| Field | Type | Description |
|-------|------|-------------|
| `event_id` | TEXT | ULID |
| `ts` | TEXT | ISO 8601 epoch seconds + 'Z' |
| `event_type` | TEXT | Operation type (see §5.2) |
| `node_id` | TEXT | Target node ID (null for edge-only ops) |
| `payload` | JSON | Operation-specific data |
| `actor` | TEXT | Who triggered this |

### 5.2 Event Types

| Event Type | Description | Allowed For |
|-----------|-------------|-------------|
| `node.create` | New node | All types |
| `node.edit` | Modify non-critical fields (title, body, tags, priority) | Non-critical types only |
| `node.supersede` | Transition node to `superseded`, create supersedes edge | All types |
| `node.deprecate` | Transition node to `deprecated` | All types |
| `node.dispute` | Transition node to `disputed` | All types |
| `edge.add` | Add edge between nodes | All |
| `edge.remove` | Remove edge | All |
| `source.add` | Add provenance source to node | All |

**claim.federation.append_only_critical**: Critical types (`decision`, `commitment`) do not support `node.edit`. To change a critical node, supersede it with a new node.

---

## 6. Write-Safety Rules

1. **Provenance gate**: Critical nodes require `sources[]` at creation time. Rejected otherwise.
2. **No in-place edit for critical types**: Use `supersede` to create a replacement.
3. **Status transitions are one-way**: `active → superseded|deprecated|disputed`. No reversal. Create a new node instead.
4. **Actor is mandatory**: Every event records who wrote it.
5. **Supersession atomicity**: `supersede` creates the edge AND transitions the old node in one operation.

---

## 7. Lifecycle Semantics

```
active ──→ superseded  (via node.supersede)
active ──→ deprecated  (via node.deprecate)
active ──→ disputed    (via node.dispute)
```

No backwards transitions. `supersedes` edges must form a DAG (no cycles).

**claim.federation.lifecycle_dag_no_cycles**: The supersedes edge graph contains no cycles.

---

## 8. CLI Contract

All commands under `decapod data federation`.

| Command | Description |
|---------|-------------|
| `add` | Create a new node (with sources for critical types) |
| `get --id ID` | Retrieve a single node with its sources and edges |
| `list [--type T] [--status S] [--priority P] [--scope S]` | List nodes with filters |
| `search --query Q` | Text search across title and body |
| `edit --id ID [--title T] [--body B] [--tags T]` | Edit non-critical node fields |
| `supersede --id OLD --by NEW` | Supersede old node with new one |
| `deprecate --id ID --reason R` | Mark node deprecated |
| `link --source ID --target ID --type T` | Add typed edge |
| `unlink --id EDGE_ID` | Remove edge |
| `graph --id ID [--depth N]` | Show node neighborhood |
| `rebuild` | Deterministic rebuild from events |
| `schema` | Print JSON schema |

Output: all commands support `--format json` (default for agents) and `--format text`.

---

## 9. Validation Gates

| Gate ID | Check | Claim |
|---------|-------|-------|
| `federation.store_purity` | federation.db and events.jsonl exist only under store root | claim.federation.store_scoped |
| `federation.provenance` | All critical nodes have ≥1 valid source | claim.federation.provenance_required_for_critical |
| `federation.write_safety` | No `node.edit` events for critical types in event log | claim.federation.append_only_critical |
| `federation.lifecycle_dag` | No cycles in supersedes edges | claim.federation.lifecycle_dag_no_cycles |

---

## 10. Security

- All access through DbBroker (serialized, audited)
- Provenance prevents hallucination anchors (can't store a "decision" without citing where it came from)
- Append-only event log enables tamper detection
- Actor field enables per-agent audit trails
- Critical types can't be overwritten — only superseded with full lineage

---

## Links

- `core/PLUGINS.md` — Subsystem registry
- `interfaces/CLAIMS.md` — Claims ledger
- `interfaces/STORE_MODEL.md` — Store semantics
- `plugins/KNOWLEDGE.md` — Knowledge subsystem (complementary, not competing)
- `methodology/MEMORY.md` — Memory doctrine

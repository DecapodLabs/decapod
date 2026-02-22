# Knowledge Store Specification

## 1. Decision

Adding a **Knowledge Store** to Decapod - a repo-native, three-memory-system architecture for persistent agent knowledge with explicit provenance, directional flow enforcement, and deterministic verification.

### Scope Boundaries
- **In scope**: Semantic memory (knowledge), procedural memory (team skills), episodic memory (self-calibration)
- **Out of scope**: Live session state, runtime agent context, external KB integration
- **Invariant protected**: All canonical knowledge lives in-repo under `.decapod/knowledge/` or `constitution/`

---

## 2. Spec

### A. Folder Layout

```
.decapod/knowledge/                    # Canonical knowledge store (repo-scoped)
├── semantic/                          # Durable facts/concepts
│   ├── v1/                           # Versioned schema
│   │   ├── entities/                 # Entity definitions (JSONL)
│   │   ├── relationships/            # Relationship graph (JSONL)
│   │   └── provenance/               # Provenance ledger (JSONL)
│   └── v2/                           # Future schema migrations
├── procedural/                       # Team skills / methodology
│   ├── v1/
│   │   ├── commit_norms/             # Commit best practices
│   │   ├── pr_expectations/          # PR templates/checklists
│   │   ├── user_expectations/        # Definition of done
│   │   └── risk_tiers/              # Risk classification
│   └── provenance/
├── episodic/                         # Agent calibration / learnings
│   ├── v1/
│   │   ├── friction_ledger.jsonl     # Operational friction observations
│   │   └── calibration/             # Agent behavior patterns provenance/
├── .
│   └──index/                          # Knowledge index (SQLite)
├── .lock                           # Write lock (semantic/procedural)
└── VERSION                         # Schema version file

constitution/interfaces/
├── KNOWLEDGE_STORE.md              # This spec
├── PROCEDURAL_NORMS.md            # Team skills examples
└── MEMORY_SYSTEMS.md              # Architecture overview
```

**Justification**:
- `.decapod/knowledge/` ensures repo-scoped canonicality (not user-scoped)
- Versioned subdirs (`v1/`) enable schema migration without breaking readers
- Separate `semantic/`, `procedural/`, `episodic/` enforces hard memory separation
- Provenance ledger in each subsystem enables full audit trail

### B. File Formats

**All formats**: JSONL (line-delimited JSON) for append-only ledgers + SQLite index

**Schema versioning**: Semver in `VERSION` file + prefix on each entry

**Naming conventions**:
- Entries: `{type}.{id}.jsonl` (e.g., `norm.commit.001.jsonl`)
- Provenance: `provenance/{timestamp}.jsonl`
- Index: `.index/knowledge.db` (SQLite)

### C. Provenance Model

Every semantic/procedural entry MUST cite:
- `evidence_type`: `"commit" | "pr" | "doc" | "test" | "transcript"`
- `evidence_ref`: commit hash, PR number, doc path, test artifact, or transcript hash
- `cited_by`: agent ID that created the entry
- `cited_at`: epoch timestamp

**Provenance is append-only**: never modify history, only add new citations.

### D. Promotion-Relevant vs Advisory

| Artifact Type | Promotion-Relevant | Advisory-Only |
|--------------|---------------------|---------------|
| `procedural/commit_norms/*` | ✅ Yes | |
| `procedural/pr_expectations/*` | ✅ Yes | |
| `procedural/user_expectations/*` | ✅ Yes | |
| `semantic/entities/*` | | ✅ Advisory |
| `episodic/friction_ledger/*` | | ✅ Advisory |

**Gate rule**: Promotion gates (PR merge, release) must verify procedural norms are satisfied.

---

## 3. CLI/Skill Surfaces (Implemented)

### Currently Implemented

```bash
# Add knowledge entry (requires provenance)
decapod data knowledge add \
  --id "entity.my-feature" \
  --title "My Feature" \
  --text "Description of the feature" \
  --provenance "commit:abc123" \
  [--claim-id "todo-123"]

# Search knowledge base
decapod data knowledge search --query "authentication"
```

### Planned (Aspirational)

```bash
# Digestion pipeline phases
decapod knowledge reduce --sources <paths>
decapod knowledge reflect
decapod knowledge reweave --entry <id> --evidence <ref>
decapod knowledge verify
decapod knowledge archive --older-than <days>

# Friction ledger
decapod friction record --type tool_error|redo|validation_fail --context <json>
decapod friction report

# Homeostasis
decapod health report
decapod health review --thresholds
```

### Input/Output Artifacts

| Command | Input | Output |
|---------|-------|--------|
| `reduce` | Source files (docs, commits, PRs) | `.decapod/knowledge/{type}/staging/` |
| `reflect` | Staging + canonical | Contradiction report JSON |
| `reweave` | Entry ID + new evidence | Updated entry + provenance |
| `verify` | All knowledge | Pass/fail + errors JSON |
| `archive` | Timestamp filter | Moved to `.decapod/knowledge/archive/` |
| `friction record` | Tool context JSON | `.decapod/knowledge/episodic/friction_ledger.jsonl` |
| `health report` | None | `.decapod/knowledge/.health/latest.json` |
| `health review` | Health report | `.decapod/knowledge/.review/proposal.json` (if thresholds trip) |

---

## 4. Validation Gates (Promotion-Binding)

| Gate | What It Checks | Fail Behavior |
|------|---------------|---------------|
| `knowledge.schema` | All entries match JSON schema | Reject write |
| `knowledge.provenance` | Every entry has valid evidence_ref | Reject write |
| `knowledge.links` | Semantic links resolve to existing entities | Warn (advisory) |
| `knowledge.staleness` | No procedural norms older than 90 days | Warn + flag for review |
| `knowledge.contradictions` | No contradictory procedural norms | Block promotion |
| `episodic.no_backflow` | Friction ledger never directly enters semantic/procedural | Block + reject |

**Only procedural memory is promotion-blocking**: semantic and episodic are advisory.

---

## 5. Tests

### Test 1: Schema + Canonicalization Stability

```rust
// tests/knowledge/stability.rs
#[test]
fn test_semantic_schema_stability() {
    // Add entry, read back, verify unchanged
    let entry = serde_json::json!({
        "id": "entity.test.001",
        "type": "entity",
        "schema_version": "1.0.0",
        "name": "TestEntity",
        "description": "A test entity",
        "provenance": [{
            "evidence_type": "commit",
            "evidence_ref": "abc123",
            "cited_by": "agent-test",
            "cited_at": 1700000000
        }]
    });
    let output = run_decapod(&dir, &["knowledge", "add", "--type", "semantic", "--content", &entry.to_string()]);
    assert!(output.status.success());
    
    // Read back and verify canonical form
    let read = run_decapod(&dir, &["knowledge", "show", "entity.test.001"]);
    let parsed: Value = serde_json::from_str(&read.stdout).unwrap();
    assert_eq!(parsed["id"], "entity.test.001");
}
```

### Test 2: Provenance Enforcement

```rust
// tests/knowledge/provenance.rs
#[test]
fn test_provenance_required_for_procedural() {
    // Try to add procedural norm without evidence
    let entry = serde_json::json!({
        "id": "norm.commit.001",
        "type": "commit_norm",
        "rule": "Use conventional commits",
        // Missing provenance!
    });
    let output = run_decapod(&dir, &["knowledge", "add", "--type", "procedural", "--norm-type", "commit", "--content", &entry.to_string()]);
    assert!(!output.status.success());
    assert!(output.stderr.contains("provenance required"));
}
```

### Test 3: Directional Flow Enforcement (No Backflow)

```rust
// tests/knowledge/directional_flow.rs
#[test]
fn test_friction_cannot_directly_enter_procedural() {
    // Record friction
    run_decapod(&dir, &["friction", "record", "--type", "validation_fail", "--context", r#"{"test":"fail"}"#]);
    
    // Try to promote friction to procedural norm directly - should fail
    let output = run_decapod(&dir, &["knowledge", "promote", "--from", "episodic/friction", "--to", "procedural"]);
    assert!(!output.status.success());
    assert!(output.stderr.contains("directional flow violation"));
}
```

---

## 6. Migration Plan

### Phase 0: Already Implemented (v0.30+)
- [x] `knowledge.db` SQLite store under `.decapod/data/`
- [x] `decapod data knowledge add` command (requires provenance)
- [x] `decapod data knowledge search` command
- [x] Decay/TTL mechanism for stale entries
- [x] Provenance field on entries

### Phase 1: Foundation (this PR)
- [x] Specification documentation
- [x] Provenance claims in CLAIMS.md
- [ ] **TODO**: Update spec to reflect implemented vs aspirational

### Phase 2: Query & Retrieval (future)
- [ ] Rich search with filters (by provenance, date, status)
- [ ] Retrieval feedback logging
- [ ] Relevance scoring

### Phase 3: Governance Gates (future)
- [ ] `knowledge.verify` command
- [ ] Provenance enforcement gate (reject entries without evidence)
- [ ] Schema validation gate

### Phase 4: Advanced Features (future)
- [ ] Friction ledger
- [ ] Health report
- [ ] Digestion pipeline (reduce/reflect/reweave)

---

## 7. Guardrails (One-Line Constraints)

1. **No backflow**: Episodic → Semantic/Procedural requires explicit promotion artifact + human approval
2. **Provenance mandatory**: Every procedural entry needs evidence_ref
3. **Schema first**: All writes validated against JSON schema before disk
4. **Promotion-blocking only procedural**: Semantic/episodic are advisory only
5. **Versioned schemas**: Never break readers; migrate via `vN/` directories
6. **Immutable provenance**: Never modify history; only append new citations
7. **Threshold-triggered, not cron**: Homeostasis loops fire on state, not schedule

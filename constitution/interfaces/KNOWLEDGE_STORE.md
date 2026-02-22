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

## 3. CLI/Skill Surfaces

### Core Commands

```bash
# Knowledge management
decapod knowledge add --type semantic --evidence <ref> --content <json>
decapod knowledge add --type procedural --norm-type commit|pr|expectation --evidence <ref> --content <json>
decapod knowledge list --type semantic [--limit N]
decapod knowledge list --type procedural [--norm-type commit]
decapod knowledge show <id>

# Digestion pipeline phases (each runs with fresh context)
decapod knowledge reduce --sources <paths>           # Parse sources into atomic norms
decapod knowledge reflect                           # Link, dedupe, detect contradictions
decapod knowledge reweave --entry <id> --evidence <ref>  # Update with new evidence
decapod knowledge verify                           # Schema + provenance + link integrity
decapod knowledge archive --older-than <days>      # Move to archive, preserve provenance

# Friction ledger
decapod friction record --type tool_error|redo|validation_fail --context <json>
decapod friction report                            # Emit friction summary

# Homeostasis triggers
decapod health report                              # Session-start health check
decapod health review --thresholds                # Emit review proposal if thresholds trip
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

## 6. Migration Plan (Smallest Incremental Steps)

### Phase 1: Foundation (v0.1)
- [ ] Create `.decapod/knowledge/` folder structure
- [ ] Add `VERSION` file with schema version
- [ ] Implement `decapod knowledge add` (minimal: just write JSONL)
- [ ] Implement `decapod knowledge list` (read JSONL)
- [ ] Add provenance field to entry schema (required)
- **Gate**: `knowledge.provenance` - reject entries without evidence

### Phase 2: Procedural Memory (v0.2)
- [ ] Define commit_norm, pr_expectation, user_expectation schemas
- [ ] Add example entries (5-10)
- [ ] Implement `decapod knowledge verify` (schema + provenance check)
- **Gate**: `knowledge.schema` + `knowledge.provenance`

### Phase 3: Friction Ledger (v0.3)
- [ ] Implement `decapod friction record`
- [ ] Implement `decapod friction report`
- [ ] Add episodic memory folder
- **Gate**: `episodic.no_backflow` (preliminary check)

### Phase 4: Homeostasis (v0.4)
- [ ] Implement `decapod health report` (session start)
- [ ] Define thresholds in JSON config
- [ ] Implement `decapod health review` (slow loop)
- **Gate**: `knowledge.staleness` (warning only)

### Phase 5: Full Digestion Pipeline (v0.5)
- [ ] Implement `reduce`, `reflect`, `reweave`, `archive`
- [ ] Add contradiction detection
- **Gate**: `knowledge.contradictions` (blocking)

---

## 7. Guardrails (One-Line Constraints)

1. **No backflow**: Episodic → Semantic/Procedural requires explicit promotion artifact + human approval
2. **Provenance mandatory**: Every procedural entry needs evidence_ref
3. **Schema first**: All writes validated against JSON schema before disk
4. **Promotion-blocking only procedural**: Semantic/episodic are advisory only
5. **Versioned schemas**: Never break readers; migrate via `vN/` directories
6. **Immutable provenance**: Never modify history; only append new citations
7. **Threshold-triggered, not cron**: Homeostasis loops fire on state, not schedule

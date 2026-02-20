# Implementation Task Plan: ObligationNode Evolution

## Executive Summary

This task plan outlines the evolution of Decapod's TODO subsystem into a governance-native ObligationNode system with strict adherence to the guardrail that no state influencing promotion may live outside the repo-scoped, versioned, proof-validated store.

## Canonical Storage Choice

**ObligationNode will use the existing `governance.db` with STATE_COMMIT integration as the canonical storage location.**

### Why This is Promotion-Safe:
1. **Repo-Scoped**: Lives within the git repository, not in user-specific directories
2. **Versioned**: All state changes recorded with commit SHAs and timestamps
3. **Promotion-Bound**: State directly influences promotion gates and validation
4. **Proof-Validated**: State undergoes cryptographic verification before promotion
5. **Existing Infrastructure**: Leverages existing STATE_COMMIT v1 protocol and governance system

## Task Dependencies and Order

Tasks must be completed in strict order due to dependencies:

1. ✅ **Task 1**: Define ObligationNode data model and schema
2. 🔄 **Task 2**: Implement ObligationNode storage  
3. 🔄 **Task 3**: Add dependency closure validation
4. 🔄 **Task 4**: Create ObligationNode CLI interface
5. 🔄 **Task 5**: Add ObligationNode to validation gates
6. 📋 **Task 6**: Update documentation and migration
7. 🧪 **Task 7**: Add comprehensive tests
8. 🔄 **Task 8**: Remove legacy TODO subsystem

## Detailed Task Specifications

### Task 1: Define ObligationNode Data Model and Schema

**Objective**: Create canonical ObligationNode schema with dependencies, proofs, and completion state

**Files to Modify/Create**:
- `src/core/obligation.rs` - New ObligationNode implementation
- `src/lib.rs` - Add ObligationNode module exports
- `src/plugins/obligation.md` - Documentation for ObligationNode subsystem
- `src/data/obligation.json` - JSON schema for ObligationNode
- `src/data/obligation_events.json` - Event schema for state transitions

**Data Model Schema**:
```rust
struct ObligationNode{
    id: String,                    // Unique identifier
    title: String,                // Human-readable title
    description: String,          // Detailed description
    dependencies: Vec<String>,    // IDs of required obligations
    proofs: Vec<ProofRequirement>, // Required proof surfaces
    status: ObligationStatus,     // pending, active, completed, blocked
    created_at: DateTime<Utc>,    // Creation timestamp
    updated_at: DateTime<Utc>,    // Last update timestamp
    completed_at: Option<DateTime<Utc>>, // Completion timestamp if completed
    assigned_to: Option<String>,  // Agent responsible (optional)
    category: String,             // Classification category
    priority: PriorityLevel,      // Priority level
}

enum ObligationStatus {
    Pending,      // Ready to be worked on
    Active,       // Currently being worked on
    Completed,    // Successfully completed
    Blocked,      // Dependencies not satisfied
}

struct ProofRequirement {
    id: String,           // Unique proof identifier
    description: String,  // What the proof verifies
    command: String,      // Command to run for verification
    expected_output: String, // Expected output for success
}
```

**Validation Invariants**:
1. **Dependency Acyclicity**: No circular dependencies allowed
2. **Proof Completeness**: All required proofs must be defined
3. **Status Consistency**: Status transitions must follow valid state machine
4. **Repo-Scoped Storage**: All ObligationNode data stored in repo-scoped location only
5. **STATE_COMMIT Integration**: All state changes must be STATE_COMMIT validated

**Tests to Add**:
- Unit tests for schema validation
- Dependency cycle detection tests
- Status transition validation tests
- STATE_COMMIT integration tests
- Proof requirement validation tests

**Definition of Done**:
- Schema compiles without errors
- All validation invariants implemented
- Basic unit tests pass
- Can create, read, update, delete ObligationNode instances
- All data stored in repo-scoped location only
- `decapod validate` passes with new schema

### Task 2: Implement ObligationNode Storage

**Objective**: Create repo-scoped storage for ObligationNode with STATE_COMMIT integration

**Files to Modify/Create**:
- `src/data/schemas.rs` - Add ObligationNode database schema
- `src/core/obligation.rs` - Implement storage layer
- `src/core/state_commit.rs` - Integrate with STATE_COMMIT v1

**Database Schema**:
```sql
CREATE TABLE IF NOT EXISTS obligations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT DEFAULT '',
    dependencies TEXT NOT NULL, -- JSON array of obligation IDs
    proofs TEXT NOT NULL, -- JSON array of proof requirements
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    completed_at TEXT,
    assigned_to TEXT,
    category TEXT NOT NULL DEFAULT '',
    priority TEXT NOT NULL DEFAULT 'medium'
);

CREATE TABLE IF NOT EXISTS obligation_edges (
    edge_id TEXT PRIMARY KEY,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'depends_on',
    created_at TEXT NOT NULL,
    UNIQUE(from_id, to_id),
    FOREIGN KEY(from_id) REFERENCES obligations(id) ON DELETE CASCADE,
    FOREIGN KEY(to_id) REFERENCES obligations(id) ON DELETE CASCADE
);
```

**Validation Invariants**:
1. **Repo-Scoped Only**: No user-scoped storage allowed for canonical data
2. **STATE_COMMIT Required**: All mutations must generate STATE_COMMIT artifacts
3. **Atomic Operations**: All state changes must be atomic and verifiable
4. **Audit Trail**: Complete event sourcing for all state transitions

**Tests to Add**:
- Storage layer integration tests
- STATE_COMMIT generation and verification tests
- Atomicity and rollback tests
- Event sourcing validation tests

**Definition of Done**:
- ObligationNode storage layer implemented
- STATE_COMMIT v1 integration working
- All operations are atomic and verifiable
- Event sourcing captures all state transitions
- `decapod validate` passes with storage integration

### Task 3: Add Dependency Closure Validation

**Objective**: Implement acyclic graph validation and dependency blocking before promotion

**Files to Modify/Create**:
- `src/core/obligation.rs` - Add dependency validation logic
- `src/core/validate.rs` - Integrate dependency checks into validation gates

**Validation Logic**:
1. **Cycle Detection**: Prevent circular dependencies using DFS algorithm
2. **Dependency Resolution**: Ensure all dependencies are satisfied before completion
3. **Status Blocking**: Automatically set status to `Blocked` when dependencies unsatisfied
4. **Promotion Gates**: Block promotion when dependencies are incomplete

**Validation Invariants**:
1. **Acyclic Graph**: No circular dependencies allowed
2. **Dependency Resolution**: All dependencies must be `Completed` before parent can complete
3. **Status Consistency**: Status automatically updates based on dependency state
4. **Promotion Safety**: Cannot promote when dependencies are blocked

**Tests to Add**:
- Cycle detection tests (positive and negative cases)
- Dependency resolution tests
- Status transition tests
- Promotion gate tests

**Definition of Done**:
- Dependency closure validation implemented
- Cycle detection working correctly
- Status automatically updates based on dependencies
- Promotion gates block when dependencies are incomplete
- `decapod validate` passes dependency validation

### Task 4: Create ObligationNode CLI Interface

**Objective**: Implement CLI commands for ObligationNode management (add, list, complete, etc.)

**Files to Modify/Create**:
- `src/lib.rs` - Add ObligationNode CLI commands
- `src/core/obligation.rs` - Implement CLI handlers
- `decapod` - Update CLI entrypoint

**CLI Commands**:
```
decapod obligation add --title "..." --description "..." --depends-on "id1,id2" --proofs "proof1,proof2"
decapod obligation list
decapod obligation get --id <id>
decapod obligation complete --id <id> --commit <sha>
decapod obligation verify --id <id>
decapod obligation dependencies --id <id>
```

**Validation Invariants**:
1. **Command Validation**: All CLI inputs must be validated
2. **Permission Checks**: Only authorized agents can modify obligations
3. **State Consistency**: CLI operations maintain data integrity
4. **Audit Trail**: All CLI operations logged with actor information

**Tests to Add**:
- CLI command integration tests
- Input validation tests
- Permission enforcement tests
- Audit trail verification tests

**Definition of Done**:
- Complete CLI interface implemented
- All commands work correctly
- Input validation and permission checks in place
- Audit trail captures all operations
- `decapod validate` passes CLI integration

### Task 5: Add ObligationNode to Validation Gates

**Objective**: Integrate ObligationNode state into promotion validation

**Files to Modify/Create**:
- `src/core/validate.rs` - Add ObligationNode validation rules
- `src/core/state_commit.rs` - Integrate with promotion gates

**Validation Rules**:
1. **Dependency Satisfaction**: All dependencies must be `Completed`
2. **Proof Verification**: All required proofs must pass
3. **STATE_COMMIT Required**: State commit must be present for completed obligations
4. **Status Consistency**: Obligation status must match actual state

**Validation Invariants**:
1. **Promotion Blocking**: Cannot promote when obligations are blocked
2. **Proof Enforcement**: All required proofs must pass before completion
3. **State Consistency**: Obligation state must be consistent with git state
4. **Audit Requirements**: All state changes must be audit-logged

**Tests to Add**:
- Promotion gate validation tests
- Proof verification integration tests
- State consistency tests
- Audit trail validation tests

**Definition of Done**:
- ObligationNode integrated into promotion validation
- All validation rules enforced
- Promotion gates block when obligations are incomplete
- `decapod validate` passes with ObligationNode validation

### Task 6: Update Documentation and Migration

**Objective**: Document ObligationNode system and migrate existing TODOs

**Files to Modify/Create**:
- `plugins/obligation.md` - Complete documentation
- `README.md` - Update with ObligationNode information
- Migration scripts for existing TODOs

**Documentation Requirements**:
1. **System Architecture**: Explain ObligationNode design
2. **Usage Guide**: CLI commands and workflows
3. **Validation Rules**: Explain promotion gates and validation
4. **Migration Guide**: How to migrate from TODO to ObligationNode

**Migration Requirements**:
1. **Data Migration**: Convert existing TODOs to ObligationNodes
2. **State Preservation**: Maintain task history and relationships
3. **Validation**: Ensure migrated data passes all validation
4. **Rollback**: Provide rollback capability if needed

**Tests to Add**:
- Documentation validation tests
- Migration script tests
- Data integrity tests

**Definition of Done**:
- Complete documentation written
- Migration scripts implemented and tested
- Existing TODOs successfully migrated
- Documentation passes validation
- `decapod validate` passes with migrated data

### Task 7: Add Comprehensive Tests

**Objective**: Create unit and integration tests for ObligationNode system

**Files to Modify/Create**:
- `tests/obligation_tests.rs` - Unit tests for ObligationNode
- `tests/integration/obligation_integration_tests.rs` - Integration tests
- `tests/golden/obligation_golden_tests.rs` - Golden tests

**Test Categories**:
1. **Unit Tests**: Individual function and method tests
2. **Integration Tests**: End-to-end workflow tests
3. **Validation Tests**: Promotion gate and validation tests
4. **Performance Tests**: Large-scale obligation management tests
5. **Security Tests**: Permission and audit tests

**Test Coverage Requirements**:
- 100% code coverage for core logic
- All validation rules tested
- All CLI commands tested
- Integration with STATE_COMMIT tested
- Performance under load tested

**Definition of Done**:
- All unit tests passing
- All integration tests passing
- Code coverage meets requirements
- Performance benchmarks acceptable
- `decapod validate` passes all tests

### Task 8: Remove Legacy TODO Subsystem

**Objective**: Deprecate and remove old TODO system after migration

**Files to Modify/Create**:
- `src/core/todo.rs` - Mark as deprecated
- `decapod` - Remove TODO CLI commands
- `README.md` - Update documentation
- Migration cleanup scripts

**Deprecation Process**:
1. **Mark as Deprecated**: Add deprecation warnings to TODO code
2. **Migration Complete**: Ensure all TODOs migrated to ObligationNode
3. **CLI Removal**: Remove TODO commands from CLI
4. **Documentation Update**: Update all references
5. **Cleanup**: Remove TODO code and schemas

**Validation Invariants**:
1. **No Data Loss**: All TODO data migrated successfully
2. **No Breaking Changes**: Existing workflows continue to work
3. **Clean Removal**: TODO code completely removed
4. **Documentation Updated**: All references updated

**Tests to Add**:
- Migration completeness tests
- CLI removal tests
- Documentation validation tests

**Definition of Done**:
- TODO subsystem completely removed
- All TODO data migrated to ObligationNode
- No TODO references remain in codebase
- `decapod validate` passes without TODO subsystem
- Documentation fully updated

## Guardrail Enforcement

Throughout all tasks, the following guardrail must be strictly enforced:

**No state that can influence promotion may live outside the repo-scoped, versioned, proof-validated store.**

This means:
- All ObligationNode data must be stored in `governance.db`
- No user-scoped storage for canonical data
- All state changes must generate STATE_COMMIT artifacts
- All operations must be verifiable and audit-able

## Non-Negotiable Constraints

- **No UI work**: Focus on core data model and validation
- **No background sync**: All operations are synchronous and verifiable
- **Completion is derived**: Never assert completion, always prove it
- **Dependency closure**: Must be enforceable (acyclic graph, required proofs mapped to executable proof surfaces)
- **STATE_COMMIT**: Emit/require immutable STATE_COMMIT for completion, not human "complete" flag

## Validation Strategy

Each task must pass `decapod validate` before proceeding to the next task. The validation must include:
- Schema validation
- State consistency checks
- Promotion gate validation
- Audit trail verification
- Performance benchmarks

## Success Criteria

Project succeeds when:
1. ObligationNode system is fully implemented
2. All TODOs successfully migrated
3. Promotion gates properly enforce obligation completion
4. All tests pass
5. Documentation is complete
6. Legacy TODO subsystem removed
7. `decapod validate` passes completely
8. No state influencing promotion lives outside repo-scoped storage

---

**Decision ID**: ARCH-2026-02-20-OBLIGATION-EVOLUTION
**Status**: Implementation Plan Ready
**Authority**: `specs/INTENT.md` (Intent-Driven Engineering Contract)
**Validation**: Must pass `decapod validate` at each task boundary
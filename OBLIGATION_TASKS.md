# ObligationNode v0 - Governance-Complete Implementation

## Task Breakdown

### Task 1: Enforce Derived Completion
- **Objective**: Remove user-settable status, implement validator that recomputes status from dependencies/proofs/commit
- **Files touched**: `src/core/obligation.rs`
- **Validation impact**: `derive_obligation_status` function recomputes status; status field becomes read-only
- **Tests required**: Unit tests for status derivation (missing dependency, missing proof, missing commit)
- **Definition of Done**: Status can only be derived via `derive_obligation_status()`, no direct status updates

### Task 2: Add Graph Validation to Validator
- **Objective**: Integrate obligation graph validation into `decapod validate`
- **Files touched**: `src/core/validate.rs`, `src/core/obligation.rs`
- **Validation impact**: Adds validation gate that fails if any obligation is not derivably satisfied
- **Tests required**: Integration tests for validation gate (pass/fail scenarios)
- **Definition of Done**: `decapod validate` checks all obligations and fails if not satisfied

### Task 3: Add Proof Surface Integration Tests
- **Objective**: Test proof mapping to executable proof surfaces
- **Files touched**: `tests/plugins/obligation.rs` (new)
- **Validation impact**: Ensures missing/failing proofs block completion
- **Tests required**: Test cases for missing proof, failing proof, passing proof
- **Definition of Done**: All proof scenarios tested and validated

### Task 4: Add Golden Test Fixtures
- **Objective**: Create minimal test fixtures for obligation graphs
- **Files touched**: `tests/fixtures/obligation/` (new directory)
- **Validation impact**: Deterministic test coverage for graph scenarios
- **Tests required**: Fixture files for valid graph, dependency chain, cycle detection
- **Definition of Done**: Fixtures cover all key scenarios, tests pass

### Task 5: Add Promotion Gate Integration
- **Objective**: Integrate obligation closure into promotion logic
- **Files touched**: `src/core/state_commit.rs`, `src/core/validate.rs`
- **Validation impact**: Promotion fails if referenced obligations not validated
- **Tests required**: Integration tests for promotion failure/success paths
- **Definition of Done**: Promotion deterministically respects obligation state

### Task 6: Add Comprehensive Unit Tests
- **Objective**: Full test coverage for obligation system
- **Files touched**: `tests/plugins/obligation.rs` (new)
- **Validation impact**: All code paths tested
- **Tests required**: Unit tests for all public functions
- **Definition of Done**: >80% code coverage, all tests pass

### Task 7: Add CLI Validation Command
- **Objective**: Add `decapod obligation validate` command
- **Files touched**: `src/core/obligation.rs`
- **Validation impact**: CLI interface for graph validation
- **Tests required**: CLI integration tests
- **Definition of Done**: Command works and returns correct status

### Task 8: Final Validation and Documentation
- **Objective**: Ensure all phases complete, add docs
- **Files touched**: `plugins/obligation.md` (new)
- **Validation impact**: Final validation that all requirements met
- **Tests required**: Full test suite passes
- **Definition of Done**: All tasks complete, `decapod validate` passes

## Execution Order

1. Task 1 - Enforce Derived Completion
2. Task 2 - Add Graph Validation to Validator  
3. Task 3 - Add Proof Surface Integration Tests
4. Task 4 - Add Golden Test Fixtures
5. Task 5 - Add Promotion Gate Integration
6. Task 6 - Add Comprehensive Unit Tests
7. Task 7 - Add CLI Validation Command
8. Task 8 - Final Validation and Documentation

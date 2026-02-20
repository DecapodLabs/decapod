# Architecture Memo: Filesystem Task Abstraction Decision

## Executive Summary

After thorough analysis of Decapod's current architecture and task management capabilities, I recommend **against** adopting a filesystem-backed task abstraction similar to Claude's approach. Decapod's existing TODO subsystem provides a more sophisticated, governance-safe, and scalable solution that better aligns with the system's intent-driven engineering principles.

## Why User-Scoped State Cannot Participate in Promotion Semantics

### Canonical Storage Definition

**Only repo-scoped, versioned, promotion-bound storage is canonical:**

1. **Versioned**: All state changes are recorded with commit SHAs and timestamps
2. **Repo-Scoped**: State lives within the git repository, not in user-specific directories
3. **Promotion-Bound**: State directly influences promotion gates and validation
4. **Proof-Validated**: State undergoes cryptographic verification before promotion

### User Store Limitations

User-scoped state (in `~/.decapod`) is appropriate for:
- Runtime configuration and preferences
- Agent-specific context and memory
- Temporary working state
- Derived data that doesn't affect promotion

But user store **cannot** be used for:
- Task definitions that influence promotion
- State that affects multi-agent coordination
- Data that determines whether code can be promoted
- Any state that bypasses validation gates

### Store Purity Principle

The fundamental principle: **No state that can influence promotion may live outside the repo-scoped, versioned, proof-validated store.**

This ensures:
- Consistent promotion semantics across all agents
- Auditable state transitions
- Reproducible promotion decisions
- Clear boundaries between canonical and runtime state

### Decapod's Existing Task System

Decapod already implements a comprehensive task management system through the TODO subsystem (`plugins/TODO.md`) with the following capabilities:

**Event-Driven Architecture:**
- Complete audit trails for all task state transitions
- Multi-agent coordination with category ownership and heartbeats
- Rich metadata including priorities, tags, dependencies, and ownership
- State lifecycle: `pending` → `active` → `done` → `archived`

**Governance Features:**
- Explicit claiming before work begins (`decapod todo claim --id <task-id>`)
- Store purity preventing cross-contamination between user and repo stores
- Proof-first validation through `decapod validate` gates
- Cryptographic STATE_COMMIT v1 for workspace integrity

**Multi-Agent Support:**
- Category-based expertise registration
- Shared and exclusive claim modes
- Automatic task assignment based on agent capabilities
- Heartbeat-based stale ownership detection

### Dependency Closure Analysis

### Current TODO Subsystem Evaluation

**Question**: Does current TODO subsystem enforce dependency closure before promotion?

If **yes**:
- TODO already provides the obligation graph functionality needed
- Enhancement should focus on performance and usability
- No filesystem integration needed for core functionality

If **no**:
- TODO must evolve into ObligationNode system
- Dependency blocking must be enforced before promotion
- Filesystem integration is a distraction from core evolution

### ObligationNode Concept

Transform TODO into a proof-addressable obligation graph:
- Each task becomes an ObligationNode with dependencies
- Promotion requires all dependent obligations to be satisfied
- State transitions are cryptographically verified
- Multi-agent coordination respects dependency closure

This evolution addresses the core gap without introducing filesystem complexity.

A filesystem-backed task abstraction typically involves:
- Task files stored as individual files in a directory structure
- Task metadata encoded in file headers or separate metadata files
- Task state managed through filesystem operations (create, rename, delete)
- Simple text-based task representation

## Critical Risks of Filesystem Integration

### 1. Shadow Workflow Layer
Allowing user-scoped filesystem storage for task data creates a shadow workflow layer where:
- User-backed storage can diverge from repo-backed state
- Agents may work with inconsistent task views
- Promotion gates may validate against different state than what agents see
- The "optional" nature makes it a slippery slope to dependency

### 2. Store Purity Erosion
If filesystem integration is allowed:
- `--store user` validation may pass while repo promotion semantics differ
- User state becomes a source of truth for promotion decisions
- The clear boundary between canonical and runtime state blurs
- Multi-agent coordination becomes unreliable due to state divergence

### 3. TODO Subsystem Calcification
Rather than evolving the TODO subsystem:
- Filesystem integration becomes a workaround that prevents proper evolution
- The core gap (dependency closure before promotion) remains unsolved
- TODO becomes a legacy system that can't be properly modernized
- Architectural debt accumulates instead of being addressed

### Advantages of Current TODO System

1. **Governance Compliance**: Aligns with Decapod's intent-driven engineering principles
2. **Multi-Agent Coordination**: Built-in support for concurrent agent work
3. **Audit Trail**: Complete event sourcing provides accountability
4. **Validation Integration**: Native integration with `decapod validate` gates
5. **Store Purity**: Prevents contamination between user and repo states
6. **Rich Metadata**: Supports complex task relationships and dependencies
7. **Proof-First**: Built on cryptographic verification foundations

### Disadvantages of Filesystem-Backed Approach

1. **Governance Violation**: Direct filesystem manipulation bypasses Decapod's control plane
2. **No Multi-Agent Support**: File-based systems don't handle concurrent agent coordination
3. **Limited Metadata**: Text files struggle to represent complex task relationships
4. **No Validation Integration**: Filesystem operations bypass proof gates
5. **Store Purity Issues**: Risk of contaminating canonical state with runtime data
6. **Simplicity vs. Sophistication**: Over-simplification of complex engineering workflows

### Proof/Signal Analysis

### Red Flag Indicators

* If filesystem integration can be toggled off without changing promotion semantics, it's redundant.
* If promotion logic ever reads from non-versioned user state, that's a red flag.
* If multi-agent concurrency still lacks dependency-closure blocking, the core gap remains unsolved.

### Validation Criteria

* All promotion-relevant state must be versioned, repo-scoped, and proof-validated
* User store may contain derived state but cannot influence promotion decisions
* Filesystem storage, if considered, must be canonical and promotion-gated, not optional
* TODO subsystem must enforce dependency closure before allowing promotion

**Current System Benefits:**
- Already battle-tested and production-ready
- Integrated with existing validation and proof systems
- Supports Decapod's multi-agent architecture
- Maintains consistency with intent-driven development

**Filesystem Approach Risks:**
- Would require significant refactoring of existing workflows
- Introduces potential for state corruption and inconsistencies
- Removes governance controls and audit capabilities
- Creates divergence from Decapod's architectural principles

## Recommendation

**Tighten Store Purity Boundaries**

### Rationale:

1. **Kernel Protection**: Prevent shadow workflow layers that erode promotion semantics
2. **Store Purity Enforcement**: Maintain strict separation between canonical and runtime state
3. **Promotion Integrity**: Ensure all promotion-relevant state is versioned and proof-validated
4. **Governance Discipline**: No state that can influence promotion may live outside the repo-scoped store
5. **Architectural Clarity**: Clear boundaries prevent slow drift into inconsistent semantics

### Enhancement Opportunities:

Focus on evolving the existing TODO subsystem rather than adding filesystem integration:

1. **ObligationNode Evolution**: Transform TODO into proof-addressable obligation graph
2. **Dependency Closure**: Ensure TODO enforces dependency blocking before promotion
3. **Canonical Storage**: Maintain repo-scoped, versioned, promotion-bound storage as the only canonical source
4. **User Store Boundaries**: Clearly define why user-scoped state cannot participate in promotion semantics
5. **Performance Optimization**: Optimize existing system without compromising governance boundaries

## Implementation Path

### Phase 1: Assessment and Planning
- Document current TODO subsystem capabilities and limitations
- Identify specific pain points in current task management
- Define success criteria for any enhancements

### Phase 2: Filesystem Integration Research
- Investigate filesystem-backed storage options
- Design hybrid storage architecture
- Define migration path for existing tasks

### Phase 3: Implementation
- Implement filesystem persistence layer as optional backend
- Add export/import capabilities for interoperability
- Enhance performance and scalability of existing system

### Phase 4: Validation and Rollout
- Comprehensive testing of hybrid approach
- Gradual migration for users who want filesystem storage
- Maintain backward compatibility

## Conclusion

The filesystem-backed task abstraction approach, while simple and appealing, would represent a significant step backward for Decapod's sophisticated, governance-aware task management system. The current TODO subsystem provides the right balance of simplicity, governance, and scalability for Decapod's intent-driven engineering environment.

However, the "optional filesystem integration" compromise is rejected. Filesystem integration that can influence promotion semantics must be canonical and promotion-gated, not optional.

**Recommendation: Maintain current TODO subsystem, evolve it into ObligationNode system with dependency closure enforcement, and reject optional filesystem integration that creates shadow workflows.**

**Guardrail**: No state that can influence promotion may live outside the repo-scoped, versioned, proof-validated store.

This protects the kernel from slow drift while addressing the core architectural gap through proper evolution rather than layering integration.

---

**Decision ID**: ARCH-2026-02-20-TASK-ABSTRACTION-TIGHTENED
**Status**: Recommended
**Authority**: `specs/INTENT.md` (Intent-Driven Engineering Contract)
**Validation**: Must pass `decapod validate` before implementation

**Guardrail**: No state that can influence promotion may live outside the repo-scoped, versioned, proof-validated store.
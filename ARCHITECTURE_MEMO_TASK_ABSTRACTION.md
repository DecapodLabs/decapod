# Architecture Memo: Filesystem Task Abstraction Decision

## Executive Summary

After thorough analysis of Decapod's current architecture and task management capabilities, I recommend **against** adopting a filesystem-backed task abstraction similar to Claude's approach. Decapod's existing TODO subsystem provides a more sophisticated, governance-safe, and scalable solution that better aligns with the system's intent-driven engineering principles.

## Current State Analysis

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

### Filesystem-Backed Task Abstraction (Claude-style)

A filesystem-backed task abstraction typically involves:
- Task files stored as individual files in a directory structure
- Task metadata encoded in file headers or separate metadata files
- Task state managed through filesystem operations (create, rename, delete)
- Simple text-based task representation

## Decision Analysis

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

### Technical Debt Considerations

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

**Maintain and Enhance Current TODO Subsystem**

### Rationale:

1. **Architectural Consistency**: The TODO system aligns with Decapod's intent-driven engineering philosophy
2. **Governance Safety**: Maintains the control plane separation between agents and system internals
3. **Multi-Agent Scalability**: Supports the sophisticated coordination patterns Decapod requires
4. **Proof Integration**: Native integration with validation and cryptographic verification
5. **Future-Proof**: Can be enhanced with filesystem-backed storage if needed, without changing the API

### Enhancement Opportunities:

Rather than replacing the TODO system, consider these enhancements:

1. **Filesystem Persistence Layer**: Add optional filesystem-backed storage for TODO data
2. **Export/Import Capabilities**: Support importing/exporting tasks to/from filesystem formats
3. **Hybrid Approach**: Use filesystem for task storage while maintaining governance through the control plane
4. **Performance Optimization**: Optimize the existing system for large-scale task management

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

Instead of replacing the existing system, focus on enhancing it with filesystem integration capabilities that maintain the governance controls and multi-agent coordination features that make Decapod unique.

**Recommendation: Maintain current TODO subsystem, add filesystem integration as enhancement rather than replacement.**

---

**Decision ID**: ARCH-2026-02-20-TASK-ABSTRACTION
**Status**: Recommended
**Authority**: `specs/INTENT.md` (Intent-Driven Engineering Contract)
**Validation**: Must pass `decapod validate` before implementation
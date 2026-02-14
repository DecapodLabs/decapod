# GAPS.md - Gap Analysis & Systemic Improvement Methodology

**Authority:** guidance (systematic gap identification and routing methodology)
**Layer:** Guides
**Binding:** No
**Scope:** how to identify, categorize, and route gaps in Decapod-managed systems
**Non-goals:** replacing TODO system, substituting for proof, or defining authoritative requirements

⚠️ **CRITICAL: Gap analysis is continuous intelligence work, not one-time audits.** ⚠️

This document defines the practice of systemic gap identification: finding what's missing, misaligned, or underdeveloped in the system, and routing those findings to the appropriate subsystems for resolution.

---

## 1. What Is a Gap

A **gap** is any delta between:
- **Current state** (what exists)
- **Required state** (what must exist)
- **Desired state** (what should exist for optimal performance)

Gaps are not bugs. Bugs are deviations from spec. Gaps are *missing or incomplete specifications, implementations, or capabilities*.

---

## 2. Gap Categories

### 2.1 Interface Gaps (`interfaces/`)
**Definition:** Missing or incomplete binding contracts, schemas, or invariants.

**Examples:**
- CLI surface without corresponding schema documentation
- Store semantics that allow contamination
- Proof surface that doesn't actually validate what it claims
- Undefined behavior at subsystem boundaries
- Schema drift (doc says X, code does Y)

**Detection:**
- Run `decapod validate` and analyze warnings
- Compare subsystem registry (PLUGINS.md) to actual CLI help output
- Check for `STUB` or `SPEC` items without graduation path
- Review error messages for undocumented edge cases

**Routing:**
- Interface contract issues → `interfaces/INTERFACES.md`
- Store model violations → `interfaces/STORE_MODEL.md`
- Doc compilation errors → `interfaces/DOC_RULES.md`
- Claims without proof → `interfaces/CLAIMS.md`
- Undefined terms → `interfaces/GLOSSARY.md`

**See:** `interfaces/INTERFACES.md` for interface contract registry

---

### 2.2 Methodology Gaps (`methodology/`)
**Definition:** Missing guidance, unclear practices, or incomplete cognitive frameworks.

**Examples:**
- Agent doesn't know how to handle a specific scenario
- Architecture practice lacks decision criteria
- Knowledge management has no staleness policy
- Memory system lacks retrieval validation
- Unclear when to use which subsystem

**Detection:**
- Agents asking repetitive clarifying questions
- Inconsistent approaches to similar problems
- Documentation exists but isn't actionable
- Process gaps in multi-agent coordination
- Missing "how to" guidance for common tasks

**Routing:**
- Intent-driven workflow gaps → `specs/INTENT.md` (binding methodology)
- Architecture practice gaps → `methodology/ARCHITECTURE.md`
- Agent behavior gaps → `methodology/SOUL.md`
- Knowledge management gaps → `methodology/KNOWLEDGE.md`
- Learning/memory gaps → `methodology/MEMORY.md`

**See:** `core/METHODOLOGY.md` for methodology registry

---

### 2.3 Plugin/Subsystem Gaps (`plugins/`)
**Definition:** Missing functionality, incomplete implementations, or subsystem boundary issues.

**Examples:**
- TODO system lacks classification features
- Health system doesn't track subsystem X
- Missing cron job scheduling granularity
- No knowledge→TODO linking mechanism
- Gap between planned (SPEC) and implemented (REAL)

**Detection:**
- Compare PLUGINS.md registry to actual capabilities
- User requests for missing features
- Workarounds agents invent for missing functionality
- Cross-subsystem coordination failures
- Performance bottlenecks at subsystem boundaries

**Routing:**
- Subsystem status issues → `core/PLUGINS.md`
- Plugin-specific gaps → respective `plugins/<NAME>.md`
- Integration gaps → relevant subsystem docs + PLUGINS.md

**See:** `core/PLUGINS.md` §3.5 for subsystem registry and truth labels

---

### 2.4 Core/Coordination Gaps (`core/`)
**Definition:** Issues in routing, navigation, or system-wide coordination.

**Examples:**
- DECAPOD.md doesn't route to a documented subsystem
- Cross-category references are broken
- OVERRIDE.md isn't being respected
- Gap between demands and enforcement
- Missing emergency protocols

**Detection:**
- `decapod validate` failures in doc graph
- Broken links in constitution
- Navigation failures (can't find docs)
- Override system not functioning
- Contradictions between core files

**Routing:**
- Router/navigation gaps → `core/DECAPOD.md`
- Interface index gaps → `core/INTERFACES.md`
- Methodology index gaps → `core/METHODOLOGY.md`
- Subsystem registry gaps → `core/PLUGINS.md`
- User demand gaps → `core/DEMANDS.md`
- Deprecation gaps → `core/DEPRECATION.md`
- Gap analysis methodology → `core/GAPS.md` (this file)

---

### 2.5 Specification Gaps (`specs/`)
**Definition:** Missing system-level contracts, security considerations, or amendment processes.

**Examples:**
- Security model doesn't cover new threat vector
- Amendment process unclear for specific change types
- System boundaries undefined for new component
- Git contract doesn't cover specific workflow
- Intent contract missing scenario coverage

**Detection:**
- Security reviews finding uncovered areas
- Amendment requests without clear process
- Cross-system integration ambiguities
- Authority disputes about who owns what

**Routing:**
- Intent/methodology contract gaps → `specs/INTENT.md`
- System definition gaps → `specs/SYSTEM.md`
- Security gaps → `specs/SECURITY.md`
- Git workflow gaps → `specs/GIT.md`
- Change control gaps → `specs/AMENDMENTS.md`

---

### 2.6 Project-Specific Gaps (`.decapod/OVERRIDE.md`)
**Definition:** Gaps between embedded constitution and project needs.

**Examples:**
- Project needs custom priority levels
- Specific subsystem needs different defaults
- Custom validation gates required
- Project-specific methodology additions

**Detection:**
- OVERRIDE.md content doesn't address need
- Project repeatedly working around constitution
- Domain-specific gaps not covered by general docs

**Routing:**
- Project overrides → `.decapod/OVERRIDE.md`

---

## 3. Gap Identification Protocol

### 3.1 Continuous Scanning
Gap identification is not a one-time audit. It happens:
- During every agent session
- When validation fails
- When agents ask clarifying questions
- When workarounds emerge
- When proof surfaces can't validate

### 3.2 Gap Signal Detection

**Strong Signals (definite gaps):**
- `decapod validate` fails with new error
- Two docs contradict each other
- Agent can't determine next step
- Proof surface exists but can't be run
- Schema documented but not implemented

**Medium Signals (likely gaps):**
- Repeated similar questions
- Workarounds documented as "temporary"
- SPEC items without graduation timeline
- Claims marked `not_enforced`
- TODOs without clear resolution path

**Weak Signals (potential gaps):**
- Performance could be better
- UX friction
- Missing "nice to have" features
- Undocumented but working behavior

### 3.3 Gap Triage Questions

When you identify a potential gap, answer:

1. **What layer?** (interface, methodology, plugin, core, spec, project)
2. **What severity?** (blocks work, causes friction, nice to have)
3. **Who owns it?** (which document/subsystem has authority)
4. **Is it known?** (check existing TODOs, issues, docs)
5. **What's the proof?** (how would we know when it's fixed)

---

## 4. Gap Documentation & Routing

### 4.1 Document the Gap

Every identified gap should have:
- **Description:** What is missing/misaligned
- **Category:** Which gap type (§2)
- **Evidence:** How you detected it
- **Impact:** What work is blocked or complicated
- **Owner:** Which subsystem/document owns resolution
- **Proof:** How to verify when fixed

### 4.2 Route to Appropriate Subsystem

Use the **routing table** in §2 to determine where the gap belongs.

**Decision tree:**
1. Is it a missing/incomplete binding contract? → `interfaces/`
2. Is it unclear how to do something? → `methodology/`
3. Is it missing functionality? → `plugins/` or `core/PLUGINS.md`
4. Is it navigation/routing? → `core/DECAPOD.md`
5. Is it system-level contract? → `specs/`
6. Is it project-specific? → `.decapod/OVERRIDE.md`

### 4.3 Create TODO (if actionable)

If the gap is actionable:
1. Create TODO via `decapod todo add`
2. Tag with appropriate category
3. Reference this GAPS.md section if gap analysis needed
4. Link to relevant subsystem docs

**Example:**
```bash
decapod todo add "Fix gap: CLI schema missing for X command" --priority high
# Add details: Category=interface, Owner=interfaces/DOC_RULES.md
```

### 4.4 Update Relevant Index

If the gap reveals missing coverage in an index file:
- Update `core/INTERFACES.md` if interface gaps
- Update `core/METHODOLOGY.md` if methodology gaps
- Update `core/PLUGINS.md` if plugin gaps

---

## 5. Gap Lifecycle

```
Identify → Categorize → Route → Document → TODO → Resolve → Verify
```

**States:**
- **Identified:** Gap spotted, not yet categorized
- **Categorized:** Layer and type determined
- **Routed:** Owner document/subsystem identified
- **Documented:** Gap described with evidence
- **Ticketed:** TODO created with priority
- **In Progress:** Being addressed
- **Resolved:** Fix implemented
- **Verified:** Proof surface confirms resolution

---

## 6. Gap Analysis Integration with Subsystems

### 6.1 Integration with TODO System

Gap findings often become TODOs:
- High-impact gaps → high-priority TODOs
- Systemic gaps → epics with multiple TODOs
- Methodology gaps → documentation TODOs
- Interface gaps → implementation + doc TODOs

**See:** `plugins/TODO.md` for work tracking

### 6.2 Integration with Validation

Gap detection often triggered by:
- `decapod validate` failures
- Doc graph reachability issues
- Schema mismatches
- Store contamination detection

**Gap findings should:**
- Add validation gates where possible
- Update validate taxonomy
- Document expected vs actual behavior

**See:** `interfaces/CONTROL_PLANE.md` §6 for validate doctrine

### 6.3 Integration with Knowledge Base

Gap analysis produces knowledge:
- Why gaps exist (historical context)
- How gaps were resolved (patterns)
- Gap taxonomy and categorization
- Common gap types by subsystem

**See:** `methodology/KNOWLEDGE.md` for knowledge management

### 6.4 Integration with Memory

Agents should remember:
- Gap patterns (avoid repeated gaps)
- Resolution strategies
- Common routing decisions
- Verification approaches

**See:** `methodology/MEMORY.md` for learning patterns

---

## 7. Gap Taxonomy Reference

### 7.1 By Layer

| Layer | Gap Type | Index File |
|-------|----------|------------|
| Interfaces | Missing contracts, schemas, invariants | `core/INTERFACES.md` |
| Methodology | Unclear practices, missing guidance | `core/METHODOLOGY.md` |
| Plugins | Missing functionality, incomplete impl | `core/PLUGINS.md` |
| Core | Routing, navigation, coordination | `core/DECAPOD.md` |
| Specs | System contracts, security, process | `specs/` |
| Project | Project-specific overrides | `.decapod/OVERRIDE.md` |

### 7.2 By Severity

| Severity | Description | Action |
|----------|-------------|--------|
| Critical | Blocks work, violates contracts | Immediate TODO, escalate |
| High | Causes friction, workarounds needed | High-priority TODO |
| Medium | Inconvenience, unclear guidance | Medium-priority TODO |
| Low | Nice to have, optimization | Backlog or knowledge entry |

### 7.3 By Lifecycle Stage

| Stage | Gap Characteristic | Typical Resolution |
|-------|-------------------|-------------------|
| Design | Missing spec for planned feature | Add SPEC docs |
| Implementation | STUB without graduation path | Implement or deprioritize |
| Production | REAL but incomplete | Fix or document limitations |
| Maintenance | Drift from documented behavior | Drift recovery |

---

## 8. Common Gap Patterns

### 8.1 "SPEC Forever"
**Pattern:** Feature marked SPEC with no graduation timeline
**Detection:** Check PLUGINS.md for old SPEC items
**Resolution:** Either implement or downgrade to IDEA

### 8.2 "Documentation Drift"
**Pattern:** Docs say X, code does Y, neither is "wrong" but they differ
**Detection:** Validation warnings, agent confusion
**Resolution:** Drift recovery protocol (see INTENT.md)

### 8.3 "Proof Gap"
**Pattern:** Claim exists in CLAIMS.md but proof surface doesn't verify it
**Detection:** `not_enforced` claims, failing validate
**Resolution:** Implement proof or downgrade claim

### 8.4 "Missing Index"
**Pattern:** Subsystem exists but not in registry
**Detection:** PLUGINS.md doesn't list working subsystem
**Resolution:** Add to appropriate index file

### 8.5 "Interface Mismatch"
**Pattern:** Two subsystems expect different interfaces
**Detection:** Integration failures at boundaries
**Resolution:** Define boundary contract in interfaces/

### 8.6 "Methodology Vacuum"
**Pattern:** Common task has no documented approach
**Detection:** Agents invent different solutions
**Resolution:** Add methodology guide

---

## 9. Gap Analysis for Leadership

### 9.1 Strategic Gap Assessment

Principals and Architects should periodically:
- Review gap distribution by layer
- Identify systemic gap patterns
- Assess gap resolution velocity
- Prioritize gap categories

### 9.2 Gap Metrics

Track:
- Gap identification rate (new gaps per week)
- Gap resolution velocity (time to close)
- Gap severity distribution
- Gap category trends
- Recurring gap patterns

### 9.3 Gap Prevention

Proactive measures:
- Thorough design before implementation
- Proof surfaces for all REAL claims
- Clear methodology documentation
- Regular validation
- Cross-subsystem integration testing

---

## 10. Emergency Gap Protocol

### 10.1 Critical Gap Detected

If you find a gap that:
- Violates security contract
- Causes data loss
- Breaks validation completely
- Creates split-brain state

**Immediate actions:**
1. STOP work
2. Document gap with evidence
3. Notify via appropriate channels
4. Consult `plugins/EMERGENCY_PROTOCOL.md`
5. Create critical TODO
6. Do not proceed until resolved

### 10.2 Authority Escalation

If gap crosses authority boundaries:
- Document the ambiguity
- Propose authority assignment
- Reference `interfaces/DOC_RULES.md` §8
- Route to AMENDMENTS.md if needed

---

## 11. Gap Analysis Checklist

**When analyzing system for gaps:**

- [ ] Run `decapod validate` and catalog all warnings
- [ ] Review PLUGINS.md registry vs actual subsystems
- [ ] Check for `STUB`/`SPEC` without graduation path
- [ ] Identify `not_enforced` claims in CLAIMS.md
- [ ] Review recent TODOs for systemic patterns
- [ ] Survey agents for unclear guidance
- [ ] Check for broken/missing links in doc graph
- [ ] Compare OVERRIDE.md to project needs
- [ ] Review emergency protocols for coverage gaps
- [ ] Assess methodology docs for actionable guidance

---

## 12. Gap Resolution Verification

Every resolved gap needs:
- [ ] Proof surface passes
- [ ] Documentation updated
- [ ] Index files current
- [ ] TODO closed with evidence
- [ ] Knowledge entry (if pattern)
- [ ] No new gaps introduced

**Run before claiming resolution:**
```bash
decapod validate
# Must pass completely
```

---

## Links

- `core/DECAPOD.md` - Router and navigation charter
- `core/INTERFACES.md` - Interface contracts index
- `core/METHODOLOGY.md` - Methodology guides index
- `core/PLUGINS.md` - Subsystem registry
- `specs/INTENT.md` - Intent contract
- `specs/SYSTEM.md` - System definition
- `specs/AMENDMENTS.md` - Change control
- `specs/SECURITY.md` - Security doctrine
- `plugins/TODO.md` - Work tracking
- `plugins/EMERGENCY_PROTOCOL.md` - Critical issues
- `interfaces/CONTROL_PLANE.md` - Validation doctrine
- `interfaces/CLAIMS.md` - Promises ledger
- `methodology/KNOWLEDGE.md` - Knowledge management
- `methodology/MEMORY.md` - Learning patterns
- `methodology/ARCHITECTURE.md` - Architecture practice

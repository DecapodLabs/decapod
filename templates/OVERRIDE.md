# OVERRIDE.md - Project-Specific Decapod Overrides

**Canonical:** OVERRIDE.md  
**Authority:** override  
**Layer:** Project  
**Binding:** Yes (overrides embedded constitution)

---

## Summary

This file is your project-local override layer for Decapod's embedded constitution.

The embedded constitution (shipped with Decapod) is read-only baseline policy.  
`OVERRIDE.md` is where you add project-specific behavior without forking Decapod.

Overrides are resolved at runtime via `decapod docs show`.

Keep overrides minimal and explicit.

---

## How To Use

1. Find the relevant section below (Core, Specs, Interfaces, Methodology, Plugins, Architecture).
2. Go to the specific heading you want to override (example: `### plugins/TODO.md`).
3. Add your project-specific markdown directly under that heading.
4. Commit this file.

**Example**

```markdown
### plugins/TODO.md

## Priority Levels (Project Override)

For this project:
- **critical**: Production down, blocking release
- **high**: Sprint commitment, must complete this iteration
- **medium**: Backlog, next sprint candidate
- **low**: Nice-to-have, future consideration
- **idea**: Exploration, needs refinement before actionable
```

---

<!-- ═══════════════════════════════════════════════════════════════════════ -->
<!-- ⚠️  CHANGES ARE NOT PERMITTED ABOVE THIS LINE                           -->
<!-- ═══════════════════════════════════════════════════════════════════════ -->

## Core Overrides (Routers and Indices)

### core/DECAPOD.md

### core/INTERFACES.md

### core/METHODOLOGY.md

### core/PLUGINS.md

### core/GAPS.md

### core/DEMANDS.md

### core/DEPRECATION.md

---

## Specs Overrides (System Contracts)

### specs/INTENT.md

### specs/SYSTEM.md

### specs/AMENDMENTS.md

### specs/SECURITY.md

### specs/GIT.md

---

## Interfaces Overrides (Binding Contracts)

### interfaces/CLAIMS.md

### interfaces/CONTROL_PLANE.md

### interfaces/DOC_RULES.md

### interfaces/GLOSSARY.md

### interfaces/STORE_MODEL.md

---

## Methodology Overrides (Practice Guides)

### methodology/ARCHITECTURE.md

### methodology/SOUL.md

### methodology/KNOWLEDGE.md

### methodology/MEMORY.md

---

## Architecture Overrides (Domain Patterns)

### architecture/DATA.md

### architecture/CACHING.md

### architecture/MEMORY.md

### architecture/WEB.md

### architecture/CLOUD.md

### architecture/FRONTEND.md

### architecture/ALGORITHMS.md

### architecture/SECURITY.md

### architecture/OBSERVABILITY.md

### architecture/CONCURRENCY.md

---

## Plugins Overrides (Operational Subsystems)

### plugins/TODO.md

### plugins/MANIFEST.md

### plugins/EMERGENCY_PROTOCOL.md

### plugins/DB_BROKER.md

### plugins/CRON.md

### plugins/REFLEX.md

### plugins/HEALTH.md

### plugins/POLICY.md

### plugins/WATCHER.md

### plugins/KNOWLEDGE.md

### plugins/ARCHIVE.md

### plugins/FEDERATION.md

### plugins/FEEDBACK.md

### plugins/TRUST.md

### plugins/CONTEXT.md

### plugins/HEARTBEAT.md

### plugins/TEAMMATE.md

### plugins/VERIFY.md

### plugins/DECIDE.md

### plugins/AUTOUPDATE.md

# OVERRIDE.md - Project-Specific Decapod Overrides

**Canonical:** OVERRIDE.md
**Authority:** override
**Layer:** Project
**Binding:** Yes (overrides embedded constitution)

This file allows you to override or extend Decapod's embedded constitution for project-specific needs.

## How Overrides Work

- **Embedded constitution** provides the base methodology (read-only, shipped with Decapod)
- **This file** allows project-specific customization without forking
- Overrides are applied at runtime when agents read the constitution
- Keep overrides minimal - only add what's truly project-specific

## Usage Pattern

```markdown
### [component-path]

Your override content here...
```

Component paths follow the embedded structure:
- `core/DECAPOD.md` - Navigation charter
- `core/CONTROL_PLANE.md` - Agent sequencing
- `specs/INTENT.md` - Authority contracts
- `specs/ARCHITECTURE.md` - System boundaries
- `plugins/TODO.md` - TODO subsystem
- etc.

---

## Core Overrides

Override core Decapod components (navigation, control plane, store model, etc.)

### core/DECAPOD.md
<!-- Override navigation charter or add project-specific routing -->

### core/CONTROL_PLANE.md
<!-- Override agent sequencing patterns -->

### core/STORE_MODEL.md
<!-- Override store purity model -->

### core/PLUGINS.md
<!-- Override subsystem registry -->

### core/DOC_RULES.md
<!-- Override documentation compiler rules -->

### core/CLAIMS.md
<!-- Override claims ledger -->

### core/SOUL.md
<!-- Override agent persona guidelines -->

---

## Specs Overrides

Override specification documents (intent, architecture, system contracts, etc.)

### specs/INTENT.md
<!-- Override authority contracts and methodology intent -->

### specs/ARCHITECTURE.md
<!-- Override system boundaries, tradeoffs, and architectural decisions -->

### specs/SYSTEM.md
<!-- Override authority and proof doctrine -->

### specs/AMENDMENTS.md
<!-- Override change control process -->

---

## Plugin Overrides

Override plugin-specific configuration (TODO, health, knowledge, policy, etc.)

### plugins/TODO.md
<!-- Override TODO subsystem behavior -->

### plugins/WORKFLOW.md
<!-- Override operating loop -->

### plugins/MANIFEST.md
<!-- Override canonical vs derived vs state definitions -->

### plugins/METHODOLOGY_GAPS.md
<!-- Override known gaps documentation -->

### plugins/TODO_USER.md
<!-- Override agent checklist -->

### plugins/EMERGENCY_PROTOCOL.md
<!-- Override emergency stop-the-line protocol -->

### plugins/DB_BROKER.md
<!-- Override broker interface contract -->

---

## Examples

### Example: Override TODO Priority Levels

```markdown
### plugins/TODO.md

## Priority Levels (Project Override)

For this project, we use a 5-level priority system:

- **critical**: Production down, blocking release
- **high**: Sprint commitment, must complete this iteration
- **medium**: Backlog, next sprint candidate
- **low**: Nice-to-have, future consideration
- **idea**: Exploration, needs refinement before actionable
```

### Example: Add Project-Specific Architectural Constraint

```markdown
### specs/ARCHITECTURE.md

## Project-Specific Constraints

### Performance Budget

All API endpoints must respond within 200ms p99 latency.
Any change that impacts this budget requires architectural review.

### Technology Restrictions

- **Prohibited**: ORMs (use raw SQL for event sourcing)
- **Required**: SQLite only (no PostgreSQL/MySQL)
- **Preferred**: Rust standard library over external crates
```

---

## Guidelines

1. **Keep it minimal** - Only override what's necessary for your project
2. **Document why** - Explain the reason for each override
3. **Version control** - Commit this file to your repo
4. **Review regularly** - Remove overrides that are no longer needed
5. **Propose upstream** - If your override would benefit all projects, submit a PR to Decapod

---

## Need Help?

- See embedded docs: `decapod docs list`
- Read a specific doc: `decapod docs show <path>`
- Validate overrides: `decapod validate`
